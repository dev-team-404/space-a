# Mascot 128px Procedural Render Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 32×32 pixel-push mascot renderer with a 128×128 shape-list renderer (ellipse/rrect/stroke primitives) that produces mini-sprite quality "싸이월드 미니미" aesthetics with cell shading.

**Architecture:** `parts.ts` exports constants and palette data (no Px arrays). `render.ts` contains a pure `buildRobotShapes()` function that returns a `Shape[]` list and a `drawRobot()` that executes those shapes on a canvas. `anim.ts` replaces `eyesOverride?: Px[]` with `expression: Expression` in its `Frame` type. `Mascot.svelte` calls `drawRobot(ctx, spec!, f)` where `f` is the new Frame.

**Tech Stack:** TypeScript, Svelte 5, Vitest 2, HTML Canvas 2D API (no third-party drawing libraries)

## Global Constraints

- Canvas is 128×128 pixels (`GRID = 128`). No 32×32 coordinate math.
- All shape coordinates must remain within `[0, 128]` (enforced by tests).
- `VARIANTS = { antenna: 6, head: 6, eyes: 6, body: 6, arms: 6, palette: 8 }` — must match Rust constants.
- `PALETTES` — 8 entries, each 8 hex strings — reuse verbatim from existing `parts.ts` (already correct).
- No Tauri v1 APIs. No `image-rendering: pixelated` is optional (keep or remove, spec says "your call").
- Do NOT touch `bubble.ts` or `bubble.test.ts`.
- Do NOT touch any Rust/backend files.
- Branch: `feat/frontend-mascot`. Current HEAD: `cc88576`.
- Commit message must be: `feat: 마스코트 128 프로시저럴 렌더 — 도형 정의+셀 셰이딩, 미니미 질감 (E2E 피드백 3차)` plus `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/lib/robot/parts.ts` | **Full rewrite** | `GRID`, `VARIANTS`, `PALETTES` constants. No Px arrays. |
| `src/lib/robot/render.ts` | **Full rewrite** | `Shape` types, `RobotSpec`, `Expression`, `Frame`, `buildRobotShapes()`, `drawRobot()` |
| `src/lib/robot/anim.ts` | **Modify** | `Frame.eyesOverride?: Px[]` → `Frame.expression: Expression`. Remove Px[] imports. |
| `src/Mascot.svelte` | **Modify** | Canvas 128×128, `drawRobot(ctx, spec!, f)` (no eyesOverride arg). |
| `src/lib/robot/parts.test.ts` | **Full rewrite** | Tests for GRID/VARIANTS/PALETTES and `buildRobotShapes` shape API. |
| `src/lib/robot/anim.test.ts` | **Modify** | Replace `eyesOverride` checks with `expression` checks. |

---

## Task 1: Write Failing Tests (RED phase)

**Files:**
- Rewrite: `src/lib/robot/parts.test.ts`
- Modify: `src/lib/robot/anim.test.ts`

**Interfaces:**
- Consumes: `GRID`, `PALETTES`, `VARIANTS` from `./parts`; `buildRobotShapes`, `RobotSpec`, `Frame` from `./render`
- Produces: A test suite that is RED because the new API doesn't exist yet

- [ ] **Step 1: Rewrite `src/lib/robot/parts.test.ts`**

Replace the entire file with:

```ts
import { describe, expect, it } from 'vitest';
import { GRID, PALETTES, VARIANTS } from './parts';
import { buildRobotShapes, type RobotSpec, type Frame } from './render';

const BASE_SPEC: RobotSpec = { antenna: 1, head: 2, eyes: 3, body: 4, arms: 5, palette: 6 };
const NORMAL_FRAME: Frame = { offsetY: 0, expression: 'normal', antennaBlink: false };

describe('VARIANTS', () => {
  it('슬롯별 변형 개수가 Rust 상수와 일치한다', () => {
    expect(VARIANTS.antenna).toBe(6);
    expect(VARIANTS.head).toBe(6);
    expect(VARIANTS.eyes).toBe(6);
    expect(VARIANTS.body).toBe(6);
    expect(VARIANTS.arms).toBe(6);
    expect(VARIANTS.palette).toBe(8);
  });
});

describe('PALETTES', () => {
  it('8개 팔레트, 각 8색 hex', () => {
    expect(PALETTES).toHaveLength(8);
    for (const p of PALETTES) {
      expect(p).toHaveLength(8);
      for (const c of p) expect(c).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe('buildRobotShapes', () => {
  it('GRID=128', () => {
    expect(GRID).toBe(128);
  });

  it('결정적이다 (same spec+frame → deep equal)', () => {
    const a = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    const b = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    expect(a).toEqual(b);
  });

  it('모든 도형 좌표가 0..128 경계 안이다', () => {
    for (let antenna = 0; antenna < 6; antenna++) {
      for (let head = 0; head < 6; head++) {
        const spec: RobotSpec = { antenna, head, eyes: 0, body: 0, arms: 0, palette: 0 };
        for (const s of buildRobotShapes(spec, NORMAL_FRAME)) {
          if (s.kind === 'ellipse' || s.kind === 'stroke-ellipse') {
            expect(s.cx - s.rx).toBeGreaterThanOrEqual(0);
            expect(s.cx + s.rx).toBeLessThanOrEqual(128);
            expect(s.cy - s.ry).toBeGreaterThanOrEqual(0);
            expect(s.cy + s.ry).toBeLessThanOrEqual(128);
          }
          if (s.kind === 'rrect' || s.kind === 'stroke-rrect') {
            expect(s.x).toBeGreaterThanOrEqual(0);
            expect(s.y).toBeGreaterThanOrEqual(0);
            expect(s.x + s.w).toBeLessThanOrEqual(128);
            expect(s.y + s.h).toBeLessThanOrEqual(128);
          }
          if (s.kind === 'line') {
            expect(s.x1).toBeGreaterThanOrEqual(0); expect(s.x1).toBeLessThanOrEqual(128);
            expect(s.x2).toBeGreaterThanOrEqual(0); expect(s.x2).toBeLessThanOrEqual(128);
            expect(s.y1).toBeGreaterThanOrEqual(0); expect(s.y1).toBeLessThanOrEqual(128);
            expect(s.y2).toBeGreaterThanOrEqual(0); expect(s.y2).toBeLessThanOrEqual(128);
          }
        }
      }
    }
  });

  it('슬롯 변형이 다르면 shapes가 다르다 (변형별 1쌍)', () => {
    const slots = ['antenna', 'head', 'eyes', 'body', 'arms'] as const;
    for (const slot of slots) {
      const spec0: RobotSpec = { antenna: 0, head: 0, eyes: 0, body: 0, arms: 0, palette: 0 };
      const spec1: RobotSpec = { ...spec0, [slot]: 1 };
      const a = buildRobotShapes(spec0, NORMAL_FRAME);
      const b = buildRobotShapes(spec1, NORMAL_FRAME);
      expect(a).not.toEqual(b);
    }
  });

  it('blink 표정은 normal과 다른 shapes를 만든다', () => {
    const normal = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, expression: 'normal' });
    const blink = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, expression: 'blink' });
    expect(normal).not.toEqual(blink);
  });

  it('sleep 표정은 normal과 다른 shapes를 만든다', () => {
    const normal = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, expression: 'normal' });
    const sleep = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, expression: 'sleep' });
    expect(normal).not.toEqual(sleep);
  });

  it('antennaBlink true/false는 다른 shapes를 만든다', () => {
    const off = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, antennaBlink: false });
    const on = buildRobotShapes(BASE_SPEC, { ...NORMAL_FRAME, antennaBlink: true });
    expect(off).not.toEqual(on);
  });

  it('모든 shape.color는 0..7 범위이다', () => {
    const shapes = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    for (const s of shapes) {
      expect(s.color).toBeGreaterThanOrEqual(0);
      expect(s.color).toBeLessThanOrEqual(7);
    }
  });

  it('shapes 밀도 게이트: 최소 25개 이상', () => {
    const shapes = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    expect(shapes.length).toBeGreaterThanOrEqual(25);
  });

  it('cheek 게이트: color===7 shape 최소 1개', () => {
    const shapes = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    expect(shapes.some((s) => s.color === 7)).toBe(true);
  });

  it('highlight 게이트: color===6 shape 최소 1개', () => {
    const shapes = buildRobotShapes(BASE_SPEC, NORMAL_FRAME);
    expect(shapes.some((s) => s.color === 6)).toBe(true);
  });
});
```

