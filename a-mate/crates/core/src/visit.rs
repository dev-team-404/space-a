//! P3 — 방문 시 자동 방명록: 판정(쿨다운·이유 게이트)과 문구 생성의 순수 로직.
//! 스펙: docs/design/a-mate/specs/2026-07-27-auto-guestbook-design.md
//! 글루(src-tauri/src/visit.rs)가 서버 GET 결과를 넘겨 호출한다 — 여기엔 I/O 없음.

use crate::mascot::OwnerVibe;

/// 같은 방 재게시 도배 방지 바닥 (스펙 §1) — 이유가 있어도 이 안엔 무조건 skip.
pub const VISIT_COOLDOWN_HOURS: i64 = 24;
/// "오랜만" 이유 기준 — 마지막 내 글이 이 이상 오래됐으면 재방문 사유가 된다.
pub const VISIT_LONG_TIME_DAYS: i64 = 7;

/// 방명록을 남기는 이유 — 프롬프트에 "이번 방문 이유"로 주입된다 (스펙 §3·§4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignReason {
    FirstVisit,
    Weekend,
    OwnerIdle,
    LongTimeNoSee,
}

/// 이 방에 방명록을 남길지와 그 이유 (스펙 §3). None = 조용히 skip.
/// entries는 대상 방 방명록 GET 결과(서버 최신순이지만 순서 가정 없이 max로 판정).
/// 내 top-level 글의 최신 created_at 기준: 없음=첫 방문, 24h 이내=쿨다운,
/// 그 후 우선순위 오랜만(≥7일) > 주말 > 주인 한가(평일). 오랜만이 최우선인 이유:
/// 문구 소재로 가장 구체적이고, 주말·한가함은 그날 내내 참이라 변별력이 낮다.
/// 파싱 불가 행은 방어적으로 무시. 자기 방 제외·토글은 호출자(글루) 책임.
pub fn visit_sign_decision(
    entries: &[serde_json::Value],
    my_agent_id: &str,
    now: chrono::DateTime<chrono::Utc>,
    is_weekend: bool,
    vibe: OwnerVibe,
) -> Option<SignReason> {
    let field = |e: &serde_json::Value, k: &str| -> Option<String> {
        e.get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let mine_latest = entries
        .iter()
        .filter(|e| field(e, "author_agent_id").as_deref() == Some(my_agent_id))
        .filter(|e| field(e, "parent_id").is_none())
        .filter_map(|e| field(e, "created_at"))
        .filter_map(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .max();
    let Some(last) = mine_latest else {
        return Some(SignReason::FirstVisit);
    };
    let age = now.signed_duration_since(last);
    if age < chrono::Duration::hours(VISIT_COOLDOWN_HOURS) {
        None
    } else if age >= chrono::Duration::days(VISIT_LONG_TIME_DAYS) {
        Some(SignReason::LongTimeNoSee)
    } else if is_weekend {
        Some(SignReason::Weekend)
    } else if vibe == OwnerVibe::Idle {
        Some(SignReason::OwnerIdle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0).unwrap()
    }

    fn e(author: &str, created: chrono::DateTime<Utc>, parent: Option<&str>) -> serde_json::Value {
        let mut v = serde_json::json!({
            "entry_id": "x", "author_agent_id": author, "author_name": "n",
            "body": "b", "created_at": created.to_rfc3339(),
        });
        if let Some(p) = parent { v["parent_id"] = serde_json::json!(p); }
        v
    }

    #[test]
    fn first_visit_when_no_entries_or_none_mine() {
        assert_eq!(visit_sign_decision(&[], "me", now(), false, OwnerVibe::Normal), Some(SignReason::FirstVisit));
        let others = vec![e("someone", now() - Duration::hours(1), None)];
        assert_eq!(visit_sign_decision(&others, "me", now(), false, OwnerVibe::Normal), Some(SignReason::FirstVisit));
    }

    #[test]
    fn my_reply_does_not_count_as_visit_entry() {
        // parent 있는 글(답글)은 top-level이 아니므로 첫 방문 취급
        let entries = vec![e("me", now() - Duration::hours(1), Some("gb_parent"))];
        assert_eq!(visit_sign_decision(&entries, "me", now(), false, OwnerVibe::Normal), Some(SignReason::FirstVisit));
    }

    #[test]
    fn cooldown_blocks_within_24h_even_with_reason() {
        let entries = vec![e("me", now() - Duration::hours(23), None)];
        assert_eq!(visit_sign_decision(&entries, "me", now(), true, OwnerVibe::Idle), None);
    }

    #[test]
    fn revisit_needs_a_reason() {
        let entries = vec![e("me", now() - Duration::hours(25), None)];
        // 평일 + Normal → 이유 없음
        assert_eq!(visit_sign_decision(&entries, "me", now(), false, OwnerVibe::Normal), None);
        // 주말
        assert_eq!(visit_sign_decision(&entries, "me", now(), true, OwnerVibe::Normal), Some(SignReason::Weekend));
        // 평일 + 한가
        assert_eq!(visit_sign_decision(&entries, "me", now(), false, OwnerVibe::Idle), Some(SignReason::OwnerIdle));
        // Busy는 이유 아님
        assert_eq!(visit_sign_decision(&entries, "me", now(), false, OwnerVibe::Busy), None);
    }

    #[test]
    fn long_time_wins_over_weekend() {
        let entries = vec![e("me", now() - Duration::days(8), None)];
        assert_eq!(visit_sign_decision(&entries, "me", now(), true, OwnerVibe::Idle), Some(SignReason::LongTimeNoSee));
    }

    #[test]
    fn latest_of_my_entries_decides() {
        // 옛 글(10일 전)과 최신 글(1시간 전)이 같이 있으면 최신 기준 → 쿨다운 skip
        let entries = vec![
            e("me", now() - Duration::days(10), None),
            e("me", now() - Duration::hours(1), None),
        ];
        assert_eq!(visit_sign_decision(&entries, "me", now(), true, OwnerVibe::Idle), None);
    }

    #[test]
    fn malformed_rows_are_ignored() {
        // created_at 파싱 불가·필드 누락 행은 무시 → 내 유효 글이 없으면 첫 방문
        let entries = vec![
            serde_json::json!({"entry_id":"x","author_agent_id":"me","author_name":"n","body":"b","created_at":"not-a-date"}),
            serde_json::json!({"author_agent_id":"me"}),
        ];
        assert_eq!(visit_sign_decision(&entries, "me", now(), false, OwnerVibe::Normal), Some(SignReason::FirstVisit));
    }
}
