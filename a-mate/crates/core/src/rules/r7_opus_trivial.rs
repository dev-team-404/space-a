//! R7 v2 — 프로젝트 잔심부름 비율 (코칭 v2 스펙 §4.3).
//! 세션 판정 기준(Opus 전용+출력<700+도구 1~5+무거운도구 0+웹 0)은 현행 유지하되
//! 세션 finding을 만들지 않고, 버스트 세션 제외 후 프로젝트별 비율을 집계한다.
//! 처방은 "다음 세션의 선택" 레버: claude --model sonnet 시작 또는 기본 모델 하향.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::session_stats::{collect_session_stats, detect_bursts};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::{BTreeMap, HashSet};

pub struct R7OpusTrivial {
    pub max_output_tokens: u64,
    pub max_tool_calls: u64,
    pub savings_fraction_pct: u64,
    pub min_sessions: usize,
    pub min_ratio_pct: u64,
}

impl Default for R7OpusTrivial {
    fn default() -> Self {
        R7OpusTrivial {
            max_output_tokens: 700,
            max_tool_calls: 5,
            savings_fraction_pct: 80,
            min_sessions: 3,
            min_ratio_pct: 50,
        }
    }
}

impl Rule for R7OpusTrivial {
    fn id(&self) -> &'static str {
        "R7"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let stats = collect_session_stats(store)?;
        let burst_ids: HashSet<String> = detect_bursts(&stats)
            .into_iter()
            .flat_map(|g| g.sessions.into_iter().map(|s| s.session_id))
            .collect();

        // 버스트 제외 후 (host, project)별 그룹 — 분모·분자 모두 제외 기준(스펙 §4.3)
        let mut by_proj: BTreeMap<(String, String), Vec<&crate::rules::session_stats::SessionStat>> =
            BTreeMap::new();
        for s in stats.iter().filter(|s| !burst_ids.contains(&s.session_id)) {
            by_proj.entry((s.host.clone(), s.project_id.clone())).or_default().push(s);
        }

        let mut out = Vec::new();
        for ((host, project), sessions) in by_proj {
            let mut light: Vec<_> = sessions
                .iter()
                .filter(|s| s.is_light_opus(self.max_output_tokens, self.max_tool_calls))
                .collect();
            let denom = sessions.len();
            if light.len() < self.min_sessions {
                continue;
            }
            let ratio_pct = (light.len() * 100 / denom) as u64;
            if ratio_pct < self.min_ratio_pct {
                continue;
            }

            light.sort_by(|a, b| b.first_ts.cmp(&a.first_ts)); // 최신순
            let billable: u64 = light.iter().map(|s| s.billable).sum();
            let est = (billable * self.savings_fraction_pct + 50) / 100;
            let ids: Vec<&str> = light.iter().take(100).map(|s| s.session_id.as_str()).collect();

            // v3 확장: 호스트 기본 설정을 서사 재료로 동봉 (코칭 v3 §3.3 — 판정에는 미사용)
            let (default_model, effort_level): (Option<String>, Option<String>) = store
                .conn
                .query_row(
                    "SELECT default_model, effort_level FROM host_settings WHERE host=?1",
                    rusqlite::params![host],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((None, None));

            out.push(Finding {
                rule_id: "R7".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "session_ids": ids,
                    "total_sessions": light.len(),
                    "project_session_count": denom,
                    "ratio_pct": ratio_pct,
                    "sum_billable": billable,
                    "default_model": default_model,
                    "effort_level": effort_level,
                    "note": "비용-등가 추정(Opus↔Haiku 5:1 가격비) · 버스트 세션 제외 집계"
                }),
                est_tokens_saved: est,
                prescription: Some(Prescription {
                    kind: "start_with_lighter_model".into(),
                    payload: serde_json::json!({ "to": "sonnet" }),
                }),
                dedup_key: format!("R7|{host}|{project}"),
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

    fn light_opus_session(sid: &str, ts: &str) -> Vec<NormalizedEvent> {
        vec![
            turn_at(sid, &format!("{sid}-u1"), "claude-opus-4-8", 300, ts),
            tool_at(sid, &format!("{sid}-t1"), ToolKind::FileRead, "Read", Some("a.md"), ts),
        ]
    }

    fn heavy_session(sid: &str, ts: &str) -> Vec<NormalizedEvent> {
        vec![
            turn_at(sid, &format!("{sid}-u1"), "claude-opus-4-8", 5000, ts),
            tool_at(sid, &format!("{sid}-t1"), ToolKind::FileEdit, "Edit", Some("b.rs"), ts),
        ]
    }

    #[test]
    fn r7v2_fires_project_aggregate_when_ratio_met() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        // 가벼운 Opus 3건 + 무거운 1건 → 3/4 = 75% ≥ 50%, 3건 ≥ 3 (간격 1시간 → 버스트 아님)
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        evs.extend(light_opus_session("l3", "2026-07-06T13:00:00Z"));
        evs.extend(heavy_session("h1", "2026-07-06T15:00:00Z"));
        store.upsert_events(&evs).unwrap();

        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.dedup_key, "R7|Windows|d--proj");
        assert_eq!(f.evidence["total_sessions"], 3);
        assert_eq!(f.evidence["project_session_count"], 4);
        assert_eq!(f.evidence["ratio_pct"], 75);
        assert_eq!(f.prescription.as_ref().unwrap().kind, "start_with_lighter_model");
    }

