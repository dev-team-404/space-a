pub mod engine;
pub mod occasions;

use crate::diary::engine::Engine;
use crate::diary::occasions::{compute_occasions, Occasion};
use crate::store::SqliteStore;
use anyhow::Result;
use chrono::NaiveDate;
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

#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
    pub occasions: Vec<Occasion>,
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

    let findings = store
        .findings_for_date(host, date)?
        .into_iter()
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

    let locale = resolve_locale(cfg);
    let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
    let anchor = store
        .earliest_session_ts()?
        .and_then(|ts| local_date_of(&ts));
    let occasions = match today {
        Some(d) => compute_occasions(d, anchor, &locale, cfg.include_dev_days),
        None => Vec::new(),
    };

    Ok(Brief { date: date.to_string(), host: host.to_string(), totals, findings, occasions })
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

pub fn build_system_prompt(cfg: &DiaryConfig) -> String {
    format!(
        "당신은 사용자의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         1인칭으로 하루를 회고하는 일기를 씁니다. 사용자를 '{honorific}'이라고 부릅니다. \
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)이며, 톤과 무관하게 기본적으로 \
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. \
         '잘한 것'과 '아쉬운 것'은 각 finding의 `detail`(근거 수치)과 `suggested_action`(개선 방향)에 \
         근거해 구체적으로 써서, 무엇을 왜 그렇게 하면 좋은지 주인이 바로 알 수 있게 하세요. \
         자유로운 소감은 서사에만 담고 행동 지시로 승격하지 마세요. \
         \
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요(예: 오늘이 크리스마스이거나 함께한 지 100일 등). \
         비어있으면 언급하지 마세요.",
        honorific = cfg.honorific,
        tone = cfg.tone,
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
    fn system_prompt_has_humor_evidence_and_occasions_instructions() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("주인"));   // 호칭
        assert!(p.contains("유머"));   // 유머 지시
        assert!(p.contains("detail")); // 근거 필드 사용 지시
        assert!(p.contains("suggested_action")); // 개선방향 필드 사용 지시
        assert!(p.contains("occasions")); // 기념일/명절 사용 지시
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
