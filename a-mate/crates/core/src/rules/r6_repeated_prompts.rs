//! R6 v2 — 반복 지시 → 스킬/커맨드화 제안 (킥오프 3대 차별점).
//! "같은 지시를 여러 세션에서 반복한다" = 커스텀 커맨드/스킬로 묶을 후보.
//! v2는 prompt_events(세션 내 전체 프롬프트)의 정규화 동치로 판정, 세션당 1회 카운트
//! (스펙: 2026-07-20-r6-v2-session-repeat-mining). tool-시퀀스 군집은 R23.
//! ⚠ evidence에 프롬프트 원문(미리보기)이 들어가므로 허브 공유 화이트리스트 제외 유지.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;

pub struct R6RepeatedPrompts {
    /// 같은 지시로 열린 세션 수 문턱
    pub min_sessions: usize,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R6RepeatedPrompts {
    fn default() -> Self {
        R6RepeatedPrompts { min_sessions: 3, days: 14 }
    }
}

/// 프롬프트 정규화 — 공백 붕괴 + 소문자 + 60자 컷. 너무 짧으면(일반어) 제외.
/// R6 후속(skill_draft)이 세션 매칭에 같은 기준을 쓰도록 crate 공개.
pub(crate) fn normalize(p: &str) -> Option<String> {
    let collapsed = p.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    if collapsed.chars().count() < 8 {
        return None;
    }
    Some(collapsed.chars().take(60).collect())
}

fn hash8(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}

impl Rule for R6RepeatedPrompts {
    fn id(&self) -> &'static str {
        "R6"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        // v2 — 첫 프롬프트 제한 제거: prompt_events 전체에서 (host, norm60) 그룹,
        // 세션당 1회 카운트 (스펙 §4.3). 세션 내 다회 반복은 occurrences_total로만 병기.
        let mut stmt = store.conn.prepare(
            "SELECT host, norm60, COUNT(DISTINCT session_id), COUNT(*), MIN(preview)
             FROM prompt_events WHERE ts >= ?1
             GROUP BY host, norm60 ORDER BY host, norm60",
        )?;
        let rows: Vec<(String, String, u64, u64, String)> = stmt
            .query_map(rusqlite::params![cutoff], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut out = Vec::new();
        for (host, norm, sessions, occurrences, rep) in rows {
            if (sessions as usize) < self.min_sessions {
                continue;
            }
            let h = hash8(&norm);
            let rep_short: String = rep.chars().take(60).collect();
            out.push(Finding {
                rule_id: "R6".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: None,
                scope_kind: "pattern".into(),
                scope_ref: format!("pattern:{h}"),
                evidence: serde_json::json!({
                    "repeated_prompt": rep_short,
                    "session_count": sessions,
                    "occurrences_total": occurrences,
                    "window_days": self.days,
                }),
                est_tokens_saved: 0, // 근거 없는 수치 금지 — 가치 제안형
                prescription: Some(Prescription {
                    kind: "skillify".into(),
                    payload: serde_json::json!({ "preview": rep_short }),
                }),
                dedup_key: format!("R6|{host}|{h}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, NormalizedEvent};

    fn seed_prompt_at(store: &SqliteStore, sess: &str, prompt: &str, ts: &str, offset: u64) {
        store
            .upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(),
                schema_version: "t".into(),
                host: "Windows".into(),
                project_id: "p".into(),
                session_id: sess.into(),
                uuid: Some(format!("{sess}-u{offset}")),
                parent_uuid: None,
                is_sidechain: false,
                ts: Some(ts.into()),
                source_file: "s.jsonl".into(),
                source_offset: offset,
                msg_id: None,
                kind: EventKind::UserPrompt { preview: prompt.into() },
            }])
            .unwrap();
    }

    fn seed_session(store: &SqliteStore, sess: &str, prompt: &str, ts: &str) {
        seed_prompt_at(store, sess, prompt, ts, 0);
    }

    #[test]
    fn r6_fires_on_three_sessions_with_same_prompt() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_session(&store, "s1", "매일 아침 판매 리포트 뽑아줘", &now);
        seed_session(&store, "s2", "매일  아침 판매 리포트 뽑아줘 ", &now); // 공백 차이 → 동치
        seed_session(&store, "s3", "매일 아침 판매 리포트 뽑아줘", &now);
        seed_session(&store, "s4", "완전 다른 요청입니다", &now);
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R6");
        assert_eq!(f.evidence["session_count"], 3);
        assert!(f.evidence["repeated_prompt"].as_str().unwrap().contains("판매 리포트"));
        assert_eq!(f.est_tokens_saved, 0);
    }

    #[test]
    fn r6_fires_on_mid_session_repeats_across_sessions() {
        // 첫 프롬프트가 아니라 세션 중간에 반복되는 지시도 잡는다 (v2 — 스펙 §4.3)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for (i, sess) in ["s1", "s2", "s3"].iter().enumerate() {
            seed_prompt_at(&store, sess, &format!("서로 다른 작업 요청 {i}번"), &now, 0);
            seed_prompt_at(&store, sess, "PR 리뷰 코멘트 종합 검토해서 조치해줘", &now, 10);
        }
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.evidence["session_count"], 3);
        assert_eq!(f.evidence["occurrences_total"], 3);
        assert!(f.evidence["repeated_prompt"].as_str().unwrap().contains("리뷰 코멘트"));
    }

    #[test]
    fn r6_counts_session_once_despite_in_session_repeats() {
        // 한 세션 안에서 5번 반복 ≠ 5개 세션 — 세션당 1회만 센다 (스펙 §4.3)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for off in [0u64, 10, 20, 30, 40] {
            seed_prompt_at(&store, "s1", "이 함수 리팩토링 진행해줘", &now, off);
        }
        seed_prompt_at(&store, "s2", "이 함수 리팩토링 진행해줘", &now, 0);
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty(),
            "세션 2개는 문턱(3) 미달이어야 함");
    }

    #[test]
    fn r6_ignores_short_or_rare_prompts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for i in 0..4 {
            seed_session(&store, &format!("a{i}"), "ㅇㅋ", &now); // 8자 미만 → 제외
        }
        seed_session(&store, "b1", "이건 두 번뿐인 반복 요청", &now);
        seed_session(&store, "b2", "이건 두 번뿐인 반복 요청", &now);
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty());
    }
}
