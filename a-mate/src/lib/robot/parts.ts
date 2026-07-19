// parts.ts -- 128x128 procedural PIXEL-ART character library
// 2026-07-19 v5 전면 전환: 로봇 → 픽셀 치비 "사람" (스타듀밸리/쯔꾸르 감성 레퍼런스).
// 시드 계약(Rust mascot.rs: 슬롯 6종×6 + 팔레트 8)은 유지하고 의미만 재해석한다:
//   antenna→헤어스타일 · head→얼굴형 · eyes→눈 · body→의상 · arms→포즈 · palette→색 조합.
// Color indices: 0=outfit, 1=outfitShade, 2=accent, 3=pants, 4=hair, 5=outline, 6=skin, 7=cheek, 8=white

export const GRID = 128;
/** 픽셀 셀 크기 — 32×32 논리 그리드 × 4px = 128 (하드 픽셀) */
export const CELL = 4;
export const COLS = 32;

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
  ['#3a5a94', '#2c4470', '#d43c3c', '#555a66', '#4a3628', '#241f26', '#f5c9a0', '#f0a0a0', '#f4f2ee'], // 네이비 교복+빨간 타이
  ['#2e6db4', '#235694', '#e8e4d8', '#3c5a8c', '#3a2e24', '#241f26', '#f5c9a0', '#f0a0a0', '#f4f2ee'], // 파랑 후드
  ['#4a9a6a', '#3a7a54', '#f0d05a', '#5a5a62', '#2c2620', '#241f26', '#f5c9a0', '#f0a0a0', '#f4f2ee'], // 초록+노랑
  ['#c46a8a', '#a05070', '#fff0f4', '#705a64', '#6a4a30', '#241f26', '#fadcb8', '#f0a0a0', '#f4f2ee'], // 핑크
  ['#8a68c4', '#6e50a0', '#e8e0f8', '#4a4454', '#241f26', '#241f26', '#f5c9a0', '#f0a0a0', '#f4f2ee'], // 보라
  ['#d49a4a', '#b07c34', '#fff8e8', '#4c5a6a', '#8a3c2c', '#241f26', '#fadcb8', '#f0a0a0', '#f4f2ee'], // 머스터드+적갈머리
  ['#5ab4a4', '#429486', '#f0fffc', '#5a5a62', '#c4a05a', '#241f26', '#fadcb8', '#f0a0a0', '#f4f2ee'], // 틸+금발
  ['#6a7a8c', '#526070', '#d4d4d4', '#3a4450', '#56423a', '#241f26', '#f5c9a0', '#f0a0a0', '#f4f2ee'], // 슬레이트
];

// ── 얼굴형 (head 슬롯) — 셀 단위 타원 파라미터 (cx는 16 고정) ──
export interface FaceVariant { rx: number; ry: number; cy: number }
export const FACE_VARIANTS: ReadonlyArray<FaceVariant> = [
  { rx: 7.6, ry: 7.0, cy: 9.6 },
  { rx: 8.2, ry: 7.0, cy: 9.6 },
  { rx: 7.0, ry: 7.5, cy: 9.9 },
  { rx: 8.6, ry: 6.8, cy: 9.5 },
  { rx: 6.9, ry: 6.8, cy: 9.8 },
  { rx: 7.9, ry: 7.4, cy: 9.8 },
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
  { bang: 3.4, fringe: [0, 0.7, 0.2, 0.7, 0, 0.5], ear: 2.2, spikes: [], strands: false, slant: 0 },              // 바가지+가지런한 앞머리 (ref1)
  { bang: 3.8, fringe: [0.2, 1.4, 0.5, 1.8, 0.3, 1.1, 0.7], ear: 1.8, spikes: [[-2, -1], [1, -1.3], [4, -0.8]], strands: false, slant: 0 }, // 덥수룩 (ref2)
  { bang: 2.8, fringe: [0, 0.4, 0.1, 0.5], ear: 1.9, spikes: [[2, -1.1]], strands: false, slant: 0.32 },          // 옆가르마
  { bang: 3.2, fringe: [0.9, 0, 0.9, 0, 0.9], ear: 2.4, spikes: [[-4, -0.6], [0, -1], [4, -0.6]], strands: false, slant: 0 }, // 곱슬(뽀글)
  { bang: 3.4, fringe: [0, 0.6, 0.2, 0.6], ear: 2.2, spikes: [], strands: true, slant: 0 },                        // 장발 (어깨 옆머리)
  { bang: 2.0, fringe: [0, 0.3, 0, 0.3], ear: 1.2, spikes: [], strands: false, slant: 0 },                         // 짧고 단정 (이마 보임)
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
