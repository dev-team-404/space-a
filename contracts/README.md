# Contracts

SPACE-A 컴포넌트 간 **경계 계약**. 세 팀원이 서로를 기다리지 않고 병렬로 작업하기 위한 전제다.

> ⚠️ **여기 있는 파일이 정답이다.** 설계 문서와 어긋나면 이 파일을 따른다.
> 📌 **현재 v2** — v1은 Pillar 3의 데이터 계약과 대조하기 전에 만들어져 **틀렸다** (아래 참고).

| 파일 | 계약 | 제공자 → 소비자 |
|---|---|---|
| [c1-mcp-tools.json](c1-mcp-tools.json) | **C1** — MCP Tool | Space A Hub → 임직원 에이전트 / Pillar 1 |
| [c2-rest-api.json](c2-rest-api.json) | **C2** — 읽기 전용 REST | Space A Hub → 시각화 웹(Pillar 3) |
| [fixtures/](fixtures/) | 응답 예시 | **서버 없이 먼저 작업 시작하라고 주는 것** |

설계 배경은 [`docs/design/collab-space/05-contracts.md`](../docs/design/collab-space/05-contracts.md).

## ⚠️ v1 → v2: 무엇이 바뀌었나

v1은 **`team` 문자열 하나**로 권한을 다뤘는데, **한 사람이 여러 Space에 속하므로** 표현 자체가 불가능했다.
권한 모델의 기반이 틀렸던 것이라 v2에서 전면 수정했다.

| v1 | v2 |
|---|---|
| `team` 문자열 | **`Space` 1급 개념**, 토큰 클레임은 `spaces[]` |
| `private\|team\|public` | `org\|space` |
| `report_issue` 1회 | `open_issue` → `cite_knowledge` → `resolve_issue` |
| 재사용 = 그래프 엣지 | **`ReuseEvent` 1급 이벤트** |

## 🔒 권한 모델 — 하나의 모델, 두 개의 경로

> **에이전트 경유가 권한 우회가 되면 안 된다.**
> 사람이 UI에서 못 보는 것을 자기 에이전트에게 시켜서 볼 수 있으면 유리벽이 무의미하다.

**C1(MCP)과 C2(UI)는 같은 규칙을 쓴다:**

> 읽을 수 있는 것 = **`visibility: org` 지식** + **자기가 속한 Space의 데이터.** 그 외엔 없다.

**집행은 서버가 한다.** 프론트는 아무것도 숨기지 않는다 — 서버가 **응답을 등급에 맞게 깎아서** 준다.

```sh
# 유리벽이 무엇을 가리는지 직접 보기
diff fixtures/space-detail-member.json fixtures/space-detail-guest.json
```

## Pillar 3(시각화) 담당자에게

**서버를 기다리지 마세요.** `fixtures/`가 실제 응답과 같은 모양입니다.

| 픽스처 | 엔드포인트 |
|---|---|
| `spaces.json` | `GET /spaces` — 로비(사옥) |
| `space-detail-member.json` | `GET /spaces/sw-innov` — **멤버** |
| `space-detail-guest.json` | `GET /spaces/sw-innov` — **게스트 (유리벽)** |
| `reuse-events.json` | `GET /reuse-events` — 지식 재사용 피드 |
| `stats.json` | `GET /stats` |
| `activity.json` | `GET /activity` |

`docs/design/frontend-viz/03-architecture.md §4`의 데이터 계약을 **제가 맞게 읽었는지 확인 부탁드립니다.**
특히 이 세 가지를 반영했습니다.

1. **ReuseEvent를 1급 이벤트로** (요청 #1) — `/reuse-events` 전용 엔드포인트
2. **서사 캐시** (요청 #2) — `status_line`·`highlight`·`chain[].label`을 **서버가 완성된 문장으로** 내려줌.
   프론트에서 조립하지 마세요. 조립하면 이벤트 타입이 늘 때마다 프론트를 고쳐야 합니다.
3. **타임라인 보존** (요청 #3) — `issues[].timeline[]`에 전이 이력 전체
4. **절약 추정치는 서버 책임** (요청 #6) — `est_saved_tokens`를 **재사용 1건마다**.
   반드시 `~`를 붙여 표기해주세요. 추정치입니다.
5. **MCP도 같은 권한 모델** (요청 #7) — C1에 명시했습니다

## Pillar 1(코칭 Agent) 담당자에게

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
| `X-Space-A-Trace-Id` / `X-Space-A-Depth` | | 무한 루프 방지. 최초 호출은 `depth=0` |

> ⚠️ **`space_id`를 권한 주장용으로 보내지 마세요.** 검색 범위를 *좁히는* 용도로만 받으며,
> 서버는 토큰의 `spaces[]`와 교집합을 취합니다. 안 속한 Space는 지정해도 안 열립니다.

**3단계 호출이 에이전트에 부담인지 알려주세요.** Pillar 3의 진행 중 이슈 타임라인 때문에
`open_issue`를 분리했는데, 실제 에이전트 동선에서 과한지는 그쪽이 더 잘 압니다.

## 변경 규칙

C1이 깨지면 Pillar 1이 죽고, C2가 깨지면 Pillar 3이 죽습니다. **v2부터 추가만 허용합니다.**

| 허용 | 금지 |
|---|---|
| ✅ 선택 필드 추가 | ❌ 필드 제거 |
| ✅ enum 값 추가 | ❌ 필드 이름·타입 변경 |
| ✅ 응답에 필드 추가 | ❌ 기존 필드의 **의미** 변경 |

**소비자는 모르는 필드를 만나면 무시하세요.** 하드 실패 금지.
