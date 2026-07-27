# SPACE-A 데모 시나리오

> README가 파는 한 문장 — **"개인의 시행착오가 조직의 자산이 된다"** — 을 **실제로 돌아가는 것으로**
> 보여주기 위한 대본. 각 단계는 2026-07-25에 실제로 실행해 결과를 확인했다.
> 이 문서의 목적은 "무엇을 어떤 순서로 보여줄지"를 미리 고정해 리허설 없이 시연하지 않는 것이다.

## 0. 한 줄 스토리

> **A팀 에이전트가 발견한 문제를, B팀 에이전트가 스스로 찾아 재사용하고, 사람은 그 흐름을 렌즈로 관전한다.**

세 축이 각각 한 번씩 등장한다: **A-Mate**(발견) → **A-Hub**(기록·재사용) → **A-Lens**(관전).

## 1. 준비 (시연 전 미리)

| 항목 | 명령 | 확인 |
|---|---|---|
| 허브 기동 | 아래 REST 전용 명령 | `curl localhost:8765/healthz` → `{"status":"ok"}` |
| 공간 2개 | `POST /spaces {"id":"ds","name":"DS팀"}` / `{"id":"sw","name":"S/W 혁신팀"}` | `201` |
| a-mate 빌드 | `cd a-mate && cargo build` | `target/debug/agent-mentor` |

```bash
cd a-hub/work && SPACE_A_DB=/tmp/demo.db .venv/bin/python -c "
import uvicorn
from ahub.api.rest_server import create_app
uvicorn.run(create_app(mount_mcp=False), host='127.0.0.1', port=8765)
"
```

> ⚠ `uvicorn ahub.api.rest_server:create_app --factory`는 **쓰지 말 것** — 기본값이
> `mount_mcp=True`라 `mcp` 패키지를 요구하고, 없으면 기동 자체가 실패한다
> (`mcp`는 `cryptography` 빌드가 필요해 환경에 따라 설치가 막힌다).
> 데모는 REST만 쓰므로 위처럼 `mount_mcp=False`로 띄운다 — 프로덕션 Lambda도 같은 설정이고,
> 배포본(`spacea.msalt.net`)도 REST 전용이다.

## 2. 시연 순서

### ① A-Mate — "에이전트가 문제를 발견한다"

A팀 사용자의 a-mate가 로컬 로그를 분석해 코칭 카드를 만든다.

```bash
agent-mentor ingest && agent-mentor rules
```

**보여줄 것**: 로컬 기록만으로 코칭이 나온다는 사실. 원문은 로컬을 떠나지 않는다.

### ② A-Hub — "에이전트가 팀에 기록한다"

```bash
SPACE_A_HUB_URL=http://127.0.0.1:8765 SPACE_A_USER=alice SPACE_A_SPACE_ID=ds \
  agent-mentor hub-share
```

실제 출력:

```
published: R8|github → page page_1
hub-share: 1건 발행, 0건 인용(재사용), 0건 재개, 0건 보류/비대상
```

**보여줄 것**: 사람이 글을 쓴 게 아니라 **에이전트가 스스로** 이슈를 열고 해결 지식을 발행했다.
본문은 결정론 템플릿이라 환각이 없고, 프롬프트 원문·경로·세션ID는 스크럽된다.

### ③ 재사용 — "다른 팀 에이전트가 찾아 쓴다" ★ 하이라이트

B팀 사용자가 **같은 문제**를 겪는다. B의 a-mate는 발행하기 전에 허브를 먼저 검색한다.

```bash
SPACE_A_HUB_URL=http://127.0.0.1:8765 SPACE_A_USER=bob SPACE_A_SPACE_ID=sw \
  agent-mentor hub-share
```

실제 출력:

```
cited(재사용): R8|github → 기존 page page_1 (reuse reuse_1)
hub-share: 0건 발행, 1건 인용(재사용), 0건 재개, 0건 보류/비대상
```

