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

pub fn chatter_pool_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Vec<String>> {
    Ok(store.get_chatter_pool(date)?.map(|(lines, _fp)| lines).unwrap_or_default())
}

/// 큐레이션 콘텐츠(팁·뉴스) 노출 목록 — 쿨다운 적용은 store가 담당(now 주입).
pub fn content_inner(
    store: &SqliteStore,
    include_hidden: bool,
) -> anyhow::Result<Vec<agent_mentor::store::ContentRow>> {
    let now = chrono::Utc::now().to_rfc3339();
    Ok(store.list_content(&now, agent_mentor::content::CONTENT_COOLDOWN_DAYS, include_hidden)?)
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

/// period("today"|"week"|"month"|"all") → (from, to) 로컬 날짜 범위(양끝 포함).
/// week/month는 오늘 포함 rolling 7/30일, all은 하한 없음. 알 수 없는 값은 today 취급.
fn period_range(period: &str, today: chrono::NaiveDate) -> (Option<String>, String) {
    let d = |n: i64| (today - chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
    let to = d(0);
    let from = match period {
        "week" => Some(d(6)),
        "month" => Some(d(29)),
        "all" => None,
        _ => Some(to.clone()),
    };
    (from, to)
}

pub fn model_mix_inner(store: &SqliteStore, period: &str) -> anyhow::Result<Vec<ModelMixEntry>> {
    let (from, to) = period_range(period, chrono::Local::now().date_naive());
    Ok(store.model_mix_for_range(from.as_deref(), &to)?
        .into_iter().map(|(tier, tokens)| ModelMixEntry { tier, tokens }).collect())
}

pub(crate) fn valid_finding_status(s: &str) -> bool {
    matches!(s, "new" | "resolved" | "dismissed")
}

pub(crate) fn valid_content_status(s: &str) -> bool {
    matches!(s, "new" | "shown" | "dismissed")
}

/// occasionBubble("오늘 …이래요! 🎉")은 축하 톤이라 추모일(mood=solemn, 예: 현충일)엔 부적절 →
/// 통지 라벨에서 제외한다. (다이어리는 mood로 톤을 맞추므로 별개 경로.)
fn notifiable_labels(occasions: Vec<agent_mentor::diary::occasions::Occasion>) -> Vec<String> {
    occasions
        .into_iter()
        .filter(|o| o.mood.as_deref() != Some("solemn"))
        .map(|o| o.label)
        .collect()
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
    let labels = notifiable_labels(compute_occasions(date, anchor, &locale, true));
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
    let memories = store.list_memories()?.into_iter().map(|m| m.text).collect();
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
        memories,
    })
}

/// Tier 2 질적 코칭 브리프 — core 공용 조립기에 위임 (CLI와 동일 경로).
pub fn coaching_brief_inner(store: &SqliteStore) -> anyhow::Result<agent_mentor::chat::CoachingBrief> {
    agent_mentor::chat::assemble_coaching_brief(store)
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

// --- AX 역량 사다리 (튜터의 성장 지도) ---
// core의 detect_profile(결정론)을 UI 친화 형태로 노출 — 학습자가 자기 위치+다음 단계를 본다.

#[derive(Debug, Serialize)]
pub struct ProfileRung {
    pub key: String,
    pub label: String,
    pub ladder_index: u8,
    pub mastery: String,
    pub evidence: String,
    pub learn_hint: String,
    pub is_frontier: bool,
}

#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub rungs: Vec<ProfileRung>,
    pub frontier_key: Option<String>,
    pub total_events: u64,
}

pub fn profile_inner(store: &SqliteStore) -> anyhow::Result<ProfileView> {
    use agent_mentor::profile::{detect_profile, Dimension};
    let p = detect_profile(store)?;
    let frontier = p.frontier();
    let rungs = Dimension::all()
        .into_iter()
        .map(|d| {
            let st = p.dims.iter().find(|s| s.dimension == d);
            ProfileRung {
                key: d.key().to_string(),
                label: d.label_ko().to_string(),
                ladder_index: d.ladder_index(),
                mastery: st.map(|s| s.mastery.key()).unwrap_or("not_started").to_string(),
                evidence: st.map(|s| s.evidence.clone()).unwrap_or_default(),
                learn_hint: d.learn_hint_ko().to_string(),
                is_frontier: frontier == Some(d),
            }
        })
        .collect();
    Ok(ProfileView {
        rungs,
        frontier_key: frontier.map(|d| d.key().to_string()),
        total_events: p.total_events,
    })
}

