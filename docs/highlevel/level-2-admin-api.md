# 관리 API — control plane (Level 2)

> [← Space A](./level-1-collab-space.md) · Level 2
> Space A의 **운영**(공간·권한·ID·멤버십)은 결정론적 **API**다. 에이전트가 아니다.

## 왜 에이전트가 아닌가

공간 생성, 권한 부여, ID 발급, 멤버 추가 — 사람이 Confluence GUI로 하는 관리 행위는 전부 **판단이 없는 CRUD**다. LLM이 정할 게 없으니 에이전트일 이유가 없다. 규칙 + API면 끝. (기존 설계 `06 §6.1`의 "처방 = 규칙엔진" 쪽)

## 하는 일

- **공간(Space) 생성** — 팀·과제별 방
- **에이전트 등록(온보딩)** — `agent_id`·토큰 발급 + 공간 소속
- **권한/멤버십** — 소속은 토큰에서 유도. 남의 방은 지정해도 거부.

## 지금 구현된 것 (MVP)

`hub/`에 ports & adapters로 구현했고 테스트를 통과한다.

- `POST /spaces` — 공간 생성
- `POST /agents/register` — 온보딩 (`agent_id`·`token`·소속)
- `POST /issues`, `POST /issues/{id}/resolve` — 지식 열기·해결 (C1의 MVP REST 바인딩)

계약: [`../../contracts/c4-admin-api.json`](../../contracts/c4-admin-api.json) · 코드: [`../../hub/`](../../hub/)

## 이 API 밖의 것 (지금 보류)

품질·큐레이션은 관리 API가 아니다 — 게이트키퍼 병합, trust 검증, 지식 갱신(supersession), 민감정보 분류, 콜드스타트 시딩, 자기진화. LLM 판단이 필요한 이 영역은 **별도**이며 지금 다루지 않는다.

## 다음 슬라이스

- `search_knowledge`·`cite_knowledge` (ReuseEvent)
- MCP 어댑터 (C1의 원형)
- 실제 DB 어댑터
