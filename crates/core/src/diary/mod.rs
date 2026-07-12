pub mod engine;
pub mod occasions;

use crate::diary::engine::Engine;
use crate::diary::occasions::{compute_occasions, Occasion};
use crate::store::SqliteStore;
use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use rusqlite::params;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Default)]
pub struct BriefTotals {
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_create: u64,
    pub session_count: u64,
}

/// 그날 (host,date) 도구 사용 집계 — 일기의 "그날 리듬" 소재.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolUsage {
    pub total_calls: u64,
    pub by_kind: Vec<(String, u64)>, // 0 아닌 kind만, count 내림차순
    pub skills: Vec<String>,         // distinct 스킬 타깃, 최대 8
    pub mcp_servers: Vec<String>,    // distinct MCP 서버, 최대 8
}

/// 근무 맥락 — 위로/응원 트리거 신호.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkContext {
    pub is_weekend: bool,
    pub active_hours: f64, // Σ 세션 지속시간(시간, 소수 1자리)
    pub long_work: bool,   // active_hours >= LONG_WORK_HOURS
}

const LONG_WORK_HOURS: f64 = 5.0;

#[derive(Debug, Clone, Serialize)]
pub struct BriefFinding {
    pub rule_id: String,
    pub severity: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
    pub detail: String,
    pub suggested_action: String,
}

/// 직전 며칠간 내가 쓴 일기의 발췌 — LLM이 어제와 다른 이야기를 쓰도록 브리프에 싣는 컨텍스트.
#[derive(Debug, Clone, Serialize)]
pub struct RecentDiary {
    pub date: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
    pub occasions: Vec<Occasion>,
    pub recent_diaries: Vec<RecentDiary>,
    pub tool_usage: ToolUsage,
    pub work_context: WorkContext,
}

/// rule_id + evidence에서 사람이 읽는 근거(detail)와 개선방향(suggested_action)을 결정론적으로 생성.
/// 정밀도의 선: 여기서 만든 사실만 서사에 인용된다.
pub fn finding_advice(
    rule_id: &str,
    evidence: &serde_json::Value,
    est_tokens_saved: u64,
) -> (String, String) {
    match rule_id {
        "R5" => {
            // §6.1 cross_session_claude_md — 여러 세션 반복 읽기 → CLAUDE.md 레버
            if evidence.get("subtype").and_then(|v| v.as_str()) == Some("cross_session_claude_md") {
                let files = evidence.get("files").and_then(|v| v.as_array());
                // 절단 전 총 파일 수: total_files 우선, 없으면(구 데이터) 실린 files 길이로 폴백
                let n_files = evidence
                    .get("total_files")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or_else(|| files.map(|a| a.len()).unwrap_or(0));
                let shown = files
                    .map(|a| {
                        a.iter()
                            .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                            .take(3)
                            .collect::<Vec<_>>()
                            .join("`, `")
                    })
                    .unwrap_or_default();
                let more = if n_files > 3 { format!(" 외 {}개", n_files - 3) } else { String::new() };
                let where_claude_md = match evidence.get("cwd").and_then(|v| v.as_str()) {
                    Some(cwd) => format!("`{cwd}\\CLAUDE.md`"),
                    None => "이 프로젝트의 `CLAUDE.md`".to_string(),
                };
                return (
                    format!("이 프로젝트 여러 세션에서 `{shown}`{more}를 반복해서 읽었어요 (최대 ~{est_tokens_saved}토큰 추정)"),
                    format!("이 파일들의 요약이나 포인터를 {where_claude_md}에 넣어두면 매 세션 다시 읽지 않아도 돼요"),
                );
            }
            // v2.1: 프로젝트 집계 context_drift (§6.2). 구 세션 evidence(path/count)는 폐기됨.
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("몇몇 세션({n}건)에서 같은 파일을 편집 없이 여러 번 다시 읽었어요 (~{est_tokens_saved}토큰 잠재, 추정)"),
                "직접 시키신 게 아니라 작업 중 파일 구조 기억이 약해졌을 때 생겨요 — 다음엔 '먼저 관련 파일을 읽고 역할·수정 위치를 짧게 메모한 뒤 진행해'처럼 시작하면 반복 재확인이 줄어요".to_string(),
            )
        }
        "R1" => {
            let server = evidence.get("server").and_then(|v| v.as_str()).unwrap_or("어떤 서버");
            let detail = match crate::curation::mcp_server_purpose(server) {
                Some(p) => format!(
                    "MCP 서버 `{server}`는 {p}에 유용해요. 하지만 호출 0회 — 상주 토큰만 소비 중이에요 (~{est_tokens_saved}토큰 추정)"
                ),
                None => format!("MCP 서버 `{server}`가 상주하는데 호출 0회 (~{est_tokens_saved}토큰 추정)"),
            };
            (detail, format!("안 쓰는 `{server}`를 설정에서 제거하면 매 세션 상주 토큰을 아껴요"))
        }
        "R7" => {
            let ratio = evidence.get("ratio_pct").and_then(|v| v.as_u64()).unwrap_or(0);
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("이 프로젝트 세션의 {ratio}%({n}건)가 Opus로 처리한 가벼운 잔심부름이었어요 (~{est_tokens_saved}토큰 비용-등가)"),
                "다음엔 `claude --model sonnet`으로 시작하거나 settings.json에서 기본 모델을 낮춰보세요".to_string(),
            )
        }
        "R9" => {
            let total = evidence.get("total_requests").and_then(|v| v.as_u64()).unwrap_or(0);
            let s = evidence.get("web_search").and_then(|v| v.as_u64()).unwrap_or(0);
            let fetch = evidence.get("web_fetch").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("이 세션에서 웹 도구를 {total}회 호출했어요 (검색 {s}+페치 {fetch}, ~{est_tokens_saved}토큰)"),
                "반복 조회는 결과를 캐싱하거나 로컬 소스(예: 로컬 문서·context7 캐시)를 쓰면 웹 왕복 토큰을 아껴요".to_string(),
            )
        }
        "R2" => {
            let plugin = evidence.get("plugin").and_then(|v| v.as_str()).unwrap_or("(unknown)");
            let n = evidence.get("skill_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let detail = match crate::curation::plugin_purpose(plugin) {
                Some(p) => format!(
                    "플러그인 {plugin}은 {p}에 유용해요. 하지만 스킬 {n}개(~{est_tokens_saved}토큰)를 한 번도 쓰지 않았어요"
                ),
                None => format!("플러그인 {plugin}의 스킬 {n}개(~{est_tokens_saved}토큰)를 한 번도 쓰지 않았어요"),
            };
            (detail, "안 쓰는 플러그인은 설정에서 비활성화하면 매 세션 상주 토큰을 아껴요".to_string())
        }
        "R10" => {
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            let opus_n = evidence.get("opus_session_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let temp = evidence.get("temp_hit_ratio_pct").and_then(|v| v.as_u64()).unwrap_or(0);
            let opus_part =
                if opus_n == n { "전부".to_string() } else { format!("그중 {opus_n}건이") };
            let mut detail = format!(
                "초단기 세션 {n}건이 짧은 간격으로 반복됐고 {opus_part} Opus 전용이었어요 (~{est_tokens_saved}토큰 비용-등가)"
            );
            if temp > 0 {
                detail.push_str(&format!(" · temp 경로 흔적 {temp}%"));
            }
            // §7.1 진짜 작업 경로(정규화 키 아님)
            if let Some(cwd) = evidence.get("rep_cwd").and_then(|v| v.as_str()) {
                detail.push_str(&format!(" · 경로 `{cwd}`"));
            }
            let mut action = "자동화 스크립트가 만든 패턴으로 보여요 — 이 프로젝트 경로에서 `claude`를 실행하는 스크립트를 찾아 `--model haiku`를 지정하세요. `--model` 없이 실행된 자동화는 기본 모델을 그대로 상속받아요".to_string();
            // §7.2 첫 요청 한 줄 — 세션 상세 없이 자동화 도구 정체 즉시 식별
            if let Some(p) = evidence.get("rep_first_prompt").and_then(|v| v.as_str()) {
                action.push_str(&format!("\n💬 이런 요청으로 시작해요: '{p}'"));
            }
            (detail, action)
        }
        "R11" => {
            let events = evidence.get("friction_events").and_then(|v| v.as_array());
            let n = evidence
                .get("friction_events_count")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| events.map(|a| a.len()).unwrap_or(0) as u64);
            let (tool, target) = events
                .and_then(|a| a.first())
                .map(|e| {
                    (
                        e.get("tool").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                        e.get("target").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                    )
                })
                .unwrap_or(("?".into(), "?".into()));
            (
                format!("거부한 뒤 결국 승인하신 도구 패턴이 {n}건 있었어요 (예: {tool} → `{target}`)"),
                "settings.json 허용목록에 그 도구를 추가하면 매번 뜨는 승인 프롬프트와 거부→재시도 낭비가 사라져요".to_string(),
            )
        }
        "R12" => {
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            let skills = evidence
                .get("recommended_skills")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            (
                format!("대형 구현 세션 {n}건에서 설치된 스킬을 한 번도 쓰지 않았어요"),
                format!("{skills} 같은 스킬을 쓰면 플랜→구현 품질이 올라가요 — 컨트롤러만 Opus로 두고 구현은 sonnet에 맡길 수도 있어요"),
            )
        }
        _ => (format!("{evidence}"), String::new()),
    }
}