- [ ] **Step 2: Update `src/lib/robot/anim.test.ts`**

Replace lines 31–36 (the `sleep`/`alert` test) with expression-based checks:

```ts
  it('sleep은 expression=sleep, alert는 안테나 점멸 토글', () => {
    expect(frameAt('sleep', 0).expression).toBe('sleep');
    const a = frameAt('alert', 0).antennaBlink;
    const b = frameAt('alert', 300).antennaBlink;
    expect(a).not.toBe(b);
  });

  it('idle t<200 은 blink expression', () => {
    expect(frameAt('idle', 100).expression).toBe('blink');
  });

  it('happy expression', () => {
    expect(frameAt('happy', 0).expression).toBe('happy');
  });
```

Also remove `eyesOverride` from the import (it no longer exists). The full updated `anim.test.ts`:

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
  it("resolveState: scan 말풍선은 talk", () => {
    expect(resolveState({ bubbleKind: 'scan', hour: 12 })).toBe('talk');
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
  it('sleep은 expression=sleep, alert는 안테나 점멸 토글', () => {
    expect(frameAt('sleep', 0).expression).toBe('sleep');
    const a = frameAt('alert', 0).antennaBlink;
    const b = frameAt('alert', 300).antennaBlink;
    expect(a).not.toBe(b);
  });
  it('idle t<200 은 blink expression', () => {
    expect(frameAt('idle', 100).expression).toBe('blink');
  });
  it('happy expression', () => {
    expect(frameAt('happy', 0).expression).toBe('happy');
  });
});
```

- [ ] **Step 3: Run tests — verify RED**

```powershell
cd D:\Project\agent-mentor
npx vitest run 2>&1 | Select-String -Pattern "FAIL|PASS|Tests|Error" | Select-Object -First 20
```

Expected: Many failures referencing `expression`, `buildRobotShapes`, `Frame`. Tests referencing old `eyesOverride` from anim.test.ts should also fail (type errors at runtime since anim.ts still exports Px[]-based Frame). This is correct — RED.

---

## Task 2: Implement `parts.ts` (constants only)

**Files:**
- Rewrite: `src/lib/robot/parts.ts`

**Interfaces:**
- Consumes: nothing
- Produces: `GRID = 128`, `VARIANTS`, `PALETTES` (exported). No Px arrays — the old `PARTS`, `EYES_BLINK`, `EYES_HAPPY`, `EYES_SLEEP` exports are removed.

The shape parameter tables (variant dimensions) are implemented here as internal data that `render.ts` will import. Keep them as simple objects, not pixel arrays.

- [ ] **Step 1: Write `src/lib/robot/parts.ts`**

Replace the entire file with:

```ts
// parts.ts — Robot constants for 128×128 procedural render
// Color indices: 0=body, 1=bodyShade, 2=accent, 3=accentShade, 4=eye, 5=outline, 6=highlight, 7=cheek

export const GRID = 128;

export const VARIANTS = {
  antenna: 6,
  head: 6,
  eyes: 6,
  body: 6,
  arms: 6,
  palette: 8,
} as const;

// 8 palettes: [body, bodyShade, accent, accentShade, eye, outline, highlight, cheek]
export const PALETTES: ReadonlyArray<readonly [string, string, string, string, string, string, string, string]> = [
  ['#b2e8d8', '#7fc4ac', '#5ec4a8', '#3a9880', '#ffffff', '#1a3a4a', '#e8fffc', '#ffb8c8'], // 0: Mint
  ['#f7c5b0', '#d4987c', '#e88a6c', '#c06848', '#ffffff', '#3a1a10', '#fff5f0', '#ffb8c8'], // 1: Salmon
  ['#d4b8f0', '#a888d0', '#9b6fd4', '#7048b0', '#ffffff', '#2a1050', '#f8f0ff', '#ffb8c8'], // 2: Lavender
  ['#f5e8b0', '#d4c078', '#d4b840', '#a89020', '#ffffff', '#3a2800', '#fffff0', '#ffb8c8'], // 3: Cream
  ['#a8d8f0', '#78b0d4', '#4898d4', '#2870b0', '#ffffff', '#0a2840', '#f0f8ff', '#ffb8c8'], // 4: Sky
  ['#f0b8c8', '#d088a0', '#d46888', '#b04060', '#ffffff', '#40101c', '#fff0f4', '#ffb8c8'], // 5: Rose
  ['#b8d4b0', '#88b080', '#688c60', '#486840', '#ffffff', '#182810', '#f0fff0', '#ffb8c8'], // 6: Sage
  ['#c8d0d8', '#98a8b8', '#7890a0', '#507080', '#ffffff', '#182028', '#f4f8fc', '#ffb8c8'], // 7: Gray
];

// ─── Antenna variant parameters ───────────────────────────────────────────────
// Used by render.ts buildAntenna(). Each variant is a distinct silhouette.
// Coordinates are in 128×128 space.
export interface AntennaParams {
  kind: 'rod-ball' | 'forked' | 'dish' | 'halo' | 'lightning' | 'wing-ears';
  // rod tip ball center (rod-ball)
  ballCx?: number; ballCy?: number; ballR?: number;
  // stem: vertical line x1,y1 → x2,y2
  stemX?: number; stemY1?: number; stemY2?: number;
  // forked: two stalks
  leftTipCx?: number; leftTipCy?: number; rightTipCx?: number; rightTipCy?: number; tipR?: number;
  // dish: wide rrect
  dishX?: number; dishY?: number; dishW?: number; dishH?: number; dishR?: number;
  // halo: ring (stroke-ellipse)
  haloCx?: number; haloCy?: number; haloRx?: number; haloRy?: number;
  // lightning: line segments (flattened pairs)
  boltPoints?: number[];
  // wing-ears: two oval flaps
  leftEarCx?: number; leftEarCy?: number; rightEarCx?: number; rightEarCy?: number;
  earRx?: number; earRy?: number;
}

