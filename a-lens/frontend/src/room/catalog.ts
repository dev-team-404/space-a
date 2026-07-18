// 부품 카탈로그 — variant별 팔레트. room.png(아이소 픽셀아트, 야간 우드톤)를 레퍼런스로 삼되
// 지금은 Graphics 프리미티브로 그린다. 스프라이트 에셋이 생기면 여기 팔레트가 텍스처 키로 바뀐다.

import type { BoardId, DecoId, DeskChoice, DeskId, FloorId, ShelfId, VariantOption, WallpaperId } from './types'

export type WallPalette = { wall: number; trim: number; shade: number }
export type FloorPalette = { a: number; b: number; edge: number }
export type DeskPalette = { top: number; side: number; leg: number }
export type BoardPalette = { frame: number; face: number; chalk: number }

// Graphics 폴백 팔레트 — 시트 4종의 근사색
export const WALLPAPERS: Record<WallpaperId, WallPalette> = {
  w1: { wall: 0xb08a5a, trim: 0x6b4423, shade: 0x9c7a4e }, // 라이트 우드 패널
  w2: { wall: 0x9a958a, trim: 0x3a3d42, shade: 0x878378 }, // 콘크리트 패널
  w3: { wall: 0xd8c9a8, trim: 0x8a5b2e, shade: 0xc4b696 }, // 크림 패널
  w4: { wall: 0x8a4438, trim: 0x4a2620, shade: 0x793a30 }, // 레드 브릭
}

export const FLOORS: Record<FloorId, FloorPalette> = {
  f1: { a: 0x9c6a38, b: 0x92622f, edge: 0x6b4423 }, // 오크 마루
  f2: { a: 0x76685c, b: 0x6c5f54, edge: 0x50463e }, // 그레이 우드
  f3: { a: 0xbe8848, b: 0xb27e40, edge: 0x8a5b2e }, // 라이트 오크
  f4: { a: 0x6e3428, b: 0x642e23, edge: 0x47201a }, // 다크 마호가니
}

/** 구버전 저장분 → 새 id 마이그레이션 */
export const LEGACY_WALL_IDS: Record<string, WallpaperId> = {
  'wood-night': 'w1', sage: 'w3', terracotta: 'w4', slate: 'w2',
}
export const LEGACY_FLOOR_IDS: Record<string, FloorId> = {
  plank: 'f1', checker: 'f3', stone: 'f2',
}

export function resolveWallpaperId(v: string): WallpaperId {
  if (v in WALLPAPERS) return v as WallpaperId
  return LEGACY_WALL_IDS[v] ?? 'w1'
}

export function resolveFloorId(v: string): FloorId {
  if (v in FLOORS) return v as FloorId
  return LEGACY_FLOOR_IDS[v] ?? 'f1'
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
  { id: 'w1', label: '라이트 우드', swatch: '#b08a5a', thumb: '/assets/kit/wall-w1.png' },
  { id: 'w2', label: '콘크리트', swatch: '#9a958a', thumb: '/assets/kit/wall-w2.png' },
  { id: 'w3', label: '크림 패널', swatch: '#d8c9a8', thumb: '/assets/kit/wall-w3.png' },
  { id: 'w4', label: '레드 브릭', swatch: '#8a4438', thumb: '/assets/kit/wall-w4.png' },
]

export const FLOOR_OPTIONS: VariantOption<FloorId>[] = [
  { id: 'f1', label: '오크 마루', swatch: '#9c6a38', thumb: '/assets/kit/floor-f1.png' },
  { id: 'f2', label: '그레이 우드', swatch: '#76685c', thumb: '/assets/kit/floor-f2.png' },
  { id: 'f3', label: '라이트 오크', swatch: '#be8848', thumb: '/assets/kit/floor-f3.png' },
  { id: 'f4', label: '다크 마호가니', swatch: '#6e3428', thumb: '/assets/kit/floor-f4.png' },
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