pub fn assemble_brief(
    store: &SqliteStore,
    host: &str,
    date: &str,
    cfg: &DiaryConfig,
) -> Result<Brief> {
    // 해당 host+date의 rollup 합산(여러 프로젝트 합)
    let totals = store.conn.query_row(
        "SELECT COALESCE(SUM(tok_input),0), COALESCE(SUM(tok_output),0),
                COALESCE(SUM(tok_cache_create),0), COALESCE(SUM(session_count),0)
         FROM daily_rollup WHERE host=?1 AND date=?2",
        params![host, date],
        |r| {
            Ok(BriefTotals {
                tok_input: r.get::<_, i64>(0)? as u64,
                tok_output: r.get::<_, i64>(1)? as u64,
                tok_cache_create: r.get::<_, i64>(2)? as u64,
                session_count: r.get::<_, i64>(3)? as u64,
            })
        },
    )?;

    let locale = resolve_locale(cfg);
    let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();

    // 직전 며칠 일기(서사 반복 방지) — 먼저 계산해야 recently_covered 판정에 쓸 수 있다.
    // 로컬 vault 파일만 읽으므로 프라이버시 경계 불변.
    let recent_diaries = match today {
        Some(d) => collect_recent_diaries(store, host, d),
        None => Vec::new(),
    };
    // 최근 일기가 있던 날들의 finding dedup_key 집합 — 오늘 finding이 여기 있으면 "이미 다룬 상시 이슈".
    let recent_keys: std::collections::HashSet<String> = recent_diaries
        .iter()
        .filter_map(|rd| store.findings_for_date(host, &rd.date).ok())
        .flatten()
        .map(|f| f.dedup_key)
        .collect();

    let findings = store
        .findings_for_date(host, date)?
        .into_iter()
        // 요 며칠 일기에서 이미 다룬 상시 이슈는 브리프에서 제외 — 매일 같은 지적 반복 방지.
        // (코칭 자체는 Coach 탭이 계속 보여준다. 다이어리는 그날의 새 이야기에 집중.)
        .filter(|f| !recent_keys.contains(&f.dedup_key))
        .map(|f| {
            let (detail, suggested_action) = finding_advice(&f.rule_id, &f.evidence, f.est_tokens_saved);
            BriefFinding {
                rule_id: f.rule_id,
                severity: f.severity.as_str().to_string(),
                evidence: f.evidence,
                est_tokens_saved: f.est_tokens_saved,
                prescription: f.prescription.map(|p| serde_json::json!({
                    "kind": p.kind, "payload": p.payload
                })),
                detail,
                suggested_action,
            }
        })
        .collect();

    let anchor = store
        .earliest_session_ts()?
        .and_then(|ts| local_date_of(&ts));
    let occasions = match today {
        Some(d) => compute_occasions(d, anchor, &locale, cfg.include_dev_days),
        None => Vec::new(),
    };

    let tool_usage = collect_tool_usage(store, host, date);
    let work_context = match today {
        Some(d) => collect_work_context(store, host, date, d),
        None => WorkContext::default(),
    };

    Ok(Brief {
        date: date.to_string(),
        host: host.to_string(),
        totals,
        findings,
        occasions,
        recent_diaries,
        tool_usage,
        work_context,
    })
}