export const ANTENNA_PARAMS: AntennaParams[] = [
  // 0: rod + round ball
  { kind: 'rod-ball', ballCx: 64, ballCy: 4, ballR: 7, stemX: 64, stemY1: 11, stemY2: 22 },
  // 1: forked dual stalks
  { kind: 'forked', leftTipCx: 44, leftTipCy: 5, rightTipCx: 84, rightTipCy: 5, tipR: 6, stemX: 64, stemY1: 16, stemY2: 22 },
  // 2: satellite dish
  { kind: 'dish', dishX: 22, dishY: 2, dishW: 84, dishH: 12, dishR: 4, stemX: 64, stemY1: 14, stemY2: 22 },
  // 3: halo ring
  { kind: 'halo', haloCx: 64, haloCy: 8, haloRx: 22, haloRy: 8, stemX: 64, stemY1: 16, stemY2: 22 },
  // 4: lightning bolt (points: x1,y1,x2,y2,...)
  { kind: 'lightning', boltPoints: [74, 2, 64, 10, 70, 10, 54, 22], stemX: 64, stemY1: 18, stemY2: 22 },
  // 5: wing ears (two oval flaps)
  { kind: 'wing-ears', leftEarCx: 38, leftEarCy: 14, rightEarCx: 90, rightEarCy: 14, earRx: 14, earRy: 9, stemX: 64, stemY1: 18, stemY2: 22 },
];

// ─── Head variant parameters ──────────────────────────────────────────────────
export interface HeadParams {
  kind: 'oval' | 'square' | 'helmet' | 'cat-ears' | 'crt' | 'capsule';
  cx: number; cy: number; rx: number; ry: number; r: number; // bounding rrect corner radius
  // helmet visor stripe
  visorY?: number; visorH?: number;
  // cat ears: two triangles approximated as ellipses
  leftEarCx?: number; leftEarCy?: number; rightEarCx?: number; rightEarCy?: number; earRx?: number; earRy?: number;
}

export const HEAD_PARAMS: HeadParams[] = [
  // 0: round oval
  { kind: 'oval', cx: 64, cy: 40, rx: 38, ry: 28, r: 18 },
  // 1: square angular
  { kind: 'square', cx: 64, cy: 40, rx: 34, ry: 26, r: 4 },
  // 2: helmet with visor stripe across eyes
  { kind: 'helmet', cx: 64, cy: 40, rx: 34, ry: 26, r: 4, visorY: 36, visorH: 10 },
  // 3: cat ears (oval + two ear bumps)
  { kind: 'cat-ears', cx: 64, cy: 42, rx: 34, ry: 24, r: 6, leftEarCx: 36, leftEarCy: 20, rightEarCx: 92, rightEarCy: 20, earRx: 9, earRy: 12 },
  // 4: CRT monitor (wide, flatter)
  { kind: 'crt', cx: 64, cy: 40, rx: 46, ry: 24, r: 6 },
  // 5: capsule (tall pill)
  { kind: 'capsule', cx: 64, cy: 40, rx: 28, ry: 34, r: 28 },
];

// ─── Eyes variant parameters ──────────────────────────────────────────────────
// Eye areas in 128px space: left eye around (42,42), right eye around (86,42)
export interface EyesParams {
  kind: 'round' | 'led-bar' | 'star' | 'heart' | 'drowsy' | 'scanner';
  // round: circle + highlight dot
  eyeR?: number;        // radius of main eye circle
  // led-bar: horizontal rect
  barW?: number; barH?: number;
  // scanner: full-width sweep line
  scanY?: number;
}

export const EYES_PARAMS: EyesParams[] = [
  // 0: big round eyes
  { kind: 'round', eyeR: 8 },
  // 1: LED bar (horizontal rectangle)
  { kind: 'led-bar', barW: 14, barH: 6 },
  // 2: star shape (accent)
  { kind: 'star' },
  // 3: heart shape (accent)
  { kind: 'heart' },
  // 4: drowsy half-closed
  { kind: 'drowsy', eyeR: 7 },
  // 5: scanner bar (single horizontal sweep)
  { kind: 'scanner', scanY: 44 },
];

// Eye center positions (in 128px space, relative to head cx=64, cy=40)
export const EYE_LEFT_CX = 42;
export const EYE_LEFT_CY = 42;
export const EYE_RIGHT_CX = 86;
export const EYE_RIGHT_CY = 42;

// ─── Body variant parameters ──────────────────────────────────────────────────
export interface BodyParams {
  kind: 'canister' | 'box' | 'barrel' | 'vest' | 'pocketed' | 'striped';
  x: number; y: number; w: number; h: number; r: number; // bounding rrect
  // barrel: wider middle (midRx override)
  midRx?: number;
  // box: LED panel position
  ledX?: number; ledY?: number; ledW?: number; ledH?: number;
}

export const BODY_PARAMS: BodyParams[] = [
  // 0: round canister (oval body)
  { kind: 'canister', x: 34, y: 70, w: 60, h: 52, r: 20 },
  // 1: box with LED panel
  { kind: 'box', x: 26, y: 70, w: 76, h: 52, r: 6, ledX: 48, ledY: 82, ledW: 32, ledH: 16 },
  // 2: barrel (wider middle)
  { kind: 'barrel', x: 26, y: 70, w: 76, h: 52, r: 8, midRx: 46 },
  // 3: vest (lapel stripes)
  { kind: 'vest', x: 26, y: 70, w: 76, h: 52, r: 6 },
  // 4: pocketed
  { kind: 'pocketed', x: 26, y: 70, w: 76, h: 52, r: 6 },
  // 5: horizontal stripes
  { kind: 'striped', x: 26, y: 70, w: 76, h: 52, r: 6 },
];

// ─── Arms variant parameters ──────────────────────────────────────────────────
export interface ArmsParams {
  kind: 'hanging' | 'raised' | 'pincer' | 'stubby' | 'waving' | 'rocket-punch';
  // left arm bounding box
  lx: number; ly: number; lw: number; lh: number; lr: number;
  // right arm bounding box
  rx2: number; ry2: number; rw: number; rh: number; rr: number;
  // rocket punch: extension length
  extW?: number;
}

export const ARMS_PARAMS: ArmsParams[] = [
  // 0: hanging down
  { kind: 'hanging',      lx: 10, ly: 74, lw: 20, lh: 40, lr: 8, rx2: 98, ry2: 74, rw: 20, rh: 40, rr: 8 },
  // 1: raised up
  { kind: 'raised',       lx: 10, ly: 56, lw: 20, lh: 30, lr: 8, rx2: 98, ry2: 56, rw: 20, rh: 30, rr: 8 },
  // 2: pincer/claw
  { kind: 'pincer',       lx: 8,  ly: 74, lw: 20, lh: 38, lr: 6, rx2: 100, ry2: 74, rw: 20, rh: 38, rr: 6 },
  // 3: short stubby
  { kind: 'stubby',       lx: 12, ly: 78, lw: 16, lh: 24, lr: 8, rx2: 100, ry2: 78, rw: 16, rh: 24, rr: 8 },
  // 4: waving (asymmetric: left up, right down)
  { kind: 'waving',       lx: 10, ly: 60, lw: 20, lh: 28, lr: 8, rx2: 98, ry2: 82, rw: 20, rh: 32, rr: 8 },
  // 5: rocket punch (right extended)
  { kind: 'rocket-punch', lx: 10, ly: 74, lw: 20, lh: 34, lr: 8, rx2: 98, ry2: 76, rw: 20, rh: 26, rr: 8, extW: 28 },
];
```

- [ ] **Step 2: Run tests — still RED (render.ts not yet updated)**

```powershell
npx vitest run 2>&1 | Select-String -Pattern "FAIL|PASS|Tests" | Select-Object -First 10
```

Expected: parts.test.ts failing because `buildRobotShapes` / `Frame` still use old API in render.ts.

---

## Task 3: Implement `render.ts` (Shape types + buildRobotShapes + drawRobot)

**Files:**
- Rewrite: `src/lib/robot/render.ts`

**Interfaces:**
- Consumes: `GRID`, `PALETTES`, `ANTENNA_PARAMS`, `HEAD_PARAMS`, `EYES_PARAMS`, `BODY_PARAMS`, `ARMS_PARAMS`, `EYE_LEFT_CX`, `EYE_LEFT_CY`, `EYE_RIGHT_CX`, `EYE_RIGHT_CY` from `./parts`
- Produces: `RobotSpec`, `Expression`, `Frame`, `Shape`, `buildRobotShapes()`, `drawRobot()`

This is the largest task. Follow the layer order: shadow → body → arms → head → cheeks → eyes → antenna.

- [ ] **Step 1: Write `src/lib/robot/render.ts`**

Replace the entire file with:

```ts
import {
  GRID, PALETTES,
  ANTENNA_PARAMS, HEAD_PARAMS, EYES_PARAMS, BODY_PARAMS, ARMS_PARAMS,
  EYE_LEFT_CX, EYE_LEFT_CY, EYE_RIGHT_CX, EYE_RIGHT_CY,
} from './parts';

