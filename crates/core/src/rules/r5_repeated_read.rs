//! R5 — 반복 파일 재확인 (프로젝트 집계, 코칭 v2.1 스펙 §6).
//! PR②는 subtype `within_session_context_drift`만 구현(§6.2). cross_session_claude_md(§6.1)는 PR③.
//! context_drift: 세션 안에서 같은 파일을 편집 없이 여러 번 다시 읽는 패턴 = 작업 기억이 흐려진 관찰.
//! 오탐 억제(핵심): 편집 후·검색 직후·compaction 직후 재읽기는 드리프트로 세지 않는다.
//! 이벤트 스트림은 tool_call+compaction만 훑는다("직전 이벤트"의 근접 기준 — 스펙 §6.2).

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::{BTreeMap, HashSet};

const TOKENS_PER_READ: u64 = 1200;

pub struct R5RepeatedRead {
    pub drift_min_reads: u64, // 세션 내 한 파일의 드리프트 읽기 임계
    pub min_sessions: usize,  // 프로젝트에서 드리프트를 보인 세션 수 임계
}

impl Default for R5RepeatedRead {
    fn default() -> Self {
        R5RepeatedRead { drift_min_reads: 3, min_sessions: 2 }
    }
}

/// 한 세션에서 어떤 target이 드리프트(드리프트 읽기 ≥ 임계)로 판정된 결과.
struct DriftSession {
    session_id: String,
    host: String,
    project: String,
    path: String,
    drift_reads: u64,
}

/// 세션 종료 시 임계 넘긴 (target, drift_reads)를 DriftSession으로 flush.
fn flush_session(
    session: &str, host: &str, proj: &str,
    drift_reads: &BTreeMap<String, u64>, min: u64,
    out: &mut Vec<DriftSession>,
) {
    for (path, &n) in drift_reads {
        if n >= min {
            out.push(DriftSession {
                session_id: session.to_string(), host: host.to_string(),
                project: proj.to_string(), path: path.clone(), drift_reads: n,
            });
        }
    }
}

