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

// 2026-07-19 v4 — 미니멀 큐트 로봇 (웹 레퍼런스 기반 전면 재설계):
//   카와이 공식: 둥근 단순 실루엣 · 대두+꼬마몸 · 파스텔 · 플랫 컬러(장식 금지).
//   스크린 얼굴 공식(Eve·Cozmo·BMO): 어두운 스크린 + 빛나는 단순한 눈 + 작은 미소.
//   표정은 전부 스크린 위 최소 요소로. 패널·줄무늬·림·하이라이트 등 장식 전면 제거.
export type Shape = (
  | { kind: 'ellipse'; cx: number; cy: number; rx: number; ry: number; color: number }
  | { kind: 'rrect'; x: number; y: number; w: number; h: number; r: number; color: number }
  | { kind: 'stroke-rrect'; x: number; y: number; w: number; h: number; r: number; color: number; width: number }
  | { kind: 'stroke-ellipse'; cx: number; cy: number; rx: number; ry: number; color: number; width: number }
  | { kind: 'line'; x1: number; y1: number; x2: number; y2: number; color: number; width: number }
  | { kind: 'arc'; cx: number; cy: number; rx: number; ry: number; start: number; end: number; color: number; width: number }
) & { alpha?: number };

// Helper: head bounds
function headBounds(hi: number): { cx: number; cy: number; rx: number; ry: number } {
  const hv = HEAD_VARIANTS[hi];
  if (hv.kind === 'round' || hv.kind === 'capsule') {
    return { cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry };
  } else {
    const h = hv as { x: number; y: number; w: number; h: number };
    return { cx: h.x + h.w / 2, cy: h.y + h.h / 2, rx: h.w / 2, ry: h.h / 2 };
  }
}

// 스크린 — 얼굴의 전부. 머리 크기에서 유도 (모든 머리 변형에서 일관).
function screenRect(hi: number): { x: number; y: number; w: number; h: number; r: number; cx: number; cy: number } {
  const b = headBounds(hi);
  const w = b.rx * 1.5;
  const h = b.ry * 1.12;
  const cx = b.cx;
  const cy = b.cy - b.ry * 0.02;
  return { x: cx - w / 2, y: cy - h / 2, w, h, r: Math.min(w, h) * 0.32, cx, cy };
}

// Shadow — 저투명 2겹
function buildShadow(): Shape[] {
  return [
    { kind: 'ellipse', cx: 64, cy: 119, rx: 26, ry: 4.5, color: 5, alpha: 0.08 },
    { kind: 'ellipse', cx: 64, cy: 119, rx: 18, ry: 3.5, color: 5, alpha: 0.12 },
  ];
}

// 발 — 아주 작은 너브 2개
function buildFeet(): Shape[] {
  const out: Shape[] = [];
  for (const cx of [54, 74]) {
    out.push({ kind: 'rrect', x: cx - 6, y: 105, w: 12, h: 9, r: 4.5, color: 0 });
    out.push({ kind: 'stroke-rrect', x: cx - 6, y: 105, w: 12, h: 9, r: 4.5, color: 5, width: 2 });
  }
  return out;
}