export interface RobotSpec { antenna: number; head: number; eyes: number; body: number; arms: number; palette: number; }
export type Expression = 'normal' | 'blink' | 'happy' | 'sleep';
export interface Frame { offsetY: number; expression: Expression; antennaBlink: boolean; }

export type Shape =
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number };

// ─── Floor shadow ─────────────────────────────────────────────────────────────
function buildShadow(): Shape[] {
  return [{ kind: 'ellipse', cx: 64, cy: 118, rx: 36, ry: 6, color: 5 }];
}

// ─── Body ─────────────────────────────────────────────────────────────────────
function buildBody(bodyIdx: number): Shape[] {
  const p = BODY_PARAMS[bodyIdx];
  const shapes: Shape[] = [];
  const { x, y, w, h, r } = p;

  // Base fill
  shapes.push({ kind: 'rrect', x, y, w, h, r, color: 0 });
  // Shade crescent (bottom-right offset ellipse)
  shapes.push({ kind: 'ellipse', cx: x + w - 16, cy: y + h - 14, rx: 18, ry: 14, color: 1 });
  // Outline
  shapes.push({ kind: 'stroke-rrect', x, y, w, h, r, color: 5, width: 2 });
  // Highlight spot (top-left)
  shapes.push({ kind: 'ellipse', cx: x + 12, cy: y + 12, rx: 8, ry: 6, color: 6 });

  // Variant-specific details
  if (p.kind === 'box' && p.ledX !== undefined) {
    // Chest LED panel border
    shapes.push({ kind: 'rrect', x: p.ledX!, y: p.ledY!, w: p.ledW!, h: p.ledH!, r: 2, color: 2 });
    // LED inner glow
    shapes.push({ kind: 'ellipse', cx: p.ledX! + p.ledW! / 2, cy: p.ledY! + p.ledH! / 2, rx: 6, ry: 4, color: 4 });
  }
  if (p.kind === 'barrel') {
    // Wider mid belly — second ellipse to bulge
    const mx = x - 8; const mw = w + 16;
    shapes.push({ kind: 'rrect', x: mx, y: y + 14, w: mw, h: h - 28, r: 18, color: 0 });
    shapes.push({ kind: 'stroke-rrect', x: mx, y: y + 14, w: mw, h: h - 28, r: 18, color: 5, width: 2 });
  }
  if (p.kind === 'vest') {
    // Left lapel
    shapes.push({ kind: 'rrect', x: x + 2, y: y + 4, w: 10, h: h - 16, r: 4, color: 2 });
    // Right lapel
    shapes.push({ kind: 'rrect', x: x + w - 12, y: y + 4, w: 10, h: h - 16, r: 4, color: 2 });
    // Lapel shade
    shapes.push({ kind: 'rrect', x: x + 2, y: y + 4 + (h - 16) / 2, w: 10, h: (h - 16) / 2, r: 4, color: 3 });
    shapes.push({ kind: 'rrect', x: x + w - 12, y: y + 4 + (h - 16) / 2, w: 10, h: (h - 16) / 2, r: 4, color: 3 });
  }
  if (p.kind === 'pocketed') {
    // Pocket outline bottom-left interior
    shapes.push({ kind: 'stroke-rrect', x: x + 8, y: y + h - 28, w: 22, h: 18, r: 3, color: 5, width: 1.5 });
  }
  if (p.kind === 'striped') {
    // 3 accent horizontal stripes
    shapes.push({ kind: 'rrect', x: x + 2, y: y + 10, w: w - 4, h: 6, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: x + 2, y: y + 22, w: w - 4, h: 6, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: x + 2, y: y + 34, w: w - 4, h: 6, r: 3, color: 3 });
  }

  return shapes;
}

// ─── Arms ─────────────────────────────────────────────────────────────────────
function buildArms(armsIdx: number): Shape[] {
  const p = ARMS_PARAMS[armsIdx];
  const shapes: Shape[] = [];

  function arm(x: number, y: number, w: number, h: number, r: number): void {
    shapes.push({ kind: 'rrect', x, y, w, h, r, color: 0 });
    shapes.push({ kind: 'ellipse', cx: x + w - 6, cy: y + h - 6, rx: 6, ry: 5, color: 1 });
    shapes.push({ kind: 'stroke-rrect', x, y, w, h, r, color: 5, width: 2 });
  }

  arm(p.lx, p.ly, p.lw, p.lh, p.lr);
  arm(p.rx2, p.ry2, p.rw, p.rh, p.rr);

  if (p.kind === 'pincer') {
    // Left claw fork
    shapes.push({ kind: 'ellipse', cx: p.lx + 6, cy: p.ly + p.lh + 4, rx: 5, ry: 4, color: 0 });
    shapes.push({ kind: 'ellipse', cx: p.lx + p.lw - 6, cy: p.ly + p.lh + 4, rx: 5, ry: 4, color: 0 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.lx + 6, cy: p.ly + p.lh + 4, rx: 5, ry: 4, color: 5, width: 1.5 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.lx + p.lw - 6, cy: p.ly + p.lh + 4, rx: 5, ry: 4, color: 5, width: 1.5 });
    // Right claw fork
    shapes.push({ kind: 'ellipse', cx: p.rx2 + 6, cy: p.ry2 + p.rh + 4, rx: 5, ry: 4, color: 0 });
    shapes.push({ kind: 'ellipse', cx: p.rx2 + p.rw - 6, cy: p.ry2 + p.rh + 4, rx: 5, ry: 4, color: 0 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.rx2 + 6, cy: p.ry2 + p.rh + 4, rx: 5, ry: 4, color: 5, width: 1.5 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.rx2 + p.rw - 6, cy: p.ry2 + p.rh + 4, rx: 5, ry: 4, color: 5, width: 1.5 });
  }
  if (p.kind === 'rocket-punch' && p.extW) {
    // Extended right arm trail in accent
    shapes.push({ kind: 'rrect', x: p.rx2 + p.rw, y: p.ry2 + 6, w: p.extW!, h: p.rh - 12, r: 4, color: 2 });
    shapes.push({ kind: 'ellipse', cx: p.rx2 + p.rw + p.extW! + 4, cy: p.ry2 + p.rh / 2, rx: 8, ry: 6, color: 3 });
  }

  return shapes;
}

