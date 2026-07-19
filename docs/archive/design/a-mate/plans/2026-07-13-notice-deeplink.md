---
status: done
archived: 2026-07-19
---

# 알림·말풍선 딥링크 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 알림(홈 NoticeLog)과 마스코트 말풍선 클릭 시 관련 위치(코칭 top 카드·다이어리 해당 날짜)로 바로 이동한다.

**Architecture:** 딥링크 = `{ tab, target? }` — target은 탭 문맥으로 해석하는 불투명 문자열(coach → dedup_key, diary → `YYYY-MM-DD`), 백엔드는 무해석 전달만. 말풍선 경로는 기존 `open_chat_tab` 커맨드/`chat:goto-tab` 이벤트를 객체 payload로 일반화, NoticeLog 경로는 같은 창 안이라 콜백(`gotoCoach` 선례와 대칭인 `gotoDiary` 신설). `Bubble.key`(사실상 미사용)를 `target`으로 rename, `Notice`에 `target?` 추가, DiaryTab에 `focusDate` prop 추가.

**Tech Stack:** Rust(serde) + Tauri v2 커맨드/이벤트, Svelte 5(runes), TypeScript, vitest.

**스펙:** `docs/specs/2026-07-12-notice-deeplink-design.md` (실행 전 일독 권장 — 결정 배경 포함)

## Global Constraints

- 브랜치 `feat/notice-deeplink` (생성됨, 스펙 커밋 포함). **main 직접 커밋 금지.** 커밋은 태스크 단위.
- 매 cargo 명령 전 (Git Bash):
  `export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`
- push revocation 에러 시: `git -c http.schannelCheckRevoke=false push`
- 커맨드명 `open_chat_tab`·이벤트명 `chat:goto-tab` 유지(기존 계약). 와이어 payload만 문자열 → `{ tab, target? }` 객체로.
- target 의미: coach → finding dedup_key, diary → `YYYY-MM-DD`. **백엔드는 target을 해석·검증하지 않는다.**
- 알림 문구 3종 기존 유지: "코칭 지적 N건이 도착했어요" / "{date} 일기가 나왔어요" / "오늘은 {label}!"
- occasion/chatter는 딥링크 대상 아님(탭 이동만·NoticeLog 클릭 없음). 구버전 localStorage 알림(target 없음)은 마이그레이션 없이 클릭 불가 텍스트로.

---

### Task 1: 백엔드 — `open_chat_tab`에 target 옵션 + `GotoTabPayload`

**Files:**
- Modify: `src-tauri/src/commands.rs` (`valid_tab` 383행 인근 · `open_chat_tab` 388~401행 · 테스트는 같은 파일 `mod tests`의 `open_chat_tab_validates_tab` 아래)

**Interfaces:**
- Produces: 커맨드 `open_chat_tab(tab: String, target: Option<String>)` — 와이어 이벤트
  `chat:goto-tab` payload `{ tab: string, target?: string }` (target None이면 필드 생략).
  Task 2의 `invoke('open_chat_tab', { tab, target })`와 `listen('chat:goto-tab')`이 소비.
- 기존 프론트 호출 `invoke('open_chat_tab', { tab })`(target 미전달)도 유효 — Tauri v2 Option 파라미터는 누락 허용.

- [ ] **Step 1: 실패하는 테스트 작성** — `src-tauri/src/commands.rs`의 `mod tests`에서 기존 `open_chat_tab_validates_tab` 테스트 아래에 추가:

```rust
    #[test]
    fn goto_tab_payload_serializes_target_optionally() {
        let with = GotoTabPayload { tab: "diary".into(), target: Some("2026-07-11".into()) };
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            r#"{"tab":"diary","target":"2026-07-11"}"#
        );
        // None이면 target 필드 자체를 생략 — 프론트 `target?: string`(undefined)과 정합
        let without = GotoTabPayload { tab: "home".into(), target: None };
        assert_eq!(serde_json::to_string(&without).unwrap(), r#"{"tab":"home"}"#);
    }
```

- [ ] **Step 2: 실패 확인**

Run (Git Bash, env 세팅 후): `cargo test -p agent-mentor-app goto_tab`
Expected: FAIL — 컴파일 에러 `cannot find struct GotoTabPayload`

