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
  antennaBlink: boolean; // 헤어핀 반짝 (구 안테나 점멸 계약 유지)
}

// 2026-07-19 v5 — 픽셀 치비 "사람" (레퍼런스: 스타듀밸리풍 교복/후드 캐릭터).
// 32×32 셀(4px) 그리드에 절차 생성: 피부 타원 → 헤어(스타일별 앞머리/옆머리) →
// 의상/포즈/바지/신발 → 실루엣 1px 다크 아웃라인 자동 생성. 눈·입·볼은 소형 오버레이.
export type Shape = (
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number }
  | { kind: 'arc'; cx: number; cy: number; rx: number; ry: number; start: number; end: number; color: number; width: number }
) & { alpha?: number };

const CX = 16; // 그리드 중심 컬럼

// ── 셀 캔버스 ──
type Cells = Map<number, number>;
const key = (r: number, c: number): number => r * COLS + c;
function put(cells: Cells, r: number, c: number, color: number): void {
  if (r < 0 || c < 0 || r >= COLS || c >= COLS) return;
  cells.set(key(r, c), color);
}
function inEllipse(c: number, r: number, cx: number, cy: number, rx: number, ry: number): boolean {
  const dx = (c - cx) / rx;
  const dy = (r - cy) / ry;
  return dx * dx + dy * dy <= 1;
}

// ── 몸통/의상/포즈/하체 ──
function paintBody(cells: Cells, outfitIdx: number, poseIdx: number): void {
  const outfit = OUTFIT_VARIANTS[outfitIdx];
  const pose = POSE_VARIANTS[poseIdx];
  const torsoColor = outfit === 'shirt' || outfit === 'overalls' ? 8 : 0;

  // 몸통 (rows 16..23)
  for (let r = 16; r <= 23; r++) {
    const hw = 6 - (r - 16) * 0.06;
    for (let c = 0; c < COLS; c++) {
      if (Math.abs(c + 0.5 - CX) <= hw) put(cells, r, c, torsoColor);
    }
  }

  // 의상 디테일
  if (outfit === 'hoodie') {
    for (let c = 12; c <= 19; c++) put(cells, 15, c, 1);           // 후드 뒤판
    for (let r = 17; r <= 23; r++) put(cells, r, 16, 1);           // 지퍼선
    put(cells, 17, 14, 8); put(cells, 18, 14, 8);                  // 끈
    put(cells, 17, 17, 8); put(cells, 18, 17, 8);
  } else if (outfit === 'blazer') {
    for (let c = 14; c <= 17; c++) put(cells, 16, c, 8);           // 셔츠 V
    put(cells, 17, 15, 8); put(cells, 17, 16, 8);
    put(cells, 16, 16, 2); for (let r = 17; r <= 20; r++) put(cells, r, 16, 2); // 타이
    put(cells, 17, 13, 1); put(cells, 18, 14, 1);                  // 라펠
    put(cells, 17, 18, 1); put(cells, 18, 17, 1);
    put(cells, 21, 14, 2);                                          // 단추
  } else if (outfit === 'tshirt') {
    for (let c = 14; c <= 17; c++) put(cells, 16, c, 1);           // 목선
  } else if (outfit === 'sweater') {
    for (let c = 12; c <= 19; c++) put(cells, 16, c, 1);           // 카라
    for (let c = 11; c <= 20; c++) put(cells, 23, c, 1);           // 밑단 골지
  } else if (outfit === 'shirt') {
    put(cells, 16, 13, 0); put(cells, 16, 14, 0);                  // 카라 (의상색)
    put(cells, 16, 17, 0); put(cells, 16, 18, 0);
    put(cells, 18, 16, 1); put(cells, 20, 16, 1); put(cells, 22, 16, 1); // 단춧줄
  } else if (outfit === 'overalls') {
    for (let r = 19; r <= 23; r++) {
      const hw = 6 - (r - 16) * 0.06;
      for (let c = 0; c < COLS; c++) if (Math.abs(c + 0.5 - CX) <= hw) put(cells, r, c, 3); // 빕
    }
    for (let r = 16; r <= 18; r++) { put(cells, r, 12, 3); put(cells, r, 19, 3); }          // 멜빵끈
  }

  // 팔 (포즈) — 소매색: 셔츠/멜빵=흰 소매, 그 외 의상색
  const sleeve = outfit === 'shirt' || outfit === 'overalls' ? 8 : 0;
  const shortSleeve = outfit === 'tshirt';
  const armDown = (side: -1 | 1): void => {
    const c0 = side < 0 ? 8 : 22;
    const end = shortSleeve ? 19 : 21;
    for (let r = 17; r <= end; r++) { put(cells, r, c0, sleeve); put(cells, r, c0 + 1, sleeve); }
    if (shortSleeve) {
      for (let r = 20; r <= 21; r++) { put(cells, r, c0, 6); put(cells, r, c0 + 1, 6); }     // 맨팔
    }
    put(cells, 22, c0, 6); put(cells, 22, c0 + 1, 6);                                        // 손
  };

  switch (pose) {
    case 'down': {
      armDown(-1); armDown(1);
      break;
    }
    case 'pockets': {
      const seq: ReadonlyArray<readonly [number, number]> = [[17, 8], [17, 9], [18, 9], [18, 10], [19, 10], [19, 11], [20, 11], [20, 12]];
      for (const [r, c] of seq) { put(cells, r, c, sleeve); put(cells, r, c + (c < CX ? 1 : -1), sleeve); }
      for (const [r, c] of seq) { const m = COLS - 1 - c; put(cells, r, m, sleeve); put(cells, r, m + (m < CX ? 1 : -1), sleeve); }
      if (outfit === 'hoodie') for (let c = 13; c <= 18; c++) put(cells, 21, c, 1);          // 캥거루 포켓
      break;
    }
    case 'wave': {
      armDown(-1);
      put(cells, 17, 22, sleeve); put(cells, 16, 22, sleeve); put(cells, 15, 23, sleeve); put(cells, 14, 23, sleeve);
      put(cells, 13, 23, 6); put(cells, 13, 24, 6);                                          // 흔드는 손
      break;
    }
    case 'front': {
      for (let r = 17; r <= 20; r++) { put(cells, r, 8, sleeve); put(cells, r, 9, sleeve); put(cells, r, 22, sleeve); put(cells, r, 23, sleeve); }
      put(cells, 21, 13, 6); put(cells, 21, 14, 6); put(cells, 21, 17, 6); put(cells, 21, 18, 6); // 앞모은 손
      break;
    }
    case 'bag': {
      armDown(-1); armDown(1);
      for (let c = 25; c <= 28; c++) put(cells, 22, c, 2);                                   // 바구니 테
      for (let r = 23; r <= 25; r++) for (let c = 25; c <= 28; c++) put(cells, r, c, (r + c) % 2 ? 2 : 1);
      put(cells, 21, 24, 2);                                                                  // 손잡이
      break;
    }
    case 'behind': {
      for (let r = 17; r <= 19; r++) { put(cells, r, 8, sleeve); put(cells, r, 9, sleeve); put(cells, r, 22, sleeve); put(cells, r, 23, sleeve); }
      break;
    }
  }

  // 바지 (rows 24..27) + 신발 (28..29)
  for (let c = 0; c < COLS; c++) if (Math.abs(c + 0.5 - CX) <= 5.5) put(cells, 24, c, 3);
  for (let r = 25; r <= 27; r++) {
    for (const c of [12, 13, 14, 17, 18, 19]) put(cells, r, c, 3);
  }
  for (const c of [11, 12, 13, 14]) put(cells, 28, c, 8);
  for (const c of [17, 18, 19, 20]) put(cells, 28, c, 8);
  for (const c of [11, 12, 13, 14, 17, 18, 19, 20]) put(cells, 29, c, 8);
}

