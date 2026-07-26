//! R10 — 자동화 버스트 (프로젝트 집계, 코칭 v2 스펙 §4.2).
//! 보수 판별은 session_stats::detect_bursts가 단일 공급원.
//! 버스트 과반이 Opus 전용일 때만 발화 — 이미 저렴한 모델로 도는 자동화는 침묵.

use crate::finding::{Finding, Severity};
use crate::rules::session_stats::{collect_session_stats, detect_bursts, SessionStat};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct R10AutomationBurst {}

fn median_u64(mut v: Vec<u64>) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn is_opus_only(s: &SessionStat) -> bool {
    s.opus_turns >= 1 && s.non_opus_turns == 0
}

impl Rule for R10AutomationBurst {
    fn id(&self) -> &'static str {
        "R10"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let stats = collect_session_stats(store)?;
        // 과반 Opus 필터는 체인 단위로 — 같은 프로젝트의 저렴한(Haiku) 버스트가
        // 별도의 Opus 버스트를 가리지 않도록 병합 전에 걸러낸다.
        let mut by_proj: BTreeMap<(String, String), Vec<SessionStat>> = BTreeMap::new();
        for g in detect_bursts(&stats) {
            let opus = g.sessions.iter().filter(|s| is_opus_only(s)).count();
            if opus * 2 <= g.sessions.len() {
                continue; // 이 체인은 과반이 Opus 전용이 아님 — 침묵
            }
            by_proj.entry((g.host, g.project_id)).or_default().extend(g.sessions);
        }

