use crate::AppState;
use agent_mentor::diary::engine::{ChatMessage, Engine, OpenAiCompatEngine};
use agent_mentor::mascot::{robot_spec_for, stable_identity, RobotSpec};
use agent_mentor::store::SqliteStore;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct Summary {
    pub date: String,
    pub user_name: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
    pub total_sessions: u64,
    pub est_tokens_saved_total: u64,
    pub last_scan: Option<String>,
}

pub fn summary_inner(store: &SqliteStore) -> anyhow::Result<Summary> {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let day = store.summary_for_date(&date)?;
    Ok(Summary {
        date,
        user_name: std::env::var("USERNAME").unwrap_or_else(|_| "user".into()),
        session_count: day.session_count,
        tok_input: day.tok_input,
        tok_output: day.tok_output,
        tok_cache_read: day.tok_cache_read,
        tok_cache_create: day.tok_cache_create,
        total_sessions: store.total_sessions()?,
        est_tokens_saved_total: store.sum_est_tokens_saved()?,
        last_scan: store.get_setting("last_scan_ts")?,
    })
}

pub fn diary_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Option<String>> {
    match store.diary_path_for(date)? {
        Some(path) => Ok(Some(std::fs::read_to_string(path)?)),
        None => Ok(None),
    }
}

pub fn daily_line_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Option<String>> {
    Ok(store.get_daily_line(date)?.map(|(text, _fp)| text))
}

