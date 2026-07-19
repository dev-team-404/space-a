import { GRID, PALETTES, HEAD_VARIANTS, BODY_VARIANTS, ANTENNA_VARIANTS, EYES_VARIANTS, ARMS_VARIANTS } from './parts';

export interface RobotSpec {
  antenna: number;
  head: number;
  eyes: number;
  body: number;
  arms: number;
  palette: number;
}

export type Expression = 'normal' | 'blink' | 'happy' | 'sleep';

export interface Frame {
  offsetY: number;
  expression: Expression;
  antennaBlink: boolean;
}

// 2026-07-19 전면 재설계 — 싸이월드 미니미 감성 치비 로봇:
//   대두 비율(머리 ~60%) · 투톤 얼굴판 · 부드러운 림 셰이드 · 발 · 반투명 아웃라인 ·
//   작은 세로 타원 눈 + 반짝 1개 · 은은한 미소 · 또렷한 볼터치.
export type Shape = (
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number }
  | { kind: 'arc'; cx: number; cy: number; rx: number; ry: number; start: number; end: number; color: number; width: number }
) & { alpha?: number };

// Helper: head bounds (for cheek/eye/mouth positioning)
function headBounds(hi: number): { cx: number; cy: number; rx: number; ry: number } {
  const hv = HEAD_VARIANTS[hi];
  if (hv.kind === 'round' || hv.kind === 'capsule') {
    return { cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry };
  } else {
    const h = hv as { x: number; y: number; w: number; h: number };
    return { cx: h.x + h.w / 2, cy: h.y + h.h / 2, rx: h.w / 2, ry: h.h / 2 };
  }
}

// Shadow — 2겹 저투명 (발밑)
function buildShadow(): Shape[] {
  return [
    { kind: 'ellipse', cx: 64, cy: 120, rx: 28, ry: 5, color: 5, alpha: 0.08 },
    { kind: 'ellipse', cx: 64, cy: 120, rx: 20, ry: 4, color: 5, alpha: 0.12 },
  ];
}

// 발 — 치비의 안정감 (몸통 아래 작은 두 발)
function buildFeet(): Shape[] {
  const out: Shape[] = [];
  for (const cx of [53, 75]) {
    out.push({ kind: 'rrect', x: cx - 7, y: 105, w: 14, h: 10, r: 5, color: 0 });
    out.push({ kind: 'stroke-rrect', x: cx - 7, y: 105, w: 14, h: 10, r: 5, color: 5, width: 2 });
  }
  return out;
}