        let mut out = Vec::new();
        for ((host, project), mut sessions) in by_proj {
            let opus_count = sessions.iter().filter(|s| is_opus_only(s)).count();

            sessions.sort_by(|a, b| a.first_ts.cmp(&b.first_ts)); // 오름차순
            let gaps: Vec<u64> = sessions
                .windows(2)
                .filter_map(|w| {
                    use chrono::DateTime;
                    let a = DateTime::parse_from_rfc3339(w[0].last_ts.as_deref()?).ok()?;
                    let b = DateTime::parse_from_rfc3339(w[1].first_ts.as_deref()?).ok()?;
                    Some((b - a).num_seconds().max(0) as u64)
                })
                .collect();
            let turns: Vec<u64> = sessions.iter().map(|s| s.assistant_turns).collect();
            let temp_sessions = sessions.iter().filter(|s| s.temp_target_hits > 0).count();
            let temp_ratio_pct = (temp_sessions * 100 / sessions.len()) as u64;
            let mut raws: Vec<&str> =
                sessions.iter().flat_map(|s| s.model_raws.iter().map(String::as_str)).collect();
            raws.sort_unstable();
            raws.dedup();
            let dominant: Option<&str> = if raws.len() == 1 { Some(raws[0]) } else { None };

            let span_from = sessions.first().and_then(|s| s.first_ts.clone());
            let span_to = sessions.last().and_then(|s| s.last_ts.clone());
            let total = sessions.len();
            let ids: Vec<&str> =
                sessions.iter().rev().take(100).map(|s| s.session_id.as_str()).collect(); // 최신순 상한 100
            // §7 식별력: 대표(최신) 세션의 진짜 경로·첫 요청 한 줄 (탐지 로직 무변경, evidence만)
            let rep = ids.first().copied().and_then(|sid| store.session_ctx(sid).ok().flatten());
            let rep_cwd = rep.as_ref().and_then(|r| r.2.clone());
            let rep_first_prompt = rep.as_ref().and_then(|r| r.3.clone());

            out.push(Finding {
                rule_id: "R10".into(),
                severity: Severity::Info,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "session_ids": ids,
                    "total_sessions": total,
                    "opus_session_count": opus_count,
                    "span": { "from": span_from, "to": span_to },
                    "median_gap_secs": median_u64(gaps),
                    "median_turns": median_u64(turns),
                    "temp_hit_ratio_pct": temp_ratio_pct,
                    "dominant_model_raw": dominant,
                    "rep_cwd": rep_cwd,
                    "rep_first_prompt": rep_first_prompt
                }),
                est_tokens_saved: 0,
                prescription: None,
                dedup_key: format!("R10|{host}|{project}"),
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

    fn opus_burst(n: usize, gap_min: u32) -> Vec<NormalizedEvent> {
        let mut evs = Vec::new();
        for i in 0..n {
            let sid = format!("b{i}");
            let ts = format!("2026-07-06T10:{:02}:00Z", i as u32 * gap_min);
            evs.push(turn_at(&sid, &format!("u{i}"), "claude-opus-4-8", 100, &ts));
            evs.push(tool_at(&sid, &format!("t{i}"), ToolKind::FileRead, "Read",
                Some("/tmp/probe.txt"), &ts));
        }
        evs
    }

    #[test]
    fn r10_fires_single_project_card_for_opus_burst() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&opus_burst(5, 5)).unwrap();
        let findings = R10AutomationBurst::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R10");
        assert_eq!(f.scope_kind, "project");
        assert_eq!(f.scope_ref, "d--proj");
        assert_eq!(f.dedup_key, "R10|Windows|d--proj");
        assert_eq!(f.evidence["total_sessions"], 5);
        assert_eq!(f.evidence["opus_session_count"], 5);
        assert_eq!(f.evidence["session_ids"].as_array().unwrap().len(), 5);
        assert_eq!(f.evidence["dominant_model_raw"], "claude-opus-4-8");
        assert!(f.evidence["temp_hit_ratio_pct"].as_u64().unwrap() > 0);
        // v3 §3.2 — 관찰 카드:
        assert_eq!(f.severity, crate::finding::Severity::Info);
        assert_eq!(f.est_tokens_saved, 0);
        assert!(f.prescription.is_none(), "R10은 처방 없는 관찰 카드");
    }

    #[test]
    fn r10_evidence_carries_rep_cwd_and_first_prompt() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = opus_burst(5, 5); // 세션 b0..b4, b4가 최신(ids[0])
        // 최신 세션 b4에 진짜 경로 + 첫 요청 부착 (sessions로 라우팅됨)
        evs.push(NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(), session_id: "b4".into(),
            uuid: Some("b4-meta".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-06T10:20:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 500,
            msg_id: None,
            kind: EventKind::SessionMeta { cwd: "D:\\Project\\cowork\\.worktrees\\probe".into(), git_branch: None },
        });
        evs.push(NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(), session_id: "b4".into(),
            uuid: Some("b4-prompt".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-06T10:20:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 600,
            msg_id: None,
            kind: EventKind::UserPrompt { preview: "이 리포의 최근 커밋 요약해줘".into(), is_command: false },
        });
        store.upsert_events(&evs).unwrap();

        let findings = R10AutomationBurst::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let ev = &findings[0].evidence;
        assert_eq!(ev["rep_cwd"], "D:\\Project\\cowork\\.worktrees\\probe");
        assert_eq!(ev["rep_first_prompt"], "이 리포의 최근 커밋 요약해줘");
    }

    #[test]
    fn r10_silent_when_majority_not_opus() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        for i in 0..5 {
            let sid = format!("h{i}");
            let ts = format!("2026-07-06T10:{:02}:00Z", i * 5);
            let model = if i < 3 { "claude-haiku-4-5" } else { "claude-opus-4-8" };
            evs.push(turn_at(&sid, &format!("u{i}"), model, 100, &ts));
        }
        store.upsert_events(&evs).unwrap();
        assert!(R10AutomationBurst::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r10_silent_when_no_burst() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&opus_burst(4, 5)).unwrap(); // 4건 < 5
        assert!(R10AutomationBurst::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r10_opus_burst_not_suppressed_by_cheap_burst_in_same_project() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        // 오전: Haiku 버스트 5건 (체인 1 — 부적격)
        for i in 0..5 {
            let sid = format!("hk{i}");
            let ts = format!("2026-07-06T09:{:02}:00Z", i * 5);
            evs.push(turn_at(&sid, &format!("hu{i}"), "claude-haiku-4-5", 50, &ts));
        }
        // 오후: Opus 버스트 5건 (체인 2 — 적격, 오전과 3시간 간격이라 별도 체인)
        for i in 0..5 {
            let sid = format!("op{i}");
            let ts = format!("2026-07-06T13:{:02}:00Z", i * 5);
            evs.push(turn_at(&sid, &format!("ou{i}"), "claude-opus-4-8", 50, &ts));
        }
        store.upsert_events(&evs).unwrap();
        let findings = R10AutomationBurst::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1); // 병합-후-검사였다면 5/10 ≤ 과반 미달로 침묵했을 것
        assert_eq!(findings[0].evidence["total_sessions"], 5); // Opus 체인만 포함
        assert_eq!(findings[0].evidence["opus_session_count"], 5);
    }

    #[test]
    fn r10_session_ids_capped_at_100_latest() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 120건 버스트 — 분 단위로 촘촘히
        let mut evs = Vec::new();
        for i in 0..120u32 {
            let sid = format!("c{i:03}");
            let ts = format!("2026-07-06T{:02}:{:02}:00Z", 10 + i / 60, i % 60);
            evs.push(turn_at(&sid, &format!("u{i}"), "claude-opus-4-8", 50, &ts));
        }
        store.upsert_events(&evs).unwrap();
        let findings = R10AutomationBurst::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let ids = findings[0].evidence["session_ids"].as_array().unwrap();
        assert_eq!(ids.len(), 100);
        assert_eq!(findings[0].evidence["total_sessions"], 120);
        assert_eq!(ids[0], "c119"); // 최신순
    }
}
