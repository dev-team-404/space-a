# 프론트엔드 2단계(마스코트) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** mascot 창(픽셀 로봇 + 애니메이션 + 말풍선)과 트리거 4종을 배선 — 스펙 `docs/specs/2026-07-03-frontend-mascot-design.md`.

**Architecture:** 로봇은 파츠 상수(TS) → `buildRobotPixels` 순수 조립 → canvas 16×16 pixelated 8배. 상태 머신·말풍선 큐·텍스트 템플릿은 전부 순수 함수로 분리해 vitest로 게이트. 백엔드는 mascot 창 정의·occasion emit·`open_chat_tab` 커맨드·트레이 토글만 추가.

**Tech Stack:** 기존 스택 + vitest (프론트 단위 테스트).

## Global Constraints

- **Tauri v2 전용. v1 API 금지.** Windows 전용.
- **빌드 환경 (cargo 실행 전 Git Bash에서 매번):**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- 기존 테스트(core 97 + app 2) 무회귀, 경고 0. `src-tauri/src/lib.rs`의 `#[cfg(not(test))]` 게이트와 `cfg_attr(test, allow(...))` 유지.
- 이벤트명: 기존 `coach:finding`/`diary:ready`/`scan:done` 불변, 신규 `occasion:today`(payload `Vec<String>`)·`chat:goto-tab`(payload `String`).
- 파이프라인 에러는 UI 알림 금지, eprintln만.
- **픽셀 파츠 데이터는 창작 콘텐츠** — 플랜은 구조·불변식·레이아웃 가이드만 정의하고, 구체 좌표는 구현자가 작성하되 Task 1의 vitest 불변식(경계·개수·비어있지 않음·변형 간 상이)이 게이트다.
- 커밋 메시지 관례(`feat:`/`fix:` + 한국어), 각 태스크 끝 커밋.

---

### Task 1: vitest 도입 + 로봇 파츠·렌더 순수 코어

**Files:**
- Modify: `package.json` (vitest devDep + `"test": "vitest run"` 스크립트)
- Create: `src/lib/robot/parts.ts`, `src/lib/robot/render.ts`, `src/lib/robot/parts.test.ts`

**Interfaces:**
- Consumes: 없음
- Produces (Task 2·4가 사용):
  - `export type Px = readonly [number, number, number]` (x, y, 팔레트색인덱스 0..3)
  - `export const VARIANTS = { antenna: 6, head: 6, eyes: 6, body: 6, arms: 6, palette: 8 } as const`
  - `export const PALETTES: ReadonlyArray<readonly [string, string, string, string]>` (8종 × [body, accent, eye, outline] hex)
  - `export const PARTS: { antenna: Px[][]; head: Px[][]; eyes: Px[][]; body: Px[][]; arms: Px[][] }` (슬롯별 변형 배열)
  - `export const EYES_BLINK: Px[]`, `EYES_HAPPY: Px[]`, `EYES_SLEEP: Px[]` (표정 스왑용 공통 눈)
  - `export interface RobotSpec { antenna: number; head: number; eyes: number; body: number; arms: number; palette: number }` (Rust RobotSpec와 동일 형태)
  - `export function buildRobotPixels(spec: RobotSpec, eyesOverride?: Px[]): Px[]` (조립 순서 body→arms→head→eyes→antenna, 뒤가 앞을 덮음)
  - `export function drawRobot(ctx: CanvasRenderingContext2D, spec: RobotSpec, opts?: { eyesOverride?: Px[]; offsetY?: number }): void` (16×16 클리어 후 픽셀 칠하기; offsetY는 바운스용)

- [ ] **Step 1: vitest 설치·스크립트**

`package.json` devDependencies에 `"vitest": "^2.0.0"` 추가, scripts에 `"test": "vitest run"` 추가. Run: `npm install`

