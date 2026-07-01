pub mod engine;

use crate::diary::engine::Engine;
use crate::store::SqliteStore;
use anyhow::Result;
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
}

#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
}

pub fn assemble_brief(store: &SqliteStore, host: &str, date: &str) -> Result<Brief> {
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
        .findings_for_date(date)?
        .into_iter()
        .map(|f| BriefFinding {
            rule_id: f.rule_id,
            severity: f.severity.as_str().to_string(),
            evidence: f.evidence,
            est_tokens_saved: f.est_tokens_saved,
            prescription: f.prescription.map(|p| serde_json::json!({
                "kind": p.kind, "payload": p.payload
            })),
        })
        .collect();

    Ok(Brief { date: date.to_string(), host: host.to_string(), totals, findings })
}

#[derive(Debug, Clone)]
pub struct DiaryConfig {
    pub vault_dir: PathBuf,
    pub tone: String,      // "A" | "B" | "C"
    pub honorific: String, // 기본 "주인"
}

impl Default for DiaryConfig {
    fn default() -> Self {
        DiaryConfig {
            vault_dir: PathBuf::from("./diary"),
            tone: "B".to_string(),
            honorific: "주인".to_string(),
        }
    }
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
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)입니다. \
         규칙(정밀도의 선): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. 자유로운 소감은 서사에만 담고 \
         행동 지시로 승격하지 마세요. '잘한 것'과 '아쉬운 것'을 均衡있게 담되 짧게 쓰세요.",
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

        let brief = assemble_brief(&store, "Windows", "2026-07-01").unwrap();
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
        };
        let tmp = tempfile::tempdir().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            tone: "B".into(),
            honorific: "주인".into(),
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
}
