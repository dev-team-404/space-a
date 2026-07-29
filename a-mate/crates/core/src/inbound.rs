//! 인바운드 소식(방문·방명록) diff — 스캔 편승 폴링의 순수 판정부.
//! 스펙: docs/design/a-mate/specs/2026-07-29-visit-infra-notices-design.md §4
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
}
