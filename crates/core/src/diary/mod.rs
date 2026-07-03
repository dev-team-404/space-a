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
            let path = evidence.get("path").and_then(|v| v.as_str()).unwrap_or("어떤 파일");
            let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("`{path}`를 {count}회 반복해서 읽음 (~{est_tokens_saved}토큰)"),
                "한 번만 읽고 그 내용을 기억해 두면 다음엔 그 토큰을 아낄 수 있어요".to_string(),
            )
        }
        "R1" => {
            let server = evidence.get("server").and_then(|v| v.as_str()).unwrap_or("어떤 서버");
            (
                format!("MCP 서버 `{server}`가 상주하는데 호출 0회 (~{est_tokens_saved}토큰 추정)"),
                format!("안 쓰는 `{server}`를 설정에서 제거하면 매 세션 상주 토큰을 아껴요"),
            )
        }
        "R7" => {
            let out = evidence.get("tok_output").and_then(|v| v.as_u64()).unwrap_or(0);
            let n = evidence.get("tool_calls").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("이 세션은 전부 Opus인데 출력 {out}토큰·도구 {n}회의 가벼운 작업이었어요 (~{est_tokens_saved}토큰 비용-등가)"),
                "이런 잔심부름은 Haiku로 전환하면 같은 결과를 훨씬 싸게 낼 수 있어요".to_string(),
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
            (
                format!("플러그인 {plugin}의 스킬 {n}개(~{est_tokens_saved}토큰)를 한 번도 쓰지 않았어요"),
                "안 쓰는 플러그인은 설정에서 비활성화하면 매 세션 상주 토큰을 아껴요".to_string(),
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
        .and_then(|ts| NaiveDate::parse_from_str(ts.get(..10)?, "%Y-%m-%d").ok());
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

pub fn generate_diary(
    store: &SqliteStore,
    engine: &dyn Engine,
    brief: &Brief,
    cfg: &DiaryConfig,
) -> Result<DiaryOutput> {
    let system = build_system_prompt(cfg);
    let user = serde_json::to_string_pretty(brief)?;

    let out = engine.generate(&system, &user)?;
    let body = format!(
        "{narrative}\n\n*— 이 일기 ~{tokens} 토큰 (엔진: {engine})*\n",
        narrative = out.text,
        tokens = out.tokens_used,
        engine = engine.name(),
    );

    std::fs::create_dir_all(&cfg.vault_dir)?;
    let path = cfg.vault_dir.join(format!("{}.md", brief.date));
    std::fs::write(&path, body)?;

    store.upsert_diary_index(
        &brief.date,
        &brief.host,
        &path.to_string_lossy(),
        out.tokens_used,
        &engine.name(),
    )?;

    Ok(DiaryOutput { path, tokens_used: out.tokens_used })
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
    fn finding_advice_r5_and_r1() {
        let (detail, action) = super::finding_advice(
            "R5",
            &serde_json::json!({"path": "report.xlsx", "count": 7}),
            7200,
        );
        assert!(detail.contains("report.xlsx"));
        assert!(detail.contains("7"));
        assert!(detail.contains("7200"));
        assert!(!action.is_empty());

        let (d1, a1) = super::finding_advice(
            "R1",
            &serde_json::json!({"server": "playwright"}),
            2500,
        );
        assert!(d1.contains("playwright"));
        assert!(a1.contains("playwright"));
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
    fn finding_advice_r7() {
        let (detail, action) = super::finding_advice(
            "R7",
            &serde_json::json!({"model":"opus","tok_output":420,"tool_calls":2}),
            48320,
        );
        assert!(detail.contains("420"));
        assert!(detail.contains("2회"));
        assert!(detail.contains("48320"));
        assert!(action.contains("Haiku"));
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
}