impl Rule for R5RepeatedRead {
    fn id(&self) -> &'static str {
        "R5"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, COALESCE(host,''), COALESCE(project_id,''),
                    kind, COALESCE(tool_kind,''), COALESCE(tool_target,'')
             FROM events
             WHERE kind IN ('tool_call','compaction')
             ORDER BY session_id, id",
        )?;
        let rows: Vec<(String, String, String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        // 세션 순차 훑기 → 세션별 드리프트 집계
        let mut drift: Vec<DriftSession> = Vec::new();
        let mut cur_session = String::new();
        let mut cur_host = String::new();
        let mut cur_proj = String::new();
        let mut edited: HashSet<String> = HashSet::new();
        let mut compaction_count: u64 = 0;
        let mut last_read_compaction: BTreeMap<String, u64> = BTreeMap::new();
        let mut drift_reads: BTreeMap<String, u64> = BTreeMap::new();
        let mut prev_was_search = false;

        for (sess, host, proj, kind, tool_kind, target) in rows {
            if sess != cur_session {
                if !cur_session.is_empty() {
                    flush_session(&cur_session, &cur_host, &cur_proj, &drift_reads, self.drift_min_reads, &mut drift);
                }
                cur_session = sess;
                cur_host = host;
                cur_proj = proj;
                edited.clear();
                compaction_count = 0;
                last_read_compaction.clear();
                drift_reads.clear();
                prev_was_search = false;
            }
            if kind == "compaction" {
                compaction_count += 1;
                prev_was_search = false;
                continue;
            }
            // tool_call
            match tool_kind.as_str() {
                "file_read" => {
                    if !target.is_empty() {
                        let post_compaction = matches!(
                            last_read_compaction.get(&target),
                            Some(&b) if compaction_count > b
                        );
                        if !edited.contains(&target) && !prev_was_search && !post_compaction {
                            *drift_reads.entry(target.clone()).or_default() += 1;
                        }
                        last_read_compaction.insert(target, compaction_count);
                    }
                    prev_was_search = false;
                }
                "file_edit" | "file_write" => {
                    if !target.is_empty() {
                        edited.insert(target);
                    }
                    prev_was_search = false;
                }
                "search" => {
                    prev_was_search = true;
                }
                _ => {
                    prev_was_search = false;
                }
            }
        }
        if !cur_session.is_empty() {
            flush_session(&cur_session, &cur_host, &cur_proj, &drift_reads, self.drift_min_reads, &mut drift);
        }

        // (host, project)별 집계 → 프로젝트 카드
        let mut by_proj: BTreeMap<(String, String), Vec<DriftSession>> = BTreeMap::new();
        for d in drift {
            by_proj.entry((d.host.clone(), d.project.clone())).or_default().push(d);
        }

        let mut out = Vec::new();
        for ((host, project), sessions) in by_proj {
            let mut sess_ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
            sess_ids.sort_unstable();
            sess_ids.dedup();
            if sess_ids.len() < self.min_sessions {
                continue;
            }
            let total_sessions = sess_ids.len();
            let capped: Vec<&str> = sess_ids.iter().take(100).copied().collect();
            let est: u64 = sessions.iter().map(|s| s.drift_reads.saturating_sub(1) * TOKENS_PER_READ).sum();
            let sessions_json: Vec<serde_json::Value> = sessions
                .iter()
                .take(100)
                .map(|s| serde_json::json!({
                    "session_id": s.session_id, "path": s.path, "drift_reads": s.drift_reads,
                }))
                .collect();
            let cwd = capped
                .first()
                .copied()
                .and_then(|sid| store.session_ctx(sid).ok().flatten())
                .and_then(|(_, _, cwd, _)| cwd);

            out.push(Finding {
                rule_id: "R5".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "subtype": "within_session_context_drift",
                    "user_actionability": "medium",
                    "sessions": sessions_json,
                    "session_ids": capped,
                    "total_sessions": total_sessions,
                    "cwd": cwd,
                }),
                est_tokens_saved: est,
                prescription: Some(Prescription {
                    kind: "next_session_habit".into(),
                    payload: serde_json::json!({ "tips": ["file_map", "scoped_read"] }),
                }),
                dedup_key: format!("R5|{host}|{project}|within_session_context_drift"),
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

    fn ev(session: &str, off: u64, kind: EventKind) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-{off}")), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: off, kind,
        }
    }
    fn read(session: &str, off: u64, path: &str) -> NormalizedEvent {
        ev(session, off, EventKind::ToolCall {
            kind: ToolKind::FileRead, raw_name: "Read".into(),
            target: Some(path.into()), tool_use_id: Some(format!("{session}-{off}")),
        })
    }
    fn edit(session: &str, off: u64, path: &str) -> NormalizedEvent {
        ev(session, off, EventKind::ToolCall {
            kind: ToolKind::FileEdit, raw_name: "Edit".into(),
            target: Some(path.into()), tool_use_id: Some(format!("{session}-{off}")),
        })
    }
    fn search(session: &str, off: u64) -> NormalizedEvent {
        ev(session, off, EventKind::ToolCall {
            kind: ToolKind::Search, raw_name: "Grep".into(),
            target: None, tool_use_id: Some(format!("{session}-{off}")),
        })
    }
    fn compaction(session: &str, off: u64) -> NormalizedEvent {
        ev(session, off, EventKind::Compaction)
    }

    /// 순수 재읽기 3회(편집·검색·compaction 없음)를 하는 세션.
    fn pure_drift(session: &str, path: &str) -> Vec<NormalizedEvent> {
        vec![read(session, 0, path), read(session, 1, path), read(session, 2, path)]
    }

    #[test]
    fn r5_pure_rereads_3x_two_sessions_fires() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = pure_drift("s1", "a.rs");
        evs.extend(pure_drift("s2", "a.rs"));
        store.upsert_events(&evs).unwrap();

        let findings = R5RepeatedRead::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R5");
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.dedup_key, "R5|Windows|d--proj|within_session_context_drift");
        assert_eq!(f.evidence["subtype"], "within_session_context_drift");
        assert_eq!(f.evidence["user_actionability"], "medium");
        assert_eq!(f.evidence["total_sessions"], 2);
        assert_eq!(f.evidence["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(f.evidence["sessions"][0]["drift_reads"], 3);
        assert_eq!(f.est_tokens_saved, (3 - 1) * 1200 * 2); // 두 세션 각 (3-1)*1200
        assert_eq!(f.prescription.as_ref().unwrap().kind, "next_session_habit");
    }

    #[test]
    fn r5_edit_then_read_suppressed() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 두 세션 모두 편집 후 읽기 3회 → 전부 억제(verify_after_edit) → 발화 없음
        let sess = |s: &str| vec![
            edit(s, 0, "a.rs"), read(s, 1, "a.rs"), read(s, 2, "a.rs"), read(s, 3, "a.rs"),
        ];
        let mut evs = sess("s1"); evs.extend(sess("s2"));
        store.upsert_events(&evs).unwrap();
        assert!(R5RepeatedRead::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r5_search_then_read_suppressed() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 매 읽기 직전에 search → search_then_read 억제
        let sess = |s: &str| vec![
            search(s, 0), read(s, 1, "a.rs"),
            search(s, 2), read(s, 3, "a.rs"),
            search(s, 4), read(s, 5, "a.rs"),
        ];
        let mut evs = sess("s1"); evs.extend(sess("s2"));
        store.upsert_events(&evs).unwrap();
        assert!(R5RepeatedRead::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r5_post_compaction_reread_suppressed() {
        let store = SqliteStore::open_in_memory().unwrap();
        // read, (compaction, read)×3 → 첫 읽기만 카운트, compaction 직후 재읽기는 매번 억제
        let sess = |s: &str| vec![
            read(s, 0, "a.rs"),
            compaction(s, 1), read(s, 2, "a.rs"),
            compaction(s, 3), read(s, 4, "a.rs"),
            compaction(s, 5), read(s, 6, "a.rs"),
        ];
        let mut evs = sess("s1"); evs.extend(sess("s2"));
        store.upsert_events(&evs).unwrap();
        assert!(R5RepeatedRead::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r5_needs_two_drift_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 드리프트 세션 1건뿐 < min_sessions(2)
        store.upsert_events(&pure_drift("s1", "a.rs")).unwrap();
        assert!(R5RepeatedRead::default().evaluate(&store).unwrap().is_empty());
    }
}