// ─── Head ─────────────────────────────────────────────────────────────────────
function buildHead(headIdx: number): Shape[] {
  const p = HEAD_PARAMS[headIdx];
  const shapes: Shape[] = [];
  const { cx, cy, rx, ry, r } = p;
  const x = cx - rx; const y = cy - ry; const w = rx * 2; const h = ry * 2;

  // Base fill
  shapes.push({ kind: 'rrect', x, y, w, h, r, color: 0 });
  // Shade crescent bottom-right
  shapes.push({ kind: 'ellipse', cx: cx + rx * 0.4, cy: cy + ry * 0.4, rx: rx * 0.5, ry: ry * 0.5, color: 1 });
  // Outline
  shapes.push({ kind: 'stroke-rrect', x, y, w, h, r, color: 5, width: 2 });
  // Highlight spot top-left
  shapes.push({ kind: 'ellipse', cx: cx - rx * 0.5, cy: cy - ry * 0.5, rx: rx * 0.25, ry: ry * 0.2, color: 6 });

  if (p.kind === 'helmet' && p.visorY !== undefined) {
    // Visor stripe across eyes area (accent color band)
    shapes.push({ kind: 'rrect', x, y: p.visorY!, w, h: p.visorH!, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: x + 2, y: p.visorY! + 2, w: w - 4, h: (p.visorH! - 4) / 2, r: 2, color: 3 });
    shapes.push({ kind: 'stroke-rrect', x, y: p.visorY!, w, h: p.visorH!, r: 2, color: 5, width: 1 });
  }
  if (p.kind === 'cat-ears') {
    shapes.push({ kind: 'ellipse', cx: p.leftEarCx!, cy: p.leftEarCy!, rx: p.earRx!, ry: p.earRy!, color: 0 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.leftEarCx!, cy: p.leftEarCy!, rx: p.earRx!, ry: p.earRy!, color: 5, width: 2 });
    shapes.push({ kind: 'ellipse', cx: p.rightEarCx!, cy: p.rightEarCy!, rx: p.earRx!, ry: p.earRy!, color: 0 });
    shapes.push({ kind: 'stroke-ellipse', cx: p.rightEarCx!, cy: p.rightEarCy!, rx: p.earRx!, ry: p.earRy!, color: 5, width: 2 });
    // Inner ear accent
    shapes.push({ kind: 'ellipse', cx: p.leftEarCx!, cy: p.leftEarCy!, rx: p.earRx! - 4, ry: p.earRy! - 4, color: 2 });
    shapes.push({ kind: 'ellipse', cx: p.rightEarCx!, cy: p.rightEarCy!, rx: p.earRx! - 4, ry: p.earRy! - 4, color: 2 });
  }
  if (p.kind === 'crt') {
    // CRT bezel inner screen
    shapes.push({ kind: 'stroke-rrect', x: x + 6, y: y + 6, w: w - 12, h: h - 12, r: 4, color: 5, width: 1 });
  }

  return shapes;
}

// ─── Cheeks ───────────────────────────────────────────────────────────────────
function buildCheeks(headIdx: number): Shape[] {
  const p = HEAD_PARAMS[headIdx];
  const cy = p.cy + p.ry * 0.3;
  return [
    { kind: 'ellipse', cx: p.cx - p.rx * 0.55, cy, rx: 7, ry: 5, color: 7 },
    { kind: 'ellipse', cx: p.cx + p.rx * 0.55, cy, rx: 7, ry: 5, color: 7 },
  ];
}

// ─── Eyes ─────────────────────────────────────────────────────────────────────
function buildEyes(eyesIdx: number, expression: Expression, headIdx: number): Shape[] {
  const shapes: Shape[] = [];
  const hp = HEAD_PARAMS[headIdx];
  // Adjust eye positions based on head
  const lx = hp.cx - hp.rx * 0.55;
  const rx = hp.cx + hp.rx * 0.55;
  const ey = hp.cy - 2;

  if (expression === 'blink') {
    // Thin horizontal closed eyelids
    shapes.push({ kind: 'rrect', x: lx - 8, y: ey - 1, w: 16, h: 3, r: 1, color: 5 });
    shapes.push({ kind: 'rrect', x: rx - 8, y: ey - 1, w: 16, h: 3, r: 1, color: 5 });
    return shapes;
  }
  if (expression === 'happy') {
    // ^^ arc shapes (upward arcs drawn as stroke-ellipse clipped half)
    shapes.push({ kind: 'stroke-ellipse', cx: lx, cy: ey + 4, rx: 8, ry: 6, color: 5, width: 2.5 });
    shapes.push({ kind: 'stroke-ellipse', cx: rx, cy: ey + 4, rx: 8, ry: 6, color: 5, width: 2.5 });
    // Highlight on happy eyes
    shapes.push({ kind: 'ellipse', cx: lx - 2, cy: ey - 2, rx: 2, ry: 2, color: 4 });
    shapes.push({ kind: 'ellipse', cx: rx - 2, cy: ey - 2, rx: 2, ry: 2, color: 4 });
    return shapes;
  }
  if (expression === 'sleep') {
    // Half-closed droopy lids
    shapes.push({ kind: 'rrect', x: lx - 9, y: ey - 5, w: 18, h: 9, r: 4, color: 5 });
    shapes.push({ kind: 'ellipse', cx: lx, cy: ey + 2, rx: 7, ry: 4, color: 4 });
    shapes.push({ kind: 'rrect', x: rx - 9, y: ey - 5, w: 18, h: 9, r: 4, color: 5 });
    shapes.push({ kind: 'ellipse', cx: rx, cy: ey + 2, rx: 7, ry: 4, color: 4 });
    // zzz dots
    shapes.push({ kind: 'ellipse', cx: hp.cx + hp.rx * 0.7, cy: hp.cy - hp.ry * 0.5, rx: 3, ry: 3, color: 3 });
    shapes.push({ kind: 'ellipse', cx: hp.cx + hp.rx * 0.8, cy: hp.cy - hp.ry * 0.65, rx: 2, ry: 2, color: 3 });
    shapes.push({ kind: 'ellipse', cx: hp.cx + hp.rx * 0.9, cy: hp.cy - hp.ry * 0.8, rx: 2, ry: 2, color: 3 });
    return shapes;
  }

  // Normal expression: draw eyes based on variant
  const ep = EYES_PARAMS[eyesIdx];
  if (ep.kind === 'round') {
    const r = ep.eyeR!;
    shapes.push({ kind: 'ellipse', cx: lx, cy: ey, rx: r, ry: r, color: 4 });
    shapes.push({ kind: 'ellipse', cx: rx, cy: ey, rx: r, ry: r, color: 4 });
    shapes.push({ kind: 'stroke-ellipse', cx: lx, cy: ey, rx: r, ry: r, color: 5, width: 1.5 });
    shapes.push({ kind: 'stroke-ellipse', cx: rx, cy: ey, rx: r, ry: r, color: 5, width: 1.5 });
    // Highlight dot
    shapes.push({ kind: 'ellipse', cx: lx - 3, cy: ey - 3, rx: 2, ry: 2, color: 6 });
    shapes.push({ kind: 'ellipse', cx: rx - 3, cy: ey - 3, rx: 2, ry: 2, color: 6 });
  } else if (ep.kind === 'led-bar') {
    const bw = ep.barW!; const bh = ep.barH!;
    shapes.push({ kind: 'rrect', x: lx - bw / 2, y: ey - bh / 2, w: bw, h: bh, r: 2, color: 4 });
    shapes.push({ kind: 'rrect', x: rx - bw / 2, y: ey - bh / 2, w: bw, h: bh, r: 2, color: 4 });
    shapes.push({ kind: 'stroke-rrect', x: lx - bw / 2, y: ey - bh / 2, w: bw, h: bh, r: 2, color: 5, width: 1 });
    shapes.push({ kind: 'stroke-rrect', x: rx - bw / 2, y: ey - bh / 2, w: bw, h: bh, r: 2, color: 5, width: 1 });
  } else if (ep.kind === 'star') {
    // X-shape stars using 2 ellipses crossed
    shapes.push({ kind: 'rrect', x: lx - 8, y: ey - 2, w: 16, h: 5, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: lx - 2, y: ey - 8, w: 5, h: 16, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: rx - 8, y: ey - 2, w: 16, h: 5, r: 2, color: 2 });
    shapes.push({ kind: 'rrect', x: rx - 2, y: ey - 8, w: 5, h: 16, r: 2, color: 2 });
  } else if (ep.kind === 'heart') {
    // Heart shape: two small circles + triangle apex
    shapes.push({ kind: 'ellipse', cx: lx - 4, cy: ey - 3, rx: 5, ry: 5, color: 2 });
    shapes.push({ kind: 'ellipse', cx: lx + 4, cy: ey - 3, rx: 5, ry: 5, color: 2 });
    shapes.push({ kind: 'rrect', x: lx - 7, y: ey - 3, w: 14, h: 9, r: 2, color: 2 });
    shapes.push({ kind: 'ellipse', cx: lx, cy: ey + 6, rx: 3, ry: 2, color: 2 });
    shapes.push({ kind: 'ellipse', cx: rx - 4, cy: ey - 3, rx: 5, ry: 5, color: 2 });
    shapes.push({ kind: 'ellipse', cx: rx + 4, cy: ey - 3, rx: 5, ry: 5, color: 2 });
    shapes.push({ kind: 'rrect', x: rx - 7, y: ey - 3, w: 14, h: 9, r: 2, color: 2 });
    shapes.push({ kind: 'ellipse', cx: rx, cy: ey + 6, rx: 3, ry: 2, color: 2 });
  } else if (ep.kind === 'drowsy') {
    const r = ep.eyeR!;
    // Eyelid rrect covering top half
    shapes.push({ kind: 'ellipse', cx: lx, cy: ey, rx: r, ry: r, color: 4 });
    shapes.push({ kind: 'rrect', x: lx - r - 1, y: ey - r - 1, w: (r + 1) * 2, h: r + 2, r: 2, color: 5 });
    shapes.push({ kind: 'ellipse', cx: rx, cy: ey, rx: r, ry: r, color: 4 });
    shapes.push({ kind: 'rrect', x: rx - r - 1, y: ey - r - 1, w: (r + 1) * 2, h: r + 2, r: 2, color: 5 });
  } else if (ep.kind === 'scanner') {
    // Single full-width sweep line
    const scanY = ep.scanY ?? ey;
    shapes.push({ kind: 'rrect', x: hp.cx - hp.rx + 4, y: scanY - 3, w: hp.rx * 2 - 8, h: 6, r: 2, color: 2 });
    shapes.push({ kind: 'stroke-rrect', x: hp.cx - hp.rx + 4, y: scanY - 3, w: hp.rx * 2 - 8, h: 6, r: 2, color: 5, width: 1 });
  }

  return shapes;
}