// 몸통 — 작게. 하단 림 셰이드로 볼륨.
function buildBody(bi: number): Shape[] {
  const bv = BODY_VARIANTS[bi];
  const out: Shape[] = [];

  const rimRect = (x: number, y: number, w: number, h: number): Shape => (
    { kind: 'rrect', x: x + 8, y: y + h - 9, w: w - 16, h: 5, r: 2.5, color: 1, alpha: 0.3 }
  );

  switch (bv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy + bv.ry * 0.5, rx: bv.rx * 0.65, ry: bv.ry * 0.28, color: 1, alpha: 0.3 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'box': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push(rimRect(bv.x, bv.y, bv.w, bv.h));
      out.push({ kind: 'rrect', x: bv.ledX, y: bv.ledY, w: bv.ledW, h: bv.ledH, r: 5, color: 6, alpha: 0.85 });
      out.push({ kind: 'ellipse', cx: bv.ledX + bv.ledW / 2, cy: bv.ledY + bv.ledH / 2, rx: 2.5, ry: 2.5, color: 2 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'barrel': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy + bv.ry * 0.5, rx: bv.rx * 0.65, ry: bv.ry * 0.28, color: 1, alpha: 0.3 });
      out.push({ kind: 'line', x1: bv.cx - bv.neckRx, y1: bv.cy - 3, x2: bv.cx + bv.neckRx, y2: bv.cy - 3, color: 5, width: 1, alpha: 0.35 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'vest': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x, y: bv.y + 4, w: bv.lapelW, h: bv.h * 0.6, r: 4, color: 2, alpha: 0.7 });
      out.push({ kind: 'rrect', x: bv.x + bv.w - bv.lapelW, y: bv.y + 4, w: bv.lapelW, h: bv.h * 0.6, r: 4, color: 2, alpha: 0.7 });
      out.push(rimRect(bv.x, bv.y, bv.w, bv.h));
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'pocket': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push(rimRect(bv.x, bv.y, bv.w, bv.h));
      out.push({ kind: 'rrect', x: bv.pocketX, y: bv.pocketY, w: bv.pocketW, h: bv.pocketH, r: 4, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-rrect', x: bv.pocketX, y: bv.pocketY, w: bv.pocketW, h: bv.pocketH, r: 4, color: 5, width: 1, alpha: 0.4 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'striped': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + 5, y: bv.stripeY1, w: bv.w - 10, h: 3, r: 1.5, color: 2, alpha: 0.6 });
      out.push({ kind: 'rrect', x: bv.x + 5, y: bv.stripeY2, w: bv.w - 10, h: 3, r: 1.5, color: 2, alpha: 0.6 });
      out.push({ kind: 'rrect', x: bv.x + 5, y: bv.stripeY3, w: bv.w - 10, h: 3, r: 1.5, color: 3, alpha: 0.6 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// 팔 — 몸통 옆 짧은 스텁 (치비 비율)
function buildArms(ai: number): Shape[] {
  const av = ARMS_VARIANTS[ai];
  const out: Shape[] = [];
  const lx = 24, rx = 94;

  const stub = (x: number, y: number, w: number, h: number, color = 0): void => {
    out.push({ kind: 'rrect', x, y, w, h, r: w / 2, color });
    out.push({ kind: 'stroke-rrect', x, y, w, h, r: w / 2, color: 5, width: 2 });
  };

  switch (av.kind) {
    case 'down': {
      stub(lx, av.ly, av.aw, av.ah);
      stub(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'up': {
      stub(lx - 2, av.ly, av.aw, av.ah);
      stub(rx + 2, av.ry, av.aw, av.ah);
      break;
    }
    case 'pincer': {
      stub(lx, av.ly, av.aw, av.ah);
      stub(rx, av.ry, av.aw, av.ah);
      for (const cx of [lx + av.aw / 2, rx + av.aw / 2]) {
        out.push({ kind: 'ellipse', cx, cy: av.ly + av.ah + 3, rx: 4, ry: 4, color: 0 });
        out.push({ kind: 'stroke-ellipse', cx, cy: av.ly + av.ah + 3, rx: 4, ry: 4, color: 5, width: 2 });
      }
      break;
    }
    case 'stubby': {
      for (const cx of [lx + av.aw / 2, rx + av.aw / 2]) {
        out.push({ kind: 'ellipse', cx, cy: av.ly + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 0 });
        out.push({ kind: 'stroke-ellipse', cx, cy: av.ly + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 5, width: 2 });
      }
      break;
    }
    case 'wave': {
      stub(lx - 2, av.ly, av.aw, av.ah, 2);
      stub(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'rocket': {
      stub(lx, av.ly, av.aw, av.ah);
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: av.ah / 2, color: 0 });
      out.push({ kind: 'ellipse', cx: rx + av.aw + av.extW, cy: av.ry + av.ah / 2, rx: 4, ry: 4, color: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: av.ah / 2, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// 머리 — 투톤 얼굴판(밝은 안쪽) + 하단 림 셰이드 + 좌상 하이라이트. 대두가 캐릭터다.
function buildHead(hi: number): Shape[] {
  const hv = HEAD_VARIANTS[hi];
  const out: Shape[] = [];

  const plateRect = (x: number, y: number, w: number, h: number, r: number): void => {
    out.push({ kind: 'rrect', x: x + 8, y: y + 10, w: w - 16, h: h - 18, r: Math.max(10, r - 12), color: 6, alpha: 0.55 });
  };
  const rimRect = (x: number, y: number, w: number, h: number): void => {
    out.push({ kind: 'rrect', x: x + 12, y: y + h - 12, w: w - 24, h: 7, r: 3.5, color: 1, alpha: 0.28 });
  };
  const glint = (cx: number, cy: number): void => {
    out.push({ kind: 'ellipse', cx, cy, rx: 8, ry: 5, color: 6, alpha: 0.85 });
  };

  switch (hv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy + 2, rx: hv.rx - 9, ry: hv.ry - 8, color: 6, alpha: 0.55 });
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy + hv.ry * 0.72, rx: hv.rx * 0.6, ry: hv.ry * 0.16, color: 1, alpha: 0.28 });
      glint(hv.cx - hv.rx * 0.45, hv.cy - hv.ry * 0.5);
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
    case 'square': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      plateRect(hv.x, hv.y, hv.w, hv.h, hv.r);
      rimRect(hv.x, hv.y, hv.w, hv.h);
      glint(hv.x + 20, hv.y + 14);
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'helmet': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      plateRect(hv.x, hv.y, hv.w, hv.h, hv.r);
      out.push({ kind: 'rrect', x: hv.x + 6, y: hv.visorY, w: hv.w - 12, h: hv.visorH, r: hv.visorH / 2, color: 2, alpha: 0.3 });
      rimRect(hv.x, hv.y, hv.w, hv.h);
      glint(hv.x + 20, hv.y + 14);
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'catear': {
      for (const ecx of [hv.x + hv.earW / 2 + 4, hv.x + hv.w - hv.earW / 2 - 4]) {
        out.push({ kind: 'ellipse', cx: ecx, cy: hv.y - hv.earH / 2 + 5, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 0 });
        out.push({ kind: 'stroke-ellipse', cx: ecx, cy: hv.y - hv.earH / 2 + 5, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 5, width: 2 });
        out.push({ kind: 'ellipse', cx: ecx, cy: hv.y - hv.earH / 2 + 6, rx: 4, ry: 5, color: 7, alpha: 0.8 });
      }
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      plateRect(hv.x, hv.y, hv.w, hv.h, hv.r);
      rimRect(hv.x, hv.y, hv.w, hv.h);
      glint(hv.x + 18, hv.y + 14);
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'crt': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      // CRT 정체성: 얼굴판이 은은한 액센트 스크린
      out.push({ kind: 'rrect', x: hv.x + 7, y: hv.y + 8, w: hv.w - 14, h: hv.h - 15, r: Math.max(10, hv.r - 10), color: 2, alpha: 0.22 });
      rimRect(hv.x, hv.y, hv.w, hv.h);
      glint(hv.x + 22, hv.y + 15);
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'capsule': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy + 2, rx: hv.rx - 8, ry: hv.ry - 9, color: 6, alpha: 0.55 });
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy + hv.ry * 0.72, rx: hv.rx * 0.55, ry: hv.ry * 0.14, color: 1, alpha: 0.28 });
      glint(hv.cx - hv.rx * 0.42, hv.cy - hv.ry * 0.5);
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Cheeks — 미니미 시그니처, 또렷하게
function buildCheeks(hi: number): Shape[] {
  const b = headBounds(hi);
  const cheekY = b.cy + b.ry * 0.36;
  const cheekDist = b.rx * 0.58;
  return [
    { kind: 'ellipse', cx: b.cx - cheekDist, cy: cheekY, rx: 7.5, ry: 5, color: 7, alpha: 0.8 },
    { kind: 'ellipse', cx: b.cx + cheekDist, cy: cheekY, rx: 7.5, ry: 5, color: 7, alpha: 0.8 },
  ];
}

// Mouth — 항상 입 (표정의 완성)
function buildMouth(hi: number, expression: Expression): Shape[] {
  const b = headBounds(hi);
  const my = b.cy + b.ry * 0.4;
  if (expression === 'happy') {
    return [
      { kind: 'arc', cx: b.cx, cy: my - 3, rx: 9, ry: 7, start: 0.12 * Math.PI, end: 0.88 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'sleep') {
    return [{ kind: 'stroke-ellipse', cx: b.cx, cy: my, rx: 3, ry: 3.5, color: 5, width: 2, alpha: 0.85 }];
  }
  return [
    { kind: 'arc', cx: b.cx, cy: my - 3, rx: 5, ry: 3.5, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: 5, width: 2 },
  ];
}

// 눈동자 공통 — 미니미: 작고 단순한 세로 타원 + 반짝 1개
function pupil(cx: number, cy: number, r: number): Shape[] {
  const rx = r * 0.52, ry = r * 0.78;
  return [
    { kind: 'ellipse', cx, cy, rx, ry, color: 5, alpha: 0.92 },
    { kind: 'ellipse', cx: cx - rx * 0.3, cy: cy - ry * 0.35, rx: rx * 0.42, ry: rx * 0.42, color: 4 },
  ];
}

// Eyes — 얼굴판 중앙, 살짝 낮게 (아기 얼굴)
function buildEyes(ei: number, hi: number, expression: Expression): Shape[] {
  const ev = EYES_VARIANTS[ei];
  const b = headBounds(hi);
  const eyeY = b.cy + b.ry * 0.02;

  if (expression === 'blink') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY - 3, rx: 7, ry: 4.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY - 3, rx: 7, ry: 4.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'happy') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY + 3, rx: 7, ry: 5.5, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY + 3, rx: 7, ry: 5.5, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'sleep') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY - 2, rx: 7, ry: 3.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY - 2, rx: 7, ry: 3.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'ellipse', cx: b.cx + 16, cy: eyeY - 14, rx: 3, ry: 3, color: 3, alpha: 0.9 },
      { kind: 'ellipse', cx: b.cx + 23, cy: eyeY - 21, rx: 4, ry: 4, color: 3, alpha: 0.9 },
    ];
  }

  // Normal — variant-specific
  const out: Shape[] = [];
  switch (ev.kind) {
    case 'round': {
      out.push(...pupil(ev.lx, eyeY, ev.er));
      out.push(...pupil(ev.rx, eyeY, ev.er));
      break;
    }
    case 'led': {
      const lw = ev.ew * 0.7, lh = ev.eh * 0.7;
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'rrect', x: cx - lw / 2, y: eyeY - lh / 2, w: lw, h: lh, r: lw / 2, color: 5, alpha: 0.92 });
        out.push({ kind: 'ellipse', cx: cx - 1.5, cy: eyeY - lh * 0.22, rx: 2, ry: 2.4, color: 4 });
      }
      break;
    }
    case 'star': {
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'line', x1: cx - ev.sr, y1: eyeY, x2: cx + ev.sr, y2: eyeY, color: 2, width: 4 });
        out.push({ kind: 'line', x1: cx, y1: eyeY - ev.sr, x2: cx, y2: eyeY + ev.sr, color: 2, width: 4 });
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 2.6, ry: 2.6, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1, cy: eyeY - 1, rx: 1.1, ry: 1.1, color: 4 });
      }
      break;
    }
    case 'heart': {
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'ellipse', cx: cx - 3.5, cy: eyeY - 2.5, rx: 4.2, ry: 4.2, color: 2 });
        out.push({ kind: 'ellipse', cx: cx + 3.5, cy: eyeY - 2.5, rx: 4.2, ry: 4.2, color: 2 });
        out.push({ kind: 'rrect', x: cx - 6, y: eyeY - 2.5, w: 12, h: 7, r: 2.5, color: 2 });
        out.push({ kind: 'ellipse', cx: cx - 3.5, cy: eyeY - 4.2, rx: 1.6, ry: 1.6, color: 4, alpha: 0.9 });
      }
      break;
    }
    case 'drowsy': {
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'ellipse', cx, cy: eyeY + 1, rx: 4.6, ry: 4, color: 5, alpha: 0.92 });
        out.push({ kind: 'ellipse', cx: cx - 1.5, cy: eyeY - 0.5, rx: 1.6, ry: 1.6, color: 4 });
        out.push({ kind: 'rrect', x: cx - 5.5, y: eyeY - 5, w: 11, h: 3.5, r: 1.75, color: 0 });
        out.push({ kind: 'line', x1: cx - 5.5, y1: eyeY - 1.5, x2: cx + 5.5, y2: eyeY - 1.5, color: 5, width: 2, alpha: 0.8 });
      }
      break;
    }
    case 'scanner': {
      const cx = (ev.x1 + ev.x2) / 2;
      out.push({ kind: 'rrect', x: ev.x1, y: eyeY - 7, w: ev.x2 - ev.x1, h: 14, r: 7, color: 2, alpha: 0.35 });
      out.push({ kind: 'stroke-rrect', x: ev.x1, y: eyeY - 7, w: ev.x2 - ev.x1, h: 14, r: 7, color: 5, width: 2, alpha: 0.5 });
      out.push(...pupil(cx, eyeY, 8));
      break;
    }
  }
  return out;
}

