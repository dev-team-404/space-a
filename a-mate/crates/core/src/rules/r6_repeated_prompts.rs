//! R6 v1 — 반복 지시 → 스킬/커맨드화 제안 (킥오프 3대 차별점, 2026-07-19 검토로 착수).
//! "같은 첫 요청으로 세션을 반복해서 열고 있다" = 커스텀 커맨드/스킬로 묶을 후보.
//! v1은 첫 프롬프트 미리보기의 정규화 동치로 판정 — tool-시퀀스 군집(v2)은 후속.
//! ⚠ evidence에 프롬프트 원문(미리보기)이 들어가므로 허브 공유 화이트리스트 제외 유지.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

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
        let mut stmt = store.conn.prepare(
            "SELECT host, first_prompt_preview FROM sessions
             WHERE first_prompt_preview IS NOT NULL AND last_ts >= ?1",
        )?;
        let rows: Vec<(Option<String>, String)> = stmt
            .query_map(rusqlite::params![cutoff], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        // (host, 정규화 지시) → (횟수, 원문 대표)
        let mut groups: BTreeMap<(String, String), (u64, String)> = BTreeMap::new();
        for (host, preview) in rows {
            let Some(norm) = normalize(&preview) else { continue };
            let host = host.unwrap_or_default();
            let e = groups.entry((host, norm)).or_insert((0, preview.clone()));
            e.0 += 1;
        }

        let mut out = Vec::new();
        for ((host, norm), (n, rep)) in groups {
            if (n as usize) < self.min_sessions {
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
                    "session_count": n,
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

    fn seed_session(store: &SqliteStore, sess: &str, prompt: &str, ts: &str) {
        store
            .upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(),
                schema_version: "t".into(),
                host: "Windows".into(),
                project_id: "p".into(),
                session_id: sess.into(),
                uuid: Some(format!("{sess}-u0")),
                parent_uuid: None,
                is_sidechain: false,
                ts: Some(ts.into()),
                source_file: "s.jsonl".into(),
                source_offset: 0,
                kind: EventKind::UserPrompt { preview: prompt.into() },
            }])
            .unwrap();
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