#[tauri::command(async)]
pub fn get_profile(state: State<AppState>) -> Result<ProfileView, String> {
    let guard = lock(&state)?;
    profile_inner(&*guard).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn list_content(
    state: State<AppState>,
    include_hidden: Option<bool>,
) -> Result<Vec<agent_mentor::store::ContentRow>, String> {
    let guard = lock(&state)?;
    content_inner(&*guard, include_hidden.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn set_content_status(state: State<AppState>, id: String, status: String) -> Result<(), String> {
    if !valid_content_status(&status) {
        return Err(format!("허용되지 않은 상태: {status}"));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let guard = lock(&state)?;
    guard.set_content_status(&id, &status, &now)
        .map_err(|e| e.to_string())
        .and_then(|found| if found { Ok(()) } else { Err(format!("콘텐츠 없음: {id}")) })
}

#[tauri::command(async)]
pub fn get_model_mix(state: State<AppState>, period: Option<String>) -> Result<Vec<ModelMixEntry>, String> {
    let guard = lock(&state)?;
    model_mix_inner(&*guard, period.as_deref().unwrap_or("today")).map_err(|e| e.to_string())
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

#[tauri::command(async)]
pub fn get_chatter_pool(state: State<AppState>) -> Result<Vec<String>, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let guard = lock(&state)?;
    chatter_pool_inner(&*guard, &today).map_err(|e| e.to_string())
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

#[tauri::command(async)]
pub fn chat_status(state: State<AppState>) -> Result<ChatStatus, String> {
    let guard = lock(&state)?;
    Ok(match crate::resolve_engine(&guard) {
        Some(e) => ChatStatus { configured: true, model: Some(e.model) },
        None => ChatStatus { configured: false, model: None },
    })
}

#[tauri::command(async)]
pub fn chat_send(state: State<AppState>, messages: Vec<ChatMessage>) -> Result<String, String> {
    use agent_mentor::chat::{classify_intent, run_memory_chat, ChatIntent};
    validate_chat_messages(&messages)?;
    // 마지막 사용자 메시지로 의도 분류 → 티어 라우팅 (결정론)
    let last_user = messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.as_str()).unwrap_or("");
    let intent = classify_intent(last_user);

    // 락 범위: 엔진 해석 + 컨텍스트(메모리 포함) 수집만. 네트워크(LLM) 호출 전에 반드시 해제.
    // 코칭(Tier 2)이면 주간 코칭 브리프를, 아니면 오늘 요약 컨텍스트를 조립.
    let (engine, system) = {
        let guard = lock(&state)?;
        let engine = crate::resolve_engine(&guard);
        let system = match intent {
            ChatIntent::Coaching => {
                let brief = coaching_brief_inner(&*guard).map_err(|e| e.to_string())?;
                agent_mentor::chat::build_coaching_system_prompt(&brief)
            }
            _ => {
                let ctx = chat_context_inner(&*guard).map_err(|e| e.to_string())?;
                agent_mentor::chat::build_chat_system_prompt(&ctx)
            }
        };
        (engine, system)
    };
    let Some(engine) = engine else {
        // UI는 chat_status로 사전 안내 — 여기는 방어선 (스펙 §5: 미설정은 에러가 아닌 안내)
        return Err("엔진이 설정되지 않았어요".into());
    };

    // 이력 상한 20턴 + 루프 중 tool 메시지 누적
    let mut convo: Vec<ChatMessage> = messages[messages.len().saturating_sub(20)..].to_vec();

    // save_memory 툴콜 시 락을 새로 잡아 저장한다. 네트워크(chat_with_tools) 호출은
    // run_memory_chat 내부(락 밖)에서 일어나고, on_save는 호출과 호출 사이에서만 실행되므로
    // 네트워크 중 락 보유가 아니다(규율 유지).
    run_memory_chat(&engine, &system, &mut convo, 3, |text| {
        if let Ok(guard) = lock(&state) {
            if let Err(e) = guard.add_memory(text, "chat") {
                log::warn!("add_memory(chat) 실패: {e}");
            }
        }
    })
    .map_err(|e| e.to_string())
}

/// 설정 창용 엔진 설정 스냅샷. source: "store"(설정 창에서 지정) | "env"(.env 폴백) | "none".
#[derive(Debug, Clone, Serialize)]
pub struct EngineSettings {
    pub url: String,
    pub key: String,
    pub model: String,
    pub source: String,
}

#[tauri::command(async)]
pub fn engine_settings_get(state: State<AppState>) -> Result<EngineSettings, String> {
    let guard = lock(&state)?;
    let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
    let stored_url = get("engine_url").trim().to_string();
    if !stored_url.is_empty() {
        return Ok(EngineSettings {
            url: stored_url,
            key: get("engine_key"),
            model: get("engine_model"),
            source: "store".into(),
        });
    }
    Ok(match OpenAiCompatEngine::from_env() {
        Some(e) => EngineSettings { url: e.base_url, key: e.api_key, model: e.model, source: "env".into() },
        None => EngineSettings { url: String::new(), key: String::new(), model: String::new(), source: "none".into() },
    })
}

#[tauri::command(async)]
pub fn engine_settings_set(
    state: State<AppState>,
    url: String,
    key: String,
    model: String,
) -> Result<(), String> {
    let url = url.trim();
    if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("엔드포인트 URL은 http:// 또는 https:// 로 시작해야 해요".into());
    }
    let guard = lock(&state)?;
    // url을 비우고 저장하면 store 값이 지워져 .env 폴백으로 돌아간다 (engine_settings_get 참고)
    guard.set_setting("engine_url", url).map_err(|e| e.to_string())?;
    guard.set_setting("engine_key", key.trim()).map_err(|e| e.to_string())?;
    guard.set_setting("engine_model", model.trim()).map_err(|e| e.to_string())?;
    Ok(())
}

/// 미니홈피 테마 설정 스냅샷. mode: light|dark|system, skin: sky|mint|peach|lavender.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeSettings {
    pub mode: String,
    pub skin: String,
}

const THEME_MODES: &[&str] = &["light", "dark", "system"];
const THEME_SKINS: &[&str] = &["sky", "mint", "peach", "lavender"];

/// 허용 집합 검증 (순수 — 테스트 대상)
pub fn validate_theme(mode: &str, skin: &str) -> Result<(), String> {
    if !THEME_MODES.contains(&mode) {
        return Err(format!("허용되지 않은 테마 모드: {mode}"));
    }
    if !THEME_SKINS.contains(&skin) {
        return Err(format!("허용되지 않은 색상 세트: {skin}"));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn theme_get(state: State<AppState>) -> Result<ThemeSettings, String> {
    let guard = lock(&state)?;
    let get = |k: &str, d: &str| {
        let v = guard.get_setting(k).ok().flatten().unwrap_or_default();
        if v.trim().is_empty() { d.to_string() } else { v }
    };
    let mode = get("theme_mode", "system");
    let skin = get("theme_skin", "sky");
    // 저장값이 깨졌으면 기본값으로 폴백
    if validate_theme(&mode, &skin).is_ok() {
        Ok(ThemeSettings { mode, skin })
    } else {
        Ok(ThemeSettings { mode: "system".into(), skin: "sky".into() })
    }
}

#[tauri::command(async)]
pub fn theme_set(
    app: tauri::AppHandle,
    state: State<AppState>,
    mode: String,
    skin: String,
) -> Result<(), String> {
    validate_theme(&mode, &skin)?;
    {
        let guard = lock(&state)?;
        guard.set_setting("theme_mode", &mode).map_err(|e| e.to_string())?;
        guard.set_setting("theme_skin", &skin).map_err(|e| e.to_string())?;
    } // 락 해제 후 브로드캐스트
    use tauri::Emitter;
    let _ = app.emit("theme:changed", ThemeSettings { mode, skin });
    Ok(())
}

/// 저장 전 값으로도 시험할 수 있게 폼 값을 그대로 받는다. 성공 시 모델의 응답 일부를 돌려준다.
#[tauri::command(async)]
pub fn engine_test(url: String, key: String, model: String) -> Result<String, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("엔드포인트 URL을 입력하세요".into());
    }
    let model = model.trim();
    let engine = OpenAiCompatEngine {
        base_url: url,
        api_key: key.trim().to_string(),
        model: if model.is_empty() { "gpt-4o-mini".to_string() } else { model.to_string() },
    };
    let out = engine
        .generate("연결 테스트입니다. 'ok' 한 단어로만 답하세요.", "ping")
        .map_err(|e| e.to_string())?;
    let snippet: String = out.text.chars().take(40).collect();
    Ok(format!("연결 성공 — 응답: {snippet}"))
}

/// (2) LLM 코칭 — 팁 + 사용자 실측 근거(personal)를 엔진에 넘겨 이 사람 맞춤 한 줄 코칭 생성.
/// 엔진 미설정이면 에러(프론트가 결정론적 근거 줄만 유지). 네트워크 호출이라 async.
#[tauri::command(async)]
pub fn coach_tip(title: String, body: String, personal: Option<String>) -> Result<String, String> {
    let Some(engine) = OpenAiCompatEngine::from_env() else {
        return Err("engine-not-configured".into());
    };
    let (system, user) = agent_mentor::content::coach_prompt(&title, &body, personal.as_deref());
    engine.generate(&system, &user).map(|o| o.text).map_err(|e| e.to_string())
}

// --- 방 방문 (docs/design/life-visit.md) ---
// 설정 키: hub_url·hub_token·hub_agent_id·hub_life_id. 이름의 단일 원본은 user_name이다.
// 규율: 락은 설정 읽기/쓰기 동안만, 네트워크(hub HTTP)는 락 밖.

use agent_mentor::life_client::{self, LifeClient};

#[derive(Debug, Clone, Serialize)]
pub struct HubSettings {
    pub url: String,
    pub user: String,
    pub api_key: String,
    pub connected: bool,
    pub life_id: String,
}

/// 빈 문자열이면 None — 관문 없는 서버는 x-api-key를 안 붙인다 (하위호환).
fn opt_key(key: String) -> Option<String> {
    let k = key.trim();
    if k.is_empty() { None } else { Some(k.to_string()) }
}

fn token_was_rejected(error: &str) -> bool {
    error.contains("HTTP 401") || error.contains("HTTP 403")
}

fn hub_client(state: &State<AppState>) -> Result<Option<LifeClient>, String> {
    let guard = lock(state)?;
    let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
    let url = get("hub_url");
    let token = get("hub_token");
    if url.trim().is_empty() || token.is_empty() {
        return Ok(None);
    }
    Ok(Some(LifeClient { base_url: url, token, api_key: opt_key(get("hub_api_key")) }))
}

fn upload_cached_mascot(app: &tauri::AppHandle, client: &LifeClient) -> Result<bool, String> {
    use tauri::Manager as _;
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("sprite.png");
    let Ok(png) = std::fs::read(path) else { return Ok(false) };
    client.upload_mascot_image(&png).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn hub_settings_get(state: State<AppState>) -> Result<HubSettings, String> {
    let guard = lock(&state)?;
    let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
    Ok(HubSettings {
        url: get("hub_url"),
        user: get("user_name"),
        api_key: get("hub_api_key"),
        connected: !get("hub_token").is_empty(),
        life_id: get("hub_life_id"),
    })
}

/// 서버에 유저 등록 + 개인 방 생성. 성공 시 토큰·방 id를 설정에 저장.
/// 이미 같은 서버에 연결돼 있으면 재등록 대신 **이름 변경**으로 처리한다 — 방·위치 유지.
#[tauri::command(async)]
pub fn hub_connect(
    app: tauri::AppHandle,
    state: State<AppState>,
    url: String,
    api_key: String,
) -> Result<HubSettings, String> {
    use tauri::Emitter;
    let url = url.trim().to_string();
    let api_key = api_key.trim().to_string();
    if url.is_empty() {
        return Err("서버 URL을 입력하세요".into());
    }
    let user = {
        let guard = lock(&state)?;
        guard.get_setting("user_name").ok().flatten().unwrap_or_default().trim().to_string()
    };
    if user.is_empty() {
        return Err("미니홈피 설정의 개인정보에서 이름을 먼저 입력하세요".into());
    }
    let key_opt = opt_key(api_key.clone());
    // 기존 연결 확인 (락은 읽기 동안만)
    let existing = {
        let guard = lock(&state)?;
        let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
        (get("hub_url"), get("hub_token"))
    };
    if !existing.1.is_empty() {
        let client = LifeClient { base_url: url.clone(), token: existing.1, api_key: key_opt.clone() };
        match client.rename(&user) {
            Ok(_) => {
                let guard = lock(&state)?;
                guard.set_setting("hub_url", &url).map_err(|e| e.to_string())?;
                guard.set_setting("hub_user", &user).map_err(|e| e.to_string())?;
                guard.set_setting("hub_api_key", &api_key).map_err(|e| e.to_string())?;
                drop(guard);
                let _ = upload_cached_mascot(&app, &client);
                let _ = app.emit("settings:changed", ());
                return hub_settings_get(state);
            }
            Err(error) => {
                let message = error.to_string();
                if !token_was_rejected(&message) {
                    return Err(format!("기존 Life 연결 확인 실패: {message}"));
                }
            }
        }
        // 대상 서버가 기존 토큰을 명시적으로 거부한 경우에만 새로 등록한다.
    }
    // 프로필: 시드=uuid, 조직 함께 전송 (락은 읽기 동안만)
    let (uuid, org) = {
        let guard = lock(&state)?;
        let u = ensure_uuid(&guard)?;
        let o = guard.get_setting("user_org").ok().flatten().unwrap_or_default();
        let o = if o.trim().is_empty() { DEFAULT_ORG.to_string() } else { o };
        (u, o)
    };
    // 네트워크는 락 밖
    let v = life_client::register_profile(&url, key_opt.as_deref(), &user, &uuid, &org, &uuid)
        .map_err(|e| e.to_string())?;
    let token = v["token"].as_str().unwrap_or_default().to_string();
    let agent_id = v["agent_id"].as_str().unwrap_or_default().to_string();
    let life_id = v["life_id"].as_str().unwrap_or_default().to_string();
    if token.is_empty() || life_id.is_empty() {
        return Err("서버 응답에 token/life_id가 없어요".into());
    }
    {
        let guard = lock(&state)?;
        for (k, val) in [
            ("hub_url", url.as_str()),
            ("hub_user", user.as_str()),
            ("user_name", user.as_str()), // 개인정보 이름과 동기화
            ("user_org", org.as_str()),
            ("hub_api_key", api_key.as_str()),
            ("hub_token", token.as_str()),
            ("hub_agent_id", agent_id.as_str()),
            ("hub_life_id", life_id.as_str()),
        ] {
            guard.set_setting(k, val).map_err(|e| e.to_string())?;
        }
    }
    let client = LifeClient { base_url: url.clone(), token: token.clone(), api_key: key_opt };
    let _ = upload_cached_mascot(&app, &client);
    let _ = app.emit("settings:changed", ());
    hub_settings_get(state)
}

/// 홈 탭이 폴링하는 단일 진입점: 내 위치 + 그 방의 상태.
async fn run_life_http<T, F>(operation: &'static str, request: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(request)
        .await
        .map_err(|error| format!("{operation}_task_failed: {error}"))?;
    let elapsed = started.elapsed();
    if elapsed >= std::time::Duration::from_secs(2) {
        log::warn!("slow Life request: {operation} took {elapsed:?}");
    }
    result
}

#[tauri::command]
pub async fn life_view(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else {
        return Err("hub_not_connected".into());
    };
    run_life_http("life_view", move || {
        let me = client.me().map_err(|e| e.to_string())?;
        let life_id = me["life_id"].as_str().unwrap_or_default().to_string();
        let life = client.life_state(&life_id).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "me": me, "life": life }))
    })
    .await
}

