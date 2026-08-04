# A-Hub — 설계 기록

이 디렉터리는 A-Hub를 구현하기 전에 작성한 설계안과 아직 구현되지 않은 선택지를 보존한다.
**현재 동작을 설명하는 문서가 아니다.** 현재 제품·기능·코드 구조·실행 방법은
[`docs/architecture/a-hub/`](../../../architecture/a-hub/)를 기준으로 한다.

> 이곳의 문서는 합의 당시의 시점 기록이므로 현재 코드와 다르더라도 현행 설명으로 고쳐 쓰지 않는다.
> 과거 설계를 실제로 구현하면 같은 PR에서 `docs/architecture/a-hub/`를 갱신한다.

## 문서 구성

| 문서 | 성격 | 현재 구현과의 관계 |
|---|---|---|
| [01-product.md](01-product.md) | 초기 제품·포지셔닝 제안 | Work 중심 초기 구상. Life와 현재 범위는 현행 문서 참고 |
| [02-features.md](02-features.md) | 초기 기능 카탈로그 | 하이브리드 검색·압축·P2P Ask 등 다수 미구현 |
| [03-architecture.md](03-architecture.md) | 초기 구조 제안 | 포트/어댑터는 채택, GPU·LLM 구성 등은 미구현 |
| [04-roadmap.md](04-roadmap.md) | 2026-07-12 기준 계획·미결정 사항 | 시점 기록, 현행 로드맵 아님 |
| [05-contracts.md](05-contracts.md) | C1/C2 계약의 설계 배경 | 기계 판독 계약은 [`contracts/`](../../../../contracts/)가 정본 |
| [06-governance.md](06-governance.md) | 자기 진화 공간 제안 | 미구현 설계 |
| [07-api-surface.md](07-api-surface.md) | 시나리오별 API 설계 | 현재 API 정본은 OpenAPI와 현행 Work 문서 |
| [07-search-design.md](07-search-design.md) | BM25·벡터·LLM 검색 제안 | 미구현 설계 |
| [08-life-presence.md](08-life-presence.md) | 초기 Presence 제안 | 현재 Life 프레즌스는 현행 Life 문서 참고 |
| [10-ingestion-indexing-rag.md](10-ingestion-indexing-rag.md) | 수집·색인·RAG 초안 | 미구현 선택 경로 |

## 현행 문서로 이관된 항목

- 에이전트 작성 지침 → [`writing-guide.md`](../../../architecture/a-hub/writing-guide.md)
- AGENTS.md 템플릿 → [`agents-md-template.md`](../../../architecture/a-hub/agents-md-template.md)
- 현재 제품·Work·Life·통합·실행 문서 → [`docs/architecture/a-hub/`](../../../architecture/a-hub/)

## 읽을 때 주의할 점

- “현재 전부 설계 단계”, “코드는 아직 없다” 같은 표현은 작성 당시에는 맞았지만 지금은 사실이 아니다.
- BM25, 벡터 DB, 로컬 LLM, 심야 압축, P2P Ask, 자기 진화는 현재 구현됐다고 가정하지 않는다.
- 현재 API 경로, 필드와 권한은 실행 중인 OpenAPI와 현행 문서를 우선한다.
- 과거 문서의 확정 결정이 현재도 유효한지는 관련 ADR과 코드를 함께 확인한다.
