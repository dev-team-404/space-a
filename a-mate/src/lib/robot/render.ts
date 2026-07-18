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

// 귀여움 원칙(2026-07-18 개편): 큰 눈동자+반짝 하이라이트, 밝은 배(어두운 얼룩 금지),
// 항상 입(미소), 부드러운 그림자(alpha), 볼터치. 얼굴 위 어두운 셰이딩은 멍처럼 보여 제거.
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

// Shadow — 2겹 부드럽게 (진한 단색 원은 딱딱해 보인다)
function buildShadow(): Shape[] {
  return [
    { kind: 'ellipse', cx: 64, cy: 118, rx: 30, ry: 6, color: 5, alpha: 0.08 },
    { kind: 'ellipse', cx: 64, cy: 118, rx: 22, ry: 4.5, color: 5, alpha: 0.12 },
  ];
}

// Body — 어두운 패널 대신 밝은 배(color 6): 밝은 배가 아기 체형처럼 읽힌다
function buildBody(bi: number): Shape[] {
  const bv = BODY_VARIANTS[bi];
  const out: Shape[] = [];

  switch (bv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy + bv.ry * 0.2, rx: bv.rx * 0.55, ry: bv.ry * 0.5, color: 6, alpha: 0.85 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'box': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 0 });
      out.push({ kind: 'rrect', x: bv.ledX, y: bv.ledY, w: bv.ledW, h: bv.ledH, r: 6, color: 6, alpha: 0.9 });
      out.push({ kind: 'ellipse', cx: bv.ledX + bv.ledW / 2, cy: bv.ledY + bv.ledH / 2, rx: 4, ry: 4, color: 2 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 5, width: 2 });
      break;
    }
    case 'barrel': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy + bv.ry * 0.15, rx: bv.rx * 0.5, ry: bv.ry * 0.45, color: 6, alpha: 0.85 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'vest': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 0 });
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.lapelW, h: bv.h * 0.7, r: 4, color: 2, alpha: 0.8 });
      out.push({ kind: 'rrect', x: bv.x + bv.w - bv.lapelW, y: bv.y, w: bv.lapelW, h: bv.h * 0.7, r: 4, color: 2, alpha: 0.8 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.3, y: bv.y + bv.h * 0.4, w: bv.w * 0.4, h: bv.h * 0.45, r: 8, color: 6, alpha: 0.85 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 5, width: 2 });
      break;
    }
    case 'pocket': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.3, y: bv.y + bv.h * 0.35, w: bv.w * 0.4, h: bv.h * 0.5, r: 8, color: 6, alpha: 0.85 });
      out.push({ kind: 'stroke-rrect', x: bv.pocketX, y: bv.pocketY, w: bv.pocketW, h: bv.pocketH, r: 5, color: 5, width: 1, alpha: 0.5 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 5, width: 2 });
      break;
    }
    case 'striped': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + 4, y: bv.stripeY1, w: bv.w - 8, h: 4, r: 2, color: 2, alpha: 0.75 });
      out.push({ kind: 'rrect', x: bv.x + 4, y: bv.stripeY2, w: bv.w - 8, h: 4, r: 2, color: 2, alpha: 0.75 });
      out.push({ kind: 'rrect', x: bv.x + 4, y: bv.stripeY3, w: bv.w - 8, h: 4, r: 2, color: 3, alpha: 0.75 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: 14, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Arms — 짧고 둥글게 (뭉툭한 팔이 귀엽다)
function buildArms(ai: number): Shape[] {
  const av = ARMS_VARIANTS[ai];
  const out: Shape[] = [];
  const lx = 16, rx = 98;

  const roundArm = (x: number, y: number, w: number, h: number, color = 0): void => {
    out.push({ kind: 'rrect', x, y, w, h, r: w / 2, color });
    out.push({ kind: 'stroke-rrect', x, y, w, h, r: w / 2, color: 5, width: 2 });
  };

  switch (av.kind) {
    case 'down': {
      roundArm(lx, av.ly, av.aw, av.ah);
      roundArm(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'up': {
      roundArm(lx - 4, av.ly, av.aw, av.ah);
      roundArm(rx + 4, av.ry, av.aw, av.ah);
      break;
    }
    case 'pincer': {
      roundArm(lx, av.ly, av.aw, av.ah);
      roundArm(rx, av.ry, av.aw, av.ah);
      out.push({ kind: 'ellipse', cx: lx + av.aw / 2, cy: av.ly + av.ah + 4, rx: 5, ry: 5, color: 0 });
      out.push({ kind: 'ellipse', cx: rx + av.aw / 2, cy: av.ry + av.ah + 4, rx: 5, ry: 5, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: lx + av.aw / 2, cy: av.ly + av.ah + 4, rx: 5, ry: 5, color: 5, width: 2 });
      out.push({ kind: 'stroke-ellipse', cx: rx + av.aw / 2, cy: av.ry + av.ah + 4, rx: 5, ry: 5, color: 5, width: 2 });
      break;
    }
    case 'stubby': {
      out.push({ kind: 'ellipse', cx: lx + av.aw / 2, cy: av.ly + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 0 });
      out.push({ kind: 'ellipse', cx: rx + av.aw / 2, cy: av.ry + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: lx + av.aw / 2, cy: av.ly + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 5, width: 2 });
      out.push({ kind: 'stroke-ellipse', cx: rx + av.aw / 2, cy: av.ry + av.ah / 2, rx: av.aw / 2 + 1, ry: av.ah / 2, color: 5, width: 2 });
      break;
    }
    case 'wave': {
      roundArm(lx - 4, av.ly, av.aw, av.ah, 2);
      roundArm(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'rocket': {
      roundArm(lx, av.ly, av.aw, av.ah);
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: av.ah / 2, color: 0 });
      out.push({ kind: 'ellipse', cx: rx + av.aw + av.extW, cy: av.ry + av.ah / 2, rx: 5, ry: 5, color: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: av.ah / 2, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Head — 얼굴 위 어두운 셰이딩 금지. 밝은 하이라이트만.
function buildHead(hi: number): Shape[] {
  const hv = HEAD_VARIANTS[hi];
  const out: Shape[] = [];

  switch (hv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx - hv.rx * 0.35, cy: hv.cy - hv.ry * 0.4, rx: 9, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
    case 'square': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 24, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.x + 16, cy: hv.y + 12, rx: 9, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 24, color: 5, width: 2 });
      break;
    }
    case 'helmet': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 26, color: 0 });
      out.push({ kind: 'rrect', x: hv.x + 4, y: hv.visorY, w: hv.w - 8, h: hv.visorH, r: hv.visorH / 2, color: 2, alpha: 0.55 });
      out.push({ kind: 'ellipse', cx: hv.x + 16, cy: hv.y + 12, rx: 9, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 26, color: 5, width: 2 });
      break;
    }
    case 'catear': {
      out.push({ kind: 'ellipse', cx: hv.x + hv.earW / 2 + 2, cy: hv.y - hv.earH / 2 + 4, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 2 });
      out.push({ kind: 'ellipse', cx: hv.x + hv.w - hv.earW / 2 - 2, cy: hv.y - hv.earH / 2 + 4, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 2 });
      out.push({ kind: 'stroke-ellipse', cx: hv.x + hv.earW / 2 + 2, cy: hv.y - hv.earH / 2 + 4, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 5, width: 2 });
      out.push({ kind: 'stroke-ellipse', cx: hv.x + hv.w - hv.earW / 2 - 2, cy: hv.y - hv.earH / 2 + 4, rx: hv.earW / 2 + 2, ry: hv.earH / 2 + 2, color: 5, width: 2 });
      out.push({ kind: 'ellipse', cx: hv.x + hv.earW / 2 + 2, cy: hv.y - hv.earH / 2 + 5, rx: 4, ry: 5, color: 7, alpha: 0.8 });
      out.push({ kind: 'ellipse', cx: hv.x + hv.w - hv.earW / 2 - 2, cy: hv.y - hv.earH / 2 + 5, rx: 4, ry: 5, color: 7, alpha: 0.8 });
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 20, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.x + 16, cy: hv.y + 12, rx: 9, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 20, color: 5, width: 2 });
      break;
    }
    case 'crt': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 24, color: 0 });
      out.push({ kind: 'rrect', x: hv.x + 6, y: hv.y + 6, w: hv.w - 12, h: hv.h - 12, r: 18, color: 2, alpha: 0.35 });
      out.push({ kind: 'ellipse', cx: hv.x + 18, cy: hv.y + 13, rx: 9, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: 24, color: 5, width: 2 });
      break;
    }
    case 'capsule': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx - hv.rx * 0.35, cy: hv.cy - hv.ry * 0.4, rx: 7, ry: 6, color: 6, alpha: 0.8 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Cheeks — 눈 바로 아래, 은은하게