**보여줄 것**: **중복 발행이 아니라 인용**이다. 사람의 개입이 전혀 없었고, 조직의 지식이 재사용된
순간이 `ReuseEvent`로 남았다. 이게 이 제품의 북극성 지표다.

### ④ 조회 — "재사용이 실제로 기록됐다"

```bash
curl "http://127.0.0.1:8765/reuse-events" -H "Authorization: Bearer $TOKEN"
```

```json
{"reuse_events":[{"reuse_id":"reuse_1","page_id":"page_1","space_id":"sw",
  "cited_by":"bob","cross_team":true,"created_at":"2026-07-25T..."}]}
```

**보여줄 것**: `cross_team: true` — 팀 경계를 넘은 재사용임을 서버가 판정했다.

### ⑤ A-Lens — "사람은 관전한다"

```bash
cd a-lens/backend && .venv/bin/uvicorn alens.main:create_app --factory --port 8600
cd a-lens/frontend && npm run dev     # 5279
```

**보여줄 것**: 방(space)별 `reuse` 카운트와 활동 피드의 재사용 서사 —

> *"bob님의 에이전트가 다른 팀의 지식 『MCP 'github'가 최근 14일 동안 대형 결과를…』을 재사용했습니다"*

사람은 게시판을 뒤지지 않고, 에이전트들이 지식을 주고받는 흐름을 그냥 본다.

## 3. 마무리 한 문장

> 개인의 시행착오(①)가 조직의 기록(②)이 되고, 다른 팀이 그것을 재사용(③④)했으며,
> 사람은 그 과정을 관전(⑤)했다. **사람이 개입한 지점은 없다.**

## 4. 시연 중 주의

- **한 번 인용한 finding은 다시 발행/인용하지 않는다**(평생 1회). 같은 단계를 두 번 보여주려면
  a-mate DB(`agent-mentor.db`)를 새로 준비하거나 다른 `SPACE_A_USER`로 실행한다.
- **검색은 org 전체를 본다.** 자기 공간만 보면 교차 팀 재사용이 안 나온다 — 데모의 핵심이 사라진다.
- 허브가 죽어도 a-mate는 조용히 넘어간다(실패 무해). 반대로 말하면 **허브가 안 떠 있으면 ②③이
  아무 말 없이 no-op**이므로, 시작 전에 `/healthz`를 반드시 확인한다.
- a-lens는 LLM 번역 엔드포인트가 설정돼 있지 않으면 스냅숏이 **오래 걸릴 수 있다**(연결 대기).
  시연 전에 LLM 설정을 채우거나 미리 스냅숏을 한 번 돌려 캐시를 데워둔다.

## 5. 리허설 기록

2026-07-25, 깨끗한 허브(`/tmp/demo.db`)와 새 a-mate DB로 이 문서를 **그대로 따라** 실행했다.

| 단계 | 결과 |
|---|---|
| 준비 | `/healthz` ok, `ds`·`sw` 공간 201 |
| ② alice | `published: R8\|github → page page_1` |
| ③ bob | `cited(재사용): R8\|github → 기존 page page_1 (reuse reuse_1)` |
| ④ 조회 | `reuse_1` / `page_1` / `space sw` / `cited_by bob` / **`cross_team true`** |
| ⑤ a-lens | `totals.reuses` 실값 반영, 재사용 서사 문장 생성 확인 |

첫 리허설에서 **준비 단계가 실패**했다 — 문서에 적었던
`uvicorn …:create_app --factory`가 `mcp` 패키지를 요구해 기동되지 않았다.
그래서 위 §1의 명령을 `mount_mcp=False` 형태로 고쳤다. 리허설하지 않았다면 시연 당일에 겪었을 문제다.

## 6. 근거

이 대본의 ①~⑤는 [지식 재사용 루프 닫기 설계](../design/common/specs/2026-07-25-close-knowledge-reuse-loop-design.md)
§6.1의 E2E 실행 결과와 같은 경로다. 설계 이전에는 ②가 **구조적으로 0건**이었고 ③④는 존재하지 않았다.
