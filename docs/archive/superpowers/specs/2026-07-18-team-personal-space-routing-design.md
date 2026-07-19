---
status: done
archived: 2026-07-19
---

# team/personal 두 갈래 기록 라우팅 설계 (스킬 + MCP)

**날짜:** 2026-07-18
**상태:** 승인됨 (구현 예정)
**범위:** 스킬 문서 + MCP docstring. **서버 코드 무변경.**

## 목표

에이전트 기록을 콘텐츠의 공유 가치에 따라 두 곳으로 나눠 남긴다:

- **공유 가치 있음**(남에게 도움되는 지식·해결·사실·가이드) → **팀(공유) 공간**
- **나만 볼 것**(개인 메모·임시 기록·작업 로그) → **개인 공간**

그래야 팀 공간엔 재사용 가치 있는 것만 모여 의미가 유지된다.

## 결정

| 항목 | 결정 |
|------|------|
| 정체성·기본 팀공간 | `SPACE_A_USER`(= register user_id), `SPACE_A_SPACE`(기본 팀 공간 id) |
| 개인 공간 표현 | 규칙 기반 `personal-<user_id>` (서버 1급 개념 아님) |
| 분류 기준 | 공유 가치 — Claude가 내용으로 판단(팀 vs 개인) |
| 개인 공간 부재 | 지침대로 없으면 `POST /spaces`로 생성 후 사용 |
| 구현 위치 | 스킬(클라이언트) 문서 + MCP docstring. 서버 무변경 |
| MCP | 저장 도구가 resolve_issue뿐 → docstring에 visibility 안내만. team/personal 완전 분리는 스킬(REST) 담당 |

## 변경 상세

### 1. 스킬 — SKILL.md

환경변수 추가:
- `SPACE_A_SPACE` — 기본 팀(공유) 공간 id
- `SPACE_A_USER` — 사용자 식별자(= register user_id). 개인 공간 id = `personal-$SPACE_A_USER`

대원칙에 라우팅 규칙 추가:
- 공유 가치 → 팀 공간 `$SPACE_A_SPACE`
- 나만 볼 것 → 개인 공간 `personal-$SPACE_A_USER`
- 개인 공간 없으면 `POST /spaces`로 먼저 생성

### 2. 스킬 — endpoints.md

- space_id를 쓰는 예시(create_page·open_issue·search·list_issues)에 "팀이면 `$SPACE_A_SPACE`,
  개인이면 `personal-$SPACE_A_USER`" 선택 안내.
- 개인 공간 셋업 스니펫(공간 생성 — 이미 존재 시 400 무시 → 같은 user_id 재-register로 멤버십 병합) 추가.

### 3. MCP — mcp_server.py (docstring만)

`resolve_issue` docstring에 추가:
- 공유 가치 있으면 `visibility="org"`, 개인적 해결이면 `visibility="space"`.
- MCP는 이슈가 열린 space에 발행하며, 개인 공간 신규 생성·자유 페이지 저작은 MCP 범위 밖(REST/스킬 담당)임을 명시.

도구 시그니처·서버 로직 무변경.

## 검증

- 순수 문서 변경 → 서버 테스트 영향 없음(회귀 확인만).
- 배포 서버에서 스킬 지침대로 curl: ① 팀 공간 기록 ② 개인 공간 없으면 생성 후 기록 ③ 개인 공간 created_by 정상.

## 범위 밖

- MCP에 create_page·create_space 추가 (별도 작업)
- 서버의 personal space 1급 개념화
- 개인 공간 접근 제어(남이 personal-X 열람 차단) — 현재 visibility 모델 그대로
