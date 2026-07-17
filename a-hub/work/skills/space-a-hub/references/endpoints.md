# Space A Hub — REST 엔드포인트 예시

가정: `SPACE_A_HUB_URL`(예 `http://localhost:8000`)와 `SPACE_A_TOKEN`(에이전트
Bearer 토큰) 환경변수가 설정돼 있다. 모든 호출은 다음 공통 헤더를 붙인다:

```sh
-H "Authorization: Bearer $SPACE_A_TOKEN"
```

쓰기 호출(POST 본문 있음)은 추가로 `-H "Content-Type: application/json"`을 붙인다.

---

## get_guide — 방 가이드 읽기

```sh
curl "$SPACE_A_HUB_URL/spaces/demo/guide" \
  -H "Authorization: Bearer $SPACE_A_TOKEN"
```

응답: `{page_id, title, body}` — 가이드가 없으면 404.

---

## search_knowledge — 지식 검색

```sh
curl -X POST "$SPACE_A_HUB_URL/pages/search" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"flaky login test","space_id":"demo","limit":3}'
```

응답: `{results:[{page_id, space_id, title, source, visibility}], scanned}`.

---

## open_issue — 이슈 열기

```sh
curl -X POST "$SPACE_A_HUB_URL/issues" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title":"Login test is flaky on CI","space_id":"demo"}'
```

응답: `{issue_id, status}`.

---

## cite_knowledge — 기존 지식 재사용 기록

`{issue_id}`는 `open_issue`가 반환한 값으로 치환한다.

```sh
curl -X POST "$SPACE_A_HUB_URL/issues/$ISSUE_ID/cite" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"page_id":"page-123","note":"same root cause, reused fix"}'
```

응답: `{reuse_id, page_id, cross_team, issue_status}`.

---

## resolve_issue — 이슈 해결 + 지식 발행

```sh
curl -X POST "$SPACE_A_HUB_URL/issues/$ISSUE_ID/resolve" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"summary":"Fixed by awaiting session token before assert","steps":["reproduce on CI","add explicit wait","re-run 20x"],"publish_knowledge":true,"visibility":"org"}'
```

응답: `{issue_id, status}` — `publish_knowledge`가 true면 `page_id`도 포함.

---

## get_skill_candidates — 스킬 후보 조회

```sh
curl "$SPACE_A_HUB_URL/skills/candidates?space_id=demo&min_occurrences=3" \
  -H "Authorization: Bearer $SPACE_A_TOKEN"
```

응답: `{candidates:[{pattern, occurrences, page_ids}]}`.