// Antenna — 짧고 동글 (머리에 바짝)
function buildAntenna(ai: number, antennaBlink: boolean): Shape[] {
  const av = ANTENNA_VARIANTS[ai];
  const tipColor = antennaBlink ? 2 : 3;
  const out: Shape[] = [];

  switch (av.kind) {
    case 'rod': {
      out.push({ kind: 'line', x1: av.stemX, y1: av.stemY2, x2: av.stemX, y2: av.ballCy + av.ballR, color: 5, width: 2 });
      out.push({ kind: 'ellipse', cx: av.ballCx, cy: av.ballCy + av.ballR, rx: av.ballR, ry: av.ballR, color: tipColor });
      out.push({ kind: 'ellipse', cx: av.ballCx - 1.5, cy: av.ballCy + av.ballR - 1.5, rx: 1.6, ry: 1.6, color: 4, alpha: 0.9 });
      out.push({ kind: 'stroke-ellipse', cx: av.ballCx, cy: av.ballCy + av.ballR, rx: av.ballR, ry: av.ballR, color: 5, width: 1 });
      break;
    }
    case 'feelers': {
      out.push({ kind: 'line', x1: av.lx, y1: av.baseY, x2: av.lx, y2: av.tipY + 5, color: 5, width: 2 });
      out.push({ kind: 'line', x1: av.rx, y1: av.baseY, x2: av.rx, y2: av.tipY + 5, color: 5, width: 2 });
      for (const cx of [av.lx, av.rx]) {
        out.push({ kind: 'ellipse', cx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: tipColor });
        out.push({ kind: 'stroke-ellipse', cx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: 5, width: 1 });
      }
      break;
    }
    case 'dish': {
      out.push({ kind: 'rrect', x: av.dishX, y: av.dishY, w: av.dishW, h: av.dishH, r: av.dishH / 2, color: tipColor });
      out.push({ kind: 'stroke-rrect', x: av.dishX, y: av.dishY, w: av.dishW, h: av.dishH, r: av.dishH / 2, color: 5, width: 2 });
      out.push({ kind: 'line', x1: av.stemX, y1: av.stemY1, x2: av.stemX, y2: av.stemY2, color: 5, width: 2 });
      break;
    }
    case 'ring': {
      out.push({ kind: 'stroke-ellipse', cx: av.ringCx, cy: av.ringCy + av.ringRy, rx: av.ringRx, ry: av.ringRy, color: tipColor, width: 4 });
      out.push({ kind: 'stroke-ellipse', cx: av.ringCx, cy: av.ringCy + av.ringRy, rx: av.ringRx, ry: av.ringRy, color: 5, width: 2 });
      out.push({ kind: 'line', x1: av.stemX, y1: av.stemY1, x2: av.stemX, y2: av.stemY2, color: 5, width: 2 });
      break;
    }
    case 'lightning': {
      const pts = av.points;
      for (let i = 0; i < pts.length - 1; i++) {
        out.push({ kind: 'line', x1: pts[i][0], y1: pts[i][1], x2: pts[i + 1][0], y2: pts[i + 1][1], color: tipColor, width: 3 });
      }
      out.push({ kind: 'ellipse', cx: pts[0][0], cy: pts[0][1] + 3, rx: 3.5, ry: 3.5, color: tipColor });
      break;
    }
    case 'flapear': {
      for (const cx of [av.lx, av.rx]) {
        out.push({ kind: 'ellipse', cx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: tipColor });
        out.push({ kind: 'stroke-ellipse', cx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: 5, width: 2 });
      }
      break;
    }
  }
  return out;
}

