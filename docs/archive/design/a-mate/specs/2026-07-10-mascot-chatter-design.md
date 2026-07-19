---
status: done
archived: 2026-07-19
---

# 마스코트 상주봇 잡담 업그레이드 설계 스펙 (마스코트 보이스 #3)

- 작성: 2026-07-10 (브레인스토밍 산출물)
- 백로그: 메모리 `diary-mascot-flavor-ideas` #3 (마스코트 보이스 계열 — #1 오늘의 한마디(PR #17 머지)와 생성 백엔드 공유)
- 선행: 마스코트 보이스 #1(`crates/core/src/mascot.rs`, `daily_line` 파이프라인) 머지, `voice_guidance()`(PR #15), `chat::ChatContext` 존재
- 다음 단계: writing-plans → `docs/plans/` → SDD → PR → 사용자 E2E

## 1. 배경과 목표

트레이 상주 마스코트 창의 **주기적 잡담 말풍선**을 "마스코트 보이스"로 업그레이드한다.

**전제(탐색으로 확인된 사실):** 미니홈피 대개편(§6)이 제거한 것은 "수집 완료/세션 갱신" 류
스캔 요약 대사이고, **"④가끔 잡담(보수적 빈도)"은 유지되어 지금도 살아 있다** —
`src/Mascot.svelte` 잡담 타이머(normal 20–40분 / low(기본) 60–120분 / off, 새벽 1–7시 침묵,
다른 말풍선 표시 중 침묵) + `bubble.ts` 정적 CHATTER 10개 랜덤 pick.

따라서 이번 작업은 잡담 되살리기가 아니라 **기존 채널의 콘텐츠·제어 업그레이드**다. 갭 두 가지:
- 콘텐츠가 정적 10개뿐이라 금방 반복되고, 사용기록 연계가 `session_count` 템플릿 수준으로 얕음.
- `chatter_level`을 바꿀 UI가 없음(commands ALLOWED에만 존재, 전원 사실상 'low' 고정) → 뮤트/빈도 제어 불가.

"의미 있고 안 성가심" 원칙: 사용기록 연계는 사실·수치에만 근거(정밀도의 선), 사실무관
카테고리(순수 잡담/응원)는 지어내지 않도록 정적 큐레이션, 빈도는 사용자가 트레이에서 제어.

## 2. 브레인스토밍 결정 (2026-07-10)

| 논점 | 결정 |
|---|---|
| 스코프 프레임 | **기존 채널 업그레이드** — 타이머·말풍선 UI·X 닫기·야간 침묵 유지, 콘텐츠와 설정 노출만 개선 |
| 카테고리·생성원 | **LLM 1결 + 정적 2결**: ①사용기록 연계 잡담 = LLM(오늘 `ChatContext` 사실 기반, 잡담 전용 경량 프롬프트) ②순수 잡담 ③응원 = 정적 큐레이션(기존 CHATTER 유지+확장, 사실 주장 금지) |
| 생성 시점·캐시 | **스캔 편승 + 풀 캐시** — scan:done 후 fingerprint stale 시에만 LLM 1회 호출로 잡담 5개 배치 생성, `chatter_pool` 날짜 캐시. 프론트 타이머는 pick만 |
| 빈도 설정 노출 | **트레이 "잡담" 서브메뉴** (자주=normal / 가끔=low / 안 함=off, 3택 1) — 기존 CheckMenuItem→settings:changed 패턴 재사용 |
| realtime_advice 관계 | **별개 채널 유지·무변경** — 실시간 조언=코칭 채널, 잡담=보이스 채널. "다른 말풍선 표시 중 잡담 침묵" 기존 규칙이 우선순위 보장 |
| 케이던스 | **현행 유지** — normal 20–40분 / low 60–120분 / off, 새벽 1–7시 침묵 (값 변경 없음) |
| 신선도·중복 방지 | 하루 안 신선도 = fp 회전(사실 바뀌면 다음 스캔에서 풀 재생성), 표시 중복 = 최근 표시 3개 회피(세션-로컬, 영속화 안 함) |
| 이벤트 | **추가 안 함** — 타이머가 발화 시점에 커맨드로 pull. 잡담엔 즉시성 요구가 없어 `daily-line:ready` 같은 이벤트 불필요 |

## 3. 아키텍처 · 데이터 흐름

