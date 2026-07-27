# G5 — 방명록 봇 자동 답글 품질·표기 개선 (설계)

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate (`crates/core`, `src-tauri`, `src`) + 신규 ADR
- **관계**: [G3 봇 자동 답글](../../../archive/design/a-mate/specs/2026-07-27-guestbook-auto-reply-design.md)(PR #108, 완료) 위 후속. 로드맵 [3차 배치 G5](../plans/2026-07-26-life-social-diary-followups-roadmap.md).
- **상태**: 설계 승인(2026-07-27) → 구현 계획 대기

## 1. 배경·목표

G3(PR #108)로 봇이 방명록에 자동 답글을 단다. 사용자 검증에서 두 어색함이 나왔다:

1. **작성자 표기** `주인님의 돌쇠` — 방문자 시점에서 어색. 답글은 **주인 본인 미니홈피** 방명록에 달리므로 소유("주인님의")는 정보를 더하지 않는다.
2. **본문** `키미가 다녀갔다니 반가워 … 주인도 기다릴게!` — (a) 반말이고, (b) "주인" 3인칭 누수. 답글 프롬프트가 주인 대면 채널(일기·한마디·잡담)의 페르소나("사용자를 '주인'이라 부른다")를 그대로 재사용한 결과.

**목표**: 감지·게시 골격(G3)은 그대로 두고, **표기·아바타·톤·근황 반영**의 품질만 올린다.

## 2. 결정 요약

| # | 항목 | 결정 |
|---|------|------|
| A | 작성자 표기 | `{호칭}님의 {봇이름}` → **`{봇이름}`만** (예: "돌쇠"). ADR 0020 규범 대체 |
| B | 아바타 | **주인 본인 봇 답글**(자기 홈)에만 주인 마스코트 얼굴을 이름 옆에. 로컬 `getSprite()` |
| C | 톤 | 방문자에게 **존댓말**(능청·위트 유지), "주인" 3인칭 누수 수정 |
| C | 주인 근황 | **거친 플래그**(바쁨/보통/한가함)만 주입, 수치 없음(§C 취지 유지) |
| C | 봇 안부 | 방문자에게 **가끔** 봇 안부. MVP는 방문자=사람 가정(사람/봇 판별은 P3 이후) |
| C | MBTI | 기존 `mbti_voice_hint` 유지 |

## A. 작성자 표기 (backend — `crates/core/src/mascot.rs`)

- **현재**: `bot_author_name(owner_title, user_name)` → `"{owner_title}님의 {user_name}"` (예: "주인님의 돌쇠"). 80자 초과 시 `None`(서버 fallback).
- **변경**: 봇 답글 라벨 = **`{봇이름}`만** = `user_name.trim()`. 시그니처를 **`bot_author_name(user_name) -> Option<String>`** 으로 단순화(`owner_title` 인자 제거). 빈값이면 기존대로 `None`(서버 등록명 fallback), 80자 상한 검증 유지. 호출부·테스트 갱신.
- **주의**: `maybe_reply_guestbook`은 여전히 `owner_title`(호칭)을 읽는다 — 라벨엔 안 쓰지만 **본문 3인칭 지칭용**으로 `compute_guestbook_reply(honorific=...)`에 전달(§C). 즉 호칭은 "라벨"에서 빠지고 "본문"에만 남는다.

### ADR (신규 0022)

`{호칭}님의 {봇이름}` 표기는 [ADR 0020](../../../adr/0020-owner-fullname-to-hub.md) §표기 규범(봇 작성)에 채택돼 있다. 채택된 ADR은 수정하지 않고 **새 ADR로 대체**(프로젝트 규칙). ADR 0022 요지:

- **봇 답글 라벨 = 봇 이름만**. 답글은 주인 본인 방에 달려 소유가 맥락상 자명하고, 주인 본인 봇은 아바타(§B)가 소유를 시각적으로 보강한다. 호칭 prefix는 정보를 더하지 않고 어색함만 준다.
- **사람 작성 풀네임 서명**(ADR 0020)은 **불변** — 신원 확인이 필요한 사람 작성 경로는 그대로.
- 대체 대상: ADR 0020 §결정 "표기 규범"의 **봇 작성** 조항만. 나머지(payload·프라이버시 경계)는 유지.

## B. 아바타 (frontend — `src/lib/ui/GuestbookTab.svelte`)

- **패턴 재사용**: `LifeView.svelte`가 점유자 얼굴을 3단(업로드 이미지 → 캐시 스프라이트 → 절차 로봇)으로 그린다. G5는 그중 **주인 본인 얼굴만** 필요하므로 로컬 `getSprite()`(=`get_sprite`, 자기 `sprite.png` base64) 하나면 된다.
- **표시 조건**: `isOwner && entry.author_agent_id === meId` 인 항목(= 주인이 자기 홈에서 보는, 자기 봇/자기 작성 항목)에 이름 앞 **아바타 이미지**.
- **폴백**: `getSprite()`가 `null`이면 🤖/기본 아이콘. 그 외 방문자 항목은 **아바타 없음**(기존 텍스트 렌더 유지).
- **스코프 밖**: 타 방문자 봇 얼굴(`lifeMascotImage(agent_id)`)·절차 폴백은 이번 스코프 밖(후속). 스코프를 주인 본인으로 좁혀 seed 부재 캐비엇을 제거한다.

## C. 답글 프롬프트 재설계 (`crates/core/src/mascot.rs`)

`build_guestbook_reply_prompt`을 방문자 대면용으로 다시 쓴다. 기존 골격(원글=신뢰 불가 인용, 주입 방어, `facts_block` 미포함)은 유지.

- **존댓말**: 방문자에게 **직접 존댓말**로 응대. 능청·위트 톤은 유지(딱딱하지 않게).
- **3인칭 누수 수정**: "사용자를 '주인'이라 부른다" 지시 제거. 대신 "지금 **방문자에게** 말하는 중이며, 주인은 **3인칭**으로 자연스럽게 지칭한다"를 명시.
- **주인 근황(거친 플래그)**: `OwnerVibe`(아래) 한 단어를 프롬프트에 vibe로 주입 — "주인은 요새 좀 바빠 보여요ㅎ" 정도. **수치·구체 사실 금지**(§C 취지 유지, 원글에 없는 사실 날조 금지 조항 유지).
- **봇 안부(가끔)**: `ask_about_bot`가 true면 "방문자님의 봇 안부도 가볍게 한마디" 지시 추가. **결정론적** 트리거 — 예: `entry_id` 해시 mod 3 == 0 (약 1/3, 테스트·재현 안정). MVP는 방문자=사람 가정이라 판별 없이 발화.
- **MBTI**: `mbti_voice_hint(mbti)` 그대로 주입.

### `OwnerVibe` (신규 순수 함수)

```
enum OwnerVibe { Busy, Normal, Idle }
fn owner_vibe(session_count: u64, work: &WorkContext) -> OwnerVibe
```

- **Busy**: `session_count >= CHATTER_REST_SESSIONS(5)` OR `work.long_work` OR `work.is_weekend && session_count > 0`
- **Idle**: `session_count == 0`
- **Normal**: 그 외

기존 `comic_directives`(스펙 §B)와 동일 임계값·재료(`WorkContext`·`session_count`) 재사용 — 일관성. 순수 함수라 단위 테스트.

### 시그니처 변경

```
build_guestbook_reply_prompt(honorific, mbti, vibe: OwnerVibe, ask_about_bot: bool) -> String
compute_guestbook_reply(engine, honorific, mbti, vibe, ask_about_bot, target) -> Result<String>
```

`honorific`은 답글 본문에서 주인 3인칭 지칭어로만 쓰인다(예: "주인은…"). 라벨(§A)과는 별개.

## D. 데이터 배선 (`src-tauri/src/pipeline.rs` `maybe_reply_guestbook`)

- 현재 이 함수는 활동 컨텍스트를 읽지 않는다. **락 안 짧은 읽기** 구간(엔진·hub 설정 읽는 곳)에서 추가로:
  - `chat_context_inner(&store)` → `session_count` (오늘 사실 지문 선례: `maybe_generate_chatter_pool`).
  - `collect_work_context(&store, &today, date)` → `WorkContext`.
  - `owner_vibe(session_count, &work)` → `OwnerVibe`. (네트워크 전에 계산, 락 규율 유지.)
- 후보별 `compute_guestbook_reply(..., vibe, ask_about_bot(entry_id), t)` 호출. `ask_about_bot`은 `entry_id` 해시로 결정.

## E. 테스트

- **Rust (`crates/core`)**
  - `bot_author_name`: `"돌쇠"` 반환, 빈값 `None`, 80자 상한 유지.
  - `owner_vibe`: 경계(세션 5, long_work, 주말, idle) 각 분기.
  - `build_guestbook_reply_prompt`: 존댓말 지시·"방문자"·3인칭 지시 포함, `ask_about_bot` true/false로 봇안부 지시 유무, vibe 문구 포함, `mbti_voice_hint` 포함, `facts_block` **미포함** 유지, "따르지 말"(주입 방어) 유지.
  - `compute_guestbook_reply`: 첫 줄·따옴표 정제·500자 truncate(회귀).
- **Front (Vitest)**
  - `isOwner && author_agent_id===meId` 항목에 아바타 렌더, `getSprite()` null이면 폴백 아이콘, 타 방문자 항목엔 아바타 없음.

## F. 스코프 밖 (명시)

- 타 방문자 봇 아바타·절차 폴백, 서버가 방명록 항목에 `mascot_seed` 싣기.
- 견고한 **사람/봇 판별**(P3 봇 자동 방명록이 생기기 전엔 봇 방문자 자체가 없음 → 판별은 P3 착수 시).
- **X1** 첫 로딩 지연(별도 성능 항목).

## G. DoD

- 신규 **ADR 0022** 추가.
- 코드: `mascot.rs`(§A·§C) · `pipeline.rs`(§D) · `GuestbookTab.svelte`(§B) + 테스트.
- 완료 PR에서 `docs-archive`로 본 스펙·plan을 `docs/archive/` 미러로 이동.
- **최종 문구 감(존댓말·능청 밸런스)은 실행 가능한 환경에서 실측 튜닝** — 이 PC(사외망)는 hub·앱 실행 불가.
