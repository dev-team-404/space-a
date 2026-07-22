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
        rollup_project_cards(store)
    }
}

pub(crate) fn rollup_project_cards(store: &SqliteStore) -> Result<Vec<String>> {
    use crate::finding::{Finding, Prescription, Severity};
    // (host, project)별 confirmed/판정합 집계
    let mut stmt = store.conn.prepare(
        "SELECT COALESCE(scope_host,''), COALESCE(scope_project,''),
                SUM(CASE WHEN status='confirmed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('confirmed','rejected') THEN 1 ELSE 0 END),
                GROUP_CONCAT(CASE WHEN status='confirmed'
                    THEN json_extract(evidence_json,'$.session_id') END)
         FROM findings
         WHERE rule_id='R7' AND scope_kind='session'
         GROUP BY scope_host, scope_project",
    )?;
    let rows: Vec<(String, String, i64, i64, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut qualifying: Vec<(String, String, i64, Vec<String>)> = Vec::new();
    for (host, proj, confirmed, judged, sids) in rows {
        if confirmed >= 3 && confirmed * 2 > judged {
            let examples: Vec<String> = sids.unwrap_or_default()
                .split(',').filter(|s| !s.is_empty()).take(5).map(String::from).collect();
            qualifying.push((host, proj, confirmed, examples));
        }
    }

    // 비자격 stale 'new' 프로젝트 카드 제거 (구 통계 카드 마이그레이션 포함).
    // 자격 세트는 소규모라 개별 확인.
    let qualifying_keys: std::collections::HashSet<String> =
        qualifying.iter().map(|(h, p, _, _)| format!("R7|{h}|{p}")).collect();
    let mut stale = store.conn.prepare(
        "SELECT dedup_key FROM findings WHERE rule_id='R7' AND scope_kind='project' AND status='new'",
    )?;
    let existing: Vec<String> = stale.query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    for key in &existing {
        if !qualifying_keys.contains(key) {
            store.conn.execute("DELETE FROM findings WHERE dedup_key=?1 AND status='new'", [key])?;
        }
    }

    // 자격 프로젝트 카드 upsert. 신규 노출만 fresh로 반환.
    let existing_set: std::collections::HashSet<&String> = existing.iter().collect();
    let now = chrono::Utc::now().to_rfc3339();
    let mut fresh = Vec::new();
    for (host, proj, confirmed, examples) in qualifying {
        let key = format!("R7|{host}|{proj}");
        let card = Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some(host.clone()), scope_project: Some(proj.clone()),
            scope_kind: "project".into(), scope_ref: proj.clone(),
            evidence: serde_json::json!({
                "over_modeled_sessions": confirmed,
                "example_session_ids": examples,
                "note": "LLM 판정: 이 프로젝트의 Opus 세션 상당수가 Sonnet으로 충분",
            }),
            est_tokens_saved: 0,
            prescription: Some(Prescription {
                kind: "start_with_lighter_model".into(),
                payload: serde_json::json!({ "to": "sonnet" }),
            }),
            dedup_key: key.clone(),
        };
        store.upsert_finding(&card, &now)?; // (R7,project) → init 'new'
        if !existing_set.contains(&key) {
            fresh.push(key); // 새로 노출된 것만 알림
        }
    }
    Ok(fresh)
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

    #[test]
    fn rollup_emits_project_card_when_enough_confirmed() {
        let store = SqliteStore::open_in_memory().unwrap();
        // confirmed 세션 3개 + rejected 1개 (같은 host/project) → 3 >= 3, 3*2 > 4 과반
        let mk = |sid: &str, status: &str| {
            let f = crate::finding::Finding {
                rule_id:"R7".into(), severity:crate::finding::Severity::Suggest,
                scope_host:Some("Windows".into()), scope_project:Some("p".into()),
                scope_kind:"session".into(), scope_ref:sid.into(),
                evidence: serde_json::json!({"session_id":sid}), est_tokens_saved:0,
                prescription:None, dedup_key: format!("R7|sess|Windows|{sid}"),
            };
            store.upsert_finding(&f, "2026-07-06T10:00:00Z").unwrap();
            store.set_judgment(&f.dedup_key, Some(status), &serde_json::json!({"attempts":1})).unwrap();
        };
        mk("s1","confirmed"); mk("s2","confirmed"); mk("s3","confirmed"); mk("s4","rejected");
        let fresh = rollup_project_cards(&store).unwrap();
        assert_eq!(fresh, vec!["R7|Windows|p".to_string()]);
        let status: String = store.conn.query_row(
            "SELECT status FROM findings WHERE dedup_key='R7|Windows|p'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "new");
    }

    #[test]
    fn rollup_silent_below_threshold_and_clears_stale() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 기존(구 통계) 프로젝트 카드가 'new'로 남아있으나 confirmed 세션 부족 → 제거돼야
        let card = crate::finding::Finding {
            rule_id:"R7".into(), severity:crate::finding::Severity::Suggest,
            scope_host:Some("Windows".into()), scope_project:Some("p".into()),
            scope_kind:"project".into(), scope_ref:"p".into(),
            evidence: serde_json::json!({}), est_tokens_saved:0, prescription:None,
            dedup_key:"R7|Windows|p".into(),
        };
        store.upsert_finding(&card, "2026-07-06T10:00:00Z").unwrap(); // 'new'
        let fresh = rollup_project_cards(&store).unwrap();
        assert!(fresh.is_empty());
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM findings WHERE dedup_key='R7|Windows|p' AND status='new'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "자격 없는 stale 카드 제거");
    }
}