function buildCheeks(hi: number): Shape[] {
  const b = headBounds(hi);
  const cheekY = b.cy + b.ry * 0.38;
  const cheekDist = b.rx * 0.6;
  return [
    { kind: 'ellipse', cx: b.cx - cheekDist, cy: cheekY, rx: 7, ry: 4.5, color: 7, alpha: 0.7 },
    { kind: 'ellipse', cx: b.cx + cheekDist, cy: cheekY, rx: 7, ry: 4.5, color: 7, alpha: 0.7 },
  ];
}

// Mouth — 항상 입이 있어야 표정이 산다
function buildMouth(hi: number, expression: Expression): Shape[] {
  const b = headBounds(hi);
  const my = b.cy + b.ry * 0.42;
  if (expression === 'happy') {
    // 활짝 — 열린 미소
    return [
      { kind: 'arc', cx: b.cx, cy: my - 3, rx: 10, ry: 8, start: 0.12 * Math.PI, end: 0.88 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'sleep') {
    // 잠 — 작은 'o' 입
    return [{ kind: 'stroke-ellipse', cx: b.cx, cy: my, rx: 3, ry: 3.5, color: 5, width: 2 }];
  }
  // 기본 — 잔잔한 미소
  return [
    { kind: 'arc', cx: b.cx, cy: my - 3, rx: 6, ry: 4.5, start: 0.18 * Math.PI, end: 0.82 * Math.PI, color: 5, width: 2 },
  ];
}

// 눈동자 공통: 어두운 눈 + 큰 반짝(좌상) + 작은 반짝(우하) — 생기의 핵심
function pupil(cx: number, cy: number, r: number): Shape[] {
  return [
    { kind: 'ellipse', cx, cy, rx: r, ry: r, color: 5 },
    { kind: 'ellipse', cx: cx - r * 0.3, cy: cy - r * 0.35, rx: r * 0.38, ry: r * 0.38, color: 4 },
    { kind: 'ellipse', cx: cx + r * 0.35, cy: cy + r * 0.3, rx: r * 0.16, ry: r * 0.16, color: 4, alpha: 0.9 },
  ];
}

// Eyes — 아기 얼굴 원칙: 눈은 크게, 살짝 낮게
function buildEyes(ei: number, hi: number, expression: Expression): Shape[] {
  const ev = EYES_VARIANTS[ei];
  const b = headBounds(hi);
  const eyeY = b.cy + b.ry * 0.02;

  if (expression === 'blink') {
    // 감은 눈 — 아래로 볼록한 부드러운 곡선
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY - 3, rx: 8, ry: 5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY - 3, rx: 8, ry: 5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'happy') {
    // ^^ — 위로 볼록한 기쁨 곡선
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY + 3, rx: 8, ry: 6, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY + 3, rx: 8, ry: 6, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 3 },
    ];
  }
  if (expression === 'sleep') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'arc', cx: lx, cy: eyeY - 2, rx: 8, ry: 4, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'arc', cx: rx2, cy: eyeY - 2, rx: 8, ry: 4, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 3 },
      { kind: 'ellipse', cx: b.cx + 16, cy: eyeY - 14, rx: 3, ry: 3, color: 3, alpha: 0.9 },
      { kind: 'ellipse', cx: b.cx + 23, cy: eyeY - 21, rx: 4, ry: 4, color: 3, alpha: 0.9 },
    ];
  }

  // Normal expression -- variant-specific
  const out: Shape[] = [];
  switch (ev.kind) {
    case 'round': {
      out.push(...pupil(ev.lx, eyeY, ev.er));
      out.push(...pupil(ev.rx, eyeY, ev.er));
      break;
    }
    case 'led': {
      // 세로 캡슐 눈 (다마고치풍) + 반짝
      out.push({ kind: 'rrect', x: ev.lx - ev.ew / 2, y: eyeY - ev.eh / 2, w: ev.ew, h: ev.eh, r: ev.ew / 2, color: 5 });
      out.push({ kind: 'rrect', x: ev.rx - ev.ew / 2, y: eyeY - ev.eh / 2, w: ev.ew, h: ev.eh, r: ev.ew / 2, color: 5 });
      out.push({ kind: 'ellipse', cx: ev.lx - 2, cy: eyeY - ev.eh * 0.2, rx: 3, ry: 3.5, color: 4 });
      out.push({ kind: 'ellipse', cx: ev.rx - 2, cy: eyeY - ev.eh * 0.2, rx: 3, ry: 3.5, color: 4 });
      break;
    }
    case 'star': {
      // 반짝이 눈 — 십자 스파클 + 중심 점
      const arms: ReadonlyArray<readonly [number, number]> = [[ev.lx, eyeY], [ev.rx, eyeY]];
      for (const [cx] of arms) {
        out.push({ kind: 'line', x1: cx - ev.sr, y1: eyeY, x2: cx + ev.sr, y2: eyeY, color: 2, width: 4 });
        out.push({ kind: 'line', x1: cx, y1: eyeY - ev.sr, x2: cx, y2: eyeY + ev.sr, color: 2, width: 4 });
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 3, ry: 3, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1, cy: eyeY - 1, rx: 1.2, ry: 1.2, color: 4 });
      }
      break;
    }
    case 'heart': {
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'ellipse', cx: cx - 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
        out.push({ kind: 'ellipse', cx: cx + 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
        out.push({ kind: 'rrect', x: cx - 7, y: eyeY - 3, w: 14, h: 8, r: 3, color: 2 });
        out.push({ kind: 'ellipse', cx: cx - 4, cy: eyeY - 5, rx: 1.8, ry: 1.8, color: 4, alpha: 0.9 });
      }
      break;
    }
    case 'drowsy': {
      // 반쯤 감긴 눈 — 눈동자는 살아있게, 위 눈꺼풀만
      for (const cx of [ev.lx, ev.rx]) {
        out.push({ kind: 'ellipse', cx, cy: eyeY + 1, rx: 6, ry: 5, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 2, cy: eyeY - 1, rx: 2, ry: 2, color: 4 });
        out.push({ kind: 'rrect', x: cx - 7, y: eyeY - 6, w: 14, h: 4, r: 2, color: 0 });
        out.push({ kind: 'line', x1: cx - 7, y1: eyeY - 2, x2: cx + 7, y2: eyeY - 2, color: 5, width: 2 });
      }
      break;
    }
    case 'scanner': {
      // 사이클롭스 — 바이저 안 큰 외눈 + 반짝
      const cx = (ev.x1 + ev.x2) / 2;
      out.push({ kind: 'rrect', x: ev.x1 + 8, y: eyeY - 8, w: ev.x2 - ev.x1 - 16, h: 16, r: 8, color: 2, alpha: 0.45 });
      out.push({ kind: 'stroke-rrect', x: ev.x1 + 8, y: eyeY - 8, w: ev.x2 - ev.x1 - 16, h: 16, r: 8, color: 5, width: 2 });
      out.push(...pupil(cx, eyeY, 7));
      break;
    }
  }
  return out;
}