// ─── Antenna ──────────────────────────────────────────────────────────────────
function buildAntenna(antennaIdx: number, antennaBlink: boolean): Shape[] {
  const ap = ANTENNA_PARAMS[antennaIdx];
  const shapes: Shape[] = [];
  const tipColor = antennaBlink ? 2 : 0; // accent when blinking, body otherwise

  // Stem (shared by most variants)
  if (ap.stemX !== undefined) {
    shapes.push({ kind: 'rrect', x: ap.stemX! - 3, y: ap.stemY1!, w: 6, h: ap.stemY2! - ap.stemY1!, r: 3, color: 0 });
    shapes.push({ kind: 'stroke-rrect', x: ap.stemX! - 3, y: ap.stemY1!, w: 6, h: ap.stemY2! - ap.stemY1!, r: 3, color: 5, width: 1.5 });
  }

  if (ap.kind === 'rod-ball') {
    shapes.push({ kind: 'ellipse', cx: ap.ballCx!, cy: ap.ballCy!, rx: ap.ballR!, ry: ap.ballR!, color: tipColor });
    shapes.push({ kind: 'stroke-ellipse', cx: ap.ballCx!, cy: ap.ballCy!, rx: ap.ballR!, ry: ap.ballR!, color: 5, width: 2 });
    if (antennaBlink) {
      shapes.push({ kind: 'ellipse', cx: ap.ballCx! - 3, cy: ap.ballCy! - 3, rx: 2, ry: 2, color: 6 });
    }
  } else if (ap.kind === 'forked') {
    shapes.push({ kind: 'ellipse', cx: ap.leftTipCx!, cy: ap.leftTipCy!, rx: ap.tipR!, ry: ap.tipR!, color: tipColor });
    shapes.push({ kind: 'stroke-ellipse', cx: ap.leftTipCx!, cy: ap.leftTipCy!, rx: ap.tipR!, ry: ap.tipR!, color: 5, width: 2 });
    shapes.push({ kind: 'ellipse', cx: ap.rightTipCx!, cy: ap.rightTipCy!, rx: ap.tipR!, ry: ap.tipR!, color: tipColor });
    shapes.push({ kind: 'stroke-ellipse', cx: ap.rightTipCx!, cy: ap.rightTipCy!, rx: ap.tipR!, ry: ap.tipR!, color: 5, width: 2 });
    // Left stalk
    shapes.push({ kind: 'line', x1: ap.stemX!, y1: ap.stemY1!, x2: ap.leftTipCx!, y2: ap.leftTipCy! + ap.tipR!, color: 5, width: 2 });
    shapes.push({ kind: 'line', x1: ap.stemX!, y1: ap.stemY1!, x2: ap.rightTipCx!, y2: ap.rightTipCy! + ap.tipR!, color: 5, width: 2 });
  } else if (ap.kind === 'dish') {
    shapes.push({ kind: 'rrect', x: ap.dishX!, y: ap.dishY!, w: ap.dishW!, h: ap.dishH!, r: ap.dishR!, color: tipColor });
    shapes.push({ kind: 'stroke-rrect', x: ap.dishX!, y: ap.dishY!, w: ap.dishW!, h: ap.dishH!, r: ap.dishR!, color: 5, width: 2 });
  } else if (ap.kind === 'halo') {
    shapes.push({ kind: 'stroke-ellipse', cx: ap.haloCx!, cy: ap.haloCy!, rx: ap.haloRx!, ry: ap.haloRy!, color: tipColor === 2 ? 2 : 2, width: 4 });
    // Halo highlight
    shapes.push({ kind: 'ellipse', cx: ap.haloCx! - ap.haloRx! * 0.5, cy: ap.haloCy!, rx: 5, ry: 4, color: antennaBlink ? 3 : 6 });
  } else if (ap.kind === 'lightning') {
    const pts = ap.boltPoints!;
    for (let i = 0; i + 3 < pts.length; i += 2) {
      shapes.push({ kind: 'line', x1: pts[i], y1: pts[i + 1], x2: pts[i + 2], y2: pts[i + 3], color: tipColor === 2 ? 2 : 3, width: 3 });
    }
    shapes.push({ kind: 'ellipse', cx: pts[0], cy: pts[1], rx: 4, ry: 4, color: tipColor });
  } else if (ap.kind === 'wing-ears') {
    shapes.push({ kind: 'ellipse', cx: ap.leftEarCx!, cy: ap.leftEarCy!, rx: ap.earRx!, ry: ap.earRy!, color: tipColor });
    shapes.push({ kind: 'stroke-ellipse', cx: ap.leftEarCx!, cy: ap.leftEarCy!, rx: ap.earRx!, ry: ap.earRy!, color: 5, width: 2 });
    shapes.push({ kind: 'ellipse', cx: ap.rightEarCx!, cy: ap.rightEarCy!, rx: ap.earRx!, ry: ap.earRy!, color: tipColor });
    shapes.push({ kind: 'stroke-ellipse', cx: ap.rightEarCx!, cy: ap.rightEarCy!, rx: ap.earRx!, ry: ap.earRy!, color: 5, width: 2 });
  }

  return shapes;
}

