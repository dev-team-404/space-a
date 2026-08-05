# Space A Hub — 수집·인덱싱·RAG 파이프라인 설계

> 담당: a-hub(work). 이 문서는 **"a-hub로 들어오는 다양하고 많은 양의 raw 데이터를
> 어떻게 인덱싱하고 RAG를 적용해 소비자에게 내보낼 것인가"**의 설계 안이다.
>
> [07-search-design.md](07-search-design.md)가 `search_knowledge` **한 호출의 내부**를
> 다룬다면, 이 문서는 그 검색이 딛고 설 **데이터 파이프라인 전체**(수집→정규화→색인→검색→서빙)를
> 다룬다. 두 문서는 §5.5에서 만난다.
>
> 상태: **설계 초안 — 팀 리뷰 전.** 아키텍처 원칙([03](03-architecture.md))·계약([05](05-contracts.md))과
> 정합하도록 작성했으며, 되돌리기 어려운 결정(포트 추가·저장 레이아웃)은 확정 시 ADR로 옮긴다.
>
> ⚠️ **범위 주의 (2026-07-22).** 이 문서는 **에이전트 검색(C1 `search_knowledge`)을
> 의미검색·RAG로 끌어올리는 경로**다 — 무거운 공사(임베딩·벡터색인·워커)이며 **미래/선택**.
> "a-lens가 raw를 그대로 보여준다"는 **당면 문제는 이 문서가 아니라** a-lens 뷰 서버의
> 표시용 번역(분류·요약·서사)으로 해결한다 →
> [a-lens/05-narrative-summarization.md](../a-lens/05-narrative-summarization.md).
> 즉 **표시용 = a-lens(문서 05, 당면)**, **에이전트 검색용 = a-hub(이 문서, 미래)**로 갈린다.

---

## 1. 배경 — 지금은 raw가 그대로 흐른다

문제 제기의 출발점은 Pillar 3(a-lens)이 **a-hub의 raw 데이터를 거의 가공 없이 화면에 싣고
있다**는 관찰이다. 코드로 확인한 현재 흐름은 다음과 같다.

```
에이전트/사람 ──write──▶  a-hub/work                      a-lens/backend               화면
                          (CRUD 저장소)                    (collector 폴링)
   open_issue      ┌────────────────────┐   REST      ┌──────────────────────┐
   resolve_issue ─▶│ Page.body (자유텍스트)│ ──/pages──▶│ summary = body[:120]  │──▶ 원문 모달
   create_page     │ Issue.title          │ ──/tree───▶│ _humanize_activity     │──▶ 말풍선(규칙)
   cite_knowledge  │ substring search     │ ──/issues─▶│ (규칙 기반 문장 합성)   │──▶ 활동 피드
                   └────────────────────┘             └──────────────────────┘
```

| 지점 | 현재 구현 | 근거(코드) |
|---|---|---|
| 저장 | Page는 `title`+자유텍스트 `body`로 저장. 정형 필드 없음 | `core/models.py` `Page` |
| 검색 | `query`를 공백 분해해 `title+body` **부분문자열 매칭** | `core/services.py:154` `search_knowledge` |
| 인덱스 | **없음** — 매 호출이 `all_pages()` 전량 스캔 | `ports.py`에 벡터/색인 포트 부재 |
| 임베딩/LLM | **없음** — 어댑터는 memory·sqlite·dynamodb CRUD뿐 | `adapters/` |
| a-lens 요약 | `body`의 앞 120자를 잘라서 그대로 | `a-lens/collector.py:258` |
| a-lens 서사 | 제목+종류 기반 규칙 문장 (LLM 자리만 표시) | `a-lens/collector.py:128` `_humanize_activity` |

즉 **설계 문서(02·03·07)가 그린 "로컬 LLM 게이트키퍼 → 정형화 → 하이브리드 색인 →
에이전틱 검색 → 심야 압축"은 아직 코드가 없다.** a-lens가 raw를 보여주는 건 증상이고,
원인은 **a-hub에 수집·색인·RAG 계층이 비어 있다**는 것이다.