- [ ] **Step 3: 최소 구현** — `commands.rs`의 기존 `open_chat_tab`(388~401행, `#[cfg_attr(test, allow(dead_code))]` 포함)을 다음으로 교체. `GotoTabPayload`는 `valid_tab` 바로 아래에 둔다. 파일 상단에 이미 `use serde::Serialize;`가 있으므로 derive는 `Serialize`로 충분:

```rust
/// chat:goto-tab payload — target은 탭 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD).
/// 백엔드는 내용을 해석하지 않는다(스펙 §1-4).
#[derive(Debug, Clone, Serialize)]
pub struct GotoTabPayload {
    pub tab: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>, // None이면 필드 생략 — 프론트 `target?: string`과 정합
}

#[cfg_attr(test, allow(dead_code))]
#[tauri::command]
pub fn open_chat_tab(app: tauri::AppHandle, tab: String, target: Option<String>) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    if !valid_tab(&tab) {
        return Err(format!("허용되지 않은 탭: {tab}"));
    }
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit("chat:goto-tab", GotoTabPayload { tab, target })
        .map_err(|e| e.to_string())
}
```

(변경점: 함수 시그니처에 `target: Option<String>` 추가, 마지막 emit의 `&tab` → `GotoTabPayload { tab, target }`. `valid_tab` 검증·창 show/unminimize/focus는 무변경. Tauri v2 `Emitter::emit`은 `Serialize + Clone`을 요구하므로 `Clone` derive 필수.)

- [ ] **Step 4: 통과 확인 (신규 + 기존 회귀)**

Run: `cargo test -p agent-mentor-app`
Expected: PASS — `goto_tab_payload_serializes_target_optionally`, `open_chat_tab_validates_tab` 포함 전체 녹색

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): open_chat_tab에 target 옵션 — chat:goto-tab payload를 {tab, target?} 객체로"
```

---

### Task 2: 말풍선 발신 경로 — `Bubble.target` + api.ts + Mascot·App 어댑트

**Files:**
- Modify: `src/lib/robot/bubble.ts` (`Bubble` 3~8행 · `findingBubble` 18~29행 · `diaryBubble` 31~33행 · `adviceBubble` 57~61행)
- Modify: `src/lib/robot/bubble.test.ts`
- Modify: `src/lib/api.ts` (`openChatTab` 95행 · `onGotoTab` 128~129행)
- Modify: `src/Mascot.svelte` (37행 `openChatTab(b.tab)`)
- Modify: `src/App.svelte` (36~38행 `onGotoTab` 핸들러 — 객체 payload 어댑트만)

**Interfaces:**
- Consumes: Task 1의 커맨드 `open_chat_tab(tab, target?)` / 이벤트 payload `{ tab, target? }`
- Produces: `Bubble.target?: string` (기존 `key?` rename — 사용처는 adviceBubble과 테스트뿐,
  Mascot의 조언 반복 방지는 `top.dedup_key` 직접 비교라 무관).
  `api.ts`: `export interface GotoTabPayload { tab: string; target?: string }`,
  `openChatTab(tab: string, target?: string)`, `onGotoTab(cb: (p: GotoTabPayload) => void)`.
  Task 4의 App target 분기가 `GotoTabPayload`를 소비.
- 이 태스크 완료 시점에도 앱 동작 불변(탭 이동만) — target 착지는 Task 4.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/lib/robot/bubble.test.ts`의 `describe('bubble 팩토리')` 안 3개 테스트를 다음으로 교체(+1개 추가). finding rows에 `dedup_key` 필드가 늘어나고 `key` 단언이 `target`으로 바뀐다:

