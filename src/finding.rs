use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity { Info, Suggest, Warn }

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Suggest => "suggest",
            Severity::Warn => "warn",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prescription {
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub scope_host: Option<String>,
    pub scope_project: Option<String>,
    pub scope_kind: String,
    pub scope_ref: String,
    pub evidence: Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<Prescription>,
    pub dedup_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;

    fn sample() -> Finding {
        Finding {
            rule_id: "R5".into(),
            severity: Severity::Suggest,
            scope_host: Some("Windows".into()),
            scope_project: Some("c--users-jibin".into()),
            scope_kind: "session".into(),
            scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"report.xlsx","count":7}),
            est_tokens_saved: 7200,
            prescription: None,
            dedup_key: "R5|s1|report.xlsx".into(),
        }
    }

    #[test]
    fn finding_upsert_is_idempotent_and_counts_occurrences() {
        use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage};
        let store = SqliteStore::open_in_memory().unwrap();
        // sample() finding은 session s1(Windows) 스코프 → findings_for_date가 조인할 세션 시드.
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.upsert_finding(&sample(), "2026-07-01T10:00:00Z").unwrap();
        store.upsert_finding(&sample(), "2026-07-01T11:00:00Z").unwrap();
        assert_eq!(store.count_findings().unwrap(), 1);

        let got = store.findings_for_date("Windows", "2026-07-01").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].est_tokens_saved, 7200);

        let occ: i64 = store
            .conn
            .query_row(
                "SELECT occurrences FROM findings WHERE dedup_key = 'R5|s1|report.xlsx'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(occ, 2, "second upsert of same dedup_key must increment occurrences");
    }
}
