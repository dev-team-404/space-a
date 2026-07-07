use crate::finding::{Finding, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

pub struct R5RepeatedRead {
    pub threshold: u64,
    pub heuristic_tokens_per_read: u64,
}

impl Default for R5RepeatedRead {
    fn default() -> Self {
        R5RepeatedRead { threshold: 4, heuristic_tokens_per_read: 1200 }
    }
}

impl Rule for R5RepeatedRead {
    fn id(&self) -> &'static str {
        "R5"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id, tool_target, COUNT(*) AS c
             FROM events
             WHERE kind='tool_call' AND tool_kind='file_read' AND tool_target IS NOT NULL
             GROUP BY session_id, tool_target
             HAVING c >= ?1
             ORDER BY c DESC",
        )?;
        let rows = stmt.query_map(params![self.threshold as i64], |r| {
            Ok((
                r.get::<_, String>(0)?, // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(), // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(), // project
                r.get::<_, String>(3)?, // target
                r.get::<_, i64>(4)? as u64, // count
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, target, count) = row?;
            out.push(Finding {
                rule_id: "R5".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({ "path": target, "count": count }),
                est_tokens_saved: (count - 1) * self.heuristic_tokens_per_read,
                prescription: None,
                dedup_key: format!("R5|{session}|{target}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::rules::Rule;
    use crate::store::SqliteStore;

    fn read_event(session: &str, uuid: &str, path: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "test".into(),
            host: "Windows".into(),
            project_id: "c--users-jibin".into(),
            session_id: session.into(),
            uuid: Some(uuid.into()),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::FileRead,
                raw_name: "Read".into(),
                target: Some(path.into()),
                tool_use_id: None,
            },
        }
    }

    #[test]
    fn r5_flags_file_read_4_or_more_times() {
        let store = SqliteStore::open_in_memory().unwrap();
        let evs: Vec<_> = (0..5)
            .map(|i| read_event("s1", &format!("u{i}"), "C:\\a\\report.xlsx"))
            .chain(std::iter::once(read_event("s1", "u9", "C:\\a\\once.txt")))
            .collect();
        store.upsert_events(&evs).unwrap();

        let findings = R5RepeatedRead::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "only report.xlsx (x5) crosses threshold");
        let f = &findings[0];
        assert_eq!(f.rule_id, "R5");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.est_tokens_saved, 4 * 1200); // (5-1)*1200
        assert_eq!(f.evidence["path"], "C:\\a\\report.xlsx");
        assert_eq!(f.evidence["count"], 5);
    }
}
