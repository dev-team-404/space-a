# 0003. 로컬 LLM 클러스터는 공용 인프라다

- 상태: 채택(Accepted) — 팀 합의 완료 (2026-07-12)
- 날짜: 2026-07-12

## 배경

SPACE-A는 토큰 비용 절감이 핵심 가치다. 이를 위해 사내 유휴 PC의 GPU(RTX 5070)에
로컬 LLM을 띄워 백오피스 연산을 전담시키기로 했다.

문제는 **이 클러스터를 누가 소유하느냐**다. 처음 설계에서는 Pillar 2(Space A Hub)의
인프라로 그려졌으나, 검토 결과 **Pillar 1(코칭 Agent)도 로컬 LLM이 필요**하다는 것이 드러났다.

| 작업 | 사용 Pillar |
|---|---|
| 개인 Claude 로그 분석·코칭 | **Pillar 1** |
| 로그 파싱 → 엔티티 추출 → 스키마 정제 | **Pillar 2** |
| 임베딩 생성 | **Pillar 2** |
| 검색 결과 재랭킹 | **Pillar 2** |
| 심야 지식 압축 배치 | **Pillar 2** |

클러스터가 Pillar 2 소유가 되면 **Pillar 1이 Pillar 2를 거쳐야 하고, Pillar 2의 배포가
Pillar 1의 기능을 멈춘다.** 컴포넌트 경계가 무너진다.

## 결정

**로컬 LLM 클러스터를 세 Pillar가 공유하는 별도 공용 인프라로 둔다.**

### 1. 소유권

- 클러스터는 **어느 Pillar에도 속하지 않는다.**
- Pillar 1과 Pillar 2는 **소비자(client)**일 뿐이다.
- 각 Pillar가 아는 것은 **OpenAI 호환 엔드포인트 주소 하나**뿐이다.
  몇 대인지, Ollama인지, 어떤 모델인지 알 필요도 없고 알아서도 안 된다.

### 2. 구성 — LiteLLM 게이트웨이 + 노드별 Ollama

```
[Pillar 1 코칭 Agent]     [Pillar 2 space-a-hub]
            └───────────┬───────────┘
                        ▼
        [메인 PC — LiteLLM 게이트웨이]   ← 단일 창구
                        │
             ┌────┬─────┼─────┬────┐
             ▼    ▼     ▼     ▼    ▼
            PC1  PC2   PC3   PC4  ...  ← 각자 Ollama
```

**분산 추론(Exo 등, 여러 VRAM을 묶어 거대 모델 1개)이 아니라 로드밸런싱을 택한다.**
일반 이더넷을 타는 분산 추론은 느리고, 백오피스 작업(파싱·요약)에는
**거대 모델 1개보다 작고 빠른 모델 여러 개**가 맞다.

### 3. Pillar 간 자원 경쟁은 모델명으로 분리한다

```yaml
# LiteLLM config.yaml
model_list:
  - model_name: space-a-ingest    # Pillar 2 전용
    litellm_params: { model: ollama/qwen2.5:7b, api_base: http://192.168.0.10:11434 }
  - model_name: space-a-coach     # Pillar 1 전용
    litellm_params: { model: ollama/qwen2.5:7b, api_base: http://192.168.0.12:11434 }

router_settings:
  routing_strategy: least-busy
  failover_status_codes: [408, 500, 502, 503, 504]
```

### 4. 인터페이스는 OpenAI 호환으로 고정한다

이게 이 결정의 실익이다. **클러스터가 없어도 개발이 멈추지 않는다.**
각 Pillar는 fake 어댑터나 로컬 Ollama 1대로 개발하다가, 나중에 **엔드포인트 주소만 바꾼다.**

## 결과

### 좋아지는 것

- **컴포넌트 경계가 지켜진다.** Pillar 2의 배포가 Pillar 1을 멈추지 않는다.
- 각 Pillar가 클러스터 유무와 무관하게 **독립적으로 개발**할 수 있다.
- 클러스터 구성을 바꿔도(노드 추가, 모델 교체) **소비자는 아무것도 안 고친다.**

### 감수하는 것 / 미결정

- **운영 주체가 필요하다.** 클러스터는 누구의 담당도 아니므로, 세팅·운영을 누가 할지
  별도로 정해야 한다.
- ⚠️ **보안: `OLLAMA_HOST=0.0.0.0`은 인증 없이 GPU를 여는 것이다.**
  Ollama에는 자체 인증이 없어, 같은 네트워크 대역의 누구나 무단 사용할 수 있다.
  **최소한 방화벽으로 LiteLLM 게이트웨이 IP만 허용해야 한다.** (인프라 담당 협의 필요)
- 게이트웨이가 **단일 장애점(SPOF)**이다. 게이트웨이 PC가 죽으면 전체가 멈춘다.
  해커톤 규모에서는 감수한다.

## 관련

- [Space A Hub 아키텍처 §5](../design/collab-space/03-architecture.md#5-공용-인프라--rtx-5070-클러스터)
- [ADR 0002](0002-repo-structure.md) — 컴포넌트 경계 유지 원칙의 연장선
