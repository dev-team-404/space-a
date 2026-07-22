//! R6 v2 — 반복 지시 → 스킬/커맨드화 제안 (킥오프 3대 차별점).
//! "같은 지시를 여러 세션에서 반복한다" = 커스텀 커맨드/스킬로 묶을 후보.
//! v2는 prompt_events(세션 내 전체 프롬프트)의 정규화 동치로 판정, 세션당 1회 카운트
//! (스펙: 2026-07-20-r6-v2-session-repeat-mining).
//! ⚠ evidence에 프롬프트 원문(미리보기)이 들어가므로 허브 공유 화이트리스트 제외 유지.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;

pub struct R6RepeatedPrompts {
    /// 같은 지시로 열린 세션 수 문턱 (다세션 반복 경로).
    pub min_sessions: usize,
    /// ⚠ CALIBRATE — 한 세션 내 반복도 잡는 발화 문턱(총 등장수). never-clear 사각용.
    /// Windows 실데이터로 핀(스펙 §8). R7·R8·F 관행.
    pub min_occurrences: u64,
    /// ⚠ CALIBRATE — 느슨한 묶기 문자 bigram Jaccard 임계(재현율 위주, 정밀도는 LLM 판정).
    /// Windows 실데이터로 핀(스펙 §8).
    pub cluster_threshold: f64,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R6RepeatedPrompts {
    fn default() -> Self {
        R6RepeatedPrompts { min_sessions: 3, min_occurrences: 5, cluster_threshold: 0.5, days: 14 }
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
        use std::collections::{BTreeMap, BTreeSet};
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let rows = store.prompt_occurrence_rows(&cutoff)?;

        // 1) (host, norm60) 집계: 세션 집합·총 등장수·대표 preview(MIN 동치).
        struct NormAgg {
            sessions: BTreeSet<String>,
            occurrences: u64,
            preview: String,
        }
        let mut per_host: BTreeMap<String, BTreeMap<String, NormAgg>> = BTreeMap::new();
        for r in rows {
            let host_map = per_host.entry(r.host).or_default();
            let agg = host_map.entry(r.norm60).or_insert_with(|| NormAgg {
                sessions: BTreeSet::new(),
                occurrences: 0,
                preview: r.preview.clone(),
            });
            agg.sessions.insert(r.session_id);
            agg.occurrences += r.occurrences;
            if r.preview < agg.preview {
                agg.preview = r.preview; // MIN(preview)
            }
        }

        // 2) host별 묶기 → 묶음별 finding. (Task 3: 싱글턴, Task 4: 실제 군집)
        let mut out = Vec::new();
        for (host, norm_map) in per_host {
            let norms: Vec<String> = norm_map.keys().cloned().collect();
            let clusters = crate::rules::r6_cluster::cluster_norms(&norms, self.cluster_threshold);

            for cluster in clusters {
                // norms 는 사전순 정렬(BTreeMap keys) → cluster[0] = 사전순 최소 = 앵커.
                let member_norms: Vec<String> =
                    cluster.iter().map(|&i| norms[i].clone()).collect();
                let anchor = &member_norms[0];
                let mut sessions: BTreeSet<&String> = BTreeSet::new();
                let mut occurrences: u64 = 0;
                for nrm in &member_norms {
                    let a = &norm_map[nrm];
                    for s in &a.sessions {
                        sessions.insert(s);
                    }
                    occurrences += a.occurrences;
                }
                let session_count = sessions.len();
                // 발화: 다세션 반복 OR 세션 내 다회 반복(never-clear).
                if session_count < self.min_sessions && occurrences < self.min_occurrences {
                    continue;
                }
                let h = hash8(anchor);
                let rep_short: String = norm_map[anchor].preview.chars().take(60).collect();
                out.push(Finding {
                    rule_id: "R6".into(),
                    severity: Severity::Suggest,
                    scope_host: Some(host.clone()),
                    scope_project: None,
                    scope_kind: "pattern".into(),
                    scope_ref: format!("pattern:{h}"),
                    evidence: serde_json::json!({
                        "repeated_prompt": rep_short,
                        "session_count": session_count,
                        "occurrences_total": occurrences,
                        "member_norms": member_norms,
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

    /// 논리 dedup 키(ts+내용)가 같은 ts·내용을 한 행으로 접으므로,
    /// "다른 세션의 실제 반복"은 반드시 서로 다른 ts로 시딩한다.
    fn ts_at(i: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::minutes(i)).to_rfc3339()
    }

    #[test]
    fn r6_fires_on_three_sessions_with_same_prompt() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "s1", "매일 아침 판매 리포트 뽑아줘", &ts_at(0));
        seed_session(&store, "s2", "매일  아침 판매 리포트 뽑아줘 ", &ts_at(1)); // 공백 차이 → 동치
        seed_session(&store, "s3", "매일 아침 판매 리포트 뽑아줘", &ts_at(2));
        seed_session(&store, "s4", "완전 다른 요청입니다", &ts_at(3));
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
        // 노이즈는 상호 비유사해야 한다 — 느슨한 묶기(A)가 유사 노이즈를 한 묶음으로 뭉쳐
        // 의도치 않은 추가 finding을 만들지 않도록.
        let noise = ["도커 이미지 빌드 캐시 정리", "리액트 훅 의존성 배열 점검", "sql 인덱스 실행계획 확인"];
        for (i, sess) in ["s1", "s2", "s3"].iter().enumerate() {
            seed_prompt_at(&store, sess, noise[i], &ts_at(i as i64 * 2), 0);
            seed_prompt_at(&store, sess, "PR 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(i as i64 * 2 + 1), 10);
        }
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.evidence["session_count"], 3);
        assert_eq!(f.evidence["occurrences_total"], 3);
        assert!(f.evidence["repeated_prompt"].as_str().unwrap().contains("리뷰 코멘트"));
    }

    #[test]
    fn r6_clusters_paraphrased_repeats_across_sessions() {
        // 완전일치만 사각: 표현이 조금씩 달라도(다른 norm60) 문자 유사도로 묶여 한 후보가 된다.
        // 세션 3개(각 1회) → 완전일치로는 각 1개라 침묵했겠지만, 묶여서 session_count=3 발화.
        let store = SqliteStore::open_in_memory().unwrap();
        seed_prompt_at(&store, "s1", "pr 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(0), 0);
        seed_prompt_at(&store, "s2", "pr 리뷰 코멘트 종합 검토하고 반영해줘", &ts_at(1), 0);
        seed_prompt_at(&store, "s3", "pr 리뷰 코멘트 종합 검토 후 조치", &ts_at(2), 0);

        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "패러프레이즈 3개가 한 묶음으로 발화");
        let f = &findings[0];
        assert_eq!(f.evidence["session_count"], 3);
        let members = f.evidence["member_norms"].as_array().unwrap();
        assert!(members.len() >= 2, "묶음에 서로 다른 표현(norm)이 여러 개");
    }

    #[test]
    fn r6_fires_on_within_session_repetition_never_clear() {
        // never-clear 사각: 한 세션에서 같은 지시를 여러 번 반복하면 세션=1~2라도
        // occurrences_total 문턱으로 발화한다 (A — R6 v2 §4.3 "세션당 1회" 대체).
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "s1", "이 함수 리팩토링 진행해줘", &ts_at(i as i64), *off);
        }
        seed_prompt_at(&store, "s2", "이 함수 리팩토링 진행해줘", &ts_at(10), 0);
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "occurrences_total(6) ≥ 문턱 → 발화");
        assert_eq!(findings[0].evidence["occurrences_total"], 6);
        assert_eq!(findings[0].evidence["session_count"], 2);
    }

    #[test]
    fn r6_fires_on_single_session_heavy_repetition() {
        // 순수 never-clear: 한 세션 안에서만 5회 반복(세션=1) — occurrence 경로로 발화.
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "solo", "매주 배포 전 체크리스트 점검해줘", &ts_at(i as i64), *off);
        }
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence["session_count"], 1);
        assert_eq!(findings[0].evidence["occurrences_total"], 5);
        assert!(findings[0].evidence.get("member_norms").is_some(), "member_norms 키 존재");
    }

    #[test]
    fn r6_ignores_short_or_rare_prompts() {
        let store = SqliteStore::open_in_memory().unwrap();
        for i in 0..4 {
            seed_session(&store, &format!("a{i}"), "ㅇㅋ", &ts_at(i)); // 8자 미만 → 제외
        }
        seed_session(&store, "b1", "이건 두 번뿐인 반복 요청", &ts_at(5));
        seed_session(&store, "b2", "이건 두 번뿐인 반복 요청", &ts_at(6));
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r6_counts_forked_copies_once() {
        // resume 포크: 같은 ts·내용 프롬프트가 3개 세션 파일에 복제돼도 "3개 세션"이
        // 되면 안 된다 (2026-07-21 데이터 위생 스펙 §1.1-1 — 실사용 junk 카드의 주범)
        let store = SqliteStore::open_in_memory().unwrap();
        let ts = chrono::Utc::now().to_rfc3339();
        for sess in ["orig", "fork1", "fork2"] {
            seed_session(&store, sess, "그 배포 버전 어제 사내망에 올린 것 맞는지 확인해줘", &ts);
        }
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty(),
            "포크 복제본이 세션 수로 계산되면 안 됨");
    }
}