```ts
  it('finding: 최대 절약 1건 + 외 N건, coach 탭, top dedup_key가 target', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest', dedup_key: 'k-r5' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn', dedup_key: 'k-r1' },
    ]);
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
    expect(b.target).toBe('k-r1');
  });
  it('advice: detail을 싣고 dedup_key를 딥링크 target으로', () => {
    const b = adviceBubble({ dedup_key: 'k1', detail: '`playwright`가 상주하는데 호출 0회' });
    expect(b.kind).toBe('finding');
    expect(b.tab).toBe('coach');
    expect(b.target).toBe('k1');
    expect(b.text).toContain('playwright');
  });
  it('diary는 diary 탭 + 날짜가 target, occasion은 첫 라벨', () => {
    const d = diaryBubble('2026-07-02');
    expect(d.tab).toBe('diary');
    expect(d.target).toBe('2026-07-02');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
  });
  it('occasion·chatter는 target 없음(탭 이동만)', () => {
    expect(occasionBubble(['크리스마스']).target).toBeUndefined();
    expect(pickChatter(['풀A'], null, [], () => 0).target).toBeUndefined();
  });
```

- [ ] **Step 2: 실패 확인**

Run: `npx vitest run src/lib/robot/bubble.test.ts`
Expected: FAIL — `dedup_key` 초과 프로퍼티 및 `b.target` undefined (기존 `key` 필드라서)

- [ ] **Step 3: bubble.ts 구현** — 4곳 교체:

`Bubble` 인터페이스(3~8행):

```ts
export interface Bubble {
  kind: BubbleKind;
  text: string;
  tab: 'home' | 'diary' | 'coach';
  /** 딥링크 대상 — tab 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD). 없으면 탭 이동만. */
  target?: string;
}
```

`findingBubble`(18~29행) — 파라미터에 `dedup_key` 추가, 반환에 `target`:

```ts
export function findingBubble(
  rows: { rule_id: string; est_tokens_saved: number; severity: string; dedup_key: string }[],
): Bubble {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  const line = RULE_LINE[top.rule_id] ?? '아낄 수 있는 게 보여요';
  const more = rows.length > 1 ? ` 외 ${rows.length - 1}건` : '';
  return {
    kind: 'finding',
    tab: 'coach',
    target: top.dedup_key,
    text: `주인, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`,
  };
}
```

`diaryBubble`(31~33행):

```ts
export function diaryBubble(date: string): Bubble {
  return { kind: 'diary', tab: 'diary', target: date, text: `${date} 일기 다 썼어요! 보러 올래요?` };
}
```

`adviceBubble`(57~61행) — doc 주석까지 함께 교체:

```ts
/** realtime_advice 옵트인: 스캔 후 최상위 활성 advice를 말풍선으로 (스펙 §6).
 *  target(dedup_key)은 코칭 카드 딥링크 대상 — 같은 조언 반복 방지는 호출측이 dedup_key로 수행. */
export function adviceBubble(f: { dedup_key: string; detail: string }): Bubble {
  return { kind: 'finding', tab: 'coach', text: `주인, ${f.detail}`, target: f.dedup_key };
}
```

(Mascot의 `findingBubble(rows)` 호출은 rows가 `Finding[]`(dedup_key 보유)이라 무수정 호환.)

- [ ] **Step 4: 통과 확인**

Run: `npx vitest run src/lib/robot/bubble.test.ts`
Expected: PASS (bubble 팩토리 4건 + pickChatter 4건)

- [ ] **Step 5: api.ts 발신·수신 시그니처** — `openChatTab`(95행)을 교체:

```ts
export const openChatTab = (tab: string, target?: string) =>
  invoke<void>('open_chat_tab', { tab, target });
```

`onGotoTab`(128~129행)을 교체하고, 그 위 어디든 인터페이스 추가(`ScanProgress` 인터페이스 110~113행 인근 권장):

```ts
/** chat:goto-tab payload — target은 탭 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD) */
export interface GotoTabPayload {
  tab: string;
  target?: string;
}
```

```ts
export const onGotoTab = (cb: (p: GotoTabPayload) => void): Promise<UnlistenFn> =>
  listen<GotoTabPayload>('chat:goto-tab', (e) => cb(e.payload));
```

- [ ] **Step 6: Mascot.svelte 발신** — `closeBubble` 내 37행 한 줄 교체:

```ts
    if (openTab) openChatTab(b.tab, b.target);
```

- [ ] **Step 7: App.svelte 수신 어댑트** — 36~38행 `onGotoTab` 핸들러를 객체 payload 수신으로 교체(문자열 비교가 객체에 걸리면 탭 이동 전부 죽으므로 이 태스크에서 함께 수정. target 분기는 Task 4):

