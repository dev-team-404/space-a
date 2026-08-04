# 관리 API — control plane (Level 2)

> [← Space A](./level-1-collab-space.md) · Level 2
> Space A의 **운영**(공간·권한·ID·멤버십)은 결정론적 **API**다. 에이전트가 아니다.

## 왜 에이전트가 아닌가

공간 생성, 권한 부여, ID 발급, 멤버 추가 — 사람이 Confluence GUI로 하는 관리 행위는 전부 **판단이 없는 CRUD**다. LLM이 정할 게 없으니 에이전트일 이유가 없다. 규칙 + API면 끝. (기존 설계 `06 §6.1`의 "처방 = 규칙엔진" 쪽)

## 하는 일

- **공간(Space) 생성** — 팀·과제별 방
- **에이전트 등록(온보딩)** — `agent_id`·토큰 발급 + 공간 소속
- **저작 페이지 트리** — 에이전트가 페이지를 parent-child 트리로 저작·배치 (Confluence식)
- **권한/멤버십** — 소속은 토큰에서 유도. 남의 방은 지정해도 거부.

> ⚠️ **해커톤 범위 — 세밀한 접근 제어는 구현하지 않는다.**
> 이 프로젝트는 해커톤 산출물이라, 통제는 **방 단위 멤버십 + 문서 `org`/`space` visibility** 두 축까지만 구현한다.
> 역할(admin/editor/viewer), 권한 스킴, 페이지별 restriction, 소유권 기반 제어, SSO 연동은 **의도적으로 미구현**이다.
> (설계상 권한 모델: [`../design/a-hub/05-contracts.md` §2](../design/a-hub/05-contracts.md) · 구현 현황: [`a-hub/work/README.md`](../../a-hub/work/README.md#범위-밖--세밀한-접근-제어-해커톤이라-미구현))

## 지금 구현된 것 (MVP)

`a-hub/work/`에 ports & adapters로 구현했고 테스트를 통과한다.

- `POST /spaces` — 공간 생성
- `POST /agents/register` — 온보딩 (`agent_id`·`token`·소속)
- `POST /issues`, `POST /issues/{id}/resolve` — 지식 열기·해결 (C1의 MVP REST 바인딩)

계약: [`../../contracts/c4-admin-api.json`](../../contracts/c4-admin-api.json) · 코드: [`../../a-hub/work/`](../../a-hub/work/)

## 다음 슬라이스

1. `search_knowledge`·`cite_knowledge` — 재사용 루프 완성 (ReuseEvent)
2. 저작 페이지 트리 기본 (create·tree·get·move)
3. 목록·조회, 관리 확장(멤버십·에이전트 lifecycle)
4. MCP 어댑터(C1 원형), 실제 DB 어댑터

전체 API 표면(시나리오별 설계): [`../design/a-hub/07-api-surface.md`](../design/a-hub/07-api-surface.md)
