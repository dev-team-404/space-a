// parts.ts -- 128x128 procedural robot parts library
// 2026-07-19 전면 재설계: 싸이월드 미니미 감성 치비 로봇 —
//   대두 비율(머리 ~60%) · 작은 몸통 · 발 · 투톤 얼굴판 · 부드러운 림 셰이드.
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

// [body, bodyShade, accent, accentShade, eye, outline, highlight, cheek]
export const PALETTES: ReadonlyArray<readonly [string, string, string, string, string, string, string, string]> = [
  ['#b2e8d8', '#7fc4ac', '#5ec4a8', '#3a9880', '#ffffff', '#1a3a4a', '#e8fffc', '#ffb8c8'],
  ['#f7c5b0', '#d4987c', '#e88a6c', '#c06848', '#ffffff', '#3a1a10', '#fff5f0', '#ffb8c8'],
  ['#d4b8f0', '#a888d0', '#9b6fd4', '#7048b0', '#ffffff', '#2a1050', '#f8f0ff', '#ffb8c8'],
  ['#f5e8b0', '#d4c078', '#d4b840', '#a89020', '#ffffff', '#3a2800', '#fffff0', '#ffb8c8'],
  ['#a8d8f0', '#78b0d4', '#4898d4', '#2870b0', '#ffffff', '#0a2840', '#f0f8ff', '#ffb8c8'],
  ['#f0b8c8', '#d088a0', '#d46888', '#b04060', '#ffffff', '#40101c', '#fff0f4', '#ffb8c8'],
  ['#b8d4b0', '#88b080', '#688c60', '#486840', '#ffffff', '#182810', '#f0fff0', '#ffb8c8'],
  ['#c8d0d8', '#98a8b8', '#7890a0', '#507080', '#ffffff', '#182028', '#f4f8fc', '#ffb8c8'],
];

// Head variant parameters — 대두: 세로의 ~60%를 머리가 차지
export type HeadVariant =
  | { kind: 'round'; cx: number; cy: number; rx: number; ry: number }
  | { kind: 'square'; x: number; y: number; w: number; h: number; r: number }
  | { kind: 'helmet'; x: number; y: number; w: number; h: number; r: number; visorY: number; visorH: number }
  | { kind: 'catear'; x: number; y: number; w: number; h: number; r: number; earW: number; earH: number }
  | { kind: 'crt'; x: number; y: number; w: number; h: number; r: number }
  | { kind: 'capsule'; cx: number; cy: number; rx: number; ry: number };

export const HEAD_VARIANTS: ReadonlyArray<HeadVariant> = [
  { kind: 'round',   cx: 64, cy: 42, rx: 40, ry: 36 },
  { kind: 'square',  x: 22, y: 8,  w: 84, h: 68, r: 30 },
  { kind: 'helmet',  x: 20, y: 6,  w: 88, h: 70, r: 32, visorY: 34, visorH: 20 },
  { kind: 'catear',  x: 24, y: 12, w: 80, h: 64, r: 28, earW: 16, earH: 16 },
  { kind: 'crt',     x: 16, y: 10, w: 96, h: 66, r: 28, },
  { kind: 'capsule', cx: 64, cy: 42, rx: 34, ry: 38 },
];

// Body variant parameters — 작은 몸통 (y 82..110)
export type BodyVariant =
  | { kind: 'round';   cx: number; cy: number; rx: number; ry: number }
  | { kind: 'box';     x: number; y: number; w: number; h: number; r: number; ledX: number; ledY: number; ledW: number; ledH: number }
  | { kind: 'barrel';  cx: number; cy: number; rx: number; ry: number; neckRx: number }
  | { kind: 'vest';    x: number; y: number; w: number; h: number; r: number; lapelW: number }
  | { kind: 'pocket';  x: number; y: number; w: number; h: number; r: number; pocketX: number; pocketY: number; pocketW: number; pocketH: number }
  | { kind: 'striped'; x: number; y: number; w: number; h: number; r: number; stripeY1: number; stripeY2: number; stripeY3: number };

export const BODY_VARIANTS: ReadonlyArray<BodyVariant> = [
  { kind: 'round',   cx: 64, cy: 96, rx: 25, ry: 15 },
  { kind: 'box',     x: 38, y: 82, w: 52, h: 28, r: 12, ledX: 56, ledY: 90, ledW: 16, ledH: 12 },
  { kind: 'barrel',  cx: 64, cy: 96, rx: 28, ry: 15, neckRx: 18 },
  { kind: 'vest',    x: 36, y: 82, w: 56, h: 28, r: 12, lapelW: 8 },
  { kind: 'pocket',  x: 36, y: 82, w: 56, h: 28, r: 12, pocketX: 46, pocketY: 92, pocketW: 14, pocketH: 10 },
  { kind: 'striped', x: 36, y: 82, w: 56, h: 28, r: 12, stripeY1: 87, stripeY2: 94, stripeY3: 101 },
];

