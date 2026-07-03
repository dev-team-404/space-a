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
    use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
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
                        eprintln!("warn: {} 감시 실패: {e}", projects.display());
                    } else {
                        out.push(w);
                    }
                }
                Err(e) => eprintln!("warn: {} watcher 생성 실패: {e}", hs.host),
            }
        }
        out
    }

    pub fn run_pipeline_once(app: &AppHandle) {
        let state = app.state::<AppState>();
        let result = (|| -> anyhow::Result<()> {
            let mut store = state.store.lock()
                .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
            let before: HashMap<String, String> = store.finding_severities()?.into_iter().collect();

            let report = agent_mentor::ops::run_ingest(&store)?;
            for w in &report.warnings { eprintln!("warn: {w}"); }
            for w in agent_mentor::ops::run_inventory(&mut store)? { eprintln!("warn: {w}"); }
            agent_mentor::ops::run_rules(&store)?;

            let after = store.finding_severities()?;
            let fresh = diff_findings(&before, &after);
            let now = chrono::Utc::now().to_rfc3339();
            store.set_setting("last_scan_ts", &now)?;

            if !fresh.is_empty() {
                let rows: Vec<_> = store.list_findings_current()?.into_iter()
                    .filter(|f| fresh.contains(&f.dedup_key)).collect();
                app.emit("coach:finding", &rows)?;
            }

            maybe_generate_diaries(app, &store)?;
            app.emit("scan:done", &now)?;
            Ok(())
        })();
        if let Err(e) = result { eprintln!("pipeline error: {e}"); }
    }

    fn maybe_generate_diaries(app: &AppHandle, store: &SqliteStore) -> anyhow::Result<()> {
        let Some(engine) = OpenAiCompatEngine::from_env() else { return Ok(()); };
        let vault = app.path().app_data_dir()?.join("diary");
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let existing = store.diary_dates()?;
        for date in missing_diary_dates(&existing, &today, 7) {
            let cfg = DiaryConfig { vault_dir: vault.clone(), ..DiaryConfig::default() };
            let brief = assemble_brief(store, "Windows", &date, &cfg)?;
            if brief.totals.session_count == 0 { continue; }
            generate_diary(store, &engine, &brief, &cfg)?;
            app.emit("diary:ready", &date)?;
        }
        Ok(())
    }
}

#[cfg(not(test))]
pub use runtime::start;