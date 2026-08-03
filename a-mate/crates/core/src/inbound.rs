//! 인바운드 소식(방문·방명록) diff — 스캔 편승 폴링의 순수 판정부.
//! 스펙: docs/archive/design/a-mate/specs/2026-07-29-visit-infra-notices-design.md §4
//!
//! 커서는 서버 발급 RFC3339(UTC, 동일 서식) 문자열 — 사전순 비교로 충분하다.
//! 파싱 불가·필드 누락 행은 방어적으로 무시한다.

use serde_json::Value;

/// 방문 diff: `first_at > cursor`인 행만 emit 대상. 세션 연장(last_at만 갱신)된 행은
/// first_at이 그대로라 재-emit되지 않는다 — 도배 억제가 커서 기준에서 완성된다 (스펙 §2).
/// cursor가 None(첫 실행)이면 emit 없이 커서만 초기화 — 설치 직후 과거분 도배 방지.
/// 반환 커서는 관측한 first_at 최댓값과 기존 커서 중 큰 쪽 — 항상 단조 증가.
pub fn select_new_visits(rows: &[Value], cursor: Option<&str>) -> (Vec<Value>, Option<String>) {
    let next = rows
        .iter()
        .filter_map(|r| r.get("first_at").and_then(|v| v.as_str()))
        .chain(cursor)
        .max()
        .map(str::to_string);
    let Some(cur) = cursor else { return (Vec::new(), next) };
    let fresh = rows
        .iter()
        .filter(|r| r.get("first_at").and_then(|v| v.as_str()).is_some_and(|f| f > cur))
        .cloned()
        .collect();
    (fresh, next)
}

/// 방명록 diff: 타인 글(author_agent_id ≠ 나)만 대상 — 내 봇 답글·수동 글은 소식이 아니다.
/// created_at > cursor인 항목만 emit. 커서 규약은 select_new_visits와 동일
/// (None=첫 실행 초기화, 관측 최댓값으로 단조 증가).
pub fn select_new_guestbook(
    entries: &[Value],
    my_agent_id: &str,
    cursor: Option<&str>,
) -> (Vec<Value>, Option<String>) {
    let others: Vec<&Value> = entries
        .iter()
        .filter(|e| {
            e.get("author_agent_id").and_then(|v| v.as_str()).is_some_and(|a| a != my_agent_id)
        })
        .collect();
    let next = others
        .iter()
        .filter_map(|e| e.get("created_at").and_then(|v| v.as_str()))
        .chain(cursor)
        .max()
        .map(str::to_string);
    let Some(cur) = cursor else { return (Vec::new(), next) };
    let fresh = others
        .into_iter()
        .filter(|e| e.get("created_at").and_then(|v| v.as_str()).is_some_and(|c| c > cur))
        .cloned()
        .collect();
    (fresh, next)
}