#[derive(Debug, Serialize)]
pub struct SessionCtx {
    pub project_id: String,
    pub first_ts: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CoachFinding {
    #[serde(flatten)]
    pub row: agent_mentor::store::FindingRow,
    pub detail: String,
    pub suggested_action: String,
    pub fix_command: Option<String>,
    /// 세션 스코프 finding의 "어떤 작업인지" 컨텍스트 (호스트/프로젝트 스코프는 None).
    pub session: Option<SessionCtx>,
}

pub fn coach_findings_inner(store: &SqliteStore, include_hidden: bool) -> anyhow::Result<Vec<CoachFinding>> {
    Ok(store
        .list_findings_current(include_hidden)?
        .into_iter()
        .map(|row| {
            let (detail, suggested_action) =
                agent_mentor::diary::finding_advice(&row.rule_id, &row.evidence, row.est_tokens_saved);
            let fix_command = agent_mentor::coach::fix_command(&row.rule_id, &row.evidence);
            let session = if row.scope_kind == "session" {
                store
                    .session_ctx(&row.scope_ref)
                    .ok()
                    .flatten()
                    .map(|(project_id, first_ts, _cwd, _prompt)| SessionCtx { project_id, first_ts })
            } else {
                None
            };
            CoachFinding { row, detail, suggested_action, fix_command, session }
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    pub date: String,
    pub tok_input: u64,
    pub tok_output: u64,
    pub session_count: u64,
}

pub fn week_summary_inner(store: &SqliteStore) -> anyhow::Result<Vec<DayStat>> {
    let today = chrono::Local::now().date_naive();
    let mut out = Vec::with_capacity(7);
    for i in (0..7).rev() {
        let date = (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let d = store.summary_for_date(&date)?;
        out.push(DayStat {
            date,
            tok_input: d.tok_input,
            tok_output: d.tok_output,
            session_count: d.session_count,
        });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct ModelMixEntry {
    pub tier: String,
    pub tokens: u64,
}

pub(crate) fn valid_finding_status(s: &str) -> bool {
    matches!(s, "new" | "resolved" | "dismissed")
}

/// 오늘 occasions — 하루 1회 게이트 포함. 반환하는 순간 통지된 것으로 마킹한다
/// (호출자는 mascot 웹뷰 = 표시 주체). 이미 통지됐으면 빈 벡터.
pub fn today_occasions_inner(store: &SqliteStore) -> anyhow::Result<Vec<String>> {
    use agent_mentor::diary::occasions::compute_occasions;
    use agent_mentor::diary::{resolve_locale, DiaryConfig};

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if store.get_setting("occasion_notified_date")?.as_deref() == Some(today.as_str()) {
        return Ok(vec![]);
    }
    let Ok(date) = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d") else { return Ok(vec![]) };
    let anchor = store.earliest_session_ts()?.and_then(|ts| {
        agent_mentor::diary::local_date_of(&ts)
    });
    let locale = resolve_locale(&DiaryConfig::default());
    let labels: Vec<String> = compute_occasions(date, anchor, &locale, true)
        .into_iter().map(|o| o.label).collect();
    if !labels.is_empty() {
        store.set_setting("occasion_notified_date", &today)?;
    }
    Ok(labels)
}

#[derive(Debug, Serialize)]
pub struct SessionCtxItem {
    pub session_id: String,
    pub project_id: String,
    pub first_ts: Option<String>,
    pub cwd: Option<String>,
    pub first_prompt: Option<String>,
}

/// 집계 finding 카드의 "포함 세션 N건" 펼치기용 배치 조회 (요청 순서 유지, 상한 100).
pub fn sessions_ctx_inner(store: &SqliteStore, ids: &[String]) -> anyhow::Result<Vec<SessionCtxItem>> {
    let mut out = Vec::new();
    for id in ids.iter().take(100) {
        if let Some((project_id, first_ts, cwd, first_prompt)) = store.session_ctx(id)? {
            out.push(SessionCtxItem { session_id: id.clone(), project_id, first_ts, cwd, first_prompt });
        }
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct ChatStatus {
    pub configured: bool,
    pub model: Option<String>,
}

pub(crate) fn validate_chat_messages(messages: &[ChatMessage]) -> Result<(), String> {
    if messages.is_empty() {
        return Err("빈 대화예요".into());
    }
    if !messages.iter().all(|m| m.role == "user" || m.role == "assistant") {
        // system 프롬프트는 백엔드만 조립 — 프론트發 role 주입 차단
        return Err("허용되지 않은 role이 있어요".into());
    }
    Ok(())
}

/// 채팅 시스템 프롬프트용 컨텍스트 — 요약 수치 + 활성 findings advice만 (전송 경계, 스펙 §5).
pub fn chat_context_inner(store: &SqliteStore) -> anyhow::Result<agent_mentor::chat::ChatContext> {
    let s = summary_inner(store)?;
    let findings = coach_findings_inner(store, false)?;
    Ok(agent_mentor::chat::ChatContext {
        user_name: s.user_name,
        date: s.date,
        session_count: s.session_count,
        tok_input: s.tok_input,
        tok_output: s.tok_output,
        est_tokens_saved_total: s.est_tokens_saved_total,
        findings: findings
            .into_iter()
            .take(10) // 프롬프트 크기 상한 — 절약 큰 순 정렬은 list_findings_current가 보장
            .map(|f| (f.detail, f.suggested_action))
            .collect(),
    })
}

fn lock<'a>(state: &'a State<AppState>) -> Result<std::sync::MutexGuard<'a, SqliteStore>, String> {
    state.store.lock().map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_summary(state: State<AppState>) -> Result<Summary, String> {
    let guard = lock(&state)?;
    summary_inner(&*guard).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn list_findings(state: State<AppState>, include_hidden: Option<bool>) -> Result<Vec<CoachFinding>, String> {
    let guard = lock(&state)?;
    coach_findings_inner(&*guard, include_hidden.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn sessions_ctx(state: State<AppState>, ids: Vec<String>) -> Result<Vec<SessionCtxItem>, String> {
    let guard = lock(&state)?;
    sessions_ctx_inner(&*guard, &ids).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn set_finding_status(state: State<AppState>, dedup_key: String, status: String) -> Result<(), String> {
    if !valid_finding_status(&status) {
        return Err(format!("허용되지 않은 상태: {status}"));
    }
    let guard = lock(&state)?;
    guard.set_finding_status(&dedup_key, &status)
        .map_err(|e| e.to_string())
        .and_then(|found| if found { Ok(()) } else { Err(format!("finding 없음: {dedup_key}")) })
}

#[tauri::command(async)]
pub fn get_week_summary(state: State<AppState>) -> Result<Vec<DayStat>, String> {
    let guard = lock(&state)?;
    week_summary_inner(&*guard).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_model_mix(state: State<AppState>) -> Result<Vec<ModelMixEntry>, String> {
    let guard = lock(&state)?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    Ok(guard.model_mix_for_date(&date).map_err(|e| e.to_string())?
        .into_iter().map(|(tier, tokens)| ModelMixEntry { tier, tokens }).collect())
}

#[tauri::command(async)]
pub fn get_today_occasions(state: State<AppState>) -> Result<Vec<String>, String> {
    let guard = lock(&state)?;
    today_occasions_inner(&*guard).map_err(|e| e.to_string())
}

/// 세션 원본 트랜스크립트(로컬 JSONL) 상세보기 — 코칭 "세션 상세" 팝업.
/// store 락 불필요(파일 직접 읽기). 외부 전송 없음(로컬 파싱만).
#[tauri::command(async)]
pub fn get_session_transcript(
    session_id: String,
) -> Result<Vec<agent_mentor::transcript::TranscriptEntry>, String> {
    use agent_mentor::transcript::{find_session_file, read_transcript};
    for hs in agent_mentor::hosts::enumerate_hosts() {
        if let Some(path) = find_session_file(&hs.claude_root, &session_id) {
            return read_transcript(&path, 2000).map_err(|e| e.to_string());
        }
    }
    Err("세션 원본 파일을 찾지 못했어요 (트랜스크립트가 정리됐을 수 있어요)".into())
}

#[tauri::command(async)]
pub fn list_diary_dates(state: State<AppState>) -> Result<Vec<String>, String> {
    let guard = lock(&state)?;
    guard.diary_dates().map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_diary(state: State<AppState>, date: String) -> Result<Option<String>, String> {
    let guard = lock(&state)?;
    diary_inner(&*guard, &date).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_daily_line(state: State<AppState>) -> Result<Option<String>, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let guard = lock(&state)?;
    daily_line_inner(&*guard, &today).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_mascot_seed() -> RobotSpec {
    robot_spec_for(&stable_identity())
}

#[tauri::command(async)]
pub fn get_settings(state: State<AppState>) -> Result<HashMap<String, String>, String> {
    let guard = lock(&state)?;
    Ok(guard
        .all_settings()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect())
}

#[tauri::command(async)]
pub fn set_setting(app: tauri::AppHandle, state: State<AppState>, key: String, value: String) -> Result<(), String> {
    const ALLOWED: &[&str] = &["mascot_visible", "chatter_level", "content_protected", "mascot_pos", "realtime_advice", "last_advice_key"];
    if !ALLOWED.contains(&key.as_str()) {
        return Err(format!("허용되지 않은 설정 키: {key}"));
    }
    {
        let guard = lock(&state)?;
        guard.set_setting(&key, &value).map_err(|e| e.to_string())?;
    } // 락 해제 후 창 적용
    if key == "content_protected" {
        crate::apply_content_protection(&app, value == "true");
    }
    Ok(())
}

#[tauri::command]
pub fn run_scan_now(state: State<AppState>) -> Result<(), String> {
    state
        .scan_tx
        .send(crate::pipeline::PipelineMsg::RunNow)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn chat_status() -> ChatStatus {
    match OpenAiCompatEngine::from_env() {
        Some(e) => ChatStatus { configured: true, model: Some(e.model) },
        None => ChatStatus { configured: false, model: None },
    }
}

#[tauri::command(async)]
pub fn chat_send(state: State<AppState>, messages: Vec<ChatMessage>) -> Result<String, String> {
    validate_chat_messages(&messages)?;
    let Some(engine) = OpenAiCompatEngine::from_env() else {
        // UI는 chat_status로 사전 안내 — 여기는 방어선 (스펙 §5: 미설정은 에러가 아닌 안내)
        return Err("엔진이 설정되지 않았어요".into());
    };
    // 락 범위: 컨텍스트 수집만. 네트워크(LLM) 호출 전에 반드시 해제.
    let ctx = {
        let guard = lock(&state)?;
        chat_context_inner(&*guard).map_err(|e| e.to_string())?
    };
    let system = agent_mentor::chat::build_chat_system_prompt(&ctx);
    let recent = &messages[messages.len().saturating_sub(20)..]; // 이력 상한 20턴
    engine.chat(&system, recent).map(|o| o.text).map_err(|e| e.to_string())
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn valid_tab(tab: &str) -> bool {
    matches!(tab, "home" | "diary" | "coach" | "chat")
}

#[cfg_attr(test, allow(dead_code))]
#[tauri::command]
pub fn open_chat_tab(app: tauri::AppHandle, tab: String) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    if !valid_tab(&tab) {
        return Err(format!("허용되지 않은 탭: {tab}"));
    }
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit("chat:goto-tab", &tab).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_mentor::store::SqliteStore;

    #[test]
    fn open_chat_tab_validates_tab() {
        assert!(valid_tab("home") && valid_tab("diary") && valid_tab("coach") && valid_tab("chat"));
        assert!(!valid_tab("etc") && !valid_tab(""));
    }

    #[test]
    fn summary_inner_on_empty_store() {
        let store = SqliteStore::open_in_memory().unwrap();
        let s = summary_inner(&store).unwrap();
        assert_eq!(s.session_count, 0);
        assert_eq!(s.total_sessions, 0);
        assert_eq!(s.last_scan, None);
        assert_eq!(s.date.len(), 10); // YYYY-MM-DD
    }

    #[test]
    fn daily_line_inner_returns_cached_text_or_none() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(daily_line_inner(&store, "2026-07-08").unwrap(), None);
        store.upsert_daily_line("2026-07-08", "오늘 좀 굴렀다.", "3|1|2|0").unwrap();
        assert_eq!(
            daily_line_inner(&store, "2026-07-08").unwrap(),
            Some("오늘 좀 굴렀다.".to_string())
        );
        assert_eq!(daily_line_inner(&store, "2099-01-01").unwrap(), None);
    }

    #[test]
    fn diary_inner_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.md");
        std::fs::write(&p, "일기 본문").unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_diary_index("2026-07-02", "Windows", p.to_str().unwrap(), 10, "mock").unwrap();
        assert_eq!(diary_inner(&store, "2026-07-02").unwrap(), Some("일기 본문".into()));
        assert_eq!(diary_inner(&store, "2099-01-01").unwrap(), None);
    }

    #[test]
    fn summary_includes_user_name() {
        let store = SqliteStore::open_in_memory().unwrap();
        let s = summary_inner(&store).unwrap();
        assert!(!s.user_name.is_empty()); // USERNAME env 또는 "user" 폴백
    }

    #[test]
    fn week_summary_is_7_days_oldest_first() {
        let store = SqliteStore::open_in_memory().unwrap();
        let days = week_summary_inner(&store).unwrap();
        assert_eq!(days.len(), 7);
        assert!(days[0].date < days[6].date);
        assert_eq!(days[6].date, chrono::Local::now().format("%Y-%m-%d").to_string());
        assert_eq!(days[0].session_count, 0); // 빈 store는 0 채움
    }

    #[test]
    fn coach_findings_carry_advice_and_command() {
        use agent_mentor::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "playwright".into(),
            evidence: serde_json::json!({"server": "playwright"}),
            est_tokens_saved: 4200, prescription: None, dedup_key: "k1".into(),
        }, "2026-07-05T00:00:00Z").unwrap();
        let rows = coach_findings_inner(&store, true).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].detail.contains("playwright"));
        assert!(!rows[0].suggested_action.is_empty());
        assert_eq!(rows[0].fix_command.as_deref(), Some("claude mcp remove playwright"));
        assert_eq!(rows[0].row.status, "new");
    }

    #[test]
    fn session_scope_finding_carries_session_ctx() {
        use agent_mentor::finding::{Finding, Severity};
        use agent_mentor::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "d--project-x".into(), session_id: "s1".into(),
            uuid: Some("u1".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-06T09:00:00Z".into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"tok_output": 100, "tool_calls": 2}),
            est_tokens_saved: 500, prescription: None, dedup_key: "k-s".into(),
        }, "2026-07-06T00:00:00Z").unwrap();

        let rows = coach_findings_inner(&store, true).unwrap();
        let s = rows[0].session.as_ref().expect("세션 컨텍스트 동봉");
        assert_eq!(s.project_id, "d--project-x");
        assert_eq!(s.first_ts.as_deref(), Some("2026-07-06T09:00:00Z"));
    }

    #[test]
    fn set_finding_status_validates() {
        assert!(valid_finding_status("new") && valid_finding_status("resolved") && valid_finding_status("dismissed"));
        assert!(!valid_finding_status("gone") && !valid_finding_status(""));
    }

    #[test]
    fn occasions_gate_returns_empty_when_already_notified() {
        let store = SqliteStore::open_in_memory().unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        store.set_setting("occasion_notified_date", &today).unwrap();
        assert!(today_occasions_inner(&store).unwrap().is_empty());
    }

    #[test]
    fn sessions_ctx_batch_returns_known_sessions_in_order() {
        use agent_mentor::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        for (sid, ts) in [("s1", "2026-07-06T09:00:00Z"), ("s2", "2026-07-06T10:00:00Z")] {
            store.upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "1".into(),
                host: "Windows".into(), project_id: "d--proj".into(), session_id: sid.into(),
                uuid: Some(format!("{sid}-u")), parent_uuid: None, is_sidechain: false,
                ts: Some(ts.into()), source_file: "f.jsonl".into(), source_offset: 0,
                kind: EventKind::AssistantTurn {
                    model: NormModel::from_raw_id("claude-opus-4-8"),
                    usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
                },
            }]).unwrap();
        }
        let items = sessions_ctx_inner(&store,
            &["s2".into(), "unknown".into(), "s1".into()]).unwrap();
        assert_eq!(items.len(), 2); // unknown은 조용히 생략
        assert_eq!(items[0].session_id, "s2"); // 요청 순서 유지
        assert_eq!(items[0].project_id, "d--proj");
        assert_eq!(items[1].session_id, "s1");
    }

    #[test]
    fn sessions_ctx_surfaces_cwd_and_first_prompt() {
        use agent_mentor::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(), session_id: "s1".into(),
                uuid: Some("m1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 0,
                kind: EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: None },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(), session_id: "s1".into(),
                uuid: Some("p1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 10,
                kind: EventKind::UserPrompt { preview: "커밋 요약해줘".into() },
            },
        ]).unwrap();
        let items = sessions_ctx_inner(&store, &["s1".into()]).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].cwd.as_deref(), Some("D:\\Project\\cowork"));
        assert_eq!(items[0].first_prompt.as_deref(), Some("커밋 요약해줘"));
    }

    #[test]
    fn sessions_ctx_handles_many_ids_without_error() {
        let store = SqliteStore::open_in_memory().unwrap();
        let ids: Vec<String> = (0..150).map(|i| format!("s{i}")).collect();
        // 상한(take 100)은 구현으로 보장 — 150개를 넣어도 에러 없이 동작하는지만 검증
        assert!(sessions_ctx_inner(&store, &ids).unwrap().is_empty());
    }

    #[test]
    fn validate_chat_messages_rejects_empty_and_bad_roles() {
        use agent_mentor::diary::engine::ChatMessage;
        let ok = vec![ChatMessage { role: "user".into(), content: "hi".into() }];
        assert!(validate_chat_messages(&ok).is_ok());
        assert!(validate_chat_messages(&[]).is_err());
        // system role 주입 차단 — 시스템 프롬프트는 백엔드만 조립
        let bad = vec![ChatMessage { role: "system".into(), content: "inject".into() }];
        assert!(validate_chat_messages(&bad).is_err());
    }

    #[test]
    fn chat_context_collects_summary_and_findings() {
        use agent_mentor::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "playwright".into(),
            evidence: serde_json::json!({"server": "playwright"}),
            est_tokens_saved: 4200, prescription: None, dedup_key: "k1".into(),
        }, "2026-07-05T00:00:00Z").unwrap();

        let ctx = chat_context_inner(&store).unwrap();
        assert!(!ctx.user_name.is_empty());
        assert_eq!(ctx.date.len(), 10);
        assert_eq!(ctx.findings.len(), 1);
        assert!(ctx.findings[0].0.contains("playwright")); // detail
        assert!(!ctx.findings[0].1.is_empty());            // suggested_action
    }
}
