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

    /// 락을 놓은 뒤 대기 커맨드가 실제로 잡을 틈을 만든다. `yield_now`만으로는 부족했다 —
    /// Windows `SwitchToThread`는 같은 프로세서의 ready 스레드에만 양보하므로 멀티코어에서는
    /// 스캔 스레드가 그대로 재획득한다. 실측(X1): ingest가 짧은 웜 스캔에서 커맨드가
    /// inventory+rules를 통째로 기다려 898ms가 나왔다. 상한을 둬서 커맨드가 끊이지 않을 때
    /// (홈 탭 2초 폴링) 스캔이 굶지 않게 한다.
    fn hand_off(state: &AppState) {
        use std::sync::atomic::Ordering;
        for _ in 0..2_000 {
            if state.store_waiters.load(Ordering::Acquire) == 0 {
                break;
            }
            std::thread::yield_now();
        }
    }

    /// 스캔 1회. 락을 **단계·파일 단위로** 잡는다 — 통짜로 쥐면 마이그레이션 후 전량 재수집
    /// (실측 16.7초) 동안 모든 커맨드가 막힌다(X1). 최장 연속 보유는 rules 블록이다.
    pub fn run_pipeline_once(app: &AppHandle) {
        let state = app.state::<AppState>();
        let scan_result = (|| -> anyhow::Result<String> {
            let scan_started = std::time::Instant::now();
            let mut longest_hold = Duration::ZERO;

            // ── 1) discover — DB를 쓰지 않으므로 락 밖 ──────────────────────────
            let (work, warnings) = agent_mentor::ops::discover_work();
            for w in &warnings {
                log::warn!("{w}");
            }
            let total: usize = work.iter().map(|w| w.files.len()).sum();

            // ── 2) 파일 수집 — 파일마다 락을 잡고 놓는다 ─────────────────────────
            let mut ingest_held = Duration::ZERO;
            let mut slowest_file = Duration::ZERO;
            let mut new_events = 0usize;
            let mut done = 0usize;
            for w in &work {
                for f in &w.files {
                    let (held, result) = {
                        let store = state
                            .store
                            .lock()
                            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                        let t = std::time::Instant::now();
                        let result = agent_mentor::store::ingest_file(&store, &w.adapter, f);
                        (t.elapsed(), result)
                    }; // guard drops here — 다음 파일 전에 대기 커맨드가 끼어든다
                    // std::sync::Mutex는 공정하지 않다. 해제 직후 이 스레드가 재획득하면
                    // 청킹이 무효가 되므로 대기자가 잡을 때까지 넘긴다. sleep은 쓰지 않는다 —
                    // Windows 타이머 해상도가 ~15ms라 파일당 sleep은 스캔에 수 초를 붙인다.
                    hand_off(&state);
                    match result {
                        Ok(n) => new_events += n,
                        Err(e) => log::warn!("{} 수집 실패(건너뜀): {e}", f.display()),
                    }
                    ingest_held += held;
                    if held > slowest_file {
                        slowest_file = held;
                    }
                    if held > longest_hold {
                        longest_hold = held;
                    }
                    done += 1;
                    // 파일 수천 개일 수 있어 5건 단위로만 emit (마지막은 항상)
                    if done == total || done % 5 == 0 {
                        let _ = app
                            .emit("scan:progress", serde_json::json!({"done": done, "total": total}));
                    }
                }
            }

            // ── 3) rollup ────────────────────────────────────────────────────
            let rollup_held = {
                let store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                store.rebuild_rollup()?;
                t.elapsed()
            };
            hand_off(&state);
            if rollup_held > longest_hold {
                longest_hold = rollup_held;
            }

            // ── 4) inventory ─────────────────────────────────────────────────
            let inventory_held = {
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                for w in agent_mentor::ops::run_inventory(&mut store)? {
                    log::warn!("{w}");
                }
                t.elapsed()
            };
            hand_off(&state);
            if inventory_held > longest_hold {
                longest_hold = inventory_held;
            }

            // ── 5) rules + diff + emit — 한 블록 ──────────────────────────────
            // before/run_rules/after/diff를 쪼개지 않는다. 지금은 쪼개도 안전하지만
            // (findings를 쓰는 커맨드는 set_finding_status 하나뿐이고 status만 바꾸며,
            // finding_severities는 status를 안 본다) 그 안전이 다른 파일의 사실 두 개에
            // 의존한다. 한 블록이면 불변식이 여기서 보인다 (X1 설계 §A).
            let (now, rules_held) = {
                let store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                let before: HashMap<String, String> =
                    store.finding_severities()?.into_iter().collect();
                agent_mentor::ops::run_rules(&store)?;
                let after = store.finding_severities()?;
                let fresh = diff_findings(&before, &after);
                let now = chrono::Utc::now().to_rfc3339();
                store.set_setting("last_scan_ts", &now)?;
                if !fresh.is_empty() {
                    let rows: Vec<_> = store
                        .list_findings_current(false)?
                        .into_iter()
                        .filter(|f| fresh.contains(&f.dedup_key))
                        .collect();
                    app.emit("coach:finding", &rows)?;
                }
                (now, t.elapsed())
                // guard drops here — 다이어리 생성(LLM 네트워크 I/O) 전에 락 해제
            };
            if rules_held > longest_hold {
                longest_hold = rules_held;
            }

            // 성능 telemetry — 스캔당 1줄. X1의 회귀를 보는 유일한 창이므로 영구 유지한다.
            // 핵심 지표는 합계가 아니라 **longest**다: 커맨드가 기다리는 시간이 그것이다.
            log::info!(
                "scan done — wall {}ms | ingest {}ms ({total} files, {new_events} events, slowest file {}ms) \
                 | rollup {}ms | inventory {}ms | rules {}ms | longest lock hold {}ms",
                scan_started.elapsed().as_millis(),
                ingest_held.as_millis(),
                slowest_file.as_millis(),
                rollup_held.as_millis(),
                inventory_held.as_millis(),
                rules_held.as_millis(),
                longest_hold.as_millis(),
            );
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
                // 묶음 ② 방문 기록 보존(30일) — 방문·토글과 무관하게 스캔마다 (ADR 0025)
                crate::visit::prune_visit_history(&state.store);
                // 묶음 ② 자율 방문 — 주말·공휴일 하루 1방(토글 off·hub 미연결·일촌 없으면 no-op)
                crate::visit::maybe_auto_visit(&state.store);
                // P4+N1 인바운드 소식 — 내 방 방문·방명록 diff를 emit (hub 미연결·구서버 no-op)
                maybe_poll_inbound(app, &state.store);
                // a-hub 지식 공유 — 유의미 finding을 이슈→해결로 발행 (env 미설정 시 no-op)
                maybe_share_findings(&state.store);
                // 텔레메트리(#46) — 전날 파생 신호 하루 1회 발행 (env 미설정 시 no-op)
                maybe_push_telemetry(&state.store);
                // 세션 회고 — "고생 끝 해결" 세션을 로컬 생성 요약으로 발행 (Engine 필요)
                maybe_post_retros(&state.store);
                // AI 스프라이트 — 캐시 없으면 1회 생성 (실패 무해, 절차 생성 폴백)
                maybe_generate_sprite(app);
                // H2 매일 마스코트 컷 — 옵트인(기본 off)·일기 파생·일일 3회 상한 (실패는 조용히)
                maybe_generate_daily_cut(app);
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
                    visits: brief.visits.clone(),
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
        // 신규 후보가 없어도 미완 재개는 해야 한다 — 재개 경로가 없으면 pending이 재선별에서
        // 빠져 그 세션이 영구 미발행이 된다(2026-07-30 발견).
        // 짧은 락으로 (pending, 그 세션들) 을 함께 떠온다 — 네트워크는 락 밖에서.
        let resumables: Vec<(String, String, agent_mentor::store::StruggleSession)> =
            match store_mutex.lock() {
                Ok(store) => {
                    let pendings = hub::retro_pendings(&store).unwrap_or_default();
                    if pendings.is_empty() {
                        Vec::new()
                    } else {
                        // 문턱·정착 조건을 풀고 전 세션 — 이미 발행 대상으로 판정된 건들이다.
                        let all = store
                            .struggle_sessions(0, 0, hub::FAR_FUTURE, hub::FAR_FUTURE)
                            .unwrap_or_default();
                        pendings
                            .into_iter()
                            .filter_map(|(k, iss)| {
                                let sid = k.trim_start_matches("retro|").to_string();
                                all.iter()
                                    .find(|s| s.session_id == sid)
                                    .map(|s| (k, iss, s.clone()))
                            })
                            .collect()
                    }
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
        if candidates.is_empty() && resumables.is_empty() { return; }
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

        // ② 락 밖: 미완 재개 — 이슈는 이미 열려 있으니 resolve만 다시. 요약은 저장하지 않으므로
        //    Engine을 다시 부른다(결정론 폴백 없음 — 스펙 §3).
        for (key, issue_id, s) in resumables {
            let (system, user) = hub::retro_prompt(&s);
            let reply = match engine.generate(&system, &user) {
                Ok(o) => o.text,
                Err(e) => { log::warn!("retro resume engine 실패(다음 스캔 재개): {e}"); continue; }
            };
            let Some((_title, summary)) = hub::parse_retro_reply(&reply) else {
                log::warn!("retro resume 응답 파싱 실패(다음 스캔 재개)");
                continue;
            };
            let marker = hub::retro_project_marker(&s);
            match client.resolve_issue(
                &issue_id,
                &hub::retro_page_summary(marker.as_deref(), &summary),
                &hub::retro_steps(&s),
            ) {
                Ok(Some(page_id)) => {
                    if let Ok(store) = store_mutex.lock() {
                        let _ = store.hub_mark_published(&key, &page_id, &now);
                    }
                    log::info!(
                        "세션 회고 재개 발행: {} → {page_id}",
                        &s.session_id[..8.min(s.session_id.len())]
                    );
                }
                Ok(None) => log::warn!("retro resume: resolve 응답에 page_id 없음 ({key})"),
                Err(e) => log::warn!("retro resume resolve 실패(다음 스캔 재개): {e}"),
            }
        }

        // ③ 락 밖: Engine 요약 → issue→resolve
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
            // 프로젝트 마커는 세션당 한 번만 — git을 부르는 경로다.
            let marker = hub::retro_project_marker(&s);
            let issue_id = match client
                .open_issue(&cfg.space_id, &hub::retro_issue_title(marker.as_deref(), &title))
            {
                Ok(id) => id,
                Err(e) => { log::warn!("retro open_issue 실패: {e}"); continue; }
            };
            if let Ok(store) = store_mutex.lock() {
                let _ = store.hub_mark_issue(&key, &issue_id, &now);
            }
            match client.resolve_issue(
                &issue_id,
                &hub::retro_page_summary(marker.as_deref(), &summary),
                &hub::retro_steps(&s),
            ) {
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
                    // 이 sprite가 어떤 시드로 그려졌는지 기록 — 컷 재구성 정합·레거시 판별용
                    // (실패해도 무해: resolve_cut_seeds가 레거시로 취급해 uuid 정합 폴백)
                    let _ = std::fs::write(dir.join("sprite.seed"), &uuid);
                    log::info!("AI 스프라이트 생성 완료: {}", path.display());
                    let _ = app.emit("sprite:ready", ());
                }
            }
            Err(e) => log::warn!("AI 스프라이트 생성 실패(다음 스캔 재시도): {e}"),
        }
    }

    /// H2 — 매일 마스코트 컷 (자동 경로). 옵트인(daily_cut_enabled 기본 off) 게이트만 여기서
    /// 보고, 생성 코어는 설정 탭 "지금 그려보기" 버튼과 공유한다 (`generate_daily_cut_core`).
    fn maybe_generate_daily_cut(app: &AppHandle) {
        let enabled = match app.state::<crate::AppState>().store.lock() {
            Ok(store) => store
                .get_setting("daily_cut_enabled")
                .ok()
                .flatten()
                .map(|v| v == "true")
                .unwrap_or(false),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if !enabled { return; }
        match generate_daily_cut_core(app, false) {
            Ok(Some(date)) => log::info!("매일 컷 생성 완료({date})"),
            Ok(None) => {} // 리컨실리에이션 skip (멱등·상한)
            Err(e) => log::warn!("daily-cut 생성 실패(다음 스캔 재시도 가능): {e}"),
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
                // P3: 봇이 쓴 원글(author_kind=bot)엔 "방문자의 봇 안부" 질문이 어색 — 억제
                agent_mentor::mascot::should_ask_about_bot(&t.entry_id) && !t.author_is_bot, t)
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
            match client.add_guestbook(&life_id, &reply, author.as_deref(), Some(&t.entry_id), Some("bot")) {
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

    /// P4+N1 — 인바운드 소식 폴링 (스캔 편승, 스펙 §4). 내 방 방문을 settings 커서
    /// 기준 diff 후 life:visit emit. hub 미연결이면 no-op, 모든 실패는 warn 후 다음
    /// 스캔 재시도. 커서 저장은 emit 성공 후 — 실패 시 커서 미갱신으로 재-emit된다.
    /// 구서버(visits 404)는 이번 실행 동안 방문 폴링만 비활성(maybe_reply_guestbook
    /// INCOMPATIBLE 선례). 기존 maybe_reply_guestbook은 건드리지 않는다(묶음 ② 충돌 억제).
    /// 알림 창이 구독을 끝냈다고 신고한 직후의 1회 폴링. 스캔은 파일 변경 구동(주기 타이머
    /// 없음)이라 준비 이전에 스캔이 지나갔으면 다음 변경까지 소식이 안 온다 — 그 공백을 메운다.
    /// 네트워크를 타므로 커맨드 스레드를 막지 않게 별 스레드에서.
    pub fn poll_inbound_now(app: AppHandle) {
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            maybe_poll_inbound(&app, &state.store);
        });
    }

    fn maybe_poll_inbound(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static VISITS_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

        // ⓪ 알림 창이 구독을 끝내기 전에는 폴링하지 않는다. emit은 수신자가 0이어도 Ok라서
        // 여기서 커서를 저장하면 앱 꺼진 동안 쌓인 소식이 영구 유실된다 (Codex 리뷰 P1).
        // 커서를 그대로 두면 다음 스캔이 같은 구간을 다시 받아온다 — 손실 없음.
        if !app.state::<AppState>().notices_ready.load(Ordering::SeqCst) {
            return;
        }

        // ① 락: 설정·커서 스냅샷 → 즉시 해제 (maybe_reply_guestbook 선례)
        let (url, token, api_key, life_id, agent_id, visits_cursor, gb_cursor) = match store_mutex.lock() {
            Ok(store) => {
                let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                let cursor = |k: &str| store.get_setting(k).ok().flatten().filter(|v: &String| !v.is_empty());
                (get("hub_url"), get("hub_token"), get("hub_api_key"),
                 get("hub_life_id"), get("hub_agent_id"),
                 cursor("inbound_visits_cursor"), cursor("inbound_guestbook_cursor"))
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if url.trim().is_empty() || token.is_empty() || life_id.is_empty() || agent_id.is_empty() {
            return;
        }
        let client = agent_mentor::life_client::LifeClient {
            base_url: url,
            token,
            api_key: { let k = api_key.trim(); (!k.is_empty()).then(|| k.to_string()) },
        };
        let save_cursor = |key: &str, val: &Option<String>| {
            let Some(v) = val.as_deref() else { return };
            match store_mutex.lock() {
                Ok(store) => {
                    if let Err(e) = store.set_setting(key, v) { log::warn!("{key} 저장 실패: {e}"); }
                }
                Err(e) => log::warn!("store lock poisoned: {e}"),
            }
        };

        // ② 락 없이 네트워크: 방문 diff → life:visit
        if !VISITS_UNSUPPORTED.load(Ordering::SeqCst) {
            match client.visits(visits_cursor.as_deref(), 100) {
                Ok(v) => {
                    let rows = v.get("visits").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let (fresh, next) =
                        agent_mentor::inbound::select_new_visits(&rows, visits_cursor.as_deref());
                    if fresh.is_empty() || app.emit("life:visit", &fresh).is_ok() {
                        save_cursor("inbound_visits_cursor", &next);
                    } else {
                        log::warn!("life:visit emit 실패 — 다음 스캔 재시도");
                    }
                }
                // err_of가 "(HTTP 404)"를 접미한다 — 구서버 판별
                Err(e) if e.to_string().contains("(HTTP 404)") => {
                    log::warn!("방문 폴링: 서버가 visits 미지원(구서버) — 이번 실행 동안 비활성");
                    VISITS_UNSUPPORTED.store(true, Ordering::SeqCst);
                }
                Err(e) => log::warn!("방문 폴링: 조회 실패(다음 스캔 재시도): {e}"),
            }
        }

        // ③ 방명록 diff → guestbook:new (타인 글만 — 내 작성분 제외는 core 판정)
        match client.guestbook(&life_id) {
            Ok(v) => {
                let entries = v.get("entries").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let (fresh, next) =
                    agent_mentor::inbound::select_new_guestbook(&entries, &agent_id, gb_cursor.as_deref());
                if fresh.is_empty() || app.emit("guestbook:new", &fresh).is_ok() {
                    save_cursor("inbound_guestbook_cursor", &next);
                } else {
                    log::warn!("guestbook:new emit 실패 — 다음 스캔 재시도");
                }
            }
            Err(e) => log::warn!("방명록 신규 폴링: 조회 실패(다음 스캔 재시도): {e}"),
        }
    }
}

// H2 daily_cut.json 상태 파일 헬퍼 — runtime 모듈이 #[cfg(not(test))]라 테스트 가능하도록
// 파일 루트에 둔다 (순수 fs 로직, Tauri 비의존).

/// daily_cut.json 로드 — 없거나 손상이면 Default (다음 판단이 안전하게 재시작).
pub(crate) fn cut_state_load(dir: &std::path::Path) -> agent_mentor::sprite::CutState {
    std::fs::read_to_string(dir.join("daily_cut.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 저장 실패를 삼키지 않는다 — attempts는 과금 상한의 원장이라, 호출자가 실패 시
/// 네트워크 진입을 중단해야 한다 (Codex 리뷰 P1). tmp→rename으로 부분 쓰기도 방지.
pub(crate) fn cut_state_save(
    dir: &std::path::Path,
    s: &agent_mentor::sprite::CutState,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("daily_cut.json.tmp");
    std::fs::write(&tmp, serde_json::to_string(s)?)?;
    std::fs::rename(&tmp, dir.join("daily_cut.json"))?;
    Ok(())
}

/// H2 — 컷의 (스펙 시드, 변주 시드) 결정. 원칙 = "화면에 걸린 sprite와 같은 로봇".
/// - sprite.seed 있음(자동 생성 기록·재생성 승격): 스펙=현재 정체성, 변주=그 시드.
/// - 시드 파일 없이 sprite.png만 있음 = **레거시**(시드 도입 전 생성) → 그 시절 규칙인
///   순수 uuid로 스펙·변주 모두 (이름 섞인 새 시드로 재구성하면 다른 로봇이 됨 — Codex 리뷰 P2).
/// - 둘 다 없음(초기): 현재 정체성 시드.
pub(crate) fn resolve_cut_seeds(
    dir: &std::path::Path,
    identity: &str,
    raw_uuid: &str,
) -> (String, String) {
    let seed_file = std::fs::read_to_string(dir.join("sprite.seed"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    match seed_file {
        Some(s) => (identity.to_string(), s),
        None if dir.join("sprite.png").is_file() => (raw_uuid.to_string(), raw_uuid.to_string()),
        None => (identity.to_string(), identity.to_string()),
    }
}

/// H2 — 컷 생성 코어 (자동 경로 `maybe_generate_daily_cut` + 설정 탭 "지금 그려보기" 공용).
/// `force=true`(버튼)면 멱등·일일 상한 판정을 건너뛰고 즉시 새로 그린다 (mascot_preview 선례
/// — 명시적 버튼은 누를 때마다 생성). 원장(attempts) 기록은 force에서도 동일하게 남긴다.
/// 네트워크는 텍스트 → 이미지 순서, 모두 store 락 밖.
/// 반환: Ok(Some(date))=생성·교체 완료, Ok(None)=리컨실리에이션 skip(force=false 전용),
/// Err(msg)=사용자에게 그대로 보여줄 수 있는 한국어 실패 사유.
/// 스펙: docs/archive/design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md
/// 동시 생성 가드 — 수동 버튼(tauri 워커)과 자동 스캔(파이프라인 스레드)이 겹치면 같은
/// daily_cut.json을 읽고 각자 attempt를 기록해 일일 상한이 뚫린다 (Codex 리뷰 P1).
/// try_lock으로 후발 호출은 즉시 거절 — 버튼엔 "이미 그리는 중" 안내, 스캔은 다음 회 재시도.
/// (멀티 인스턴스는 AGENT_MENTOR_DATA_DIR로 데이터 디렉터리를 분리하는 게 전제 — 범위 밖.)
static CUT_GENERATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn generate_daily_cut_core(
    app: &tauri::AppHandle,
    force: bool,
) -> Result<Option<String>, String> {
    use agent_mentor::sprite;
    use tauri::{Emitter as _, Manager as _};
    let Ok(_gen_guard) = CUT_GENERATION_LOCK.try_lock() else {
        return Err("이미 대문사진을 그리는 중이에요 — 잠시 후 다시 시도해주세요".into());
    };
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let state = app.state::<crate::AppState>();

    // ① 짧은 락: 재료 읽기 → 즉시 해제 (네트워크 전 해제 규율)
    let (engine, cfg, identity, raw_uuid, mbti, date, diary) = {
        let store = state.lock_store().map_err(|_| "store lock poisoned".to_string())?;
        let Some(engine) = crate::resolve_engine(&store) else {
            return Err("텍스트 엔진이 설정되지 않았어요 — 설정 → 연결 → 텍스트 엔진을 확인해주세요".into());
        };
        let Some(cfg) = crate::resolve_sprite_cfg(&store) else {
            return Err("이미지 모델이 설정되지 않았어요 — 설정 → 연결 → 캐릭터 이미지에서 URL·키를 넣어주세요".into());
        };
        let (identity, mbti) = crate::commands::sprite_identity(&store)
            .map_err(|e| format!("프로필 해석 실패: {e}"))?;
        // 레거시 sprite(시드 파일 없음) 정합용 순수 uuid — resolve_cut_seeds가 사용
        let raw_uuid = crate::commands::ensure_uuid(&store)?;
        // diary_dates()는 date ASC 정렬 — 마지막이 최신 일기
        let Some(date) = store.diary_dates().map_err(|e| e.to_string())?.last().cloned() else {
            return Err("아직 일기가 없어요 — 첫 일기가 생긴 뒤 다시 시도해주세요".into());
        };
        let Some(diary) = crate::commands::diary_inner(&store, &date).map_err(|e| e.to_string())? else {
            return Err("일기 본문을 읽지 못했어요 — 다음 스캔 후 다시 시도해주세요".into());
        };
        (engine, cfg, identity, raw_uuid, mbti, date, diary)
    }; // guard drops here

    // ② 리컨실리에이션 (파일 IO만) — 자동 경로 전용. png 실존까지 확인해
    //    "메타만 완료" 고착(파일 삭제 등)을 방지. 버튼(force)은 즉시 새로 그린다.
    let mut cut = cut_state_load(&dir);
    if !force {
        let png_exists = dir.join("daily_cut.png").is_file();
        if !sprite::decide_cut(&date, &cut, png_exists) {
            return Ok(None);
        }
    }
    // 시도는 네트워크 **전에** persist — 실패·크래시에도 일일 상한 보장.
    // 원장 쓰기가 실패하면(디스크 풀·읽기 전용) 상한을 보장할 수 없으므로 진행하지 않는다.
    sprite::register_attempt(&mut cut, &date);
    cut_state_save(&dir, &cut).map_err(|e| format!("시도 기록 저장 실패 — 생성을 중단했어요: {e}"))?;

    // ③ 텍스트 엔진: 일기 → 추상 장면 + 캡션 (실패 시 이미지 호출 안 감)
    let shot = sprite::pick_cut_shot(&date, &identity);
    let (scene_en, caption) = match sprite::compute_cut_scene(&engine, &diary, shot, mbti.as_deref()) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(format!(
                "장면 응답을 해석하지 못했어요 ({}/{}회)",
                cut.attempts,
                sprite::MAX_CUT_ATTEMPTS_PER_DAY
            ))
        }
        Err(e) => {
            return Err(format!(
                "장면 생성 실패 ({}/{}회): {e}",
                cut.attempts,
                sprite::MAX_CUT_ATTEMPTS_PER_DAY
            ))
        }
    };

    // ④ 이미지 엔진: 화풍 앵커 + (마스코트 샷이면 현재 정체성 묘사) + 추상 장면
    //    시드는 resolve_cut_seeds가 결정 — 원칙은 "화면에 걸린 sprite와 같은 로봇"
    //    (레거시 sprite는 순수 uuid, 그 외엔 스펙=정체성·변주=sprite.seed).
    let (spec_seed, var_seed) = resolve_cut_seeds(&dir, &identity, &raw_uuid);
    let desc = shot.has_mascot().then(|| {
        let spec = agent_mentor::mascot::robot_spec_from_profile(&spec_seed, mbti.as_deref());
        let spec = agent_mentor::mascot::respec_pose_for_seed(spec, mbti.as_deref(), &var_seed);
        sprite::character_description(&spec, mbti.as_deref(), &var_seed)
    });
    let prompt = sprite::build_cut_image_prompt(shot, desc.as_deref(), &scene_en);
    let png = sprite::generate_cut(&cfg, &prompt).map_err(|e| {
        format!("이미지 생성 실패 ({}/{}회): {e}", cut.attempts, sprite::MAX_CUT_ATTEMPTS_PER_DAY)
    })?;

    // ⑤ 한 버전으로 발행 — tmp 쓰기 → 메타 선커밋 → rename (Codex 리뷰 P2).
    //    메타 저장이 실패하면 발행하지 않고(emit 없음) 기존 컷+메타 쌍 유지.
    //    rename이 실패하면 메타를 원복해 "새 메타 + 옛 이미지" 불일치를 막는다.
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("daily_cut.png.tmp");
    std::fs::write(&tmp, png).map_err(|e| format!("컷 저장 실패: {e}"))?;
    let prev = cut.clone();
    sprite::register_success(&mut cut, &date, &caption, shot);
    cut_state_save(&dir, &cut).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("컷 메타 저장 실패 — 발행 취소: {e}")
    })?;
    if let Err(e) = std::fs::rename(&tmp, dir.join("daily_cut.png")) {
        if let Err(e2) = cut_state_save(&dir, &prev) {
            log::warn!("daily-cut 메타 원복 실패(쌍 불일치 가능): {e2}");
        }
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("컷 교체 실패: {e}"));
    }
    let _ = app.emit("daily_cut:ready", &date);
    Ok(Some(date))
}

#[cfg(not(test))]
pub use runtime::{poll_inbound_now, start};

#[cfg(test)]
mod cut_state_tests {
    #[test]
    fn cut_state_roundtrips_and_survives_corruption() {
        let dir = tempfile::tempdir().unwrap();
        // 없으면 Default
        let s = super::cut_state_load(dir.path());
        assert_eq!(s, agent_mentor::sprite::CutState::default());
        // 저장 → 로드 왕복 (tmp→rename 경로)
        let mut st = agent_mentor::sprite::CutState::default();
        agent_mentor::sprite::register_attempt(&mut st, "2026-07-28");
        super::cut_state_save(dir.path(), &st).unwrap();
        assert_eq!(super::cut_state_load(dir.path()), st);
        assert!(!dir.path().join("daily_cut.json.tmp").exists(), "tmp는 rename으로 소진");
        // 손상 → Default로 재초기화 (스펙 에러 처리)
        std::fs::write(dir.path().join("daily_cut.json"), "{corrupt").unwrap();
        assert_eq!(
            super::cut_state_load(dir.path()),
            agent_mentor::sprite::CutState::default()
        );
    }

    #[test]
    fn resolve_cut_seeds_covers_fresh_legacy_and_reroll() {
        let dir = tempfile::tempdir().unwrap();
        let id = "u-1|둘쇠";
        let raw = "u-1";
        // ① 아무것도 없음(첫 실행, sprite도 아직) → 현재 정체성 시드로 스펙·변주 모두
        assert_eq!(super::resolve_cut_seeds(dir.path(), id, raw), (id.into(), id.into()));
        // ② sprite.png만 있고 시드 파일 없음 = 레거시(시드 도입 전) sprite
        //    → 그 시절 규칙인 순수 uuid로 정합 (화면의 sprite와 같은 로봇)
        std::fs::write(dir.path().join("sprite.png"), b"png").unwrap();
        assert_eq!(super::resolve_cut_seeds(dir.path(), id, raw), (raw.into(), raw.into()));
        // ③ 빈/공백 시드 파일도 레거시 취급
        std::fs::write(dir.path().join("sprite.seed"), "   ").unwrap();
        assert_eq!(super::resolve_cut_seeds(dir.path(), id, raw), (raw.into(), raw.into()));
        // ④ 시드 파일 존재(자동 생성 기록 or 재생성 승격) → 스펙=현재 정체성, 변주=그 시드
        std::fs::write(dir.path().join("sprite.seed"), "reroll-xyz\n").unwrap();
        assert_eq!(
            super::resolve_cut_seeds(dir.path(), id, raw),
            (id.into(), "reroll-xyz".into())
        );
    }
}
