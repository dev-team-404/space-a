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

/// 사람이 친 지시가 아니라 **하니스/스케줄러가 주입한** 프롬프트인지.
///
/// 크론이 넣은 `<scheduled-task …>`는 매번 똑같이 반복되므로 R6에 완벽한 반복 패턴으로
/// 보이지만, 스킬로 묶어봐야 의미가 없다 — 이미 자동화돼 있고 사람이 반복하는 일이 아니다.
/// (실측 2026-07-25: 한 사용자의 R6 3건이 전부 `<scheduled-task>` 오탐이었다.)
/// 슬래시 커맨드 실행 로그(`<command-name>`)와 그 출력도 같은 이유로 제외한다.
fn is_harness_injected(p: &str) -> bool {
    const MARKERS: [&str; 4] = [
        "<scheduled-task",
        "<command-name>",
        "<local-command-stdout>",
        "<local-command-caveat>",
    ];
    let head: String = p.trim_start().chars().take(200).collect::<String>().to_lowercase();
    MARKERS.iter().any(|m| head.contains(m))
}

/// 프롬프트 정규화 — 공백 붕괴 + 소문자 + 60자 컷. 너무 짧으면(일반어) 제외.
/// R6 후속(skill_draft)이 세션 매칭에 같은 기준을 쓰도록 crate 공개.
pub(crate) fn normalize(p: &str) -> Option<String> {
    // 하니스 주입 프롬프트는 마이닝 대상이 아니다. 60자 컷 전에 원문에서 판정한다
    // (마커가 앞부분에 있으나 컷 이후로 밀릴 수 있어 truncate 전에 본다).
    if is_harness_injected(p) {
        return None;
    }
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

/// 새 묶음의 멤버 **과반(≥50%)**이 억제 집합에 있으면 사실상 같은 반복으로 보고 침묵한다
/// (스펙 §5.4). "하나라도 겹치면 침묵"은 묶음이 커지며 무관한 반복까지 삼키고, 완전 일치는
/// 변형 하나만 늘어도 뚫린다 — 과반이 그 사이다.
fn is_dismiss_suppressed(
    member_norms: &[String],
    suppressed: &std::collections::HashSet<String>,
) -> bool {
    if suppressed.is_empty() {
        return false;
    }
    let hit = member_norms.iter().filter(|n| suppressed.contains(*n)).count();
    hit * 2 >= member_norms.len()
}

impl Rule for R6RepeatedPrompts {
    fn id(&self) -> &'static str {
        "R6"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        use std::collections::{BTreeMap, BTreeSet};
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let rows = store.prompt_occurrence_rows(&cutoff)?;
        // 「무시」 처분은 키가 아니라 내용에 붙는다 — 앵커가 바뀌어도 우회되지 않게 (§5.4).
        let dismissed_norms = store.dismissed_r6_member_norms()?;

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
                // 노출 필터 — 판정은 그대로 두고 방출만 막는다. 새 행도 만들지 않는다
                // (원래 무시된 행이 이미 남아 있으므로 중복).
                if is_dismiss_suppressed(&member_norms, &dismissed_norms) {
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

    /// 실측 오탐(2026-07-25): 크론이 주입한 프롬프트가 "9세션 반복"으로 잡혀
    /// 스킬화 후보로 제안됐다. 사람이 반복하는 일이 아니므로 마이닝 대상이 아니다.
    #[test]
    fn scheduled_task_prompts_are_not_mined() {
        let cron = "<scheduled-task name=\"ai-history-book-daily-chapter\" \
                    file=\"/Users/someone/.claude/tasks/x.md\">오늘의 챕터를 써줘</scheduled-task>";
        assert_eq!(normalize(cron), None, "크론 주입 프롬프트는 제외돼야 한다");

        // 슬래시 커맨드 실행 로그와 그 출력도 사람이 친 지시가 아니다
        assert_eq!(normalize("<command-name>/model</command-name> 어쩌고"), None);
        assert_eq!(normalize("<local-command-stdout>Set model to opus</local-command-stdout>"), None);

        // 사람이 실제로 반복하는 지시는 그대로 남는다 (과도 차단 방지)
        assert!(
            normalize("이 프로젝트 테스트 돌리고 실패한 것만 정리해줘").is_some(),
            "일반 지시까지 막으면 R6의 존재 이유가 사라진다"
        );
        // 'scheduled'라는 단어가 본문에 들어간 정상 지시는 막지 않는다 (태그 형태만 차단)
        assert!(normalize("scheduled 배포 일정 정리해서 표로 만들어줘").is_some());
    }

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
                kind: EventKind::UserPrompt { preview: prompt.into(), is_command: false },
            }])
            .unwrap();
    }

    fn seed_session(store: &SqliteStore, sess: &str, prompt: &str, ts: &str) {
        seed_prompt_at(store, sess, prompt, ts, 0);
    }

    /// 스킬/커맨드 호출로 판정된 프롬프트 시드 (어댑터가 is_command=true로 방출한 것과 동치).
    fn seed_command(store: &SqliteStore, sess: &str, prompt: &str, ts: &str) {
        store
            .upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(),
                schema_version: "t".into(),
                host: "Windows".into(),
                project_id: "p".into(),
                session_id: sess.into(),
                uuid: Some(format!("{sess}-cmd")),
                parent_uuid: None,
                is_sidechain: false,
                ts: Some(ts.into()),
                source_file: "s.jsonl".into(),
                source_offset: 0,
                msg_id: None,
                kind: EventKind::UserPrompt { preview: prompt.into(), is_command: true },
            }])
            .unwrap();
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
    fn r6_ignores_command_invocation_prompts() {
        // 실사용 오탐(2026-07-23): "…플랜을 superpowers:executing-plans 로 실행" 처럼
        // 이미 스킬/커맨드를 호출하는 지시는 브랜치·플랜문서만 바뀌는 템플릿형 반복이라
        // R6이 "스킬로 묶어라"를 순환 제안한다. 어댑터가 is_command=true로 표시한 프롬프트는
        // store가 prompt_events에서 제외하므로 R6 재료가 되지 않는다.
        let store = SqliteStore::open_in_memory().unwrap();
        seed_command(&store, "s1",
            "feat/install-signal 브랜치에서 …플랜을 superpowers:subagent-driven-development 로 실행", &ts_at(0));
        seed_command(&store, "s2",
            "feat/install-signal 브랜치에서 …플랜을 superpowers:subagent-driven-development 로 실행", &ts_at(1));
        seed_command(&store, "s3",
            "feat/windows-hook-shell-fix 브랜치에서 …Task 12–14만 superpowers:executing-plans로 실행", &ts_at(2));
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty(),
            "스킬/커맨드 호출 프롬프트는 R6 카드로 올라오면 안 됨");
    }

    /// finding을 저장하고 사용자 처분을 찍는다 (UI에서 「무시」/「해결함」을 누른 상태와 동치).
    fn dispose(store: &SqliteStore, f: &Finding, status: &str) {
        store.upsert_finding(f, &ts_at(0)).unwrap();
        assert!(store.set_finding_status(&f.dedup_key, status, "2026-08-03T00:00:00Z").unwrap());
    }

    /// 한 묶음으로 뭉치는 패러프레이즈 3종 — 세션 3개.
    fn seed_review_cluster(store: &SqliteStore) {
        seed_prompt_at(store, "s1", "pr 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(0), 0);
        seed_prompt_at(store, "s2", "pr 리뷰 코멘트 종합 검토하고 반영해줘", &ts_at(1), 0);
        seed_prompt_at(store, "s3", "pr 리뷰 코멘트 종합 검토 후 조치", &ts_at(2), 0);
    }

    /// 같은 묶음에 붙되 **사전순으로 더 앞서는** 변형 — 앵커(=dedup_key)를 갈아치운다.
    fn seed_anchor_drift_variant(store: &SqliteStore) {
        seed_prompt_at(store, "s4", "aa 리뷰 코멘트 종합 검토해서 조치하자", &ts_at(3), 0);
    }

    #[test]
    fn r6_stays_silent_when_majority_of_members_were_dismissed() {
        // §5.4 — 처분을 키가 아니라 내용(member_norms)에 붙인다. 같은 묶음이 다시 잡히면 침묵.
        let store = SqliteStore::open_in_memory().unwrap();
        seed_review_cluster(&store);
        let first = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(first.len(), 1);
        dispose(&store, &first[0], "dismissed");

        let again = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert!(again.is_empty(), "무시한 묶음은 다시 방출되면 안 됨");
    }

    #[test]
    fn r6_dismissal_survives_anchor_drift() {
        // 이 작업의 핵심 회귀: 사전순 최소 변형이 새로 붙으면 dedup_key가 바뀐다.
        // 키 기반 억제였다면 여기서 뚫려 무시가 우회된다.
        let control = SqliteStore::open_in_memory().unwrap();
        seed_review_cluster(&control);
        seed_anchor_drift_variant(&control);
        let drifted = R6RepeatedPrompts::default().evaluate(&control).unwrap();
        assert_eq!(drifted.len(), 1);

        let store = SqliteStore::open_in_memory().unwrap();
        seed_review_cluster(&store);
        let base = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(base.len(), 1);
        assert_ne!(
            drifted[0].dedup_key, base[0].dedup_key,
            "전제 확인 — 새 변형이 앵커를 갈아치워 키가 실제로 달라진다"
        );

        dispose(&store, &base[0], "dismissed");
        seed_anchor_drift_variant(&store);
        assert!(
            R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty(),
            "키가 바뀌어도 멤버 과반이 겹치면 침묵해야 한다"
        );
    }

    #[test]
    fn r6_fires_when_dismissed_members_are_a_minority() {
        // 무시한 건 1-멤버 묶음이었는데, 나중에 다른 표현 2개가 더 붙어 묶음이 커졌다.
        // 겹침 1/3 < 과반 → 사실상 다른 반복이므로 정상 방출한다.
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "s1", "pr 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(i as i64), *off);
        }
        let first = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].evidence["member_norms"].as_array().unwrap().len(), 1);
        dispose(&store, &first[0], "dismissed");

        seed_prompt_at(&store, "s2", "pr 리뷰 코멘트 종합 검토하고 반영해줘", &ts_at(10), 0);
        seed_prompt_at(&store, "s3", "pr 리뷰 코멘트 종합 검토 후 조치", &ts_at(11), 0);
        let again = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(again.len(), 1, "과반 미만이면 억제하지 않는다");
        assert_eq!(again[0].evidence["member_norms"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn r6_resolved_findings_do_not_suppress() {
        // 억제는 `dismissed`에만 건다 — 「해결함」 뒤에 같은 묶음이 또 잡힌 건 재발 신호다.
        // ⚠ 여기서 보장하는 건 **방출까지**다. 그 뒤 `upsert_finding`의 ON CONFLICT가 기존
        // status를 보존하므로, 같은 키로 재발하면 행은 `resolved`인 채 화면에 안 뜬다.
        // 그 복귀는 스펙 §5.1 재발 감지(`status_evidence_n` 스냅숏, PR③) 몫이다 — 여기서
        // status를 되돌리면 R6은 매 스캔 같은 묶음을 방출하므로 처분 60초 뒤 카드가 되살아난다(D5).
        let store = SqliteStore::open_in_memory().unwrap();
        seed_review_cluster(&store);
        let first = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(first.len(), 1);
        dispose(&store, &first[0], "resolved");

        assert_eq!(
            R6RepeatedPrompts::default().evaluate(&store).unwrap().len(),
            1,
            "해결함에는 억제를 걸지 않는다"
        );
    }

    #[test]
    fn r6_unrelated_dismissal_does_not_suppress() {
        // 억제는 내용 기반이다 — 무관한 묶음을 무시했다고 R6 전체가 조용해지면 안 된다.
        // (무시 행이 아예 없는 경로는 이 모듈의 나머지 테스트가 전부 커버한다.)
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "other", "도커 이미지 빌드 캐시 정리해줘", &ts_at(i as i64), *off);
        }
        let other = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(other.len(), 1);
        dispose(&store, &other[0], "dismissed");

        seed_review_cluster(&store);
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert!(
            findings.iter().any(|f| {
                f.evidence["repeated_prompt"].as_str().unwrap_or_default().contains("리뷰 코멘트")
            }),
            "겹치지 않는 묶음은 그대로 방출"
        );
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
