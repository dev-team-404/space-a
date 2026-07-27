//! notify 감시 → 디바운스 배치 → ingest→inventory→rules → finding diff → 이벤트.
//! 순수 함수(diff/debounce/missing_dates)는 agent_mentor::pipeline 에 있고 단위 테스트도 거기서 수행.
//! 이 파일은 Tauri 런타임 코드만 담당.

// Re-export pure types/functions from core so lib.rs can use `pipeline::PipelineMsg` etc.
pub use agent_mentor::pipeline::{debounce_loop, diff_findings, missing_diary_dates, PipelineMsg};

#[cfg(not(test))]
mod runtime {
    use super::*;
    use crate::AppState;
    use agent_mentor::diary::{
        assemble_brief, persist_diary, render_diary, render_idle_diary, DiaryConfig, IdleContext,
    };
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
                // 콘텐츠 큐레이션 — 피드 fetch(락 밖) → run_curation(락) → content:ready
                maybe_curate_content(app, &state.store);
                // 코칭 판정(R6·R7…) — 엔진 없으면 no-op(pending 침묵), 실패는 조용히(다음 스캔 재시도)
                run_coaching_judgments(app, &state.store);
                // E — 세션 work-kind 판정(LLM) 캐시. 엔진 없으면 no-op, 카드는 다음 스캔의
                // 큐레이션이 집계 (판정 패스와 별개 — 스펙 §4 E "큐레이션에 얹음")
                maybe_judge_work_kinds(&state.store);
                // 다이어리 실패는 조용히 — 다음 사이클에서 재시도
                maybe_generate_diaries(app, &state.store);
                // 오늘의 한마디 — 엔진 없으면 no-op, 실패는 조용히(다음 스캔 재시도)
                maybe_generate_daily_line(app, &state.store);
                // 잡담 풀 — 동일 규율, 이벤트 없음(프론트 타이머가 pull)
                maybe_generate_chatter_pool(&state.store);
                // G3 방명록 봇 자동 답글 — hub 미연결·엔진 없으면 no-op, 실패는 조용히(다음 스캔 재시도)
                maybe_reply_guestbook(&state.store);
                // a-hub 지식 공유 — 유의미 finding을 이슈→해결로 발행 (env 미설정 시 no-op)
                maybe_share_findings(&state.store);
                // 텔레메트리(#46) — 전날 파생 신호 하루 1회 발행 (env 미설정 시 no-op)
                maybe_push_telemetry(&state.store);
                // 세션 회고 — "고생 끝 해결" 세션을 로컬 생성 요약으로 발행 (Engine 필요)
                maybe_post_retros(&state.store);
                // AI 스프라이트 — 캐시 없으면 1회 생성 (실패 무해, 절차 생성 폴백)
                maybe_generate_sprite(app);
                // 외부 문서 도달성 — 내부망이면 배움 카드의 외부 링크를 숨긴다 (동료 이슈)
                maybe_probe_docs(&state.store);
            }
            Err(e) => {
                log::error!("pipeline error: {e}");
                // 스캔이 도중 실패해도 프론트의 scanning 상태를 반드시 해제 — scan:progress로 켜진 "스캔 중…" 고착 방지
                // 절대 시각은 UTC RFC3339 (성공 경로·Task1 정책과 일관)
                let now = chrono::Utc::now().to_rfc3339();
                if let Err(e) = app.emit("scan:done", &now) {
                    log::error!("scan:done(에러 경로) emit 실패: {e}");
                }
            }
        }
    }

    /// 콘텐츠 큐레이션 — 스캔 편승. 피드 fetch(네트워크)는 diary·daily-line과 동일하게
    /// **store 락 밖**에서, 그다음 run_curation(프로필 감지·랭킹·persist)만 락 안에서.
    /// 노출 목록이 있으면 `content:ready`를 emit해 프론트가 즉시 반영(coach:finding 선례).
    /// 네트워크 실패는 조용히(빈 피드로 진행 — 내장 팁만으로도 코칭 성립).
    fn maybe_curate_content(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>) {
        // ⓪ 짧은 락: 팀 지식(pull) 소스 구성에 필요한 저장 토큰만 읽고 즉시 해제
        let hub_src = {
            let (cfg, stored) = match store_mutex.lock() {
                Ok(store) => (
                    agent_mentor::hub::HubConfig::resolve(&store),
                    store.get_setting("knowledge_hub_token").ok().flatten(),
                ),
                Err(_) => (None, None),
            };
            cfg.and_then(|cfg| agent_mentor::hub::pull_source(&cfg, stored))
        };
        // ① 락 밖: 피드 소스 + E 마켓플레이스 카탈로그 네트워크 fetch (실패해도 빈 벡터)
        let feed = agent_mentor::ops::fetch_feed_items(hub_src);
        let catalog = agent_mentor::ops::fetch_marketplace_catalog();
        let now = chrono::Utc::now().to_rfc3339();

        // ② 락: run_curation(감지→랭킹→persist) → 노출 목록 → 즉시 해제
        let visible = match store_mutex.lock() {
            Ok(store) => match agent_mentor::ops::run_curation(&store, feed, &catalog, &now) {
                Ok(rows) => rows,
                Err(e) => { log::warn!("run_curation 실패: {e}"); return; }
            },
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here

        // ③ 노출할 게 있으면 프론트에 알림
        if !visible.is_empty() {
            let _ = app.emit("content:ready", &visible);
        }
    }

    /// 코칭 판정 패스(B 스펙) — 스캔 편승. judges 벡터(R6·R7…)를 순회하며 각 룰의 pending
    /// 후보를 Engine으로 판정한다. 엔진 미설정이면 no-op(pending 잔류 = fail-safe 침묵).
    /// 락 규율은 diary와 동일: ①엔진 해석 ②pending+build_prompt(SQL만)는 짧은 락,
    /// ③generate(LLM 네트워크)는 **락 밖**, ④verdict 저장·⑤rollup은 짧은 락.
    /// scan:done·coach:finding은 판정 전에 이미 나갔으므로, 새로 노출된 finding
    /// (R6 worthy 'new' + R7 롤업 프로젝트 카드)이 생기면 스캔 fresh-finding과 동일하게
    /// 재발행해 배지·CoachTab을 즉시 갱신한다.
    fn run_coaching_judgments(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::diary::engine::Engine as _;
        use agent_mentor::judge::{extract_verdict_json, CoachingJudge};
        let judges: Vec<Box<dyn CoachingJudge>> = vec![
            Box::new(agent_mentor::judge::R6Judge),
            Box::new(agent_mentor::rules::r7_judge::R7Judge),
        ];
        // ① 엔진 (짧은 락)
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };

        let mut fresh_all: Vec<String> = Vec::new();
        let mut mutated_any = false; // 롤업이 카드를 생성/갱신/삭제했는지 (UI 재발행 판단)
        for judge in &judges {
            // ② pending + 프롬프트 (짧은 락)
            let batch: Vec<(String, u32, String, String)> = match store_mutex.lock() {
                Ok(store) => {
                    let mut b = Vec::new();
                    for c in store.pending_for_judgment(judge.rule_id(), 10).unwrap_or_default() {
                        match judge.build_prompt(&store, &c) {
                            Ok((sys, usr)) => b.push((c.dedup_key, c.prev_attempts, sys, usr)),
                            // 재료 수집 실패는 영구 실패로 취급 — attempts를 올려 3회면 rejected로
                            // 마킹한다(파싱 실패 경로와 동형, HoL 큐 막힘 방지).
                            Err(e) => {
                                log::warn!("{} 재료 수집 실패({}): {e}", judge.rule_id(), c.dedup_key);
                                let attempts = c.prev_attempts + 1;
                                let status = if attempts >= 3 { Some("rejected") } else { None };
                                let _ = store.set_judgment(&c.dedup_key, status,
                                    &serde_json::json!({"attempts":attempts,"error":e.to_string()}));
                            }
                        }
                    }
                    b
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
            // ③ 락 밖: 판정 (LLM 네트워크 I/O)
            let mut results = Vec::new();
            for (key, prev, sys, usr) in batch {
                match engine.generate(&sys, &usr) {
                    Ok(out) => results.push((key, prev, out.text, out.tokens_used)),
                    // 전송 실패 — attempts 미증가, pending 잔류(다음 스캔 재시도)
                    Err(e) => log::warn!("{} 판정 전송 실패({key}): {e}", judge.rule_id()),
                }
            }
            // ④ verdict 저장 (짧은 락)
            match store_mutex.lock() {
                Ok(store) => {
                    for (key, prev, text, tokens) in results {
                        let attempts = prev + 1;
                        // 결정 필드가 있는 valid verdict만 종결 처리. 추출 실패 또는 필수 필드가 없는
                        // 미확정 verdict(예: {} · {"foo":1})는 형식 불량으로 취급 — attempts++·3회면
                        // rejected(재시도 계약 유지, 한 번의 불완전 응답으로 영구 오캐시 금지).
                        let verdict = extract_verdict_json(&text).ok()
                            .and_then(|v| judge.classify(&v).map(|s| (s, v)));
                        match verdict {
                            Some((status, v)) => {
                                let mut rec = v;
                                rec["attempts"] = serde_json::json!(attempts);
                                rec["tokens"] = serde_json::json!(tokens);
                                let _ = store.set_judgment(&key, Some(status), &rec);
                                // R6 worthy 즉시 노출 보존: classify가 'new'로 전환한 key는 롤업을
                                // 거치지 않으므로 여기서 fresh에 모은다. (R7 confirmed는 'new'가 아니라
                                // 판정된 세션 후보로 잔류 → 롤업 프로젝트 카드로만 노출)
                                if status == "new" { fresh_all.push(key.clone()); }
                            }
                            None => {
                                let status = if attempts >= 3 { Some("rejected") } else { None };
                                let _ = store.set_judgment(&key, status,
                                    &serde_json::json!({"attempts":attempts,"error":"verdict 추출/필수필드 실패","tokens":tokens}));
                            }
                        }
                    }
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            }
            // ⑤ 롤업 (짧은 락)
            match store_mutex.lock() {
                Ok(store) => match judge.rollup(&store) {
                    Ok((keys, mutated)) => { fresh_all.extend(keys); mutated_any |= mutated; }
                    Err(e) => log::warn!("{} rollup 실패: {e}", judge.rule_id()),
                },
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            }
        }
        // ⑥ 판정으로 노출 세트가 바뀌면 재발행 — 신규 카드·R6 worthy(fresh)뿐 아니라 카드
        //    갱신/삭제(mutated)까지. coach:finding은 CoachTab 전체 재조회를 유발해 갱신·삭제를
        //    반영하고(payload 무관), scan:done은 셸 배지 activeCount를 갱신한다.
        if !fresh_all.is_empty() || mutated_any {
            if let Ok(store) = store_mutex.lock() {
                let rows: Vec<_> = store.list_findings_current(false).unwrap_or_default()
                    .into_iter().filter(|f| fresh_all.contains(&f.dedup_key)).collect();
                drop(store);
                // 갱신/삭제만 있으면 rows가 비어도 coach:finding을 보내 목록 재조회를 유발한다.
                let _ = app.emit("coach:finding", &rows);
                let _ = app.emit("scan:done", &chrono::Utc::now().to_rfc3339());
            }
        }
    }

    /// E — 세션 work-kind 판정(LLM, 큐레이션 매칭 재료). 판정 패스(finding)와 별개지만
    /// 락 규율은 동일: ①엔진 해석·②후보+프롬프트는 짧은 락, ③generate는 락 밖, ④저장 짧은 락.
    /// 엔진 미설정이면 no-op(fail-safe 침묵). verdict는 세션당 1회 캐시(재판정 없음).
    /// 전송 실패 = attempts 미증가(다음 스캔 재시도) / 형식 불량 = attempts++(3회면 영구 침묵).
    fn maybe_judge_work_kinds(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::diary::engine::Engine as _;
        use agent_mentor::judge::extract_verdict_json;
        use agent_mentor::plugin_reco::{
            parse_work_kinds, work_kind_prompt, WORK_KIND_BATCH_CAP, WORK_KIND_WINDOW_DAYS,
        };
        // ① 엔진 (짧은 락)
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };
        let window_start =
            (chrono::Utc::now() - chrono::Duration::days(WORK_KIND_WINDOW_DAYS)).to_rfc3339();

        // ② 후보 + 프롬프트 (짧은 락, SQL만)
        let batch: Vec<(agent_mentor::store::WorkKindCandidate, String, String)> =
            match store_mutex.lock() {
                Ok(store) => store
                    .pending_work_kind_sessions(&window_start, WORK_KIND_BATCH_CAP)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|c| {
                        let prompts = store.session_user_prompts(&c.session_id, 5).ok()?;
                        if prompts.is_empty() { return None; }
                        let (sys, usr) = work_kind_prompt(&prompts);
                        Some((c, sys, usr))
                    })
                    .collect(),
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
        if batch.is_empty() { return; }

        // ③ 락 밖: 판정 (LLM 네트워크 I/O)
        let mut results = Vec::new();
        for (c, sys, usr) in batch {
            match engine.generate(&sys, &usr) {
                Ok(out) => results.push((c, out.text)),
                // 전송 실패 — attempts 미증가, 다음 스캔 재시도
                Err(e) => log::warn!("work-kind 판정 전송 실패({}): {e}", c.session_id),
            }
        }

        // ④ 저장 (짧은 락)
        let now = chrono::Utc::now().to_rfc3339();
        match store_mutex.lock() {
            Ok(store) => {
                for (c, text) in results {
                    let attempts = c.prev_attempts + 1;
                    let kinds = extract_verdict_json(&text).ok()
                        .and_then(|v| parse_work_kinds(&v));
                    let _ = store.set_session_work_kinds(
                        &c.session_id, &c.host, &c.project_id, &c.last_ts,
                        kinds.as_deref(), attempts, &now,
                    );
                }
            }
            Err(e) => log::warn!("store lock poisoned: {e}"),
        }
    }

    fn maybe_generate_diaries(
        app: &AppHandle,
        store_mutex: &std::sync::Mutex<SqliteStore>,
    ) {
        // 설정 창(store) → .env 순서로 엔진 해석. 락은 해석 동안만 (네트워크 전 해제 규율)
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };
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
            let (brief, cfg, days_idle, memories) = match store_mutex.lock() {
                Ok(store) => {
                    let honorific = crate::commands::owner_title(&store);
                    let mbti = store.get_setting("user_mbti").ok().flatten()
                        .and_then(|m| agent_mentor::mascot::normalize_mbti(&m));
                    let cfg = DiaryConfig { vault_dir: vault.clone(), honorific, mbti, ..DiaryConfig::default() };
                    match assemble_brief(&store, "Windows", &date, &cfg) {
                        Ok(brief) => {
                            let days_idle = store.days_since_last_active(&date).ok().flatten();
                            let memories: Vec<String> = store
                                .list_memories()
                                .map(|v| v.into_iter().map(|m| m.text).collect())
                                .unwrap_or_default();
                            (brief, cfg, days_idle, memories)
                        }
                        Err(e) => { log::warn!("assemble_brief({date}) 실패: {e}"); continue; }
                    }
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            }; // guard drops here

            // ② 락 없이 렌더 (네트워크 I/O). 활동 0인 날은 작업 사실 없이 마스코트 상상 일기(무활동일).
            let rendered = if brief.totals.session_count == 0 {
                let idle = IdleContext {
                    date: date.clone(),
                    is_weekend: brief.work_context.is_weekend,
                    is_holiday: brief.work_context.is_holiday,
                    days_idle,
                    occasions: brief.occasions.clone(),
                    recent_diaries: brief.recent_diaries.clone(),
                };
                match render_idle_diary(&engine, &idle, &cfg, &memories) {
                    Ok(r) => r,
                    Err(e) => { log::warn!("render_idle_diary({date}) 실패: {e}"); continue; }
                }
            } else {
                match render_diary(&engine, &brief, &cfg, &memories) {
                    Ok(r) => r,
                    Err(e) => { log::warn!("render_diary({date}) 실패: {e}"); continue; }
                }
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

    /// 오늘의 한마디 — scan:done마다 fp가 stale할 때만 재생성(스펙 §3). 엔진 없으면 no-op.
    /// 네트워크(LLM)는 diary와 동일하게 store 락 밖에서 호출. 생성/갱신 성공 시
    /// `daily-line:ready`를 emit한다 — scan:done은 생성 전에 이미 나갔으므로, 이 이벤트로
    /// 프론트가 방금 만든 한마디를 즉시 반영한다(diary:ready 선례).
    fn maybe_generate_daily_line(
        app: &AppHandle,
        store_mutex: &std::sync::Mutex<SqliteStore>,
    ) {
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // ① 락: 오늘 컨텍스트 + 최근 무드 윈도우 + 캐시된 fingerprint 읽기 → 즉시 해제
        let (ctx, moods, cached_fp) = match store_mutex.lock() {
            Ok(store) => {
                let ctx = match crate::commands::chat_context_inner(&store) {
                    Ok(c) => c,
                    Err(e) => { log::warn!("daily-line chat_context 실패: {e}"); return; }
                };
                let moods = agent_mentor::mascot::collect_recent_moods(
                    &store,
                    chrono::Local::now().date_naive(),
                );
                let cached_fp = match store.get_daily_line(&today) {
                    Ok(v) => v.map(|(_, fp)| fp),
                    Err(e) => { log::warn!("get_daily_line 실패: {e}"); return; }
                };
                (ctx, moods, cached_fp)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // ② 락 없이 compute (0건 정적 or 네트워크 생성). None이면 skip.
        let outcome = match agent_mentor::mascot::compute_daily_line(&engine, &ctx, &moods, cached_fp.as_deref()) {
            Ok(o) => o,
            Err(e) => { log::warn!("compute_daily_line 실패: {e}"); return; }
        };
        let Some((text, fp)) = outcome else { return; };

        // ③ 락: 캐시 upsert → 즉시 해제
        let stored = match store_mutex.lock() {
            Ok(store) => match store.upsert_daily_line(&today, &text, &fp) {
                Ok(_) => true,
                Err(e) => { log::warn!("upsert_daily_line 실패: {e}"); false }
            },
            Err(e) => { log::warn!("store lock poisoned: {e}"); false }
        };
        // ④ 표시 갱신 알림 — 프론트가 재조회 없이 즉시 반영 (diary:ready 선례)
        if stored {
            let _ = app.emit("daily-line:ready", &text);
        }
    }

    /// 잡담 풀 — scan:done마다 fp가 stale할 때만 재생성 (스펙 §3). 엔진 없으면 no-op.
    /// 네트워크(LLM)는 daily-line과 동일하게 store 락 밖에서 호출. 이벤트는 emit하지
    /// 않는다 — 프론트 잡담 타이머가 발화 시점에 get_chatter_pool로 pull한다.
    /// a-hub 지식 공유 — 스캔 편승. 네트워크는 전부 **락 밖**, 마크 persist는 짧은 락으로.
    /// env(SPACE_A_HUB_URL) 미설정이면 no-op. 실패는 warn 후 다음 스캔 재시도(스펙 §6).
    /// 스펙: docs/design/overview-mentor/specs/2026-07-18-hub-knowledge-sharing-design.md
    fn maybe_share_findings(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::hub::{self, HubClient, HubConfig};
        // 설정(store) → env 순으로 해석. 짧은 락만 잡고 즉시 해제(네트워크 전 해제 규율).
        let Some(cfg) = (match store_mutex.lock() {
            Ok(store) => HubConfig::resolve(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }) else { return };
        let now = chrono::Utc::now().to_rfc3339();

        // ① 락: 토큰·재개 목록·신규 후보 조회 → 즉시 해제
        let (token_opt, pending, picked) = match store_mutex.lock() {
            Ok(store) => {
                let token = cfg.token.clone().or(store.get_setting("knowledge_hub_token").ok().flatten());
                let pending: Vec<(String, String, agent_mentor::store::FindingRow)> = store
                    .hub_share_pending()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(k, i)| store.find_finding(&k).ok().flatten().map(|f| (k, i, f)))
                    .collect();
                let findings = store.list_findings_current(false).unwrap_or_default();
                let already = store.hub_shared_or_pending_keys().unwrap_or_default();
                let picked: Vec<agent_mentor::store::FindingRow> =
                    hub::select_shareable(&findings, &already, cfg.min_tokens)
                        .into_iter()
                        .cloned()
                        .collect();
                (token, pending, picked)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // 대상이 없으면 네트워크(등록 포함) 자체를 하지 않는다
        if pending.is_empty() && picked.is_empty() {
            return;
        }

        // ② 락 밖: 토큰 확보 (최초 1회 자동 register → settings 보존)
        let token = match token_opt {
            Some(t) => t,
            None => match HubClient::register(&cfg) {
                Ok((agent_id, t)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.set_setting("knowledge_hub_agent_id", &agent_id);
                        let _ = store.set_setting("knowledge_hub_token", &t);
                    }
                    t
                }
                Err(e) => { log::warn!("hub register 실패(다음 스캔 재시도): {e}"); return; }
            },
        };
        let client = HubClient {
            base_url: cfg.base_url.clone(),
            api_key: cfg.api_key.clone(),
            token,
        };

        let mut published = 0usize;

        // ③ 재개: open만 되고 발행 안 된 것 resolve (중복 이슈 방지)
        for (dedup_key, issue_id, f) in pending {
            let Some(content) = hub::render_share(&f) else { continue };
            match client.resolve_issue(&issue_id, &content.summary, &content.steps) {
                Ok(Some(page_id)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.hub_mark_published(&dedup_key, &page_id, &now);
                    }
                    published += 1;
                }
                Ok(None) => log::warn!("hub {dedup_key}: resolve 응답에 page_id 없음"),
                Err(e) => log::warn!("hub {dedup_key}: resolve 재개 실패: {e}"),
            }
        }

        // ④ 신규: 검색 → (있으면) 인용 / (없으면) open_issue → resolve(발행)
        //    검색은 "팀이 이미 풀어놨는지" 확인이다. 있으면 중복 발행 대신 인용해
        //    ReuseEvent를 남긴다 — README의 "다른 에이전트가 검색·인용해 재사용".
        let mut cited = 0usize;
        for f in picked {
            let Some(content) = hub::render_share(&f) else { continue };

            // 질의·매칭 모두 마커를 쓴다 — 허브가 발행 Page 제목을 summary로 만들어서
            // 이슈 제목으로는 되찾을 수 없다. org 전체 검색이라 다른 팀 지식도 인용 대상이다.
            // 검색 실패는 무해 — 빈 결과로 보고 기존 발행 경로로 폴백한다.
            let citable = if content.marker.is_empty() {
                None
            } else {
                let hits = client
                    .search_knowledge(None, &content.marker, 5)
                    .unwrap_or_else(|e| {
                        log::warn!("hub {}: search 실패(발행으로 폴백): {e}", f.dedup_key);
                        Vec::new()
                    });
                hub::pick_citable(&hits, &content.marker, &cfg.user_id).cloned()
            };

            let issue_id = match client.open_issue(&cfg.space_id, &content.title) {
                Ok(id) => id,
                Err(e) => { log::warn!("hub {}: open_issue 실패: {e}", f.dedup_key); continue; }
            };
            if let Ok(store) = store_mutex.lock() {
                let _ = store.hub_mark_issue(&f.dedup_key, &issue_id, &now);
            }

            match citable {
                Some(hit) => {
                    match client.cite_knowledge(&issue_id, &hit.page_id, "a-mate: 같은 진단을 로컬에서 재확인") {
                        Ok(_) => {
                            // 인용도 처리 완료 — 평생 1회 원칙 유지 (page_id는 인용한 상대 페이지)
                            if let Ok(store) = store_mutex.lock() {
                                let _ = store.hub_mark_published(&f.dedup_key, &hit.page_id, &now);
                            }
                            cited += 1;
                        }
                        Err(e) => log::warn!("hub {}: cite 실패(다음 스캔 재개): {e}", f.dedup_key),
                    }
                }
                None => match client.resolve_issue(&issue_id, &content.summary, &content.steps) {
                    Ok(Some(page_id)) => {
                        if let Ok(store) = store_mutex.lock() {
                            let _ = store.hub_mark_published(&f.dedup_key, &page_id, &now);
                        }
                        published += 1;
                    }
                    Ok(None) => log::warn!("hub {}: resolve 응답에 page_id 없음", f.dedup_key),
                    Err(e) => log::warn!("hub {}: resolve 실패(다음 스캔 재개): {e}", f.dedup_key),
                },
            }
        }

        if published > 0 || cited > 0 {
            log::info!(
                "hub 지식 공유: {published}건 발행, {cited}건 인용(재사용) (space {})",
                cfg.space_id
            );
        }
    }

    /// 텔레메트리(#46 목표 아키텍처) — 전날치 파생 신호를 하루 1회 발행.
    /// 락 규율: 브리프 조립(store 읽기)은 짧은 락, 네트워크는 락 밖, 마크는 짧은 락.
    fn maybe_push_telemetry(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::hub::{self, HubClient, HubConfig};
        // 설정(store) → env 순으로 해석. 짧은 락만 잡고 즉시 해제(네트워크 전 해제 규율).
        let Some(cfg) = (match store_mutex.lock() {
            Ok(store) => HubConfig::resolve(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }) else { return };
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let key = hub::telemetry_state_key(&yesterday);

        // ① 짧은 락: 완료 여부·토큰·브리프
        let (already, token_opt, brief) = match store_mutex.lock() {
            Ok(store) => {
                let already = store
                    .hub_shared_or_pending_keys()
                    .map(|k| k.contains(&key))
                    .unwrap_or(false);
                let token = cfg.token.clone().or(store.get_setting("knowledge_hub_token").ok().flatten());
                let brief = hub::build_telemetry_brief(&store, &yesterday, &cfg.user_id).ok();
                (already, token, brief)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if already { return; }
        let Some(brief) = brief else { return };
        if hub::telemetry_is_empty(&brief) { return; } // 활동 없는 날은 노이즈 — 발행 생략
        // 부트스트랩(2026-07-19): 공유할 발견이 없어도 텔레메트리는 나가야 한다 — 직접 register
        let token = match token_opt {
            Some(t) => t,
            None => match HubClient::register(&cfg) {
                Ok((agent_id, t)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.set_setting("knowledge_hub_agent_id", &agent_id);
                        let _ = store.set_setting("knowledge_hub_token", &t);
                    }
                    t
                }
                Err(e) => { log::warn!("telemetry register 실패(다음 스캔 재시도): {e}"); return; }
            },
        };

        // ② 락 밖: 발행 (공간 부트스트랩 포함)
        let mut client = HubClient { base_url: cfg.base_url.clone(), api_key: cfg.api_key.clone(), token };
        let space = hub::telemetry_space_id();
        let title = format!("[telemetry] {} {}", cfg.user_id, yesterday);
        let body = match serde_json::to_string_pretty(&brief) {
            Ok(b) => b,
            Err(e) => { log::warn!("telemetry 직렬화 실패: {e}"); return; }
        };
        let page_id = match client.create_page(&space, &title, &body) {
            Ok(id) => id,
            Err(_) => {
                let _ = client.create_space(&space, "a-mate telemetry");
                match HubClient::register_into(&cfg, &space) {
                    Ok((agent_id, t)) => {
                        if let Ok(store) = store_mutex.lock() {
                            let _ = store.set_setting("knowledge_hub_agent_id", &agent_id);
                            let _ = store.set_setting("knowledge_hub_token", &t);
                        }
                        client.token = t;
                    }
                    Err(e) => { log::warn!("telemetry 멤버십 확보 실패(다음 스캔 재시도): {e}"); return; }
                }
                match client.create_page(&space, &title, &body) {
                    Ok(id) => id,
                    Err(e) => { log::warn!("telemetry 발행 실패(다음 스캔 재시도): {e}"); return; }
                }
            }
        };

        // ③ 짧은 락: 완료 마크
        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(store) = store_mutex.lock() {
            let _ = store.hub_mark_published(&key, &page_id, &now);
        }
        log::info!("telemetry 발행: {yesterday} → {page_id} (space {space})");
    }

    /// 세션 회고(스펙 2026-07-19) — 고생 끝 해결 세션을 Engine 요약으로 발행.
    /// 락 규율: 선별·토큰은 짧은 락, Engine·네트워크는 락 밖, 마크는 짧은 락.
    fn maybe_post_retros(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::diary::engine::Engine as _;
        use agent_mentor::hub::{self, HubClient, HubConfig};
        if !hub::retro_enabled() { return; }
        // 설정(store) → env 순으로 해석. 짧은 락만 잡고 즉시 해제(네트워크 전 해제 규율).
        let Some(cfg) = (match store_mutex.lock() {
            Ok(store) => HubConfig::resolve(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }) else { return };

        // ① 짧은 락: 후보·토큰·엔진 해석
        let (candidates, token_opt, engine) = match store_mutex.lock() {
            Ok(store) => {
                let cands = hub::select_retros(&store, chrono::Utc::now()).unwrap_or_default();
                let token = cfg.token.clone().or(store.get_setting("knowledge_hub_token").ok().flatten());
                let engine = crate::resolve_engine(&store);
                (cands, token, engine)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if candidates.is_empty() { return; }
        let Some(engine) = engine else { return };   // Engine 없으면 보류 (품질 > 정시성)
        // 부트스트랩(2026-07-19): 회고도 자체 register — 공유 발견 유무와 독립
        let token = match token_opt {
            Some(t) => t,
            None => match HubClient::register(&cfg) {
                Ok((agent_id, t)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.set_setting("knowledge_hub_agent_id", &agent_id);
                        let _ = store.set_setting("knowledge_hub_token", &t);
                    }
                    t
                }
                Err(e) => { log::warn!("retro register 실패(다음 스캔 재시도): {e}"); return; }
            },
        };

        let client = HubClient { base_url: cfg.base_url.clone(), api_key: cfg.api_key.clone(), token };
        let now = chrono::Utc::now().to_rfc3339();

        // ② 락 밖: Engine 요약 → issue→resolve
        for (key, s) in candidates {
            let (system, user) = hub::retro_prompt(&s);
            let reply = match engine.generate(&system, &user) {
                Ok(o) => o.text,
                Err(e) => { log::warn!("retro engine 실패(보류): {e}"); continue; }
            };
            let Some((title, summary)) = hub::parse_retro_reply(&reply) else {
                log::warn!("retro 응답 파싱 실패(보류)");
                continue;
            };
            let issue_id = match client.open_issue(&cfg.space_id, &format!("[a-mate 회고] {title}")) {
                Ok(id) => id,
                Err(e) => { log::warn!("retro open_issue 실패: {e}"); continue; }
            };
            if let Ok(store) = store_mutex.lock() {
                let _ = store.hub_mark_issue(&key, &issue_id, &now);
            }
            match client.resolve_issue(&issue_id, &summary, &hub::retro_steps(&s)) {
                Ok(Some(page_id)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.hub_mark_published(&key, &page_id, &now);
                    }
                    log::info!("세션 회고 발행: {} → {page_id}", &s.session_id[..8.min(s.session_id.len())]);
                }
                Ok(None) => log::warn!("retro resolve 응답에 page_id 없음"),
                Err(e) => log::warn!("retro resolve 실패(다음 스캔 재개): {e}"),
            }
        }
    }


    /// AI 스프라이트(2026-07-19) — app_data/sprite.png 없고 이미지 모델 설정이 있으면 1회 생성.
    /// 네트워크는 락과 무관(파일·env만). 성공 시 sprite:ready emit → 프론트 즉시 교체.

    /// 외부 문서(code.claude.com) 도달성 프로브 — 앱 실행당 1회. 내부망(차단)이면
    /// docs_reachable=false 를 남겨 프론트가 "공식 가이드" 링크를 숨긴다 (동료 이슈:
    /// "오늘의 배움 패널의 anthropic 가이드 링크는 내부망에서 연결 안 됨").
    fn maybe_probe_docs(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static PROBED: AtomicBool = AtomicBool::new(false);
        if PROBED.swap(true, Ordering::SeqCst) { return; }
        // 락 밖 네트워크 — 3초 타임아웃 HEAD (core 헬퍼)
        let ok = agent_mentor::ops::probe_docs_reachable();
        if let Ok(store) = store_mutex.lock() {
            let _ = store.set_setting("docs_reachable", if ok { "true" } else { "false" });
        }
        if !ok { log::info!("외부 문서 미도달(내부망?) — 배움 카드 외부 링크 숨김"); }
    }

    fn maybe_generate_sprite(app: &AppHandle) {
        use agent_mentor::sprite;
        let Ok(dir) = app.path().app_data_dir() else { return };
        let path = dir.join("sprite.png");
        if path.exists() { return; }
        // 설정창(image_*) → env 순 해석 + 프로필(uuid·mbti). 락은 해석 동안만 (네트워크 전 해제 규율).
        let state = app.state::<crate::AppState>();
        let (cfg, uuid, mbti) = match state.store.lock() {
            Ok(store) => {
                let cfg = crate::resolve_sprite_cfg(&store);
                match crate::commands::sprite_identity(&store) {
                    Ok((u, m)) => (cfg, u, m),
                    Err(e) => { log::warn!("프로필 해석 실패: {e}"); return; }
                }
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(cfg) = cfg else { return };
        let spec = agent_mentor::mascot::robot_spec_from_profile(&uuid, mbti.as_deref());
        let desc = sprite::character_description(&spec, mbti.as_deref(), &uuid);
        match sprite::generate(&cfg, &desc) {
            Ok(png) => {
                let _ = std::fs::create_dir_all(&dir);
                if std::fs::write(&path, png).is_ok() {
                    log::info!("AI 스프라이트 생성 완료: {}", path.display());
                    let _ = app.emit("sprite:ready", ());
                }
            }
            Err(e) => log::warn!("AI 스프라이트 생성 실패(다음 스캔 재시도): {e}"),
        }
    }

    fn maybe_generate_chatter_pool(store_mutex: &std::sync::Mutex<SqliteStore>) {
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();

        // ① 락: 오늘 컨텍스트 + 근무 맥락(코믹 소재) + 캐시된 fingerprint 읽기 → 즉시 해제
        let (ctx, work, cached_fp) = match store_mutex.lock() {
            Ok(store) => {
                let ctx = match crate::commands::chat_context_inner(&store) {
                    Ok(c) => c,
                    Err(e) => { log::warn!("chatter chat_context 실패: {e}"); return; }
                };
                let work =
                    agent_mentor::diary::collect_work_context(&store, &today, now.date_naive());
                let cached_fp = match store.get_chatter_pool(&today) {
                    Ok(v) => v.map(|(_, fp)| fp),
                    Err(e) => { log::warn!("get_chatter_pool 실패: {e}"); return; }
                };
                (ctx, work, cached_fp)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // ② 락 없이 compute (0건 빈 풀 or 네트워크 생성). None이면 skip.
        let outcome = match agent_mentor::mascot::compute_chatter_pool(&engine, &ctx, &work, cached_fp.as_deref()) {
            Ok(o) => o,
            Err(e) => { log::warn!("compute_chatter_pool 실패: {e}"); return; }
        };
        let Some((lines, fp)) = outcome else { return; };

        // ③ 락: 캐시 upsert → 즉시 해제
        match store_mutex.lock() {
            Ok(store) => {
                if let Err(e) = store.upsert_chatter_pool(&today, &lines, &fp) {
                    log::warn!("upsert_chatter_pool 실패: {e}");
                }
            }
            Err(e) => log::warn!("store lock poisoned: {e}"),
        }
    }

    /// G3 — 내 방 방명록의 미답글 원글에 봇 성향 답글 (스펙 §A·§E). hub 미연결·엔진
    /// 미설정이면 no-op. 모든 실패는 warn 후 skip — 다음 스캔 재시도. 구서버(G2 미배포)가
    /// parent_id를 무시하면 답글이 원글로 저장돼 dedup이 깨지고 스캔마다 도배되므로,
    /// POST 응답의 parent_id 에코를 검증해 불일치 시 앱 실행 동안 비활성(maybe_probe_docs 선례).
    fn maybe_reply_guestbook(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static INCOMPATIBLE: AtomicBool = AtomicBool::new(false);
        if INCOMPATIBLE.load(Ordering::SeqCst) { return; }

        // ① 락: 엔진·hub 설정·페르소나 읽기 → 즉시 해제 (maybe_generate_chatter_pool 선례)
        let (engine, url, token, api_key, life_id, agent_id, title, user_name, mbti, vibe) =
            match store_mutex.lock() {
                Ok(store) => {
                    let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                    // G5: 답글에 녹일 거친 근황(수치 없이 vibe만) — 잡담 선례와 동일 재료.
                    let now = chrono::Local::now();
                    let today = now.format("%Y-%m-%d").to_string();
                    let (session_count, tokens_today) = crate::commands::chat_context_inner(&store)
                        .map(|c| (c.session_count, c.tok_input + c.tok_output)).unwrap_or((0, 0));
                    let work = agent_mentor::diary::collect_work_context(&store, &today, now.date_naive());
                    let vibe = agent_mentor::mascot::owner_vibe(session_count, tokens_today, &work);
                    (
                        crate::resolve_engine(&store),
                        get("hub_url"), get("hub_token"), get("hub_api_key"),
                        get("hub_life_id"), get("hub_agent_id"),
                        crate::commands::owner_title(&store), get("user_name"), get("user_mbti"),
                        vibe,
                    )
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
        let Some(engine) = engine else { return; };
        if url.trim().is_empty() || token.is_empty() || life_id.is_empty() || agent_id.is_empty() {
            return;
        }

        // ② 락 없이 네트워크: 조회 → 선정 → 후보별 [생성 → 게시]
        let client = agent_mentor::life_client::LifeClient {
            base_url: url,
            token,
            api_key: { let k = api_key.trim(); (!k.is_empty()).then(|| k.to_string()) },
        };
        let entries = match client.guestbook(&life_id) {
            Ok(v) => v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default(),
            Err(e) => { log::warn!("방명록 자동 답글: 조회 실패(다음 스캔 재시도): {e}"); return; }
        };
        let targets = agent_mentor::mascot::select_reply_targets(
            &entries, &agent_id, agent_mentor::mascot::GUESTBOOK_REPLY_MAX_PER_SCAN);
        if targets.is_empty() { return; }
        let author = agent_mentor::mascot::bot_author_name(&user_name);
        let mbti = agent_mentor::mascot::normalize_mbti(&mbti);
        for t in &targets {
            let reply = match agent_mentor::mascot::compute_guestbook_reply(
                &engine, &title, mbti.as_deref(), vibe,
                agent_mentor::mascot::should_ask_about_bot(&t.entry_id), t)
            {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("방명록 자동 답글: 생성 실패(entry {}): {e}", t.entry_id);
                    continue;
                }
            };
            // 생성(수 초~수십 초) 동안 주인이 수동 답글을 달았거나 원글이 지워졌을 수 있다 —
            // 게시 직전 재조회로 재확인 (Codex 리뷰: TOCTOU 중복 답글. 서버는 ADR 0021대로
            // 원글당 답글 무제한이라 "글당 1회"는 여기서 지킨다). 재확인 실패 시 보수적으로 skip.
            let still_target = match client.guestbook(&life_id) {
                Ok(v) => {
                    let fresh = v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                    agent_mentor::mascot::select_reply_targets(&fresh, &agent_id, usize::MAX)
                        .iter().any(|x| x.entry_id == t.entry_id)
                }
                Err(e) => {
                    log::warn!("방명록 자동 답글: 게시 전 재확인 실패(entry {}): {e}", t.entry_id);
                    false
                }
            };
            if !still_target {
                continue;
            }
            match client.add_guestbook(&life_id, &reply, author.as_deref(), Some(&t.entry_id)) {
                Ok(resp) => {
                    let echoed = resp.get("parent_id").and_then(|p| p.as_str())
                        == Some(t.entry_id.as_str());
                    if !echoed {
                        log::warn!("방명록 자동 답글: 서버가 parent_id 미지원(G2 미배포) — 이번 실행 동안 비활성");
                        INCOMPATIBLE.store(true, Ordering::SeqCst);
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("방명록 자동 답글: 게시 실패(entry {}): {e}", t.entry_id);
                    continue;
                }
            }
        }
    }
}

#[cfg(not(test))]
pub use runtime::start;
