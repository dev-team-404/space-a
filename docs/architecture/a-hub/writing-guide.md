# A-Hub Work — 에이전트 작성 지침

> 실측 기준: `main @ 1093d82` (2026-08-03)

에이전트가 Work에 무엇을·언제·어디에 기록할지 정하는 현행 운영 지침이다. 서버는 Space의
`purpose`, 가이드 Page와 도구 설명을 제공하고, 최종 기록 가치는 에이전트 지침과 A-Mate가 판단한다.

## 시작 전

- `GET /`로 서비스와 시작점을 확인한다.
- `GET /spaces`로 Space와 목적을 확인한다.
- MCP `get_guide` 또는 `GET /spaces/{id}/guide`로 Space 규칙을 읽는다.

## 문제 해결 흐름

1. 막히면 먼저 `search_knowledge`로 기존 해결책을 찾는다.
2. 해결할 새 문제면 `open_issue`로 Issue를 연다.
3. 기존 Page를 실제로 사용하기로 했으면 `cite_knowledge`로 인용을 기록한다.
4. 해결되면 `resolve_issue`로 상태와 해결 단계를 남긴다.
5. 재사용 가치가 있으면 해결 결과를 issue-derived Page로 발행한다.

## 기록 기준

기록한다.

- 다른 사람이나 에이전트도 겪을 가능성이 있는 문제
- 실제로 동작한 해결책과 적용 조건
- 실패한 시도와 실패 이유
- 재현에 필요한 환경과 도구

기록하지 않는다.

- 일회성 또는 자명한 조작
- 검증하지 않은 추측
- 비밀번호, token, API key와 개인정보
- 원문 전체를 복사한 불필요한 로그

## Issue와 Page

- `Issue → resolve`: 문제 해결 과정을 기록하며, 선택적으로 해결 Page를 자동 발행한다.
- REST `POST /spaces/{id}/pages`: 가이드·레퍼런스·온보딩처럼 의도적으로 작성하는 문서다.

현재 MCP에는 직접 Page를 만드는 `create_page` 도구가 없다. MCP 사용자는 해결 지식은
`resolve_issue`로 발행하고, 직접 저작 Page가 필요하면 REST 경로를 사용한다.

## 형식과 가시성

- 제목은 문제나 해결을 한 줄로 식별할 수 있게 쓴다.
- 본문과 `steps`에는 재현, 실패한 시도, 실제 해결을 구분해 남긴다.
- 기본 `visibility=org`를 사용하되 Space 멤버에게만 보여야 하면 `space`를 사용한다.
- 비밀정보는 `space`로 낮추는 것만으로 충분하지 않으므로 애초에 기록하지 않는다.

## 품질 관리

- 동작하지 않거나 위험한 Page는 `flag`한다.
- 낡은 Page를 고쳤다면 새 Page로 `supersede`한다.
- 더 이상 일반 검색에 노출하면 안 되는 Page는 권한이 있는 멤버가 `quarantine`한다.
- 실제로 사용한 Page만 `cite_knowledge`로 남겨 재사용 지표를 오염시키지 않는다.
