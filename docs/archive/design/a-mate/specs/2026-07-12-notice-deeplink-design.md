---
status: done
archived: 2026-07-19
---

# 알림·말풍선 딥링크 설계 스펙

- 작성: 2026-07-12 (브레인스토밍 산출물)
- 배경: 알림(홈 NoticeLog)과 마스코트 말풍선이 탭 이동까지만 지원 — NoticeLog는 클릭 자체가
  없고, 말풍선은 `openChatTab(tab)`으로 탭만 연다. 관련 위치(코칭 카드·다이어리 날짜)로
  바로 가는 경로가 없다. 백로그 ① (roadmap).
- 다음 단계: writing-plans → TDD 구현 → push+PR(base=main, 브랜치 `feat/notice-deeplink`)

## 1. 결정 사항 (브레인스토밍 논점 4건)

1. **대상 범위 = finding 카드 + diary 날짜**: finding → 코칭 탭 해당 카드 스크롤·펼침
   (`gotoCoach` 선례 재사용), diary → 다이어리 탭 해당 날짜 열기. occasion/chatter는
   현행 유지(탭 이동만), NoticeLog의 occasion 항목은 클릭 없음.
2. **NoticeLog 클릭 UX = target 있는 항목만**: target 있으면 버튼(호버 강조), 없으면 지금처럼
   일반 텍스트. 기존 localStorage 저장분(target 없음)은 자연 비활성 — 최대 20개 휘발성
   데이터라 마이그레이션 불필요.
3. **finding 착지 = top 카드**: "코칭 지적 N건" 알림·"…외 N건" 말풍선 클릭 시 절약량 1위
   (`est_tokens_saved` 최대) finding의 dedup_key로 포커스 — 말풍선 본문이 언급하는 항목과 일치.
4. **전달 방식 = 기존 경로 일반화**: `open_chat_tab`에 `target: Option<String>` 추가,
   `chat:goto-tab` payload를 문자열 → `{ tab, target? }` 객체로. 새 커맨드/이벤트 신설 기각
   (거의 같은 일을 하는 IPC 경로 중복). 백엔드는 target을 해석·검증하지 않고 전달만
   — 탭별 의미는 프론트 소관(coach → dedup_key, diary → `YYYY-MM-DD`).

## 2. 변경 상세

### 2a. 말풍선 — `src/lib/robot/bubble.ts`

- `Bubble.key?` → **`target?`으로 rename**. 사용처는 `adviceBubble`과 그 테스트뿐이고,
  Mascot의 조언 반복 방지는 `top.dedup_key` 직접 비교라 이 필드를 읽지 않는다(사실상 미사용).
- `findingBubble`: `target = top.dedup_key` (절약량 내림차순 정렬 기존 로직 그대로).
- `diaryBubble(date)`: `target = date`.
- `adviceBubble`: rename만 (`target = f.dedup_key`).
- `occasionBubble`/`pickChatter`: target 없음 — 무변경.

### 2b. 마스코트 — `src/Mascot.svelte`

- `closeBubble(true)`: `openChatTab(b.tab)` → `openChatTab(b.tab, b.target)` 한 줄.

### 2c. 프론트 API — `src/lib/api.ts`

```ts
export interface GotoTabPayload { tab: string; target?: string }
export const openChatTab = (tab: string, target?: string) =>
  invoke<void>('open_chat_tab', { tab, target });
export const onGotoTab = (cb: (p: GotoTabPayload) => void): Promise<UnlistenFn> =>
  listen<GotoTabPayload>('chat:goto-tab', (e) => cb(e.payload));
```

### 2d. 알림 — `src/lib/notices.ts`

- `Notice`에 `target?: string` 추가.
- 순수 헬퍼 신설 — App.svelte `record()`의 문구 조합을 이관해 vitest 가능하게.
  문구는 기존과 동일("코칭 지적 N건이 도착했어요" / "{date} 일기가 나왔어요" / "오늘은 {label}!"):

```ts
/** finding 알림 — target은 절약량 1위 finding의 dedup_key */
export function findingNotice(rows: { dedup_key: string; est_tokens_saved: number }[], ts: string): Notice
export function diaryNotice(date: string, ts: string): Notice        // target = date
export function occasionNotice(labels: string[], ts: string): Notice // target 없음
/** 클릭 착지 판별 — target 없으면 null(클릭 불가). finding→coach, diary→diary, occasion→null */
export function noticeDest(n: Notice): { tab: 'coach' | 'diary'; target: string } | null
```

### 2e. 홈 알림 위젯 — `src/lib/ui/home/NoticeLog.svelte`

