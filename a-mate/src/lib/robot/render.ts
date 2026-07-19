import {
  GRID, CELL, COLS, PALETTES, FACE_VARIANTS, HAIR_VARIANTS, EYE_VARIANTS, OUTFIT_VARIANTS, POSE_VARIANTS,
} from './parts';

export interface RobotSpec {
  antenna: number; // 헤어스타일
  head: number;    // 얼굴형
  eyes: number;
  body: number;    // 의상
  arms: number;    // 포즈
  palette: number;
}

export type Expression = 'normal' | 'blink' | 'happy' | 'sleep';

export interface Frame {
  offsetY: number;
  expression: Expression;
  antennaBlink: boolean; // 헤어핀 반짝 (구 계약 유지)
}

// 2026-07-19 v6 — 레퍼런스 "그대로 차용" (사용자 제공 픽셀 치비 이미지 실측):
//   머리 ~40% / 몸통 / 긴 다리 / 신발 · 64그리드(셀 2px) · 팔은 실루엣 안 음영 밴드 ·
//   2톤 셰이딩(헤어 언더뱅·의상) · 큰 타원 눈 + 하이라이트 · 실루엣 1셀 아웃라인.
export type Shape = (
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number }
  | { kind: 'arc'; cx: number; cy: number; rx: number; ry: number; start: number; end: number; color: number; width: number }
) & { alpha?: number };

const CX = 32;
/** 실루엣 아웃라인 전용 색 인덱스 (팔레트 밖 상수) */
export const OUTLINE_IDX = 9;
export const OUTLINE_COLOR = '#241f26';

// 신체 레이아웃 (셀 rows): 머리 5..29 / 몸통 29..45 / 다리 45..55 / 신발 55..59
const T0 = 29, T1 = 45;
const L0 = 45, L1 = 55;
const S0 = 55, S1 = 59;
const THW = 14;

type Cells = Map<number, number>;
const key = (r: number, c: number): number => r * COLS + c;
function put(cells: Cells, r: number, c: number, color: number): void {
  if (r < 0 || c < 0 || r >= COLS || c >= COLS) return;
  cells.set(key(r, c), color);
}

