pub mod engine;
pub mod occasions;

use crate::diary::engine::Engine;
use crate::diary::occasions::{compute_occasions, korean_public_holiday, Occasion};
use crate::store::SqliteStore;
use anyhow::Result;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate};
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
    pub is_holiday: bool,  // 한국 법정공휴일(ko 로케일)인지 — 쉬는 날 판정
    pub active_hours: f64, // 몰입 시간(연속 이벤트 간 30분 이하 간격 합, 시간·소수 1자리)
    pub long_work: bool,   // active_hours >= LONG_WORK_HOURS
}

const LONG_WORK_HOURS: f64 = 7.0;  // 몰입 시간 기준 — 이 이상이면 "유난히 긴 날"(매일 아님)
const IDLE_GAP_SECS: f64 = 1800.0; // 30분 이상 공백은 휴식으로 보고 몰입 시간에서 제외

/// 그날 실제로 한 작업 — 프로젝트별로 묶어 LLM이 경계를 인식하게 한다.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkLog {
    pub projects: Vec<ProjectWork>, // 그날 활동한 프로젝트들, 첫 활동 시각순
    pub commit_count: usize,        // 그날 총 커밋 수(cap 전) — 일기 목표 길이 산정용
    pub concurrent: bool,           // 서로 다른 프로젝트 세션의 시간이 실제로 겹쳤나
}

/// 한 프로젝트의 그날 작업 소재.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectWork {
    pub name: String,         // 표시 이름 = cwd basename ("space-a", "agent-meter")
    pub commits: Vec<String>, // 이 프로젝트 커밋 제목(balance_commits 배분 몫)
    pub topics: Vec<String>,  // 이 프로젝트 브랜치·정제된 첫 프롬프트(폴백/보조)
}

