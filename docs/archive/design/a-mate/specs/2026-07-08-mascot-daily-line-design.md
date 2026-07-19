---
status: done
archived: 2026-07-19
---

# 마스코트 '오늘의 한마디' 설계 스펙 (마스코트 보이스 #1)

- 작성: 2026-07-08 (브레인스토밍 산출물)
- 백로그: 메모리 `diary-mascot-flavor-ideas` #1 (마스코트 보이스 계열 — #3 상주봇 말풍선과 동일 생성 백엔드 공유)
- 선행: 다이어리 문체 자연화(PR #15, `voice_guidance()`) 머지, 채팅 탭(`chat::ChatContext`) 존재
- 다음 단계: writing-plans → `docs/plans/` → SDD → PR → 사용자 E2E

## 1. 배경과 목표

프로필 초상(`RobotPortrait`, `App.svelte` 상시 사이드바) 밑에 **마스코트가 그날 하루를 한 문장으로 표현하는 상시 1줄**을 둔다.
재미·개성 강화가 목적. 다이어리(과거 하루 회고, 여러 문단)와 달리 **오늘 라이브 기분/재치 1줄**이라 별개 표면이다.
기존 말풍선(`bubble.ts`, 트리거형 finding/diary/occasion/chatter)과도 별개 — 상시 표시.

**재사용(핵심):** 사실 기반은 `chat::ChatContext`(오늘 세션수·입출력 토큰·절약 총량·상위 findings — 이미 조립됨),
자연스러움은 `voice_guidance()`(PR #15), 페르소나는 채팅과 동일 인물, 생성은 `Engine::generate`. 정밀도의 선(사실·수치만) 계승.

## 2. 브레인스토밍 결정 (2026-07-08)

| 논점 | 결정 |
|---|---|
| 생성 시점·신선도 | **scan:done 시 갱신(stale할 때만) + 날짜 캐시**. fingerprint(세션수\|입력\|출력\|findings수)가 바뀌었거나 캐시 없을 때만 재생성. 다이어리 backfill과 일관 |
| 폴백 | **엔진 미설정 → 줄 숨김**(초상만). 오늘 활동 있으면 LLM 생성. **활동 0건 → LLM 없이 정적 1줄** |
| 정적 문구 | "오늘은 널널하네. 근데 좀 심심;;;" (여유롭게 노는 건 좋지만 살짝 심심한 다마고치 기색) |
| 모듈 위치 | **신규 `crates/core/src/mascot.rs`** — 메모리가 #1/#3을 "마스코트 보이스"로 묶음. #3 상주봇 한마디가 나중에 여기 재사용 |
| 사실 기반 | `chat::ChatContext` 재사용(오늘 요약). 새 수집 불필요 |
| 관점/길이 | chat 페르소나(1인칭·주인 호칭·능청) + voice_guidance. 짧은 한 문장(≤40자), 브리프 사실·수치에만 근거 |

## 3. 아키텍처 · 데이터 흐름

```
scan:done → maybe_generate_daily_line(store, engine):
  ctx = chat_context_inner(store)          # 오늘 요약 (기존)
  fp  = mascot::facts_fingerprint(&ctx)    # "세션수|입력|출력|findings수"
  if ctx.session_count == 0:
      upsert_daily_line(today, static_daily_line(), fp)   # LLM 없음
  elif store.get_daily_line(today).fingerprint == fp:
      skip                                                 # 사실 불변 → 재생성 안 함
  else:
      system = mascot::build_daily_line_prompt(&ctx)       # 페르소나+사실+voice_guidance
      text   = engine.generate(system, "")                 # 락 밖 네트워크
      upsert_daily_line(today, text, fp)

commands::get_daily_line() -> Option<String>   # 오늘 날짜 캐시 조회(없으면 None)
App.svelte: RobotPortrait 밑 1줄 — mount + scan:done에 get_daily_line() 갱신, None이면 미표시
```

- 엔진 미설정 시 `maybe_generate_daily_line`은 no-op(엔진 없으면 생성/정적 모두 스킵) → 캐시 비어 command가 None → 줄 숨김.
- 네트워크(LLM)는 다이어리와 동일하게 **store 락 밖**에서 호출.
- "오늘" = 로컬 자정 기준 날짜(다이어리 날짜 정책 §7 계승).

## 4. 파일 / 컴포넌트

- **`crates/core/src/mascot.rs`** (신규):
  - `pub fn build_daily_line_prompt(ctx: &ChatContext) -> String` — chat 페르소나 + `[오늘 요약]` 사실 블록 + `voice_guidance()` + "오늘 하루를 기분/재치 담아 짧은 한 문장(≤40자)으로, 사실·수치에만 근거, 딱 하나만."
  - `pub fn static_daily_line() -> &'static str` — 0건 폴백 문구(§2).
  - `pub fn facts_fingerprint(ctx: &ChatContext) -> String` — `"{session_count}|{tok_input}|{tok_output}|{findings.len()}"`.
- **`crates/core/src/store.rs`**: `daily_line(date TEXT PRIMARY KEY, text TEXT, fingerprint TEXT)` 테이블 + migration(전례 따라 `CREATE TABLE IF NOT EXISTS`) + `get_daily_line(date) -> Option<(String, String)>`(text, fp), `upsert_daily_line(date, text, fp)`.
- **파이프라인**(다이어리 backfill 옆, core `pipeline.rs` 또는 앱 스캔 경로): `maybe_generate_daily_line(store, engine)` — §3 흐름. 엔진 Option, 없으면 no-op.
- **`src-tauri/src/commands.rs`**: `get_daily_line(state) -> Result<Option<String>, String>` — 오늘 날짜로 `get_daily_line` 조회 후 text만.
- **`src/lib/api.ts`**: `getDailyLine(): Promise<string | null>` 바인딩.
- **`src/App.svelte`**: `<RobotPortrait />` 밑에 `<p class="daily-line">` — `$state`로 값 보관, 마운트 + `scan:done` 이벤트에 `getDailyLine()` 재조회, null이면 미렌더. 스타일은 `theme.css` 토큰만.

## 5. 프롬프트 & 폴백 상세

- **페르소나/사실**: `build_chat_system_prompt`와 동일 인물·오늘 요약 블록을 공유하되, 지시는 "대화"가 아니라 "**오늘 하루의 기분이나 재치를 담은 짧은 한 문장**(≤40자). 브리프에 없는 수치 지어내기 금지. 문장 하나만 출력."
- **자연스러움**: `voice_guidance()` 주입(번역투·이중피동·상투구·이모지 남발 회피, 구어체·리듬). 헤지 carve-out 동일 계승.
- **정적(0건)**: `static_daily_line()` 고정 — LLM 호출 없이 캐시. fingerprint는 `"0|0|0|N"` 형태라 활동이 생기면 자연히 재생성.
- **미설정**: 엔진 없으면 캐시에 아무것도 안 씀 → command None → 줄 숨김(에러 아님, chat 관례 계승).

## 6. 검증 / 테스트 기준

- **`mascot.rs` 단위**: `build_daily_line_prompt`가 페르소나(주인)·오늘 사실(세션수)·voice_guidance 마커·"한 문장"/"짧" 지시 포함 / `facts_fingerprint`가 사실 바뀌면 달라지고 같으면 동일 / `static_daily_line`이 정적 문구 반환.
- **store**: `daily_line` 라운드트립(upsert→get, text·fp) / 없으면 None.
- **파이프라인**: MockEngine으로 — 활동 있음→생성 호출·캐시, fp 동일→skip(재호출 없음), 0건→정적 문구 캐시(엔진 미호출), 엔진 None→no-op(캐시 빔).
- **command**: 오늘 캐시 있음→Some(text), 없음→None.
- **프론트**: `getDailyLine()` null→줄 미표시, 값→렌더(vitest 가능 범위) + `npm run build`.
- **수동 E2E**: 실엔진으로 오늘 한마디 생성, 초상 밑 표시·자연스러움·사실 정합 육안. 활동 0인 날 정적 문구 확인.

## 7. 스코프 · 후속

- **이번만**: #1 오늘의 한마디(상시 1줄). #3 상주봇 주기 말풍선은 후속(같은 `mascot.rs`·생성 백엔드 재사용, 표면·케이던스만 다름).
- 정적 문구 다변화(요일/랜덤 회전)는 YAGNI — 이번엔 단일 문구.
- 함께 반영(브랜치 동반): gemini PR #16 권고 #2·#3(R5 cross_session evidence `total_files`) — 이미 커밋 완료(별도 사안).

## 8. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- 엔진 URL은 네이티브에서 `localhost:4444`(`.env`), dev 빌드는 dotenvy 자동 로드.
- 진행: 이 스펙 승인 → writing-plans(`docs/plans/`) → SDD → PR → 사용자 E2E.