- props에 `onGoto: (dest: { tab: 'coach' | 'diary'; target: string }) => void` 추가.
- `noticeDest(n)`이 truthy인 항목만 본문을 버튼으로 렌더(호버 강조), 클릭 시 `onGoto(dest)`.
  나머지는 기존 텍스트 그대로.

### 2f. 셸 — `src/App.svelte`

- `record()`의 문구 조합을 2d 헬퍼 호출로 교체(저장·상태 갱신은 그대로).
- `gotoDiary(date)` 신설: `diaryFocus = date; tab = 'diary'` (`gotoCoach`와 대칭, `diaryFocus` 상태 추가).
- `onGotoTab` 핸들러: `{ tab, target }` 객체 수신. 탭 유효성 검증 후 —
  target 있고 `tab === 'coach'` → `gotoCoach(target)`, target 있고 `tab === 'diary'` →
  `gotoDiary(target)`, 그 외 탭 전환만.
- 배선: `<DiaryTab focusDate={diaryFocus} />`, `<HomeTab … onGotoNotice={…} />` →
  NoticeLog `onGoto`로 전달. dest.tab에 따라 `gotoCoach`/`gotoDiary` 분기.

### 2g. 다이어리 탭 — `src/lib/ui/DiaryTab.svelte`

- props `{ focusDate?: string | null }` 추가.
- `$effect`: `focusDate`가 있고 `dates.has(focusDate)`(비동기 로드 대기)이고 미소비 상태면 —
  캘린더 연월을 focusDate의 연월로 이동 + `pick(focusDate)` + 소비 기록.
- 소비 가드(같은 값 1회만 적용): `dates` 재로드(diary:ready)로 effect가 재실행될 때
  사용자가 넘겨보던 달에서 끌려오지 않게 함. 탭 재진입(재마운트) 시 가드가 리셋되어
  focusDate가 다시 적용되는 것은 CoachTab `focusKey`와 동일 거동으로 수용.

### 2h. 백엔드 — `src-tauri/src/commands.rs`

```rust
#[derive(serde::Serialize, Clone)]
pub struct GotoTabPayload {
    pub tab: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>, // None이면 필드 생략 — 프론트 `target?: string`과 정합
}

#[tauri::command]
pub fn open_chat_tab(app: tauri::AppHandle, tab: String, target: Option<String>) -> Result<(), String>
```

- `valid_tab` 검증·창 show/focus 기존 유지, emit payload만 `GotoTabPayload { tab, target }`.
- 프론트에서 target 생략 시 `None` (Tauri v2 Option 파라미터는 누락 허용).

## 3. 엣지 처리

- target finding이 resolved/dismissed로 카드 부재 → CoachTab `focusKey` 기존 동작대로 무해하게 무시.
- diary target 날짜에 일기 없음(파일 삭제 등) → `dates.has` 가드로 effect 미발동, 탭 이동까지만.
- 구버전 알림(target 없음)·occasion 알림 → `noticeDest` null → 클릭 불가 텍스트.
- chat 창이 숨김/다른 탭 상태 → 기존 `open_chat_tab` 동작(show·unminimize·focus 후 이동) 그대로.

## 4. 검증 / 테스트 기준 (TDD)

- **front vitest**:
  - `bubble.test.ts`: findingBubble target=top dedup_key(복수 건 정렬 포함), diaryBubble
    target=date, adviceBubble rename 회귀, occasion/chatter target 없음.
  - `notices.test.ts`: 헬퍼 3종의 text·kind·target, `noticeDest` 매핑
    (finding/diary → dest, occasion·target 없는 구버전 → null), `pushNotice` 기존 회귀.
- **rust**: `GotoTabPayload` 직렬화(Some → `{"tab":…,"target":…}`, None → target 필드 생략), `valid_tab` 기존 유지.
- `npx vitest run` + `npm run build` + `cargo test`.
- **최종 판정**: 앱 실행 육안 — 일기 말풍선 클릭 → 다이어리 해당 날짜 열림, finding 알림
  클릭 → 코칭 top 카드 포커스, 구버전·occasion 알림은 클릭 불가.

## 5. 스코프 밖

occasion 딥링크, chatter 대상 부여, NoticeLog 항목 삭제·더보기·영속화 방식 변경(localStorage 유지),
외부 URL 스킴 딥링크, CoachTab focusKey 재마운트 거동 개선, 알림 클릭 후 읽음 처리.

## 6. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- push revocation 에러 시 `git -c http.schannelCheckRevoke=false push`.
- 커밋 태스크 단위, main 직접 커밋 금지.