- [ ] **Step 2: 실패하는 테스트 작성** — `src/lib/robot/parts.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { EYES_BLINK, EYES_HAPPY, EYES_SLEEP, PALETTES, PARTS, VARIANTS } from './parts';
import { buildRobotPixels, type RobotSpec } from './render';

const SLOTS = ['antenna', 'head', 'eyes', 'body', 'arms'] as const;

describe('parts 불변식', () => {
  it('변형 개수가 Rust 상수와 일치한다', () => {
    for (const s of SLOTS) expect(PARTS[s]).toHaveLength(VARIANTS[s]);
    expect(PALETTES).toHaveLength(VARIANTS.palette);
  });

  it('모든 픽셀이 16×16 경계·색인덱스 0..3 안이다', () => {
    const all = [...SLOTS.flatMap((s) => PARTS[s].flat()), ...EYES_BLINK, ...EYES_HAPPY, ...EYES_SLEEP];
    for (const [x, y, c] of all) {
      expect(x).toBeGreaterThanOrEqual(0); expect(x).toBeLessThan(16);
      expect(y).toBeGreaterThanOrEqual(0); expect(y).toBeLessThan(16);
      expect(c).toBeGreaterThanOrEqual(0); expect(c).toBeLessThan(4);
    }
  });

  it('변형은 비어있지 않고 같은 슬롯 안에서 서로 다르다', () => {
    for (const s of SLOTS) {
      const seen = new Set<string>();
      for (const v of PARTS[s]) {
        expect(v.length).toBeGreaterThan(0);
        const key = JSON.stringify([...v].sort());
        expect(seen.has(key)).toBe(false);
        seen.add(key);
      }
    }
  });

  it('팔레트는 4색 hex다', () => {
    for (const p of PALETTES) {
      expect(p).toHaveLength(4);
      for (const c of p) expect(c).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe('buildRobotPixels', () => {
  const spec: RobotSpec = { antenna: 1, head: 2, eyes: 3, body: 4, arms: 5, palette: 6 };

  it('결정적이다', () => {
    expect(buildRobotPixels(spec)).toEqual(buildRobotPixels(spec));
  });

  it('eyesOverride가 기본 눈을 대체한다', () => {
    const a = buildRobotPixels(spec);
    const b = buildRobotPixels(spec, EYES_SLEEP);
    expect(a).not.toEqual(b);
  });

  it('스펙이 다르면 결과가 다르다 (샘플 페어)', () => {
    const other: RobotSpec = { antenna: 0, head: 0, eyes: 0, body: 0, arms: 0, palette: 0 };
    expect(buildRobotPixels(spec)).not.toEqual(buildRobotPixels(other));
  });
});
```

- [ ] **Step 3: 실패 확인**

Run: `npx vitest run 2>&1 | tail -5`
Expected: FAIL (parts.ts 모듈 없음)

- [ ] **Step 4: 구현**

`parts.ts` — 구조는 Interfaces대로. **픽셀 레이아웃 가이드**(16×16 그리드): antenna는 y 0–3, head는 y 3–8 (x 4–11), eyes는 y 5–7 (head 내부), body는 y 8–14 (x 3–12), arms는 y 9–13의 좌(x 1–3)·우(x 12–14) 대칭. 각 변형은 실루엣이 구분되게(예: antenna — 막대/더듬이2개/접시/고리/지그재그/없음에 가까운 스텁, body — 사각/둥근/역사다리/가슴LED/줄무늬/포켓). 색인덱스: 0=body색, 1=accent, 2=eye색, 3=outline. `EYES_BLINK`(감은 한 줄), `EYES_HAPPY`(^^), `EYES_SLEEP`(수평선+처짐)은 어느 head와도 겹치도록 y 5–7 안에서 작성. `PALETTES`는 레트로 톤 8종(파스텔 민트/살몬/라벤더/크림/스카이/로즈/세이지/그레이 계열, 각 4색).

`render.ts`:

```ts
import { EYES_BLINK, PALETTES, PARTS, type Px } from './parts';

export interface RobotSpec {
  antenna: number; head: number; eyes: number; body: number; arms: number; palette: number;
}

/** 조립: body→arms→head→eyes→antenna. 같은 좌표는 나중 파츠가 덮는다. */
export function buildRobotPixels(spec: RobotSpec, eyesOverride?: Px[]): Px[] {
  const layers: Px[][] = [
    PARTS.body[spec.body],
    PARTS.arms[spec.arms],
    PARTS.head[spec.head],
    eyesOverride ?? PARTS.eyes[spec.eyes],
    PARTS.antenna[spec.antenna],
  ];
  const grid = new Map<string, Px>();
  for (const layer of layers) for (const px of layer) grid.set(`${px[0]},${px[1]}`, px);
  return [...grid.values()].sort((a, b) => a[1] - b[1] || a[0] - b[0]);
}

export function drawRobot(
  ctx: CanvasRenderingContext2D,
  spec: RobotSpec,
  opts: { eyesOverride?: Px[]; offsetY?: number } = {},
): void {
  const palette = PALETTES[spec.palette];
  ctx.clearRect(0, 0, 16, 16);
  for (const [x, y, c] of buildRobotPixels(spec, opts.eyesOverride)) {
    ctx.fillStyle = palette[c];
    ctx.fillRect(x, y + (opts.offsetY ?? 0), 1, 1);
  }
}
```