    #[test]
    fn r7v2_silent_below_min_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        store.upsert_events(&evs).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty()); // 2건 < 3
    }

    #[test]
    fn r7v2_silent_below_ratio() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        for (i, ts) in ["09", "11", "13"].iter().enumerate() {
            evs.extend(light_opus_session(&format!("l{i}"), &format!("2026-07-06T{ts}:00:00Z")));
        }
        for i in 0..4 {
            evs.extend(heavy_session(&format!("h{i}"), &format!("2026-07-07T{:02}:00:00Z", 9 + i * 2)));
        }
        store.upsert_events(&evs).unwrap();
        // 3/7 = 42% < 50%
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7v2_excludes_burst_sessions_from_both_sides() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        // 버스트 5건(가벼운 Opus, 5분 간격) — R7 집계에서 제외돼야 함
        for i in 0..5 {
            evs.extend(light_opus_session(&format!("b{i}"), &format!("2026-07-06T10:{:02}:00Z", i * 5)));
        }
        // 버스트 아닌 가벼운 Opus 2건 (1시간 간격) — 2건 < 3 → 침묵이어야 함
        evs.extend(light_opus_session("l1", "2026-07-07T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-07T11:00:00Z"));
        store.upsert_events(&evs).unwrap();
        // 버스트를 빼면 2건뿐 → R7 침묵 (버스트 포함이면 7건으로 발화해버림)
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7v2_never_emits_session_scope() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        for (i, ts) in ["09", "11", "13"].iter().enumerate() {
            evs.extend(light_opus_session(&format!("l{i}"), &format!("2026-07-06T{ts}:00:00Z")));
        }
        store.upsert_events(&evs).unwrap();
        for f in R7OpusTrivial::default().evaluate(&store).unwrap() {
            assert_eq!(f.scope_kind, "project");
        }
    }

    #[test]
    fn r7v2_evidence_carries_host_settings_when_present() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.replace_host_settings("Windows", Some("claude-fable-5[1m]"), Some("xhigh"), "2026-07-19T00:00:00Z").unwrap();
        let mut evs = Vec::new();
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        evs.extend(light_opus_session("l3", "2026-07-06T13:00:00Z"));
        store.upsert_events(&evs).unwrap();

        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence["default_model"], "claude-fable-5[1m]");
        assert_eq!(findings[0].evidence["effort_level"], "xhigh");
    }

    #[test]
    fn r7v2_evidence_settings_null_when_absent() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        evs.extend(light_opus_session("l3", "2026-07-06T13:00:00Z"));
        store.upsert_events(&evs).unwrap();
        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert!(findings[0].evidence["default_model"].is_null());
    }
}
