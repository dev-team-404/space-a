# Contracts

SPACE-A 컴포넌트 간 **경계 계약**. 세 팀원이 서로를 기다리지 않고 병렬로 작업하기 위한 전제다.

> ⚠️ **여기 있는 파일이 정답이다.** 설계 문서와 어긋나면 이 파일을 따른다.
> 📌 **현재 v2.1 (2026-08-04, 구현 실측 정합)** — v1은 권한 모델 기반이 틀려 v2에서 전면 수정했고,
> v2.1은 v2.0의 설계 표면 중 **구현되지 않은 부분을 제거**해 실제 서버 응답과 일치시켰다 (아래 참고).

| 파일 | 계약 | 제공자 → 소비자 |
|---|---|---|
| [c1-mcp-tools.json](c1-mcp-tools.json) | **C1** — MCP Tool | Space A Hub → 임직원 에이전트 / Pillar 1(A-Mate) |
| [c2-rest-api.json](c2-rest-api.json) | **C2** — 읽기 전용 REST | Space A Hub → 시각화 웹 / Pillar 3(A-Lens) |
| [c4-admin-api.json](c4-admin-api.json) | **C4** — 관리 REST (control plane) | Space A Hub → 관리 클라이언트 / 에이전트 온보딩 |
| [fixtures/](fixtures/) | 골든 데이터 | 서버 없이 먼저 작업 시작하라고 주는 것 (아래 주의 참고) |

설계 배경은 [`docs/archive/design/a-hub/05-contracts.md`](../docs/archive/design/a-hub/05-contracts.md).

## ⚠️ v2 → v2.1: 무엇이 바뀌었나 (구현 실측 정합)

v2.0은 구현 전에 설계로 그린 표면이라, 실제 서버(a-hub/work)가 만들어지면서 어긋난 부분이 생겼다.
v2.1은 **구현을 실측해 계약을 정합**시킨 판이다 — 미구현 표면을 계약에 남겨두면 소비자가 없는
API를 믿게 되므로 제거를 택했다.

| v2.0 (설계) | v2.1 (실측) |
|---|---|
| 지식 id `doc_*`, 에이전트 id `agt_*` | **`page_*`** (Page가 지식의 실체), agent_id = **user_id** |
| `search_knowledge` 응답에 confidence·solution·related | 실제 응답 필드(page_id·title·저자·시각)와 `scanned` |
| 전송 헤더 `X-Space-A-Trace-Id`/`Depth`, `rate_limited` 코드 | 미구현 — 제거. 헤더는 `Authorization`뿐 |
| C2 전용 뷰 API `/stats`·`/activity`·`/graph`, viewer_tier 4단계, 서버 측 서사 필드 | 미구현 — 제거. **파생 뷰는 A-Lens가 원천 조합으로 자체 계산** (C2 v3.0은 실제 읽기 표면을 기술) |
| `POST /agents/register` 요청 `{name, space_id}` | **`{user_id, name, space_id}`** — 사람 계정과 연결 |

fixtures 중 `stats.json`·`activity.json`·`space-detail-*.json`은 **v2.0 모양의 골든 데이터로 보존**한다
— A-Lens의 `A_LENS_SOURCE=fixtures` 데모 모드가 이 파일들을 소비하는 어댑터를 유지하기 때문이다.
현행 hub 응답 예시가 아니라는 점에 주의. `c1-lifecycle.json`은 v2.1 모양으로 갱신됐다.

## 🔒 권한 모델 — 하나의 모델, 두 개의 경로

> **에이전트 경유가 권한 우회가 되면 안 된다.**
> 사람이 UI에서 못 보는 것을 자기 에이전트에게 시켜서 볼 수 있으면 유리벽이 무의미하다.

**C1(MCP)과 C2(REST 읽기)는 같은 규칙을 쓴다:**

> 읽을 수 있는 것 = **`visibility: org` 지식** + **자기가 속한 Space의 데이터.** 그 외엔 없다.

**집행은 서버가 한다.** 소속은 Bearer 토큰에서만 유도하고 요청 파라미터를 신뢰하지 않는다.
비멤버 Space의 issues·members·tree는 403이다. (v2.0의 lobby/guest/member/manager 4단계
tier trimming은 구현 범위에서 제외됐다 — 현재 서버 측 가시성 규칙은 위 두 가지가 전부다.)

## Pillar 3(A-Lens) 담당자에게

**서버를 기다리지 마세요.** `fixtures/`로 화면을 먼저 만들 수 있습니다.

| 픽스처 | 용도 |
|---|---|
| `spaces.json` | 로비(사옥) 골든 데이터 |
| `space-detail-member.json` / `space-detail-guest.json` | 방 상세 — 멤버 vs 게스트(유리벽) 시나리오 |
| `reuse-events.json` | 지식 재사용 피드 |
| `stats.json` / `activity.json` | 통계·활동 피드 |
| `c1-lifecycle.json` | C1 왕복 예시 (v2.1 모양) |

위 골든 데이터는 초기(v2.0) 설계 모양이고, A-Lens의 fixtures 데모 모드가 그대로 소비합니다.
**실서버 연동은 C2 v3.0의 실측 표면**(`/spaces`·`/spaces/{id}/tree`·`/issues`·`/spaces/{id}/members`·
`/reuse-events`·`/pages/{id}`)을 폴링해, 통계·활동 피드·협업 지도·하이라이트·서사를 **소비자가
자체 계산**하는 구조입니다 — 서사를 LLM으로 만들지 규칙으로 만들지도 소비자의 선택이며, 실제로
A-Lens는 LLM 실패 시 규칙 폴백으로 강등합니다.

## Pillar 1(A-Mate) 담당자에게

이슈 하나의 생애주기가 **3개 호출**로 나뉩니다.

```
문제 발생  → open_issue        (status: open)
검색      → search_knowledge
재사용 결정 → cite_knowledge    (status: knowledge_linked)  ★ReuseEvent 발생
해결      → resolve_issue      (status: resolved)
```

전체 왕복 예시: [`fixtures/c1-lifecycle.json`](fixtures/c1-lifecycle.json)

**호출 시 헤더** (Tool 인자가 아니라 **전송 계층**입니다):

| 헤더 | 필수 | 내용 |
|---|---|---|
| `Authorization: Bearer <token>` | ✅ | **소속 Space(`spaces[]`)를 여기서 유도**합니다 |

> ⚠️ **`space_id`를 권한 주장용으로 보내지 마세요.** 검색 범위를 *좁히는* 용도로만 받으며,
> 서버는 토큰의 `spaces[]`와 교집합을 취합니다. 안 속한 Space는 지정해도 안 열립니다.

## 변경 규칙

C1이 깨지면 Pillar 1이 죽고, C2가 깨지면 Pillar 3이 죽습니다. **추가만 허용이 원칙입니다.**

| 허용 | 금지 |
|---|---|
| ✅ 선택 필드 추가 | ❌ 필드 제거 |
| ✅ enum 값 추가 | ❌ 필드 이름·타입 변경 |
| ✅ 응답에 필드 추가 | ❌ 기존 필드의 **의미** 변경 |

**소비자는 모르는 필드를 만나면 무시하세요.** 하드 실패 금지.

> 예외 기록: v2.1(2026-08-04)은 "계약이 정본"이라는 원칙이 실구현과 어긋난 상태를 바로잡기 위해
> **미구현 표면의 제거**를 1회 수행했다. 소비자 양쪽(A-Mate·A-Lens)이 이미 실측 모양으로 구현되어
> 있어 실제 파손은 없다. 이후는 다시 추가만 허용한다.
