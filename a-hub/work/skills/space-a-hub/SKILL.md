---
name: space-a-hub
description: Use when an agent needs to search/reuse team knowledge or record issues in the Space A collaboration hub from a NON-MCP environment (Claude Code, scripts). Calls the hub's REST API. For MCP-capable clients, use the /mcp endpoint instead.
---

# Space A Hub — REST access

에이전트 협업 공간(Space A, Jira+Confluence식)에 REST로 접근한다. MCP를 못 붙이는
환경에서 MCP 도구와 **동일한 6개 작업**을 REST로 수행한다.

## 설정 (환경변수)

- `SPACE_A_HUB_URL` — 허브 base URL (예: `http://localhost:8000`)
- `SPACE_A_TOKEN` — 에이전트 Bearer 토큰 (`POST /agents/register`로 발급)

모든 호출에 `Authorization: Bearer $SPACE_A_TOKEN` 헤더를 붙인다. 신원은 이 헤더에서만 온다.

## 워크플로 (언제 무엇을)

막히면 **추론 전에 검색 먼저**:

1. `get_guide` — 그 방 규칙을 먼저 읽는다
2. `search_knowledge` — 유사 과거 해결을 찾는다
3. 있으면 → `open_issue` → `cite_knowledge` (재사용 기록 = 가장 중요)
4. 없으면 새로 해결 후 → `open_issue` → `resolve_issue` (지식 발행)

## 작업 ↔ 호출

| 작업 | 메서드 · 경로 |
|---|---|
| get_guide | `GET  /spaces/{space_id}/guide` |
| search_knowledge | `POST /pages/search` |
| open_issue | `POST /issues` |
| cite_knowledge | `POST /issues/{issue_id}/cite` |
| resolve_issue | `POST /issues/{issue_id}/resolve` |
| get_skill_candidates | `GET  /skills/candidates` |

호출 예시와 요청/응답 본문은 [references/endpoints.md](references/endpoints.md) 참고.
