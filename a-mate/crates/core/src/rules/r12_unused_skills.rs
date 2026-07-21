//! R12 — 설치 스킬 미활용 (프로젝트 집계, 코칭 v2 스펙 §4.5, 킥오프 6①).
//! 가치 제안형: 절약이 아닌 "이 스킬을 쓰면 더 잘 됩니다". est_tokens_saved = 0.
//! 미설치 스킬 추천 없음(설치 확인 필수) — 6②는 SkillRecommendationSource 어댑터로 후속.

use crate::curation::{BuiltinCurationSource, SkillRecommendationSource, WorkPattern};
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::session_stats::collect_session_stats;
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct R12UnusedSkills {
    pub min_file_edits: u64,
    pub min_sessions: usize,
}

impl Default for R12UnusedSkills {
    fn default() -> Self {
        R12UnusedSkills { min_file_edits: 10, min_sessions: 2 }
    }
}

/// host의 설치 스킬 전체 이름 목록 (plugin_inventory.skills_json 평탄화)
fn installed_skills(store: &SqliteStore, host: &str) -> Result<Vec<String>> {
    let mut stmt = store
        .conn
        .prepare("SELECT skills_json FROM plugin_inventory WHERE host=?1")?;
    let rows: Vec<String> = stmt
        .query_map(rusqlite::params![host], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut out = Vec::new();
    for json in rows {
        if let Ok(skills) = serde_json::from_str::<Vec<String>>(&json) {
            out.extend(skills);
        }
    }
    Ok(out)
}

impl Rule for R12UnusedSkills {
    fn id(&self) -> &'static str {
        "R12"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let stats = collect_session_stats(store)?;
        let mut by_proj: BTreeMap<(String, String), Vec<&crate::rules::session_stats::SessionStat>> =
            BTreeMap::new();
        for s in stats
            .iter()
            .filter(|s| s.file_edits >= self.min_file_edits && s.skill_calls == 0)
        {
            by_proj.entry((s.host.clone(), s.project_id.clone())).or_default().push(s);
        }

        let source = BuiltinCurationSource;
        let mut out = Vec::new();
        for ((host, project), mut sessions) in by_proj {
            if sessions.len() < self.min_sessions {
                continue;
            }
            let installed = installed_skills(store, &host)?;
            let matched: Vec<&'static str> = source
                .recommend(&WorkPattern::LargeImplNoSkill)
                .into_iter()
                .filter(|rec| {
                    installed.iter().any(|sk| {
                        rec.match_substrings.iter().any(|sub| sk.contains(sub))
                    })
                })
                .map(|rec| rec.display)
                .collect();
            if matched.is_empty() {
                continue; // 미설치 추천 없음 (스펙 §4.5)
            }

            sessions.sort_by(|a, b| b.first_ts.cmp(&a.first_ts)); // 최신순
            let total = sessions.len();
            let ids: Vec<&str> = sessions.iter().take(100).map(|s| s.session_id.as_str()).collect();

            out.push(Finding {
                rule_id: "R12".into(),
                severity: Severity::Info,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "session_ids": ids,
                    "total_sessions": total,
                    "pattern": "large_impl_no_skill",
                    "recommended_skills": matched.clone(),
                }),
                est_tokens_saved: 0, // 가치 제안형 — 절약 주장 안 함
                prescription: Some(Prescription {
                    kind: "use_skill".into(),
                    payload: serde_json::json!({ "skills": matched }),
                }),
                dedup_key: format!("R12|{host}|{project}"),
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

    fn turn_at(session: &str, uuid: &str, model: &str, output: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { output, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn tool_at(session: &str, uuid: &str, kind: ToolKind, raw: &str, target: Option<&str>, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::ToolCall { kind, raw_name: raw.into(), target: target.map(String::from), tool_use_id: None },
        }
    }

    /// 대형 구현 세션: file_edit 10회, Skill 0회
    fn large_impl_session(sid: &str, ts: &str) -> Vec<NormalizedEvent> {
        let mut evs = vec![turn_at(sid, &format!("{sid}-u"), "claude-opus-4-8", 3000, ts)];
        for i in 0..10 {
            evs.push(tool_at(sid, &format!("{sid}-t{i}"), ToolKind::FileEdit, "Edit",
                Some(&format!("f{i}.rs")), ts));
        }
        evs
    }

    fn install_superpowers(store: &mut SqliteStore) {
        store.replace_plugin_inventory("Windows", &[PluginRecord {
            plugin_key: "superpowers@claude-plugins-official".into(),
            namespace: "superpowers".into(),
            skill_count: 3,
            resident_tokens: 900,
            skills: vec![
                "superpowers:brainstorming".into(),
                "superpowers:writing-plans".into(),
                "superpowers:subagent-driven-development".into(),
            ],
            mcp_servers: vec![],
        }]).unwrap();
    }

    #[test]
    fn r12_fires_when_pattern_matches_and_skills_installed() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        install_superpowers(&mut store);
        let mut evs = Vec::new();
        evs.extend(large_impl_session("s1", "2026-07-06T09:00:00Z"));
        evs.extend(large_impl_session("s2", "2026-07-06T14:00:00Z"));
        store.upsert_events(&evs).unwrap();

        let findings = R12UnusedSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R12");
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.dedup_key, "R12|Windows|d--proj");
        assert_eq!(f.est_tokens_saved, 0); // 가치 제안형 — 절약 주장 안 함 (스펙 §4.5)
        assert_eq!(f.evidence["total_sessions"], 2);
        assert_eq!(f.evidence["pattern"], "large_impl_no_skill");
        assert!(f.evidence["recommended_skills"].as_array().unwrap().len() >= 1);
        assert_eq!(f.prescription.as_ref().unwrap().kind, "use_skill");
    }

    #[test]
    fn r12_silent_when_skills_not_installed() {
        let store = SqliteStore::open_in_memory().unwrap(); // 인벤토리 없음
        let mut evs = Vec::new();
        evs.extend(large_impl_session("s1", "2026-07-06T09:00:00Z"));
        evs.extend(large_impl_session("s2", "2026-07-06T14:00:00Z"));
        store.upsert_events(&evs).unwrap();
        assert!(R12UnusedSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r12_silent_below_min_sessions() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        install_superpowers(&mut store);
        store.upsert_events(&large_impl_session("s1", "2026-07-06T09:00:00Z")).unwrap();
        assert!(R12UnusedSkills::default().evaluate(&store).unwrap().is_empty()); // 1건 < 2
    }

    #[test]
    fn r12_skill_using_session_does_not_match() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        install_superpowers(&mut store);
        let mut evs = Vec::new();
        for sid in ["s1", "s2"] {
            evs.extend(large_impl_session(sid, "2026-07-06T09:00:00Z"));
            evs.push(tool_at(sid, &format!("{sid}-sk"),
                ToolKind::Skill { name: "superpowers:writing-plans".into() },
                "Skill", Some("superpowers:writing-plans"), "2026-07-06T09:01:00Z"));
        }
        store.upsert_events(&evs).unwrap();
        assert!(R12UnusedSkills::default().evaluate(&store).unwrap().is_empty());
    }
}