function paintBody(cells: Cells, outfitIdx: number, poseIdx: number): void {
  const outfit = OUTFIT_VARIANTS[outfitIdx];
  const pose = POSE_VARIANTS[poseIdx];
  const torso = outfit === 'shirt' || outfit === 'overalls' ? 8 : 0;

  // 몸통 (어깨 슬로프 → 직선)
  for (let r = T0; r < T1; r++) {
    const hw = Math.min(THW, 11 + (r - T0) * 1.6);
    for (let c = 0; c < COLS; c++) {
      if (Math.abs(c + 0.5 - CX) <= hw) put(cells, r, c, torso);
    }
  }
  // 팔 힌트 — 실루엣 안 음영 밴드 (레퍼런스 방식: 팔이 밖으로 튀지 않는다)
  for (let r = T0 + 3; r < T1 - 1; r++) {
    for (const dc of [0, 1, 2]) {
      put(cells, r, CX - THW + dc, 1);
      put(cells, r, CX + THW - 1 - dc, 1);
    }
  }

  // 의상 디테일
  if (outfit === 'hoodie') {
    for (let c = CX - 9; c <= CX + 9; c++) { put(cells, T0, c, 1); put(cells, T0 + 1, c, 1); }
    for (let r = T0 + 2; r < T1; r++) put(cells, r, CX, 1);
    for (let r = T0 + 2; r < T0 + 9; r++) { put(cells, r, CX - 4, 8); put(cells, r, CX + 4, 8); }
    put(cells, T0 + 9, CX - 4, 8); put(cells, T0 + 9, CX + 4, 8);
  } else if (outfit === 'blazer') {
    const vw = [5, 4, 3, 2, 1];
    for (let i = 0; i < vw.length; i++) {
      for (let c = CX - vw[i]; c < CX + vw[i]; c++) put(cells, T0 + i, c, 8);
    }
    put(cells, T0, CX - 1, 2); put(cells, T0, CX, 2);
    for (let r = T0 + 1; r < T0 + 10; r++) { put(cells, r, CX - 1, 2); put(cells, r, CX, 2); }
    put(cells, T0 + 10, CX - 1, 2);
    for (let i = 0; i < 5; i++) {
      put(cells, T0 + i, CX - 6 - Math.floor(i / 2), 1);
      put(cells, T0 + i, CX + 5 + Math.floor(i / 2), 1);
    }
    put(cells, T0 + 11, CX - 5, 2); put(cells, T0 + 13, CX - 5, 2);
  } else if (outfit === 'tshirt') {
    for (let c = CX - 5; c < CX + 5; c++) put(cells, T0, c, 1);
    for (let r = T0 + 7; r < T1 - 1; r++) {
      for (const dc of [0, 1, 2]) {
        put(cells, r, CX - THW + dc, 6);
        put(cells, r, CX + THW - 1 - dc, 6);
      }
    }
  } else if (outfit === 'sweater') {
    for (let c = CX - 10; c < CX + 10; c++) { put(cells, T0, c, 1); put(cells, T0 + 1, c, 1); }
    for (let c = 0; c < COLS; c++) {
      if (Math.abs(c + 0.5 - CX) <= THW) { put(cells, T1 - 1, c, 1); put(cells, T1 - 2, c, 1); }
    }
  } else if (outfit === 'shirt') {
    put(cells, T0, CX - 4, 0); put(cells, T0, CX - 3, 0); put(cells, T0 + 1, CX - 2, 0);
    put(cells, T0, CX + 3, 0); put(cells, T0, CX + 2, 0); put(cells, T0 + 1, CX + 1, 0);
    for (let r = T0 + 3; r < T1; r += 4) put(cells, r, CX, 1);
  } else if (outfit === 'overalls') {
    for (let r = T0 + 6; r < T1; r++) {
      const hw = Math.min(THW, 11 + (r - T0) * 1.6);
      for (let c = 0; c < COLS; c++) if (Math.abs(c + 0.5 - CX) <= hw) put(cells, r, c, 3);
    }
    for (let r = T0; r < T0 + 6; r++) {
      put(cells, r, CX - 7, 3); put(cells, r, CX - 6, 3);
      put(cells, r, CX + 6, 3); put(cells, r, CX + 5, 3);
    }
    put(cells, T0 + 6, CX - 7, 2); put(cells, T0 + 6, CX + 6, 2);
  }

  // 포즈
  const sleeve = outfit === 'shirt' || outfit === 'overalls' ? 8 : 0;
  switch (pose) {
    case 'down': {
      for (let r = T1 - 3; r < T1; r++) {
        put(cells, r, CX - THW + 1, 6); put(cells, r, CX - THW + 2, 6);
        put(cells, r, CX + THW - 2, 6); put(cells, r, CX + THW - 3, 6);
      }
      break;
    }
    case 'pockets': {
      for (const dc of [3, 4, 5, 6]) {
        put(cells, T1 - 5, CX - THW + dc, 1);
        put(cells, T1 - 5, CX + THW - 1 - dc, 1);
      }
      break;
    }
    case 'wave': {
      for (let r = T1 - 3; r < T1; r++) { put(cells, r, CX - THW + 1, 6); put(cells, r, CX - THW + 2, 6); }
      const arm: ReadonlyArray<readonly [number, number]> = [
        [T0 + 4, CX + THW], [T0 + 3, CX + THW + 1], [T0 + 2, CX + THW + 1], [T0 + 1, CX + THW + 2], [T0, CX + THW + 2],
      ];
      for (const [r, c] of arm) { put(cells, r, c, sleeve); put(cells, r, c + 1, sleeve); }
      put(cells, T0 - 1, CX + THW + 2, 6); put(cells, T0 - 1, CX + THW + 3, 6); put(cells, T0 - 2, CX + THW + 3, 6);
      break;
    }
    case 'front': {
      for (let c = CX - 3; c < CX + 3; c++) { put(cells, T1 - 3, c, 6); put(cells, T1 - 2, c, 6); }
      break;
    }
    case 'bag': {
      for (let r = T1 - 3; r < T1; r++) { put(cells, r, CX - THW + 1, 6); put(cells, r, CX - THW + 2, 6); }
      put(cells, T1 - 3, CX + THW - 2, 6); put(cells, T1 - 3, CX + THW - 1, 6);
      const b0 = CX + THW - 1;
      for (let c = b0; c < b0 + 9; c++) put(cells, T1 - 1, c, 2);
      for (let r = T1; r < T1 + 6; r++) {
        for (let c = b0; c < b0 + 9; c++) put(cells, r, c, (r + c) % 2 ? 2 : 1);
      }
      put(cells, T1 - 3, b0 + 3, 2); put(cells, T1 - 4, b0 + 4, 2); put(cells, T1 - 3, b0 + 5, 2);
      break;
    }
    case 'behind':
      break; // 뒷짐 — 팔 음영만
  }

  // 다리 + 신발
  for (let r = L0; r < L1; r++) {
    for (let c = CX - 10; c < CX - 1; c++) put(cells, r, c, 3);
    for (let c = CX + 1; c < CX + 10; c++) put(cells, r, c, 3);
  }
  for (let r = S0; r < S1; r++) {
    for (let c = CX - 11; c < CX - 1; c++) put(cells, r, c, 8);
    for (let c = CX + 1; c < CX + 11; c++) put(cells, r, c, 8);
  }
}