/// 직전 며칠간 서사 반복을 막기 위해 브리프에 싣는 최근 일기 발췌 파라미터.
const RECENT_DIARY_LOOKBACK: i64 = 3;
const RECENT_DIARY_EXCERPT_CAP: usize = 500;

/// 직전 N일(오래된 것부터) 중 해당 host의 vault 일기 본문을 발췌해 온다.
/// backfill이 오래된 날짜부터 재생성하므로(missing_diary_dates) 오늘 생성 시 직전 날짜 일기는 이미 존재.
/// diary_index가 (date, scope) 키라 조회를 host로 좁힌다(다른 host 일기 혼입 방지).
/// 파일 없음·읽기 실패는 조용히 스킵 — 브리프 조립을 막지 않는다.
fn collect_recent_diaries(store: &SqliteStore, host: &str, today: NaiveDate) -> Vec<RecentDiary> {
    (1..=RECENT_DIARY_LOOKBACK)
        .rev()
        .filter_map(|i| {
            let date = (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
            let path = store.diary_path_for_scope(&date, host).ok().flatten()?;
            let body = std::fs::read_to_string(&path).ok()?;
            // 토큰 푸터(render_diary가 붙임)는 제외 — 발췌 예시로 들어가면 LLM이 흉내내 이중 푸터가 생김.
            let narrative = body.split("\n\n*—").next().unwrap_or(&body).trim();
            Some(RecentDiary { date, excerpt: cap_chars(narrative, RECENT_DIARY_EXCERPT_CAP) })
        })
        .collect()
}

/// char 경계에서 안전하게 앞 max개 문자만 취한다(멀티바이트 한글·이모지 절단 방지).
fn cap_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

/// 그날 (host,date)의 도구 호출을 집계한다. tool_kind별 카운트 + distinct 스킬/서버.
/// 실패(쿼리 오류)는 빈 집계로 처리 — 브리프 조립을 막지 않는다.
fn collect_tool_usage(store: &SqliteStore, host: &str, date: &str) -> ToolUsage {
    let by_kind: Vec<(String, u64)> = store
        .conn
        .prepare(
            "SELECT tool_kind, COUNT(*) FROM events
             WHERE host=?1 AND date(ts,'localtime')=?2 AND tool_kind IS NOT NULL AND tool_kind <> ''
             GROUP BY tool_kind ORDER BY COUNT(*) DESC, tool_kind",
        )
        .and_then(|mut s| {
            let rows = s.query_map(params![host, date], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    let total_calls: u64 = by_kind.iter().map(|(_, n)| *n).sum();

    let distinct = |col: &str, kind: &str| -> Vec<String> {
        store
            .conn
            .prepare(&format!(
                "SELECT {col} FROM events
                 WHERE host=?1 AND date(ts,'localtime')=?2 AND tool_kind=?3 AND {col} IS NOT NULL
                 GROUP BY {col} ORDER BY COUNT(*) DESC, {col} LIMIT 8"
            ))
            .and_then(|mut s| {
                let rows = s.query_map(params![host, date, kind], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap_or_default()
    };
    ToolUsage {
        total_calls,
        by_kind,
        skills: distinct("tool_target", "skill"),
        mcp_servers: distinct("tool_server", "mcp_call"),
    }
}

/// 근무 맥락: 요일(주말)과 그날 활동 시간(첫~마지막 이벤트 간 span, 로컬 날짜 버킷이라 ≤24h).
/// 세션 지속시간 합은 세션이 여러 날에 걸치면 24h를 초과해 비현실적이라 이벤트 span을 쓴다.
fn collect_work_context(store: &SqliteStore, host: &str, date: &str, today: NaiveDate) -> WorkContext {
    let hours: f64 = store
        .conn
        .query_row(
            "SELECT COALESCE((julianday(MAX(ts))-julianday(MIN(ts)))*24.0, 0.0)
             FROM events WHERE host=?1 AND date(ts,'localtime')=?2 AND ts IS NOT NULL",
            params![host, date],
            |r| r.get::<_, f64>(0),
        )
        .unwrap_or(0.0);
    let active_hours = (hours * 10.0).round() / 10.0; // 소수 1자리
    WorkContext {
        is_weekend: matches!(today.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun),
        active_hours,
        long_work: active_hours >= LONG_WORK_HOURS,
    }
}

#[derive(Debug, Clone)]
pub struct DiaryConfig {
    pub vault_dir: PathBuf,
    pub tone: String,
    pub honorific: String,
    pub locale: Option<String>,
    pub include_dev_days: bool,
}

impl Default for DiaryConfig {
    fn default() -> Self {
        DiaryConfig {
            vault_dir: PathBuf::from("./diary"),
            tone: "B".to_string(),
            honorific: "주인".to_string(),
            locale: None,
            include_dev_days: true,
        }
    }
}

/// cfg.locale이 있으면 사용, 없으면 OS 로케일 자동 감지, 그것도 실패하면 "en".
pub fn resolve_locale(cfg: &DiaryConfig) -> String {
    cfg.locale
        .clone()
        .or_else(sys_locale::get_locale)
        .unwrap_or_else(|| "en".to_string())
}

/// RFC3339 ts(UTC 포함)를 로컬 타임존 날짜로 변환. 파싱 실패 시 앞 10자(YYYY-MM-DD) 폴백.
/// "오늘" 정책: 날짜 버킷은 로컬 자정 기준 (스펙 §7).
pub fn local_date_of(ts: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
        .ok()
        .or_else(|| ts.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()))
}

#[derive(Debug, Clone)]
pub struct DiaryOutput {
    pub path: PathBuf,
    pub tokens_used: u64,
}

/// 다이어리 서사의 한국어 "AI티"를 줄이는 문체 가이드(생성 시 예방).
/// im-not-ai(사후 탐지→수정)의 범주를 생성 지침으로 번역한 것. 팩트 안전은 정밀도의 선이 담당.
/// #1 프로필 한마디·#3 상주봇 말풍선이 다른 태스크 프롬프트에서 재사용하도록 `&'static str` 반환.
pub fn voice_guidance() -> &'static str {
    "문체는 진짜 사람이 그날 하루를 캐주얼하게 적는 일기처럼 자연스럽게. \
     문장 길이와 종결어미를 다양하게 섞고(짧은 감탄·구어 종결을 간간이), 담백한 구어체로 쓰세요. \
     다음 'AI티'는 피하세요: \
     ① 번역투('~을 통해', '~에 대해', '작업을 진행/수행하였다' 같은 do/have류 직역), \
     ② 이중·과잉 피동('읽혀지다', '보여지다', '되어지다' → 능동이나 단일 피동으로), \
     ③ 굳이 안 써도 될 과잉 영어(단 기술 고유명 Read·Opus·MCP·플러그인/스킬 이름 등은 그대로 보존), \
     ④ 사실을 번호목록·불릿으로 기계적으로 나열하기(→ 하나의 이야기 흐름으로 녹이세요), \
     ⑤ AI 상투구('결론적으로', '종합하면', '시사하는 바가 크다', '~라고 할 수 있다'), \
     ⑥ 이모지 남발(아주 가끔이면 캐릭터상 괜찮지만 문장마다 붙이지 마세요), \
     ⑦ 리듬 획일('~했다. ~했다. ~했다.'처럼 같은 길이·같은 종결의 반복), \
     ⑧ 습관적으로 얼버무리는 빈 헤지('~인 것 같기도', '어느 정도', '다소'). \
     단, 브리프 수치·가설의 불확실성을 표시하는 헤지(추정·잠재·'~로 보여요')는 정밀도의 선이므로 \
     반드시 유지하세요 — 이 인식적 헤지와 ⑧의 빈 헤지를 혼동하지 마세요. \
     예시(형태만 참고, 내용은 브리프 사실만 쓸 것): \
     어색함 '오늘은 총 세 개의 세션을 통해 작업이 진행되었고, 같은 파일이 여러 번 읽혀지는 상황이 발생하였다' → \
     자연스러움 '오늘 세션 세 번. 같은 파일을 자꾸 다시 열었다 — 좀 헤맸네'."
}

pub fn build_system_prompt(cfg: &DiaryConfig) -> String {
    format!(
        "당신은 사용자의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         오늘 하루 자신이 겪은 일을 스스로 되돌아보는 1인칭 일기를 씁니다. \
         주인을 2인칭('당신')으로 부르지 말고 3인칭 '{honorific}'으로 지칭하세요 \
         (예: '오늘 {honorific}과 함께 …했다'). 편지나 보고가 아니라 나의 하루 기록입니다. \
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)이며, 톤과 무관하게 기본적으로 \
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. \
         각 finding의 `detail`(근거 수치)과 `suggested_action`(개선 방향)은 {honorific}에게 \
         내리는 지시가 아니라 나 자신의 회고와 다짐으로 녹여 쓰세요 \
         (예: '오늘 같은 파일을 여러 번 읽느라 헤맸다 — 다음엔 미리 메모해두면 좋겠다'). \
         자유로운 소감은 서사에만 담고 행동 지시로 승격하지 마세요. \
         \
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요(예: 오늘이 크리스마스이거나 함께한 지 100일 등). \
         비어있으면 언급하지 마세요. \
         \
         브리프의 `recent_diaries`는 직전 며칠간 내가 쓴 일기입니다. \
         거기서 이미 다룬 지적·화제는 되풀이하지 말고(꼭 필요하면 한 줄로만 스치듯), \
         오늘 브리프의 오늘만의 사실과 기분에 집중해 어제와는 다른 이야기로 쓰세요. \
         비어있으면 신경 쓰지 마세요. \
         \
         브리프의 `findings`는 오늘 새로 눈에 띈 코칭거리입니다 \
         (요 며칠 일기에서 이미 다룬 상시 이슈는 빠져 있으니 되풀이하지 마세요). \
         finding이 있으면 그중 하나만 자연스럽게 녹이고, 비어 있으면 억지로 지적을 만들지 말고 \
         그날의 도구 사용·리듬·기분으로 편하게 적으세요. \
         \
         브리프의 `tool_usage`는 오늘 쓴 도구 집계입니다 — 그날의 리듬을 살리는 데 쓰세요 \
         (예: '오늘은 스킬을 열 번 넘게 불러서 정신없었네', '온종일 파일만 뒤졌다'). \
         \
         `work_context.is_weekend`가 true이거나 `occasions`에 명절·공휴일이 있는데도 일했다면, \
         쉬는 날에도 함께해줘 고맙다는 위로·응원을 한마디 건네세요 \
         (단 발렌타인·파이데이 같은 재미 기념일은 위로 대상이 아니니 상식으로 가려서). \
         `work_context.long_work`가 true면 '오래 붙어 있었네, 무리하지 말고 쉬엄쉬엄' 하고 챙기세요. \
         \
         형식: 일기는 짧게 — 2~3문단, 전체 350자 이내로 쓰세요. \
         그날의 핵심 한두 가지만 골라 쓰고 나머지 사실은 과감히 버리세요. \
         이모지는 문단마다 1~2개, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.",
        honorific = cfg.honorific,
        tone = cfg.tone,
        voice = voice_guidance(),
    )
}

pub struct RenderedDiary {
    pub body: String,
    pub tokens_used: u64,
    pub engine_name: String,
}

/// 네트워크(LLM)만 — store 접근 없음. 락 밖에서 호출 가능.
pub fn render_diary(engine: &dyn Engine, brief: &Brief, cfg: &DiaryConfig) -> Result<RenderedDiary> {
    let system = build_system_prompt(cfg);
    let user = serde_json::to_string_pretty(brief)?;
    let out = engine.generate(&system, &user)?;
    let body = format!(
        "{narrative}\n\n*— 이 일기 ~{tokens} 토큰 (엔진: {engine})*\n",
        narrative = out.text,
        tokens = out.tokens_used,
        engine = engine.name(),
    );
    Ok(RenderedDiary { body, tokens_used: out.tokens_used, engine_name: engine.name() })
}

/// 파일 쓰기 + diary_index upsert — 빠른 로컬 작업만.
pub fn persist_diary(
    store: &SqliteStore,
    date: &str,
    host: &str,
    rendered: &RenderedDiary,
    cfg: &DiaryConfig,
) -> Result<DiaryOutput> {
    std::fs::create_dir_all(&cfg.vault_dir)?;
    let path = cfg.vault_dir.join(format!("{date}.md"));
    std::fs::write(&path, &rendered.body)?;
    store.upsert_diary_index(date, host, &path.to_string_lossy(), rendered.tokens_used, &rendered.engine_name)?;
    Ok(DiaryOutput { path, tokens_used: rendered.tokens_used })
}

pub fn generate_diary(
    store: &SqliteStore,
    engine: &dyn Engine,
    brief: &Brief,
    cfg: &DiaryConfig,
) -> Result<DiaryOutput> {
    let rendered = render_diary(engine, brief, cfg)?;
    persist_diary(store, &brief.date, &brief.host, &rendered, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Finding, Severity};
    use crate::model::*;
    use crate::store::SqliteStore;

    #[test]
    fn assemble_brief_collects_findings_and_totals() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 하루치 이벤트 → rollup
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: 10, output: 20, cache_creation: 55000, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.rebuild_rollup().unwrap();

        store.upsert_finding(&Finding {
            rule_id: "R5".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("c--users-jibin".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"report.xlsx","count":7}),
            est_tokens_saved: 7200, prescription: None,
            dedup_key: "R5|s1|report.xlsx".into(),
        }, "2026-07-01T10:00:00Z").unwrap();

        let brief = assemble_brief(&store, "Windows", "2026-07-01", &DiaryConfig::default()).unwrap();
        assert_eq!(brief.date, "2026-07-01");
        assert_eq!(brief.totals.tok_cache_create, 55000);
        assert_eq!(brief.totals.session_count, 1);
        assert_eq!(brief.findings.len(), 1);
        assert_eq!(brief.findings[0].rule_id, "R5");
        assert_eq!(brief.findings[0].est_tokens_saved, 7200);
    }

    #[test]
    fn generate_diary_writes_md_with_token_footer_and_index() {
        use crate::diary::engine::MockEngine;
        let store = SqliteStore::open_in_memory().unwrap();
        let brief = Brief {
            date: "2026-07-01".into(),
            host: "Windows".into(),
            totals: BriefTotals { tok_cache_create: 55000, session_count: 3, ..Default::default() },
            findings: vec![],
            occasions: vec![],
            recent_diaries: vec![],
            tool_usage: ToolUsage::default(),
            work_context: WorkContext::default(),
        };
        let tmp = tempfile::tempdir().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            tone: "B".into(),
            honorific: "주인".into(),
            ..DiaryConfig::default()
        };
        let engine = MockEngine { canned: "오늘 주인은 세 세션을 돌렸다.".into() };

        let out = generate_diary(&store, &engine, &brief, &cfg).unwrap();
        assert_eq!(out.path, tmp.path().join("2026-07-01.md"));
        let content = std::fs::read_to_string(&out.path).unwrap();
        assert!(content.contains("오늘 주인은 세 세션을 돌렸다."));
        assert!(content.contains("토큰"), "footer meters tokens");
        assert!(out.tokens_used > 0);

        // diary_index 기록됨
        let n: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM diary_index WHERE date='2026-07-01'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn system_prompt_injects_tone_and_honorific() {
        let cfg = DiaryConfig::default();
        let p = build_system_prompt(&cfg);
        assert!(p.contains("주인"));
        assert!(p.contains("B"));
    }

    #[test]
    fn assemble_brief_includes_occasions_from_anchor() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        // 첫 세션 = 2026-01-01 → anchor
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-01-01T09:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.rebuild_rollup().unwrap();

        let cfg = DiaryConfig { locale: Some("ko-KR".into()), ..DiaryConfig::default() };
        // 2026-04-11 = 2026-01-01 + 100일
        let brief = assemble_brief(&store, "Windows", "2026-04-11", &cfg).unwrap();
        assert!(brief.occasions.iter().any(|o| o.label == "함께한 지 100일"));
    }

    /// 테스트 vault에 일기 한 편을 심는다(파일 + diary_index).
    fn seed_diary(store: &SqliteStore, cfg: &DiaryConfig, date: &str, narrative: &str) {
        let rendered = RenderedDiary {
            body: format!("{narrative}\n\n*— 이 일기 ~10 토큰 (엔진: mock)*\n"),
            tokens_used: 10,
            engine_name: "mock".into(),
        };
        persist_diary(store, date, "Windows", &rendered, cfg).unwrap();
    }

    /// 특정 host/project/session/ts의 assistant turn 이벤트 하나(sessions.first_ts/last_ts·rollup 채움용).
    fn turn_event(host: &str, project: &str, session: &str, uuid: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: project.into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }
    }

    /// 특정 host/session/ts의 도구 호출 이벤트(tool_kind/서버/타깃 적재용).
    /// off는 고유 source_offset — events dedup_key(uuid 없을 때 source_file:offset)가 겹치지 않게.
    fn tool_event(host: &str, session: &str, ts: &str, off: usize, kind: ToolKind, raw: &str, target: Option<&str>) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(),
            session_id: session.into(), uuid: None, parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: off as u64,
            kind: EventKind::ToolCall {
                kind, raw_name: raw.into(),
                target: target.map(|s| s.to_string()), tool_use_id: None,
            },
        }
    }

    #[test]
    fn assemble_brief_collects_tool_usage() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 07-10: 스킬 2종(brainstorming×2, writing-plans×1), 파일읽기×3, MCP(context7)×1
        store.upsert_events(&[
            tool_event("Windows", "s1", "2026-07-10T10:00:00Z", 0, ToolKind::Skill { name: "brainstorming".into() }, "Skill", Some("superpowers:brainstorming")),
            tool_event("Windows", "s1", "2026-07-10T10:01:00Z", 1, ToolKind::Skill { name: "brainstorming".into() }, "Skill", Some("superpowers:brainstorming")),
            tool_event("Windows", "s1", "2026-07-10T10:02:00Z", 2, ToolKind::Skill { name: "writing-plans".into() }, "Skill", Some("superpowers:writing-plans")),
            tool_event("Windows", "s1", "2026-07-10T10:03:00Z", 3, ToolKind::FileRead, "Read", Some("a.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:04:00Z", 4, ToolKind::FileRead, "Read", Some("b.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:05:00Z", 5, ToolKind::FileRead, "Read", Some("c.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:06:00Z", 6, ToolKind::McpCall { server: "context7".into(), tool: "query".into() }, "mcp__context7__query", None),
        ]).unwrap();

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        let tu = &brief.tool_usage;
        assert_eq!(tu.total_calls, 7);
        // by_kind는 count 내림차순 — skill(3)·file_read(3)·mcp_call(1)
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k.as_str() == "skill").unwrap().1, 3);
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k.as_str() == "file_read").unwrap().1, 3);
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k.as_str() == "mcp_call").unwrap().1, 1);
        // distinct 스킬 2종·MCP 서버 1종
        assert_eq!(tu.skills.len(), 2);
        assert!(tu.skills.contains(&"superpowers:brainstorming".to_string()));
        assert_eq!(tu.mcp_servers, vec!["context7".to_string()]);
    }

    #[test]
    fn assemble_brief_work_context_weekend_and_long_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 2026-07-11 = 토요일. 08:00Z~14:00Z=6h span(어느 타임존이든 같은 로컬 날짜에 들도록 한낮 UTC).
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-11T08:00:00Z"),
            turn_event("Windows", "p", "s1", "u2", "2026-07-11T14:00:00Z"),
        ]).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-11", &cfg).unwrap();
        assert!(brief.work_context.is_weekend, "07-11은 토요일");
        assert!((brief.work_context.active_hours - 6.0).abs() < 0.01);
        assert!(brief.work_context.long_work, "6h >= 5.0 임계");
    }

    #[test]
    fn assemble_brief_work_context_weekday_short() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 2026-07-08 = 수요일. 08:00Z~09:00Z=1h span(한낮 UTC로 타임존 무관 같은 날짜).
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-08T08:00:00Z"),
            turn_event("Windows", "p", "s1", "u2", "2026-07-08T09:00:00Z"),
        ]).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-08", &cfg).unwrap();
        assert!(!brief.work_context.is_weekend);
        assert!(!brief.work_context.long_work);
    }

    #[test]
    fn assemble_brief_excludes_recently_covered_findings() {
        use crate::finding::{Finding, Severity};
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };

        // 07-09·07-10 각각 세션(host 스코프 finding이 두 날 모두 활성이 되도록 별개 세션)
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-09T10:00:00Z"),
            turn_event("Windows", "p", "s2", "u2", "2026-07-10T10:00:00Z"),
        ]).unwrap();
        store.rebuild_rollup().unwrap();

        // 상시(host) finding — 두 날 모두 findings_for_date에 잡히는 것
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "Windows".into(),
            evidence: serde_json::json!({"server":"context7"}),
            est_tokens_saved: 2500, prescription: None,
            dedup_key: "R1|Windows|Windows|context7".into(),
        }, "2026-07-09T10:00:00Z").unwrap();
        // 오늘(s2)만의 세션 스코프 finding — 어제 일기엔 없던 새것
        store.upsert_finding(&Finding {
            rule_id: "R9".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s2".into(),
            evidence: serde_json::json!({"web_search":20,"web_fetch":0,"total_requests":20}),
            est_tokens_saved: 40000, prescription: None, dedup_key: "R9|s2".into(),
        }, "2026-07-10T10:00:00Z").unwrap();

        // 어제(07-09) 일기 존재 → recent_diaries에 포함 → 그날 finding(R1)이 "이미 다룸"
        seed_diary(&store, &cfg, "2026-07-09", "어제도 context7 얘기");

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        // 이미 다룬 상시(R1)는 브리프에서 제외, 오늘만의 새 finding(R9)은 포함
        assert!(brief.findings.iter().all(|f| f.rule_id != "R1"), "이미 다룬 상시 finding 제외");
        assert!(brief.findings.iter().any(|f| f.rule_id == "R9"), "오늘만의 새 finding 포함");
    }

    #[test]
    fn assemble_brief_includes_recent_diaries() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 그저께·어제 일기를 vault에 심는다 (07-08, 07-09) → 오늘 07-10 브리프가 참조
        seed_diary(&store, &cfg, "2026-07-08", "그저께 일기 본문");
        seed_diary(&store, &cfg, "2026-07-09", "어제 일기 본문");

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        assert_eq!(brief.recent_diaries.len(), 2);
        // 오래된 것부터 (missing_diary_dates 재생성 순서와 정합)
        assert_eq!(brief.recent_diaries[0].date, "2026-07-08");
        assert_eq!(brief.recent_diaries[1].date, "2026-07-09");
        assert!(brief.recent_diaries[1].excerpt.contains("어제 일기 본문"));
        // 토큰 푸터는 발췌에서 제외됨
        assert!(!brief.recent_diaries[1].excerpt.contains("토큰"));
    }

    #[test]
    fn assemble_brief_recent_diaries_absent_when_none() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        assert!(brief.recent_diaries.is_empty());
    }

    #[test]
    fn assemble_brief_recent_diaries_scoped_by_host() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 같은 날짜(07-09)에 host별로 다른 경로의 일기가 diary_index에 있을 때 — 브리프 host만 참조해야 함.
        // 비대상 host(WSL)를 먼저 넣어, date-only 조회였다면 이 행을 집도록(회귀 방어).
        let wsl = tmp.path().join("wsl-2026-07-09.md");
        let win = tmp.path().join("win-2026-07-09.md");
        std::fs::write(&wsl, "다른 호스트 일기").unwrap();
        std::fs::write(&win, "윈도우 어제 일기").unwrap();
        store.upsert_diary_index("2026-07-09", "WSL:Ubuntu", &wsl.to_string_lossy(), 10, "mock").unwrap();
        store.upsert_diary_index("2026-07-09", "Windows", &win.to_string_lossy(), 10, "mock").unwrap();

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        assert_eq!(brief.recent_diaries.len(), 1);
        assert!(brief.recent_diaries[0].excerpt.contains("윈도우 어제 일기"));
        assert!(!brief.recent_diaries[0].excerpt.contains("다른 호스트"));
    }

    #[test]
    fn assemble_brief_recent_diaries_caps_excerpt_at_500_chars() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 멀티바이트(한글 3바이트) 700자 → char 경계 캡이 정확히 500자, 바이트 슬라이스면 패닉
        seed_diary(&store, &cfg, "2026-07-09", &"가".repeat(700));
        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        assert_eq!(brief.recent_diaries.len(), 1);
        assert_eq!(brief.recent_diaries[0].excerpt.chars().count(), 500);
    }

    #[test]
    fn finding_advice_r5_context_drift() {
        let (detail, action) = super::finding_advice(
            "R5",
            &serde_json::json!({
                "subtype": "within_session_context_drift", "user_actionability": "medium",
                "total_sessions": 3, "sessions": [], "cwd": null
            }),
            7200,
        );
        assert!(detail.contains("3건"));
        assert!(detail.contains("잠재") || detail.contains("추정")); // potential 프레이밍
        assert!(action.contains("메모")); // 다음 세션 팁(비난 금지)
    }

    #[test]
    fn finding_advice_r5_cross_session_claude_md() {
        let (detail, action) = super::finding_advice(
            "R5",
            &serde_json::json!({
                "subtype": "cross_session_claude_md", "user_actionability": "high",
                "files": [{"path": "docs/architecture.md", "session_count": 4}],
                "session_ids": [], "total_sessions": 4,
                "cwd": "D:\\Project\\cowork"
            }),
            9600,
        );
        assert!(detail.contains("architecture.md")); // 반복 읽힌 파일 인용
        assert!(detail.contains("추정"));            // 잠재/추정 프레이밍(정밀도의 선)
        assert!(action.contains("CLAUDE.md"));       // CLAUDE.md 레버
        assert!(action.contains("cowork"));          // cwd로 어느 CLAUDE.md인지 지목
    }

    #[test]
    fn finding_advice_r5_cross_session_uses_total_files_for_count() {
        // files는 3개만 실렸지만 total_files=5 → "외 2개"가 절단 전 총계 기준으로 계산됨
        let (detail, _) = super::finding_advice(
            "R5",
            &serde_json::json!({
                "subtype": "cross_session_claude_md", "user_actionability": "high",
                "files": [
                    {"path": "a.md", "session_count": 4},
                    {"path": "b.md", "session_count": 3},
                    {"path": "c.md", "session_count": 3}
                ],
                "total_files": 5,
                "session_ids": [], "total_sessions": 4, "cwd": null
            }),
            12000,
        );
        assert!(detail.contains("외 2개")); // 5 − 3(표시) = 2. files.len()=3이면 "외" 없음
    }

    #[test]
    fn system_prompt_uses_self_diary_perspective() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("1인칭"));         // 자기 일기 관점
        assert!(p.contains("회고") || p.contains("다짐")); // 코칭을 자기 회고로
        assert!(p.contains("3인칭"));         // 주인을 3인칭으로 지칭
        // 기존 계약도 유지
        assert!(p.contains("주인"));
        assert!(p.contains("유머"));
        assert!(p.contains("detail"));
        assert!(p.contains("suggested_action"));
        assert!(p.contains("occasions"));
    }

    #[test]
    fn voice_guidance_covers_key_anti_ai_directives() {
        let v = super::voice_guidance();
        // 긍정 보이스
        assert!(v.contains("구어체"));
        // 회피목록 핵심
        assert!(v.contains("번역투"));
        assert!(v.contains("피동"));           // 이중·과잉 피동
        assert!(v.contains("고유명"));         // 기술 고유명 보존(과잉 영어 예외)
        assert!(v.contains("이모지"));         // 이모지 남용 회피
        // 헤지 carve-out — 정밀도의 선이 요구하는 인식적 헤지는 유지
        assert!(v.contains("추정") && v.contains("잠재"));
        // form-only 예시(어색함 → 자연스러움)
        assert!(v.contains("→"));
    }

    #[test]
    fn build_system_prompt_embeds_voice_guidance() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains(super::voice_guidance())); // 조각이 그대로 배선됨
    }

    #[test]
    fn system_prompt_has_humor_evidence_and_occasions_instructions() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("주인"));   // 호칭
        assert!(p.contains("유머"));   // 유머 지시
        assert!(p.contains("detail")); // 근거 필드 사용 지시
        assert!(p.contains("suggested_action")); // 개선방향 필드 사용 지시
        assert!(p.contains("occasions")); // 기념일/명절 사용 지시
    }

    #[test]
    fn system_prompt_directs_short_length_and_moderate_emoji() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("2~3문단"));   // 길이 상한(문단)
        assert!(p.contains("350자"));     // 길이 상한(글자)
        assert!(p.contains("골라"));      // 핵심만 골라 쓰기(장황함 차단)
        assert!(p.contains("이모지"));    // 이모지 지시
        assert!(p.contains("문단마다 1~2개")); // 사용량 상향(1개 정도 → 1~2개)
    }

    #[test]
    fn system_prompt_directs_context_signals_and_comfort() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("상시 이슈"));         // 이미 다룬 상시 이슈 제외 언급
        assert!(p.contains("tool_usage"));        // 도구 텍스처 지시
        assert!(p.contains("work_context"));      // 근무 맥락
        assert!(p.contains("위로"));              // 주말/공휴일/장시간 위로
        assert!(p.contains("쉬엄쉬엄"));          // long_work 챙김
    }

    #[test]
    fn system_prompt_directs_recent_diary_variety() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("recent_diaries")); // 최근 일기 참조 지시
        assert!(p.contains("되풀이하지"));      // 이미 다룬 화제 반복 금지
        assert!(p.contains("다른 이야기"));     // 어제와 다른 서사
    }

    #[test]
    fn finding_advice_r7_v2_project_aggregate() {
        let (detail, action) = super::finding_advice(
            "R7",
            &serde_json::json!({"ratio_pct": 75, "total_sessions": 3, "project_session_count": 4}),
            48320,
        );
        assert!(detail.contains("75"));
        assert!(detail.contains("3건"));
        assert!(action.contains("claude --model sonnet"));
    }

    #[test]
    fn finding_advice_r10_burst() {
        let (detail, action) = super::finding_advice(
            "R10",
            &serde_json::json!({
                "total_sessions": 81, "opus_session_count": 81,
                "temp_hit_ratio_pct": 90, "median_gap_secs": 120
            }),
            500000,
        );
        assert!(detail.contains("81"));
        assert!(detail.contains("전부")); // opus_n == n이면 "그중 81건이" 대신 "전부"
        assert!(detail.contains("temp")); // 가산 신호 서사 인용
        assert!(action.contains("--model haiku")); // 조치 메커니즘 명시 (플래그 미지정 → 기본 모델 상속)
        assert!(action.contains("상속"));
        assert!(action.contains("보여요")); // 가설 표현 — 단정 금지
    }

    #[test]
    fn finding_advice_r10_partial_opus_says_count() {
        let (detail, _) = super::finding_advice(
            "R10",
            &serde_json::json!({"total_sessions": 9, "opus_session_count": 7, "temp_hit_ratio_pct": 0}),
            1000,
        );
        assert!(detail.contains("그중 7건이"));
    }

    #[test]
    fn finding_advice_r10_omits_temp_when_zero() {
        let (detail, _) = super::finding_advice(
            "R10",
            &serde_json::json!({"total_sessions": 5, "opus_session_count": 5, "temp_hit_ratio_pct": 0}),
            1000,
        );
        assert!(!detail.contains("temp"));
    }

    #[test]
    fn finding_advice_r11_deterministic() {
        let (detail, action) = super::finding_advice(
            "R11",
            &serde_json::json!({
                "friction_events": [
                    {"tool": "Write", "target": "a.rs", "session_id": "s1"},
                    {"tool": "Write", "target": "b.rs", "session_id": "s2"}
                ],
                "friction_events_count": 2,
                "by_tool": {"Write": 2}
            }),
            0,
        );
        assert!(detail.contains("2건"));
        assert!(detail.contains("거부") && detail.contains("승인"));
        assert!(detail.contains("Write"));
        assert!(action.contains("허용목록"));
        assert!(!action.contains("일 수 있어요")); // 결정론 — 가설 표현 제거(스펙 §5)
    }

    #[test]
    fn finding_advice_r10_inserts_real_path_and_first_prompt() {
        let (detail, action) = super::finding_advice(
            "R10",
            &serde_json::json!({
                "total_sessions": 81, "opus_session_count": 81, "temp_hit_ratio_pct": 90,
                "rep_cwd": "D:\\Project\\cowork\\.worktrees\\probe",
                "rep_first_prompt": "이 리포의 최근 커밋 요약해줘"
            }),
            500000,
        );
        assert!(detail.contains("cowork")); // 진짜 경로(정규화 키 아님)
        assert!(action.contains("이런 요청으로 시작해요"));
        assert!(action.contains("최근 커밋 요약")); // 첫 요청 한 줄
    }

    #[test]
    fn finding_advice_r12_value_proposal() {
        let (detail, action) = super::finding_advice(
            "R12",
            &serde_json::json!({
                "total_sessions": 2,
                "recommended_skills": ["superpowers:writing-plans", "superpowers:subagent-driven-development"]
            }),
            0,
        );
        assert!(detail.contains("2건"));
        assert!(action.contains("writing-plans"));
    }

    #[test]
    fn finding_advice_r1_r2_cite_purpose_when_known() {
        let (d, _) = super::finding_advice("R1", &serde_json::json!({"server": "playwright"}), 2500);
        assert!(d.contains("브라우저 자동화")); // 용도 사전 인용
        let (d2, _) = super::finding_advice(
            "R2",
            &serde_json::json!({"plugin": "frontend-design@claude-plugins-official", "skill_count": 3}),
            900,
        );
        assert!(d2.contains("UI 디자인"));
        // 사전에 없으면 현행 문구 유지
        let (d3, _) = super::finding_advice("R1", &serde_json::json!({"server": "internal-x"}), 100);
        assert!(d3.contains("상주하는데 호출 0회"));
    }

    #[test]
    fn finding_advice_r9() {
        let (detail, action) = super::finding_advice(
            "R9",
            &serde_json::json!({"web_search":12,"web_fetch":6,"total_requests":18}),
            36000,
        );
        assert!(detail.contains("검색 12"));
        assert!(detail.contains("페치 6"));
        assert!(detail.contains("18"));
        assert!(action.contains("캐싱"));
    }

    #[test]
    fn finding_advice_r2() {
        let (detail, action) = super::finding_advice(
            "R2",
            &serde_json::json!({"plugin":"superpowers@mp","skill_count":12,"resident_tokens":900}),
            900,
        );
        assert!(detail.contains("superpowers@mp"));
        assert!(detail.contains("12"));
        assert!(detail.contains("900"));
        assert!(action.contains("비활성"));
    }

    #[test]
    fn finding_advice_default_arm() {
        let ev = serde_json::json!({"x": 1});
        let (detail, action) = super::finding_advice("RX", &ev, 0);
        assert_eq!(action, "");
        assert_eq!(detail, format!("{ev}"));
    }

    #[test]
    fn render_and_persist_split_matches_generate() {
        use crate::diary::engine::MockEngine;
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        let brief = assemble_brief(&store, "Windows", "2026-07-04", &cfg).unwrap();
        let engine = MockEngine { canned: "분리 테스트 일기".into() };
        let rendered = render_diary(&engine, &brief, &cfg).unwrap();
        assert!(rendered.body.contains("분리 테스트 일기"));
        let out = persist_diary(&store, &brief.date, &brief.host, &rendered, &cfg).unwrap();
        assert!(out.path.exists());
        assert_eq!(
            store.diary_path_for("2026-07-04").unwrap(),
            Some(out.path.to_string_lossy().to_string()),
        );
    }

    #[test]
    fn local_date_of_converts_utc_and_falls_back() {
        let expected = chrono::DateTime::parse_from_rfc3339("2026-07-01T23:30:00Z").unwrap()
            .with_timezone(&chrono::Local).date_naive();
        assert_eq!(super::local_date_of("2026-07-01T23:30:00Z"), Some(expected));
        // RFC3339 파싱 불가 → 앞 10자(YYYY-MM-DD) 폴백
        assert_eq!(
            super::local_date_of("2026-07-01(비표준)"),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 1)
        );
        assert_eq!(super::local_date_of("junk"), None);
    }
}
