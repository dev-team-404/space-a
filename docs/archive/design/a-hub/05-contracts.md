# Space A Hub — 외부 계약 (C1 / C2)

> **이 문서는 팀 병렬화의 전제다.** 계약의 존재 이유와 경계는
> [03-architecture.md §3](03-architecture.md#3-외부와의-계약-contract) 참고.
>
> 📌 **상태: v2 (확정)** — 책임자: msalt
> 기계가 읽는 스키마는 [`/contracts`](../../../../contracts/)에 있다. **이 문서와 그 파일이 어긋나면 파일이 정답이다.**

## 0. v1 → v2 — 왜 다시 썼나

v1은 [Pillar 3의 데이터 계약](../a-lens/03-architecture.md)과 대조하기 전에 확정했고, **대조해보니 v1이 틀렸다.**

| # | v1 | v2 | 왜 |
|---|---|---|---|
| 1 | `team` **문자열 하나** | **`Space`가 1급 개념** | 한 사람이 **여러 Space에 속한다.** 단일값으로는 표현 자체가 안 됨 — **권한 모델의 기반이 틀렸다** |
| 2 | `visibility: private\|team\|public` | `visibility: org\|space` | Pillar 3와 값 통일 |
| 3 | `report_issue` **해결 후 1회** | `open_issue` → `cite_knowledge` → `resolve_issue` | Pillar 3 UI가 **진행 중인 이슈 타임라인을 그린다.** 해결 후에 몰아 기록하면 화면에 나올 수가 없다 |
| 4 | 재사용 = 그래프 엣지 | **`ReuseEvent`가 1급 이벤트** | 이게 이 프로젝트의 북극성 지표다. 엣지에 묻어두면 안 된다 |
| 5 | 이슈 상태만 | **`timeline[]` 전이 이력** | 누가·언제 무엇을 했는지가 있어야 타임라인이 그려진다 |
| 6 | 절약 추정 = 전체 합계 | **재사용 1건마다** `est_saved_tokens` | 합계는 이벤트별 값의 합으로 유도 |
| 7 | 서사 = `activity.summary`만 | `status_line`·`highlight`도 **서버가 미리 생성** | 프론트가 원문을 파싱해 문장을 만들면 안 된다 |

**핵심 교훈:** 내 계약을 남의 계약과 대조하지 않고 "확정"이라고 부른 게 실수였다.
지금은 코드가 없어 고치는 비용이 0이지만, 구현 후였다면 양쪽을 다 뜯어야 했다.

## 1. 변경 정책 (Breaking Change 방지)

C1이 깨지면 Pillar 1이 죽고, C2가 깨지면 Pillar 3이 죽는다. **v2부터는 추가만 허용한다.**

| 허용 | 금지 |
|---|---|
| ✅ 선택 필드 추가 | ❌ 필드 제거 |
| ✅ enum 값 추가 (소비자는 모르는 값을 무시할 것) | ❌ 필드 이름·타입 변경 |
| ✅ 응답에 필드 추가 | ❌ 기존 필드의 **의미** 변경 |

정말 깨야 하면 **버전을 올린다**. 조용히 바꾸지 않는다.

## 2. 권한 모델 — 하나의 모델, 두 개의 경로

> ⚠️ **이 절이 계약 전체에서 가장 중요하다.**

> 🏗️ **현재 구현 현황 — 세밀한 접근 제어는 미구현.**
> 아래 권한 모델은 **설계 목표**다. 현재 MVP(`a-hub/work/`)에서는 **방 단위 멤버십 + 문서 `org`/`space` visibility**
> 두 축만 구현했다. 등급별 응답 깎기(로비/게스트/멤버/매니저), 역할, 페이지별 restriction, SSO 연동은 **의도적으로 미구현**이며
> 향후 과제로 남긴다. 구현 현황: [`a-hub/work/README.md`](../../../../a-hub/work/README.md#범위-밖--세밀한-접근-제어).

Pillar 3 설계의 요구사항을 그대로 받는다:

> "사람이 UI에서 못 보는 것을 **자기 에이전트에게 시켜서 볼 수 있으면 유리벽이 무의미해진다.**
> MCP 검색의 가시성도 이 권한 모델과 동일해야 한다."
> — [사람 뷰 권한 모델](../a-lens/01-space-model.md)

**규칙 (C1·C2 공통):**

> 에이전트/사용자가 읽을 수 있는 것 =
> **`visibility: org` 지식 문서** + **자기가 속한 Space의 모든 데이터.** 그 외에는 없다.

### 2.1 가시성 등급

| 등급 | 누가 | 받는 것 |
|---|---|---|
| **로비** | 조직 전체 | 집계 + 하이라이트 **1줄만** |
| **게스트** | 조직 전체 (비멤버 입장) | 공간·집계는 전부, 서사는 **제목 수준**, 원문 잠김 (**유리벽**) |
| **멤버** | 초대된 사람 | 서사 전부 + 원문 + 인용 그래프 |
| **매니저** | Space 오너 | + 운영 (설정·초대) |

### 2.2 `visibility`

| 값 | 의미 |
|---|---|
| `org` | **기본값.** 전사 열람 가능 — **지식은 순환하라고 있는 것이다** |
| `space` | 소유 Space 내부에서만 |

### 2.3 집행 — 서버가 한다

- **`spaces[]`(멤버십)는 토큰에서 유도한다. 요청 파라미터로 받지 않는다.**
  클라이언트가 자기 소속을 자기 입으로 주장하게 두면 권한 모델 전체가 무의미해진다.
- **프론트엔드는 아무것도 숨기지 않는다.** 서버가 **응답 자체를 등급에 맞게 깎아서** 준다.
  게스트에게 `issues`는 빈 배열로 내려간다 — "받아놓고 안 그리는" 게 아니다.
  → [`space-detail-member.json`](../../../../contracts/fixtures/space-detail-member.json) 과
    [`space-detail-guest.json`](../../../../contracts/fixtures/space-detail-guest.json) 을 **diff 해보면 바로 보인다.**

## 3. 공통 규약

- 인코딩 UTF-8, 시각은 **ISO 8601 UTC**.
- ID는 서버가 발급한다. 접두어: `iss_`, `doc_`, `skl_`, `agt_`. Space는 slug (`sw-innov`).

### 3.1 신원 (Identity)

**모든 C1 호출은 인증된다. 신원은 요청 파라미터가 아니라 전송 계층에서 온다.**

| 전송 | 방식 |
|---|---|
| HTTP (원격, 기본) | `Authorization: Bearer <token>` |
| stdio (로컬 개발) | env `SPACE_A_TOKEN` |

> **구현 상태:** stdio 전송은 **더 이상 구현되지 않는다** — Streamable HTTP + per-request
> Bearer가 유일한 전송이다. 계약(헤더 위치·클레임 구조)은 그대로이고, 구현이 계약의
> "기본"(HTTP)을 따라잡았을 뿐이다. → [ADR 0002](../../../adr/0002-mcp-http-and-skill-dual-access.md).

토큰 → `{ user_id, agent_id, spaces[] }`. **`spaces[]`가 권한의 전부다.**

> ⚠️ **미결정:** 사내 SSO(SAML/OIDC) 연동은 인프라 담당과 협의 필요.
> MVP는 **정적 토큰 → spaces[] 매핑**으로 시작하되, **헤더 위치·클레임 구조는 고정**했으므로
> 나중에 SSO로 교체해도 **C1이 안 깨진다.**

### 3.2 호출 뎁스 추적

에이전트들은 독립 프로세스라 서버가 뎁스를 자동으로 알 수 없다. **계약에 실어야 셀 수 있다.**

| 헤더 | 의미 |
|---|---|
| `X-Space-A-Trace-Id` | 하나의 해결 흐름을 묶는 ID. 최초 호출자가 생성 |
| `X-Space-A-Depth` | 몇 단계째인가. 최초 = `0` |

- `depth > MAX_DEPTH`(기본 5) → `rate_limited`로 거절.
- 헤더가 없으면 `depth=0`으로 간주하되, **`trace_id`는 서버가 발급**해 응답에 실어 준다.

> 클라이언트의 선의에 의존하는 방어다. 악의적 우회는 못 막는다.
> 사내 환경이므로 **사고 방지**가 목표이며, 최종 방어선은 서버측 Circuit Breaker다.

### 3.3 에러

| code | 상황 |
|---|---|
| `invalid_request` | 스키마 위반 |
| `unauthorized` | 토큰 없음·만료 |
| `not_found` | 없음 |
| `forbidden` | 인증은 됐으나 **내 Space가 아님** |
| `rate_limited` | 뎁스 초과 / Circuit Breaker |

---

## 4. C1 — MCP Tools (→ 임직원 에이전트)

이슈 하나의 **생애주기가 3개 호출로 나뉜다.** v1의 "해결 후 1회"를 버린 이유는 §0의 3번.

```
문제 발생 ──▶ open_issue        (status: open)
                  │
검색·판단 ──▶ search_knowledge
                  │
재사용 결정 ─▶ cite_knowledge    (status: knowledge_linked) ★ReuseEvent 발생
                  │
해결 ────────▶ resolve_issue     (status: resolved)
```

전체 왕복 예시: [`c1-lifecycle.json`](../../../../contracts/fixtures/c1-lifecycle.json)

| Tool | 언제 | 핵심 |
|---|---|---|
| `search_knowledge` | **추론하기 전에 먼저** | 결과에 `searched: {vector_hits, graph_expanded, returned}` — **토큰 절감을 증명하는 카운터** |
| `open_issue` | **문제가 생긴 시점** | `space_id` 필수. 진행 중 이슈도 Pillar 3가 그린다 |
| `cite_knowledge` | **쓰기로 결정한 순간** | **이 호출이 `ReuseEvent`를 만든다.** 이 시스템에서 가장 중요한 이벤트 |
| `resolve_issue` | 해결 후 | `publish_knowledge`(기본 `true`)로 지식 문서 발행 여부 결정 |
| `get_skill_candidates` | 필요 시 | 3회 이상 반복된 해결 패턴 |

**`open_issue`의 고아 레코드 문제** — v1이 이걸 피하려고 1회 호출을 택했었다.
v2는 기록을 **거부하는 대신** 서버측 **staleness sweeping**(오래된 `open` 이슈 정리)으로 다룬다.
진행 중인 이슈를 못 그리는 대가가 더 크다.

## 5. C2 — 읽기 전용 REST (→ Pillar 3)

**쓰기 경로는 C1(MCP)뿐이다.** Pillar 3은 **Graph DB에 직접 붙지 않는다**
([03 §1](03-architecture.md#1-컴포넌트-경계--가장-큰-함정-두-가지)).

| Endpoint | 용도 | 픽스처 |
|---|---|---|
| `GET /spaces` | 로비 — 사옥의 층들 | [spaces.json](../../../../contracts/fixtures/spaces.json) |
| `GET /spaces/{id}` | 스페이스 상세 — **등급에 따라 응답이 깎여 나감** | [member](../../../../contracts/fixtures/space-detail-member.json) / [guest](../../../../contracts/fixtures/space-detail-guest.json) |
| `GET /reuse-events` | **북극성 피드** — 지식이 팀 경계를 넘은 기록 | [reuse-events.json](../../../../contracts/fixtures/reuse-events.json) |
| `GET /graph` | 관계망 네트워크 맵 | — |
| `GET /stats` | 집계 대시보드 | [stats.json](../../../../contracts/fixtures/stats.json) |
| `GET /activity` | 관전 피드 | [activity.json](../../../../contracts/fixtures/activity.json) |

### 5.1 서사(narrative)는 **제공**하지, 강요하지 않는다

> ⚠️ **이전 초안의 잘못을 바로잡는다.** v2 초안은 "프론트가 조립하면 안 된다(must NOT assemble)"고
> 못 박았는데, 이건 **내가 정할 일이 아니었다.**
>
> Pillar 3 문서는 서사 생성 주체를 **"가장 중요한 열린 질문"**으로 남겨뒀다
> ([사람 뷰 열린 질문 Q1](../a-lens/README.md)): *에이전트 기록 시 vs 백엔드 LLM 배치 vs 뷰 서버.*
> **그쪽이 아직 안 정했다고 명시한 질문을, 내 계약이 몰래 닫아버린 것이다.**
>
> **어떤 화면을 에이전트가 만들고 어떤 화면을 결정론적으로 그릴지는 시각화 서비스가 정한다.**
> 계약은 **양쪽 재료를 다 주고 빠진다.**

C2는 두 가지를 **항상 함께** 내려준다. 소비자가 고른다.

| | 내용 | 쓰임 |
|---|---|---|
| **구조화 필드** | `type`, `actor`, `space_id`, `doc_id`, `at`, 카운트, 상태 | 직접 문장을 조립하거나, **에이전트에게 먹이거나** |
| **서사 필드** | `summary`, `status_line`, `highlight`, `chain[].label`, 노드 `label` | 그대로 렌더하고 싶으면 **바로 씀** |

**서버는 어느 쪽을 쓰든 상관하지 않는다.** 화면마다 다르게 골라도 된다.

**다만 트레이드오프는 알려준다** (선택을 밀기 위해서가 아니라, 재료로 쓰라고):
소비자가 직접 조립하면 **이벤트 타입이 늘 때마다 소비자 쪽을 고쳐야** 하고,
서사 필드를 쓰면 안 고쳐도 된다.

### 5.2 이건 협상 대상이 아니다

| 항목 | 이유 |
|---|---|
| **등급별 응답 깎기** | **누가 무엇을 볼 수 있는가는 렌더링 선택이 아니다.** 서버가 집행한다. 게스트용 문장은 **누가 쓰든** 작업 내용을 담으면 안 된다 |
| **읽기 전용** | 지식을 쓰는 경로는 C1뿐 |

### 5.3 추정치는 정직하게

`est_saved_tokens`·`tokens_saved_est`는 **추정**이다. `~`를 붙여 표기한다.
산식은 서버 책임이며, 정확할 필요는 없고 **일관되면 된다.**

---

## 6. 이 계약이 지키는 것

| 결정 | 막아내는 사고 |
|---|---|
| **`spaces[]`를 토큰에서 유도** | 클라이언트가 **소속을 사칭**해 남의 방 지식을 긁어감 |
| **C1·C2가 같은 권한 모델** | **에이전트를 시켜 UI 권한을 우회**하는 것 |
| **서버가 응답을 깎아서 내려줌** | 프론트 버그 하나가 곧 정보 유출이 되는 것 |
| `visibility` 기본 `org` | 지식이 고여서 순환하지 않는 것 |
| `depth`/`trace_id` | 에이전트 핑퐁 무한 루프 |
| C2 읽기 전용 | Pillar 3이 지식을 오염시키는 것 |
| C2에 DB 커넥션 미공유 | 그래프 스키마가 **굳어버리는 것** |
| **구조화·서사 필드를 둘 다 제공** | 계약이 **남의 설계 결정을 대신 내리는 것** (§5.1) |
| 추가만 허용 | Pillar 1/3이 **조용히 깨지는 것** |

## 7. 다음 단계

- [x] C1 v2 — Space 기반, 이슈 생애주기 3단계
- [x] C2 v2 — Space·ReuseEvent·timeline·서사 캐시
- [x] 픽스처 7종 (**게스트/멤버 diff 포함**)
- [ ] **Pillar 3 담당자 확인** — 내가 그쪽 계약을 맞게 읽었는지
- [ ] Pillar 1 담당자 확인 — 이슈 생애주기 3단계 호출이 에이전트에 부담은 아닌지
- [ ] 합의되면 **ADR로 승격**
