// parts.ts -- 128x128 procedural robot parts library
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

// Head variant parameters
export type HeadVariant =
  | { kind: 'round'; cx: number; cy: number; rx: number; ry: number }
  | { kind: 'square'; x: number; y: number; w: number; h: number; r: number }
  | { kind: 'helmet'; x: number; y: number; w: number; h: number; r: number; visorY: number; visorH: number }
  | { kind: 'catear'; x: number; y: number; w: number; h: number; r: number; earW: number; earH: number }
  | { kind: 'crt'; x: number; y: number; w: number; h: number; r: number }
  | { kind: 'capsule'; cx: number; cy: number; rx: number; ry: number };

export const HEAD_VARIANTS: ReadonlyArray<HeadVariant> = [
  { kind: 'round',   cx: 64, cy: 38, rx: 38, ry: 28 },
  { kind: 'square',  x: 28, y: 12, w: 72, h: 56, r: 4 },
  { kind: 'helmet',  x: 26, y: 10, w: 76, h: 58, r: 6, visorY: 30, visorH: 16 },
  { kind: 'catear',  x: 30, y: 18, w: 68, h: 52, r: 4, earW: 14, earH: 18 },
  { kind: 'crt',     x: 18, y: 10, w: 92, h: 56, r: 8 },
  { kind: 'capsule', cx: 64, cy: 38, rx: 28, ry: 34 },
];

// Body variant parameters
export type BodyVariant =
  | { kind: 'round';   cx: number; cy: number; rx: number; ry: number }
  | { kind: 'box';     x: number; y: number; w: number; h: number; r: number; ledX: number; ledY: number; ledW: number; ledH: number }
  | { kind: 'barrel';  cx: number; cy: number; rx: number; ry: number; neckRx: number }
  | { kind: 'vest';    x: number; y: number; w: number; h: number; r: number; lapelW: number }
  | { kind: 'pocket';  x: number; y: number; w: number; h: number; r: number; pocketX: number; pocketY: number; pocketW: number; pocketH: number }
  | { kind: 'striped'; x: number; y: number; w: number; h: number; r: number; stripeY1: number; stripeY2: number; stripeY3: number };

export const BODY_VARIANTS: ReadonlyArray<BodyVariant> = [
  { kind: 'round',   cx: 64, cy: 90, rx: 28, ry: 22 },
  { kind: 'box',     x: 32, y: 70, w: 64, h: 44, r: 4, ledX: 50, ledY: 80, ledW: 28, ledH: 16 },
  { kind: 'barrel',  cx: 64, cy: 90, rx: 34, ry: 24, neckRx: 22 },
  { kind: 'vest',    x: 30, y: 70, w: 68, h: 44, r: 4, lapelW: 10 },
  { kind: 'pocket',  x: 30, y: 70, w: 68, h: 44, r: 4, pocketX: 36, pocketY: 88, pocketW: 20, pocketH: 14 },
  { kind: 'striped', x: 30, y: 70, w: 68, h: 44, r: 4, stripeY1: 78, stripeY2: 88, stripeY3: 98 },
];

// Antenna variant parameters
export type AntennaVariant =
  | { kind: 'rod';       stemX: number; stemY1: number; stemY2: number; ballCx: number; ballCy: number; ballR: number }
  | { kind: 'feelers';   lx: number; rx: number; baseY: number; tipY: number; tipR: number }
  | { kind: 'dish';      dishX: number; dishY: number; dishW: number; dishH: number; stemX: number; stemY1: number; stemY2: number }
  | { kind: 'ring';      ringCx: number; ringCy: number; ringRx: number; ringRy: number; stemX: number; stemY1: number; stemY2: number }
  | { kind: 'lightning'; points: ReadonlyArray<readonly [number, number]> }
  | { kind: 'flapear';   lx: number; rx: number; earY: number; earRx: number; earRy: number };

export const ANTENNA_VARIANTS: ReadonlyArray<AntennaVariant> = [
  { kind: 'rod',       stemX: 64, stemY1: 0, stemY2: 14, ballCx: 64, ballCy: 0, ballR: 7 },
  { kind: 'feelers',   lx: 44, rx: 84, baseY: 12, tipY: 0, tipR: 6 },
  { kind: 'dish',      dishX: 32, dishY: 0, dishW: 64, dishH: 10, stemX: 64, stemY1: 10, stemY2: 14 },
  { kind: 'ring',      ringCx: 64, ringCy: 4, ringRx: 18, ringRy: 8, stemX: 64, stemY1: 10, stemY2: 14 },
  { kind: 'lightning', points: [[64,0],[72,4],[60,8],[68,12],[56,14]] as const },
  { kind: 'flapear',   lx: 26, rx: 102, earY: 15, earRx: 10, earRy: 14 },
];

// Eyes variant parameters
export type EyesVariant =
  | { kind: 'round';   lx: number; rx: number; ey: number; er: number }
  | { kind: 'led';     lx: number; rx: number; ey: number; ew: number; eh: number }
  | { kind: 'star';    lx: number; rx: number; ey: number; sr: number }
  | { kind: 'heart';   lx: number; rx: number; ey: number }
  | { kind: 'drowsy';  lx: number; rx: number; ey: number; ew: number; eh: number }
  | { kind: 'scanner'; ey: number; x1: number; x2: number };

export const EYES_VARIANTS: ReadonlyArray<EyesVariant> = [
  { kind: 'round',   lx: 48, rx: 80, ey: 36, er: 9 },
  { kind: 'led',     lx: 48, rx: 80, ey: 36, ew: 12, eh: 16 },
  { kind: 'star',    lx: 48, rx: 80, ey: 36, sr: 7 },
  { kind: 'heart',   lx: 48, rx: 80, ey: 36 },
  { kind: 'drowsy',  lx: 48, rx: 80, ey: 36, ew: 14, eh: 4 },
  { kind: 'scanner', ey: 36, x1: 28, x2: 100 },
];

// Arms variant parameters
export type ArmsVariant =
  | { kind: 'down';    ly: number; ry: number; aw: number; ah: number }
  | { kind: 'up';      ly: number; ry: number; aw: number; ah: number }
  | { kind: 'pincer';  ly: number; ry: number; aw: number; ah: number }
  | { kind: 'stubby';  ly: number; ry: number; aw: number; ah: number }
  | { kind: 'wave';    ly: number; ry: number; aw: number; ah: number }
  | { kind: 'rocket';  ly: number; ry: number; aw: number; ah: number; extW: number };

export const ARMS_VARIANTS: ReadonlyArray<ArmsVariant> = [
  { kind: 'down',   ly: 76, ry: 76, aw: 14, ah: 28 },
  { kind: 'up',     ly: 58, ry: 58, aw: 14, ah: 28 },
  { kind: 'pincer', ly: 76, ry: 76, aw: 14, ah: 24 },
  { kind: 'stubby', ly: 80, ry: 80, aw: 10, ah: 14 },
  { kind: 'wave',   ly: 60, ry: 78, aw: 14, ah: 24 },
  { kind: 'rocket', ly: 78, ry: 78, aw: 14, ah: 20, extW: 10 },
];
