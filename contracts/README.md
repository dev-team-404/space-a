# Contracts

SPACE-A 컴포넌트 간 **경계 계약**. 세 팀원이 서로를 기다리지 않고 병렬로 작업하기 위한 전제다.

> ⚠️ **여기 있는 파일이 정답이다.** 설계 문서와 어긋나면 이 파일을 따른다.

| 파일 | 계약 | 제공자 → 소비자 |
|---|---|---|
| [c1-mcp-tools.json](c1-mcp-tools.json) | **C1** — MCP Tool 시그니처 | Space A Hub(Pillar 2) → 임직원 에이전트 / Pillar 1 |
| [c2-rest-api.json](c2-rest-api.json) | **C2** — 읽기 전용 REST | Space A Hub(Pillar 2) → 시각화 웹(Pillar 3) |
| [fixtures/](fixtures/) | 응답 예시 | **서버 없이 먼저 작업 시작하라고 주는 것** |

설계 배경과 각 필드의 의도는 [`docs/design/collab-space/05-contracts.md`](../docs/design/collab-space/05-contracts.md) 참고.

## Pillar 3(시각화) 담당자에게

**서버를 기다리지 마세요.** `fixtures/`의 JSON이 실제 응답과 같은 모양입니다.

```
fixtures/graph.json      → GET /graph     (네트워크 맵)
fixtures/stats.json      → GET /stats     (대시보드)
fixtures/activity.json   → GET /activity  (관전 피드)
```

이걸로 먼저 붙여두면, 나중에 엔드포인트 주소만 바꾸면 됩니다.

**세 가지만 지켜주세요.**

1. **Graph DB에 직접 붙지 마세요.** 이 세 엔드포인트가 전부입니다.
   (직접 붙는 순간 제 그래프 스키마가 프론트의 공개 API가 되어 못 바꾸게 됩니다.)
2. **`summary` 문장을 프론트에서 조립하지 마세요.** 서버가 완성된 한국어 문장으로 줍니다.
   조립하면 이벤트 타입이 늘 때마다 프론트를 고쳐야 합니다.
3. **`tokens_saved_est`는 추정치입니다.** `~412,000` 처럼 물결표를 붙여 정직하게 표기해주세요.

**`REUSED` 엣지를 강조해서 그려주세요.** "A가 푼 걸 B가 재사용했다" = 지식이 팀 경계를 넘었다는 증거이고,
이 프로젝트의 핵심 서사입니다.

## Pillar 1(코칭 Agent) 담당자에게

`c1-mcp-tools.json`의 3개 Tool을 호출하시면 됩니다.

- `search_knowledge` — 막혔을 때 **추론하기 전에 먼저** 호출
- `report_issue` — **해결한 뒤 1회** 호출 (발생 시점에 여는 단계는 없습니다)
- `get_skill_candidates` — Skill 승격 후보 조회. Pillar 1의 "Skill화 후보 추천"과 신호를 공유합니다

`fixtures/search_knowledge.json`에 요청·응답 왕복 예시가 있습니다.

## 변경 규칙 (중요)

C1이 깨지면 Pillar 1이 죽고, C2가 깨지면 Pillar 3이 죽습니다.

| 허용 | 금지 |
|---|---|
| ✅ 선택 필드 추가 | ❌ 필드 제거 |
| ✅ enum 값 추가 | ❌ 필드 이름·타입 변경 |
| ✅ 응답에 필드 추가 | ❌ 기존 필드의 **의미** 변경 |

- **소비자는 모르는 필드를 만나면 무시하세요.** 하드 실패 금지.
- 정말 깨야 하면 버전을 올립니다 (`search_knowledge_v2`). 조용히 바꾸지 않습니다.
