// 부품 카탈로그 — variant별 팔레트. room.png(아이소 픽셀아트, 야간 우드톤)를 레퍼런스로 삼되
// 지금은 Graphics 프리미티브로 그린다. 스프라이트 에셋이 생기면 여기 팔레트가 텍스처 키로 바뀐다.

import type { BoardId, DecoId, DeskChoice, DeskId, FloorId, ShelfId, VariantOption, WallpaperId } from './types'

export type WallPalette = { wall: number; trim: number; shade: number }
export type FloorPalette = { a: number; b: number; edge: number }
export type DeskPalette = { top: number; side: number; leg: number }
export type BoardPalette = { frame: number; face: number; chalk: number }

export const WALLPAPERS: Record<WallpaperId, WallPalette> = {
  'wood-night': { wall: 0x7a5a3a, trim: 0x5c4127, shade: 0x684c30 },
  sage: { wall: 0x6f7d62, trim: 0x53614a, shade: 0x60705a }, // 세이지 그린
  terracotta: { wall: 0x9c5f43, trim: 0x7a4630, shade: 0x8a5238 },
  slate: { wall: 0x4c5566, trim: 0x39404e, shade: 0x424a5a },
}

export const FLOORS: Record<FloorId, FloorPalette> = {
  plank: { a: 0x8a5f36, b: 0x81572f, edge: 0x6b4423 },
  checker: { a: 0x8a6a45, b: 0x74563a, edge: 0x5f4630 },
  stone: { a: 0x707a86, b: 0x646d78, edge: 0x525a64 },
}

// 스프라이트가 못 뜰 때(킷 로드 실패)의 Graphics 폴백 팔레트 — 시트가 전부 우드톤이라 공통
const WOOD_DESK: DeskPalette = { top: 0xa5713d, side: 0x8a5b2e, leg: 0x6b4423 }
export const DESKS: Record<DeskId, DeskPalette> = {
  d1: WOOD_DESK, d2: WOOD_DESK, d3: WOOD_DESK,
  d4: WOOD_DESK, d5: WOOD_DESK, d6: WOOD_DESK,
  d7: { top: 0xa5713d, side: 0x8a5b2e, leg: 0x3a3f46 }, // 메탈 프레임
  d8: WOOD_DESK, d9: WOOD_DESK,
}

/** 구버전 저장분(oak/walnut/white) → 새 id 마이그레이션 */
export const LEGACY_DESK_IDS: Record<string, DeskId> = { oak: 'd1', walnut: 'd6', white: 'd7' }

/** 'mix'·구버전 id를 실제 DeskId로 해석. k = 책상 인덱스(mix 순환용) */
export function resolveDeskId(choice: string, k = 0): DeskId {
  if (choice === 'mix') return `d${(k % 9) + 1}` as DeskId
  if (choice in DESKS) return choice as DeskId
  return LEGACY_DESK_IDS[choice] ?? 'd1'
}

/** 책장 variant가 붙는 벽 — 스프라이트가 그려진 원근 방향에 따름 */
export const SHELF_SIDE: Record<ShelfId, 'left' | 'right'> = {
  s1: 'left', s2: 'left',
  s3: 'right', s4: 'right', s5: 'right', s6: 'right', s7: 'right',
  s8: 'right', s9: 'right', s10: 'right', s11: 'right',
}

export function resolveShelfId(v: string | undefined): ShelfId {
  return v && v in SHELF_SIDE ? (v as ShelfId) : 's1'
}

export const BOARDS: Record<BoardId, BoardPalette> = {
  'chalk-green': { frame: 0x8a5b2e, face: 0x2d4a3a, chalk: 0xe8e6d8 },
  'chalk-black': { frame: 0x8a5b2e, face: 0x23262b, chalk: 0xe8e6d8 },
  glass: { frame: 0x9aa4b2, face: 0x2b3542, chalk: 0xbfe3ff },
}

export const WALLPAPER_OPTIONS: VariantOption<WallpaperId>[] = [
  { id: 'wood-night', label: '우드 나이트', swatch: '#7a5a3a' },
  { id: 'sage', label: '세이지', swatch: '#6f7d62' },
  { id: 'terracotta', label: '테라코타', swatch: '#9c5f43' },
  { id: 'slate', label: '슬레이트', swatch: '#4c5566' },
]