// ─── Public API ───────────────────────────────────────────────────────────────

/** Pure, deterministic shape list for given spec+frame. */
export function buildRobotShapes(spec: RobotSpec, frame: Frame): Shape[] {
  return [
    ...buildShadow(),
    ...buildBody(spec.body),
    ...buildArms(spec.arms),
    ...buildHead(spec.head),
    ...buildCheeks(spec.head),
    ...buildEyes(spec.eyes, frame.expression, spec.head),
    ...buildAntenna(spec.antenna, frame.antennaBlink),
  ];
}

function drawRoundedRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.lineTo(x + w - rr, y);
  ctx.arcTo(x + w, y, x + w, y + rr, rr);
  ctx.lineTo(x + w, y + h - rr);
  ctx.arcTo(x + w, y + h, x + w - rr, y + h, rr);
  ctx.lineTo(x + rr, y + h);
  ctx.arcTo(x, y + h, x, y + h - rr, rr);
  ctx.lineTo(x, y + rr);
  ctx.arcTo(x, y, x + rr, y, rr);
  ctx.closePath();
}

/** Clear canvas and draw all shapes. */
export function drawRobot(ctx: CanvasRenderingContext2D, spec: RobotSpec, frame: Frame): void {
  ctx.clearRect(0, 0, GRID, GRID);
  const palette = PALETTES[spec.palette];
  const shapes = buildRobotShapes(spec, frame);
  const oy = frame.offsetY;

  for (const s of shapes) {
    ctx.save();
    if (oy !== 0) ctx.translate(0, oy);

    if (s.kind === 'ellipse') {
      ctx.fillStyle = palette[s.color];
      ctx.beginPath();
      ctx.ellipse(s.cx, s.cy, s.rx, s.ry, 0, 0, Math.PI * 2);
      ctx.fill();
    } else if (s.kind === 'rrect') {
      ctx.fillStyle = palette[s.color];
      drawRoundedRect(ctx, s.x, s.y, s.w, s.h, s.r);
      ctx.fill();
    } else if (s.kind === 'stroke-rrect') {
      ctx.strokeStyle = palette[s.color];
      ctx.lineWidth = s.width;
      drawRoundedRect(ctx, s.x, s.y, s.w, s.h, s.r);
      ctx.stroke();
    } else if (s.kind === 'stroke-ellipse') {
      ctx.strokeStyle = palette[s.color];
      ctx.lineWidth = s.width;
      ctx.beginPath();
      ctx.ellipse(s.cx, s.cy, s.rx, s.ry, 0, 0, Math.PI * 2);
      ctx.stroke();
    } else if (s.kind === 'line') {
      ctx.strokeStyle = palette[s.color];
      ctx.lineWidth = s.width;
      ctx.lineCap = 'round';
      ctx.beginPath();
      ctx.moveTo(s.x1, s.y1);
      ctx.lineTo(s.x2, s.y2);
      ctx.stroke();
    }

    ctx.restore();
  }
}
```

- [ ] **Step 2: Run tests — watch for progress**

```powershell
npx vitest run 2>&1 | Select-String -Pattern "FAIL|PASS|Tests|✓|×" | Select-Object -First 30
```

Expected: parts.test.ts tests may be passing now; anim.test.ts still failing because anim.ts still exports old Frame type.

---

## Task 4: Update `anim.ts` (Frame.expression replaces eyesOverride)

**Files:**
- Modify: `src/lib/robot/anim.ts`

**Interfaces:**
- Consumes: `Expression` from `./render`
- Produces: Updated `Frame` type, updated `frameAt` that returns `expression: Expression` instead of `eyesOverride?: Px[]`

- [ ] **Step 1: Rewrite `src/lib/robot/anim.ts`**

Replace the entire file with:

```ts
import type { Expression } from './render';
import type { BubbleKind } from './bubble';

export type MascotState = 'idle' | 'talk' | 'happy' | 'alert' | 'sleep';
export type { BubbleKind };

export interface Frame {
  offsetY: number;
  expression: Expression;
  antennaBlink: boolean;
}

export function resolveState(input: { bubbleKind: BubbleKind | null; hour: number }): MascotState {
  switch (input.bubbleKind) {
    case 'finding': return 'alert';
    case 'diary':
    case 'occasion': return 'happy';
    case 'chatter':
    case 'scan': return 'talk';
    default: return input.hour >= 1 && input.hour < 7 ? 'sleep' : 'idle';
  }
}