- [ ] **Step 5: 통과 확인**

Run: `npx vitest run 2>&1 | tail -5`
Expected: 7 tests passed

- [ ] **Step 6: Commit**

```bash
git add package.json package-lock.json src/lib/robot
git commit -m "feat: 로봇 파츠 라이브러리 + buildRobotPixels — vitest 불변식 게이트"
```

---

### Task 2: 애니메이션 상태 머신 + 말풍선 큐·텍스트 (순수 로직)

**Files:**
- Create: `src/lib/robot/anim.ts`, `src/lib/robot/bubble.ts`, `src/lib/robot/anim.test.ts`, `src/lib/robot/bubble.test.ts`

**Interfaces:**
- Consumes: Task 1 `Px`, `EYES_BLINK/HAPPY/SLEEP`
- Produces (Task 4가 사용):
  - `export type MascotState = 'idle' | 'talk' | 'happy' | 'alert' | 'sleep'`
  - `export interface Frame { offsetY: number; eyesOverride?: Px[]; antennaBlink: boolean }`
  - `export function resolveState(input: { bubbleKind: BubbleKind | null; hour: number }): MascotState`
  - `export function frameAt(state: MascotState, tMs: number): Frame` (결정적 — 랜덤 없음, 깜빡임은 t 기반 의사주기)
  - `export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter'`
  - `export interface Bubble { kind: BubbleKind; text: string; tab: 'home' | 'diary' | 'coach' }`
  - `export class BubbleQueue { push(b: Bubble): void; next(): Bubble | null; readonly size: number }` (최대 5, 초과 시 oldest 드롭)
  - `export function bubbleText(kind, payload): Bubble` 팩토리들: `findingBubble(rows: {rule_id: string; est_tokens_saved: number; severity: string}[]): Bubble`, `diaryBubble(date: string): Bubble`, `occasionBubble(labels: string[]): Bubble`, `chatterBubble(pick: number, summary: {session_count: number} | null): Bubble`

- [ ] **Step 1: 실패하는 테스트 작성** — `anim.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { frameAt, resolveState } from './anim';

describe('resolveState', () => {
  it('말풍선이 최우선이다', () => {
    expect(resolveState({ bubbleKind: 'finding', hour: 3 })).toBe('alert');
    expect(resolveState({ bubbleKind: 'diary', hour: 3 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'occasion', hour: 12 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'chatter', hour: 12 })).toBe('talk');
  });
  it('01~07시 무풍선이면 sleep, 그 외 idle', () => {
    expect(resolveState({ bubbleKind: null, hour: 1 })).toBe('sleep');
    expect(resolveState({ bubbleKind: null, hour: 6 })).toBe('sleep');
    expect(resolveState({ bubbleKind: null, hour: 7 })).toBe('idle');
    expect(resolveState({ bubbleKind: null, hour: 12 })).toBe('idle');
  });
});

describe('frameAt', () => {
  it('결정적이다', () => {
    expect(frameAt('idle', 1234)).toEqual(frameAt('idle', 1234));
  });
  it('idle은 바운스가 -1..1 안에서 주기적으로 변한다', () => {
    const ys = [0, 400, 800, 1200].map((t) => frameAt('idle', t).offsetY);
    for (const y of ys) { expect(y).toBeGreaterThanOrEqual(-1); expect(y).toBeLessThanOrEqual(1); }
    expect(new Set(ys).size).toBeGreaterThan(1);
  });
  it('sleep은 눈 감김, alert는 안테나 점멸 토글', () => {
    expect(frameAt('sleep', 0).eyesOverride).toBeDefined();
    const a = frameAt('alert', 0).antennaBlink;
    const b = frameAt('alert', 300).antennaBlink;
    expect(a).not.toBe(b);
  });
});
```