// Antenna variant parameters — 짧고 동글 (머리 위 y 0..12)
export type AntennaVariant =
  | { kind: 'rod';       stemX: number; stemY1: number; stemY2: number; ballCx: number; ballCy: number; ballR: number }
  | { kind: 'feelers';   lx: number; rx: number; baseY: number; tipY: number; tipR: number }
  | { kind: 'dish';      dishX: number; dishY: number; dishW: number; dishH: number; stemX: number; stemY1: number; stemY2: number }
  | { kind: 'ring';      ringCx: number; ringCy: number; ringRx: number; ringRy: number; stemX: number; stemY1: number; stemY2: number }
  | { kind: 'lightning'; points: ReadonlyArray<readonly [number, number]> }
  | { kind: 'flapear';   lx: number; rx: number; earY: number; earRx: number; earRy: number };

export const ANTENNA_VARIANTS: ReadonlyArray<AntennaVariant> = [
  { kind: 'rod',       stemX: 64, stemY1: 0, stemY2: 10, ballCx: 64, ballCy: 0, ballR: 5 },
  { kind: 'feelers',   lx: 46, rx: 82, baseY: 10, tipY: 0, tipR: 5 },
  { kind: 'dish',      dishX: 40, dishY: 0, dishW: 48, dishH: 8, stemX: 64, stemY1: 8, stemY2: 12 },
  { kind: 'ring',      ringCx: 64, ringCy: 2, ringRx: 14, ringRy: 6, stemX: 64, stemY1: 8, stemY2: 12 },
  { kind: 'lightning', points: [[64,0],[70,4],[60,7],[66,10],[58,12]] as const },
  { kind: 'flapear',   lx: 24, rx: 104, earY: 14, earRx: 8, earRy: 11 },
];

// Eyes variant parameters — 미니미: 작은 세로 타원 눈 (buildEyes가 er 기준으로 축소 렌더)
export type EyesVariant =
  | { kind: 'round';   lx: number; rx: number; ey: number; er: number }
  | { kind: 'led';     lx: number; rx: number; ey: number; ew: number; eh: number }
  | { kind: 'star';    lx: number; rx: number; ey: number; sr: number }
  | { kind: 'heart';   lx: number; rx: number; ey: number }
  | { kind: 'drowsy';  lx: number; rx: number; ey: number; ew: number; eh: number }
  | { kind: 'scanner'; ey: number; x1: number; x2: number };

export const EYES_VARIANTS: ReadonlyArray<EyesVariant> = [
  { kind: 'round',   lx: 48, rx: 80, ey: 42, er: 9 },
  { kind: 'led',     lx: 48, rx: 80, ey: 42, ew: 12, eh: 16 },
  { kind: 'star',    lx: 48, rx: 80, ey: 42, sr: 8 },
  { kind: 'heart',   lx: 48, rx: 80, ey: 42 },
  { kind: 'drowsy',  lx: 48, rx: 80, ey: 42, ew: 14, eh: 4 },
  { kind: 'scanner', ey: 42, x1: 34, x2: 94 },
];

// Arms variant parameters — 몸통 옆 짧은 스텁 (좌 x≈24, 우 x≈94 기준)
export type ArmsVariant =
  | { kind: 'down';    ly: number; ry: number; aw: number; ah: number }
  | { kind: 'up';      ly: number; ry: number; aw: number; ah: number }
  | { kind: 'pincer';  ly: number; ry: number; aw: number; ah: number }
  | { kind: 'stubby';  ly: number; ry: number; aw: number; ah: number }
  | { kind: 'wave';    ly: number; ry: number; aw: number; ah: number }
  | { kind: 'rocket';  ly: number; ry: number; aw: number; ah: number; extW: number };

export const ARMS_VARIANTS: ReadonlyArray<ArmsVariant> = [
  { kind: 'down',   ly: 84, ry: 84, aw: 10, ah: 18 },
  { kind: 'up',     ly: 72, ry: 72, aw: 10, ah: 18 },
  { kind: 'pincer', ly: 84, ry: 84, aw: 10, ah: 14 },
  { kind: 'stubby', ly: 88, ry: 88, aw: 9,  ah: 11 },
  { kind: 'wave',   ly: 72, ry: 86, aw: 10, ah: 16 },
  { kind: 'rocket', ly: 86, ry: 86, aw: 10, ah: 14, extW: 8 },
];