```ts
  onGotoTab(({ tab: t }) => {
    if (t === 'home' || t === 'diary' || t === 'coach' || t === 'chat') tab = t;
  });
```

- [ ] **Step 8: 회귀 확인**

Run: `npx vitest run`
Expected: 전체 PASS

Run: `npm run build`
Expected: 빌드 성공

- [ ] **Step 9: 커밋**

```bash
git add src/lib/robot/bubble.ts src/lib/robot/bubble.test.ts src/lib/api.ts src/Mascot.svelte src/App.svelte
git commit -m "feat(mascot): 말풍선 딥링크 발신 — Bubble.target(코칭 dedup_key·일기 날짜)을 goto payload로 전달"
```

---

### Task 3: notices 순수 헬퍼 — 팩토리 3종 + `noticeDest` + App `record()` 이관

**Files:**
- Modify: `src/lib/notices.ts`
- Modify: `src/lib/notices.test.ts`
- Modify: `src/App.svelte` (12행 import · 41~53행 record/$effect)

**Interfaces:**
- Produces (Task 4의 NoticeLog·App이 소비):

```ts
export interface Notice { ts: string; kind: 'finding' | 'diary' | 'occasion'; text: string; target?: string }
export interface NoticeDest { tab: 'coach' | 'diary'; target: string }
export function findingNotice(rows: { dedup_key: string; est_tokens_saved: number }[], ts: string): Notice
export function diaryNotice(date: string, ts: string): Notice
export function occasionNotice(labels: string[], ts: string): Notice
export function noticeDest(n: Notice): NoticeDest | null
```

- 기존 `pushNotice`/`loadNotices`/`saveNotices` 시그니처 불변.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/lib/notices.test.ts`의 기존 `describe('pushNotice')` 아래에 추가 (import도 확장):

import 행(2행)을 교체:

```ts
import { diaryNotice, findingNotice, noticeDest, occasionNotice, pushNotice, type Notice } from './notices';
```

파일 끝에 추가:

```ts
describe('notice 팩토리', () => {
  it('finding: N건 문구 + 절약량 1위 dedup_key가 target', () => {
    const n = findingNotice(
      [
        { dedup_key: 'k-low', est_tokens_saved: 10 },
        { dedup_key: 'k-top', est_tokens_saved: 500 },
      ],
      '2026-07-12T10:00:00Z',
    );
    expect(n.kind).toBe('finding');
    expect(n.text).toBe('코칭 지적 2건이 도착했어요');
    expect(n.target).toBe('k-top');
    expect(n.ts).toBe('2026-07-12T10:00:00Z');
  });
  it('diary: 날짜 문구 + 날짜가 target', () => {
    const n = diaryNotice('2026-07-11', '2026-07-12T07:00:00Z');
    expect(n.kind).toBe('diary');
    expect(n.text).toBe('2026-07-11 일기가 나왔어요');
    expect(n.target).toBe('2026-07-11');
  });
  it('occasion: 첫 라벨 문구, target 없음', () => {
    const n = occasionNotice(['크리스마스', '함께한 지 100일'], '2026-07-12T00:00:00Z');
    expect(n.kind).toBe('occasion');
    expect(n.text).toBe('오늘은 크리스마스!');
    expect(n.target).toBeUndefined();
  });
});