`bubble.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { BubbleQueue, chatterBubble, diaryBubble, findingBubble, occasionBubble } from './bubble';

describe('BubbleQueue', () => {
  it('FIFO이고 최대 5건, 초과 시 oldest 드롭', () => {
    const q = new BubbleQueue();
    for (let i = 0; i < 7; i++) q.push(diaryBubble(`2026-07-0${i}`));
    expect(q.size).toBe(5);
    expect(q.next()!.text).toContain('2026-07-02'); // 0,1 드롭됨
  });
});

describe('bubble 팩토리', () => {
  it('finding: 최대 절약 1건 + 외 N건, coach 탭', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn' },
    ]);
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
  });
  it('diary는 diary 탭, occasion은 첫 라벨, chatter는 수치 삽입', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
    expect(chatterBubble(0, { session_count: 7 }).text).toContain('7');
    expect(chatterBubble(3, null).tab).toBe('home');
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run 2>&1 | tail -3` → FAIL (모듈 없음)

- [ ] **Step 3: 구현** — `anim.ts`:

```ts
import { EYES_BLINK, EYES_HAPPY, EYES_SLEEP, type Px } from './parts';
import type { BubbleKind } from './bubble';

export type MascotState = 'idle' | 'talk' | 'happy' | 'alert' | 'sleep';
export type { BubbleKind };

export interface Frame {
  offsetY: number;
  eyesOverride?: Px[];
  antennaBlink: boolean;
}

export function resolveState(input: { bubbleKind: BubbleKind | null; hour: number }): MascotState {
  switch (input.bubbleKind) {
    case 'finding': return 'alert';
    case 'diary':
    case 'occasion': return 'happy';
    case 'chatter': return 'talk';
    default: return input.hour >= 1 && input.hour < 7 ? 'sleep' : 'idle';
  }
}

/** 결정적 프레임 계산 — 렌더 루프가 t만 넘긴다. */
export function frameAt(state: MascotState, tMs: number): Frame {
  const bounce = Math.round(Math.sin((tMs / 1600) * Math.PI * 2)); // -1..1
  switch (state) {
    case 'sleep':
      return { offsetY: 0, eyesOverride: EYES_SLEEP, antennaBlink: false };
    case 'alert':
      return { offsetY: bounce, antennaBlink: Math.floor(tMs / 250) % 2 === 0 };
    case 'happy':
      return { offsetY: Math.floor(tMs / 200) % 2 === 0 ? -2 : 0, eyesOverride: EYES_HAPPY, antennaBlink: false };
    case 'talk':
      return { offsetY: bounce, eyesOverride: Math.floor(tMs / 350) % 2 === 0 ? undefined : EYES_BLINK, antennaBlink: false };
    default: {
      // idle: 4초 주기 중 200ms 깜빡임
      const blink = tMs % 4000 < 200;
      return { offsetY: bounce, eyesOverride: blink ? EYES_BLINK : undefined, antennaBlink: false };
    }
  }
}
```

(`talk`의 `eyesOverride: undefined` 분기는 exactOptionalPropertyTypes가 아니므로 허용 — 만약 tsc가 거부하면 조건부 spread로 조정하고 리포트에 명시.)

`bubble.ts`:

```ts
export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter';

export interface Bubble {
  kind: BubbleKind;
  text: string;
  tab: 'home' | 'diary' | 'coach';
}

const MAX_QUEUE = 5;

export class BubbleQueue {
  private items: Bubble[] = [];
  push(b: Bubble): void {
    this.items.push(b);
    if (this.items.length > MAX_QUEUE) this.items.shift();
  }
  next(): Bubble | null {
    return this.items.shift() ?? null;
  }
  get size(): number {
    return this.items.length;
  }
}

const RULE_LINE: Record<string, string> = {
  R1: '안 쓰는 MCP가 상주 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '같은 파일을 반복해서 읽고 있어요',
  R7: '단순 작업에 Opus는 과해요',
  R9: '웹 검색을 너무 많이 돌렸어요',
};

export function findingBubble(
  rows: { rule_id: string; est_tokens_saved: number; severity: string }[],
): Bubble {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  const line = RULE_LINE[top.rule_id] ?? '아낄 수 있는 게 보여요';
  const more = rows.length > 1 ? ` 외 ${rows.length - 1}건` : '';
  return {
    kind: 'finding',
    tab: 'coach',
    text: `주인, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`,
  };
}

export function diaryBubble(date: string): Bubble {
  return { kind: 'diary', tab: 'diary', text: `${date} 일기 다 썼어요! 보러 올래요?` };
}

export function occasionBubble(labels: string[]): Bubble {
  return { kind: 'occasion', tab: 'home', text: `오늘 ${labels[0]}이래요! 🎉` };
}

const CHATTER: ((n: number | null) => string)[] = [
  (n) => (n === null ? '오늘도 화이팅이에요, 주인!' : `오늘 벌써 ${n}세션이나 돌렸어요`),
  () => '토큰은 아끼라고 있는 거예요',
  () => '스킬로 만들면 편할 텐데…',
  () => '주인, 물 한 잔 마시고 해요',
  (n) => (n === null ? '심심해요…' : `${n}세션째… 저 좀 굴리는데요?`),
  () => '캐시 히트가 곧 절약이에요',
  () => '커밋은 자주, 후회는 짧게',
  () => '오늘 일기 기대해 주세요',
  () => '레지스트리에 새 스킬 구경 갈까요',
  () => 'zzz… 아 깨어있어요!',
];

export function chatterBubble(pick: number, summary: { session_count: number } | null): Bubble {
  const f = CHATTER[((pick % CHATTER.length) + CHATTER.length) % CHATTER.length];
  return { kind: 'chatter', tab: 'home', text: f(summary?.session_count ?? null) };
}
```