// ── 얼굴 피부 + 헤어 ──
function paintHead(cells: Cells, faceIdx: number, hairIdx: number, blinkPin: boolean): void {
  const f = FACE_VARIANTS[faceIdx];
  const h = HAIR_VARIANTS[hairIdx];
  const top = f.cy - f.ry;

  // 피부
  for (let r = 0; r < COLS; r++) {
    for (let c = 0; c < COLS; c++) {
      if (inEllipse(c + 0.5, r + 0.5, CX, f.cy, f.rx, f.ry)) put(cells, r, c, 6);
    }
  }
  // 헤어 — 앞머리(hairline) + 옆머리 + 스타일 액센트
  const n = h.fringe.length;
  for (let r = 0; r < COLS; r++) {
    for (let c = 0; c < COLS; c++) {
      if (!inEllipse(c + 0.5, r + 0.5, CX, f.cy - 0.15, f.rx + 0.45, f.ry + 0.55)) continue;
      const fr = h.fringe[((c % n) + n) % n];
      const hairline = top + h.bang + fr + h.slant * (c + 0.5 - CX);
      const side = Math.abs(c + 0.5 - CX) >= f.rx - 1.8 && r + 0.5 <= f.cy + h.ear;
      if (r + 0.5 <= hairline || side) put(cells, r, c, 4);
    }
  }
  for (const [dx, dy] of h.spikes) put(cells, Math.round(top + dy), CX + dx, 4);
  if (h.strands) {
    const cL = Math.round(CX - f.rx - 0.4);
    const cR = Math.round(CX + f.rx + 0.4);
    for (let r = Math.round(f.cy); r <= 15; r++) { put(cells, r, cL, 4); put(cells, r, cR, 4); }
    put(cells, 15, cL + 1, 4); put(cells, 15, cR - 1, 4);
  }
  if (blinkPin) put(cells, Math.round(top + 1), CX + 4, 2); // 헤어핀 반짝
  // 볼터치 (픽셀)
  const br = Math.round(f.cy + 2.2);
  put(cells, br, Math.round(CX - f.rx + 1.6), 7); put(cells, br, Math.round(CX - f.rx + 2.6), 7);
  put(cells, br, Math.round(CX + f.rx - 2.6), 7); put(cells, br, Math.round(CX + f.rx - 3.6), 7);
}

