//! R11 — 권한 재시도 마찰 (프로젝트 집계, 코칭 v2 스펙 §4.4).
//! 사실: 동일 (도구, 대상) 연속 ≥3회 run. 해석(가설): 권한 거부 후 재시도 —
//! ToolResult 미수집이므로 단정하지 않고 advice에서 사실/가설을 구분한다.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct R11PermissionFriction {
    pub min_run_length: usize,
    pub min_events: usize,
}

impl Default for R11PermissionFriction {
    fn default() -> Self {
        R11PermissionFriction { min_run_length: 3, min_events: 2 }
    }
}

struct Friction {
    session_id: String,
    tool: String,
    target: String,
    run_length: usize,
}

impl Rule for R11PermissionFriction {
    fn id(&self) -> &'static str {
        "R11"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        // "연속" = 세션 내 tool_call을 삽입 순서(id)로 정렬했을 때
        // 사이에 다른 도구 호출이 끼지 않은 run (스펙 §4.4)
        let mut stmt = store.conn.prepare(
            "SELECT session_id, COALESCE(host,''), COALESCE(project_id,''), raw_name, tool_target
             FROM events
             WHERE kind='tool_call' AND tool_target IS NOT NULL AND tool_target <> ''
             ORDER BY session_id, id",
        )?;
        let rows: Vec<(String, String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut frictions: BTreeMap<(String, String), Vec<Friction>> = BTreeMap::new();
        let mut run: Option<(String, String, String, String, String, usize)> = None; // (sess, host, proj, tool, target, len)
        let min_run_length = self.min_run_length;
        let flush = |run: &mut Option<(String, String, String, String, String, usize)>,
                         frictions: &mut BTreeMap<(String, String), Vec<Friction>>| {
            if let Some((sess, host, proj, tool, target, len)) = run.take() {
                if len >= min_run_length {
                    frictions.entry((host, proj)).or_default().push(Friction {
                        session_id: sess, tool, target, run_length: len,
                    });
                }
            }
        };
        for (sess, host, proj, tool, target) in rows {
            let same_run = matches!(&run,
                Some((s, _, _, t, tg, _)) if *s == sess && *t == tool && *tg == target);
            if same_run {
                if let Some((_, _, _, _, _, len)) = &mut run {
                    *len += 1;
                }
            } else {
                flush(&mut run, &mut frictions);
                run = Some((sess, host, proj, tool, target, 1));
            }
        }
        flush(&mut run, &mut frictions);

        let mut out = Vec::new();
        for ((host, project), events) in frictions {
            if events.len() < self.min_events {
                continue;
            }
            let mut by_tool: BTreeMap<&str, u64> = BTreeMap::new();
            for e in &events {
                *by_tool.entry(e.tool.as_str()).or_default() += 1;
            }
            let mut session_ids: Vec<&str> = events.iter().map(|e| e.session_id.as_str()).collect();
            session_ids.dedup();
            let tools: Vec<&str> = by_tool.keys().copied().collect();
            let ev_json: Vec<serde_json::Value> = events
                .iter()
                .take(20)
                .map(|e| serde_json::json!({
                    "tool": e.tool, "target": e.target,
                    "run_length": e.run_length, "session_id": e.session_id,
                }))
                .collect();

            out.push(Finding {
                rule_id: "R11".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "session_ids": session_ids,
                    "total_sessions": session_ids.len(),
                    "friction_events": ev_json,
                    "by_tool": by_tool,
                }),
                est_tokens_saved: 0, // tool_call에 토큰 컬럼 없음 — 근거 없는 수치 금지 (스펙 §4.4)
                prescription: Some(Prescription {
                    kind: "permission_allowlist".into(),
                    payload: serde_json::json!({ "tools": tools }),
                }),
                dedup_key: format!("R11|{host}|{project}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn write_call(session: &str, uuid: &str, target: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::FileWrite, raw_name: "Write".into(),
                target: Some(target.into()),
            },
        }
    }

    #[test]
    fn r11_fires_on_two_friction_events() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            // 세션 s1: Write 3연발 (마찰 1)
            write_call("s1", "u1", "a.rs"), write_call("s1", "u2", "a.rs"), write_call("s1", "u3", "a.rs"),
            // 세션 s2: Write 3연발 (마찰 2)
            write_call("s2", "v1", "b.rs"), write_call("s2", "v2", "b.rs"), write_call("s2", "v3", "b.rs"),
        ]).unwrap();
        let findings = R11PermissionFriction::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R11");
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.dedup_key, "R11|Windows|d--proj");
        assert_eq!(f.est_tokens_saved, 0); // 스펙 §4.4 — 근거 없는 수치 금지
        assert_eq!(f.evidence["friction_events"].as_array().unwrap().len(), 2);
        assert_eq!(f.evidence["by_tool"]["Write"], 2);
        assert_eq!(f.prescription.as_ref().unwrap().kind, "permission_allowlist");
    }

    #[test]
    fn r11_silent_on_single_friction_event() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            write_call("s1", "u1", "a.rs"), write_call("s1", "u2", "a.rs"), write_call("s1", "u3", "a.rs"),
        ]).unwrap();
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r11_run_of_two_is_not_friction() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            write_call("s1", "u1", "a.rs"), write_call("s1", "u2", "a.rs"),
            write_call("s2", "v1", "b.rs"), write_call("s2", "v2", "b.rs"),
        ]).unwrap();
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r11_interleaved_call_breaks_run() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            write_call("s1", "u1", "a.rs"),
            write_call("s1", "u2", "other.rs"), // 다른 target이 사이에 낌 → run 끊김
            write_call("s1", "u3", "a.rs"), write_call("s1", "u4", "a.rs"),
        ]).unwrap();
        // a.rs 최장 run = 2 → 마찰 아님
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }
}