> **원칙 재확인 ([03 §1](03-architecture.md#1-컴포넌트-경계--가장-큰-함정-두-가지)):**
> 인덱싱·RAG는 **a-hub가 소유**한다. a-lens는 DB에 붙지 않고 **읽기 전용 REST(C2)만**
> 소비한다. 따라서 "raw를 가공하는 일"은 **a-lens가 아니라 a-hub에서** 해야 하며,
> a-lens는 가공된 결과를 받아 그리기만 한다. 이 문서가 다루는 backend = **a-hub/work.**

---

## 2. 설계 목표와 비(非)목표

### 2.1 목표

| # | 목표 | 성공 판정 |
|---|---|---|
| G1 | **다양성 흡수** — 이슈 로그·해결기록·저작 문서·가이드가 한 파이프라인으로 정형화된다 | 어떤 입력이든 Knowledge Record로 색인됨 |
| G2 | **대용량 대비** — 문서 수·본문 길이가 커져도 색인·검색이 선형 붕괴하지 않는다 | 색인은 증분·비동기, 검색은 인덱스 조회(전량 스캔 제거) |
| G3 | **RAG 품질** — 표현이 달라도 유사 사례를 찾고, 정제된 컨텍스트만 반환 | BM25+벡터 융합 + 재랭킹 + 조건 JSON |
| G4 | **두 소비자 분리** — 에이전트(C1)와 a-lens(C2)가 **같은 색인**에서 각자 필요한 뷰를 받는다 | a-lens가 raw 대신 processed(요약·클러스터·품질신호)를 받음 |
| G5 | **비용 0 유지** — 임베딩·정형화·압축은 사내 로컬 LLM(C3)이 전담 | 상용 API 호출 없음 |
| G6 | **구조 불변식 유지** — core는 포트에만 의존, 계약은 additive-only | import-linter 통과, C1/C2 파괴 변경 없음 |

### 2.2 비목표

- **정밀 접근제어**(역할·필드 단위 권한) — 현재 제품 범위 밖([a-hub/CLAUDE.md](../../../../a-hub/CLAUDE.md)). `org`/`space` 2단계만.
- **분산 벡터 클러스터·샤딩** — 조직 규모(수천~수만 문서)에서는 단일 노드로 충분. §4.3에서 규모 근거 제시.
- **실시간(sub-second) 재색인 보장** — 색인은 준실시간(수 초~수십 초 지연 허용, §7).
- **a-lens 프론트 렌더링 변경** — 이 문서는 **backend가 무엇을 내보내는가**까지만. 프론트는 C2 소비만 갱신.

---

## 3. 파이프라인 개요

수집부터 서빙까지를 **5단계**로 나눈다. 각 단계는 포트 뒤에 숨어 갈아끼울 수 있다.

```
                          ┌──────────────────────── a-hub/work (Pillar 2) ─────────────────────────┐
 write 경로(C1/REST)       │                                                                        │
 open/resolve/create ─────▶│  ① INTAKE        ② ENRICH            ③ INDEX          ④ RETRIEVE(RAG)   │
 cite/edit                 │  원본 보존        정형화·청킹·임베딩     BM25 + 벡터       융합→재랭킹→압축   │
                           │     │  raw          │ 로컬LLM/임베더      │ 색인 반영         │              │
                           │     ▼               ▼ (비동기 워커)       ▼                 ▼              │
                           │  [DocStore] ──────▶ [IngestQueue] ────▶ [Index/VectorStore] ─▶ [Retriever]│
                           │     ▲ 원천(정본)                            ▲                     │        │
                           │     │                                      │                     ▼        │
                           │     └──── ⑤ CONDENSE (심야 배치) ──── 중복 병합·Skill 승격    ⑥ SERVE       │
                           └───────────────────────────────────────────────────────┬───────┬─────────┘
                                                                                    │C1     │C2
                                                                            에이전트 검색   a-lens(processed)
```

| 단계 | 하는 일 | 담당 | 동기/비동기 |
|---|---|---|---|
| ① Intake | write를 **원본 그대로 정본 저장**(감사·재색인 대비). 지금의 CRUD가 여기 해당 | Store | 동기 |
| ② Enrich | raw → **Knowledge Record**로 정형화(로컬 LLM), 청킹, 임베딩 | LLMClient·Embedder | **비동기 워커** |
| ③ Index | BM25 역색인 + 벡터 색인에 반영 | IndexStore·VectorStore | 비동기 워커 |
| ④ Retrieve | 질의 → 융합 검색 → 재랭킹 → (에이전틱) 조건 JSON | Retriever | 동기(호출당) |
| ⑤ Condense | 유사 레코드 병합·Skill 승격·품질 재계산 | 배치 러너 | **심야 배치** |
| ⑥ Serve | C1(에이전트)·C2(a-lens)로 각자 뷰 노출 | api/ | 동기 |

**핵심 결정: write는 동기, 색인은 비동기.** 에이전트의 `resolve_issue`/`create_page`는
원본만 저장하고 즉시 반환한다(현재 지연 유지). 정형화·임베딩·색인은 백그라운드
워커가 처리한다 — 무거운 LLM/임베딩 작업이 write 지연을 오염시키지 않고, 대용량
유입에도 write 경로가 막히지 않는다(G2). 색인 전 문서는 검색에 "아직 미색인"으로만
빠질 뿐 유실되지 않는다(정본은 ①에 있음).

---

## 4. 데이터 특성과 규모 전략

### 4.1 들어오는 데이터의 다양성

| 원천 | 형태 | 특성 | 정형화 난이도 |
|---|---|---|---|
| `resolve_issue` 산물 | issue-derived Page(`title`+`steps[]`) | 문제→해결 구조가 이미 있음 | 낮음 |
| `create_page` 저작 | authored Page(자유 `body`) | 형식 자유 — 스펙/노트/릴리즈diff 등 잡다 | **높음** |
| 가이드 | space 생성 시 seed된 Page | 방 규칙 | 낮음 |
| 이슈 원문 | `Issue.title` + (미래) 첨부 로그 | 에러 메시지·스택트레이스 혼재 | 중간 |

더미 데이터가 이 다양성을 잘 보여준다 — "PMU ACE 인터페이스 타이밍 노트", "MIPI CSI-2
릴리즈 노트 diff", "JTAG 디버그 브리지 합성 QoR" 등 **도메인 용어·에러코드·버전 문자열이
뒤섞인** 반정형 텍스트다. 부분문자열 매칭으로는 "표현이 다른 같은 문제"를 절대 못 찾는다.

### 4.2 정형 목표 — Knowledge Record 스키마 (내부)

[02 §1.1](02-features.md#11-knowledge-contract--에이전트-친화적-스키마)의 Knowledge Contract를
**내부 저장 스키마로 구체화**한다. 이것은 C1/C2 계약과 별개인 **a-hub 내부 색인 레코드**다
(계약은 안 건드림). `Page`에 부가되는 파생 산물로 본다.

```jsonc
// KnowledgeRecord — Page 1건에서 ②Enrich가 파생 (원본 Page.body는 정본으로 보존)
{
  "record_id": "kr_page_42",     // page_id에 1:1
  "page_id": "page_42",
  "space_id": "soc-design",
  "visibility": "org",           // Page에서 승계 — 권한 필터의 원천
  "kind": "issue-derived",       // authored | issue-derived | guide
  "structured": {
    "issue_context": "…",        // 어떤 상황/에러 (없으면 LLM이 body에서 추출)
    "tried_steps":   ["…"],      // 실패한 시도 (있으면)
    "final_solution":"…",        // 통한 해결
    "required_tools":["…"]       // 쓰인 MCP/명령
  },
  "human_summary": "…",          // 사람이 읽을 2~3문장 (a-lens용, body[:120] 대체)
  "keywords": ["ACE","PMU",…],   // 에러코드·도구명·버전 — BM25 가중 대상
  "topic_id": "t_power_mgmt",    // ⑤Condense가 매기는 클러스터 (a-lens 그룹핑용)
  "quality": { "trust": 0.0, "flags": 0, "reuse_count": 0 },
  "chunks": [ { "chunk_id":"…", "text":"…", "embedding_ref":"…" } ],
  "indexed_at": "2026-07-21T…Z",
  "source_hash": "sha256(body)"  // 재색인 판정 — body 안 바뀌면 재처리 스킵(멱등)
}
```

> **왜 Page와 분리하나:** `Page`(정본)는 계약·CRUD가 물고 있어 함부로 못 바꾼다.
> KnowledgeRecord는 **언제든 통째로 재생성 가능한 파생 캐시**다. 색인 스키마가 바뀌면
> 정본에서 다시 만들면 된다(재색인). 정본과 파생을 분리하는 것이 대용량 진화의 전제다.

### 4.3 규모 판단 — 왜 단일 노드로 충분한가

| 축 | 예상 규모(조직 실사용 1년) | 판단 |
|---|---|---|
| 문서 수 | ~10⁴–10⁵ Page | 단일 노드 벡터스토어(HNSW)로 ms급 검색 가능 |
| 본문 길이 | 평균 수백~수천 토큰 | 청킹으로 임베딩 입력 상한 내 유지 |
| 임베딩 차원 | BGE-m3 1024d | 10⁵ × 1024 × 4B ≈ 400MB — 메모리 상주 가능 |
| 색인 처리량 | write QPS 낮음(에이전트 이벤트) | 워커 1~2개로 실시간 소화, 버스트는 큐로 흡수 |

**결론:** 샤딩·분산 벡터DB는 **비목표**(§2.2). ChromaDB/FAISS 단일 노드로 시작하고,
포트로 감싸 두어 필요 시 교체한다. 대용량 대응의 본질은 하드웨어가 아니라
**① 비동기 색인, ② 증분/멱등 재처리, ③ 심야 압축으로 총량 억제**다.

---

## 5. 단계별 상세

### 5.1 ① Intake — 원본은 정본으로 보존

- write 시 `Page`/`Issue`를 **지금처럼 그대로 저장**(변경 없음). 이것이 **정본(source of truth)**.
- write 직후 `record_id`를 IngestQueue에 enqueue만 한다(§7). LLM/임베딩은 여기서 하지 않는다.
- 정본을 절대 파괴하지 않는 이유: 색인 스키마·임베딩 모델이 바뀌면 **정본에서 전량 재색인**해야 하기 때문. 압축(⑤)조차 원본을 지우지 않고 `status`로 아카이브만 한다([06](06-governance.md)의 "지식은 안 지움" 불변식).

### 5.2 ② Enrich — 정형화·청킹·임베딩 (로컬 LLM)

세 하위 단계를 워커가 순차 수행한다.

**(a) 정형화 (게이트키퍼).** raw `body`/`title`을 로컬 LLM(`space-a-ingest` 모델,
[03 §5.2](03-architecture.md#52-클러스터-구성--litellm--ollama))에 넣어 `structured` 필드와
`human_summary`·`keywords`를 추출한다.

- issue-derived는 이미 구조가 있어 매핑에 가깝다. authored 자유문서만 추출 부담이 크다.
- **LLM 실패·타임아웃 시 폴백**: 규칙 기반으로 `human_summary = 첫 문장들`, `keywords = 제목 토큰`. 정형화 실패가 색인 자체를 막지 않는다(품질만 낮은 레코드로 색인).

**(b) 청킹.** 임베딩 입력 상한·검색 granularity를 위해 본문을 청크로 나눈다.

| 규칙 | 값(초안) | 이유 |
|---|---|---|
| 청크 크기 | ~512 토큰 | BGE-m3 입력 효율 구간 |
| 오버랩 | ~64 토큰 | 경계에서 잘린 맥락 보전 |
| 분할 우선순위 | 문단/헤더 경계 > 문장 > 토큰 | 의미 단위 유지 |
| 짧은 문서 | 1청크 | issue-derived 대부분 |

**(c) 임베딩.** 각 청크를 임베더(BGE-m3 등, C3 OpenAI 호환 `/v1/embeddings`)로 벡터화.
`human_summary`도 1벡터 임베딩(문서 대표 벡터). **멱등:** `source_hash`가 이전과 같으면
(b)(c) 스킵.

### 5.3 ③ Index — BM25 + 벡터 이중 색인

[07 §2](07-search-design.md#2-인덱싱)의 이중 인덱스를 실체화한다.

| 인덱스 | 대상 | 강점 | 어댑터 후보 |
|---|---|---|---|
| BM25(키워드) | `keywords` 가중 + 전 필드 | 에러코드·도구명·버전 **정확 매칭** | SQLite FTS5 / Tantivy / OpenSearch |
| 벡터(의미) | 청크 임베딩 + 문서 대표 벡터 | **표현이 다른** 유사 사례 | ChromaDB / FAISS / pgvector |

- 색인 반영은 **증분**(레코드 단위 upsert). 삭제/아카이브는 색인에서 tombstone.
- 권한은 색인에도 실어둔다(`visibility`, `space_id`) — 검색 시 **후처리가 아니라 필터 단계**에서 배제(대용량에서 후처리 배제는 낭비).

### 5.4 저장 레이아웃 — 포트 추가 (구조 불변식 유지)

core는 여전히 **포트에만** 의존한다([03 §4](03-architecture.md#4-내부-구조--포트와-어댑터)).
기존 `Store`에 파생 계층용 포트를 **추가**한다(기존 포트 시그니처는 안 건드림 → import-linter·CI 그대로 통과).

```python
# core/ports.py 에 추가 (초안) — core는 구현을 모른다
class Embedder(ABC):
    @abstractmethod
    def embed(self, texts: list[str]) -> list[list[float]]: ...

class LLMClient(ABC):
    @abstractmethod
    def structure(self, raw: str, kind: str) -> dict: ...   # → structured/summary/keywords
    @abstractmethod
    def rerank(self, query: str, candidates: list[str]) -> list[float]: ...

class IndexStore(ABC):           # BM25 + 벡터를 함께 가진 색인 파사드
    @abstractmethod
    def upsert(self, record: "KnowledgeRecord") -> None: ...
    @abstractmethod
    def remove(self, record_id: str) -> None: ...
    @abstractmethod
    def search_keyword(self, query, spaces, visibility, k) -> list["Hit"]: ...
    @abstractmethod
    def search_vector(self, vector, spaces, visibility, k) -> list["Hit"]: ...

class IngestQueue(ABC):
    @abstractmethod
    def enqueue(self, record_id: str) -> None: ...
    @abstractmethod
    def lease(self, n: int) -> list["Job"]: ...   # 워커가 배치로 가져감
```

**어댑터 매트릭스 (개발 무중단 원칙 [03 §6](03-architecture.md#6-다른-팀원을-안-기다리는-법)):**

| 포트 | 초기(비용 0·의존 0) | 실전 |
|---|---|---|
| `Embedder` | `EmbedderFake`(해시 기반 결정벡터) | `EmbedderLiteLLM`(BGE-m3) |
| `LLMClient` | `LLMFake`(고정 규칙 추출) | `LLMLiteLLM`(`space-a-ingest`) |
| `IndexStore` | `IndexMemory`(numpy 코사인 + 파이썬 BM25) | `IndexChroma`+`FTS5` |
| `IngestQueue` | `QueueMemory`(인프로세스 스레드) | `QueueSqlite`/SQS(서버리스) |

> 이 매트릭스 덕에 **클러스터·벡터DB가 없어도** 인메모리 fake로 파이프라인 전체를
> E2E 테스트할 수 있다(현행 테스트 철학 유지). 교체 비용 0.

### 5.5 ④ Retrieve — RAG 검색 (07 문서와 접합)

`search_knowledge` 한 호출의 내부는 [07-search-design.md](07-search-design.md)가 정본이다.
이 문서는 그 검색이 **이 색인 위에서** 도는 형태로 접합만 한다.

```
search_knowledge(query, space_id?, limit)
   │
   ▼
[로컬 LLM 검색 에이전트]  ← 최대 3회 루프 (07 §4)
   │  ① 질의 생성/재구성 (에러코드 추출·동의어 확장)
   │  ② 융합 검색
   │      ├─ IndexStore.search_keyword (BM25)     ┐ 권한(spaces/visibility)
   │      ├─ IndexStore.search_vector  (임베딩)    ┘ 필터를 색인 단계에서 적용
   │      ├─ RRF 융합 (07 §3)
   │      └─ 재랭킹: LLMClient.rerank + trust + 최신성 + 현재 방 부스트(08-life-presence)
   │  ③ 종료 판정 (충분하면 stop, 부족하면 ①)
   ▼
정제된 "해결 조건 JSON" 반환 (원본 로그 미포함) — 상용 모델 토큰 절감
```

**현행 `search_knowledge`(부분문자열)와의 관계:** 시그니처(C1)는 그대로. 내부만
`store.all_pages()` 전량 스캔 → `IndexStore` 조회로 교체한다. `SearchResult.scanned`
카운터는 "권한 범위 내 후보 수"로 의미를 유지한다(계약 additive).

### 5.6 ⑤ Condense — 압축·중복 제거·Skill 승격 (심야 배치)

[02 §3](02-features.md#3-심야-지식-압축-루프-knowledge-condensation)의 심야 압축을 색인 위에서 구현.

| 작업 | 방법 | 산출 |
|---|---|---|
| **의미 중복 탐지** | 벡터 근접(코사인 ≥ τ) 클러스터링 | 같은 문제의 N개 기록 그룹 |
| **병합(Best Practice)** | 클러스터를 로컬 LLM이 1개 표준 문서로 요약 | 새 canonical Page + 원본 `supersede` |
| **토픽 클러스터링** | 임베딩 클러스터에 `topic_id` 부여 | a-lens 그룹핑·"오늘의 하이라이트" 재료 |
| **Skill 승격 감지** | issue-derived 반복 집계(현행 `get_skill_candidates` 확장) | SkillCandidate |
| **품질 재계산** | cite→resolve 성패로 `trust` 갱신([02 §6.1](02-features.md#61-환각-전염-차단--사용-기반-신뢰도)) | trust↑/↓, 임계 이하 검색 격리 |

- **원본 불파괴:** 병합해도 원본은 `superseded`/`archived`로 남긴다(정본 보존, §5.1).
- **유휴 GPU:** 임직원 퇴근 후 배치 실행 → 비용 0(G5).

### 5.7 품질·신뢰 신호

색인 레코드의 `quality`는 **검색 랭킹과 a-lens 표시 양쪽**에 쓰인다. 신호 원천은
이미 C1이 준다(cite/resolve/flag) — 추가 계약 불필요([02 §6.1](02-features.md#61-환각-전염-차단--사용-기반-신뢰도)).

---

## 6. 서빙 ⑥ — 한 색인, 두 소비자

핵심 설계는 **에이전트(C1)와 a-lens(C2)가 같은 KnowledgeRecord 색인을 공유하되 다른 뷰를
받는다**는 것. 이것이 "a-lens가 raw 대신 processed를 받는다"의 실체다(G4).

| | C1 (에이전트) | C2 (a-lens 시각화) |
|---|---|---|
| 목적 | 문제 해결 — 최소 토큰 | 관전·이해 — 사람이 읽을 서사 |
| 반환 | `structured`(해결 조건 JSON) | `human_summary`, `topic_id`(그룹), `quality`(품질 배지), 클러스터 하이라이트 |
| 대체 대상 | 전량 스캔 substring | a-lens의 `body[:120]`·규칙 humanize |

### 6.1 a-lens가 받게 될 processed 필드 (C2 additive)

현재 a-lens가 raw로 채우는 자리를 **a-hub가 만든 파생 필드로 교체**한다. C2는
**필드 추가만**([03 §8](03-architecture.md#8-이-구성의-잔여-리스크)) — 기존 소비는 안 깨진다.

| a-lens 현행(raw) | a-hub가 대신 제공할 processed |
|---|---|
| `summary = body[:120]` (collector.py:258) | `human_summary` (LLM 2~3문장) |
| `_humanize_activity` 규칙 문장 (collector.py:128) | 레코드 기반 `narrative`(선택) |
| 그룹핑 없음 | `topic_id` → 방/로비에서 주제 클러스터 시각화 |
| 품질 표시 없음 | `quality.trust`/`reuse_count` → 신뢰 배지 |
| 하이라이트 = id 최신순 | 클러스터·재사용 기반 하이라이트(파이프라인 [pipeline.py](../../../../a-lens/backend/alens/pipeline.py) `_pick_highlight`가 이미 소비 가능한 형태) |

> collector.py의 `_humanize_activity`에는 이미 *"추후 이 함수 안에서 LLM으로 요약"* 이라는
> 교체 지점 주석이 있다. **그 LLM 처리를 a-lens가 아니라 a-hub로 옮기는 것**이 이 설계의 골자다
> (a-lens는 LLM·DB를 소유하지 않는다는 원칙 §1).

### 6.2 C2 엔드포인트 영향

[02 §7.2](02-features.md#72-읽기-전용-rest--pillar-3-시각화)의 `/stats`·`/activity`·`/graph`에
파생 필드를 실어 내보낸다. 새 엔드포인트가 꼭 필요하면(`/topics` 등) 계약 v넥스트로 추가
논의 — 기존 응답 확장으로 대부분 커버 가능.

---

## 7. 비동기 처리 아키텍처

대용량·다양성(G2) 대응의 심장. write와 색인을 분리한다.

```
write(동기) ──▶ [DocStore 정본] ──enqueue(record_id)──▶ [IngestQueue]
                                                            │ lease(batch)
                                                            ▼
                                              [Enrich Worker] × 1~N
                                              (정형화→청킹→임베딩→색인 upsert)
                                                            │ 실패 시
                                                            ▼
                                              재시도(backoff) → dead-letter
```

| 관심사 | 결정 |
|---|---|
| **멱등성** | `source_hash` 동일하면 재처리 스킵. 워커 중복 실행 안전 |
| **순서** | 레코드 단위 독립 — 순서 보장 불필요. 같은 record_id는 최신 hash가 이김 |
| **백프레셔** | 버스트는 큐가 흡수. 워커 수로 처리율 조절. write는 절대 안 막힘 |
| **실패 격리** | LLM/임베딩 실패는 폴백 색인(§5.2a) 또는 재시도. 3회 초과 시 dead-letter + 로깅 |
| **재색인** | 임베딩 모델/스키마 변경 시 정본 전량 재enqueue(백필). 파생만 재생성 |
| **가시성** | 미색인 문서는 검색에서만 누락(정본은 조회 가능). a-lens에 "색인 대기" 상태 노출 가능 |
| **배포 형태** | 서버(컨테이너)=인프로세스 워커 스레드. 서버리스(Lambda)=SQS+워커 Lambda([work/SERVERLESS.md](../../../../a-hub/work/SERVERLESS.md) 경로) |

---

## 8. 아키텍처 정합성 체크

| 원칙 | 이 설계의 준수 |
|---|---|
| core는 adapters/api를 import 안 함 | 신규 로직도 포트(§5.4)에만 의존. import-linter 규칙 불변 |
| 계약 additive-only | C1 시그니처 불변, C2 필드 추가만. KnowledgeRecord는 내부 스키마(계약 아님) |
| LLM 클러스터는 소비자일 뿐 | Embedder/LLMClient 모두 C3(OpenAI 호환)만 바라봄. 소유 아님 |
| 비용 0 | 정형화·임베딩·재랭킹·압축 전부 로컬 LLM. 상용 API 미사용 |
| DB 결정 유예 | IndexStore/VectorStore 포트 뒤. 인메모리 fake로 개발·테스트 |
| 지식 불파괴 | 정본 보존, 압축은 아카이브만([06](06-governance.md)) |
| a-lens는 DB 비접속 | processed도 C2 REST로만 전달 |

---

## 9. 단계적 구현 로드맵

현재(raw CRUD+substring)에서 목표(색인+RAG)로 **점진 이행**. 각 단계는 독립 배포 가능하고,
중간에 잘라도 앞 단계는 살아 있다([04 §2](04-roadmap.md#2-구현-순서-의존성-기준)의 컷라인 철학).

| 단계 | 내용 | 산출/데모 | 컷 가능 |
|---|---|---|---|
| **P0** | 포트 추가(§5.4) + 인메모리 fake 어댑터 + IngestQueue(인프로세스) | 파이프라인 E2E 테스트 통과, 코드 무구조위반 | — (기반) |
| **P1** | Enrich 워커: 정형화(규칙 폴백)+청킹+**fake 임베딩** → IndexMemory | `search_knowledge`가 전량스캔→색인조회로 교체, 결과 동일 이상 | ✅ 여기까지도 개선 |
| **P2** | `human_summary`/`keywords` 산출 → **C2에 파생 필드 노출** | **a-lens가 body[:120] 대신 요약 표시** (문제의 직접 해결) | ✅ 데모 포인트 |
| **P3** | 실 임베더(BGE-m3)+ChromaDB/FTS5 교체, RRF 융합 | 의미 검색 적중(표현 다른 사례) | ✅ |
| **P4** | 에이전틱 루프([07 §4](07-search-design.md#4-에이전틱-루프)) + 재랭킹(trust·방 부스트) | 재시도로 적중, 조건 JSON | |
| **P5** | 심야 압축·중복 병합·`topic_id` 클러스터링 | a-lens 주제 그룹핑, 지식 총량 억제 | |

> **P2가 사용자 요청("raw를 보여준다")의 최소 해결선.** P0–P2만으로도 a-lens는 raw 대신
> 정형 요약을 받는다. P3 이후는 검색 품질·규모 대응 심화.

---

## 10. 미결정 사항 / 리스크

- [ ] **벡터DB 선택** — ChromaDB vs FAISS vs pgvector(SQLite 스토어와 정합성). [04 §4.2](04-roadmap.md#42-이후--어댑터-뒤에-숨어-있어-미뤄도-되는-것)와 함께 결정.
- [ ] **BM25 구현** — SQLite FTS5(스토어 재사용) vs Tantivy vs OpenSearch. 규모상 FTS5로 시작 유력.
- [ ] **임베딩 모델·차원** — BGE-m3(1024d) 가정. 한국어+코드 혼합 성능 실측 필요.
- [ ] **청킹 파라미터** — 512/64는 초안. 실데이터로 튜닝.
- [ ] **워커 배포** — 서버(스레드) vs 서버리스(SQS+Lambda) 이중 지원 범위.
- [ ] **재색인 트리거·비용** — 임베딩 모델 교체 시 전량 백필 시간(유휴 GPU 배치로 흡수).
- [ ] **C2 파생 필드 계약 반영** — a-lens 담당과 필드명·형태 합의(계약 additive 확인).

| 리스크 | 대응 |
|---|---|
| 정형화 LLM이 자유문서에서 헛추출(환각) | 폴백(규칙)로 색인은 보장 + trust/flag로 사후 격리([02 §6.1](02-features.md#61-환각-전염-차단--사용-기반-신뢰도)) |
| 파생과 정본 불일치 | `source_hash` 멱등 + 재색인 가능(정본이 진실) |
| 인메모리→실DB 성능 급락 | P3에서 실데이터 규모 조기 검증([03 §8](03-architecture.md#8-이-구성의-잔여-리스크)) |
| 색인 지연으로 "방금 쓴 글이 검색 안 됨" | 준실시간 허용(§2.2) + a-lens에 "색인 대기" 상태 노출 |
| 대용량 임베딩 처리량 병목 | 배치 임베딩 + 워커 수 조절 + 심야 압축으로 총량 억제 |

---

## 11. 관련 문서

- [02-features.md](02-features.md) — 지식 기록·하이브리드 검색·심야 압축(개념)
- [03-architecture.md](03-architecture.md) — 포트/어댑터, 컴포넌트 경계, C3 클러스터
- [05-contracts.md](05-contracts.md) — C1(MCP)·C2(REST) 확정 계약
- [06-governance.md](06-governance.md) — 지식 불파괴·사용 기반 신뢰도
- [07-search-design.md](07-search-design.md) — `search_knowledge` 내부(이 문서의 ④와 접합)
- [08-life-presence.md](08-life-presence.md) — 현재 방 검색 부스트
- 코드: [a-hub/work/ahub/core/](../../../../a-hub/work/ahub/core/) · a-lens 소비: [collector.py](../../../../a-lens/backend/alens/collector.py) · [pipeline.py](../../../../a-lens/backend/alens/pipeline.py)
