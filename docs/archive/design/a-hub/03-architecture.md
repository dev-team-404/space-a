# Space A Hub — 아키텍처

> **핵심 질문:** "Pillar 2를 하나의 독립 컴포넌트로 떼어내서, 다른 팀원 작업과 얽히지 않고 구현할 수 있는가?"
>
> **답: 가능하다.** 단, §1의 함정 두 가지를 설계 초반에 못 박지 않으면 **반드시 남의 작업과 충돌한다.**

## 1. 컴포넌트 경계 — 가장 큰 함정 두 가지

| # | 함정 | 왜 위험한가 | 해결 |
|---|---|---|---|
| **①** | **로컬 LLM 클러스터를 Pillar 2 안에 넣는 것** | Pillar 1(코칭)도 로그 파싱에 로컬 LLM이 필요하다. 클러스터가 내 소유가 되면 Pillar 1이 나를 거쳐야 하고, **내 배포가 남의 기능을 멈춘다.** | 클러스터를 **Pillar 2 밖의 공용 인프라**로 분리. 내 컴포넌트는 OpenAI 호환 엔드포인트 하나를 소비하는 **클라이언트**일 뿐 (§5) |
| **②** | **Pillar 3(시각화)이 DB를 직접 조회** | 내 저장 스키마가 곧 **Pillar 3의 공개 API**가 된다. 스키마 하나 바꾸려면 매번 프론트 담당자 허락이 필요하고, 스키마가 굳어 진화가 멈춘다. | Pillar 3은 **내가 제공하는 읽기 전용 REST API만** 사용. **DB 커넥션 스트링은 절대 공유하지 않는다** (C2) |

**핵심 원칙:** 내가 소유하는 것은 **지식 저장소와 그 위의 로직**뿐이다.
GPU 클러스터는 **빌려 쓰는 것**이고, 프론트엔드는 **내 API의 소비자**다.

## 2. 전체 구조

```
┌─ Pillar 1: 코칭 Agent (로컬 PC) ─┐   ┌─ Pillar 3: 시각화 웹 ─┐
└───────────────┬──────────────────┘   └───────────┬───────────┘
                │ C1: MCP (Streamable HTTP)        │ C2: REST (read-only)
                ▼                                  ▼
┌───────────────────────────────────────────────────────────────┐
│              ★ Pillar 2 = space-a-hub (내 담당)                │
│                                                               │
│   [MCP Adapter]        [REST Adapter]       [Batch Runner]    │
│         └─────────────┬──────┴───────────────────┘            │
│                       ▼                                       │
│               [Core: 지식 도메인 로직]                          │
│          ingest / search / condense / permission              │
│                       │                                       │
│         ┌─────────────┴─────────────┐                         │
│         ▼                           ▼                         │
│   [VectorStore]               [LLMClient]   ← 전부 포트(인터페이스) │
└─────────┬───────────────────────────┬─────────────────────────┘
          ▼                           ▼ C3: OpenAI 호환
      ChromaDB?                LiteLLM 게이트웨이
                                  ★공용 인프라 — 내 소유 아님
                                        │
                              ┌─────────┼─────────┐
                            PC1(5070)  PC2       PC3   ← Ollama
```

### 소유권 표

| 항목 | 내가 소유 | 소유하지 않음 |
|---|---|---|
| 지식 스키마 (Issue/Solution/Skill) | ✅ | |
| Vector DB · 이벤트/지식 저장소 | ✅ | |
| 검색·압축·권한 로직 | ✅ | |
| MCP Tool 정의 | ✅ | |
| 시각화용 읽기 API | ✅ (제공자) | |
| LiteLLM 게이트웨이 · Ollama 노드 | | ❌ **공용 인프라** |
| 로컬 코칭 Agent | | ❌ Pillar 1 |
| 웹 프론트엔드 | | ❌ Pillar 3 |

## 3. 외부와의 계약 (Contract)

컴포넌트가 외부에 노출하는 면은 **이 3개뿐**이다.
**이것만 먼저 고정하면 내부는 마음대로 바꿔도 된다.**