function paintHead(cells: Cells, faceIdx: number, hairIdx: number, blinkPin: boolean): void {
  const f = FACE_VARIANTS[faceIdx];
  const h = HAIR_VARIANTS[hairIdx];
  const top = f.cy - f.ry;

  // 피부
  for (let r = 0; r < COLS; r++) {
    for (let c = 0; c < COLS; c++) {
      const dx = (c + 0.5 - CX) / f.rx;
      const dy = (r + 0.5 - f.cy) / f.ry;
      if (dx * dx + dy * dy <= 1) put(cells, r, c, 6);
    }
  }
  // 헤어
  const n = h.fringe.length;
  for (let r = 0; r < COLS; r++) {
    for (let c = 0; c < COLS; c++) {
      const dx = (c + 0.5 - CX) / (f.rx + 0.8);
      const dy = (r + 0.5 - (f.cy - 0.3)) / (f.ry + 1.0);
      if (dx * dx + dy * dy > 1) continue;
      const fr = h.fringe[((c % n) + n) % n];
      const hairline = top + h.bang + fr + h.slant * (c + 0.5 - CX);
      const side = Math.abs(c + 0.5 - CX) >= f.rx - 3.2 && r + 0.5 <= f.cy + h.ear;
      if (r + 0.5 <= hairline || side) put(cells, r, c, 4);
    }
  }
  // 언더뱅 셰이드 (앞머리가 피부와 만나는 밑단을 어둡게 — 레퍼런스 음영)
  for (let c = 0; c < COLS; c++) {
    for (let r = 0; r < COLS - 1; r++) {
      if (cells.get(key(r, c)) === 4 && cells.get(key(r + 1, c)) === 6) put(cells, r, c, 5);
    }
  }
  for (const [dx, dy] of h.spikes) {
    put(cells, Math.round(top + dy), CX + dx, 4);
    put(cells, Math.round(top + dy) + 1, CX + dx, 4);
  }
  if (h.strands) {
    const cL = Math.round(CX - f.rx - 1);
    const cR = Math.round(CX + f.rx);
    for (let r = Math.round(f.cy); r < T0 + 4; r++) {
      put(cells, r, cL, 4); put(cells, r, cL + 1, 4);
      put(cells, r, cR, 4); put(cells, r, cR + 1, 4);
    }
    for (const c of [cL, cL + 1, cR, cR + 1]) put(cells, T0 + 4, c, 5);
  }
  if (blinkPin) { put(cells, Math.round(top + 2), CX + 8, 2); put(cells, Math.round(top + 3), CX + 8, 2); } // 헤어핀
  // 볼터치
  const br = Math.round(f.cy + 4.5);
  put(cells, br, Math.round(CX - f.rx + 4), 7); put(cells, br, Math.round(CX - f.rx + 5), 7);
  put(cells, br, Math.round(CX + f.rx - 5), 7); put(cells, br, Math.round(CX + f.rx - 6), 7);
}

function outline(cells: Cells): void {
  const border: number[] = [];
  for (let r = 0; r < COLS; r++) {
    for (let c = 0; c < COLS; c++) {
      if (cells.has(key(r, c))) continue;
      if (
        (r > 0 && cells.has(key(r - 1, c))) || (r < COLS - 1 && cells.has(key(r + 1, c))) ||
        (c > 0 && cells.has(key(r, c - 1))) || (c < COLS - 1 && cells.has(key(r, c + 1)))
      ) border.push(key(r, c));
    }
  }
  for (const k of border) cells.set(k, OUTLINE_IDX);
}

