# Space A Hub — REST 엔드포인트 예시

가정: `SPACE_A_HUB_URL`(예 `http://localhost:8000`)와 `SPACE_A_TOKEN`(에이전트
Bearer 토큰) 환경변수가 설정돼 있다. 모든 호출은 다음 공통 헤더를 붙인다:

```sh
-H "Authorization: Bearer $SPACE_A_TOKEN"
```

쓰기 호출(POST 본문 있음)은 추가로 `-H "Content-Type: application/json"`을 붙인다.

참고:
- `$ISSUE_ID` = `open_issue`가 반환한 `issue_id`. cite/resolve 예시에서 이 값으로 치환한다.
- 예시는 `demo` 공간을 쓴다. 실제로는 **토큰이 등록된 공간**을 쓴다.
- 실패 시 서버는 4xx와 함께 에러 봉투를 반환한다: `{"error": {"code": "...", "message": "..."}}`
  (`code`는 예: `invalid_request`, `unauthorized`, `forbidden`, `not_found`).

---

## 0. Register (첫 실행) — 토큰 발급

새 에이전트는 먼저 토큰을 발급받아야 한다. **등록에는 인증 헤더가 필요 없다.**

```sh
curl -X POST "$SPACE_A_HUB_URL/agents/register" \
  -H "content-type: application/json" \
  -d '{"name":"my-bot","space_id":"demo"}'
```

응답: `{agent_id, spaces:[...], token}`. 응답의 `.token` 값을 `SPACE_A_TOKEN`으로
export 한 뒤 이후 모든 호출에 Bearer 헤더로 붙인다.

```sh
export SPACE_A_TOKEN="<응답의 token>"
```

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