- [ ] **Step 4: 통과 확인** — `npx vitest run 2>&1 | tail -3` → 전체 통과 (Task 1의 7 + 신규 7)

- [ ] **Step 5: Commit**

```bash
git add src/lib/robot
git commit -m "feat: 마스코트 상태 머신·말풍선 큐·텍스트 템플릿 — 순수 로직 + vitest"
```

---

### Task 3: 백엔드 — mascot 창·occasion emit·open_chat_tab·트레이 토글

**Files:**
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/src/tray.rs`

**Interfaces:**
- Consumes: core `diary::occasions::compute_occasions(date, anchor, locale, include_dev_days)`, `diary::{resolve_locale, DiaryConfig}`, `store.earliest_session_ts()`, 기존 AppState/lock 패턴
- Produces:
  - 창 `mascot` (mascot.html, 160×160, transparent/undecorated/alwaysOnTop/skipTaskbar/resizable:false/visible:false)
  - 커맨드 `open_chat_tab(tab: String)` — 허용 탭 home/diary/coach/chat, chat 창 show/focus 후 `emit("chat:goto-tab", tab)`
  - 이벤트 `occasion:today` (payload `Vec<String>`)
  - `set_setting` allowlist에 `"mascot_pos"` 추가
  - 트레이 `마스코트 표시` CheckMenuItem (설정 `mascot_visible`, 기본 true)
  - 순수 함수 `pub fn should_notify_occasion(today: &str, last_notified: Option<&str>) -> bool` (pipeline.rs, 테스트 대상)

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/pipeline.rs` 테스트 모듈에 추가:

```rust
#[test]
fn occasion_notifies_once_per_day() {
    assert!(should_notify_occasion("2026-07-03", None));
    assert!(should_notify_occasion("2026-07-03", Some("2026-07-02")));
    assert!(!should_notify_occasion("2026-07-03", Some("2026-07-03")));
}
```

`src-tauri/src/commands.rs` 테스트 모듈에 추가:

```rust
#[test]
fn open_chat_tab_validates_tab() {
    assert!(valid_tab("home") && valid_tab("diary") && valid_tab("coach") && valid_tab("chat"));
    assert!(!valid_tab("etc") && !valid_tab(""));
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p agent-mentor-app 2>&1 | tail -3` → 컴파일 에러

- [ ] **Step 3: 구현**

`tauri.conf.json` `app.windows`에 추가:

```json
{
  "label": "mascot",
  "url": "mascot.html",
  "title": "mentor",
  "width": 160,
  "height": 160,
  "transparent": true,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "resizable": false,
  "shadow": false,
  "visible": false
}
```

`capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "chat·mascot 창 기본 권한",
  "windows": ["chat", "mascot"],
  "permissions": [
    "core:default",
    "core:window:allow-set-size",
    "core:window:allow-set-position",
    "core:window:allow-outer-position",
    "core:window:allow-start-dragging"
  ]
}
```

`commands.rs` — 검증을 순수 함수로 분리:

```rust
pub(crate) fn valid_tab(tab: &str) -> bool {
    matches!(tab, "home" | "diary" | "coach" | "chat")
}

#[tauri::command]
pub fn open_chat_tab(app: tauri::AppHandle, tab: String) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    if !valid_tab(&tab) {
        return Err(format!("허용되지 않은 탭: {tab}"));
    }
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit("chat:goto-tab", &tab).map_err(|e| e.to_string())
}
```

`set_setting`의 `ALLOWED`에 `"mascot_pos"` 추가 (4개가 됨).

`pipeline.rs` — 순수 함수 + 스캔 성공 블록 끝(다이어리 처리와 같은 자리, guard drop 이후 새 lock 스코프)에 occasion 처리:

```rust
/// 오늘 occasions를 하루 1회만 알린다.
pub fn should_notify_occasion(today: &str, last_notified: Option<&str>) -> bool {
    last_notified != Some(today)
}
```

runtime 쪽 (maybe_generate_diaries 다음, 실패는 전부 eprintln — `?` 전파 금지):

```rust
fn maybe_notify_occasions(app: &tauri::AppHandle, store_mutex: &std::sync::Mutex<agent_mentor::store::SqliteStore>) {
    use agent_mentor::diary::occasions::compute_occasions;
    use agent_mentor::diary::{resolve_locale, DiaryConfig};
    use tauri::Emitter;

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let labels: Option<Vec<String>> = {
        let Ok(store) = store_mutex.lock() else { return };
        let last = store.get_setting("occasion_notified_date").ok().flatten();
        if !should_notify_occasion(&today, last.as_deref()) {
            return;
        }
        let Ok(date) = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d") else { return };
        let anchor = store
            .earliest_session_ts()
            .ok()
            .flatten()
            .and_then(|ts| {
                ts.get(..10)
                    .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            });
        let locale = resolve_locale(&DiaryConfig::default());
        let occ = compute_occasions(date, anchor, &locale, true);
        if occ.is_empty() {
            let _ = store.set_setting("occasion_notified_date", &today); // 빈 날도 재계산 방지
            None
        } else {
            let _ = store.set_setting("occasion_notified_date", &today);
            Some(occ.into_iter().map(|o| o.label).collect())
        }
    };
    if let Some(labels) = labels {
        if let Err(e) = app.emit("occasion:today", &labels) {
            eprintln!("warn: occasion emit 실패: {e}");
        }
    }
}
```

`run_pipeline_once`에서 `maybe_generate_diaries(...)` 호출 다음 줄에 `maybe_notify_occasions(app, &state.store);` 추가.

`lib.rs` setup — 파이프라인 기동 다음에 mascot 표시·위치 복원:

```rust
            // mascot 창: 설정 보고 표시 + 위치 복원
            {
                use tauri::Manager;
                let state = app.state::<AppState>();
                let (visible, pos) = {
                    let store = state.store.lock().map_err(|_| anyhow::anyhow!("store lock"))?;
                    (
                        store.get_setting("mascot_visible")?.map(|v| v == "true").unwrap_or(true),
                        store.get_setting("mascot_pos")?,
                    )
                };
                if let Some(w) = app.get_webview_window("mascot") {
                    if let Some(p) = pos {
                        if let Some((x, y)) = p.split_once(',') {
                            if let (Ok(x), Ok(y)) = (x.parse::<i32>(), y.parse::<i32>()) {
                                let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                            }
                        }
                    }
                    if visible {
                        let _ = w.show();
                    }
                }
            }
```

(setup 클로저 반환이 `Box<dyn Error>` 계열이므로 `anyhow!` 대신 `?`가 안 맞으면 `.map_err(|e| e.to_string())?` 등 컴파일되는 형태로 조정 — 조정 시 리포트에 명시.)

`tray.rs` — `마스코트 표시` CheckMenuItem (open 다음, scan 앞):

```rust
    let mascot_on = {
        let state = app.state::<AppState>();
        let guard = state.store.lock().ok();
        guard
            .and_then(|s| s.get_setting("mascot_visible").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(true)
    };
    let mascot = CheckMenuItem::with_id(app, "mascot", "마스코트 표시", true, mascot_on, None::<&str>)?;
```

메뉴 배열에 `&mascot` 삽입, 핸들러에:

```rust
            "mascot" => {
                use tauri::Manager;
                let visible = app
                    .get_webview_window("mascot")
                    .map(|w| w.is_visible().unwrap_or(false))
                    .unwrap_or(false);
                if let Some(w) = app.get_webview_window("mascot") {
                    let _ = if visible { w.hide() } else { w.show() };
                }
                if let Ok(store) = app.state::<AppState>().store.lock() {
                    let _ = store.set_setting("mascot_visible", if visible { "false" } else { "true" });
                }
            }
```

`invoke_handler`에 `commands::open_chat_tab` 등록.

- [ ] **Step 4: 통과·회귀 확인**

