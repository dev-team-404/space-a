// parts.ts -- 128x128 procedural PIXEL-ART character library
// 2026-07-19 v5 전면 전환: 로봇 → 픽셀 치비 "사람" (스타듀밸리/쯔꾸르 감성 레퍼런스).
// 시드 계약(Rust mascot.rs: 슬롯 6종×6 + 팔레트 8)은 유지하고 의미만 재해석한다:
//   antenna→헤어스타일 · head→얼굴형 · eyes→눈 · body→의상 · arms→포즈 · palette→색 조합.
// Color indices: 0=outfit, 1=outfitShade, 2=accent, 3=pants, 4=hair, 5=hairShade, 6=skin, 7=cheek, 8=white, (9=OUTLINE 상수)

export const GRID = 128;
/** 픽셀 셀 크기 — 64×64 논리 그리드 × 2px = 128 (레퍼런스 해상도) */
export const CELL = 2;
export const COLS = 64;

export const VARIANTS = {
  antenna: 6, // 헤어스타일
  head: 6,    // 얼굴형
  eyes: 6,
  body: 6,    // 의상
  arms: 6,    // 포즈
  palette: 8,
} as const;

// [outfit, outfitShade, accent, pants, hair, outline, skin, cheek, white]
export const PALETTES: ReadonlyArray<readonly [string, string, string, string, string, string, string, string, string]> = [
  ['#3a5a94', '#2c4470', '#d43c3c', '#555a66', '#4a3628', '#33241a', '#f5c9a0', '#f0a8a0', '#f4f2ee'],
  ['#2e6db4', '#235694', '#e8e4d8', '#3c5a8c', '#3a2e24', '#261c12', '#f5c9a0', '#f0a8a0', '#f4f2ee'],
  ['#4a9a6a', '#3a7a54', '#f0d05a', '#5a5a62', '#2c2620', '#1a1512', '#f5c9a0', '#f0a8a0', '#f4f2ee'],
  ['#c46a8a', '#a05070', '#fff0f4', '#705a64', '#6a4a30', '#4a3320', '#fadcb8', '#f0a8a0', '#f4f2ee'],
  ['#8a68c4', '#6e50a0', '#e8e0f8', '#4a4454', '#2a2430', '#181420', '#f5c9a0', '#f0a8a0', '#f4f2ee'],
  ['#d49a4a', '#b07c34', '#fff8e8', '#4c5a6a', '#8a3c2c', '#5f281d', '#fadcb8', '#f0a8a0', '#f4f2ee'],
  ['#5ab4a4', '#429486', '#f0fffc', '#5a5a62', '#c4a05a', '#96763c', '#fadcb8', '#f0a8a0', '#f4f2ee'],
  ['#6a7a8c', '#526070', '#d4d4d4', '#3a4450', '#56423a', '#3a2c26', '#f5c9a0', '#f0a8a0', '#f4f2ee'],
];

// ── 얼굴형 (head 슬롯) — 셀 단위 타원 파라미터 (cx는 16 고정) ──
export interface FaceVariant { rx: number; ry: number; cy: number }
export const FACE_VARIANTS: ReadonlyArray<FaceVariant> = [
  { rx: 13.0, ry: 12.0, cy: 17.0 },
  { rx: 14.0, ry: 12.0, cy: 17.0 },
  { rx: 12.5, ry: 12.6, cy: 17.4 },
  { rx: 14.5, ry: 11.6, cy: 16.8 },
  { rx: 12.0, ry: 11.8, cy: 17.2 },
  { rx: 13.5, ry: 12.4, cy: 17.2 },
];

// ── 헤어스타일 (antenna 슬롯) ──
//   bang: 이마를 덮는 앞머리 깊이(셀) · fringe: 컬럼별 들쭉 패턴 · ear: 옆머리가 내려오는 깊이
//   spikes: 정수리 위 삐친 머리 셀 오프셋 · strands: 어깨까지 내려오는 옆머리(장발)
export interface HairVariant {
  bang: number;
  fringe: ReadonlyArray<number>;
  ear: number;
  spikes: ReadonlyArray<readonly [number, number]>;
  strands: boolean;
  slant: number; // 옆가르마: 컬럼당 기울기
}
export const HAIR_VARIANTS: ReadonlyArray<HairVariant> = [
  { bang: 7.5, fringe: [0, 1.2, 0.3, 1.4, 0.2, 1.0], ear: 5.5, spikes: [], strands: false, slant: 0 },
  { bang: 8.5, fringe: [0.5, 2.6, 1.0, 3.2, 0.6, 2.2, 1.2], ear: 4.5, spikes: [[-5, -1.6], [1, -2.2], [7, -1.2]], strands: false, slant: 0 },
  { bang: 6.0, fringe: [0, 0.8, 0.2, 1.0], ear: 4.8, spikes: [[5, -1.8]], strands: false, slant: 0.35 },
  { bang: 7.0, fringe: [1.8, 0, 1.8, 0, 1.8, 0], ear: 6.0, spikes: [[-8, -1.0], [0, -1.8], [8, -1.0]], strands: false, slant: 0 },
  { bang: 7.5, fringe: [0, 1.2, 0.4, 1.2], ear: 5.5, spikes: [], strands: true, slant: 0 },
  { bang: 4.5, fringe: [0, 0.6, 0.1, 0.6], ear: 3.0, spikes: [], strands: false, slant: 0 },
];

// ── 눈 (eyes 슬롯) — 픽셀 위 소형 벡터 (레퍼런스의 매끈-타원 눈) ──
export type EyeStyle = 'oval' | 'round' | 'calm' | 'sparkle' | 'droopy' | 'happy';
export const EYE_VARIANTS: ReadonlyArray<EyeStyle> = ['oval', 'round', 'calm', 'sparkle', 'droopy', 'happy'];

// ── 의상 (body 슬롯) ──
export type OutfitKind = 'hoodie' | 'blazer' | 'tshirt' | 'sweater' | 'shirt' | 'overalls';
export const OUTFIT_VARIANTS: ReadonlyArray<OutfitKind> = ['hoodie', 'blazer', 'tshirt', 'sweater', 'shirt', 'overalls'];

// ── 포즈 (arms 슬롯) ──
export type PoseKind = 'down' | 'pockets' | 'wave' | 'front' | 'bag' | 'behind';
export const POSE_VARIANTS: ReadonlyArray<PoseKind> = ['down', 'pockets', 'wave', 'front', 'bag', 'behind'];