```
scan:done 후 (pipeline.rs, maybe_generate_daily_line 다음):
  maybe_generate_chatter_pool(app 불필요 — store_mutex만):
    ① 락: ctx = chat_context_inner(store), cached_fp = get_chatter_pool(today).fp → 즉시 해제
    ② 락 밖: compute_chatter_pool(engine, ctx, cached_fp)
         fp 동일               → None (skip)
         ctx.session_count==0  → Some((빈 풀, fp))    # LLM 호출 없음
         그 외                 → LLM 1회 → parse_chatter_lines → Some((잡담 ≤5개, fp))
    ③ 락: upsert_chatter_pool(today, lines JSON, fp) → 즉시 해제

프론트 잡담 타이머 발화 시 (Mascot.svelte — 기존 타이머·게이트 유지):
  pool = getChatterPool()                    # 오늘 풀 pull (없으면/실패 시 빈 배열)
  b = pickChatter(pool, summary, 최근 3개, rand)     # bubble.ts 순수 함수 (정적 후보는 내부에서 summary로 렌더)
  showBubble(b)                              # kind 'chatter', 기존 표시/닫기 그대로
```

- 네트워크(LLM)는 daily-line·diary와 동일하게 **store 락 밖**(①read→drop ②network ③upsert 규율).
- 엔진 미설정 시 `maybe_generate_chatter_pool`은 no-op → 풀 빈 채로 유지 → 프론트는 정적만 pick(자연 폴백).
- "오늘" = 로컬 자정 기준 날짜(daily_line 정책 계승). 자정 넘김 시 어제 풀은 날짜 키로 자연 무시.
- fingerprint는 `mascot::facts_fingerprint(ctx)` 재사용(#1과 동일 필드 `세션수|입력|출력|findings수`).
  daily_line과 chatter_pool의 fp 게이트가 같은 값이라 스캔당 LLM 호출은 최대 2회(한마디 1 + 잡담 풀 1).

## 4. 파일 / 컴포넌트

- **`crates/core/src/mascot.rs`** (기존 파일에 추가 — #1 옆):
  - `pub fn build_chatter_prompt(ctx: &ChatContext, n: usize) -> String` — 동일 페르소나(1인칭·주인·능청)
    + `[오늘 요약]`·findings 사실 블록(#1과 동일 재료) + `voice_guidance()` + 정밀도의 선
    + 지시: "코칭 조언·보고가 아니라 **가벼운 잡담** N개. 한 줄에 하나씩, 각 40자 이내, 번호·불릿 없이."
  - `pub fn parse_chatter_lines(raw: &str, max_n: usize) -> Vec<String>` — 줄 분리, trim,
    방어적 선두 불릿/번호 제거(`- ` `• ` `* ` `1. ` 류), 각 줄 `strip_wrapping_quotes`(같은 모듈의
    기존 private fn 그대로 호출 — 가시성 변경 불필요), 빈 줄 제거, 최대 N개.
  - `pub fn compute_chatter_pool(engine, ctx, cached_fp) -> anyhow::Result<Option<(Vec<String>, String)>>`
    — store 무접근·engine만(MockEngine 테스트). 결정 순서: fp 동일→None / 0건→빈 풀 / 생성.
- **`crates/core/src/store.rs`**: `chatter_pool(date TEXT PRIMARY KEY, lines TEXT, fingerprint TEXT)`
  테이블(lines는 JSON 배열 문자열, `CREATE TABLE IF NOT EXISTS` 전례) +
  `get_chatter_pool(date) -> Option<(Vec<String>, String)>`, `upsert_chatter_pool(date, lines, fp)`.
- **`src-tauri/src/pipeline.rs`**: `maybe_generate_chatter_pool(store_mutex)` —
  `maybe_generate_daily_line` 직후 호출, 락 규율 동일. 실패는 `log::warn` 후 다음 스캔 재시도.
- **`src-tauri/src/commands.rs` / `lib.rs`**: `get_chatter_pool(state) -> Result<Vec<String>, String>`
  — 오늘 날짜 조회, 없으면 빈 벡터. invoke_handler 등록.
- **`src-tauri/src/tray.rs`**: "잡담" 서브메뉴(SubmenuBuilder) + CheckMenuItem 3개
  (자주=normal / 가끔=low / 안 함=off, 수동 라디오 — 클릭 시 store `chatter_level` set,
  세 항목 체크 상태 갱신, `settings:changed` emit). 초기 체크는 store 값(기본 low).
  muda 자동 토글 주의(실시간 조언 선례: store를 소스오브트루스로).
- **`src/lib/robot/bubble.ts`**: 정적 큐레이션 확장 — 기존 CHATTER 10개 유지 + 응원 결 5개 내외 추가
  (사실 주장 금지, 수치는 기존처럼 summary 템플릿만). 신설
  `pickChatter(pool: string[], summary, recent: string[], rand: () => number): Bubble` 순수 함수 —
  LLM 풀 + 정적 후보(summary 렌더 후)를 합쳐 최근 표시 3개 텍스트를 제외하고 균등 랜덤
  (가중치 없음). 제외 후 후보가 비면 recent 무시하고 전체에서 pick(기아 방지).
- **`src/lib/api.ts`**: `getChatterPool(): Promise<string[]>` 바인딩.
- **`src/Mascot.svelte`**: 잡담 타이머 콜백에서 `getSummary()`와 함께 `getChatterPool()` pull,
  `pickChatter`로 선택, 최근 표시 텍스트 3개를 세션-로컬 `$state`로 유지(영속화 안 함 —
  재시작 리셋 허용). 타이머 구조·게이트(레벨/새벽/말풍선 중 침묵)는 무변경.

## 5. 프롬프트 & 폴백 상세

- **페르소나/사실**: #1 `build_daily_line_prompt`와 동일 인물·동일 오늘 요약/findings 블록.
  지시만 다름 — "오늘 요약을 근거로 한 **가벼운 잡담·혼잣말 N개**. 요약에 없는 구체 수치 지어내기 금지.
  코칭 조언처럼 굴지 말 것(그건 다른 채널이 함). 한 줄에 하나, 각 40자 이내, 번호·불릿·따옴표 없이."
- **자연스러움**: `voice_guidance()` 주입(#1·다이어리와 동일 조각, 헤지 carve-out 계승).
- **파싱 내성**: 지시를 어겨 불릿/따옴표가 붙어도 `parse_chatter_lines`가 방어적으로 정제.
  5개 미만이면 있는 만큼 사용, 전부 실패(빈 벡터)면 빈 풀 캐시 → 정적 폴백.
- **활동 0건**: LLM 호출 없이 **빈 풀 + fp 캐시**(재-skip 보장). 프론트는 정적만 pick —
  #1의 정적 한마디("심심;;;")가 이미 idle 정서를 담당하므로 잡담 쪽 idle 전용 문구는 두지 않는다.
- **엔진 미설정**: no-op → 풀 빔 → 정적만. 에러 아님(chat·daily-line 관례 계승).

## 6. 검증 / 테스트 기준

- **`mascot.rs` 단위**: `build_chatter_prompt` 계약(주인·유저명·오늘 사실·voice_guidance 마커·
  "잡담"·개수 N·"40자"·"지어내지 마세요" 포함) / `parse_chatter_lines`(정상 N줄, 불릿·번호·따옴표 정제,
  빈 줄 제거, max_n 초과 절단, 전부 쓰레기→빈 벡터) / `compute_chatter_pool`(MockEngine: fp 동일→None,
  0건→빈 풀+엔진 미호출, 생성→파싱된 풀+fp).
- **store**: `chatter_pool` 라운드트립(upsert→get, JSON 직렬화 왕복) / 없으면 None.
- **command**: 오늘 캐시 있음→lines, 없음→빈 벡터.
- **프론트 vitest**: `pickChatter` — rand 주입 결정성, 최근 3개 회피, 빈 풀→정적만,
  전 후보 recent 시 기아 방지(전체 pick), 풀+정적 혼합 시 양쪽 모두 선택 가능.
- **게이트**: `cargo test -p agent-mentor`, `cargo build -p agent-mentor-app`(경고 0),
  `npm run build` + `npx vitest run`.
- **수동 E2E**(실엔진 `.env` localhost:4444): 스캔 후 `chatter_pool` 생성 육안(DB/로그),
  말풍선에 LLM 잡담·정적 혼합 표시, 트레이 서브메뉴 토글(자주/가끔/안 함) 동작·off 시 침묵,
  X 닫기·본문 클릭(홈 탭) 기존 동작 유지.

## 7. 스코프 · 후속

- **이번만**: #3 상주봇 잡담 업그레이드. 케이던스 값 변경, 미니홈피 설정 UI,
  realtime_advice 변경, 말풍선 UI/지속시간 변경, daily_line과의 텍스트 공유·통합,
  명언 큐레이션 DB화는 전부 스코프 밖.
- 정적 큐레이션 문구의 지속 확충·요일/시간대 결은 YAGNI — 이번엔 응원 결 추가 정도.
- 후속 후보: 풀 고갈 시(모두 recent) 스캔 외 재생성 트리거, 잡담 클릭 시 관련 탭 라우팅 다변화.

## 8. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- 엔진 URL은 네이티브에서 `localhost:4444`(`.env`), dev 빌드는 dotenvy 자동 로드.
- 진행: 이 스펙 승인 → writing-plans(`docs/plans/`) → SDD(태스크별 sonnet 구현+리뷰,
  최종 whole-branch opus) → finishing-a-development-branch(push + PR base=main).
- SDD 렛저: `.superpowers/sdd/progress.md` 최상단에 신규 섹션.