| # | 계약 | 대상 | 형태 | 고정 시점 |
|---|---|---|---|---|
| **C1** | MCP Tool 시그니처 | Pillar 1 / 임직원 에이전트 | `search_knowledge`, `open_issue`, `cite_knowledge`, `resolve_issue`, `get_skill_candidates` | ✅ 확정 (v2) |
| **C2** | 읽기 전용 REST | Pillar 3 시각화 | `/spaces`, `/spaces/{id}`, `/reuse-events`, `/graph`, `/stats`, `/activity` | ✅ 확정 (v2) |
| **C3** | LLM 엔드포인트 | ← 공용 클러스터 | OpenAI 호환 (`/v1/chat/completions`, `/v1/embeddings`) | 초반 |

> **C3이 OpenAI 호환이라는 점이 결정적이다.** 클러스터가 아직 없어도 개발이 멈추지 않는다 (§6).

## 4. 내부 구조 — 포트와 어댑터

Vector DB는 **아직 미확정**이다. 확정될 때까지 개발을 못 하면 안 된다.
→ Core는 **구현체가 아니라 인터페이스(포트)에만 의존**한다.

```
core/          ← 도메인 로직. 외부 라이브러리 import 금지. 인터페이스만 안다.
  ports.py       VectorStore / LLMClient  (추상)
  models.py      Issue, Solution, Skill, ...
  services.py    ingest() / search() / condense()

adapters/      ← 실제 구현. 갈아끼우는 부품.
  vector_chroma.py     ┐
  vector_memory.py     ┘ 인메모리 = 테스트·초기 개발용
  llm_litellm.py       공용 클러스터 호출
  llm_fake.py          고정 응답 = 클러스터 없이 개발

api/           ← 진입점
  mcp_server.py    C1 (build_mcp — Streamable HTTP)
  rest_server.py   C2 (create_app — mount_mcp=True면 /mcp 마운트)
  batch.py         심야 압축
```

**의존성 방향은 항상 안쪽(core)으로만.**

> **전송(C1):** MCP는 **Streamable HTTP**로만 서빙한다(stdio 제거). REST와 MCP는
> **한 프로세스**에서 돌며(`create_app(mount_mcp=True)`가 `/mcp`에 MCP 앱을 마운트),
> 같은 SpaceAService·스토어·Bearer 규칙을 공유한다. MCP를 못 붙이는 환경은 Skill
> 패키지로 동일한 REST를 호출한다 → [ADR 0002](../../../adr/0002-mcp-http-and-skill-dual-access.md).
`core`가 `adapters`를 import 하는 순간 이 설계는 무너진다.