// 몸 — 플랫 실루엣. 장식은 가슴 불빛 1개 이하.
function buildBody(bi: number): Shape[] {
  const bv = BODY_VARIANTS[bi];
  const out: Shape[] = [];

  switch (bv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'box': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      // 가슴 불빛 하나 (유일한 장식)
      out.push({ kind: 'ellipse', cx: bv.ledX + bv.ledW / 2, cy: bv.ledY + bv.ledH / 2, rx: bv.ledW / 2, ry: bv.ledH / 2, color: 2, alpha: 0.9 });
      break;
    }
    case 'barrel': {
      out.push({ kind: 'ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: bv.cx, cy: bv.cy, rx: bv.rx, ry: bv.ry, color: 5, width: 2 });
      break;
    }
    case 'vest': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'pocket': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      break;
    }
    case 'striped': {
      out.push({ kind: 'rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: bv.x, y: bv.y, w: bv.w, h: bv.h, r: bv.r, color: 5, width: 2 });
      // 허리선 하나 (유일한 장식)
      out.push({ kind: 'line', x1: bv.x + 8, y1: bv.stripeY1, x2: bv.x + bv.w - 8, y2: bv.stripeY1, color: 2, width: 2, alpha: 0.6 });
      break;
    }
  }
  return out;
}

// 팔 — 작은 너브
function buildArms(ai: number): Shape[] {
  const av = ARMS_VARIANTS[ai];
  const out: Shape[] = [];
  const lx = 30, rx = 89;

  const nub = (x: number, y: number, w: number, h: number, color = 0): void => {
    out.push({ kind: 'rrect', x, y, w, h, r: w / 2, color });
    out.push({ kind: 'stroke-rrect', x, y, w, h, r: w / 2, color: 5, width: 2 });
  };

  switch (av.kind) {
    case 'down': {
      nub(lx, av.ly, av.aw, av.ah);
      nub(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'up': {
      nub(lx - 2, av.ly, av.aw, av.ah);
      nub(rx + 2, av.ry, av.aw, av.ah);
      break;
    }
    case 'pincer': {
      nub(lx, av.ly, av.aw, av.ah);
      nub(rx, av.ry, av.aw, av.ah);
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
      nub(lx - 2, av.ly, av.aw, av.ah);
      nub(rx, av.ry, av.aw, av.ah);
      break;
    }
    case 'rocket': {
      nub(lx, av.ly, av.aw, av.ah);
      nub(rx, av.ry, av.aw + av.extW, av.ah);
      break;
    }
  }
  return out;
}

// 머리 — 플랫 실루엣 + 스크린. 장식 없음 (실루엣 차이만).
function buildHead(hi: number): Shape[] {
  const hv = HEAD_VARIANTS[hi];
  const out: Shape[] = [];

  switch (hv.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
    case 'square':
    case 'helmet':
    case 'crt': {
      const h = hv as { x: number; y: number; w: number; h: number; r: number };
      out.push({ kind: 'rrect', x: h.x, y: h.y, w: h.w, h: h.h, r: h.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: h.x, y: h.y, w: h.w, h: h.h, r: h.r, color: 5, width: 2 });
      break;
    }
    case 'catear': {
      for (const ecx of [hv.x + 16, hv.x + hv.w - 16]) {
        out.push({ kind: 'ellipse', cx: ecx, cy: hv.y - 4, rx: hv.earW / 2, ry: hv.earH / 2 + 2, color: 0 });
        out.push({ kind: 'stroke-ellipse', cx: ecx, cy: hv.y - 4, rx: hv.earW / 2, ry: hv.earH / 2 + 2, color: 5, width: 2 });
      }
      out.push({ kind: 'rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 0 });
      out.push({ kind: 'stroke-rrect', x: hv.x, y: hv.y, w: hv.w, h: hv.h, r: hv.r, color: 5, width: 2 });
      break;
    }
    case 'capsule': {
      out.push({ kind: 'ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 0 });
      out.push({ kind: 'stroke-ellipse', cx: hv.cx, cy: hv.cy, rx: hv.rx, ry: hv.ry, color: 5, width: 2 });
      break;
    }
  }

  // 스크린 — 얼굴의 무대 (아웃라인 없는 플랫 다크)
  const s = screenRect(hi);
  out.push({ kind: 'rrect', x: s.x, y: s.y, w: s.w, h: s.h, r: s.r, color: 5, alpha: 0.92 });
  return out;
}

// 얼굴 — 스크린 위 빛나는 최소 요소: 눈 + 작은 미소 + 볼터치.
function buildFace(ei: number, hi: number, expression: Expression): Shape[] {
  const s = screenRect(hi);
  const out: Shape[] = [];
  const exL = s.cx - s.w * 0.22;
  const exR = s.cx + s.w * 0.22;
  const eyeY = s.cy - s.h * 0.1;
  const smileY = s.cy + s.h * 0.26;

  // 볼터치 — 스크린 위 핑크 점 (tech-kawaii)
  const blush = (): void => {
    out.push({ kind: 'ellipse', cx: s.cx - s.w * 0.34, cy: smileY, rx: 3.5, ry: 2.5, color: 7, alpha: 0.85 });
    out.push({ kind: 'ellipse', cx: s.cx + s.w * 0.34, cy: smileY, rx: 3.5, ry: 2.5, color: 7, alpha: 0.85 });
  };

  if (expression === 'blink') {
    out.push({ kind: 'line', x1: exL - 5, y1: eyeY, x2: exL + 5, y2: eyeY, color: 6, width: 3 });
    out.push({ kind: 'line', x1: exR - 5, y1: eyeY, x2: exR + 5, y2: eyeY, color: 6, width: 3 });
    out.push({ kind: 'arc', cx: s.cx, cy: smileY - 2, rx: 5, ry: 3.5, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: 6, width: 2.5 });
    blush();
    return out;
  }
  if (expression === 'happy') {
    // ^^ + 활짝 미소 (Cozmo 행복 표정)
    out.push({ kind: 'arc', cx: exL, cy: eyeY + 3, rx: 6, ry: 5, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 6, width: 3 });
    out.push({ kind: 'arc', cx: exR, cy: eyeY + 3, rx: 6, ry: 5, start: 1.15 * Math.PI, end: 1.85 * Math.PI, color: 6, width: 3 });
    out.push({ kind: 'arc', cx: s.cx, cy: smileY - 3, rx: 8, ry: 6, start: 0.12 * Math.PI, end: 0.88 * Math.PI, color: 6, width: 3 });
    blush();
    return out;
  }
  if (expression === 'sleep') {
    out.push({ kind: 'arc', cx: exL, cy: eyeY - 1, rx: 6, ry: 3.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 6, width: 3 });
    out.push({ kind: 'arc', cx: exR, cy: eyeY - 1, rx: 6, ry: 3.5, start: 0.15 * Math.PI, end: 0.85 * Math.PI, color: 6, width: 3 });
    out.push({ kind: 'stroke-ellipse', cx: s.cx, cy: smileY, rx: 2.5, ry: 3, color: 6, width: 2 });
    out.push({ kind: 'ellipse', cx: s.cx + s.w * 0.55, cy: s.y - 6, rx: 3, ry: 3, color: 3, alpha: 0.9 });
    out.push({ kind: 'ellipse', cx: s.cx + s.w * 0.66, cy: s.y - 13, rx: 4, ry: 4, color: 3, alpha: 0.9 });
    return out;
  }

  // Normal — 눈 변형 6종 (스크린 글로우 스타일)
  const ev = EYES_VARIANTS[ei];
  switch (ev.kind) {
    case 'round': {
      out.push({ kind: 'ellipse', cx: exL, cy: eyeY, rx: 5, ry: 5.5, color: 6 });
      out.push({ kind: 'ellipse', cx: exR, cy: eyeY, rx: 5, ry: 5.5, color: 6 });
      break;
    }
    case 'led': {
      for (const cx of [exL, exR]) {
        out.push({ kind: 'rrect', x: cx - 3.5, y: eyeY - 6.5, w: 7, h: 13, r: 3.5, color: 6 });
      }
      break;
    }
    case 'star': {
      for (const cx of [exL, exR]) {
        out.push({ kind: 'line', x1: cx - 6, y1: eyeY, x2: cx + 6, y2: eyeY, color: 6, width: 3 });
        out.push({ kind: 'line', x1: cx, y1: eyeY - 6, x2: cx, y2: eyeY + 6, color: 6, width: 3 });
        out.push({ kind: 'ellipse', cx, cy: eyeY, rx: 2, ry: 2, color: 6 });
      }
      break;
    }
    case 'heart': {
      for (const cx of [exL, exR]) {
        out.push({ kind: 'ellipse', cx: cx - 2.6, cy: eyeY - 1.8, rx: 3.2, ry: 3.2, color: 7 });
        out.push({ kind: 'ellipse', cx: cx + 2.6, cy: eyeY - 1.8, rx: 3.2, ry: 3.2, color: 7 });
        out.push({ kind: 'rrect', x: cx - 4.6, y: eyeY - 1.8, w: 9.2, h: 5.4, r: 2, color: 7 });
      }
      break;
    }
    case 'drowsy': {
      for (const cx of [exL, exR]) {
        out.push({ kind: 'rrect', x: cx - 5, y: eyeY - 2, w: 10, h: 5, r: 2.5, color: 6 });
      }
      break;
    }
    case 'scanner': {
      // 사이클롭스 — 넓은 외눈 바 (부드러운 캡슐)
      out.push({ kind: 'rrect', x: s.cx - s.w * 0.28, y: eyeY - 4.5, w: s.w * 0.56, h: 9, r: 4.5, color: 6 });
      break;
    }
  }
  // 작은 미소 (BMO)
  out.push({ kind: 'arc', cx: s.cx, cy: smileY - 2, rx: 5, ry: 3.5, start: 0.2 * Math.PI, end: 0.8 * Math.PI, color: 6, width: 2.5 });
  blush();
  return out;
}

// Antenna — 작게
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
    ...buildFace(spec.eyes, spec.head, frame.expression).map(shift),
    ...buildAntenna(spec.antenna, frame.antennaBlink).map(shift),
  ];
}

// drawRobot
function drawShape(ctx: CanvasRenderingContext2D, s: Shape, palette: ReadonlyArray<string>): void {
  ctx.save();
  if (s.alpha !== undefined) {
    ctx.globalAlpha = s.alpha;
  } else if (s.color === 5 && (s.kind === 'stroke-rrect' || s.kind === 'stroke-ellipse' || s.kind === 'line')) {
    // 부드러운 반투명 아웃라인 (플랫 스타일의 얇은 선)
    ctx.globalAlpha = 0.5;
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
