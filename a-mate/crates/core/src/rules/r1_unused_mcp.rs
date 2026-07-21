use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

/// R1 — 안 쓰는 always-on MCP.
///
/// v0 단순화 (스펙과의 의도적 이탈, 추후 보정):
/// - 스펙 §5는 "최근 10세션 호출 0"이지만 v0는 **전체 기간 호출 0**으로 판정(더 보수적, 오탐 적음).
/// - 스펙 §8은 "세션 첫 턴 cache_creation"이지만 v0는 프로젝트 전체 assistant_turn의 **MAX(tok_cache_create)**를 상주 비용 근사로 사용.
/// - 서버별 정확 귀속(옵트인 프로브)은 유예 — est_tokens_saved는 flat heuristic("약~").
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
            let is_global = project == "*";

            // 2) 호출 횟수 — host 필터 필수. 글로벌은 host 전체, 프로젝트는 host+project.
            let calls: i64 = if is_global {
                store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                    params![host, server],
                    |r| r.get(0),
                )?
            } else {
                store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND project_id=?2 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?3",
                    params![host, project, server],
                    |r| r.get(0),
                )?
            };
            if calls > 0 {
                continue;
            }

            // 3) 대표 상주 비용(첫 턴 cache_create 근사 = MAX). 글로벌은 host 전체, 프로젝트는 host+project.
            let resident: i64 = if is_global {
                store.conn.query_row(
                    "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                     WHERE host=?1 AND kind='assistant_turn'",
                    params![host],
                    |r| r.get(0),
                )?
            } else {
                store.conn.query_row(
                    "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                     WHERE host=?1 AND project_id=?2 AND kind='assistant_turn'",
                    params![host, project],
                    |r| r.get(0),
                )?
            };
            if resident <= self.min_resident_tokens as i64 {
                continue;
            }

            let scope_kind = if is_global { "host" } else { "project" };
            let scope_ref = if is_global { host.clone() } else { project.clone() };
            out.push(Finding {
                rule_id: "R1".into(),
                severity: Severity::Warn,
                scope_host: Some(host.clone()),
                scope_project: if is_global { None } else { Some(project.clone()) },
                scope_kind: scope_kind.into(),
                scope_ref: scope_ref.clone(),
                evidence: serde_json::json!({
                    "server": server,
                    "resident_tokens_total": resident,
                    "calls": 0,
                    "scope": scope_kind,
                    "note": "약(~) 추정 — 서버별 정확 귀속은 옵트인 프로브(유예)"
                }),
                est_tokens_saved: self.heuristic_tokens_per_server,
                prescription: Some(Prescription {
                    kind: "remove_mcp".into(),
                    payload: serde_json::json!({ "server": server }),
                }),
                dedup_key: format!("R1|{host}|{scope_ref}|{server}"),
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
            msg_id: None,
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
            msg_id: None,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "x".into() },
                raw_name: format!("mcp__{server}__x"), target: None, tool_use_id: None,
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

    // host 필터가 없으면 wsl 세션 호출이 Windows 인벤토리를 잘못 "사용됨"으로 만든다.
    #[test]
    fn r1_does_not_cross_contaminate_across_hosts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "shared-proj";

        // Windows: playwright 호출됨(사용). 상주 55k.
        store.upsert_events(&[
            first_turn(proj, 55000),
            mcp_call(proj, "w1", "playwright"),
        ]).unwrap();
        // wsl: 같은 project_id, 하지만 playwright 호출 없음. 상주 55k.
        let mut wsl_turn = first_turn(proj, 55000);
        wsl_turn.host = "wsl:Ubuntu-22.04".into();
        wsl_turn.uuid = Some("wsl_turn".into());
        store.upsert_events(&[wsl_turn]).unwrap();

        // 두 호스트 모두 playwright 인벤토리 보유.
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();
        store.upsert_inventory("wsl:Ubuntu-22.04", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "wsl 호스트에서만 미사용으로 잡혀야 함");
        assert_eq!(findings[0].scope_host.as_deref(), Some("wsl:Ubuntu-22.04"));
        assert_eq!(findings[0].evidence["server"], "playwright");
    }

    #[test]
    fn r1_flags_host_global_plugin_server_unused_across_host() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 상주 55k 세션이 프로젝트 p1 에 존재, context7(글로벌)은 어디서도 호출 안 됨.
        store.upsert_events(&[first_turn("p1", 55000)]).unwrap();
        store.upsert_inventory("Windows", "*", &[
            McpServer { name: "context7".into(), source: "plugin".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.scope_kind, "host");
        assert_eq!(f.scope_project, None);
        assert_eq!(f.scope_ref, "Windows");
        assert_eq!(f.evidence["server"], "context7");
    }

    #[test]
    fn r1_host_global_silent_when_called_anywhere_on_host() {
        let store = SqliteStore::open_in_memory().unwrap();
        // context7 이 다른 프로젝트 p2 에서 호출됨 → 글로벌 미사용 아님.
        store.upsert_events(&[
            first_turn("p1", 55000),
            mcp_call("p2", "c1", "context7"),
        ]).unwrap();
        store.upsert_inventory("Windows", "*", &[
            McpServer { name: "context7".into(), source: "plugin".into() },
        ]).unwrap();
        assert!(R1UnusedMcp::default().evaluate(&store).unwrap().is_empty());
    }
}
