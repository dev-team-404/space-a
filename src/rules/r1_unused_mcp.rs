use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

pub struct R1UnusedMcp {
    pub min_resident_tokens: u64,
    pub heuristic_tokens_per_server: u64,
}

impl Default for R1UnusedMcp {
    fn default() -> Self {
        R1UnusedMcp { min_resident_tokens: 2000, heuristic_tokens_per_server: 2500 }
    }
}

impl Rule for R1UnusedMcp {
    fn id(&self) -> &'static str {
        "R1"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        // 1) 인벤토리에 등록된 (host, project, server) 전체
        let mut inv_stmt = store
            .conn
            .prepare("SELECT host, project_id, server FROM mcp_inventory")?;
        let inv: Vec<(String, String, String)> = inv_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;

        let mut out = Vec::new();
        for (host, project, server) in inv {
            // 2) 이 서버가 해당 project에서 호출된 횟수
            let calls: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM events
                 WHERE project_id=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                params![project, server],
                |r| r.get(0),
            )?;
            if calls > 0 {
                continue;
            }

            // 3) 해당 project의 대표 상주 비용(첫 턴 cache_create 최댓값)
            let resident: i64 = store.conn.query_row(
                "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                 WHERE project_id=?1 AND kind='assistant_turn'",
                params![project],
                |r| r.get(0),
            )?;
            if (resident as u64) <= self.min_resident_tokens {
                continue;
            }

            out.push(Finding {
                rule_id: "R1".into(),
                severity: Severity::Warn,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "server": server,
                    "resident_tokens_total": resident,
                    "calls": 0,
                    "note": "약(~) 추정 — 서버별 정확 귀속은 옵트인 프로브(유예)"
                }),
                est_tokens_saved: self.heuristic_tokens_per_server,
                prescription: Some(Prescription {
                    kind: "remove_mcp".into(),
                    payload: serde_json::json!({ "server": server }),
                }),
                dedup_key: format!("R1|{project}|{server}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::McpServer;
    use crate::model::*;
    use crate::rules::Rule;
    use crate::store::SqliteStore;

    fn first_turn(project: &str, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: project.into(), session_id: "s1".into(),
            uuid: Some("u_turn".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { cache_creation: cache_create, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn mcp_call(project: &str, uuid: &str, server: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: project.into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:01:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "x".into() },
                raw_name: format!("mcp__{server}__x"), target: None,
            },
        }
    }

    #[test]
    fn r1_flags_configured_but_unused_server_with_resident_cost() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "c--users-jibin";
        // 상주 비용 55k, context7만 호출됨 → playwright는 미사용
        store.upsert_events(&[
            first_turn(proj, 55000),
            mcp_call(proj, "m1", "context7"),
        ]).unwrap();
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "context7".into(), source: "project".into() },
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R1");
        assert_eq!(f.evidence["server"], "playwright");
        assert_eq!(f.prescription.as_ref().unwrap().kind, "remove_mcp");
        assert_eq!(f.est_tokens_saved, 2500);
    }

    #[test]
    fn r1_silent_when_no_resident_cost() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "c--users-jibin";
        store.upsert_events(&[first_turn(proj, 100)]).unwrap(); // 상주 비용 미미
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();
        assert!(R1UnusedMcp::default().evaluate(&store).unwrap().is_empty());
    }
}
