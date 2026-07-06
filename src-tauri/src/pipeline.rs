//! notify 감시 → 디바운스 배치 → ingest→inventory→rules → finding diff → 이벤트.
//! 순수 함수(diff/debounce/missing_dates)는 agent_mentor::pipeline 에 있고 단위 테스트도 거기서 수행.
//! 이 파일은 Tauri 런타임 코드만 담당.

// Re-export pure types/functions from core so lib.rs can use `pipeline::PipelineMsg` etc.
pub use agent_mentor::pipeline::{debounce_loop, diff_findings, missing_diary_dates, PipelineMsg};

#[cfg(not(test))]
mod runtime {
    use super::*;
    use crate::AppState;
    use agent_mentor::diary::engine::OpenAiCompatEngine;
    use agent_mentor::diary::{assemble_brief, persist_diary, render_diary, DiaryConfig};
    use agent_mentor::hosts::enumerate_hosts;
    use agent_mentor::store::SqliteStore;
    use notify::Watcher;
    use std::collections::HashMap;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter, Manager};

    pub fn start(
        app: AppHandle,
        rx: std::sync::mpsc::Receiver<PipelineMsg>,
        tx: std::sync::mpsc::Sender<PipelineMsg>,
    ) {
        std::thread::spawn(move || {
            let _watchers = spawn_watchers(tx);
            run_pipeline_once(&app);
            let app2 = app.clone();
            debounce_loop(rx, Duration::from_secs(60), move || run_pipeline_once(&app2));
        });
    }

    fn spawn_watchers(tx: std::sync::mpsc::Sender<PipelineMsg>) -> Vec<Box<dyn Watcher + Send>> {
        let mut out: Vec<Box<dyn Watcher + Send>> = Vec::new();
        for hs in enumerate_hosts() {
            let projects = hs.claude_root.join("projects");
            if !projects.is_dir() { continue; }
            let tx2 = tx.clone();
            let handler = move |res: notify::Result<notify::Event>| {
                if res.is_ok() { let _ = tx2.send(PipelineMsg::FileChanged); }
            };
            let watcher: notify::Result<Box<dyn Watcher + Send>> = if hs.host == "Windows" {
                notify::recommended_watcher(handler).map(|w| Box::new(w) as _)
            } else {
                notify::PollWatcher::new(
                    handler,
                    notify::Config::default().with_poll_interval(Duration::from_secs(30)),
                ).map(|w| Box::new(w) as _)
            };
            match watcher {
                Ok(mut w) => {
                    if let Err(e) = w.watch(&projects, notify::RecursiveMode::Recursive) {
                        log::warn!("{} 감시 실패: {e}", projects.display());
                    } else {
                        out.push(w);
                    }
                }
                Err(e) => log::warn!("{} watcher 생성 실패: {e}", hs.host),
            }
        }
        out
    }

    pub fn run_pipeline_once(app: &AppHandle) {
        let state = app.state::<AppState>();
        let scan_result = (|| -> anyhow::Result<String> {
            // ── 스캔·diff·emit: 락을 잡는 범위 ──────────────────────────────────
            let (now, _fresh_findings) = {
                let mut store = state.store.lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let before: HashMap<String, String> = store.finding_severities()?.into_iter().collect();

                let report = agent_mentor::ops::run_ingest_with_progress(&store, &mut |done, total| {
                    // 파일 수천 개일 수 있어 5건 단위로만 emit (마지막은 항상)
                    if done == total || done % 5 == 0 {
                        let _ = app.emit("scan:progress", serde_json::json!({"done": done, "total": total}));
                    }
                })?;
                for w in &report.warnings { log::warn!("{w}"); }
                for w in agent_mentor::ops::run_inventory(&mut store)? { log::warn!("{w}"); }
                agent_mentor::ops::run_rules(&store)?;

                let after = store.finding_severities()?;
                let fresh = diff_findings(&before, &after);
                let now = chrono::Utc::now().to_rfc3339();
                store.set_setting("last_scan_ts", &now)?;

                if !fresh.is_empty() {
                    let rows: Vec<_> = store.list_findings_current(false)?.into_iter()
                        .filter(|f| fresh.contains(&f.dedup_key)).collect();
                    app.emit("coach:finding", &rows)?;
                }
                (now, fresh)
                // guard drops here — 다이어리 생성(LLM 네트워크 I/O) 전에 락 해제
            };
            Ok(now)
        })();

        match scan_result {
            Ok(now) => {
                // scan:done은 스캔 성공 시 다이어리 결과와 무관하게 emit (§7)
                if let Err(e) = app.emit("scan:done", &now) {
                    log::error!("pipeline error: scan:done emit 실패: {e}");
                }
                // 다이어리 실패는 조용히 — 다음 사이클에서 재시도
                maybe_generate_diaries(app, &state.store);
            }
            Err(e) => {
                log::error!("pipeline error: {e}");
                // 스캔이 도중 실패해도 프론트의 scanning 상태를 반드시 해제 — scan:progress로 켜진 "스캔 중…" 고착 방지
                let now = chrono::Local::now().to_rfc3339();
                if let Err(e) = app.emit("scan:done", &now) {
                    log::error!("scan:done(에러 경로) emit 실패: {e}");
                }
            }
        }
    }

    fn maybe_generate_diaries(
        app: &AppHandle,
        store_mutex: &std::sync::Mutex<SqliteStore>,
    ) {
        let Some(engine) = OpenAiCompatEngine::from_env() else { return; };
        let vault = match app.path().app_data_dir() {
            Ok(d) => d.join("diary"),
            Err(e) => { log::warn!("diary vault 경로 실패: {e}"); return; }
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // 락을 짧게 잡아 missing dates 목록만 조회 후 즉시 해제
        let dates = match store_mutex.lock() {
            Ok(store) => match store.diary_dates() {
                Ok(existing) => missing_diary_dates(&existing, &today, 7),
                Err(e) => { log::warn!("diary_dates 실패: {e}"); return; }
            },
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };

        for date in dates {
            // ① 락 획득 → assemble_brief + session_count 체크 → 즉시 해제
            let (brief, cfg) = match store_mutex.lock() {
                Ok(store) => {
                    let cfg = DiaryConfig { vault_dir: vault.clone(), ..DiaryConfig::default() };
                    match assemble_brief(&store, "Windows", &date, &cfg) {
                        Ok(brief) => (brief, cfg),
                        Err(e) => { log::warn!("assemble_brief({date}) 실패: {e}"); continue; }
                    }
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            }; // guard drops here

            if brief.totals.session_count == 0 { continue; }

            // ② 락 없이 render_diary (네트워크 I/O)
            let rendered = match render_diary(&engine, &brief, &cfg) {
                Ok(r) => r,
                Err(e) => { log::warn!("render_diary({date}) 실패: {e}"); continue; }
            };

            // ③ 락 획득 → persist_diary (로컬 파일·DB) → 즉시 해제
            let persist_result = match store_mutex.lock() {
                Ok(store) => persist_diary(&store, &date, &brief.host, &rendered, &cfg),
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            }; // guard drops here
            match persist_result {
                Ok(_) => { let _ = app.emit("diary:ready", &date); }
                Err(e) => log::warn!("persist_diary({date}) 실패: {e}"),
            }
        }
    }
}

#[cfg(not(test))]
pub use runtime::start;