// ── 실루엣 아웃라인 (1셀 다크 보더 — 레퍼런스의 픽셀 셀아웃) ──
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
  for (const k of border) cells.set(k, 5);
}

// ── 눈·입 오버레이 (표정 포함) ──
function paintFace(faceIdx: number, eyesIdx: number, expression: Expression): Shape[] {
  const f = FACE_VARIANTS[faceIdx];
  const out: Shape[] = [];
  const ex = f.rx * CELL * 0.45;
  const eyeL = CX * CELL - ex;
  const eyeR = CX * CELL + ex;
  const eyeY = (f.cy + 0.7) * CELL;
  const mouthY = (f.cy + f.ry * 0.56) * CELL;

  if (expression === 'blink') {
    out.push({ kind: 'line', x1: eyeL - 3, y1: eyeY, x2: eyeL + 3, y2: eyeY, color: 5, width: 2 });
    out.push({ kind: 'line', x1: eyeR - 3, y1: eyeY, x2: eyeR + 3, y2: eyeY, color: 5, width: 2 });
    out.push({ kind: 'arc', cx: CX * CELL, cy: mouthY - 1, rx: 3, ry: 2, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: 5, width: 1.6 });
    return out;
  }
  if (expression === 'happy') {
    for (const cx of [eyeL, eyeR]) {
      out.push({ kind: 'arc', cx, cy: eyeY + 1.5, rx: 3.6, ry: 3, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 2 });
    }
    out.push({ kind: 'arc', cx: CX * CELL, cy: mouthY - 2, rx: 5, ry: 3.5, start: 0.12 * Math.PI, end: 0.88 * Math.PI, color: 5, width: 2 });
    return out;
  }
  if (expression === 'sleep') {
    for (const cx of [eyeL, eyeR]) {
      out.push({ kind: 'arc', cx, cy: eyeY - 1, rx: 3.6, ry: 2.2, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 5, width: 2 });
    }
    out.push({ kind: 'stroke-ellipse', cx: CX * CELL, cy: mouthY, rx: 1.6, ry: 2, color: 5, width: 1.4 });
    out.push({ kind: 'ellipse', cx: (CX + f.rx) * CELL, cy: (f.cy - f.ry) * CELL, rx: 2.5, ry: 2.5, color: 3, alpha: 0.9 });
    out.push({ kind: 'ellipse', cx: (CX + f.rx + 1.6) * CELL, cy: (f.cy - f.ry - 1.6) * CELL, rx: 3.2, ry: 3.2, color: 3, alpha: 0.9 });
    return out;
  }

  const style = EYE_VARIANTS[eyesIdx];
  const eye = (cx: number): void => {
    switch (style) {
      case 'oval':
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 3, ry: 4.2, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1, cy: eyeY - 1.4, rx: 1.1, ry: 1.1, color: 8 });
        break;
      case 'round':
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 3.8, ry: 3.8, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1.2, cy: eyeY - 1.2, rx: 1.3, ry: 1.3, color: 8 });
        break;
      case 'calm':
        out.push({ kind: 'ellipse', cx, cy: eyeY + 0.5, rx: 3.4, ry: 2.2, color: 5 });
        break;
      case 'sparkle':
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 3, ry: 4, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1, cy: eyeY - 1.4, rx: 1.2, ry: 1.2, color: 8 });
        out.push({ kind: 'ellipse', cx: cx + 1.2, cy: eyeY + 1.4, rx: 0.7, ry: 0.7, color: 8 });
        break;
      case 'droopy':
        out.push({ kind: 'ellipse', cx, cy: eyeY + 1, rx: 3, ry: 3.4, color: 5 });
        out.push({ kind: 'ellipse', cx: cx - 1, cy: eyeY - 0.2, rx: 1, ry: 1, color: 8 });
        break;
      case 'happy':
        out.push({ kind: 'arc', cx, cy: eyeY + 1.5, rx: 3.4, ry: 3, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 5, width: 2 });
        break;
    }
  };
  eye(eyeL);
  eye(eyeR);
  out.push({ kind: 'arc', cx: CX * CELL, cy: mouthY - 1, rx: 3, ry: 2, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: 5, width: 1.6 });
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
    { kind: 'ellipse', cx: 64, cy: 122, rx: 24, ry: 3.5, color: 5, alpha: 0.1 },
    { kind: 'ellipse', cx: 64, cy: 122, rx: 16, ry: 2.8, color: 5, alpha: 0.12 },
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

// drawRobot — 하드 픽셀 (스무딩 없는 사각 셀)
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
