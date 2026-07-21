use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

/// R2 — 미사용 플러그인(스킬 제공).
///
/// enabled + 스킬≥1 제공 플러그인 중, 그 플러그인의 스킬·MCP를 하나도 안 쓴 것을 host 단위로 지목.
/// - "사용됨" = 그 플러그인 네임스페이스의 스킬 호출 ≥1 OR 그 플러그인 MCP 서버 호출 ≥1.
/// - R1 비충돌: MCP 서버가 쓰이면 침묵("쓰는 플러그인 끄기" 방지). MCP 전용 미사용은 R1(서버 단위)이 담당.
/// - 스코프 host-global(enabledPlugins·스킬은 호스트 전 세션 상주). 상주토큰은 chars/4 heuristic.
pub struct R2UnusedPluginSkills {
    pub min_resident_tokens: u64,
}

impl Default for R2UnusedPluginSkills {
    fn default() -> Self {
        R2UnusedPluginSkills { min_resident_tokens: 300 }
    }
}

impl Rule for R2UnusedPluginSkills {
    fn id(&self) -> &'static str {
        "R2"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT host, plugin_key, namespace, skill_count, resident_tokens, skills_json, mcp_servers_json
             FROM plugin_inventory
             ORDER BY resident_tokens DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,      // host
                    r.get::<_, String>(1)?,      // plugin_key
                    r.get::<_, String>(2)?,      // namespace
                    r.get::<_, i64>(3)? as u64,  // skill_count
                    r.get::<_, i64>(4)? as u64,  // resident_tokens
                    r.get::<_, String>(5)?,      // skills_json
                    r.get::<_, String>(6)?,      // mcp_servers_json
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut out = Vec::new();
        for (host, plugin_key, namespace, skill_count, resident, skills_json, mcp_json) in rows {
            // 1) 스킬 사용? (LIKE 대신 substr 접두사 비교 — ns의 `_`/`%`가 와일드카드로 오작동 방지)
            let prefix = format!("{namespace}:");
            let skill_used: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM events
                 WHERE host=?1 AND kind='tool_call' AND tool_kind='skill'
                   AND substr(tool_target, 1, length(?2)) = ?2",
                params![host, prefix],
                |r| r.get(0),
            )?;
            if skill_used > 0 {
                continue;
            }
            // 2) MCP 사용? (R1 비충돌)
            let servers: Vec<String> = serde_json::from_str(&mcp_json).unwrap_or_default();
            let mut mcp_used = false;
            for srv in &servers {
                let n: i64 = store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                    params![host, srv],
                    |r| r.get(0),
                )?;
                if n > 0 {
                    mcp_used = true;
                    break;
                }
            }
            if mcp_used {
                continue;
            }
            // 3) 임계값
            if resident < self.min_resident_tokens {
                continue;
            }

            let skills: Vec<String> = serde_json::from_str(&skills_json).unwrap_or_default();
            out.push(Finding {
                rule_id: "R2".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: None,
                scope_kind: "host".into(),
                scope_ref: host.clone(),
                evidence: serde_json::json!({
                    "plugin": plugin_key,
                    "skill_count": skill_count,
                    "skills": skills,
                    "resident_tokens": resident,
                    "note": "약(~) 추정 — chars/4 heuristic"
                }),
                est_tokens_saved: resident,
                prescription: Some(Prescription {
                    kind: "disable_plugin".into(),
                    payload: serde_json::json!({ "plugin": plugin_key }),
                }),
                dedup_key: format!("R2|{host}|{plugin_key}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::PluginRecord;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn skill_call(host: &str, uuid: &str, skill: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            msg_id: None,
            kind: EventKind::ToolCall {
                kind: ToolKind::Skill { name: skill.into() },
                raw_name: "Skill".into(), target: Some(skill.into()), tool_use_id: None,
            },
        }
    }

    fn mcp_call(host: &str, uuid: &str, server: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            msg_id: None,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "x".into() },
                raw_name: format!("mcp__{server}__x"), target: None, tool_use_id: None,
            },
        }
    }

    fn rec(plugin_key: &str, namespace: &str, resident: u64, skills: &[&str], mcp: &[&str]) -> PluginRecord {
        PluginRecord {
            plugin_key: plugin_key.into(), namespace: namespace.into(),
            skill_count: skills.len() as u64, resident_tokens: resident,
            skills: skills.iter().map(|s| s.to_string()).collect(),
            mcp_servers: mcp.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn r2_flags_unused_skill_plugin() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming", "tdd"], &[]),
        ]).unwrap();
        // 다른 플러그인 스킬만 호출됨 → superpowers 미사용
        store.upsert_events(&[skill_call("Windows", "u1", "vercel:deploy")]).unwrap();

        let findings = R2UnusedPluginSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R2");
        assert_eq!(f.scope_kind, "host");
        assert_eq!(f.scope_ref, "Windows");
        assert_eq!(f.evidence["plugin"], "superpowers@mp");
        assert_eq!(f.evidence["skill_count"], 2);
        assert_eq!(f.est_tokens_saved, 900);
        let p = f.prescription.as_ref().unwrap();
        assert_eq!(p.kind, "disable_plugin");
        assert_eq!(p.payload["plugin"], "superpowers@mp");
    }

    #[test]
    fn r2_silent_when_skill_used() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.upsert_events(&[skill_call("Windows", "u1", "superpowers:brainstorming")]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_silent_when_plugin_mcp_used() {
        // R1 비충돌: 플러그인 MCP 서버가 쓰이면 R2 침묵.
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("vercel@mp", "vercel", 900, &["deploy"], &["vercel"]),
        ]).unwrap();
        store.upsert_events(&[mcp_call("Windows", "u1", "vercel")]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_silent_under_threshold() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        // 상주 100 < 300
        store.replace_plugin_inventory("Windows", &[
            rec("tiny@mp", "tiny", 100, &["one"], &[]),
        ]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_scoped_per_host() {
        // 같은 플러그인이 두 호스트에 상주, wsl에서만 스킬 사용 → Windows만 지목.
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.replace_plugin_inventory("wsl:Ubuntu", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.upsert_events(&[skill_call("wsl:Ubuntu", "u1", "superpowers:brainstorming")]).unwrap();

        let findings = R2UnusedPluginSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope_ref, "Windows");
    }

    #[test]
    fn r2_underscore_in_namespace_not_wildcard() {
        // ns의 `_`가 SQL 와일드카드로 해석되면 "myxplugin:foo"가 my_plugin 사용으로
        // 오매칭되어 침묵(false-negative)한다 — substr 접두사 비교로 지목되어야 함.
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("my_plugin@mp", "my_plugin", 900, &["one"], &[]),
        ]).unwrap();
        store.upsert_events(&[skill_call("Windows", "u1", "myxplugin:foo")]).unwrap();

        let findings = R2UnusedPluginSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "다른 플러그인 이벤트에 오매칭되어 침묵하면 안 됨");
        assert_eq!(findings[0].evidence["plugin"], "my_plugin@mp");
    }
}