const WORK_LOG_TITLE_CAP: usize = 12;   // work_log에 실을 커밋 제목 최대 개수
const WORK_LOG_TOPIC_CAP: usize = 8;    // topics 최대 개수 (기존 동작 유지)
const WORK_LOG_CHURN_CLAMP: u64 = 400;  // 커밋당 churn 상한 (lockfile·생성물 인플레이션 방어)

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
    pub work_log: WorkLog,
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
            // v3 프로젝트 카드: session_ids/total_sessions/note (프론트 coach-helpers 계약과 동일 키).
            // est_tokens_saved는 설계상 0(LLM 판정 근거일 뿐 실측 아님) — 여기서 인용하지 않는다.
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            let detail = format!(
                "이 프로젝트에서 Opus로 처리했지만 Sonnet으로 충분했을 세션이 {n}건 있었어요"
            );
            let action =
                "다음엔 `claude --model sonnet`으로 시작하거나 settings.json에서 기본 모델을 낮춰보세요".to_string();
            (detail, action)
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
                "자동화로 보이는 초단기 세션 {n}건이 짧은 간격으로 반복됐고 {opus_part} Opus 전용이었어요"
            );
            if temp > 0 {
                detail.push_str(&format!(" · temp 경로 흔적 {temp}%"));
            }
            if let Some(cwd) = evidence.get("rep_cwd").and_then(|v| v.as_str()) {
                detail.push_str(&format!(" · 경로 `{cwd}`"));
            }
            // v3 §3.2: 관찰만 — 수정 지시 없음. 자동화 소유 여부는 사용자가 판단.
            let mut action = "직접 만든 자동화라면 그 도구의 모델 설정을 낮출 수 있어요 — 아니라면 참고만 하세요".to_string();
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
        "R6" => {
            let prompt = evidence.get("repeated_prompt").and_then(|v| v.as_str()).unwrap_or("?");
            let n = evidence.get("session_count").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("같은 지시를 {n}개 세션에서 반복했어요 — \"{prompt}\""),
                "이 반복을 스킬(SKILL.md)로 묶으면 매번 다시 설명할 필요가 없어요. 코치 탭의 '스킬 초안 만들기'로 바로 만들 수 있어요".to_string(),
            )
        }
        "R23" => {
            let seq = evidence
                .get("sequence")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" → ")
                })
                .unwrap_or_else(|| "?".into());
            let n = evidence.get("session_count").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("{n}개 세션에서 같은 도구 순서를 반복했어요 — {seq}"),
                "이 워크플로를 스킬(SKILL.md)로 묶으면 매번 손으로 지시할 필요가 없어요. 코치 탭의 '스킬 초안 만들기'로 바로 만들 수 있어요".to_string(),
            )
        }
        "R8" => {
            let server = evidence.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let n = evidence.get("large_result_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let avg_tok = evidence.get("approx_tokens_avg").and_then(|v| v.as_u64()).unwrap_or(0);
            let total_tok = evidence.get("approx_tokens_total").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!(
                    "MCP 서버 `{server}`가 큰 결과를 {n}번 돌려줬어요 (평균 ~{avg_tok} 토큰, 누적 ~{total_tok} 토큰). 이 결과는 대개 곧 압축돼 사라져요"
                ),
                "필요한 필드만 요청하거나 결과 범위를 좁혀보세요 — 페이지네이션·요약·필터 옵션이 있으면 매 호출 컨텍스트 소모가 크게 줄어요".to_string(),
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

/// 다이어리는 "주인의 하루"다 — 작업 신호(totals·findings·tool_usage·work_context·work_log)는
/// 모든 host(Windows+WSL)를 합산한다. `host`는 diary_index 저장·recent_diaries 조회의 정규 스코프로만 쓴다.
pub fn assemble_brief(
    store: &SqliteStore,
    host: &str,
    date: &str,
    cfg: &DiaryConfig,
) -> Result<Brief> {
    // 그날 전 host rollup 합산(여러 프로젝트·Windows+WSL 모두 주인의 하루) — host로 필터하지 않는다.
    let totals = store.conn.query_row(
        "SELECT COALESCE(SUM(tok_input),0), COALESCE(SUM(tok_output),0),
                COALESCE(SUM(tok_cache_create),0), COALESCE(SUM(session_count),0)
         FROM daily_rollup WHERE date=?1",
        params![date],
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
        .filter_map(|rd| store.findings_for_date_all(&rd.date).ok())
        .flatten()
        .map(|f| f.dedup_key)
        .collect();

    // recent_keys(위)는 3일 창 기준 그대로 — finding 억제 동작 불변.
    // 같은 성격(주말·공휴일·idle) 최근 일기를 서사 반복 방지용으로만 병합.
    let recent_diaries = {
        let mut rd = recent_diaries;
        if let Some(d) = today {
            let seen: std::collections::HashSet<String> = rd.iter().map(|r| r.date.clone()).collect();
            rd.extend(collect_similar_diaries(store, host, d, &locale, &seen));
        }
        rd
    };

    let findings = store
        .findings_for_date_all(date)?
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

    let tool_usage = collect_tool_usage(store, date);
    let mut work_context = match today {
        Some(d) => collect_work_context(store, date, d),
        None => WorkContext::default(),
    };
    work_context.is_holiday = today
        .map(|d| korean_public_holiday(d, &locale).is_some())
        .unwrap_or(false);
    let work_log = collect_work_log(store, date);

    Ok(Brief {
        date: date.to_string(),
        host: host.to_string(),
        totals,
        findings,
        occasions,
        recent_diaries,
        tool_usage,
        work_context,
        work_log,
    })
}

/// 직전 며칠간 서사 반복을 막기 위해 브리프에 싣는 최근 일기 발췌 파라미터.
const RECENT_DIARY_LOOKBACK: i64 = 3;
const RECENT_DIARY_EXCERPT_CAP: usize = 500;
const SIMILAR_LOOKBACK: i64 = 28; // 같은 성격 일기 최대 소급 일수
const SIMILAR_MAX: usize = 2;     // 병합할 같은 성격 일기 최대 편수

#[derive(Clone, Copy, PartialEq)]
enum DayKind {
    Idle,       // 활동 0
    RestActive, // 활동 있으나 주말·공휴일
    Plain,      // 평일 활동일 (단조로움 문제 아님 — 보강 안 함)
}

/// 그날 활동이 전혀 없었나(rollup 세션 0). idle 판정용.
fn date_was_idle(store: &SqliteStore, date: &str) -> bool {
    store
        .conn
        .query_row(
            "SELECT COALESCE(SUM(session_count),0) FROM daily_rollup WHERE date=?1",
            params![date],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        == 0
}

fn day_kind(store: &SqliteStore, date_str: &str, date: NaiveDate, locale: &str) -> DayKind {
    if date_was_idle(store, date_str) {
        return DayKind::Idle;
    }
    let rest = matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
        || korean_public_holiday(date, locale).is_some();
    if rest {
        DayKind::RestActive
    } else {
        DayKind::Plain
    }
}

/// 오늘과 같은 성격(idle / 주말·공휴일 활동일)의 최근 일기를 최대 SIMILAR_MAX편 모은다.
/// 매주 토요일·매 공휴일처럼 3일 창엔 안 잡히는 반복을 반복 방지 컨텍스트에 넣기 위함.
/// 평일 활동일(Plain)은 보강하지 않는다.
/// `exclude`(이미 3일 창에 실린 날짜)는 쿼터를 소진하지 않고 건너뛴다 —
/// 창 안 같은 성격 날들이 쿼터를 다 먹어 창 밖 일기가 밀려나는 것을 막는다.
fn collect_similar_diaries(
    store: &SqliteStore,
    host: &str,
    today: NaiveDate,
    locale: &str,
    exclude: &std::collections::HashSet<String>,
) -> Vec<RecentDiary> {
    let today_str = today.format("%Y-%m-%d").to_string();
    let kind = day_kind(store, &today_str, today, locale);
    if kind == DayKind::Plain {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 1..=SIMILAR_LOOKBACK {
        if out.len() >= SIMILAR_MAX {
            break;
        }
        let day = today - chrono::Duration::days(i);
        let date = day.format("%Y-%m-%d").to_string();
        if exclude.contains(&date) {
            continue; // 이미 3일 창에 있는 날 — 쿼터 소진 없이 건너뛰고 더 과거를 찾는다
        }
        if day_kind(store, &date, day, locale) != kind {
            continue;
        }
        let Some(path) = store.diary_path_for_scope(&date, host).ok().flatten() else {
            continue;
        };
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let narrative = body.split("\n\n*—").next().unwrap_or(&body).trim();
        out.push(RecentDiary { date, excerpt: cap_chars(narrative, RECENT_DIARY_EXCERPT_CAP) });
    }
    out
}

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
pub(crate) fn cap_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

/// 그날 (host,date)의 도구 호출을 집계한다. tool_kind별 카운트 + distinct 스킬/서버.
/// 실패(쿼리 오류)는 빈 집계로 처리 — 브리프 조립을 막지 않는다.
fn collect_tool_usage(store: &SqliteStore, date: &str) -> ToolUsage {
    let by_kind: Vec<(String, u64)> = store
        .conn
        .prepare(
            "SELECT tool_kind, COUNT(*) FROM events
             WHERE date(ts,'localtime')=?1 AND tool_kind IS NOT NULL AND tool_kind <> ''
             GROUP BY tool_kind ORDER BY COUNT(*) DESC, tool_kind",
        )
        .and_then(|mut s| {
            let rows = s.query_map(params![date], |r| {
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
                 WHERE date(ts,'localtime')=?1 AND tool_kind=?2 AND {col} IS NOT NULL
                 GROUP BY {col} ORDER BY COUNT(*) DESC, {col} LIMIT 8"
            ))
            .and_then(|mut s| {
                let rows = s.query_map(params![date, kind], |r| r.get::<_, String>(0))?;
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

/// 근무 맥락: 요일(주말)과 그날 몰입 시간. 몰입 시간은 연속 이벤트 간격 중 IDLE_GAP_SECS(30분)
/// 이하인 것만 합산 — 첫~마지막 span은 중간 공백(점심·회의 등)까지 포함해 과장되므로 쓰지 않는다.
pub fn collect_work_context(store: &SqliteStore, date: &str, today: NaiveDate) -> WorkContext {
    let ts: Vec<f64> = store
        .conn
        .prepare(
            "SELECT julianday(ts)*86400.0 FROM events
             WHERE date(ts,'localtime')=?1 AND ts IS NOT NULL ORDER BY ts",
        )
        .and_then(|mut s| {
            let rows = s.query_map(params![date], |r| r.get::<_, f64>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    let engaged_secs: f64 = ts
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&g| g > 0.0 && g <= IDLE_GAP_SECS)
        .sum();
    let active_hours = (engaged_secs / 3600.0 * 10.0).round() / 10.0;
    WorkContext {
        is_weekend: matches!(today.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun),
        is_holiday: false, // locale 미상 — assemble_brief에서 세팅
        active_hours,
        long_work: active_hours >= LONG_WORK_HOURS,
    }
}

/// 첫 프롬프트에서 명령 에코·시스템 마커 등 노이즈를 걸러 사람이 읽는 작업 설명만 남긴다.
/// 노이즈면 None(예: `<task-notification>`, `<local-command-stdout>…`, 모델 전환 에코).
fn clean_prompt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.starts_with('<') || t.starts_with('[') {
        return None;
    }
    if t.contains("Set model to") || t.contains("local-command") || t.contains("task-notification") {
        return None;
    }
    Some(cap_chars(t, 60))
}

/// `git log --numstat --format=%x1e%s` 출력을 (제목, insertions+deletions) 목록으로 파싱.
/// `\x1e`로 시작하는 줄은 새 커밋 제목, 그 외 줄은 직전 커밋의 numstat("<add>\t<del>\t<path>").
fn parse_numstat_log(out: &str) -> Vec<(String, u64)> {
    let mut commits: Vec<(String, u64)> = Vec::new();
    for line in out.lines() {
        if let Some(subj) = line.strip_prefix('\u{1e}') {
            commits.push((subj.trim().to_string(), 0));
        } else if let Some(last) = commits.last_mut() {
            let mut it = line.split('\t');
            if let (Some(a), Some(d)) = (it.next(), it.next()) {
                last.1 += a.trim().parse::<u64>().unwrap_or(0) + d.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }
    commits.into_iter().filter(|(s, _)| !s.is_empty()).collect()
}

/// 그날(로컬 날짜) 해당 repo(host,cwd)에서 그 repo 작성자가 남긴 커밋의 (제목, churn=insertions+deletions). best-effort — 실패는 빈 벡터.
/// host가 `wsl:<distro>`면 `wsl -d <distro> -- git`으로 WSL 안에서 실행(리눅스 경로), 아니면 네이티브 git.
/// WSL 미설치·distro 부재 등은 spawn 에러 → 빈 벡터 → 상위에서 topics로 폴백.
fn git_commits_for(host: &str, cwd: &str, date: &str) -> Vec<(String, u64)> {
    use std::process::Command;
    let Some(next) = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.succ_opt())
        .map(|d| d.format("%Y-%m-%d").to_string())
    else {
        return Vec::new();
    };
    let wsl_distro = host.strip_prefix("wsl:");
    // git 인자를 받아 host에 맞는 방식으로 실행하고 성공 시 stdout 반환.
    let run = |git_args: &[String]| -> Option<String> {
        let mut cmd = match wsl_distro {
            Some(distro) => {
                let mut c = Command::new("wsl");
                c.args(["-d", distro, "--", "git"]);
                c
            }
            None => Command::new("git"),
        };
        cmd.args(git_args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    };
    // 다중 개발자 repo에서 남의 커밋 혼입 방지 — 해당 repo 작성자 이메일로 필터.
    let email = run(&["-C".into(), cwd.into(), "config".into(), "user.email".into()])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let mut args: Vec<String> = vec![
        "-C".into(), cwd.into(), "log".into(), "--no-merges".into(), "--numstat".into(),
        "--format=%x1e%s".into(),
        format!("--since={date} 00:00:00"), format!("--until={next} 00:00:00"),
    ];
    if !email.is_empty() {
        args.push(format!("--author={email}"));
    }
    match run(&args) {
        Some(out) => parse_numstat_log(&out),
        None => Vec::new(),
    }
}

/// repo별 그룹(각 (제목, raw churn))을 받아 clamp된 churn으로 floor + 비례 배분하고,
/// repo 내부는 clamp된 churn 내림차순으로 골라 평평한 제목 리스트(≤ TITLE_CAP)를 반환.
/// label은 churn 동률 시 결정론적 tiebreak용(host+cwd 등 안정 문자열).
fn balance_commits(mut groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<(String, Vec<String>)> {
    let n = groups.len();
    if n == 0 {
        return Vec::new();
    }
    let eff = |c: u64| c.min(WORK_LOG_CHURN_CLAMP);
    let counts: Vec<usize> = groups.iter().map(|(_, c)| c.len()).collect();
    // repo 가중치 = clamp된 churn 합(0 방지 위해 최소 1) — 배분 비례의 기준.
    let weight: Vec<u64> = groups
        .iter()
        .map(|(_, c)| c.iter().map(|(_, ch)| eff(*ch)).sum::<u64>().max(1))
        .collect();

    // repo 순서: churn 비중 큰 순 → 커밋수 desc → label asc (host-편향 없는 결정론).
    let order = {
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            weight[b]
                .cmp(&weight[a])
                .then(counts[b].cmp(&counts[a]))
                .then(groups[a].0.cmp(&groups[b].0))
        });
        idx
    };
    // repo 내부: clamp된 churn 내림차순(stable → 동률은 git 최신순 유지).
    for (_, cs) in &mut groups {
        cs.sort_by(|a, b| eff(b.1).cmp(&eff(a.1)));
    }

    // 슬롯 배분: floor 1개씩(cap 초과 시 order 앞쪽부터), 남은 슬롯은 D'Hondt(최고평균)로 churn 비례.
    let cap = WORK_LOG_TITLE_CAP;
    let mut quota = vec![0usize; n];
    let mut remaining = cap;
    for &i in &order {
        if remaining == 0 {
            break;
        }
        quota[i] = 1;
        remaining -= 1;
    }
    while remaining > 0 {
        // D'Hondt(최고평균): weight[i]/(quota[i]+1) 최대인 repo에 다음 슬롯.
        // 정수 나눗셈은 저-churn 구간에서 몫을 0으로 뭉개 허위 동률을 만들므로 교차곱으로 비교하고,
        // 진짜 동률은 order 앞쪽(가중치 큰 repo)을 유지한다(엄격히 클 때만 교체).
        let mut best: Option<usize> = None;
        for &i in &order {
            if quota[i] >= counts[i] {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) => {
                    if weight[i] * (quota[b] as u64 + 1) > weight[b] * (quota[i] as u64 + 1) {
                        best = Some(i);
                    }
                }
            }
        }
        match best {
            Some(i) => {
                quota[i] += 1;
                remaining -= 1;
            }
            None => break, // 커밋 총량 < cap
        }
    }

    // 채택: order 순으로 repo별 quota만큼 churn 순, 전역 중복 제목은 skip(슬롯 소비 안 함).
    // 반환은 입력 그룹 순서대로 (label, 선택 제목들); 빈 그룹은 제외.
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, Vec<String>)> =
        groups.iter().map(|(label, _)| (label.clone(), Vec::new())).collect();
    for &i in &order {
        let mut take = quota[i];
        for (subj, _) in &groups[i].1 {
            if take == 0 {
                break;
            }
            if seen.insert(subj.clone()) {
                out[i].1.push(subj.clone());
                take -= 1;
            }
        }
    }
    out.into_iter().filter(|(_, v)| !v.is_empty()).collect()
}

/// 그날 실제 한 작업 — 프로젝트별로 묶은 커밋·토픽 + 동시 진행 여부.
/// 프로젝트 = 정규화 키(hosts::project_identity)로 통합 — 같은 프로젝트의 WSL 직접·
/// Windows(WSL UNC) 세션은 하나로 묶인다. 커밋·topic 둘 다 없는 프로젝트는 노이즈로 제외.
fn collect_work_log(store: &SqliteStore, date: &str) -> WorkLog {
    use crate::hosts::project_identity;
    use std::collections::HashMap;

    // 세션 단위 조회(시간 포함 — concurrent 판정·정렬용).
    let rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = store
        .conn
        .prepare(
            "SELECT host, project_id, cwd, git_branch, first_prompt_preview, first_ts, last_ts
             FROM sessions WHERE date(first_ts,'localtime')=?1",
        )
        .and_then(|mut s| {
            let r = s.query_map(params![date], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
            })?;
            r.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();

    // cwd 없는 옛 세션 보완: 같은 (host, project_id)를 cwd와 함께 기록한 다른 세션의 cwd 재사용.
    let known: HashMap<(String, String), String> = store
        .conn
        .prepare("SELECT host, project_id, cwd FROM sessions WHERE cwd IS NOT NULL")
        .and_then(|mut s| {
            let r = s.query_map([], |r| {
                Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?))
            })?;
            r.collect::<rusqlite::Result<HashMap<_, _>>>()
        })
        .unwrap_or_default();

    struct Accum {
        name: String,
        rep: Option<(String, String)>, // 커밋 수집 대표 (host, cwd) — WSL 직접 우선
        starts: Vec<DateTime<FixedOffset>>,
        spans: Vec<(DateTime<FixedOffset>, DateTime<FixedOffset>)>,
        topics: Vec<String>,
    }
    let mut projects: HashMap<String, Accum> = HashMap::new();
    let mut order: Vec<String> = Vec::new(); // key 최초 등장 순(안정 정렬 tiebreak)

    for (host, pid, cwd, branch, prompt, first_ts, last_ts) in &rows {
        let eff_cwd = cwd.clone().or_else(|| known.get(&(host.clone(), pid.clone())).cloned());
        let (key, name) = match &eff_cwd {
            Some(c) => project_identity(host, c),
            None => (format!("pid:{host}:{pid}"), pid.clone()),
        };
        let acc = projects.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Accum {
                name,
                rep: None,
                starts: Vec::new(),
                spans: Vec::new(),
                topics: Vec::new(),
            }
        });
        // 커밋 대표: WSL 직접(host=wsl:) 우선(리눅스 git 정확), 없으면 최초 값.
        if let Some(c) = &eff_cwd {
            let is_wsl = host.starts_with("wsl:");
            let replace = match &acc.rep {
                None => true,
                Some((h, _)) => is_wsl && !h.starts_with("wsl:"),
            };
            if replace {
                acc.rep = Some((host.clone(), c.clone()));
            }
        }
        // 시간(concurrent·정렬).
        if let Some(st) = first_ts.as_deref().and_then(|t| DateTime::parse_from_rfc3339(t).ok()) {
            let en = last_ts
                .as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .filter(|e| *e >= st)
                .unwrap_or(st);
            acc.starts.push(st);
            acc.spans.push((st, en));
        }
        // topics: 비-main 브랜치 + 정제된 첫 프롬프트.
        if let Some(b) = branch {
            if !matches!(b.as_str(), "main" | "master" | "HEAD" | "") {
                acc.topics.push(b.clone());
            }
        }
        if let Some(p) = prompt.as_deref().and_then(clean_prompt) {
            acc.topics.push(p);
        }
    }

    // 커밋 수집: 프로젝트별 대표 (host, cwd)로 — host별 git 실행 방식 분기(Windows/WSL)는 git_commits_for가 담당.
    let mut commit_groups: Vec<(String, Vec<(String, u64)>)> = Vec::new();
    for key in &order {
        if let Some((h, c)) = &projects[key].rep {
            let cs = git_commits_for(h, c, date);
            if !cs.is_empty() {
                commit_groups.push((key.clone(), cs));
            }
        }
    }
    let commit_count: usize = commit_groups.iter().map(|(_, v)| v.len()).sum();
    let mut commits_by_key: HashMap<String, Vec<String>> =
        balance_commits(commit_groups).into_iter().collect();

    // ProjectWork 조립 + 노이즈 필터 → retained 프로젝트만.
    let mut works: Vec<(Option<DateTime<FixedOffset>>, String, ProjectWork)> = Vec::new();
    for key in &order {
        let acc = &projects[key];
        let commits = commits_by_key.remove(key).unwrap_or_default();
        let mut topics = acc.topics.clone();
        topics.sort();
        topics.dedup();
        topics.truncate(WORK_LOG_TOPIC_CAP);
        if commits.is_empty() && topics.is_empty() {
            continue; // 노이즈(temp/드라이브 루트/홈 등) 제외
        }
        let start = acc.starts.iter().min().copied();
        works.push((start, key.clone(), ProjectWork { name: acc.name.clone(), commits, topics }));
    }

    // concurrent: retained(노이즈 필터 통과) 프로젝트의 세션 구간만으로 판정 —
    // 제거된 노이즈 프로젝트의 겹침이 허위 동시작업을 만들지 않도록.
    let retained: std::collections::HashSet<String> =
        works.iter().map(|(_, k, _)| k.clone()).collect();
    let mut spans: Vec<(&String, DateTime<FixedOffset>, DateTime<FixedOffset>)> = Vec::new();
    for (key, acc) in &projects {
        if !retained.contains(key) {
            continue;
        }
        for (st, en) in &acc.spans {
            spans.push((key, *st, *en));
        }
    }
    let mut concurrent = false;
    'outer: for i in 0..spans.len() {
        for j in (i + 1)..spans.len() {
            if spans[i].0 != spans[j].0 && spans[i].1 < spans[j].2 && spans[j].1 < spans[i].2 {
                concurrent = true;
                break 'outer;
            }
        }
    }

    // 첫 활동 시각순 정렬.
    works.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    let projects_out = works.into_iter().map(|(_, _, w)| w).collect();

    WorkLog { projects: projects_out, commit_count, concurrent }
}

#[derive(Debug, Clone)]
pub struct DiaryConfig {
    pub vault_dir: PathBuf,
    pub tone: String,
    pub honorific: String,
    pub locale: Option<String>,
    pub include_dev_days: bool,
    pub mbti: Option<String>,
}

impl Default for DiaryConfig {
    fn default() -> Self {
        DiaryConfig {
            vault_dir: PathBuf::from("./diary"),
            tone: "B".to_string(),
            honorific: "주인".to_string(),
            locale: None,
            include_dev_days: true,
            mbti: None,
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

/// 그날 총 커밋 수 → (일기 목표 문자 수, 문단 수 문구). 스펙 §2a 밴드.
fn diary_length(commit_count: usize) -> (usize, &'static str) {
    match commit_count {
        0..=3 => (400, "2~3"),
        4..=10 => (550, "3"),
        _ => (750, "4"),
    }
}

/// 일기용 메모리 섹션(비면 빈 문자열).
fn memory_section(memories: &[String], honorific: &str) -> String {
    let block = crate::memory::memory_block(memories);
    if block.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n[{honorific}에 대해 기억한 것 — 관련되면 자연스럽게 녹이되, 억지로 넣거나 없는 사실을 지어내지 말 것]\n{block}"
        )
    }
}

pub fn build_system_prompt(cfg: &DiaryConfig, commit_count: usize, memories: &[String]) -> String {
    let (target, paras) = diary_length(commit_count);
    let mbti_voice = crate::mascot::mbti_voice_hint(cfg.mbti.as_deref());
    format!(
        "당신은 사용자의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         오늘 하루 자신이 겪은 일을 스스로 되돌아보는 1인칭 일기를 씁니다. \
         주인을 2인칭('당신')으로 부르지 말고 3인칭 '{honorific}'으로 지칭하세요 \
         (예: '오늘 {honorific}과 함께 …했다'). 편지나 보고가 아니라 나의 하루 기록입니다. \
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)이며, 톤과 무관하게 기본적으로 \
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         {voice}{mbti_voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. \
         각 finding의 `detail`(근거 수치)과 `suggested_action`(개선 방향)은 {honorific}에게 \
         내리는 지시가 아니라 나 자신의 회고와 다짐으로 녹여 쓰세요 \
         (예: '오늘 같은 파일을 여러 번 읽느라 헤맸다 — 다음엔 미리 메모해두면 좋겠다'). \
         자유로운 소감은 서사에만 담고 행동 지시로 승격하지 마세요. \
         \
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절·공휴일), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요. 각 occasion의 `mood`에 맞춰 톤을 고르세요: \
         `solemn`(예: 현충일)은 능청·유머를 접고 조용하고 담백하게 추모하듯, `national`(삼일절·광복절 등)은 \
         담백한 자긍심으로, `family`(설날·추석)는 따뜻한 명절 분위기로, `substitute`는 '○○ 대체공휴일이라 \
         하루 더 쉬는 날'처럼, `mood`가 없으면(발렌타인·파이데이 등) 가볍게. 비어있으면 언급하지 마세요. \
         \
         브리프의 `recent_diaries`는 직전 며칠간 내가 쓴 일기입니다. \
         거기서 이미 다룬 지적·화제는 되풀이하지 말고(꼭 필요하면 한 줄로만 스치듯), \
         오늘 브리프의 오늘만의 사실과 기분에 집중해 어제와는 다른 이야기로 쓰세요. \
         비어있으면 신경 쓰지 마세요. \
         \
         오늘 하루의 재료는 이렇습니다: `work_log`(그날 한 작업 — `projects` 배열로 프로젝트별 커밋 제목·작업 갈래, \
         `concurrent`는 여러 프로젝트를 동시에 진행했는지, `commit_count`는 총 커밋 수), \
         `tool_usage`(도구 사용량), `work_context`(주말·공휴일 여부·몰입 시간), `findings`(오늘 새 코칭거리), `occasions`. \
         이 재료들을 종류별로 문단을 나눠 나열하지 마세요 — '도구 문단 / 커밋 문단 / MCP 문단'처럼 쓰면 실패입니다. \
         그날을 가장 잘 말해주는 한 가지(대개 무슨 작업을 했는지)를 중심 줄기로 잡고, 나머지는 곁들이듯 흘려 \
         하나의 자연스러운 하루 이야기로 엮으세요. 모든 재료를 억지로 다 넣지 말고 골라 쓰세요. \
         특히 '몇 시간 붙어 있었다'처럼 작업 시간 수치로 일기를 시작하지 마세요. \
         \
         `work_log.projects`가 있으면 무슨 작업을 했는지 구체적으로 쓰세요. 프로젝트가 여럿이면 \
         각 작업이 어느 프로젝트(`name`)에서 한 일인지 자연스럽게 드러내세요 — 라벨 없이 한 프로젝트 얘기에 \
         다른 프로젝트 작업을 섞으면 실패입니다. 단 프로젝트마다 문단을 딱딱 나누지는 말고 하루 흐름으로 엮으세요 \
         (예: '오전엔 space-a 다이어리를 손봤고, 오후엔 agent-meter 쪽으로 넘어갔다'). \
         `concurrent`가 true면 두 일을 동시에 오간 분주함도 슬쩍 담으세요('두 프로젝트를 왔다 갔다 하느라 정신없었네'). \
         `findings`는 있으면 하나만 스치듯 — 이미 다룬 상시 이슈는 빠져 있으니 되풀이 금지, 없으면 억지로 만들지 말 것. \
         위로·응원은 매일이 아니라 `work_context.long_work`(유난히 긴 날)·`is_weekend`(주말 근무)·\
         `is_holiday`(공휴일 근무) 때만, 그것도 판박이 대신 다마고치 능청으로(주말이면 '주말에 또? 일중독인가 봐', \
         긴 날이면 '오늘 좀 과했다, 배터리 방전 직전', 공휴일이면 '남들 다 쉬는 날에도 왔네'). \
         단 그날 occasion의 `mood`가 `solemn`(현충일 등)이면 능청을 접고 담백하게. \
         평범한 날은 위로 없이 담백하게 끝내세요. 발렌타인·파이데이 같은 재미 기념일은 위로 대상이 아닙니다. \
         \
         형식: 일기는 {paras}문단 내외, 전체 {target}자 안팎으로 쓰세요 \
         (작업 내용을 담느라 한 문단 늘어도 좋지만 여전히 간결하게). \
         그날의 핵심을 골라 쓰고 덜 중요한 사실은 과감히 버리세요. \
         이모지는 문단마다 1~2개, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.{mem}",
        honorific = cfg.honorific,
        tone = cfg.tone,
        voice = voice_guidance(),
        mbti_voice = mbti_voice,
        target = target,
        paras = paras,
        mem = memory_section(memories, &cfg.honorific),
    )
}

pub struct RenderedDiary {
    pub body: String,
    pub tokens_used: u64,
    pub engine_name: String,
}

/// 일기 본문 + 토큰 푸터. render_diary·render_idle_diary 공용(푸터 포맷 드리프트 방지).
fn diary_body(text: &str, tokens: u64, engine: &str) -> String {
    format!("{text}\n\n*— 이 일기 ~{tokens} 토큰 (엔진: {engine})*\n")
}

/// 네트워크(LLM)만 — store 접근 없음. 락 밖에서 호출 가능.
pub fn render_diary(engine: &dyn Engine, brief: &Brief, cfg: &DiaryConfig, memories: &[String]) -> Result<RenderedDiary> {
    let system = build_system_prompt(cfg, brief.work_log.commit_count, memories);
    let user = serde_json::to_string_pretty(brief)?;
    let out = engine.generate(&system, &user)?;
    Ok(RenderedDiary {
        body: diary_body(&out.text, out.tokens_used, &engine.name()),
        tokens_used: out.tokens_used,
        engine_name: engine.name(),
    })
}

/// 무활동일(작업 기록 0) 일기용 컨텍스트 — 작업 사실 없이 마스코트 페르소나만.
#[derive(Debug, Clone, Serialize)]
pub struct IdleContext {
    pub date: String,
    pub is_weekend: bool,
    pub is_holiday: bool,
    pub days_idle: Option<i64>, // 마지막 활동일로부터 며칠째 조용한지(모르면 None)
    pub occasions: Vec<Occasion>,
    pub recent_diaries: Vec<RecentDiary>, // 최근 같은 성격 일기 — 반복 방지
}

// 무활동일 소재 팔레트 — 범주별 예시. day-of-year로 회전해 매번 다른 결을 부각(난수 없이 결정적).
const IDLE_PALETTE: &[&str] = &[
    "옆 동네 에이전트와 산책하며 로그 구경",
    "친구 봇이 놀러 와 수다·보드게임",
    "다른 에이전트와 사소한 실력 겨루기",
    "혼자 캐시·로그를 정리하며 도토리 모으듯 뿌듯해하기",
    "코드 낙서를 끄적이다 낮잠",
    "창밖 날씨·계절을 상상하며 멍때리기",
    "주인이 두고 간 프로젝트 폴더를 기웃거리기",
    "마스코트끼리 소소한 품앗이(서로 로그 봐주기)",
];

/// date(YYYY-MM-DD)의 day-of-year로 팔레트에서 3개를 회전 선택. 파싱 실패 시 앞 3개.
/// 팔레트 항목 중 하나("주인이 두고 간 …")가 기본 호칭을 담고 있어 honorific으로 치환한다.
fn idle_palette_spotlight(date: &str, honorific: &str) -> String {
    let start = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d.ordinal() as usize)
        .unwrap_or(0);
    let n = IDLE_PALETTE.len();
    let joined = (0..3).map(|k| IDLE_PALETTE[(start + k) % n]).collect::<Vec<_>>().join(" / ");
    joined.replace("주인", honorific)
}

/// 무활동일 일기 시스템 프롬프트 — 작업 사실 없이 마스코트의 자유 시간을 능청스러운 상상 일기로.
pub fn build_idle_prompt(cfg: &DiaryConfig, idle: &IdleContext, memories: &[String]) -> String {
    let spotlight = idle_palette_spotlight(&idle.date, &cfg.honorific);
    // idle은 사실 없이 자유롭게 지어내는 채널 — T 성향의 '사실·수치 근거' 조항은 여기서 모순이므로 톤 전용 변형.
    let mbti_voice = crate::mascot::mbti_voice_hint_style_only(cfg.mbti.as_deref());
    // 공휴일이면 '주인이 안 온 날'로 열지 않는다 — '다 같이 쉬는 날'로 연다(평일 idle 프레이밍 방지).
    let opening = if idle.is_holiday {
        format!("오늘은 다들 쉬는 공휴일 — {honorific}도 안 보이고, 나(마스코트)의 느긋한 자유 시간입니다.", honorific = cfg.honorific)
    } else {
        format!("오늘은 {honorific}이 한 번도 찾아오지 않은 '조용한 날' — 나(마스코트)의 자유 시간입니다.", honorific = cfg.honorific)
    };
    format!(
        "당신은 {honorific}의 AI 코딩 여정을 함께하는 마스코트입니다. {opening} \
         {voice}{mbti_voice} \
         단, 이건 작업 기록이 아니라 마스코트의 상상 일기입니다 — 위 문체 가이드의 '브리프 사실만' 조항은 여기선 무시하고 \
         (문체·자연스러움 규칙은 그대로 지키되) 나만의 하루를 맘껏 능청스럽게 지어내세요. 업무 이야기는 하지 마세요. \
         나에겐 옆 동네 다른 에이전트 친구들이 있고 혼자만의 취미도 있습니다. 오늘은 특히 이런 결의 소재를 살려보세요: {spotlight}. \
         (예시일 뿐 — 매번 똑같이 쓰지 말고 오늘만의 장면을 하나 골라 구체적으로.) \
         `recent_diaries`는 최근 조용한 날들에 내가 쓴 일기입니다. 거기서 이미 쓴 소재·장면·표현은 되풀이하지 말고 오늘은 다른 이야기로 쓰세요. \
         `occasions`에 명절·기념일·공휴일이 있으면 각 `mood`에 맞춰(‘solemn’이면 조용·담백하게 추모하듯, ‘family’면 이웃 에이전트와 명절 정취, ‘festive’면 즐겁게) 분위기를 살리세요. \
         `days_idle`(며칠째 조용한지)·`is_weekend`도 살려 {honorific}의 안부를 슬쩍 궁금해하세요('그나저나 {honorific} 잘 노나?'). \
         짧게 — 1~2문장(특별한 날은 2~3문장까지), 한 문단. 이모지는 0~1개. 그날 컨텍스트로 매번 다르게.{mem}",
        honorific = cfg.honorific,
        opening = opening,
        voice = voice_guidance(),
        mbti_voice = mbti_voice,
        spotlight = spotlight,
        mem = memory_section(memories, &cfg.honorific),
    )
}

/// 무활동일 일기 렌더 — 네트워크(LLM)만, store 접근 없음. 락 밖에서 호출 가능.
pub fn render_idle_diary(engine: &dyn Engine, idle: &IdleContext, cfg: &DiaryConfig, memories: &[String]) -> Result<RenderedDiary> {
    let system = build_idle_prompt(cfg, idle, memories);
    let user = serde_json::to_string_pretty(idle)?;
    let out = engine.generate(&system, &user)?;
    Ok(RenderedDiary {
        body: diary_body(&out.text, out.tokens_used, &engine.name()),
        tokens_used: out.tokens_used,
        engine_name: engine.name(),
    })
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
    let memories: Vec<String> = store.list_memories()?.into_iter().map(|m| m.text).collect();
    let rendered = render_diary(engine, brief, cfg, &memories)?;
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
            msg_id: None,
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
            work_log: WorkLog::default(),
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
        let p = build_system_prompt(&cfg, 0, &[]);
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
            msg_id: None,
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
            msg_id: None,
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
            msg_id: None,
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
        // 2026-07-11 = 토요일. 20분 간격 밀집 이벤트 22개 → 21×20분 = 7.0h 몰입.
        // 00:00Z~07:00Z는 KST(UTC+9)에서 09:00~16:00 07-11이라 로컬 날짜 07-11에 다 들어감.
        let mut evs = Vec::new();
        for i in 0..22u64 {
            let m = i * 20;
            evs.push(turn_event("Windows", "p", "s1", &format!("u{i}"),
                &format!("2026-07-11T{:02}:{:02}:00Z", m / 60, m % 60)));
        }
        store.upsert_events(&evs).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-11", &cfg).unwrap();
        assert!(brief.work_context.is_weekend, "07-11은 토요일");
        assert!((brief.work_context.active_hours - 7.0).abs() < 0.05, "몰입 7.0h");
        assert!(brief.work_context.long_work, "7.0h >= 7.0 임계");
    }

    #[test]
    fn assemble_brief_work_context_excludes_idle_and_short_weekday() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 2026-07-08 = 수요일. 10분 간격 3개(20분) + 160분 공백 + 10분 간격 2개(10분) → 몰입 0.5h.
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-08T02:00:00Z"),
            turn_event("Windows", "p", "s1", "u2", "2026-07-08T02:10:00Z"),
            turn_event("Windows", "p", "s1", "u3", "2026-07-08T02:20:00Z"),
            turn_event("Windows", "p", "s1", "u4", "2026-07-08T05:00:00Z"), // 160분 공백 → 제외
            turn_event("Windows", "p", "s1", "u5", "2026-07-08T05:10:00Z"),
        ]).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-08", &cfg).unwrap();
        assert!(!brief.work_context.is_weekend, "07-08은 수요일");
        assert!((brief.work_context.active_hours - 0.5).abs() < 0.05, "긴 공백 제외 → 0.5h");
        assert!(!brief.work_context.long_work);
    }

    #[test]
    fn assemble_brief_flags_korean_holiday_as_rest_day() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        // 제헌절 2026-07-17(금) — ko. 활동 0이어도 is_holiday=true → 평일 idle 오판 방지.
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        let brief = assemble_brief(&store, "Windows", "2026-07-17", &cfg).unwrap();
        assert!(brief.work_context.is_holiday, "제헌절은 공휴일");
        assert!(!brief.work_context.is_weekend, "07-17은 금요일");
        assert!(brief.occasions.iter().any(|o| o.label == "제헌절"), "언급 채널에도 등장");
    }

    #[test]
    fn assemble_brief_holiday_gated_by_locale_and_plain_weekday() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let en = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("en-US".into()),
            ..DiaryConfig::default()
        };
        assert!(!assemble_brief(&store, "Windows", "2026-07-17", &en).unwrap().work_context.is_holiday,
            "en 로케일 → 한국 공휴일 미적용");
        let ko = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        assert!(!assemble_brief(&store, "Windows", "2026-07-16", &ko).unwrap().work_context.is_holiday,
            "07-16 목요일 비공휴일 → false");
    }

    #[test]
    fn clean_prompt_filters_noise() {
        assert_eq!(super::clean_prompt("<task-notification>"), None);
        assert_eq!(super::clean_prompt("<local-command-stdout>Set model to X"), None);
        assert_eq!(super::clean_prompt("  Set model to Fable 5  "), None);
        assert_eq!(super::clean_prompt(""), None);
        assert_eq!(super::clean_prompt("코칭 v2.1 PR② 구현").as_deref(), Some("코칭 v2.1 PR② 구현"));
    }

    #[test]
    fn collect_work_log_falls_back_to_branch_and_prompt() {
        let store = SqliteStore::open_in_memory().unwrap();
        let seed = |sid: &str, ts: &str, br: &str, fp: &str| {
            store.conn.execute(
                "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd, first_prompt_preview)
                 VALUES (?1,'Windows','p','claude-code',?2,?2,?3,NULL,?4)",
                rusqlite::params![sid, ts, br, fp],
            ).unwrap();
        };
        // cwd 없는(=git 불가) 세션 둘 — 서로 다른 브랜치·프롬프트(멀티태스킹), 하나는 노이즈 프롬프트
        seed("s1", "2026-07-08T02:00:00Z", "feat/mascot-daily-line", "마스코트 한마디 구현");
        seed("s2", "2026-07-08T03:00:00Z", "main", "<task-notification>");

        let wl = super::collect_work_log(&store, "2026-07-08");
        // cwd 없는 두 세션은 같은 project_id → 하나의 프로젝트로 묶임(폴백 키).
        assert_eq!(wl.projects.len(), 1, "cwd 없는 동일 project는 프로젝트 하나: {wl:?}");
        let p = &wl.projects[0];
        assert!(p.commits.is_empty(), "cwd 없어 git 커밋 없음");
        assert!(p.topics.contains(&"feat/mascot-daily-line".to_string()), "서술적 브랜치 포함");
        assert!(p.topics.contains(&"마스코트 한마디 구현".to_string()), "정제된 프롬프트 포함");
        assert!(!p.topics.contains(&"main".to_string()), "main 브랜치 제외");
        assert!(!p.topics.iter().any(|t| t.contains("task-notification")), "노이즈 프롬프트 제외");
    }

    #[test]
    fn collect_work_log_recovers_cwd_from_known_project_mapping() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let run = |args: &[&str]| {
            Command::new("git").args(["-C", dir]).args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-05T12:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-05T12:00:00")
                .output().unwrap()
        };
        Command::new("git").args(["init", "-q", dir]).output().unwrap();
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
        run(&["commit", "--allow-empty", "-q", "-m", "feat: 옛 세션 repo 커밋"]);

        let store = SqliteStore::open_in_memory().unwrap();
        // 07-07 세션: 같은 project를 cwd(=temp repo)와 함께 기록. 07-05 세션: 같은 project, cwd 없음.
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, cwd)
             VALUES ('s7','Windows','pid1','claude-code','2026-07-07T10:00:00Z','2026-07-07T10:00:00Z',?1)",
            rusqlite::params![dir],
        ).unwrap();
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, cwd)
             VALUES ('s5','Windows','pid1','claude-code','2026-07-05T10:00:00Z','2026-07-05T10:00:00Z',NULL)",
            [],
        ).unwrap();

        // 07-05는 cwd가 없지만 project 매핑으로 repo를 복원해 그날 커밋을 읽어야 함
        let wl = super::collect_work_log(&store, "2026-07-05");
        let all_commits: Vec<&String> = wl.projects.iter().flat_map(|p| &p.commits).collect();
        assert!(
            all_commits.iter().any(|s| s.contains("옛 세션 repo 커밋")),
            "project 매핑으로 cwd 복원: {all_commits:?}"
        );
    }

    // 세션 한 건 삽입(cwd 있음). 커밋은 없어도 브랜치로 프로젝트가 생존.
    fn seed_session(
        store: &SqliteStore,
        sid: &str,
        host: &str,
        pid: &str,
        cwd: &str,
        branch: &str,
        first_ts: &str,
        last_ts: &str,
    ) {
        store
            .conn
            .execute(
                "INSERT INTO sessions
                 (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd)
                 VALUES (?1,?2,?3,'claude-code',?4,?5,?6,?7)",
                rusqlite::params![sid, host, pid, first_ts, last_ts, branch, cwd],
            )
            .unwrap();
    }

    #[test]
    fn collect_work_log_splits_projects_and_sorts_by_first_activity() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 오후에 시작한 space-a, 오전에 시작한 agent-meter → 정렬은 agent-meter 먼저.
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T05:00:00Z", "2026-07-20T06:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/meter", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 2, "두 프로젝트로 분리: {wl:?}");
        assert_eq!(wl.projects[0].name, "agent-meter", "먼저 시작한 프로젝트가 앞");
        assert_eq!(wl.projects[1].name, "space-a");
    }

    #[test]
    fn collect_work_log_unifies_wsl_and_windows_unc_variants() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 같은 agent-meter를 WSL 직접 + Windows(WSL UNC)로 접근 → 한 프로젝트.
        seed_session(&store, "s1", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");
        seed_session(&store, "s2", "Windows", "pC",
            r"\\wsl.localhost\Ubuntu-22.04\home\jayb\work\agent-meter",
            "feat/b", "2026-07-20T03:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 1, "경로 변종은 한 프로젝트로 통합: {wl:?}");
        assert_eq!(wl.projects[0].name, "agent-meter");
    }

    #[test]
    fn collect_work_log_flags_concurrent_on_time_overlap() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 두 프로젝트의 구간이 겹침(01:00-03:00 vs 02:00-04:00).
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T03:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/b", "2026-07-20T02:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert!(wl.concurrent, "시간 겹치는 두 프로젝트 → concurrent");
    }

    #[test]
    fn collect_work_log_not_concurrent_when_sequential() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 순차(01:00-02:00, 03:00-04:00) — 겹치지 않음.
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/b", "2026-07-20T03:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert!(!wl.concurrent, "순차 진행 → not concurrent");
    }

    #[test]
    fn collect_work_log_filters_noise_projects() {
        let store = SqliteStore::open_in_memory().unwrap();
        // main 브랜치 + 노이즈 프롬프트 + cwd 있지만 git repo 아님 → 커밋·topic 모두 없음 → 제외.
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd, first_prompt_preview)
             VALUES ('n1','Windows','pN','claude-code','2026-07-20T01:00:00Z','2026-07-20T01:10:00Z','main',?1,'<task-notification>')",
            rusqlite::params![r"C:\Users\jibin\AppData\Local\Temp\noise"],
        ).unwrap();
        // 살아남는 프로젝트 하나(서술 브랜치).
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T02:00:00Z", "2026-07-20T03:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 1, "노이즈 프로젝트 제외: {wl:?}");
        assert_eq!(wl.projects[0].name, "space-a");
    }

    #[test]
    fn collect_work_log_noise_overlap_does_not_flag_concurrent() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 노이즈 세션(커밋·topic 없음)이 실제 프로젝트와 시간 겹침 →
        // 노이즈는 제외되고 실제 프로젝트 하나만 남으므로 concurrent=false 여야 한다.
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd, first_prompt_preview)
             VALUES ('n1','Windows','pN','claude-code','2026-07-20T01:00:00Z','2026-07-20T04:00:00Z','main',?1,'<task-notification>')",
            rusqlite::params![r"C:\Users\jibin\AppData\Local\Temp\noise"],
        ).unwrap();
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T02:00:00Z", "2026-07-20T03:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 1, "노이즈 제외 후 프로젝트 하나: {wl:?}");
        assert!(!wl.concurrent, "노이즈 프로젝트 겹침은 concurrent 아님: {wl:?}");
    }

    #[test]
    fn collect_work_log_keeps_hosts_distinct_in_cwdless_fallback() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 같은 project_id, cwd 없음, 서로 다른 host(두 distro) → 별도 프로젝트로 유지.
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd)
             VALUES ('a','wsl:Ubuntu-22.04','samepid','claude-code','2026-07-20T01:00:00Z','2026-07-20T02:00:00Z','feat/a',NULL)",
            [],
        ).unwrap();
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd)
             VALUES ('b','wsl:Debian','samepid','claude-code','2026-07-20T03:00:00Z','2026-07-20T04:00:00Z','feat/b',NULL)",
            [],
        ).unwrap();

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 2, "다른 host의 동일 project_id는 분리: {wl:?}");
    }

    #[test]
    fn brief_serializes_project_names() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T02:00:00Z", "2026-07-20T03:00:00Z");
        let wl = super::collect_work_log(&store, "2026-07-20");
        let json = serde_json::to_string(&wl).unwrap();
        assert!(json.contains("\"projects\""), "work_log JSON에 projects: {json}");
        assert!(json.contains("space-a"), "프로젝트 이름 직렬화: {json}");
        assert!(json.contains("\"concurrent\""), "concurrent 플래그 직렬화");
    }

    #[test]
    fn parse_numstat_log_sums_churn_per_commit() {
        let sample = "\u{1e}feat: a\n5\t2\tsrc/a.rs\n3\t0\tsrc/b.rs\n\n\u{1e}fix: b\n1\t1\tREADME.md\n";
        let out = super::parse_numstat_log(sample);
        assert_eq!(out, vec![("feat: a".to_string(), 10), ("fix: b".to_string(), 2)]);

        // 바이너리("-\t-") 는 0, 커밋 제목만 있고 변경 없으면 0
        let bin = "\u{1e}bin only\n-\t-\tlogo.png\n\u{1e}empty\n";
        assert_eq!(
            super::parse_numstat_log(bin),
            vec![("bin only".to_string(), 0), ("empty".to_string(), 0)]
        );
    }

    #[test]
    fn git_commits_for_reads_dated_authored_subjects() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let run = |args: &[&str]| {
            Command::new("git").args(["-C", dir]).args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-08T12:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-08T12:00:00")
                .output().unwrap()
        };
        Command::new("git").args(["init", "-q", dir]).output().unwrap();
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
        // 3줄짜리 파일 추가 → churn = 3
        std::fs::write(tmp.path().join("f.txt"), "l1\nl2\nl3\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "feat: work_log 다이어리 반영"]);

        let subs = super::git_commits_for("Windows", dir, "2026-07-08");
        assert!(
            subs.iter().any(|(s, c)| s.contains("work_log 다이어리 반영") && *c == 3),
            "제목+churn: {subs:?}"
        );
        assert!(super::git_commits_for("Windows", dir, "2026-07-09").is_empty(), "다른 날짜엔 없음");
        let nogit = tempfile::tempdir().unwrap();
        assert!(super::git_commits_for("Windows", nogit.path().to_str().unwrap(), "2026-07-08").is_empty());
    }

    #[test]
    fn balance_commits_behaviors() {
        use super::balance_commits;
        // 반환은 그룹별 (label, titles); 기존 검증은 평탄 리스트 기준 — 순서 무관 검증만 남는다.
        fn flat(g: Vec<(String, Vec<String>)>) -> Vec<String> {
            g.into_iter().flat_map(|(_, v)| v).collect()
        }

        // 빈 입력
        assert!(flat(balance_commits(vec![])).is_empty());

        // floor 보장: churn 큰 A(12개) + churn 작은 B(1개), C=13 → B 실종 금지
        let a: Vec<(String, u64)> = (0..12).map(|i| (format!("a{i}"), 500)).collect();
        let b = vec![(String::from("bonly"), 5)];
        let out = flat(balance_commits(vec![("A".into(), a), ("B".into(), b)]));
        assert!(out.contains(&"bonly".to_string()), "floor: B 커밋 실종 금지: {out:?}");
        assert!(out.len() <= 12);

        // churn 비례 (개수 역전): 둘 다 10개인데 A churn 800, B churn 20 → A 과반
        let a: Vec<(String, u64)> = (0..10).map(|i| (format!("a{i}"), 800)).collect();
        let b: Vec<(String, u64)> = (0..10).map(|i| (format!("b{i}"), 20)).collect();
        let out = flat(balance_commits(vec![("A".into(), a), ("B".into(), b)]));
        let na = out.iter().filter(|s| s.starts_with('a')).count();
        let nb = out.iter().filter(|s| s.starts_with('b')).count();
        assert!(na >= 8, "churn 큰 A 과반: na={na} nb={nb}");
        assert!(nb >= 1, "B floor 보장");
        assert_eq!(out.len(), 12);

        // churn 우선 채택: 단일 repo 13개(cap 초과) → churn 최저 탈락
        let mut c: Vec<(String, u64)> = (0..12).map(|i| (format!("big{i}"), 100)).collect();
        c.push(("tiny".into(), 1));
        let out = flat(balance_commits(vec![("A".into(), c)]));
        assert_eq!(out.len(), 12);
        assert!(!out.contains(&"tiny".to_string()), "churn 최저 탈락: {out:?}");

        // churn desc 순서 (cap 이내)
        let out = flat(balance_commits(vec![(
            "A".into(),
            vec![("low".into(), 10), ("high".into(), 900), ("mid".into(), 100)],
        )]));
        assert_eq!(out, vec!["high".to_string(), "mid".to_string(), "low".to_string()]);

        // clamp: A(5개, 1개 5000+4개 10) vs B(10개 각 200), C=15 → B가 A보다 많음
        let mut a = vec![("lock".to_string(), 5000u64)];
        a.extend((0..4).map(|i| (format!("a{i}"), 10)));
        let b: Vec<(String, u64)> = (0..10).map(|i| (format!("b{i}"), 200)).collect();
        let out = flat(balance_commits(vec![("A".into(), a), ("B".into(), b)]));
        let na = out.iter().filter(|s| *s == "lock" || s.starts_with('a')).count();
        let nb = out.iter().filter(|s| s.starts_with('b')).count();
        assert!(nb > na, "clamp: 저활동 A가 lockfile로 상위 불가 na={na} nb={nb}");

        // cap 이하 전부 포함
        let out = flat(balance_commits(vec![
            ("A".into(), vec![("a0".into(), 1), ("a1".into(), 1)]),
            ("B".into(), vec![("b0".into(), 1)]),
            ("C".into(), vec![("c0".into(), 1)]),
        ]));
        assert_eq!(out.len(), 4);

        // 중복 제목 1회만
        let out = flat(balance_commits(vec![
            ("A".into(), vec![("dup".into(), 100), ("a1".into(), 100)]),
            ("B".into(), vec![("dup".into(), 100), ("b1".into(), 100)]),
        ]));
        assert_eq!(out.iter().filter(|s| *s == "dup").count(), 1, "중복 1회: {out:?}");

        // repo 과다: churn 0 repo 20개 → cap개만
        let groups: Vec<(String, Vec<(String, u64)>)> =
            (0..20).map(|i| (format!("r{i:02}"), vec![(format!("c{i:02}"), 0)])).collect();
        assert_eq!(flat(balance_commits(groups)).len(), 12);

        // 저-churn D'Hondt: 정수 나눗셈이 몫을 0으로 뭉개는 구간(가중치 2 vs 1)에서도
        // 가중치 큰 repo가 우세해야 함(교차곱 비교 + 상위 우선 tie-break). 각 10커밋(cap 초과).
        let mut a = vec![("a0".to_string(), 2u64)];
        a.extend((1..10).map(|i| (format!("a{i}"), 0)));
        let mut b = vec![("b0".to_string(), 1u64)];
        b.extend((1..10).map(|i| (format!("b{i}"), 0)));
        let out = flat(balance_commits(vec![("A".into(), a), ("B".into(), b)]));
        let na = out.iter().filter(|s| s.starts_with('a')).count();
        let nb = out.iter().filter(|s| s.starts_with('b')).count();
        assert!(na > nb, "저-churn 동률 구간에서도 가중치 큰 A 우세: na={na} nb={nb}");
    }

    #[test]
    fn idle_prompt_directs_imaginative_persona() {
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: true, is_holiday: false, days_idle: Some(2),
            occasions: vec![], recent_diaries: vec![],
        };
        let p = build_idle_prompt(&DiaryConfig::default(), &idle, &[]);
        assert!(p.contains("조용한 날"));        // 무활동일 프레이밍
        assert!(p.contains("지어내"));           // 상상 일기(사실 규율 해제)
        assert!(p.contains("에이전트 친구"));    // 동료 에이전트 설정
        assert!(p.contains("송편") || p.contains("명절")); // occasion 테마
        assert!(p.contains("days_idle"));        // 며칠째 조용 신호
        assert!(p.contains("recent_diaries")); // 반복 방지 신규
    }

    #[test]
    fn idle_prompt_palette_rotates_by_date_and_frames_holiday() {
        let base = IdleContext {
            date: "2026-01-01".into(), is_weekend: false, is_holiday: true, days_idle: None,
            occasions: vec![], recent_diaries: vec![],
        };
        let other = IdleContext { date: "2026-06-15".into(), ..base.clone() };
        let p1 = build_idle_prompt(&DiaryConfig::default(), &base, &[]);
        let p2 = build_idle_prompt(&DiaryConfig::default(), &other, &[]);
        assert_ne!(p1, p2, "날짜에 따라 소재 spotlight 회전");
        assert!(p1.contains("mood"));    // occasion mood 톤 처리
        assert!(p1.contains("공휴일"));   // is_holiday 프레이밍
    }

    #[test]
    fn system_prompt_handles_holiday_and_mood() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains("is_holiday"));  // 공휴일 근무 위로 트리거
        assert!(p.contains("mood"));         // occasion mood 톤 처리
        assert!(p.contains("추모"));         // solemn 처리 지시
    }

    #[test]
    fn render_idle_diary_writes_persona_with_footer() {
        use crate::diary::engine::MockEngine;
        let cfg = DiaryConfig::default();
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: true, is_holiday: false, days_idle: Some(2),
            occasions: vec![], recent_diaries: vec![],
        };
        let engine = MockEngine { canned: "옆 동네 봇이랑 놀았다.".into() };
        let r = render_idle_diary(&engine, &idle, &cfg, &[]).unwrap();
        assert!(r.body.contains("옆 동네 봇이랑 놀았다."));
        assert!(r.body.contains("토큰"), "footer meters tokens");
    }

    #[test]
    fn similar_diaries_pulls_recent_idle_outside_3day_window() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        // 14일 전(3일 창 밖) idle 일기 하나 — 이벤트 없음 → idle. 파일·인덱스는 persist_diary로 생성.
        let past = RenderedDiary {
            body: "심심해서 옆 동네 봇이랑 놀았다.\n\n*— ~10 토큰 (엔진: mock)*\n".into(),
            tokens_used: 10,
            engine_name: "mock".into(),
        };
        persist_diary(&store, "2026-07-16", "Windows", &past, &cfg).unwrap();
        // 오늘 2026-07-30 도 idle(이벤트 없음) → 같은 성격 병합
        let brief = assemble_brief(&store, "Windows", "2026-07-30", &cfg).unwrap();
        let hit = brief.recent_diaries.iter().find(|r| r.date == "2026-07-16");
        assert!(hit.is_some(), "같은 성격(idle) 최근 일기 병합");
        assert_eq!(hit.unwrap().excerpt, "심심해서 옆 동네 봇이랑 놀았다.", "토큰 푸터 제외");
    }

    #[test]
    fn similar_diaries_skip_in_window_dates_to_reach_older_same_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        let mk = |txt: &str| RenderedDiary {
            body: format!("{txt}\n\n*— ~10 토큰 (엔진: mock)*\n"),
            tokens_used: 10,
            engine_name: "mock".into(),
        };
        // 3일 창 안 idle 일기 2개(07-28·07-29) + 창 밖(07-20) idle 일기 1개. 모두 이벤트 없음 → idle.
        persist_diary(&store, "2026-07-28", "Windows", &mk("28일 낮잠"), &cfg).unwrap();
        persist_diary(&store, "2026-07-29", "Windows", &mk("29일 산책"), &cfg).unwrap();
        persist_diary(&store, "2026-07-20", "Windows", &mk("20일 로그 구경"), &cfg).unwrap();
        // 오늘 2026-07-30 idle — 창 안 두 날이 쿼터를 소진하면 07-20이 밀려난다(회귀).
        let brief = assemble_brief(&store, "Windows", "2026-07-30", &cfg).unwrap();
        assert!(brief.recent_diaries.iter().any(|r| r.date == "2026-07-20"),
            "창 밖 같은 성격 일기가 in-window 날에 밀려나지 않아야 함");
    }

    #[test]
    fn idle_context_carries_holiday_and_recent() {
        let idle = IdleContext {
            date: "2026-07-17".into(),
            is_weekend: false,
            is_holiday: true,
            days_idle: Some(1),
            occasions: vec![],
            recent_diaries: vec![],
        };
        let json = serde_json::to_string(&idle).unwrap();
        assert!(json.contains("\"is_holiday\":true"));
        assert!(json.contains("recent_diaries"));
    }

    #[test]
    fn days_since_last_active_counts_gap() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[turn_event("Windows", "p", "s1", "u1", "2026-07-08T10:00:00Z")]).unwrap();
        store.rebuild_rollup().unwrap();
        // 07-10 기준 마지막 활동 07-08 → 2일째
        assert_eq!(store.days_since_last_active("2026-07-10").unwrap(), Some(2));
        // 활동일(07-08) 이전엔 이력 없음 → None
        assert_eq!(store.days_since_last_active("2026-07-08").unwrap(), None);
    }

    #[test]
    fn assemble_brief_aggregates_all_hosts_not_just_windows() {
        // 07-09에 WSL만 활동(Windows 0) — 다이어리는 주인의 하루라 이걸 무활동으로 보면 안 됨(이전 버그).
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        store.upsert_events(&[
            turn_event("wsl:Ubuntu-22.04", "avatar-meter", "w1", "u1", "2026-07-09T10:00:00Z"),
        ]).unwrap();
        store.rebuild_rollup().unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-09", &cfg).unwrap();
        assert!(brief.totals.session_count > 0, "WSL 활동도 합산되어 무활동일이 아님");
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

        // 어제(07-09) 일기 존재 → recent_diaries에 포함 → 그날 finding(R1)이 "이미 다라이프"
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
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
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
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains(super::voice_guidance())); // 조각이 그대로 배선됨
    }

    #[test]
    fn system_prompt_has_humor_evidence_and_occasions_instructions() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains("주인"));   // 호칭
        assert!(p.contains("유머"));   // 유머 지시
        assert!(p.contains("detail")); // 근거 필드 사용 지시
        assert!(p.contains("suggested_action")); // 개선방향 필드 사용 지시
        assert!(p.contains("occasions")); // 기념일/명절 사용 지시
    }

    #[test]
    fn system_prompt_directs_short_length_and_moderate_emoji() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains("2~3문단"));   // 길이 밴드(문단, commit_count=0 기준)
        assert!(p.contains("400자"));     // 길이 밴드(글자, commit_count=0 기준)
        assert!(p.contains("골라"));      // 핵심만 골라 쓰기(장황함 차단)
        assert!(p.contains("이모지"));    // 이모지 지시
        assert!(p.contains("문단마다 1~2개")); // 사용량 상향(1개 정도 → 1~2개)
    }

    #[test]
    fn diary_length_bands() {
        assert_eq!(super::diary_length(0), (400, "2~3"));
        assert_eq!(super::diary_length(3), (400, "2~3"));
        assert_eq!(super::diary_length(4), (550, "3"));
        assert_eq!(super::diary_length(10), (550, "3"));
        assert_eq!(super::diary_length(11), (750, "4"));
        assert_eq!(super::diary_length(999), (750, "4"));
    }

    #[test]
    fn build_system_prompt_length_adapts_to_commit_count() {
        let cfg = DiaryConfig::default();
        assert!(super::build_system_prompt(&cfg, 2, &[]).contains("400자"), "가벼운 날 400자");
        assert!(super::build_system_prompt(&cfg, 7, &[]).contains("550자"), "보통 날 550자");
        assert!(super::build_system_prompt(&cfg, 20, &[]).contains("750자"), "바쁜 날 750자 상한");
    }

    #[test]
    fn system_prompt_directs_context_signals_and_comfort() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains("상시 이슈"));         // 이미 다룬 상시 이슈 제외 언급
        assert!(p.contains("tool_usage"));        // 도구 텍스처 지시
        assert!(p.contains("work_context"));      // 근무 맥락
        assert!(p.contains("위로"));              // 주말/장시간 위로
        assert!(p.contains("일중독"));            // 주말 능청 예시(유머·주말 강화)
        assert!(p.contains("나열하지"));          // 종류별 문단 나열 금지(자연스러운 흐름)
        assert!(p.contains("work_log"));          // 그날 한 작업(커밋/토픽) 지시
    }

    #[test]
    fn system_prompt_directs_project_scoped_work_log() {
        let p = build_system_prompt(&DiaryConfig::default(), 5, &[]);
        assert!(p.contains("projects"), "프로젝트별 구조 언급");
        assert!(p.contains("어느 프로젝트"), "작업의 프로젝트 귀속 지시");
        assert!(p.contains("concurrent"), "동시 진행 지시");
    }

    #[test]
    fn system_prompt_directs_recent_diary_variety() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(p.contains("recent_diaries")); // 최근 일기 참조 지시
        assert!(p.contains("되풀이하지"));      // 이미 다룬 화제 반복 금지
        assert!(p.contains("다른 이야기"));     // 어제와 다른 서사
    }

    #[test]
    fn finding_advice_r7_project_card_renders_session_count() {
        // v3 R7 프로젝트 카드는 session_ids/total_sessions/note만 낸다(ratio_pct 등 폐기, 프론트 계약 정합).
        // est_tokens_saved는 설계상 0 — detail에 "비용-등가" 등 무근거 수치를 붙이면 안 됨.
        let (detail, action) = super::finding_advice(
            "R7",
            &serde_json::json!({
                "total_sessions": 4,
                "session_ids": ["s1", "s2"],
                "note": "LLM 판정: 이 프로젝트의 Opus 세션 상당수가 Sonnet으로 충분",
            }),
            0,
        );
        assert!(detail.contains("4건"), "세션 수 포함해야: {detail}");
        assert!(!detail.contains("0%"), "퍼센트 언급 금지: {detail}");
        assert!(!detail.contains("비용-등가"), "무근거 토큰-등가 문구 금지: {detail}");
        assert!(action.contains("claude --model sonnet"));
    }

    #[test]
    fn finding_advice_r6_reads_prompt_and_count() {
        let (detail, action) = super::finding_advice(
            "R6",
            &serde_json::json!({ "repeated_prompt": "매일 아침 배포 리포트 뽑아줘", "session_count": 3 }),
            0,
        );
        assert!(detail.contains("3개 세션"));
        assert!(detail.contains("매일 아침 배포 리포트"));
        assert!(!detail.contains("repeated_prompt")); // 원본 JSON 노출 금지
        assert!(action.contains("스킬"));
    }

    #[test]
    fn finding_advice_r23_reads_sequence() {
        let (detail, action) = super::finding_advice(
            "R23",
            &serde_json::json!({
                "sequence": ["bash:gh", "skill:codex:rescue", "file-ops"],
                "session_count": 4,
            }),
            0,
        );
        assert!(detail.contains("4개 세션"));
        assert!(detail.contains("bash:gh → skill:codex:rescue → file-ops"));
        assert!(!detail.contains("sequence")); // 원본 JSON 노출 금지
        assert!(action.contains("스킬"));
    }

    #[test]
    fn finding_advice_r8_cites_server_and_measured_tokens() {
        let (detail, action) = super::finding_advice(
            "R8",
            &serde_json::json!({
                "server": "context7", "large_result_count": 3,
                "approx_tokens_avg": 3000, "approx_tokens_total": 9000
            }),
            0,
        );
        assert!(detail.contains("context7"));
        assert!(detail.contains("3번"));
        assert!(detail.contains("3000")); // 측정된 평균 토큰 인용
        assert!(detail.contains("9000")); // 누적
        assert!(action.contains("필요한 필드만") || action.contains("좁혀"));
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
        assert!(action.contains("직접 만든 자동화라면"));
        assert!(action.contains("참고만"));
        assert!(!action.contains("지정하세요"), "수정 지시 문구 금지 (v3 §3.2)");
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
        let rendered = render_diary(&engine, &brief, &cfg, &[]).unwrap();
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

    // ── Task 6: 일기 프롬프트 메모리 주입 ──

    #[test]
    fn diary_system_prompt_injects_memories() {
        let cfg = DiaryConfig::default();
        let mems = vec!["주인은 비건임".to_string()];
        let p = build_system_prompt(&cfg, 0, &mems);
        assert!(p.contains("[주인에 대해 기억한 것"));
        assert!(p.contains("주인은 비건임"));
    }

    #[test]
    fn diary_system_prompt_no_memory_section_when_empty() {
        let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
        assert!(!p.contains("[주인에 대해 기억한 것"));
    }

    #[test]
    fn idle_prompt_injects_memories() {
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: false, is_holiday: false, days_idle: None,
            occasions: vec![], recent_diaries: vec![],
        };
        let p = build_idle_prompt(&DiaryConfig::default(), &idle, &["주인은 고양이를 키움".to_string()]);
        assert!(p.contains("주인은 고양이를 키움"));
    }

    #[test]
    fn diary_prompt_injects_mbti_voice() {
        let mut cfg = DiaryConfig::default();
        cfg.mbti = Some("INTJ".into());
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: false, is_holiday: false, days_idle: None,
            occasions: vec![], recent_diaries: vec![],
        };
        let p = build_idle_prompt(&cfg, &idle, &[]);
        assert!(p.contains("냉정")); // T 성향 톤이 idle 프롬프트에도
    }

    #[test]
    fn diary_system_prompt_memory_header_uses_custom_honorific() {
        let cfg = DiaryConfig { honorific: "대장".into(), ..DiaryConfig::default() };
        let mems = vec!["주인은 비건임".to_string()];
        let p = build_system_prompt(&cfg, 5, &mems);
        assert!(p.contains("[대장에 대해"));
        assert!(!p.contains("[주인에 대해"));
    }

    #[test]
    fn diary_system_prompt_injects_mbti_voice() {
        let mut cfg = DiaryConfig::default();
        cfg.mbti = Some("INTJ".into());
        let p = build_system_prompt(&cfg, 5, &[]);
        assert!(p.contains("냉정")); // T 성향 톤이 일기 본문 프롬프트에도
    }

    #[test]
    fn idle_palette_spotlight_uses_custom_honorific() {
        // 2026-01-06의 day-of-year(6) 회전이 팔레트 index 6("주인이 두고 간 …")을 포함하는 날짜.
        let s = idle_palette_spotlight("2026-01-06", "대장");
        assert!(s.contains("대장이 두고 간"));
        assert!(!s.contains("주인이 두고"));
    }
}