Run: `cargo test 2>&1 | grep -E "test result"` → core 97 + app 4 (기존 2 + 신규 2)
Run: `cargo test 2>&1 | grep -cE "^warning"` → 0
Run: `cargo build -p agent-mentor-app 2>&1 | tail -3` → 성공

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: mascot 창·occasion:today emit·open_chat_tab·트레이 마스코트 토글"
```

---

### Task 4: Mascot.svelte — 캔버스·말풍선·드래그·트리거 배선

**Files:**
- Create: `src/mascot.html`, `src/mascot.ts`, `src/Mascot.svelte`
- Modify: `vite.config.ts` (input에 mascot 추가), `src/lib/api.ts` (이벤트·커맨드 래퍼 추가)

**Interfaces:**
- Consumes: Task 1·2 전부, Task 3 커맨드·이벤트, 기존 `get_mascot_seed`/`get_summary`/`get_settings`/`set_setting`
- Produces: 동작하는 mascot 창

- [ ] **Step 1: api.ts에 추가**

```ts
import type { RobotSpec } from './robot/render';

export const getMascotSeed = () => invoke<RobotSpec>('get_mascot_seed');
export const openChatTab = (tab: string) => invoke<void>('open_chat_tab', { tab });
export const getSettings = () => invoke<Record<string, string>>('get_settings');
export const setSetting = (key: string, value: string) => invoke<void>('set_setting', { key, value });

export const onDiaryReady = (cb: (date: string) => void): Promise<UnlistenFn> =>
  listen<string>('diary:ready', (e) => cb(e.payload));