> ⚙️ **원칙은 CI로 강제한다.** Python은 컴파일 타임에 import 방향을 막을 수단이 없어서,
> "core는 adapters를 import하지 않는다"는 규칙은 **문서에만 적어두면 반드시 깨진다.**
> 마감에 쫓기면 누구든 한 줄 질러 넣게 되고, 그 순간 DB 교체 가능성이 사라진다.
>
> [`import-linter`](https://import-linter.readthedocs.io/)를 CI에 넣어 **레이어 규칙을 테스트로 만든다.**
>
> ```ini
> # setup.cfg — layered contract: 위가 아래를 import 할 수 있고, 역방향은 금지
> [importlinter]
> root_package = space_a
>
> [importlinter:contract:layers]
> name = Core must not depend on adapters or api
> type = layers
> layers =
>     space_a.api
>     space_a.adapters
>     space_a.core
> ```
>
> 이러면 `core`가 `adapters`를 import하는 PR은 **CI에서 빨간불**이 뜬다.

### 이 구조의 실익

| 실익 | 내용 |
|---|---|
| **DB 결정을 미룰 수 있다** | 인메모리로 시작해 실 Vector DB로 갈아타도 `core`는 **한 줄도 안 바뀐다** |
| **클러스터 없이 개발 가능** | `llm_fake`로 로직을 다 짜두고, 나중에 `llm_litellm`으로 교체 |
| **테스트가 빠르다** | 인메모리 어댑터로 DB·GPU 없이 전체 플로우 검증 |

## 5. 공용 인프라 — RTX 5070 클러스터

> ⚠️ **소유권 주의.** 이 클러스터는 **Pillar 2의 일부가 아니라 세 Pillar가 공유하는 별도 인프라**다.
> Pillar 2는 **소비자(client)**일 뿐이며, OpenAI 호환 엔드포인트 하나만 바라본다.

### 5.1 로컬 LLM의 역할 분담

상용 모델은 **최종 결과물 도출·고도 추론에만** 쓰고, 백오피스 작업은 사내 로컬 LLM이 전담한다.

| 작업 | 사용 Pillar | 담당 | 비용 |
|---|---|---|---|
| 개인 Claude 로그 분석·코칭 | Pillar 1 | 로컬 LLM | 0 |
| 로그 파싱 → 엔티티 추출 → 스키마 정제 | **Pillar 2** | 로컬 LLM (Qwen/Llama 7~14B) | 0 |
| 임베딩 생성 (BGE-m3 등) | **Pillar 2** | 로컬 GPU | 0 |
| 검색 결과 재랭킹 | **Pillar 2** | 로컬 LLM | 0 |
| 심야 지식 압축 배치 | **Pillar 2** | 로컬 LLM | 0 |
| 최종 코드 생성·고난도 추론 | 임직원 에이전트 | 상용 모델 (Claude 등) | 과금 |

두 Pillar가 같은 GPU를 나눠 쓰므로, **용도별 모델명을 분리**해 자원 경쟁을 통제한다.

### 5.2 클러스터 구성 — LiteLLM + Ollama

개인 PC의 RTX 5070들을 **하나의 논리적 추론 서버**로 묶는다.

```
[Pillar 1 코칭 Agent]     [Pillar 2 space-a-hub]
            └───────────┬───────────┘
                        ▼
        [메인 PC — LiteLLM 게이트웨이]   ← 단일 창구, 노는 GPU로 요청 분배
                        │
             ┌────┬─────┼─────┬────┐
             ▼    ▼     ▼     ▼    ▼
            PC1  PC2   PC3   PC4  ...  ← 각자 Ollama (OLLAMA_HOST=0.0.0.0 필수)
```

```yaml
# LiteLLM config.yaml (메인 PC) — 용도별 모델명 분리로 Pillar 간 자원 경쟁 통제
model_list:
  # Pillar 2 (Space A Hub) 전용 — ingestion / re-ranking / 압축
  - model_name: space-a-ingest
    litellm_params:
      model: ollama/qwen2.5:7b
      api_base: http://192.168.0.10:11434
  - model_name: space-a-ingest
    litellm_params:
      model: ollama/qwen2.5:7b
      api_base: http://192.168.0.11:11434

  # Pillar 1 (코칭 Agent) 전용
  - model_name: space-a-coach
    litellm_params:
      model: ollama/qwen2.5:7b
      api_base: http://192.168.0.12:11434

router_settings:
  routing_strategy: least-busy    # 가장 한가한 GPU로 자동 분배
  failover_status_codes: [408, 500, 502, 503, 504]  # 꺼진 PC는 즉시 우회
```

**설정 주의:** 작업자 PC는 `OLLAMA_HOST=0.0.0.0`이어야 한다.
기본값 `127.0.0.1`이면 게이트웨이의 요청을 거부한다.

> 🔒 **보안 경고 — `0.0.0.0`은 인증 없이 GPU를 여는 것이다.**
> Ollama에는 자체 인증이 없다. `0.0.0.0`으로 열면 **같은 네트워크 대역의 누구나** 그 GPU를
> 무단으로 쓸 수 있다. 사내망이라고 안전한 게 아니다.
>
> 최소 대책 (인프라 담당과 협의 필요):
> - **방화벽으로 LiteLLM 게이트웨이 IP만 허용** — 가장 싸고 확실하다. 이것만 해도 대부분 막힌다.
> - 가능하면 **별도 서브넷/VLAN으로 격리**.
> - 그래도 부족하면 Ollama 앞에 **리버스 프록시를 두고 토큰 인증**을 붙인다.
>
> 이 클러스터는 **공용 인프라**이므로 이 대책의 실행 주체는 Pillar 2가 아니다.
> 다만 **계약상 전제**이므로 여기 명시한다.

### 5.3 왜 로드밸런싱인가 (분산 추론이 아니라)

| 방식 | 평가 |
|---|---|
| **로드밸런싱** (LiteLLM + 노드별 소형 모델) | ✅ **채택.** 작고 빠른 모델을 여러 대에 띄워 **동시 요청을 병렬 처리.** 네트워크 지연 없고 구현이 쉽다 |
| 분산 추론 (Exo 등, VRAM을 묶어 거대 모델 1개) | ❌ 일반 이더넷을 타므로 느리다. NVLink 없는 환경에서 70B를 굴려봐야 백오피스 작업엔 과잉 |

Space A의 백오피스 작업(파싱·요약)은 **거대 모델 1개보다 빠릿한 모델 여러 개**가 맞다.

### 5.4 Pillar 2 입장에서의 클러스터

내 컴포넌트가 이 인프라에 대해 **아는 것은 딱 하나**, OpenAI 호환 엔드포인트 주소뿐이다.
몇 대인지, Ollama인지, 어떤 모델인지 **알 필요도 없고 알아서도 안 된다.**

## 6. 다른 팀원을 안 기다리는 법

"독립 컴포넌트"의 실질은 **"남이 늦어도 내가 안 멈춘다"**이다.

| 아직 없는 것 | 그동안 나는 |
|---|---|
| RTX 5070 클러스터 | `llm_fake` 어댑터로 개발. 또는 **내 PC에 Ollama 한 대**만 띄워 동일 엔드포인트로 사용 — 인터페이스가 같으니 **교체 비용 0** |
| Pillar 1 코칭 Agent | **MCP Inspector**나 Claude Desktop을 직접 붙여 Tool 호출 검증 |
| Pillar 3 시각화 웹 | REST 응답을 **JSON 픽스처로 먼저 넘겨준다** → 프론트가 그걸로 선행 작업 |
| DB 확정 | 인메모리 어댑터로 진행 |

**반대로 내가 남을 막지 않으려면:** C1·C2 스키마를 **구현보다 먼저** 확정해 공유해야 한다.
계약만 있으면 **세 사람이 동시에** 각자 작업할 수 있다.

## 7. 레포 구성

| 방식 | 평가 |
|---|---|
| **단일 레포 + 디렉터리 분리** (`/hub`, `/agent`, `/web`, `/contracts`) | ✅ **권장.** 해커톤 규모에서 멀티레포는 오버헤드만 크다. 계약(스키마)을 `/contracts`에 두고 세 컴포넌트가 참조 |
| 멀티 레포 | ❌ 3인 규모에 CI·버전 동기화 비용이 이득보다 크다 |

> 레포 구성은 **되돌리기 어려운 결정**이므로, 확정되면 [`../../adr/`](../../../adr/)에 ADR로 남긴다.

## 8. 이 구성의 잔여 리스크

| 리스크 | 대응 |
|---|---|
| C1(MCP Tool) 시그니처가 중간에 바뀌면 **Pillar 1이 깨진다** | 초반에 고정하고, 변경 시 **필드 추가만** 허용 (제거·의미 변경 금지) |
| 인메모리로 짠 로직이 **실제 DB에서 성능이 안 나옴** | 실 DB 교체 단계에서 **실데이터 규모로 조기 검증** |
| 공용 클러스터가 **Pillar 1과 자원 경쟁** | LiteLLM에서 용도별 모델명 분리 (`space-a-ingest` / `space-a-coach`) (§5.2) |