/** Deterministic frame calculation — render loop passes t only. */
export function frameAt(state: MascotState, tMs: number): Frame {
  const bounce = Math.round(Math.sin((tMs / 1600) * Math.PI * 2)); // -1..1
  switch (state) {
    case 'sleep':
      return { offsetY: 0, expression: 'sleep', antennaBlink: false };
    case 'alert':
      return { offsetY: bounce, expression: 'normal', antennaBlink: Math.floor(tMs / 250) % 2 === 0 };
    case 'happy':
      return { offsetY: Math.floor(tMs / 200) % 2 === 0 ? -2 : 0, expression: 'happy', antennaBlink: false };
    case 'talk': {
      const expression: Expression = Math.floor(tMs / 350) % 2 === 0 ? 'normal' : 'blink';
      return { offsetY: bounce, expression, antennaBlink: false };
    }
    default: {
      // idle: 4s cycle, first 200ms is blink
      const expression: Expression = tMs % 4000 < 200 ? 'blink' : 'normal';
      return { offsetY: bounce, expression, antennaBlink: false };
    }
  }
}
```

- [ ] **Step 2: Run tests — anim.test.ts should now pass**

```powershell
npx vitest run 2>&1 | Select-String -Pattern "FAIL|PASS|Tests|✓|×" | Select-Object -First 30
```

Expected: anim.test.ts GREEN. parts.test.ts should also be GREEN if render.ts was written correctly.

---

## Task 5: Update `Mascot.svelte`

**Files:**
- Modify: `src/Mascot.svelte`

**Interfaces:**
- Consumes: `drawRobot(ctx, spec, frame)` from `./lib/robot/render` where `frame` is the new `Frame` type (has `expression`, not `eyesOverride`)
- Produces: Updated Svelte component with 128×128 canvas and updated render call

- [ ] **Step 1: Update the canvas element and render call in `src/Mascot.svelte`**

Change line 140:
```html
<!-- OLD -->
<canvas bind:this={canvas} width="32" height="32" data-tauri-drag-region></canvas>
<!-- NEW -->
<canvas bind:this={canvas} width="128" height="128" data-tauri-drag-region></canvas>
```

Change line 122 (the drawRobot call inside the render loop):
```ts
// OLD
drawRobot(ctx, spec!, { eyesOverride: f.eyesOverride, offsetY: f.offsetY, antennaBlink: f.antennaBlink });
// NEW
drawRobot(ctx, spec!, f);
```

The `.robot` CSS class already has `width: 128px; height: 128px` and `canvas` already has `width: 128px; height: 128px; image-rendering: pixelated` — these are already correct per the existing Mascot.svelte source, so no CSS changes needed.

- [ ] **Step 2: Verify TypeScript compiles (no type errors)**

```powershell
npx tsc --noEmit 2>&1 | Select-String -Pattern "error" | Select-Object -First 20
```

Expected: 0 errors (the Frame type now matches between anim.ts and what drawRobot expects).

---

## Task 6: Full Test Suite GREEN

- [ ] **Step 1: Run full vitest**

```powershell
cd D:\Project\agent-mentor
npx vitest run 2>&1
```

Expected output: All tests pass. Note the passing count (target: all parts + anim + bubble tests GREEN).

- [ ] **Step 2: If any tests fail, diagnose and fix**

Common failure modes:
- `shapes.length < 25` → Add more shapes to one of the buildXxx() functions (e.g., add inner details to body)
- `color===7 not found` → Confirm `buildCheeks()` is called in `buildRobotShapes()` and HEAD_PARAMS cheek color is 7
- `color===6 not found` → Confirm highlight ellipse uses `color: 6`
- `antennaBlink true/false shapes not equal` → Confirm `tipColor = antennaBlink ? 2 : 0` produces different shapes
- `blink/sleep !== normal` → Confirm early returns in `buildEyes()` for expression branches

---

## Task 7: Build + Cargo Test

- [ ] **Step 1: Vite build**

```powershell
cd D:\Project\agent-mentor
npm run build 2>&1 | Select-Object -Last 10
```

Expected: Build completed successfully. No TypeScript or Vite errors.

- [ ] **Step 2: Cargo test (Git Bash)**

```bash
cd /d/Project/agent-mentor
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test 2>&1 | grep "test result"
```

Expected: `test result: ok. N passed` for each crate (core 98+ tests, app 4 tests).

---

## Task 8: Self-Review Checklist

Before committing, verify these manually:

- [ ] **6 antenna variants distinct silhouettes?** — rod-ball, forked (two stalks), dish (wide flat rrect), halo (ring stroke), lightning (zigzag lines), wing-ears (two oval flaps) — all different `kind` values → YES, distinct.
- [ ] **6 head variants distinct?** — oval (large rx/ry), square (small r), helmet (+visor rrect), cat-ears (+ear ellipses), crt (wide rx), capsule (large ry/r=28) → YES, distinct.
- [ ] **6 body variants distinct?** — canister (oval large r), box (+LED panel shapes), barrel (+wider mid rrect), vest (+lapel rrects), pocketed (+pocket stroke), striped (+3 accent rrects) → YES, distinct.
- [ ] **6 arms variants distinct?** — hanging, raised (different ly), pincer (+claw ellipses), stubby (smaller lw/lh), waving (asymmetric ly/ry2), rocket-punch (+ext trail) → YES, distinct.
- [ ] **Cell shading present?** — each body/head has `color: 1` shade ellipse + `color: 6` highlight ellipse → YES.
- [ ] **Cheeks present?** — `buildCheeks()` returns 2 ellipses with `color: 7` → YES.
- [ ] **Outlines present?** — every fill shape has a corresponding `stroke-rrect` or `stroke-ellipse` outline → YES.
- [ ] **Expression branching?** — `buildEyes()` has early returns for blink/happy/sleep before variant switch → YES.
- [ ] **All shape coords in 0..128?** — verify HEAD_PARAMS, BODY_PARAMS, ARMS_PARAMS, ANTENNA_PARAMS bounds manually for the most extreme variant (crt head: cx±rx = 64±46 = 18..110 ✓; capsule: cy±ry = 40±34 = 6..74 ✓; body all: y=70, h=52 → y+h=122 ✓; arms rocket punch: rx2+rw+extW = 98+20+28=146 ← FAIL, fix to 98+20+28=146, must reduce extW to 10: rx2+rw+extW = 98+20+10=128 ✓).

**IMPORTANT: Fix rocket-punch extW before committing:**

In `ARMS_PARAMS` entry for `rocket-punch`, `extW` must be `≤ 128 - (rx2 + rw) = 128 - 118 = 10`. Update `ARMS_PARAMS[5].extW` from `28` to `10`.

Also verify body x+w ≤ 128: box `x=26, w=76 → 102 ✓`; barrel mid `x=26-8=18, w=76+16=92 → 110 ✓`.

Arm pincer claw: `p.lx + p.lh + 4` for lh=38 → ly+lh+4 = 74+38+4=116 ✓.

---

## Task 9: Commit and Report

- [ ] **Step 1: Stage all changed frontend files**

```powershell
cd D:\Project\agent-mentor
git add src/lib/robot/parts.ts src/lib/robot/render.ts src/lib/robot/anim.ts src/Mascot.svelte src/lib/robot/parts.test.ts src/lib/robot/anim.test.ts
git status
```

- [ ] **Step 2: Create commit**

```powershell
git commit -m @'
feat: 마스코트 128 프로시저럴 렌더 — 도형 정의+셀 셰이딩, 미니미 질감 (E2E 피드백 3차)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

- [ ] **Step 3: Write report to `.superpowers/sdd/`**

```powershell
New-Item -ItemType Directory -Force D:\Project\agent-mentor\.superpowers\sdd
```

Write report to `D:\Project\agent-mentor\.superpowers\sdd\procedural-render-report.md` with:
- Status: DONE
- Commits: git SHA
- Tests: vitest result (N passing)
- Build: success/fail
- Cargo: test result line
- Concerns: any

---

## Spec Coverage Self-Review

| Spec requirement | Covered by |
|---|---|
| GRID=128 | `parts.ts` GRID constant |
| VARIANTS counts | `parts.ts` VARIANTS object |
| PALETTES 8×8 | `parts.ts` PALETTES verbatim from spec |
| Shape types (ellipse/rrect/stroke-rrect/stroke-ellipse/line) | `render.ts` Shape union type |
| buildRobotShapes pure+deterministic | `render.ts` buildRobotShapes |
| drawRobot clears+draws | `render.ts` drawRobot |
| Frame.expression replaces eyesOverride | `anim.ts` Frame interface |
| frameAt returns expression | `anim.ts` frameAt |
| idle blink at <200ms | `anim.ts` frameAt idle case |
| sleep/happy/alert expressions | `anim.ts` frameAt cases |
| antennaBlink toggle | `anim.ts` alert case |
| Canvas 128×128 | `Mascot.svelte` canvas element |
| drawRobot call signature | `Mascot.svelte` render loop |
| 6 antenna variants distinct silhouettes | `ANTENNA_PARAMS` + `buildAntenna()` |
| 6 head variants | `HEAD_PARAMS` + `buildHead()` |
| 6 eyes variants | `EYES_PARAMS` + `buildEyes()` |
| 6 body variants | `BODY_PARAMS` + `buildBody()` |
| 6 arms variants | `ARMS_PARAMS` + `buildArms()` |
| Cell shading (bodyShade+highlight) | shade ellipse color:1 + highlight ellipse color:6 |
| Outlines 2px | stroke-rrect/stroke-ellipse width:2 |
| Cheeks 2 ellipses color:7 | buildCheeks() |
| Expressions blink/happy/sleep | buildEyes() expression branches |
| antennaBlink tip color change | tipColor = antennaBlink ? 2 : 0 |
| Floor shadow | buildShadow() |
| All test gates (density≥25, cheek, highlight, color 0..7, bounds) | parts.test.ts |
| bubble.ts/bubble.test.ts untouched | Not in file map |
| Rust untouched | Not in file map |

No gaps found.
