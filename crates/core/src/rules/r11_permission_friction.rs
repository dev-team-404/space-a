//! R11 — 권한 재시도 마찰 (프로젝트 집계, 코칭 v2.1 스펙 §5).
//! 결정론 판정: 한 세션에서 같은 (raw_name, tool_target)에 `denied` 결과가 있고,
//! 그 **이후** 같은 (raw_name, tool_target)가 `ok`로 실행된 경우 = 마찰 1건.
//! 거부 후 결국 승인 = 사용자가 그 조치를 원했음이 증명됨 → allowlist 처방이 항상 정확.
//! 의도적 거부(재승인 없음)·allow-always(첫 실행 ok)는 자연 제외.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct R11PermissionFriction {
    pub min_events: usize,
}

impl Default for R11PermissionFriction {
    fn default() -> Self {
        R11PermissionFriction { min_events: 2 }
    }
}

struct Friction {
    session_id: String,
    tool: String,
    target: String,
}

impl Rule for R11PermissionFriction {
    fn id(&self) -> &'static str {
        "R11"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        // tool_call ↔ tool_result 를 (session_id, tool_use_id)로 조인.
        // tr.id(결과 도착 순서) 오름차순으로 (도구,대상)별 denied→ok 전이를 판정.
        let mut stmt = store.conn.prepare(
            "SELECT tc.session_id, COALESCE(tc.host,''), COALESCE(tc.project_id,''),
                    tc.raw_name, tc.tool_target, tr.result_status
             FROM events tr
             JOIN events tc
               ON tc.session_id = tr.session_id AND tc.tool_use_id = tr.tool_use_id
             WHERE tr.kind='tool_result' AND tc.kind='tool_call'
               AND tr.tool_use_id IS NOT NULL AND tr.tool_use_id <> ''
               AND tc.tool_target IS NOT NULL AND tc.tool_target <> ''
               AND tc.raw_name IS NOT NULL
             ORDER BY tr.session_id, tr.id",
        )?;
        let rows: Vec<(String, String, String, String, String, Option<String>)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        // (host, project) -> Vec<Friction>. (session, tool, target)별 denied→ok 최초 1회만.
        let mut frictions: BTreeMap<(String, String), Vec<Friction>> = BTreeMap::new();
        // (session, tool, target) -> (seen_denied, counted)
        let mut state: BTreeMap<(String, String, String), (bool, bool)> = BTreeMap::new();
        for (sess, host, proj, tool, target, status) in rows {
            let entry = state.entry((sess.clone(), tool.clone(), target.clone())).or_insert((false, false));
            match status.as_deref() {
                Some("denied") => entry.0 = true,
                Some("ok") => {
                    if entry.0 && !entry.1 {
                        entry.1 = true;
                        frictions.entry((host, proj)).or_default().push(Friction {
                            session_id: sess, tool, target,
                        });
                    }
                }
                _ => {} // error 등은 무시
            }
        }

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
            session_ids.sort_unstable();
            session_ids.dedup();
            let total_sessions = session_ids.len();
            session_ids.truncate(100);
            let tools: Vec<&str> = by_tool.keys().copied().collect();
            let ev_json: Vec<serde_json::Value> = events
                .iter()
                .take(20)
                .map(|e| serde_json::json!({
                    "tool": e.tool, "target": e.target, "session_id": e.session_id,
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
                    "total_sessions": total_sessions,
                    "friction_events": ev_json,
                    "friction_events_count": events.len(),
                    "by_tool": by_tool,
                }),
                est_tokens_saved: 0, // tool_call에 토큰 컬럼 없음 — 근거 없는 수치 금지(스펙 §5)
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

    fn call(session: &str, off: u64, tuid: &str, raw: &str, target: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{tuid}-c")), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: off,
            kind: EventKind::ToolCall {
                kind: ToolKind::from_raw_name(raw), raw_name: raw.into(),
                target: Some(target.into()), tool_use_id: Some(tuid.into()),
            },
        }
    }
    fn result(session: &str, off: u64, tuid: &str, status: ResultStatus) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{tuid}-r")), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: off,
            kind: EventKind::ToolResult { tool_use_id: tuid.into(), status },
        }
    }

    /// 세션에서 (raw, target)이 denied 후 ok로 실행 = 마찰 1건. tuid는 호출마다 고유.
    fn friction_session(session: &str, raw: &str, target: &str) -> Vec<NormalizedEvent> {
        vec![
            call(session, 0, &format!("{session}-a"), raw, target),
            result(session, 1, &format!("{session}-a"), ResultStatus::Denied),
            call(session, 2, &format!("{session}-b"), raw, target),
            result(session, 3, &format!("{session}-b"), ResultStatus::Ok),
        ]
    }

    #[test]
    fn r11_fires_on_two_denied_then_approved_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = friction_session("s1", "Write", "a.rs");
        evs.extend(friction_session("s2", "Write", "b.rs"));
        store.upsert_events(&evs).unwrap();

        let findings = R11PermissionFriction::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R11");
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.dedup_key, "R11|Windows|d--proj");
        assert_eq!(f.est_tokens_saved, 0);
        assert_eq!(f.evidence["friction_events_count"], 2);
        assert_eq!(f.evidence["friction_events"].as_array().unwrap().len(), 2);
        assert_eq!(f.evidence["by_tool"]["Write"], 2);
        assert_eq!(f.evidence["total_sessions"], 2);
        assert_eq!(f.prescription.as_ref().unwrap().kind, "permission_allowlist");
        assert_eq!(f.prescription.as_ref().unwrap().payload["tools"][0], "Write");
    }

    #[test]
    fn r11_silent_on_denied_only_no_reapproval() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 두 세션 모두 거부만(재승인 없음) → 의도적 거부, 마찰 아님
        store.upsert_events(&[
            call("s1", 0, "s1-a", "Write", "a.rs"),
            result("s1", 1, "s1-a", ResultStatus::Denied),
            call("s2", 0, "s2-a", "Write", "b.rs"),
            result("s2", 1, "s2-a", ResultStatus::Denied),
        ]).unwrap();
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r11_silent_on_ok_only_allow_always() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 첫 실행부터 ok(거부 기록 없음) → allow-always, 자연 침묵
        store.upsert_events(&[
            call("s1", 0, "s1-a", "Write", "a.rs"),
            result("s1", 1, "s1-a", ResultStatus::Ok),
            call("s2", 0, "s2-a", "Write", "b.rs"),
            result("s2", 1, "s2-a", ResultStatus::Ok),
        ]).unwrap();
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r11_silent_on_single_friction_event() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&friction_session("s1", "Write", "a.rs")).unwrap();
        // 마찰 1건뿐 < min_events(2)
        assert!(R11PermissionFriction::default().evaluate(&store).unwrap().is_empty());
    }
}