export const onOccasionToday = (cb: (labels: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>('occasion:today', (e) => cb(e.payload));
export const onGotoTab = (cb: (tab: string) => void): Promise<UnlistenFn> =>
  listen<string>('chat:goto-tab', (e) => cb(e.payload));
```

- [ ] **Step 2: vite input에 mascot 추가**

```ts
      input: {
        chat: fileURLToPath(new URL('./src/chat.html', import.meta.url)),
        mascot: fileURLToPath(new URL('./src/mascot.html', import.meta.url)),
      },
```

`src/mascot.html`은 chat.html과 동일 구조(타이틀 "mentor", 스크립트 `/mascot.ts`, body 배경 투명). `src/mascot.ts`는 chat.ts와 동일하게 `Mascot.svelte` 마운트.

- [ ] **Step 3: Mascot.svelte 구현**

```svelte
<script lang="ts">
  import { getCurrentWindow, PhysicalPosition, PhysicalSize } from '@tauri-apps/api/window';
  import {
    getMascotSeed, getSummary, getSettings, setSetting,
    onDiaryReady, onNewFindings, onOccasionToday, openChatTab,
  } from './lib/api';
  import { drawRobot, type RobotSpec } from './lib/robot/render';
  import { frameAt, resolveState, type BubbleKind } from './lib/robot/anim';
  import { BubbleQueue, chatterBubble, diaryBubble, findingBubble, occasionBubble, type Bubble } from './lib/robot/bubble';

  const win = getCurrentWindow();
  const BASE = { w: 160, h: 160 };
  const EXPANDED = { w: 320, h: 230 };

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let bubble = $state<Bubble | null>(null);
  let chatterLevel = $state('low');

  const queue = new BubbleQueue();
  let showing = false;

  async function pump() {
    if (showing) return;
    const b = queue.next();
    if (!b) return;
    showing = true;
    bubble = b;
    await expand(true);
    setTimeout(async () => {
      bubble = null;
      await expand(false);
      showing = false;
      setTimeout(pump, 500);
    }, 6000);
  }

  async function expand(on: boolean) {
    // 캐릭터(창 우하단 고정)가 화면상 제자리를 지키도록 위치 보정
    const pos = await win.outerPosition();
    const dw = EXPANDED.w - BASE.w, dh = EXPANDED.h - BASE.h;
    if (on) {
      await win.setPosition(new PhysicalPosition(pos.x - dw, pos.y - dh));
      await win.setSize(new PhysicalSize(EXPANDED.w, EXPANDED.h));
    } else {
      await win.setSize(new PhysicalSize(BASE.w, BASE.h));
      await win.setPosition(new PhysicalPosition(pos.x + dw, pos.y + dh));
    }
  }

  function enqueue(b: Bubble) {
    queue.push(b);
    pump();
  }

  // 트리거 배선
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && enqueue(findingBubble(rows))),
      onDiaryReady((date) => enqueue(diaryBubble(date))),
      onOccasionToday((labels) => labels.length && enqueue(occasionBubble(labels))),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  // 잡담 타이머
  $effect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const [min, max] = chatterLevel === 'normal' ? [20, 40] : [60, 120];
      const delayMin = min + Math.random() * (max - min);
      timer = setTimeout(async () => {
        const hour = new Date().getHours();
        if (chatterLevel !== 'off' && !(hour >= 1 && hour < 7) && !showing) {
          const summary = await getSummary().catch(() => null);
          enqueue(chatterBubble(Math.floor(Math.random() * 10), summary));
        }
        schedule();
      }, delayMin * 60_000);
    };
    schedule();
    return () => clearTimeout(timer);
  });

  // 위치 저장 (이동 1초 디바운스)
  $effect(() => {
    let t: ReturnType<typeof setTimeout>;
    const p = win.onMoved(({ payload }) => {
      clearTimeout(t);
      t = setTimeout(() => setSetting('mascot_pos', `${payload.x},${payload.y}`).catch(() => {}), 1000);
    });
    return () => { clearTimeout(t); p.then((u) => u()); };
  });

  // 렌더 루프
  $effect(() => {
    if (!canvas || !spec) return;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    const loop = (t: number) => {
      const state = resolveState({ bubbleKind: (bubble?.kind ?? null) as BubbleKind | null, hour: new Date().getHours() });
      const f = frameAt(state, t);
      drawRobot(ctx, spec!, { eyesOverride: f.eyesOverride, offsetY: f.offsetY });
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  getMascotSeed().then((s) => (spec = s));
  getSettings().then((s) => (chatterLevel = s['chatter_level'] ?? 'low')).catch(() => {});
</script>

<div class="stage" class:expanded={bubble !== null}>
  {#if bubble}
    <button class="bubble" onclick={() => { openChatTab(bubble!.tab); }}>
      {bubble.text}
    </button>
  {/if}
  <div class="robot" data-tauri-drag-region>
    <canvas bind:this={canvas} width="16" height="16"></canvas>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; background: transparent; overflow: hidden; }
  .stage { width: 100vw; height: 100vh; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-end; }
  .robot { width: 128px; height: 128px; margin: 0 16px 16px 0; cursor: grab; }
  canvas { width: 128px; height: 128px; image-rendering: pixelated; }
  .bubble {
    max-width: 280px; margin: 8px 12px 0 0; padding: 8px 10px;
    background: #fffdf5; color: #33325a; border: 3px solid #33325a;
    box-shadow: 3px 3px 0 #33325a; font: 12px 'Galmuri11', 'DungGeunMo', monospace;
    cursor: pointer; text-align: left;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
</style>
```

(주: `data-tauri-drag-region`은 캔버스 wrapper에 — 말풍선 버튼은 클릭 가능해야 하므로 제외. canvas 자체가 drag region 안이라 클릭-드래그로 이동.)

- [ ] **Step 4: 빌드 게이트**

Run: `npm run build 2>&1 | tail -3` → mascot.html·chat.html 둘 다 dist에
Run: `npx vitest run 2>&1 | tail -3` → 전체 통과 유지

- [ ] **Step 5: Commit**

```bash
git add src vite.config.ts
git commit -m "feat: Mascot.svelte — 캔버스 로봇·말풍선 큐·드래그·트리거 4종 배선"
```

---

### Task 5: chat goto-tab 수신 + 최종 검증

**Files:**
- Modify: `src/App.svelte`

**Interfaces:**
- Consumes: Task 3 `chat:goto-tab` 이벤트, Task 4 `onGotoTab`

- [ ] **Step 1: App.svelte에 리스너 추가** (기존 onScanDone 근처)

```ts
  import { onGotoTab } from './lib/api';
  // ...
  onGotoTab((t) => {
    if (t === 'home' || t === 'diary' || t === 'coach' || t === 'chat') tab = t;
  });
```

- [ ] **Step 2: 전체 게이트**

Run: `cargo test 2>&1 | grep -E "test result"` → core 97 + app 4
Run: `cargo test 2>&1 | grep -cE "^warning"` → 0
Run: `npx vitest run 2>&1 | tail -3` → 통과
Run: `npm run build 2>&1 | tail -3` → 성공
Run: `cargo build -p agent-mentor-app 2>&1 | tail -3` → 성공

- [ ] **Step 3: Commit**

```bash
git add src/App.svelte
git commit -m "feat: chat 창 goto-tab 수신 — 말풍선 클릭 시 해당 탭 전환"
```

- [ ] **Step 4: 수동 E2E** — 스펙 §9 체크리스트 (사용자/컨트롤러 GUI 확인 대기)
