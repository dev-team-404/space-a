# R6 반복 지시 → SKILL.md 초안 생성 (킥오프 3대 차별점 완성)

> **목표**: "반복 작업은 Skill로 전환된다"(README)를 **탐지에서 산출물까지** 잇는다.
> 관련: [코칭 규칙 v3](../plans/) · [세션 회고 스펙](2026-07-19-session-retro-knowledge-design.md) · 백로그 [04-history-and-roadmap.md §5]

## 0. 30초 요약

- **결정**: R6가 "같은 지시로 세션을 반복해서 연다"를 감지하면(v1), 그 반복이 **실제로 어떤 도구 시퀀스였는지** 세션에서 모아
  붙여넣기만 하면 되는 **`.claude/skills/<name>/SKILL.md` 초안**을 만든다(v2).
- **정밀도의 선**: 재료 수집은 결정론(SQL) — 대표 프롬프트로 세션 되짚기 + 실제 사용 도구 상위 집계.
  서사(절차 문장·인자화)만 LLM. **엔진이 부실하거나 없으면 결정론 골격**으로도 바로 쓸 수 있는 초안을 낸다.
- 사용자는 코치 탭 R6 카드의 "🧩 스킬 초안 만들기" → 미리보기 → **복사** 또는 **저장**(`~/.claude/skills/`)까지 한 번에.

## 1. 왜 이게 마지막 반 걸음인가

킥오프 3대 차별점(WSL+Windows 감시, 컨텍스트-비용 정적분석, 데스크톱 코치 UX) 위에 얹힌
제품 목표는 "집계가 아니라 **코칭**". R6 v1은 "묶으세요"라고 **제안만** 했다 — 이는 대시보드의 언어다.
튜터라면 **전환 자체를 대신 해줘야** 한다. 감지(R6) → 재료(도구 시퀀스) → **산출물(SKILL.md)**.

## 2. 데이터 흐름

```
sessions.first_prompt_preview ──정규화 동치──▶ R6 Finding{ repeated_prompt, session_count }
        │                                              │
        │  gather_context(host, repeated_prompt)       │  (코치 탭에서 사용자가 버튼 클릭)
        ▼                                              ▼
  같은 지시로 열린 세션 집합 ──▶ events(tool_call) 상위 집계 ──▶ DraftContext
                                                       │
                                    build_draft(ctx, engine?)
                                     ├─ 엔진 有: 골격을 주고 LLM이 보강 → 검증 통과 시 채택
                                     └─ 엔진 無/부실: 결정론 골격(skeleton_draft)
                                                       │
                                     write_draft(~/.claude/skills/<slug>/SKILL.md)  (충돌 시 -2,-3…)
```

## 3. 결정론 vs LLM 경계

| 단계 | 방식 | 근거 |
|---|---|---|
| 반복 감지 | 결정론 (R6, 정규화 동치) | 근거 없는 수치 금지 — `est_tokens_saved=0` 가치제안형 |
| 세션 되짚기 | 결정론 (`sessions_with_prompts` + 같은 `normalize`) | R6와 동일 기준이어야 매칭이 일관 |
| 도구 집계 | 결정론 (`tool_usage_for_sessions`, `GROUP BY raw_name`) | "실제로 쓴 것"만 — 환각 절차 방지 |
| 절차 서사 | LLM (골격 보강) | 문장 다듬기·인자화는 생성 영역 |
| 품질 게이트 | 결정론 (프런트매터+절차+길이≥180) | LLM이 골격보다 부실하면 **골격 채택** |

## 4. 프라이버시

- SKILL.md 초안에는 사용자의 프롬프트·도구 이름이 들어간다 → **전적으로 로컬**. 허브로 나가지 않는다.
  (R6 Finding 자체가 허브 공유 화이트리스트에서 제외됨 — evidence에 프롬프트 원문 포함.)
- 저장은 사용자의 명시적 "저장" 클릭에서만. 기존 스킬은 **덮어쓰지 않고** `-2,-3…` 접미로 보호.

## 5. 표면

- **CLI**: `agent-mentor skill-draft` — R6 Finding마다 초안 출력(엔진 설정 시 LLM, 아니면 골격). 검증·데모용.
- **앱**: 코치 탭 R6 카드 → `generate_skill_draft(host, representative)` → 모달(복사·저장) → `save_skill_draft(slug, md)`.

## 6. 검증 (2026-07-19, 실경로 E2E)

- 합성 세션 3건(같은 첫 지시 + Bash·Bash·Write) ingest → `rules`에서 R6 발화(`session_count:3`).
- `skill-draft`(OpenRouter `google/gemini-2.5-flash`): 도구 집계 `Bash 6 / Write 3` 정확, LLM이
  '언제 쓰나'+'절차'(도구 반영, `date` 인자화) 포함한 완결 SKILL.md 생성. 엔진 부실 시 골격 폴백 확인.
- 단위 테스트: `gather_context`(동치·도구 분리), `skeleton`(유효 SKILL.md), `build`(LLM 채택/폴백),
  `write_draft`(생성·충돌회피), `slugify`(한글 폴백). core 309 + app 25 + vitest 89 그린.

## 7. 후속 (유예)

- v2 tool-시퀀스 유사도 군집(첫 프롬프트 동치를 넘어 "비슷한 작업 흐름"까지) — 백로그 §5.
- 저장 후 실제 스킬 사용 여부를 추적해 "이 스킬이 반복을 줄였다"를 종단 서사로 환류.
