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

    fn classify(&self, verdict: &serde_json::Value) -> Option<&'static str> {
        match verdict.get("over_modeled").and_then(|v| v.as_bool()) {
            Some(true) => Some("confirmed"),
            Some(false) => Some("rejected"),
            None => None, // over_modeled 필드 없음 = 판정 미확정(형식 불량 취급, 재시도)
        }
    }

    fn rollup(&self, store: &SqliteStore) -> Result<(Vec<String>, bool)> {
        rollup_project_cards(store)
    }
}

/// 반환 = (새로 노출된 프로젝트 카드 key, mutated). mutated = 카드가 생성/삭제/내용변경돼
/// UI 재발행이 필요한지. fresh는 신규 노출만(마스코트 알림용), mutated는 갱신·삭제까지 포함.
pub(crate) fn rollup_project_cards(store: &SqliteStore) -> Result<(Vec<String>, bool)> {
    use crate::finding::{Finding, Prescription, Severity};
    // 관찰창(14일) 내 세션만 집계 — 노후 verdict가 카드를 무한 유지하지 않도록(recency 경계).
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
    // (host, project)별 confirmed/판정합 집계
    let mut stmt = store.conn.prepare(
        "SELECT COALESCE(scope_host,''), COALESCE(scope_project,''),
                SUM(CASE WHEN status='confirmed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('confirmed','rejected') THEN 1 ELSE 0 END),
                GROUP_CONCAT(CASE WHEN status='confirmed'
                    THEN json_extract(evidence_json,'$.session_id') END)
         FROM findings
         WHERE rule_id='R7' AND scope_kind='session'
           AND json_extract(evidence_json,'$.first_ts') >= ?1
         GROUP BY scope_host, scope_project",
    )?;
    let rows: Vec<(String, String, i64, i64, Option<String>)> = stmt
        .query_map([&cutoff], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut qualifying: Vec<(String, String, i64, Vec<String>)> = Vec::new();
    for (host, proj, confirmed, judged, sids) in rows {
        if confirmed >= 3 && confirmed * 2 > judged {
            let examples: Vec<String> = sids.unwrap_or_default()
                .split(',').filter(|s| !s.is_empty()).take(5).map(String::from).collect();
            qualifying.push((host, proj, confirmed, examples));
        }
    }

    // 기존 'new' 프로젝트 카드(key → evidence_json) — 변경/삭제 감지에 사용.
    let qualifying_keys: std::collections::HashSet<String> =
        qualifying.iter().map(|(h, p, _, _)| format!("R7|{h}|{p}")).collect();
    let mut stale = store.conn.prepare(
        "SELECT dedup_key, evidence_json FROM findings
         WHERE rule_id='R7' AND scope_kind='project' AND status='new'",
    )?;
    let existing: std::collections::HashMap<String, String> = stale
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut mutated = false;
    // 비자격 stale 'new' 카드 제거(구 통계 카드 마이그레이션 포함).
    for key in existing.keys() {
        if !qualifying_keys.contains(key) {
            store.conn.execute("DELETE FROM findings WHERE dedup_key=?1 AND status='new'", [key])?;
            mutated = true;
        }
    }

    // 자격 카드 upsert. 신규 노출만 fresh, 내용 변화(신규·갱신)는 mutated로도 표시.
    let now = chrono::Utc::now().to_rfc3339();
    let mut fresh = Vec::new();
    for (host, proj, confirmed, examples) in qualifying {
        let key = format!("R7|{host}|{proj}");
        // 프론트 계약(coach-helpers.ts: session_ids/total_sessions)에 맞춘 evidence 키.
        let evidence = serde_json::json!({
            "total_sessions": confirmed,
            "session_ids": examples,
            "note": "LLM 판정: 이 프로젝트의 Opus 세션 상당수가 Sonnet으로 충분",
        });
        match existing.get(&key) {
            None => { fresh.push(key.clone()); mutated = true; } // 신규 노출
            Some(prev_json) => {
                let prev: serde_json::Value =
                    serde_json::from_str(prev_json).unwrap_or(serde_json::Value::Null);
                if prev.get("total_sessions") != evidence.get("total_sessions")
                    || prev.get("session_ids") != evidence.get("session_ids")
                {
                    mutated = true; // 카운트·예시 변화 → UI 갱신 필요
                }
            }
        }
        let card = Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some(host.clone()), scope_project: Some(proj.clone()),
            scope_kind: "project".into(), scope_ref: proj.clone(),
            evidence,
            est_tokens_saved: 0,
            prescription: Some(Prescription {
                kind: "start_with_lighter_model".into(),
                payload: serde_json::json!({ "to": "sonnet" }),
            }),
            dedup_key: key.clone(),
        };
        store.upsert_finding(&card, &now)?; // (R7,project) → init 'new'
    }
    Ok((fresh, mutated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::CoachingJudge;
    #[test]
    fn r7_judge_classify_maps_over_modeled() {
        let j = R7Judge;
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": true})), Some("confirmed"));
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": false})), Some("rejected"));
        assert_eq!(j.classify(&serde_json::json!({})), None); // 필드 없음 = 미확정(재시도)
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

    // 관찰창(14일) 내 recent 타임스탬프 — 하드코딩 날짜는 창 밖으로 밀려나므로 now 기준.
    fn recent_ts() -> String {
        (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339()
    }
    fn seed_session(store: &SqliteStore, sid: &str, status: &str, first_ts: &str) {
        let f = crate::finding::Finding {
            rule_id: "R7".into(), severity: crate::finding::Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: sid.into(),
            evidence: serde_json::json!({"session_id": sid, "first_ts": first_ts}),
            est_tokens_saved: 0, prescription: None,
            dedup_key: format!("R7|sess|Windows|{sid}"),
        };
        store.upsert_finding(&f, "2026-07-06T10:00:00Z").unwrap();
        store.set_judgment(&f.dedup_key, Some(status), &serde_json::json!({"attempts":1})).unwrap();
    }

    #[test]
    fn rollup_emits_project_card_when_enough_confirmed() {
        let store = SqliteStore::open_in_memory().unwrap();
        // confirmed 세션 3개 + rejected 1개 (같은 host/project) → 3 >= 3, 3*2 > 4 과반
        let ts = recent_ts();
        seed_session(&store, "s1", "confirmed", &ts);
        seed_session(&store, "s2", "confirmed", &ts);
        seed_session(&store, "s3", "confirmed", &ts);
        seed_session(&store, "s4", "rejected", &ts);
        let (fresh, mutated) = rollup_project_cards(&store).unwrap();
        assert_eq!(fresh, vec!["R7|Windows|p".to_string()]);
        assert!(mutated, "신규 카드 노출 = mutated");
        let (status, ev): (String, String) = store.conn.query_row(
            "SELECT status, evidence_json FROM findings WHERE dedup_key='R7|Windows|p'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(status, "new");
        // 프론트 계약 키(session_ids/total_sessions) 사용 확인
        let ev: serde_json::Value = serde_json::from_str(&ev).unwrap();
        assert_eq!(ev["total_sessions"], serde_json::json!(3));
        assert_eq!(ev["session_ids"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn rollup_ignores_sessions_outside_observation_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        // confirmed 3개지만 first_ts가 창(14일) 밖 → 카드 안 생김
        let old = (chrono::Utc::now() - chrono::Duration::days(20)).to_rfc3339();
        seed_session(&store, "s1", "confirmed", &old);
        seed_session(&store, "s2", "confirmed", &old);
        seed_session(&store, "s3", "confirmed", &old);
        let (fresh, _mutated) = rollup_project_cards(&store).unwrap();
        assert!(fresh.is_empty(), "창 밖 세션은 롤업 제외");
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM findings WHERE dedup_key='R7|Windows|p'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
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
        let (fresh, mutated) = rollup_project_cards(&store).unwrap();
        assert!(fresh.is_empty());
        assert!(mutated, "stale 카드 삭제 = mutated (UI 재발행 필요)");
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM findings WHERE dedup_key='R7|Windows|p' AND status='new'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "자격 없는 stale 카드 제거");
    }

    #[test]
    fn rollup_stable_card_is_not_mutated() {
        let store = SqliteStore::open_in_memory().unwrap();
        let ts = recent_ts();
        for sid in ["s1", "s2", "s3"] { seed_session(&store, sid, "confirmed", &ts); }
        let (_fresh, first) = rollup_project_cards(&store).unwrap();
        assert!(first, "첫 노출은 mutated");
        // 두 번째 롤업: 세션·verdict 변화 없음 → 카드 내용 동일 → mutated=false (재발행 안 함)
        let (fresh2, mutated2) = rollup_project_cards(&store).unwrap();
        assert!(fresh2.is_empty(), "이미 존재 → fresh 없음");
        assert!(!mutated2, "내용 불변 → mutated 아님(불필요한 재발행 방지)");
    }
}