function paintFace(faceIdx: number, eyesIdx: number, expression: Expression): Shape[] {
  const f = FACE_VARIANTS[faceIdx];
  const out: Shape[] = [];
  const ex = f.rx * CELL * 0.48;
  const eL = CX * CELL - ex;
  const eR = CX * CELL + ex;
  const ey = (f.cy + 1.8) * CELL;
  const my = (f.cy + f.ry * 0.62) * CELL;

  if (expression === 'blink') {
    out.push({ kind: 'line', x1: eL - 4, y1: ey, x2: eL + 4, y2: ey, color: OUTLINE_IDX, width: 2.4 });
    out.push({ kind: 'line', x1: eR - 4, y1: ey, x2: eR + 4, y2: ey, color: OUTLINE_IDX, width: 2.4 });
    out.push({ kind: 'arc', cx: CX * CELL, cy: my - 2, rx: 4, ry: 3, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: OUTLINE_IDX, width: 2 });
    return out;
  }
  if (expression === 'happy') {
    for (const cx of [eL, eR]) {
      out.push({ kind: 'arc', cx, cy: ey + 2, rx: 4.4, ry: 3.6, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: OUTLINE_IDX, width: 2.4 });
    }
    out.push({ kind: 'arc', cx: CX * CELL, cy: my - 3, rx: 6, ry: 4.5, start: 0.12 * Math.PI, end: 0.88 * Math.PI, color: OUTLINE_IDX, width: 2.4 });
    return out;
  }
  if (expression === 'sleep') {
    for (const cx of [eL, eR]) {
      out.push({ kind: 'arc', cx, cy: ey - 1, rx: 4.4, ry: 2.8, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: OUTLINE_IDX, width: 2.4 });
    }
    out.push({ kind: 'stroke-ellipse', cx: CX * CELL, cy: my, rx: 2, ry: 2.4, color: OUTLINE_IDX, width: 1.6 });
    out.push({ kind: 'ellipse', cx: (CX + f.rx) * CELL, cy: (f.cy - f.ry) * CELL, rx: 2.5, ry: 2.5, color: 3, alpha: 0.9 });
    out.push({ kind: 'ellipse', cx: (CX + f.rx + 2.2) * CELL, cy: (f.cy - f.ry - 2.2) * CELL, rx: 3.2, ry: 3.2, color: 3, alpha: 0.9 });
    return out;
  }

  const style = EYE_VARIANTS[eyesIdx];
  const eye = (cx: number): void => {
    if (style === 'calm') {
      out.push({ kind: 'ellipse', cx, cy: ey + 0.5, rx: 4, ry: 3, color: OUTLINE_IDX });
      out.push({ kind: 'ellipse', cx: cx - 1.4, cy: ey - 0.6, rx: 1, ry: 1, color: 8 });
    } else if (style === 'happy') {
      out.push({ kind: 'arc', cx, cy: ey + 2, rx: 4, ry: 3.5, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: OUTLINE_IDX, width: 2.4 });
    } else {
      let rx = 4, ry = 5.6, off = 0;
      if (style === 'round') { rx = 4.6; ry = 4.6; }
      if (style === 'droopy') { ry = 4.8; off = 1.2; }
      out.push({ kind: 'ellipse', cx, cy: ey + off, rx, ry, color: OUTLINE_IDX });
      out.push({ kind: 'ellipse', cx: cx - 1.4, cy: ey - 2.2 + off, rx: 1.2, ry: 1.2, color: 8 });
      if (style === 'sparkle') out.push({ kind: 'ellipse', cx: cx + 1.7, cy: ey + 1.9 + off, rx: 0.9, ry: 0.9, color: 8 });
    }
  };
  eye(eL);
  eye(eR);
  out.push({ kind: 'arc', cx: CX * CELL, cy: my - 2, rx: 4, ry: 3, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: OUTLINE_IDX, width: 2 });
  return out;
}

// buildRobotShapes (pure, deterministic)
export function buildRobotShapes(spec: RobotSpec, frame: Frame): Shape[] {
  const { offsetY } = frame;

  const cells: Cells = new Map();
  paintBody(cells, spec.body, spec.arms);
  paintHead(cells, spec.head, spec.antenna, frame.antennaBlink);
  outline(cells);

  const out: Shape[] = [
    { kind: 'ellipse', cx: 64, cy: 123, rx: 28, ry: 3.5, color: OUTLINE_IDX, alpha: 0.1 },
    { kind: 'ellipse', cx: 64, cy: 123, rx: 20, ry: 2.8, color: OUTLINE_IDX, alpha: 0.12 },
  ];
  for (const [k, color] of cells) {
    const r = Math.floor(k / COLS);
    const c = k % COLS;
    out.push({ kind: 'rrect', x: c * CELL, y: r * CELL + offsetY, w: CELL, h: CELL, r: 0, color });
  }
  for (const s of paintFace(spec.head, spec.eyes, frame.expression)) {
    if (s.kind === 'ellipse' || s.kind === 'stroke-ellipse' || s.kind === 'arc') out.push({ ...s, cy: s.cy + offsetY });
    else if (s.kind === 'line') out.push({ ...s, y1: s.y1 + offsetY, y2: s.y2 + offsetY });
    else if (s.kind === 'rrect' || s.kind === 'stroke-rrect') out.push({ ...s, y: s.y + offsetY });
  }
  return out;
}

// drawRobot — 하드 픽셀
function drawShape(ctx: CanvasRenderingContext2D, s: Shape, palette: ReadonlyArray<string>): void {
  ctx.save();
  if (s.alpha !== undefined) ctx.globalAlpha = s.alpha;
  const color = s.color === OUTLINE_IDX ? OUTLINE_COLOR : palette[s.color];
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
    if (s.r > 0) {
      ctx.beginPath();
      ctx.roundRect(s.x, s.y, s.w, s.h, s.r);
      ctx.fill();
    } else {
      ctx.fillRect(s.x, s.y, s.w, s.h);
    }
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
