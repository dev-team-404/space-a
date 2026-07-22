//! B — 모델 적정화(R7 v3)의 LLM 판정. 세션 후보의 사용자 요청 + main-chain 사실로
//! "이 작업이 Opus가 필요했나"를 판정한다. 근거는 사용자 프롬프트·객관 사실뿐 —
//! 에이전트/서브에이전트 산출물·모델선택은 판단 대상 아님(증거 경계).

use crate::judge::{CoachingJudge, PendingCandidate};
use crate::store::SqliteStore;
use anyhow::{anyhow, Result};

pub struct R7Judge;

impl CoachingJudge for R7Judge {
    fn rule_id(&self) -> &'static str { "R7" }

    fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)> {
        let sid = c.evidence.get("session_id").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("R7 evidence에 session_id 없음"))?;
        // 세션의 사용자 프롬프트 전문(최대 5개) — main-chain만 (prompt_events는 이미 sidechain 제외)
        let prompts = store.session_user_prompts(sid, 5)?;
        if prompts.is_empty() {
            return Err(anyhow!("세션 {sid}에 사용자 프롬프트 없음 — 판정 근거 부족"));
        }
        let facts = format!(
            "모델: Opus / 어시스턴트 턴 {} / 도구 호출 {} / 출력 토큰 {}",
            c.evidence.get("opus_turns").and_then(|v| v.as_u64()).unwrap_or(0),
            c.evidence.get("tool_calls").and_then(|v| v.as_u64()).unwrap_or(0),
            c.evidence.get("tok_output").and_then(|v| v.as_u64()).unwrap_or(0),
        );
        let system = "당신은 Claude Code 사용 습관을 코칭하는 심사관입니다. \
사용자가 이 세션에서 시킨 작업이 Opus가 꼭 필요했는지, Sonnet으로 충분했는지 판정하세요.\n\
작업의 '무게'만 보세요 — 에이전트가 잘했는지·서브에이전트 모델 선택은 판단 대상이 아닙니다.\n\
over_modeled=true: 사소·정형 작업(단순 조회·이름변경·짧은 수정 등)이라 Sonnet으로 충분.\n\
over_modeled=false: 복잡한 추론·설계·큰 구현 등 Opus가 정당. **확신이 없으면 false.**\n\
아래 JSON 객체 하나만 출력(코드펜스·사족 금지):\n\
{\"over_modeled\": true 또는 false, \"reason\": \"한국어 한 문장\", \"suggested_model\": \"sonnet\"}"
            .to_string();
        let joined = prompts.iter()
            .map(|p| format!("- \"{}\"", p.replace('"', "'")))
            .collect::<Vec<_>>().join("\n");
        let user = format!("사용자 요청:\n{joined}\n\n객관적 작업량: {facts}");
        Ok((system, user))
    }

    fn classify(&self, verdict: &serde_json::Value) -> &'static str {
        if verdict.get("over_modeled").and_then(|v| v.as_bool()).unwrap_or(false) {
            "confirmed"
        } else {
            "rejected"
        }
    }

    fn rollup(&self, store: &SqliteStore) -> Result<Vec<String>> {
        crate::rules::r7_judge::rollup_project_cards(store) // Task 6에서 구현
    }
}

// Task 6에서 실제 구현으로 교체. 지금은 스텁.
pub(crate) fn rollup_project_cards(_store: &SqliteStore) -> Result<Vec<String>> {
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::CoachingJudge;
    #[test]
    fn r7_judge_classify_maps_over_modeled() {
        let j = R7Judge;
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": true})), "confirmed");
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": false})), "rejected");
        assert_eq!(j.classify(&serde_json::json!({})), "rejected");
        assert_eq!(j.rule_id(), "R7");
    }

    #[test]
    fn r7_judge_prompt_carries_user_request_and_forbids_agent_grading() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            crate::model::NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(), host: "Windows".into(),
                project_id: "p".into(), session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 0, msg_id: None,
                kind: crate::model::EventKind::UserPrompt { preview: "로그 파일 개수만 세줘".into() },
            },
        ]).unwrap();
        let c = PendingCandidate {
            dedup_key: "R7|sess|Windows|s1".into(), scope_host: "Windows".into(),
            scope_project: Some("p".into()),
            evidence: serde_json::json!({"session_id":"s1","opus_turns":1,"tool_calls":1,"tok_output":120}),
            prev_attempts: 0,
        };
        let (system, user) = R7Judge.build_prompt(&store, &c).unwrap();
        assert!(system.contains("over_modeled"));
        assert!(system.contains("무게"), "작업 무게만 보라는 경계 명시");
        assert!(system.contains("서브에이전트"), "subagent 모델 판단 금지 명시");
        assert!(user.contains("개수만 세줘"), "사용자 요청 전문 포함");
    }
}