export const FLOOR_OPTIONS: VariantOption<FloorId>[] = [
  { id: 'plank', label: '원목 마루', swatch: '#8a5f36' },
  { id: 'checker', label: '체커', swatch: '#74563a' },
  { id: 'stone', label: '스톤 타일', swatch: '#707a86' },
]

const deskThumb = (id: DeskId) => `/assets/kit/desk-${id}.png`
export const DESK_OPTIONS: VariantOption<DeskChoice>[] = [
  { id: 'd1', label: '듀얼 모니터', swatch: '#a5713d', thumb: deskThumb('d1') },
  { id: 'd2', label: '와이드 듀얼', swatch: '#a5713d', thumb: deskThumb('d2') },
  { id: 'd3', label: '노트북 캐비닛', swatch: '#a5713d', thumb: deskThumb('d3') },
  { id: 'd4', label: '선반형', swatch: '#a5713d', thumb: deskThumb('d4') },
  { id: 'd5', label: '책장 수납', swatch: '#a5713d', thumb: deskThumb('d5') },
  { id: 'd6', label: 'ㄱ자 코너', swatch: '#6e4a2c', thumb: deskThumb('d6') },
  { id: 'd7', label: '미니멀 메탈', swatch: '#3a3f46', thumb: deskThumb('d7') },
  { id: 'd8', label: '트리플 모니터', swatch: '#a5713d', thumb: deskThumb('d8') },
  { id: 'd9', label: '수납형 노트북', swatch: '#a5713d', thumb: deskThumb('d9') },
  { id: 'mix', label: '랜덤 믹스', swatch: '#d9a441' },
]

const shelfThumb = (id: ShelfId) => `/assets/kit/shelf-${id}.png`
export const SHELF_OPTIONS: VariantOption<ShelfId>[] = [
  { id: 's1', label: '왼벽 높은 1', swatch: '#8a5b2e', thumb: shelfThumb('s1') },
  { id: 's2', label: '왼벽 높은 2', swatch: '#8a5b2e', thumb: shelfThumb('s2') },
  { id: 's3', label: '오른벽 높은 1', swatch: '#8a5b2e', thumb: shelfThumb('s3') },
  { id: 's4', label: '오른벽 높은 2', swatch: '#8a5b2e', thumb: shelfThumb('s4') },
  { id: 's5', label: '오른벽 낮은 1', swatch: '#8a5b2e', thumb: shelfThumb('s5') },
  { id: 's6', label: '오른벽 낮은 2', swatch: '#8a5b2e', thumb: shelfThumb('s6') },
  { id: 's7', label: '낮은 가로 1', swatch: '#8a5b2e', thumb: shelfThumb('s7') },
  { id: 's8', label: '낮은 가로 2', swatch: '#8a5b2e', thumb: shelfThumb('s8') },
  { id: 's9', label: '중간 높이 1', swatch: '#8a5b2e', thumb: shelfThumb('s9') },
  { id: 's10', label: '중간 높이 2', swatch: '#8a5b2e', thumb: shelfThumb('s10') },
  { id: 's11', label: '좁은 책장', swatch: '#8a5b2e', thumb: shelfThumb('s11') },
]

export const BOARD_OPTIONS: VariantOption<BoardId>[] = [
  { id: 'chalk-green', label: '초록 칠판', swatch: '#2d4a3a' },
  { id: 'chalk-black', label: '블랙보드', swatch: '#23262b' },
  { id: 'glass', label: '글래스 보드', swatch: '#2b3542' },
]

export const DECO_OPTIONS: VariantOption<DecoId>[] = [
  { id: 'plant', label: '화분', swatch: '#4e7a3a' },
  { id: 'water-cooler', label: '정수기', swatch: '#b8c4cc' },
  { id: 'rug', label: '러그', swatch: '#b0524a' },
  { id: 'string-lights', label: '전구 조명', swatch: '#e8b54a' },
]
