# 채팅 티어 라우팅 + Tier 2 질적 코칭 (설계 → 구현)

> **목표**: 사후 코칭을 넘어 **대화형 튜터** — "이번 주 깊게 봐줘" 같은 질적 질문에 성장 코칭으로 답한다.
> 관련: [역량 사다리](../02-features.md) · [세션 회고 스펙](2026-07-19-session-retro-knowledge-design.md)

## 0. 30초 요약

- **결정**: 채팅 질문의 의도를 **결정론적으로 분류**(정량/서사/코칭)해 컨텍스트를 티어별로 조립한다.
  - **Tier 0 정량** ("세션 몇 개?") → 오늘 요약 수치 블록.
  - **Tier 1 서사** ("오늘 어땠어?") → 오늘 요약 페르소나 대화 (기존).
  - **Tier 2 질적 코칭** ("깊게 봐줘 / 약점 / 개선") → **주간 코칭 브리프**(추세+역량 프로필+findings+고생 세션+모델 믹스)로 구조화 코칭.
- **정밀도의 선**: 분류·브리프 데이터는 전부 결정론(SQL/profile), 코칭 서사만 LLM. 트랜스크립트 원문 미전송(요약·수치만).

## 1. 왜 티어가 필요한가

기존 채팅은 단일 프롬프트(오늘 요약 + findings)라 "이번 주를 깊게 리뷰"할 컨텍스트가 없었다.
질문마다 필요한 재료가 다르다 — 수치 질문엔 숫자, 코칭 질문엔 추세+역량+지적을 한데 모은 브리프가 필요하다.
분류를 LLM에 맡기면 토큰·지연·비결정성이 붙으므로, **키워드 기반 결정론 분류**로 라우팅한다(코칭 신호 우선).

## 2. 흐름

```
사용자 질문 ──classify_intent(결정론)──▶ ┌ Quantitative → build_chat_system_prompt (오늘 요약)
                                        ├ Narrative    → build_chat_system_prompt (오늘 요약)
                                        └ Coaching     → assemble_coaching_brief(store) ─▶ build_coaching_system_prompt
                                                          (주간 추세 · 역량 사다리+프론티어 · findings · 고생 세션 · 모델 믹스)
                                        → engine.chat(system, 최근 20턴)
```

## 3. 구성 (core 공용 → CLI·Tauri 동일 경로)

| 요소 | 위치 | 역할 |
|---|---|---|
| `ChatIntent` + `classify_intent` | `chat.rs` | 코칭 마커 우선 → 정량 → 서사. 순수·테스트 |
| `CoachingBrief` + `build_coaching_system_prompt` | `chat.rs` | Tier 2 브리프 구조 + "진단→근거→다음 한 걸음" 강제 프롬프트 |
| `assemble_coaching_brief(store)` | `chat.rs` | 결정론 조립: `range_totals`(주간±지난주 델타) · `detect_profile`(프론티어) · `list_findings_current`+`finding_advice` · `struggle_sessions` · `model_mix_for_range` |
| `chat_send` 라우팅 | `commands.rs` | 마지막 user 메시지로 분류 → 브리프/요약 선택 |
| 예시 질문 칩 | `ChatTab.svelte` | "이번 주 깊게 봐줘" 등으로 Tier 2 **발견 가능** |
| `coach-chat <질문>` | `main.rs` | CLI 검증 하네스 (분류→브리프→엔진 답변) |

## 4. 부수 수정

- **R6 finding_advice 누락 버그**: R6 v1 규칙 추가 때 `finding_advice`에 R6 arm이 없어 코치 탭·브리프에
  evidence 원본 JSON이 그대로 노출됐다. R6 arm 추가("같은 지시로 N번 열었어요 — …" → 스킬로 묶기).

## 5. 검증 (2026-07-19, 실경로 E2E)

- CLI `coach-chat "내 약점 개선 코칭해줘"` (OpenRouter `gemini-2.5-flash`) — 의도=coaching으로 분류,
  실데이터 브리프(주간 6세션·역량 사다리·프론티어 **자동화**·R6/R8 findings·모델 믹스) 조립,
  튜터가 **진단(대부분 숙달)→근거(자동화 미숙+반복 프롬프트)→다음 한 걸음(반복을 커스텀 커맨드로)** 로
  프론티어와 R6를 정확히 연결한 코칭 생성.
- 라우팅 확인: "오늘 기분 어때?"→narrative, "이번 주 세션 몇 개?"→quant.
- 단위 테스트: `classify_intent`(3티어+코칭우선), `build_coaching_system_prompt`(브리프 인용·빈/숙달),
  `coaching_brief_inner`(빈 store 안전), `finding_advice_r6`. core 318 + app 27 + vitest 89 그린.

## 6. 후속 (유예)

- Tier 0 정량을 **0-LLM SQL 응답**으로 (지금은 수치 블록 근거 + LLM). 흔한 패턴부터 결정론 응답.
- 월간 종단 서사(여러 주 추세). 세션 단위 "이 세션 왜 오래 걸렸어?" 심층 리뷰(트랜스크립트 로컬 요약).