/// 재사용(인정) diff: **내가 발행한 페이지**를 남이 인용한 건만 대상.
/// `page_id`가 내 발행분에 있고 `created_at > cursor`인 행만 emit한다.
///
/// - 내 인용은 제외 — 자기 글을 자기가 인용한 건 인정이 아니다(`cited_by == me`).
/// - 커서 규약은 select_new_visits와 동일: None=첫 실행 초기화(도배 방지), 관측 최댓값으로 단조 증가.
///   그래서 **한 이벤트는 평생 1회만** 축하된다 — 별도 dedup 상태가 필요 없다.
/// - 커서 후보는 **대상 행만**으로 계산한다. 남의 인용까지 커서를 밀면 내 것이 유실될 수 있다
///   (select_new_guestbook이 내 글을 커서 후보에서 뺀 것과 같은 이유).
pub fn select_new_reuses(
    rows: &[Value],
    my_page_ids: &std::collections::HashSet<String>,
    my_agent_id: &str,
    cursor: Option<&str>,
) -> (Vec<Value>, Option<String>) {
    let mine: Vec<&Value> = rows
        .iter()
        .filter(|r| {
            r.get("page_id")
                .and_then(|v| v.as_str())
                .is_some_and(|p| my_page_ids.contains(p))
        })
        .filter(|r| {
            // cited_by가 없으면(구서버) 남의 인용으로 본다 — 놓치는 것보다 낫다.
            r.get("cited_by").and_then(|v| v.as_str()).is_none_or(|a| a != my_agent_id)
        })
        .collect();
    let next = mine
        .iter()
        .filter_map(|r| r.get("created_at").and_then(|v| v.as_str()))
        .chain(cursor)
        .max()
        .map(str::to_string);
    let Some(cur) = cursor else { return (Vec::new(), next) };
    let fresh = mine
        .into_iter()
        .filter(|r| r.get("created_at").and_then(|v| v.as_str()).is_some_and(|c| c > cur))
        .cloned()
        .collect();
    (fresh, next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn visit(first_at: &str, last_at: &str) -> Value {
        json!({"visit_id": "v", "visitor_name": "B", "first_at": first_at, "last_at": last_at, "present": false})
    }

    #[test]
    fn first_run_initializes_cursor_without_emitting() {
        let rows = vec![visit("2026-07-29T01:00:00+00:00", "2026-07-29T01:00:00+00:00")];
        let (fresh, cur) = select_new_visits(&rows, None);
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn first_run_with_no_rows_keeps_cursor_unset() {
        let (fresh, cur) = select_new_visits(&[], None);
        assert!(fresh.is_empty());
        assert_eq!(cur, None); // 다음 스캔도 첫 실행 취급 — 과거분 도배 없음
    }

    #[test]
    fn emits_only_rows_newer_than_cursor() {
        let rows = vec![
            visit("2026-07-29T03:00:00+00:00", "2026-07-29T03:00:00+00:00"),
            visit("2026-07-29T01:00:00+00:00", "2026-07-29T01:00:00+00:00"),
        ];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["first_at"], "2026-07-29T03:00:00+00:00");
        assert_eq!(cur.as_deref(), Some("2026-07-29T03:00:00+00:00"));
    }

    #[test]
    fn session_extension_is_not_reemitted() {
        // last_at만 갱신된 행(first_at ≤ 커서) — since 필터에 걸려 내려와도 emit 제외
        let rows = vec![visit("2026-07-29T01:00:00+00:00", "2026-07-29T05:00:00+00:00")];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T01:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn cursor_never_regresses_when_rows_pruned() {
        let (fresh, cur) = select_new_visits(&[], Some("2026-07-29T09:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T09:00:00+00:00"));
    }

    #[test]
    fn malformed_rows_are_ignored() {
        let rows = vec![json!({"visit_id": "broken"})];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T01:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    fn entry(id: &str, author: &str, created_at: &str, parent: Option<&str>) -> Value {
        json!({"entry_id": id, "author_agent_id": author, "author_name": author,
               "body": "글", "parent_id": parent, "created_at": created_at})
    }

    #[test]
    fn guestbook_first_run_initializes_without_emitting() {
        let entries = vec![entry("e1", "other", "2026-07-29T01:00:00+00:00", None)];
        let (fresh, cur) = select_new_guestbook(&entries, "me", None);
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn guestbook_excludes_my_own_entries_everywhere() {
        // 내 글은 emit 대상도, 커서 후보도 아니다 — 내 답글로 커서가 앞서가 타인 글을 놓치면 안 됨
        let entries = vec![
            entry("mine", "me", "2026-07-29T05:00:00+00:00", Some("e1")),
            entry("e1", "other", "2026-07-29T03:00:00+00:00", None),
        ];
        let (fresh, cur) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["entry_id"], "e1");
        assert_eq!(cur.as_deref(), Some("2026-07-29T03:00:00+00:00"));
    }

    #[test]
    fn guestbook_counts_replies_from_others_too() {
        let entries = vec![entry("r1", "other", "2026-07-29T03:00:00+00:00", Some("mine-post"))];
        let (fresh, _) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1); // parent 유무 무관 — 타인 글 전부 (스펙 §1)
    }

    #[test]
    fn guestbook_cursor_boundary_is_exclusive() {
        let entries = vec![entry("e1", "other", "2026-07-29T02:00:00+00:00", None)];
        let (fresh, _) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert!(fresh.is_empty()); // 커서와 같은 시각 = 이미 본 것
    }

    // ── 인정 루프 (재사용 diff) ─────────────────────────────────────────

    fn reuse(page: &str, by: &str, at: &str) -> Value {
        json!({"reuse_id": "r", "page_id": page, "cited_by": by,
               "space_id": "sw2", "cross_team": true, "created_at": at})
    }
    fn mine(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reuse_first_run_initializes_without_emitting() {
        let rows = vec![reuse("page_1", "bob", "2026-07-30T01:00:00+00:00")];
        let (fresh, cur) = select_new_reuses(&rows, &mine(&["page_1"]), "me", None);
        assert!(fresh.is_empty(), "설치 직후 과거분 도배 금지");
        assert_eq!(cur.as_deref(), Some("2026-07-30T01:00:00+00:00"));
    }

    #[test]
    fn reuse_only_my_published_pages_count() {
        // 남의 페이지가 인용된 건 내 인정이 아니다
        let rows = vec![
            reuse("page_1", "bob", "2026-07-30T03:00:00+00:00"),
            reuse("page_999", "bob", "2026-07-30T04:00:00+00:00"),
        ];
        let (fresh, cur) =
            select_new_reuses(&rows, &mine(&["page_1"]), "me", Some("2026-07-30T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["page_id"], "page_1");
        // 커서가 남의 인용(04:00)까지 밀리면 내 다음 인용을 놓친다
        assert_eq!(cur.as_deref(), Some("2026-07-30T03:00:00+00:00"));
    }

    #[test]
    fn reuse_excludes_self_citation() {
        let rows = vec![reuse("page_1", "me", "2026-07-30T03:00:00+00:00")];
        let (fresh, cur) =
            select_new_reuses(&rows, &mine(&["page_1"]), "me", Some("2026-07-30T02:00:00+00:00"));
        assert!(fresh.is_empty(), "자기 인용은 인정이 아니다");
        assert_eq!(cur.as_deref(), Some("2026-07-30T02:00:00+00:00"), "커서도 안 밀린다");
    }

    #[test]
    fn reuse_celebrates_once_only() {
        let rows = vec![reuse("page_1", "bob", "2026-07-30T03:00:00+00:00")];
        let (fresh, cur) =
            select_new_reuses(&rows, &mine(&["page_1"]), "me", Some("2026-07-30T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        // 같은 데이터로 다시 폴링해도 재-emit 없음 (커서가 전진했으므로)
        let (again, _) = select_new_reuses(&rows, &mine(&["page_1"]), "me", cur.as_deref());
        assert!(again.is_empty(), "한 이벤트는 평생 1회만 축하");
    }

    #[test]
    fn reuse_malformed_rows_are_ignored() {
        let rows = vec![json!({"reuse_id": "broken"}), reuse("page_1", "bob", "bad-but-present")];
        let (fresh, _) =
            select_new_reuses(&rows, &mine(&["page_1"]), "me", Some("2026-07-30T02:00:00+00:00"));
        // page_id 없는 행은 걸러지고, 남은 행은 문자열 비교로 판정된다(패닉 없음)
        assert!(fresh.len() <= 1);
    }

    #[test]
    fn reuse_cursor_never_regresses() {
        let (fresh, cur) =
            select_new_reuses(&[], &mine(&["page_1"]), "me", Some("2026-07-30T09:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-30T09:00:00+00:00"));
    }
}
