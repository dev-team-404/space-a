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

export type Shape =
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number };

// Helper: head bounds (for cheek/eye positioning)
function headBounds(hi: number): { cx: number; cy: number; rx: number; ry: number } {
  const hv = HEAD_VARIANTS[hi];
  if (hv.kind === 'round' || hv.kind === 'capsule') {
    return { cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry };
  } else {
    const h = hv as { x: number; y: number; w: number; h: number };
    return { cx: h.x + h.w / 2, cy: h.y + h.h / 2, rx: h.w / 2, ry: h.h / 2 };
  }
}

// Shadow
function buildShadow(): Shape[] {
  return [
    { kind: 'ellipse', cx: 64, cy: 118, rx: 30, ry: 6, color: 5 },
  ];
}

// Body
function buildBody(bi: number): Shape[] {
  const bv = BODY_VARIANTS[bi];
  const out: Shape[] = [];

  switch (bv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx + 8, cy: bv.cy + 8, rx: bv.rx * 0.5, ry: bv.ry * 0.4, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.cx - 10, cy: bv.cy - 8, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'box': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.4, y: bv.y + bv.h * 0.55, w: bv.w * 0.5, h: bv.h * 0.35, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.x + 8, cy: bv.y + 8, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'rrect', x: bv.ledX, y: bv.ledY, w: bv.ledW, h: bv.ledH, r: 2, color: 2 });
      out.push({ kind: 'stroke-rrect', x: bv.ledX, y: bv.ledY, w: bv.ledW, h: bv.ledH, r: 2, color: 5, width: 1 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'barrel': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: bv.cx + 10, cy: bv.cy + 8, rx: bv.rx * 0.45, ry: bv.ry * 0.4, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.cx - 12, cy: bv.cy - 10, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'line', x1: bv.cx - bv.neckRx, y1: bv.cy - 8, x2: bv.cx + bv.neckRx, y2: bv.cy - 8, color: 5, width: 1 });
      out.push({ kind: 'line', x1: bv.cx - bv.neckRx, y1: bv.cy + 8, x2: bv.cx + bv.neckRx, y2: bv.cy + 8, color: 5, width: 1 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'vest': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.lapelW, h: bv.h * 0.7, r: 2, color: 2 });
      out.push({ kind: 'rrect', x: bv.x + bv.w - bv.lapelW, y: bv.y, w: bv.lapelW, h: bv.h * 0.7, r: 2, color: 2 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.4, y: bv.y + bv.h * 0.55, w: bv.w * 0.5, h: bv.h * 0.35, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.x + 10, cy: bv.y + 8, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'pocket': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.4, y: bv.y + bv.h * 0.55, w: bv.w * 0.5, h: bv.h * 0.35, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.x + 10, cy: bv.y + 8, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'stroke-rrect', x: bv.pocketX, y: bv.pocketY, w: bv.pocketW, h: bv.pocketH, r: 2, color: 5, width: 1 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'striped': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'rrect', x: bv.x + 2, y: bv.stripeY1, w: bv.w - 4, h: 4, r: 0, color: 2 });
      out.push({ kind: 'rrect', x: bv.x + 2, y: bv.stripeY2, w: bv.w - 4, h: 4, r: 0, color: 2 });
      out.push({ kind: 'rrect', x: bv.x + 2, y: bv.stripeY3, w: bv.w - 4, h: 4, r: 0, color: 3 });
      out.push({ kind: 'rrect', x: bv.x + bv.w * 0.4, y: bv.y + bv.h * 0.7, w: bv.w * 0.5, h: bv.h * 0.2, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: bv.x + 10, cy: bv.y + 8, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Arms
function buildArms(ai: number): Shape[] {
  const av = ARMS_VARIANTS[ai];
  const out: Shape[] = [];
  const lx = 16, rx = 98;

  switch (av.kind) {
    case 'down': {
      out.push({ kind: 'rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: lx + av.aw*0.4, y: av.ly + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'rrect', x: rx + av.aw*0.4, y: av.ry + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'stroke-rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      break;
    }
    case 'up': {
      out.push({ kind: 'rrect', x: lx - 4, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: rx + 4, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: lx - 4 + av.aw*0.4, y: av.ly + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'rrect', x: rx + 4 + av.aw*0.4, y: av.ry + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'stroke-rrect', x: lx - 4, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: rx + 4, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      break;
    }
    case 'pincer': {
      out.push({ kind: 'rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: lx - 2, y: av.ly + av.ah, w: 6, h: 8, r: 2, color: 0 });
      out.push({ kind: 'rrect', x: lx + av.aw - 4, y: av.ly + av.ah, w: 6, h: 8, r: 2, color: 0 });
      out.push({ kind: 'rrect', x: rx - 2, y: av.ry + av.ah, w: 6, h: 8, r: 2, color: 0 });
      out.push({ kind: 'rrect', x: rx + av.aw - 4, y: av.ry + av.ah, w: 6, h: 8, r: 2, color: 0 });
      out.push({ kind: 'rrect', x: lx + av.aw*0.4, y: av.ly + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.3, r: 2, color: 1 });
      out.push({ kind: 'rrect', x: rx + av.aw*0.4, y: av.ry + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.3, r: 2, color: 1 });
      out.push({ kind: 'stroke-rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      break;
    }
    case 'stubby': {
      out.push({ kind: 'ellipse', cx: lx + av.aw/2, cy: av.ly + av.ah/2, rx: av.aw/2, ry: av.ah/2, color: 0 });
      out.push({ kind: 'ellipse', cx: rx + av.aw/2, cy: av.ry + av.ah/2, rx: av.aw/2, ry: av.ah/2, color: 0 });
      out.push({ kind: 'ellipse', cx: lx + av.aw/2 + 2, cy: av.ly + av.ah/2 + 2, rx: av.aw/4, ry: av.ah/4, color: 1 });
      out.push({ kind: 'ellipse', cx: rx + av.aw/2 + 2, cy: av.ry + av.ah/2 + 2, rx: av.aw/4, ry: av.ah/4, color: 1 });
      out.push({ kind: 'stroke-ellipse', cx: lx + av.aw/2, cy: av.ly + av.ah/2, rx: av.aw/2, ry: av.ah/2, color: 5, width: 2 });
      out.push({ kind: 'stroke-ellipse', cx: rx + av.aw/2, cy: av.ry + av.ah/2, rx: av.aw/2, ry: av.ah/2, color: 5, width: 2 });
      break;
    }
    case 'wave': {
      out.push({ kind: 'rrect', x: lx - 4, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 2 });
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: lx - 4 + av.aw*0.4, y: av.ly + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 3 });
      out.push({ kind: 'rrect', x: rx + av.aw*0.4, y: av.ry + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'stroke-rrect', x: lx - 4, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      break;
    }
    case 'rocket': {
      out.push({ kind: 'rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: 4, color: 0 });
      out.push({ kind: 'rrect', x: rx + av.aw + av.extW, y: av.ry + 2, w: av.extW, h: av.ah - 4, r: 2, color: 2 });
      out.push({ kind: 'rrect', x: lx + av.aw*0.4, y: av.ly + av.ah*0.5, w: av.aw*0.5, h: av.ah*0.4, r: 2, color: 1 });
      out.push({ kind: 'stroke-rrect', x: lx, y: av.ly, w: av.aw, h: av.ah, r: 4, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: rx, y: av.ry, w: av.aw + av.extW, h: av.ah, r: 4, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Head
function buildHead(hi: number): Shape[] {
  const hv = HEAD_VARIANTS[hi];
  const out: Shape[] = [];

  switch (hv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx + 10, cy: hv.cy + 8, rx: hv.rx * 0.45, ry: hv.ry * 0.35, color: 1 });
      out.push({ kind: 'ellipse', cx: hv.cx - 12, cy: hv.cy - 10, rx: 10, ry: 7, color: 6 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
    case 'square': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      out.push({ kind: 'rrect', x: hv.x + hv.w*0.4, y: hv.y + hv.h*0.5, w: hv.w*0.5, h: hv.h*0.35, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: hv.x + 10, cy: hv.y + 8, rx: 10, ry: 7, color: 6 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'helmet': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      out.push({ kind: 'rrect', x: hv.x, y: hv.visorY, w: hv.w, h: hv.visorH, r: 0, color: 2 });
      out.push({ kind: 'rrect', x: hv.x + hv.w*0.4, y: hv.y + hv.h*0.6, w: hv.w*0.5, h: hv.h*0.3, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: hv.x + 10, cy: hv.y + 8, rx: 10, ry: 7, color: 6 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'catear': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      out.push({ kind: 'rrect', x: hv.x, y: hv.y - hv.earH, w: hv.earW, h: hv.earH, r: 2, color: 2 });
      out.push({ kind: 'rrect', x: hv.x + hv.w - hv.earW, y: hv.y - hv.earH, w: hv.earW, h: hv.earH, r: 2, color: 2 });
      out.push({ kind: 'rrect', x: hv.x + hv.w*0.4, y: hv.y + hv.h*0.5, w: hv.w*0.5, h: hv.h*0.35, r: 2, color: 1 });
      out.push({ kind: 'ellipse', cx: hv.x + 10, cy: hv.y + 8, rx: 10, ry: 7, color: 6 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y - hv.earH, w: hv.earW, h: hv.earH, r: 2, color: 5, width: 1 });
      out.push({ kind: 'stroke-rrect', x: hv.x + hv.w - hv.earW, y: hv.y - hv.earH, w: hv.earW, h: hv.earH, r: 2, color: 5, width: 1 });
      break;
    }
    case 'crt': {
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      out.push({ kind: 'rrect', x: hv.x + 6, y: hv.y + 6, w: hv.w - 12, h: hv.h - 12, r: 4, color: 2 });
      out.push({ kind: 'rrect', x: hv.x + hv.w*0.5, y: hv.y + hv.h*0.55, w: hv.w*0.4, h: hv.h*0.3, r: 2, color: 3 });
      out.push({ kind: 'ellipse', cx: hv.x + 14, cy: hv.y + 10, rx: 10, ry: 7, color: 6 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'capsule': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'ellipse', cx: hv.cx + 8, cy: hv.cy + 10, rx: hv.rx * 0.45, ry: hv.ry * 0.35, color: 1 });
      out.push({ kind: 'ellipse', cx: hv.cx - 10, cy: hv.cy - 12, rx: 8, ry: 6, color: 6 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
  }
  return out;
}

// Cheeks
function buildCheeks(hi: number): Shape[] {
  const b = headBounds(hi);
  const cheekY = b.cy + b.ry * 0.3;
  const cheekDist = b.rx * 0.65;
  return [
    { kind: 'ellipse', cx: b.cx - cheekDist, cy: cheekY, rx: 6, ry: 4, color: 7 },
    { kind: 'ellipse', cx: b.cx + cheekDist, cy: cheekY, rx: 6, ry: 4, color: 7 },
  ];
}

// Eyes
function buildEyes(ei: number, hi: number, expression: Expression): Shape[] {
  const ev = EYES_VARIANTS[ei];
  const b = headBounds(hi);
  const eyeY = b.cy - b.ry * 0.1;

  if (expression === 'blink') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'line', x1: lx - 8, y1: eyeY, x2: lx + 8, y2: eyeY, color: 5, width: 2 },
      { kind: 'line', x1: rx2 - 8, y1: eyeY, x2: rx2 + 8, y2: eyeY, color: 5, width: 2 },
    ];
  }
  if (expression === 'happy') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'stroke-ellipse', cx: lx, cy: eyeY + 4, rx: 8, ry: 5, color: 5, width: 2 },
      { kind: 'stroke-ellipse', cx: rx2, cy: eyeY + 4, rx: 8, ry: 5, color: 5, width: 2 },
    ];
  }
  if (expression === 'sleep') {
    const lx = b.cx - b.rx * 0.35;
    const rx2 = b.cx + b.rx * 0.35;
    return [
      { kind: 'rrect', x: lx - 7, y: eyeY - 2, w: 14, h: 5, r: 2, color: 4 },
      { kind: 'rrect', x: rx2 - 7, y: eyeY - 2, w: 14, h: 5, r: 2, color: 4 },
      { kind: 'ellipse', cx: b.cx + 14, cy: eyeY - 12, rx: 3, ry: 3, color: 3 },
      { kind: 'ellipse', cx: b.cx + 20, cy: eyeY - 18, rx: 4, ry: 4, color: 3 },
    ];
  }

  // Normal expression -- variant-specific
  const out: Shape[] = [];
  switch (ev.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: ev.lx, cy: eyeY, rx: ev.er, ry: ev.er, color: 4 });
      out.push({ kind: 'ellipse', cx: ev.rx, cy: eyeY, rx: ev.er, ry: ev.er, color: 4 });
      out.push({ kind: 'ellipse', cx: ev.lx - 2, cy: eyeY - 2, rx: 2, ry: 2, color: 6 });
      out.push({ kind: 'ellipse', cx: ev.rx - 2, cy: eyeY - 2, rx: 2, ry: 2, color: 6 });
      out.push({ kind: 'stroke-ellipse', cx: ev.lx, cy: eyeY, rx: ev.er, ry: ev.er, color: 5, width: 1 });
      out.push({ kind: 'stroke-ellipse', cx: ev.rx, cy: eyeY, rx: ev.er, ry: ev.er, color: 5, width: 1 });
      break;
    }
    case 'led': {
      out.push({ kind: 'rrect', x: ev.lx - ev.ew/2, y: eyeY - ev.eh/2, w: ev.ew, h: ev.eh, r: 2, color: 4 });
      out.push({ kind: 'rrect', x: ev.rx - ev.ew/2, y: eyeY - ev.eh/2, w: ev.ew, h: ev.eh, r: 2, color: 4 });
      break;
    }
    case 'star': {
      out.push({ kind: 'line', x1: ev.lx - ev.sr, y1: eyeY, x2: ev.lx + ev.sr, y2: eyeY, color: 2, width: 3 });
      out.push({ kind: 'line', x1: ev.lx, y1: eyeY - ev.sr, x2: ev.lx, y2: eyeY + ev.sr, color: 2, width: 3 });
      out.push({ kind: 'line', x1: ev.rx - ev.sr, y1: eyeY, x2: ev.rx + ev.sr, y2: eyeY, color: 2, width: 3 });
      out.push({ kind: 'line', x1: ev.rx, y1: eyeY - ev.sr, x2: ev.rx, y2: eyeY + ev.sr, color: 2, width: 3 });
      break;
    }
    case 'heart': {
      out.push({ kind: 'ellipse', cx: ev.lx - 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
      out.push({ kind: 'ellipse', cx: ev.lx + 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
      out.push({ kind: 'rrect', x: ev.lx - 7, y: eyeY - 3, w: 14, h: 8, r: 0, color: 2 });
      out.push({ kind: 'ellipse', cx: ev.rx - 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
      out.push({ kind: 'ellipse', cx: ev.rx + 4, cy: eyeY - 3, rx: 5, ry: 5, color: 2 });
      out.push({ kind: 'rrect', x: ev.rx - 7, y: eyeY - 3, w: 14, h: 8, r: 0, color: 2 });
      break;
    }
    case 'drowsy': {
      out.push({ kind: 'rrect', x: ev.lx - ev.ew/2, y: eyeY - ev.eh/2, w: ev.ew, h: ev.eh, r: 2, color: 4 });
      out.push({ kind: 'rrect', x: ev.rx - ev.ew/2, y: eyeY - ev.eh/2, w: ev.ew, h: ev.eh, r: 2, color: 4 });
      out.push({ kind: 'line', x1: ev.lx - ev.ew/2, y1: eyeY - ev.eh/2, x2: ev.lx + ev.ew/2, y2: eyeY - ev.eh/2, color: 5, width: 2 });
      out.push({ kind: 'line', x1: ev.rx - ev.ew/2, y1: eyeY - ev.eh/2, x2: ev.rx + ev.ew/2, y2: eyeY - ev.eh/2, color: 5, width: 2 });
      break;
    }
    case 'scanner': {
      out.push({ kind: 'line', x1: ev.x1, y1: ev.ey, x2: ev.x2, y2: ev.ey, color: 2, width: 4 });
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
      out.push({ kind: 'rrect', x: av.dishX, y: av.dishY, w: av.dishW, h: av.dishH, r: 2, color: tipColor });
      out.push({ kind: 'stroke-rrect', x: av.dishX, y: av.dishY, w: av.dishW, h: av.dishH, r: 2, color: 5, width: 2 });
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
        out.push({ kind: 'line', x1: pts[i][0], y1: pts[i][1], x2: pts[i+1][0], y2: pts[i+1][1], color: tipColor, width: 3 });
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
    if (s.kind === 'ellipse' || s.kind === 'stroke-ellipse') return { ...s, cy: s.cy + offsetY };
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
    ...buildAntenna(spec.antenna, frame.antennaBlink).map(shift),
  ];
}

// drawRobot
function drawShape(ctx: CanvasRenderingContext2D, s: Shape, palette: ReadonlyArray<string>): void {
  ctx.save();
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
