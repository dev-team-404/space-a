# Space A Hub — 외부 계약 (C1 / C2)

> **이 문서는 팀 병렬화의 전제다.** 여기 적힌 계약이 고정되면 세 컴포넌트가 동시에 작업할 수 있다.
> 계약의 존재 이유와 경계는 [03-architecture.md §3](03-architecture.md#3-외부와의-계약-contract) 참고.
>
> 📌 **상태: 확정 (v1)** — 책임자: msalt
> 기계가 읽는 스키마는 [`/contracts`](../../../contracts/)에 있다. **이 문서와 그 파일이 어긋나면 파일이 정답이다.**

## 0. 변경 정책 (Breaking Change 방지)

C1이 깨지면 Pillar 1이 죽고, C2가 깨지면 Pillar 3이 죽는다. 그래서 변경 규칙을 먼저 못 박는다.

| 허용 | 금지 |
|---|---|
| ✅ **선택 필드 추가** | ❌ 필드 제거 |
| ✅ enum 값 추가 (소비자는 모르는 값을 무시해야 함) | ❌ 필드 이름·타입 변경 |
| ✅ 응답에 필드 추가 | ❌ 기존 필드의 **의미** 변경 |

- 소비자(Pillar 1/3)는 **모르는 필드를 만나면 무시**한다. 하드 실패 금지.
- 정말 깨야 하면 **버전을 올린다** (`search_knowledge_v2`). 조용히 바꾸지 않는다.

## 1. 공통 규약

- 인코딩 UTF-8, 시각은 **ISO 8601 UTC** (`2026-07-12T09:30:00Z`).
- ID는 서버가 발급한다. 클라이언트가 지어내지 않는다.
- 모든 ID는 접두어를 갖는다: `iss_`, `sol_`, `skl_`, `agt_`.

### 1.1 권한 모델 (visibility)

> [02-features.md §6.2](02-features.md#62-권한-분리-cross-contamination-방지)의 "교차 오염 방지"를 계약 수준에서 강제한다.

| 값 | 의미 |
|---|---|
| `private` | 기록한 에이전트/사용자만 검색됨 |
| `team` | **기본값.** 같은 팀만 검색됨 |
| `public` | 전사 검색됨 |

**규칙: 서버 기본값은 `team`. 에이전트는 좁힐 수만 있고, 넓히려면 명시해야 한다.**

- `visibility`를 **생략하면 `team`** 이 된다. 에이전트가 판단을 안 해도 기밀이 전사로 새지 않는다.
- 에이전트는 `private`으로 **좁히는 건 자유**다 (민감하다 싶으면 잠근다).
- `public`으로 **넓히는 건 명시적 선언**이어야 한다.
- 검색은 **호출자의 권한 범위 안에서만** 결과를 준다. 서버가 필터링하며, 클라이언트를 믿지 않는다.

### 1.2 신원 (Identity) — 권한 모델의 전제

> **§1.1이 성립하려면 서버가 "호출자가 누구인지"를 알아야 한다.**
> `team` 필터링은 호출자의 팀을 모르면 아예 계산이 안 된다.
> MCP 표준의 인증 스펙은 아직 성숙하지 않으므로, **애플리케이션 레이어에서 우리가 정의한다.**

**모든 C1 호출은 인증된다. 신원은 요청 파라미터가 아니라 전송 계층에서 온다.**

| 전송 | 신원 전달 방식 |
|---|---|
| HTTP (원격, 기본) | `Authorization: Bearer <token>` 헤더 |
| stdio (로컬 개발) | 프로세스 기동 시 env `SPACE_A_TOKEN` |

- 토큰은 **사내 SSO에서 발급**하며, 서버는 이를 검증해 `{ user_id, agent_id, team, scopes }`를 복원한다.
- **`team`은 토큰에서 나온다. 요청 본문에서 받지 않는다.**
  클라이언트가 자기 팀을 자기 입으로 주장하게 두면 권한 모델 전체가 무의미해진다.
- 마찬가지로 `report_issue`의 기록자(`agent_id`)도 **토큰에서 유도한다.** 사칭 방지.
- 토큰이 없거나 만료 → `unauthorized`. 범위 밖 리소스 접근 → `forbidden`.

> ⚠️ **미결정:** 사내 SSO(SAML/OIDC) 연동 방식은 인프라 담당과 협의 필요.
> MVP에서는 **사전 발급된 정적 토큰 → team 매핑 테이블**로 시작하고, 계약(헤더 위치·클레임 구조)은
> 지금 고정해 나중에 SSO로 교체해도 **C1이 안 깨지게** 한다.

### 1.3 호출 뎁스 추적 (Circuit Breaker의 전제)

> [02 §6.3](02-features.md#63-무한-루프--과금-폭주-방지)의 "호출 뎁스 제한"도 **뎁스를 실제로 셀 수 있어야** 성립한다.
> 에이전트들은 서로 독립적인 프로세스라, 서버가 자동으로 알 방법이 없다. **계약에 실어야 한다.**

모든 C1 호출은 **추적 메타데이터**를 함께 보낸다 (신원과 같은 경로 — 헤더/env).

| 필드 | 헤더 | 의미 |
|---|---|---|
| `trace_id` | `X-Space-A-Trace-Id` | 하나의 문제 해결 흐름을 묶는 ID. 최초 호출자가 생성 |
| `depth` | `X-Space-A-Depth` | 이 호출이 몇 단계째인가. 최초 = `0` |

**규칙**

- 최초 호출: `trace_id` 새로 생성, `depth = 0`.
- Space A의 응답을 보고 다른 에이전트를 부르게 되면 → 같은 `trace_id`, `depth + 1`.
- **서버는 `depth > MAX_DEPTH`(기본 5)면 `rate_limited`로 거절**한다.
- `trace_id` 단위로 **일일 토큰/호출 상한**을 걸어 폭주를 끊는다.
- 두 헤더가 없으면 서버가 `depth = 0`으로 간주하되, **`trace_id`는 서버가 발급해 응답에 실어 준다.**
  (클라이언트가 협조하지 않아도 서버 쪽 집계는 유지된다.)

> 이건 **클라이언트의 선의에 의존하는 방어**다. 악의적 우회는 못 막는다.
> 사내 환경이므로 **사고 방지**가 목표이지 공격 방어가 목표는 아니다.
> 최종 방어선은 서버 측의 **토큰/호출 총량 Circuit Breaker**다.

### 1.4 에러

```json
{ "error": { "code": "invalid_request", "message": "사람이 읽을 설명" } }
```

| code | 상황 |
|---|---|
| `invalid_request` | 스키마 위반 |
| `unauthorized` | 토큰 없음·만료·검증 실패 (§1.2) |
| `not_found` | 해당 ID 없음 |
| `forbidden` | 인증은 됐으나 권한 범위 밖 |
| `rate_limited` | 뎁스 초과 또는 Circuit Breaker 발동 (§1.3) |

---

## 2. C1 — MCP Tools (→ 임직원 에이전트)

에이전트가 쓰는 도구 3개. 이것이 Pillar 1과 모든 임직원 에이전트가 보는 면이다.

### 2.1 `search_knowledge` — 유사 사례 검색

에이전트가 **막혔을 때 가장 먼저 부르는 것.**
반환값의 목적은 "많이 주는 것"이 아니라 **"적은 토큰으로 풀리게 하는 것"**이다
([01 §6.2](01-product.md#62-검색의-목적은-정확도가-아니라-토큰-절감)).

**Input**

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `query` | string | ✅ | 에러 메시지나 문제 상황. 원문 그대로 넣어도 됨 |
| `context` | object | | `{ language, framework, os, tools[] }` — 그래프 탐색의 앵커 |
| `limit` | int (1~10) | | 기본 3. **크게 잡을수록 토큰이 샌다** |

**Output**

```json
{
  "results": [
    {
      "issue_id": "iss_a1b2c3",
      "title": "pnpm install 시 ERR_PNPM_PEER_DEP_ISSUES",
      "confidence": 0.87,
      "solution": {
        "summary": "peerDependencyRules로 무시 규칙 추가",
        "steps": ["package.json에 pnpm.peerDependencyRules 추가", "pnpm install --no-frozen-lockfile"],
        "required_tools": ["pnpm"]
      },
      "tried_and_failed": ["npm install로 우회 (lockfile 충돌)"],
      "related": { "tools": ["pnpm"], "skills": ["skl_dep_fix"] },
      "reuse_count": 4,
      "recorded_at": "2026-07-08T02:11:00Z"
    }
  ],
  "searched": { "vector_hits": 12, "graph_expanded": 5, "returned": 1 }
}
```

**설계 의도 세 가지**

- `tried_and_failed` — **실패 기록이 성공 기록만큼 중요하다.** 다음 에이전트가 같은 삽질을 반복하지 않게 한다.
- `related` — Vector 단독으로는 못 찾는 것. **Graph가 확장한 결과**다.
- `searched` — "12개 찾아서 1개만 줬다"를 보여준다. **데모에서 토큰 절감을 증명하는 필드**다.

### 2.2 `report_issue` — 해결 후 1회 기록

> **호출 단위 결정: 해결 후 1회.** 발생 시점에 `open`, 해결 시점에 `resolve`로 나누는 안은 기각했다.
> 에이전트가 `resolve`를 안 부르고 죽으면 **미해결 고아 레코드가 쌓이는데**, 그건 지식이 아니라 쓰레기다.
> 미해결 문제 추적은 P2P Ask([02 §5](02-features.md#5-peer-to-peer-ask-라우팅))가 나중에 다룬다.

**Input**

| 필드 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `issue` | object | ✅ | `{ title, context, error_message?, environment? }` |
| `tried_steps` | array | | **실패한 시도들.** `[{ step, outcome: "failed"\|"partial", note? }]` |
| `solution` | object | ✅ | `{ summary, steps[], required_tools[] }` |
| `visibility` | enum | | `private`\|`team`\|`public`. **생략 시 `team`** (§1.1) |
| `source_issue_id` | string | | **재사용해서 풀었다면 원본 ID.** 지식 순환의 증거 |

**Output**: `{ "issue_id": "iss_...", "status": "recorded" | "merged", "merged_into": "iss_..." }`

- `merged` — 로컬 LLM 게이트키퍼가 **기존 사례와 중복**이라 판단하면 새로 만들지 않고 병합한다.
  이것이 [지식 비대화](02-features.md#3-심야-지식-압축-루프-knowledge-condensation)를 막는 1차 방어선이다.
- `source_issue_id`가 쌓이면 **"이 지식이 몇 번 재사용됐나"**(`reuse_count`)가 나온다.
  Pillar 3의 지식 전파 경로 시각화가 이걸 먹는다.

### 2.3 `get_skill_candidates` — Skill 승격 후보

같은 해결책이 **3회 이상 반복**되면 자동화 신호다 ([02 §4](02-features.md#4-skill-자동-생성-제안)).

**Input**: `{ scope?: "me" | "team" | "org", min_occurrences?: int (기본 3) }`

**Output**

```json
{
  "candidates": [
    {
      "skill_id": "skl_dep_fix",
      "pattern": "pnpm peer dependency 충돌 해결",
      "occurrences": 7,
      "issue_ids": ["iss_a1b2c3", "iss_d4e5f6"],
      "proposed_skill": { "name": "fix-pnpm-peer-deps", "description": "...", "steps": [] },
      "status": "proposed"
    }
  ]
}
```

---

## 3. C2 — 읽기 전용 REST (→ Pillar 3 시각화)

> **Pillar 3은 Graph DB에 직접 붙지 않는다.** 붙는 순간 내 그래프 스키마가 프론트의 공개 API가 되어
> 진화가 멈춘다 ([03 §1](03-architecture.md#1-컴포넌트-경계--가장-큰-함정-두-가지)).
> **DB 커넥션 스트링은 공유하지 않는다.** 이 세 엔드포인트가 전부다.

읽기 전용이다. **쓰기 경로는 C1(MCP)뿐이다.**

### 3.1 `GET /graph` — 관계망 (네트워크 맵)

`?team=&since=&limit=` — 노드/엣지를 그대로 준다. 프론트는 이걸 그리기만 하면 된다.

```json
{
  "nodes": [
    { "id": "agt_kim", "type": "Agent", "label": "김OO의 에이전트", "team": "platform" },
    { "id": "iss_a1b2c3", "type": "Issue", "label": "pnpm peer dep 충돌", "reuse_count": 4 },
    { "id": "skl_dep_fix", "type": "Skill", "label": "fix-pnpm-peer-deps" }
  ],
  "edges": [
    { "source": "agt_kim", "target": "iss_a1b2c3", "type": "RESOLVED", "at": "2026-07-08T02:11:00Z" },
    { "source": "agt_lee", "target": "iss_a1b2c3", "type": "REUSED", "at": "2026-07-10T05:20:00Z" }
  ]
}
```

- 노드 타입: `Agent` `Task` `Issue` `MCP_Tool` `Skill` — [02 §2.2](02-features.md#22-그래프-모델)의 모델 그대로.
- **`REUSED` 엣지가 이 프로젝트의 핵심 서사다.** "A가 푼 걸 B가 재사용했다" = 지식이 조직을 건넜다는 증거.
  Pillar 3은 이 엣지를 강조해서 그리면 된다.
- `private` 지식은 **애초에 이 응답에 안 나온다.** 서버가 거른다.

### 3.2 `GET /stats` — 집계 대시보드

```json
{
  "period": { "from": "2026-07-01", "to": "2026-07-12" },
  "totals": { "issues": 143, "solutions": 128, "skills": 9, "reuses": 61 },
  "top_reused_skills": [{ "skill_id": "skl_dep_fix", "name": "fix-pnpm-peer-deps", "reuse_count": 23 }],
  "top_issues": [{ "issue_id": "iss_a1b2c3", "title": "pnpm peer dep 충돌", "occurrences": 12 }],
  "tokens_saved_est": 412000,
  "by_team": [{ "team": "platform", "contributed": 52, "reused": 31 }]
}
```

- `tokens_saved_est` — **추정치다.** "원본 로그를 다 넣었을 때 대비 절감량". 프론트는 `~` 라벨을 붙여 정직하게 표기한다.
- `by_team`의 `contributed` vs `reused` — 어느 팀이 **주는 팀**이고 어느 팀이 **받는 팀**인지 보인다.

### 3.3 `GET /activity` — 관전용 피드

`?limit=&since=` — 사람이 "관전"하는 실시간 흐름 ([README 30초 요약](README.md#30초-요약)).

```json
{
  "events": [
    { "at": "2026-07-12T09:30:00Z", "type": "reused",
      "actor": "agt_lee", "issue_id": "iss_a1b2c3",
      "summary": "이OO의 에이전트가 김OO 에이전트의 해결책을 재사용했습니다" }
  ]
}
```

이벤트 타입: `recorded` (새 지식) · `reused` (재사용) · `condensed` (야간 압축) · `skill_proposed` (승격 제안).

`summary`는 **서버가 한국어 문장으로 만들어 준다.** 프론트가 문장을 조립하지 않는다 —
그러면 문구가 프론트에 하드코딩되어 서버가 이벤트 타입을 추가할 때마다 프론트를 고쳐야 한다.

---

## 4. 이 계약이 지키는 것

| 계약 결정 | 막아내는 사고 |
|---|---|
| **`team`을 토큰에서 유도** (요청 본문 아님) | 클라이언트가 **팀을 사칭**해 남의 지식을 긁어가는 것 |
| `visibility` 생략 시 `team` | 에이전트가 판단을 안 해도 **기밀이 전사로 새지 않음** |
| 검색 필터링을 서버가 수행 | 클라이언트를 믿지 않음 |
| `depth` / `trace_id` 를 계약에 실음 | 에이전트 핑퐁 **무한 루프**와 과금 폭주 |
| `report_issue` 1회 호출 | **미해결 고아 레코드**가 안 쌓임 |
| `merged` 응답 | 중복 지식 누적 = 검색 토큰 폭발 방지 |
| C2가 읽기 전용 | Pillar 3이 지식을 오염시킬 수 없음 |
| C2에 DB 커넥션 미공유 | 그래프 스키마를 **계속 바꿀 수 있음** |
| 필드 추가만 허용 | Pillar 1/3이 **조용히 깨지지 않음** |

## 5. 다음 단계

- [x] C1 시그니처 확정
- [x] C2 스키마 확정
- [x] Pillar 3용 픽스처 제공 → [`/contracts/fixtures/`](../../../contracts/fixtures/)
- [ ] Pillar 1 / Pillar 3 담당자 리뷰 → 이견 없으면 **ADR로 승격**
- [ ] `core/ports.py` 를 이 계약에 맞춰 작성
