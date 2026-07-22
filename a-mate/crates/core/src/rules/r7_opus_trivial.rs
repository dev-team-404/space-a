//! R7 — 세션 후보 채굴 (코칭 v3 재설계 §4.3).
//! 관찰창(14일) 내 main-chain Opus 세션마다 후보 finding을 낸다(세션당 1건).
//! sub_agent 등 무거운 도구를 쓴 세션·버스트 세션은 "실질 작업"으로 보아 제외.
//! LLM 판정(후속 태스크)이 이 후보들을 심사해 프로젝트 롤업 카드로 승격시킨다.

use crate::finding::{Finding, Severity};
use crate::rules::session_stats::{collect_session_stats, detect_bursts};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::HashSet;

#[derive(Default)]
pub struct R7OpusTrivial;

impl Rule for R7OpusTrivial {
    fn id(&self) -> &'static str {
        "R7"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
        let stats = collect_session_stats(store)?; // Task 1: main-chain only
        let burst_ids: HashSet<String> = detect_bursts(&stats)
            .into_iter()
            .flat_map(|g| g.sessions.into_iter().map(|s| s.session_id))
            .collect();

        let mut out = Vec::new();
        for s in &stats {
            // 후보 조건: main-chain Opus 사용 + subagent/버스트 아님 + 관찰창 내
            if s.opus_turns == 0 { continue; }
            if s.heavy_tools > 0 { continue; }          // sub_agent 등 무거운 도구 = 실질 작업
            if burst_ids.contains(&s.session_id) { continue; }
            match s.first_ts.as_deref() {
                Some(ts) if ts >= cutoff.as_str() => {}
                _ => continue,                            // 관찰창 밖 or ts 없음
            }
            out.push(Finding {
                rule_id: "R7".into(),
                severity: Severity::Suggest,
                scope_host: Some(s.host.clone()),
                scope_project: Some(s.project_id.clone()),
                scope_kind: "session".into(),
                scope_ref: s.session_id.clone(),
                evidence: serde_json::json!({
                    "session_id": s.session_id,
                    "host": s.host,
                    "project": s.project_id,
                    "opus_turns": s.opus_turns,
                    "tok_output": s.tok_output,
                    "tool_calls": s.tool_calls,
                    "first_ts": s.first_ts,
                }),
                est_tokens_saved: 0,
                prescription: None, // 처방은 프로젝트 롤업 카드에만
                dedup_key: format!("R7|sess|{}|{}", s.host, s.session_id),
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

    /// 관찰창(14일) 안쪽의 상대 타임스탬프 — 하드코딩 날짜가 창 밖으로 밀려나 테스트가
    /// 썩는 것을 막는다 (r6_repeated_prompts.rs의 ts_at 패턴과 동일).
    fn recent(h: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339()
    }

    #[test]
    fn r7_mines_session_candidates_for_main_chain_opus() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        // Opus 세션 2개(1시간 이상 간격 → 버스트 아님, 관찰창 내)
        evs.extend(light_opus_session("s1", &recent(3)));
        evs.extend(light_opus_session("s2", &recent(1)));
        store.upsert_events(&evs).unwrap();
        let f = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(f.len(), 2, "Opus 세션마다 후보 1건");
        assert!(f.iter().all(|x| x.scope_kind == "session"));
        assert!(f.iter().any(|x| x.dedup_key == "R7|sess|Windows|s1"));
        assert_eq!(f[0].est_tokens_saved, 0);
        assert!(f.iter().all(|x| x.evidence.get("session_id").is_some()));
    }

    #[test]
    fn r7_excludes_subagent_and_burst_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        // sub_agent 쓴 Opus 세션 → 제외
        evs.push(turn_at("sa", "sa-u", "claude-opus-4-8", 300, &recent(5)));
        evs.push(tool_at("sa", "sa-t", ToolKind::SubAgent, "Task", None, &recent(5)));
        // 비Opus 세션 → 제외 (opus_turns 0)
        evs.push(turn_at("hk", "hk-u", "claude-haiku-4-5", 100, &recent(2)));
        store.upsert_events(&evs).unwrap();
        let f = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert!(f.iter().all(|x| x.evidence["session_id"] != "sa"), "sub_agent 세션 제외");
        assert!(f.iter().all(|x| x.evidence["session_id"] != "hk"), "비Opus 세션 제외");
    }
}