// Antenna
function buildAntenna(ai: number, antennaBlink: boolean): Shape[] {
  const av = ANTENNA_VARIANTS[ai];
  const tipColor = antennaBlink ? 2 : 3;
  const out: Shape[] = [];

  switch (av.kind) {
    case 'rod': {
      out.push({ kind: 'line', x1: av.stemX, y1: av.stemY2, x2: av.stemX, y2: av.ballCy + av.ballR, color: 5, width: 2 });
      out.push({ kind: 'ellipse', cx: av.ballCx, cy: av.ballCy + av.ballR, rx: av.ballR, ry: av.ballR, color: tipColor });
      out.push({ kind: 'ellipse', cx: av.ballCx - 2, cy: av.ballCy + av.ballR - 2, rx: 2, ry: 2, color: 4, alpha: 0.9 });
      out.push({ kind: 'stroke-ellipse', cx: av.ballCx, cy: av.ballCy + av.ballR, rx: av.ballR, ry: av.ballR, color: 5, width: 1 });
      break;
    }
    case 'feelers': {
      out.push({ kind: 'line', x1: av.lx, y1: av.baseY, x2: av.lx, y2: av.tipY + 6, color: 5, width: 2 });
      out.push({ kind: 'line', x1: av.rx, y1: av.baseY, x2: av.rx, y2: av.tipY + 6, color: 5, width: 2 });
      out.push({ kind: 'ellipse', cx: av.lx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: tipColor });
      out.push({ kind: 'ellipse', cx: av.rx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: tipColor });
      out.push({ kind: 'stroke-ellipse', cx: av.lx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: 5, width: 1 });
      out.push({ kind: 'stroke-ellipse', cx: av.rx, cy: av.tipY + av.tipR, rx: av.tipR, ry: av.tipR, color: 5, width: 1 });
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
      out.push({ kind: 'ellipse', cx: pts[0][0], cy: pts[0][1] + 4, rx: 4, ry: 4, color: tipColor });
      break;
    }
    case 'flapear': {
      out.push({ kind: 'ellipse', cx: av.lx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: tipColor });
      out.push({ kind: 'ellipse', cx: av.rx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: tipColor });
      out.push({ kind: 'stroke-ellipse', cx: av.lx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: 5, width: 2 });
      out.push({ kind: 'stroke-ellipse', cx: av.rx, cy: av.earY, rx: av.earRx, ry: av.earRy, color: 5, width: 2 });
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
  if (s.alpha !== undefined) ctx.globalAlpha = s.alpha;
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