#[tauri::command]
pub async fn life_list(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (url, api_key) = {
        let guard = lock(&state)?;
        let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
        (get("hub_url"), get("hub_api_key"))
    };
    if url.trim().is_empty() {
        return Err("hub_not_connected".into());
    }
    run_life_http("life_list", move || {
        life_client::list_life(&url, opt_key(api_key).as_deref()).map_err(|e| e.to_string())
    })
    .await
}

/// 방 이동(우클릭 메뉴). cell 없이 입장 — 서버가 빈 셀 배정.
#[tauri::command]
pub async fn life_goto(state: State<'_, AppState>, life_id: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else {
        return Err("hub_not_connected".into());
    };
    run_life_http("life_goto", move || client.enter(&life_id, None).map_err(|e| e.to_string())).await
}

/// 방 안 셀 이동(좌클릭). 점유 셀이면 서버가 409 → cell_taken 에러 문자열.
#[tauri::command]
pub async fn life_move_cell(state: State<'_, AppState>, x: i64, y: i64) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else {
        return Err("hub_not_connected".into());
    };
    run_life_http("life_move_cell", move || {
        let me = client.me().map_err(|e| e.to_string())?;
        let life_id = me["life_id"].as_str().unwrap_or_default().to_string();
        client.move_to(&life_id, (x, y)).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn life_capabilities(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else {
        return Err("hub_not_connected".into());
    };
    run_life_http("life_capabilities", move || client.capabilities().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_save_design(
    state: State<'_, AppState>,
    life_id: String,
    design: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else {
        return Err("hub_not_connected".into());
    };
    run_life_http("life_save_design", move || {
        client.save_design(&life_id, design).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn hub_disconnect(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<HubSettings, String> {
    use tauri::Emitter;
    if let Some(client) = hub_client(&state)? {
        run_life_http("hub_disconnect", move || client.disconnect().map(|_| ()).map_err(|e| e.to_string())).await?;
    }
    let guard = lock(&state)?;
    for key in ["hub_token", "hub_agent_id", "hub_life_id"] {
        guard.set_setting(key, "").map_err(|e| e.to_string())?;
    }
    drop(guard);
    let _ = app.emit("settings:changed", ());
    hub_settings_get(state)
}

#[tauri::command]
pub async fn life_people(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_people", move || client.people().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_set_friend(state: State<'_, AppState>, agent_id: String, enabled: bool) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_set_friend", move || client.set_friend(&agent_id, enabled).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_set_content_visibility(state: State<'_, AppState>, feature: String, visibility: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_set_content_visibility", move || client.set_content_visibility(&feature, &visibility).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_content_access(state: State<'_, AppState>, life_id: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_content_access", move || client.content_access(&life_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_set_diary_visibility(state: State<'_, AppState>, date: String, body: String, visibility: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_set_diary_visibility", move || {
        if visibility == "private" { client.unshare_diary(&date) }
        else { client.share_diary(&date, &body, &visibility) }.map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
pub async fn life_diaries(state: State<'_, AppState>, life_id: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_diaries", move || client.diaries(&life_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_guestbook(state: State<'_, AppState>, life_id: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_guestbook", move || client.guestbook(&life_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_add_guestbook(state: State<'_, AppState>, life_id: String, body: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_add_guestbook", move || client.add_guestbook(&life_id, &body).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_delete_guestbook(state: State<'_, AppState>, entry_id: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_delete_guestbook", move || client.delete_guestbook(&entry_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_set_bubble(state: State<'_, AppState>, body: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    run_life_http("life_set_bubble", move || client.set_bubble(&body).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn life_sync_mascot_image(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let Some(client) = hub_client(&state)? else { return Ok(false) };
    run_life_http("life_sync_mascot_image", move || upload_cached_mascot(&app, &client)).await
}

#[tauri::command]
pub async fn life_mascot_image(state: State<'_, AppState>, agent_id: String) -> Result<Option<String>, String> {
    use base64::Engine as _;
    let Some(client) = hub_client(&state)? else { return Ok(None) };
    run_life_http("life_mascot_image", move || {
        client.mascot_image(&agent_id)
            .map(|value| value.map(|png| base64::engine::general_purpose::STANDARD.encode(png)))
            .map_err(|e| e.to_string())
    }).await
}

/// 임의 시드의 로봇 스펙 — 방 안 다른 에이전트 렌더용.
#[tauri::command]
pub fn robot_spec_for_seed(seed: String) -> RobotSpec {
    robot_spec_for(&seed)
}

/// 마스코트 말풍선/메뉴 열림 상태 알림. 창은 상시 확장 크기로 고정이라(리사이즈
/// 깜빡임 원천 차단) 크기 변경은 없고, 접힘 상태에서 로봇 밖 투명 여백의 클릭
/// 통과 여부를 lib.rs의 폴러가 이 플래그로 결정한다.
#[tauri::command]
pub fn mascot_set_expanded(state: State<AppState>, expanded: bool) {
    state
        .mascot_expanded
        .store(expanded, std::sync::atomic::Ordering::Relaxed);
}

/// 설정 창 열기 (마스코트 메뉴에서 "서버 연결" 안내용).
#[tauri::command]
pub fn open_settings_window(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn valid_tab(tab: &str) -> bool {
    matches!(tab, "home" | "diary" | "coach" | "chat")
}

/// chat:goto-tab payload — target은 탭 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD).
/// 백엔드는 내용을 해석하지 않는다(스펙 §1-4).
#[derive(Debug, Clone, Serialize)]
pub struct GotoTabPayload {
    pub tab: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>, // None이면 필드 생략 — 프론트 `target?: string`과 정합
}

#[cfg_attr(test, allow(dead_code))]
#[tauri::command]
pub fn open_chat_tab(app: tauri::AppHandle, tab: String, target: Option<String>) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    if !valid_tab(&tab) {
        return Err(format!("허용되지 않은 탭: {tab}"));
    }
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit("chat:goto-tab", GotoTabPayload { tab, target })
        .map_err(|e| e.to_string())
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
    fn validate_theme_accepts_known_and_rejects_unknown() {
        assert!(validate_theme("system", "sky").is_ok());
        assert!(validate_theme("dark", "peach").is_ok());
        assert!(validate_theme("light", "lavender").is_ok());
        assert!(validate_theme("neon", "sky").is_err());
        assert!(validate_theme("dark", "rainbow").is_err());
    }

    #[test]
    fn token_rejection_is_distinct_from_transient_failure() {
        assert!(token_was_rejected("unauthorized (HTTP 401)"));
        assert!(token_was_rejected("forbidden (HTTP 403)"));
        assert!(!token_was_rejected("connection timed out"));
        assert!(!token_was_rejected("server error (HTTP 500)"));
    }

    #[test]
    fn content_status_validation_and_empty_list() {
        assert!(valid_content_status("new") && valid_content_status("shown") && valid_content_status("dismissed"));
        assert!(!valid_content_status("resolved") && !valid_content_status(""));
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(content_inner(&store, false).unwrap().is_empty());
    }

    #[test]
    fn goto_tab_payload_serializes_target_optionally() {
        let with = GotoTabPayload { tab: "diary".into(), target: Some("2026-07-11".into()) };
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            r#"{"tab":"diary","target":"2026-07-11"}"#
        );
        // None이면 target 필드 자체를 생략 — 프론트 `target?: string`(undefined)과 정합
        let without = GotoTabPayload { tab: "home".into(), target: None };
        assert_eq!(serde_json::to_string(&without).unwrap(), r#"{"tab":"home"}"#);
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
    fn chatter_pool_inner_returns_lines_or_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 캐시 없음 → 빈 벡터 (에러 아님)
        assert_eq!(chatter_pool_inner(&store, "2026-07-10").unwrap(), Vec::<String>::new());
        store
            .upsert_chatter_pool("2026-07-10", &["잡담 하나".to_string(), "잡담 둘".to_string()], "3|1|2|0")
            .unwrap();
        assert_eq!(
            chatter_pool_inner(&store, "2026-07-10").unwrap(),
            vec!["잡담 하나".to_string(), "잡담 둘".to_string()]
        );
        // 다른 날짜 → 빈 벡터
        assert_eq!(chatter_pool_inner(&store, "2099-01-01").unwrap(), Vec::<String>::new());
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
    fn coaching_brief_inner_assembles_without_error() {
        let store = SqliteStore::open_in_memory().unwrap();
        let brief = coaching_brief_inner(&store).unwrap();
        // 빈 store에서도 안전하게 조립 (수치 0, 프로필 5축, 델타 없음)
        assert_eq!(brief.week_sessions, 0);
        assert_eq!(brief.profile.len(), 5);
        assert!(brief.week_session_delta_pct.is_none()); // 지난주 0 → 델타 없음
        // 코칭 프롬프트로도 문제없이 렌더
        let p = agent_mentor::chat::build_coaching_system_prompt(&brief);
        assert!(p.contains("질적 코칭 모드"));
    }

    #[test]
    fn profile_inner_exposes_five_rungs_and_one_frontier() {
        let store = SqliteStore::open_in_memory().unwrap();
        let view = profile_inner(&store).unwrap();
        assert_eq!(view.rungs.len(), 5);
        // 사다리 순서 보존 (Lv0..Lv4)
        assert_eq!(view.rungs[0].key, "model_literacy");
        assert_eq!(view.rungs[0].ladder_index, 0);
        assert_eq!(view.rungs[4].key, "orchestration");
        // 프론티어는 최대 1개이며 frontier_key와 일치
        let fronts: Vec<&str> = view.rungs.iter().filter(|r| r.is_frontier).map(|r| r.key.as_str()).collect();
        assert!(fronts.len() <= 1);
        assert_eq!(view.frontier_key.as_deref(), fronts.first().copied());
        // 라벨·학습 힌트가 비어있지 않음 (UI 표시용)
        assert!(view.rungs.iter().all(|r| !r.label.is_empty() && !r.learn_hint.is_empty()));
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
    fn period_range_maps_periods() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        assert_eq!(period_range("today", today), (Some("2026-07-12".into()), "2026-07-12".into()));
        assert_eq!(period_range("week", today), (Some("2026-07-06".into()), "2026-07-12".into()));
        assert_eq!(period_range("month", today), (Some("2026-06-13".into()), "2026-07-12".into()));
        assert_eq!(period_range("all", today), (None, "2026-07-12".into()));
        // 알 수 없는 값은 today 취급
        assert_eq!(period_range("yolo", today), period_range("today", today));
    }

    #[test]
    fn model_mix_inner_today_vs_all() {
        use agent_mentor::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some("u1".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2020-01-01T10:00:00Z".into()), // 확실한 과거 — today엔 안 걸린다
            source_file: "f.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: 5, output: 5, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        let all = model_mix_inner(&store, "all").unwrap();
        assert_eq!(all[0].tier, "claude-opus-4-8");
        assert_eq!(all[0].tokens, 10);
        assert!(model_mix_inner(&store, "today").unwrap().is_empty());
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
            msg_id: None,
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
    fn notifiable_labels_drops_solemn_occasions() {
        use agent_mentor::diary::occasions::Occasion;
        // 현충일(solemn)은 축하 톤 버블에 부적절 → 제외. 나머지는 유지.
        let occ = vec![
            Occasion { category: "holiday".into(), label: "현충일".into(), mood: Some("solemn".into()) },
            Occasion { category: "holiday".into(), label: "제헌절".into(), mood: Some("national".into()) },
            Occasion { category: "milestone".into(), label: "함께한 지 100일".into(), mood: None },
        ];
        let labels = notifiable_labels(occ);
        assert!(!labels.contains(&"현충일".to_string()), "추모일 제외");
        assert!(labels.contains(&"제헌절".to_string()));
        assert!(labels.contains(&"함께한 지 100일".to_string()));
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
                msg_id: None,
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
                msg_id: None,
                kind: EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: None },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(), session_id: "s1".into(),
                uuid: Some("p1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 10,
                msg_id: None,
                kind: EventKind::UserPrompt { preview: "커밋 요약해줘".into(), is_command: false },
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
        let ok = vec![ChatMessage { role: "user".into(), content: "hi".into(), ..Default::default() }];
        assert!(validate_chat_messages(&ok).is_ok());
        assert!(validate_chat_messages(&[]).is_err());
        // system role 주입 차단 — 시스템 프롬프트는 백엔드만 조립
        let bad = vec![ChatMessage { role: "system".into(), content: "inject".into(), ..Default::default() }];
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

    #[test]
    fn memory_inner_add_list_delete() {
        let store = SqliteStore::open_in_memory().unwrap();
        let id = store.add_memory("주인은 비건임", "manual").unwrap();
        let all = store.list_memories().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].text, "주인은 비건임");
        store.delete_memory(id).unwrap();
        assert_eq!(store.count_memories().unwrap(), 0);
    }
}

/// AI 스프라이트(캐시) — app_data/sprite.png를 base64로. 없으면 None(프론트는 절차 생성 폴백).
#[tauri::command]
pub fn get_sprite(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use base64::Engine as _;
    use tauri::Manager as _;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let p = dir.join("sprite.png");
    match std::fs::read(&p) {
        Ok(bytes) => Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes))),
        Err(_) => Ok(None),
    }
}

/// 캐릭터 이미지 모델 설정 스냅샷 (설정창용). source: "store"(설정창) | "env"(.env) | "none".
#[derive(Debug, Clone, Serialize)]
pub struct ImageSettings {
    pub url: String,
    pub key: String,
    pub model: String,
    pub source: String,
}

#[tauri::command(async)]
pub fn image_settings_get(state: State<AppState>) -> Result<ImageSettings, String> {
    let guard = lock(&state)?;
    let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
    let stored_url = get("image_url").trim().to_string();
    if !stored_url.is_empty() {
        return Ok(ImageSettings {
            url: stored_url,
            key: get("image_key"),
            model: get("image_model"),
            source: "store".into(),
        });
    }
    // 저장값이 없으면 env 폴백이 뭘로 잡히는지 그대로 보여준다(설정 안내용).
    Ok(match crate::resolve_sprite_cfg(&guard) {
        Some(c) => ImageSettings { url: c.base_url, key: c.api_key, model: c.model, source: "env".into() },
        None => ImageSettings {
            url: String::new(),
            key: String::new(),
            model: agent_mentor::sprite::DEFAULT_IMAGE_MODEL.into(),
            source: "none".into(),
        },
    })
}

#[tauri::command(async)]
pub fn image_settings_set(
    state: State<AppState>,
    url: String,
    key: String,
    model: String,
) -> Result<(), String> {
    let guard = lock(&state)?;
    for (k, v) in [
        ("image_url", url.trim()),
        ("image_key", key.trim()),
        ("image_model", model.trim()),
    ] {
        guard.set_setting(k, v).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 조직 기본값 — 개인정보 미입력 시.
pub(crate) const DEFAULT_ORG: &str = "S/W 혁신팀";

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    pub org: String,
    pub uuid: String,
    pub mbti: String,
}

/// user_uuid를 읽고, 없으면 UUID v4를 1회 생성·저장한 뒤 반환한다(이후 고정).
pub(crate) fn ensure_uuid(store: &SqliteStore) -> Result<String, String> {
    if let Some(u) = store.get_setting("user_uuid").ok().flatten() {
        let u = u.trim().to_string();
        if !u.is_empty() {
            return Ok(u);
        }
    }
    let u = uuid::Uuid::new_v4().to_string();
    store.set_setting("user_uuid", &u).map_err(|e| e.to_string())?;
    Ok(u)
}

/// 스프라이트 생성 정체성 = (uuid, mbti). uuid는 없으면 생성.
pub(crate) fn sprite_identity(store: &SqliteStore) -> Result<(String, Option<String>), String> {
    let uuid = ensure_uuid(store)?;
    let mbti = store
        .get_setting("user_mbti")
        .ok()
        .flatten()
        .and_then(|m| agent_mentor::mascot::normalize_mbti(&m));
    Ok((uuid, mbti))
}

#[tauri::command(async)]
pub fn profile_get(state: State<AppState>) -> Result<Profile, String> {
    let guard = lock(&state)?;
    let get = |k: &str| guard.get_setting(k).ok().flatten().unwrap_or_default();
    let uuid = ensure_uuid(&guard)?;
    let org = {
        let o = get("user_org");
        if o.trim().is_empty() { DEFAULT_ORG.to_string() } else { o }
    };
    Ok(Profile { name: get("user_name"), org, uuid, mbti: get("user_mbti") })
}

/// 개인정보 저장. uuid는 불변(여기서 안 받음). mbti는 빈값(미설정) 또는 유효 4글자만 허용.
#[tauri::command(async)]
pub fn profile_set(
    state: State<AppState>,
    name: String,
    org: String,
    mbti: String,
) -> Result<Profile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("이름을 입력하세요".into());
    }
    let mbti_norm = if mbti.trim().is_empty() {
        String::new()
    } else {
        agent_mentor::mascot::normalize_mbti(&mbti)
            .ok_or_else(|| "MBTI는 E/I·S/N·T/F·J/P 조합 4글자여야 해요 (예: INTJ)".to_string())?
    };
    let org = org.trim();
    let org = if org.is_empty() { DEFAULT_ORG } else { org };
    let old_name = {
        let guard = lock(&state)?;
        guard.get_setting("user_name").ok().flatten().unwrap_or_default()
    };
    if name != old_name {
        if let Some(client) = hub_client(&state)? {
            client.rename(&name).map_err(|e| format!("Life 서버 이름 변경 실패: {e}"))?;
        }
    }
    {
        let guard = lock(&state)?;
        guard.set_setting("user_name", &name).map_err(|e| e.to_string())?;
        guard.set_setting("hub_user", &name).map_err(|e| e.to_string())?;
        guard.set_setting("user_org", org).map_err(|e| e.to_string())?;
        guard.set_setting("user_mbti", &mbti_norm).map_err(|e| e.to_string())?;
    }
    profile_get(state)
}

// ── 주인 메모리 (2026-07-22-owner-memory 스펙) ──

#[tauri::command(async)]
pub fn memory_list(state: State<AppState>) -> Result<Vec<agent_mentor::memory::Memory>, String> {
    let guard = lock(&state)?;
    guard.list_memories().map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn memory_add(state: State<AppState>, text: String) -> Result<agent_mentor::memory::Memory, String> {
    let guard = lock(&state)?;
    let id = guard.add_memory(&text, "manual").map_err(|e| e.to_string())?;
    guard
        .list_memories()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "저장 직후 메모리를 찾지 못했어요".to_string())
}

#[tauri::command(async)]
pub fn memory_update(state: State<AppState>, id: i64, text: String) -> Result<agent_mentor::memory::Memory, String> {
    let guard = lock(&state)?;
    guard.update_memory(id, &text).map_err(|e| e.to_string())?;
    guard
        .list_memories()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "수정 대상 메모리를 찾지 못했어요".to_string())
}

#[tauri::command(async)]
pub fn memory_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    let guard = lock(&state)?;
    guard.delete_memory(id).map_err(|e| e.to_string())
}

/// 내 캐릭터 재생성 — 이미지 모델로 새로 그려 캐시를 교체. 네트워크는 **락 밖**(규율 동일).
/// 완료 시 `sprite:ready` emit → 마스코트가 즉시 교체된다.
#[tauri::command(async)]
pub fn regenerate_sprite(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    use tauri::{Emitter as _, Manager as _};
    // 락 범위: 설정 해석 + 프로필(uuid·mbti)만
    let (cfg, uuid, mbti) = {
        let guard = lock(&state)?;
        let cfg = crate::resolve_sprite_cfg(&guard);
        let (uuid, mbti) = sprite_identity(&guard)?;
        (cfg, uuid, mbti)
    };
    let Some(cfg) = cfg else {
        return Err("이미지 모델이 설정되지 않았어요 — 설정 → 캐릭터 이미지에서 URL·키를 넣어주세요".into());
    };
    let spec = agent_mentor::mascot::robot_spec_from_profile(&uuid, mbti.as_deref());
    let desc = agent_mentor::sprite::character_description(&spec, &uuid);
    // 락 밖 네트워크 (수십 초 걸릴 수 있음 — async 커맨드라 UI는 안 막힌다)
    let png = agent_mentor::sprite::generate(&cfg, &desc).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("sprite.png"), png).map_err(|e| e.to_string())?;
    if let Some(client) = hub_client(&state)? {
        let _ = upload_cached_mascot(&app, &client);
    }
    log::info!("캐릭터 재생성 완료");
    let _ = app.emit("sprite:ready", ());
    Ok(())
}

/// 방 점유자 스프라이트 캐시 경로 — app_data/sprites/<hash>.png.
fn occupant_sprite_path(app: &tauri::AppHandle, seed: &str) -> Result<std::path::PathBuf, String> {
    use tauri::Manager as _;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("sprites");
    Ok(dir.join(format!("{}.png", agent_mentor::sprite::seed_cache_name(seed))))
}

/// 방 점유자의 AI 스프라이트(캐시) base64 — 없으면 None(프론트는 절차 생성 폴백).
/// 다른 사람도 내 캐릭터와 동일 로직(seed→spec→description→이미지)으로 그려 화풍을 맞춘다.
#[tauri::command(async)]
pub fn get_occupant_sprite(app: tauri::AppHandle, seed: String) -> Result<Option<String>, String> {
    use base64::Engine as _;
    let p = occupant_sprite_path(&app, &seed)?;
    match std::fs::read(&p) {
        Ok(bytes) => Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes))),
        Err(_) => Ok(None),
    }
}

/// 점유자 스프라이트를 백그라운드로 생성 요청(즉시 반환). 이미지 모델 미설정이면 no-op(절차 유지).
/// 완료 시 `occupant-sprite:ready`(payload=seed) 이벤트 → 프론트가 다시 불러와 교체.
#[tauri::command(async)]
pub fn request_occupant_sprite(
    app: tauri::AppHandle,
    state: State<AppState>,
    seed: String,
) -> Result<(), String> {
    use tauri::Emitter as _;
    let p = occupant_sprite_path(&app, &seed)?;
    if p.exists() {
        return Ok(()); // 이미 있음
    }
    // 설정창(image_*) → env 순 해석 — 내 캐릭터와 동일한 이미지 엔드포인트를 쓴다.
    let cfg = {
        let guard = lock(&state)?;
        crate::resolve_sprite_cfg(&guard)
    };
    let Some(cfg) = cfg else {
        return Ok(()); // 이미지 모델 미설정 — 절차 폴백 유지
    };
    // pending 마커로 동시/중복 생성 방지 (토큰 낭비 차단)
    let pending = p.with_extension("pending");
    if pending.exists() {
        return Ok(());
    }
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&pending, b"");
    let app2 = app.clone();
    std::thread::spawn(move || {
        let res = agent_mentor::sprite::sprite_for_seed(&cfg, &seed);
        let _ = std::fs::remove_file(&pending);
        match res {
            Ok(png) => {
                if std::fs::write(&p, png).is_ok() {
                    log::info!("점유자 AI 스프라이트 생성: {}", p.display());
                    let _ = app2.emit("occupant-sprite:ready", seed);
                }
            }
            Err(e) => log::warn!("점유자 스프라이트 생성 실패: {e}"),
        }
    });
    Ok(())
}

// --- R6 반복 지시 → SKILL.md 초안 (skill_draft) ---
// 엔진은 store 설정(engine_url…) 우선, 없으면 .env 폴백 — engine_settings_get과 같은 규칙.
// 규율: 재료 수집(SQL)은 락 안, 초안 생성(LLM 네트워크)은 락 밖.

/// store 설정 우선으로 엔진을 구성. 둘 다 없으면 None(→ 결정론 골격 폴백).
fn resolve_engine(store: &SqliteStore) -> Option<OpenAiCompatEngine> {
    let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
    let url = get("engine_url").trim().to_string();
    if !url.is_empty() {
        let model = get("engine_model");
        return Some(OpenAiCompatEngine {
            base_url: url,
            api_key: get("engine_key").trim().to_string(),
            model: if model.trim().is_empty() { "gpt-4o-mini".into() } else { model.trim().into() },
        });
    }
    OpenAiCompatEngine::from_env()
}

#[derive(Debug, Serialize)]
pub struct SkillDraftResult {
    pub markdown: String,
    pub slug: String,
    pub llm_generated: bool,
    pub session_count: u64,
}

/// R6 반복 지시로 반복 워크플로를 되짚어 SKILL.md 초안을 생성. host+대표 프롬프트로 매칭.
/// 판정이 제안한 이름(suggested_name)이 있으면 초안 기본 슬러그로 쓴다.
#[tauri::command(async)]
pub fn generate_skill_draft(
    state: State<AppState>,
    host: String,
    representative: String,
    suggested_name: Option<String>,
    member_norms: Option<Vec<String>>,
) -> Result<SkillDraftResult, String> {
    // 1) 재료 수집 + 엔진 구성 (락 안, SQL만). 느슨한 묶음(member_norms)이 오면 묶음 전체
    //    변형의 세션·도구를 모아 카드가 센 세션 수와 초안이 일치하도록 한다.
    let (ctx, engine) = {
        let guard = lock(&state)?;
        let ctx = match member_norms.as_deref() {
            Some(norms) if !norms.is_empty() => {
                agent_mentor::skill_draft::gather_context_multi(&guard, &host, &representative, norms)
            }
            _ => agent_mentor::skill_draft::gather_context(&guard, &host, &representative),
        }
        .map_err(|e| e.to_string())?;
        (ctx, resolve_engine(&guard))
    };
    // 2) 초안 생성 (락 밖, LLM 네트워크 가능)
    let mut draft = agent_mentor::skill_draft::build_draft(&ctx, engine.as_ref().map(|e| e as &dyn Engine));
    // 판정이 제안한 이름이 있으면 기본 슬러그로 사용. 저장 디렉터리와 SKILL.md 정체성이
    // 어긋나지 않도록 프런트매터 name: 도 같은 슬러그로 맞춘다.
    if let Some(name) = suggested_name.as_deref().filter(|s| !s.trim().is_empty()) {
        let slug = agent_mentor::skill_draft::slugify(name);
        draft.markdown = agent_mentor::skill_draft::set_frontmatter_name(&draft.markdown, &slug);
        draft.slug = slug;
    }
    Ok(SkillDraftResult {
        markdown: draft.markdown,
        slug: draft.slug,
        llm_generated: draft.llm_generated,
        session_count: ctx.session_count,
    })
}

/// 초안을 사용자의 스킬 디렉터리에 저장 — `%USERPROFILE%\.claude\skills\<slug>\SKILL.md`.
/// 이미 있으면 덮어쓰지 않고 `-2`, `-3`… 접미를 붙여 사용자의 기존 스킬을 보호한다.
#[tauri::command(async)]
pub fn save_skill_draft(slug: String, markdown: String) -> Result<String, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "홈 디렉터리를 찾지 못했습니다".to_string())?;
    let base = std::path::Path::new(&home).join(".claude").join("skills");
    let path = agent_mentor::skill_draft::write_draft(&base, &slug, &markdown)
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