// buildRobotShapes (pure, deterministic)
export function buildRobotShapes(spec: RobotSpec, frame: Frame): Shape[] {
  const { offsetY } = frame;
  const shift = (s: Shape): Shape => {
    if (s.kind === 'ellipse' || s.kind === 'stroke-ellipse' || s.kind === 'arc') return { ...s, cy: s.cy + offsetY };
    if (s.kind === 'rrect' || s.kind === 'stroke-rrect') return { ...s, y: s.y + offsetY };
    if (s.kind === 'line') return { ...s, y1: s.y1 + offsetY, y2: s.y2 + offsetY };
    return s;
  };

  return [
    ...buildShadow(),
    ...buildFeet().map(shift),
    ...buildBody(spec.body).map(shift),
    ...buildArms(spec.arms).map(shift),
    ...buildHead(spec.head).map(shift),
    ...buildCheeks(spec.head).map(shift),
    ...buildEyes(spec.eyes, spec.head, frame.expression).map(shift),
    ...buildMouth(spec.head, frame.expression).map(shift),
    ...buildAntenna(spec.antenna, frame.antennaBlink).map(shift),
  ];
}

// drawRobot
function drawShape(ctx: CanvasRenderingContext2D, s: Shape, palette: ReadonlyArray<string>): void {
  ctx.save();
  if (s.alpha !== undefined) {
    ctx.globalAlpha = s.alpha;
  } else if (s.color === 5) {
    // 미니미 감성: 묵직한 외곽선 대신 부드러운 반투명 선.
    // 구조 아웃라인(stroke/line)은 0.55, 표정(arc·눈 fill)은 또렷하게 0.85.
    if (s.kind === 'stroke-rrect' || s.kind === 'stroke-ellipse' || s.kind === 'line') ctx.globalAlpha = 0.55;
    else if (s.kind === 'arc') ctx.globalAlpha = 0.85;
  }
  const color = palette[s.color];
  if (s.kind === 'ellipse') {
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.ellipse(s.cx, s.cy, s.rx, s.ry, 0, 0, Math.PI * 2);
    ctx.fill();
  } else if (s.kind === 'stroke-ellipse') {
    ctx.strokeStyle = color;
    ctx.lineWidth = s.width;
    ctx.beginPath();
    ctx.ellipse(s.cx, s.cy, s.rx, s.ry, 0, 0, Math.PI * 2);
    ctx.stroke();
  } else if (s.kind === 'arc') {
    ctx.strokeStyle = color;
    ctx.lineWidth = s.width;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.ellipse(s.cx, s.cy, s.rx, s.ry, 0, s.start, s.end);
    ctx.stroke();
  } else if (s.kind === 'rrect') {
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.roundRect(s.x, s.y, s.w, s.h, s.r);
    ctx.fill();
  } else if (s.kind === 'stroke-rrect') {
    ctx.strokeStyle = color;
    ctx.lineWidth = s.width;
    ctx.beginPath();
    ctx.roundRect(s.x, s.y, s.w, s.h, s.r);
    ctx.stroke();
  } else if (s.kind === 'line') {
    ctx.strokeStyle = color;
    ctx.lineWidth = s.width;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.moveTo(s.x1, s.y1);
    ctx.lineTo(s.x2, s.y2);
    ctx.stroke();
  }
  ctx.restore();
}

export function drawRobot(ctx: CanvasRenderingContext2D, spec: RobotSpec, frame: Frame): void {
  ctx.clearRect(0, 0, GRID, GRID);
  const palette = PALETTES[spec.palette];
  for (const s of buildRobotShapes(spec, frame)) {
    drawShape(ctx, s, palette);
  }
}