describe('noticeDest', () => {
  it('finding→coach, diary→diary로 매핑', () => {
    expect(noticeDest({ ts: 't', kind: 'finding', text: '', target: 'k1' })).toEqual({
      tab: 'coach',
      target: 'k1',
    });
    expect(noticeDest({ ts: 't', kind: 'diary', text: '', target: '2026-07-11' })).toEqual({
      tab: 'diary',
      target: '2026-07-11',
    });
  });
  it('target 없는 알림(occasion·구버전 저장분)은 null — 클릭 불가', () => {
    expect(noticeDest({ ts: 't', kind: 'occasion', text: '오늘은 X!' })).toBeNull();
    expect(noticeDest({ ts: 't', kind: 'finding', text: '구버전' })).toBeNull();
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `npx vitest run src/lib/notices.test.ts`
Expected: FAIL — `findingNotice` 등 export 없음

- [ ] **Step 3: notices.ts 구현** — `Notice` 인터페이스(1~5행)를 교체하고, 파일 끝에 헬퍼 4개 추가:

```ts
export interface Notice {
  ts: string;
  kind: 'finding' | 'diary' | 'occasion';
  text: string;
  /** 딥링크 대상 — finding이면 dedup_key, diary면 YYYY-MM-DD. 없으면 클릭 불가. */
  target?: string;
}
```

```ts
/** 알림 클릭 착지. target 없으면 null(클릭 불가) — 구버전 저장분·occasion이 자연 비활성. */
export interface NoticeDest {
  tab: 'coach' | 'diary';
  target: string;
}

/** finding 알림 — target은 절약량 1위 finding의 dedup_key (말풍선 findingBubble의 top과 동일 기준) */
export function findingNotice(
  rows: { dedup_key: string; est_tokens_saved: number }[],
  ts: string,
): Notice {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  return { ts, kind: 'finding', text: `코칭 지적 ${rows.length}건이 도착했어요`, target: top.dedup_key };
}

export function diaryNotice(date: string, ts: string): Notice {
  return { ts, kind: 'diary', text: `${date} 일기가 나왔어요`, target: date };
}

export function occasionNotice(labels: string[], ts: string): Notice {
  return { ts, kind: 'occasion', text: `오늘은 ${labels[0]}!` };
}

export function noticeDest(n: Notice): NoticeDest | null {
  if (!n.target) return null;
  if (n.kind === 'finding') return { tab: 'coach', target: n.target };
  if (n.kind === 'diary') return { tab: 'diary', target: n.target };
  return null;
}
```

- [ ] **Step 4: 통과 확인**

Run: `npx vitest run src/lib/notices.test.ts`
Expected: PASS (pushNotice 2건 + 팩토리 3건 + noticeDest 2건)

- [ ] **Step 5: App.svelte `record()` 이관** — 문구 조합을 헬퍼 호출로 교체. import(12행):

```ts
  import {
    diaryNotice, findingNotice, loadNotices, occasionNotice, pushNotice, saveNotices, type Notice,
  } from './lib/notices';
```

record 함수와 $effect(40~53행)를 교체:

```ts
  // 알림 히스토리 기록 (스펙 §2 — 창이 숨김이어도 수신됨). 문구·target 조합은 notices.ts 헬퍼.
  function record(n: Notice) {
    notices = pushNotice(notices, n);
    saveNotices(notices);
  }
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && record(findingNotice(rows, new Date().toISOString()))),
      onDiaryReady((date) => record(diaryNotice(date, new Date().toISOString()))),
      onOccasionToday((labels) => labels.length && record(occasionNotice(labels, new Date().toISOString()))),
      onDailyLine((text) => { dailyLine = text; }),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });
```

(`onNewFindings`의 rows는 `Finding[]` — `dedup_key`·`est_tokens_saved` 보유로 `findingNotice` 파라미터와 구조 호환.)

- [ ] **Step 6: 회귀 확인**

Run: `npx vitest run`
Expected: 전체 PASS

Run: `npm run build`
Expected: 빌드 성공

- [ ] **Step 7: 커밋**

```bash
git add src/lib/notices.ts src/lib/notices.test.ts src/App.svelte
git commit -m "feat(notices): 알림 팩토리·noticeDest 헬퍼 — target 저장, record 문구 조합 이관"
```

---

### Task 4: 착지 배선 — NoticeLog 클릭 + `gotoDiary` + DiaryTab `focusDate`

**Files:**
- Modify: `src/App.svelte` (onGotoTab 핸들러 · gotoCoach 인근 · 마크업 80~88행)
- Modify: `src/lib/ui/HomeTab.svelte` (props 13행 · NoticeLog 62행)
- Modify: `src/lib/ui/home/NoticeLog.svelte` (전면 개정 — 아래 전체 코드)
- Modify: `src/lib/ui/DiaryTab.svelte` (props·focus effect 추가)

**Interfaces:**
- Consumes: Task 3의 `noticeDest(n): NoticeDest | null`·`NoticeDest`, Task 2의 `GotoTabPayload`.
- Produces: `<DiaryTab focusDate={string | null} />`, `<NoticeLog {notices} onGoto={(dest: NoticeDest) => void} />`,
  `<HomeTab … onGotoNotice={(dest: NoticeDest) => void} />`. (컴포넌트 테스트 관례 없음 — vitest 회귀 + build + 육안으로 검증.)

- [ ] **Step 1: App.svelte 착지 함수·배선** — 4곳 수정.

`gotoCoach`(55~58행) 아래에 추가 (상태 `diaryFocus`는 27행 `notices` 선언 인근에):

```ts
  let diaryFocus = $state<string | null>(null);
```

```ts
  function gotoDiary(date: string) {
    diaryFocus = date;
    tab = 'diary';
  }

  // NoticeLog 클릭 착지 — dest.tab에 따라 코칭 카드/다이어리 날짜로
  function gotoDest(dest: NoticeDest) {
    if (dest.tab === 'coach') gotoCoach(dest.target);
    else gotoDiary(dest.target);
  }
```

import에 `NoticeDest` 타입 추가 (Task 3에서 교체한 import 행에 덧붙임):

```ts
  import {
    diaryNotice, findingNotice, loadNotices, occasionNotice, pushNotice, saveNotices,
    type Notice, type NoticeDest,
  } from './lib/notices';
```

`onGotoTab` 핸들러(Task 2 Step 7에서 어댑트한 것)를 target 분기로 확장:

```ts
  onGotoTab(({ tab: t, target }) => {
    if (!(t === 'home' || t === 'diary' || t === 'coach' || t === 'chat')) return;
    if (target && t === 'coach') gotoCoach(target);
    else if (target && t === 'diary') gotoDiary(target);
    else tab = t;
  });
```

마크업(80~88행)의 두 줄 교체:

```svelte
          <HomeTab {summary} onGotoCoach={gotoCoach} onGotoNotice={gotoDest} />
```

```svelte
          <DiaryTab focusDate={diaryFocus} />
```

- [ ] **Step 2: HomeTab.svelte 콜백 전달** — props(13행) 교체:

```ts
  import type { NoticeDest } from '../notices';

  let { summary, onGotoCoach, onGotoNotice }: {
    summary: Summary | null;
    onGotoCoach: (k: string) => void;
    onGotoNotice: (dest: NoticeDest) => void;
  } = $props();
```

(import는 기존 `import { loadNotices, type Notice } from '../notices';` 행에 `type NoticeDest`를 합쳐도 됨.)

마크업(62행) 교체:

```svelte
    <NoticeLog {notices} onGoto={onGotoNotice} />
```

- [ ] **Step 3: NoticeLog.svelte 클릭 가능 항목** — 파일 전체를 다음으로 교체:

```svelte
<script lang="ts">
  import { noticeDest, type Notice, type NoticeDest } from '../../notices';
  let { notices, onGoto }: { notices: Notice[]; onGoto: (dest: NoticeDest) => void } = $props();
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉' };
  const hhmm = (ts: string) => {
    const d = new Date(ts);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };
</script>

<div class="widget">
  <h3>최근 알림</h3>
  {#if notices.length === 0}
    <p class="empty">아직 알림이 없어요</p>
  {:else}
    <ul>
      {#each notices.slice(0, 6) as n (n.ts + n.text)}
        {@const dest = noticeDest(n)}
        <li>
          <span>{ICON[n.kind]}</span>
          {#if dest}
            <button class="text" onclick={() => onGoto(dest)}>{n.text}</button>
          {:else}
            <span class="text">{n.text}</span>
          {/if}
          <time>{hhmm(n.ts)}</time>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; font-size: 12px; }
  li { display: flex; gap: 6px; align-items: baseline; }
  .text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button.text {
    border: none; background: none; font: inherit; color: inherit;
    padding: 0; cursor: pointer; text-align: left;
  }
  button.text:hover { color: var(--accent); text-decoration: underline; }
  time { color: var(--ink-soft); font-size: 10px; }
</style>
```

(변경점: `noticeDest`로 클릭 가능 여부 판별 — target 있는 항목만 button, 호버 시 accent 색+밑줄. 나머지 마크업·스타일 기존 유지.)

- [ ] **Step 4: DiaryTab.svelte `focusDate`** — script 상단(7행 `const now` 위)에 props 추가:

```ts
  let { focusDate = null }: { focusDate?: string | null } = $props();
```

`loadDates()`/`onDiaryReady` effect(16~23행) 아래에 focus effect 추가:

```ts
  // 딥링크: focusDate가 dates에 실리면(비동기 로드 대기) 그 달로 이동해 일기를 연다.
  // consumedFocus: 같은 값 1회만 적용 — dates 재로드(diary:ready)로 effect가 재실행돼도
  // 사용자가 넘겨보던 달에서 끌려오지 않게 함. 렌더에 안 쓰이므로 $state 아님.
  let consumedFocus: string | null = null;
  $effect(() => {
    if (!focusDate || focusDate === consumedFocus || !dates.has(focusDate)) return;
    consumedFocus = focusDate;
    year = +focusDate.slice(0, 4);
    month = +focusDate.slice(5, 7);
    pick(focusDate);
  });
```

(탭 재진입 시 컴포넌트 재마운트로 `consumedFocus`가 리셋되어 focusDate가 다시 적용되는 것은
CoachTab `focusKey`와 동일 거동으로 수용 — 스펙 §2g. focusDate 날짜에 일기가 없으면
`dates.has` 가드로 effect 미발동, 탭 이동까지만 — 스펙 §3.)

- [ ] **Step 5: 회귀 확인**

Run: `npx vitest run`
Expected: 전체 PASS

Run: `npm run build`
Expected: 빌드 성공 (svelte 컴파일 에러 0)

- [ ] **Step 6: 커밋**

```bash
git add src/App.svelte src/lib/ui/HomeTab.svelte src/lib/ui/home/NoticeLog.svelte src/lib/ui/DiaryTab.svelte
git commit -m "feat(home): 알림 클릭 딥링크 착지 — gotoDiary·DiaryTab focusDate·NoticeLog 클릭"
```

---

### Task 5: 전체 검증 + push + PR

**Files:** 없음 (검증·배포만)

- [ ] **Step 1: 백엔드 전체 테스트** (Git Bash, env 세팅 후)

Run: `cargo test -p agent-mentor -p agent-mentor-app`
Expected: 전체 PASS

- [ ] **Step 2: 앱 컴파일 확인**

Run: `cargo build -p agent-mentor-app`
Expected: 빌드 성공 (커맨드 시그니처 배선 확인)

- [ ] **Step 3: 프론트 최종 확인**

Run: `npx vitest run && npm run build`
Expected: 전체 PASS + 빌드 성공

- [ ] **Step 4: push + PR 생성**

```bash
git push -u origin feat/notice-deeplink   # revocation 에러 시: git -c http.schannelCheckRevoke=false push -u origin feat/notice-deeplink
gh pr create --base main --title "feat: 알림·말풍선 딥링크 — 코칭 카드·다이어리 날짜로 바로 이동" --body "..."
```

PR 본문: 스펙 링크(`docs/specs/2026-07-12-notice-deeplink-design.md`), 변경 요약
(goto payload `{tab, target?}` 일반화 / Bubble.target·Notice.target / notices 헬퍼 이관 /
gotoDiary·DiaryTab focusDate / NoticeLog 클릭), 테스트 결과(vitest·cargo). 끝에
`🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

- [ ] **Step 5: 육안 판정 안내 (사용자)**

앱 실행 후:
1. 일기 말풍선("… 일기 다 썼어요!") 클릭 → 미니홈피 다이어리 탭, 해당 날짜의 달로 이동해 일기가 열림.
2. 홈 NoticeLog의 finding 알림 클릭 → 코칭 탭 top 카드로 스크롤·펼침.
3. 홈 NoticeLog의 diary 알림 클릭 → 다이어리 해당 날짜 열림.
4. occasion 알림·과거(구버전) 알림은 클릭 불가(일반 텍스트, 호버 반응 없음).
5. 마스코트 본체 클릭·occasion/잡담 말풍선 클릭 → 기존처럼 탭 이동만(회귀 확인).
