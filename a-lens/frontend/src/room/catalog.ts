// 부품 카탈로그 — variant별 팔레트. room.png(아이소 픽셀아트, 야간 우드톤)를 레퍼런스로 삼되
// 지금은 Graphics 프리미티브로 그린다. 스프라이트 에셋이 생기면 여기 팔레트가 텍스처 키로 바뀐다.

import type { BoardId, DecoId, DeskId, FloorId, VariantOption, WallpaperId } from './types'

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

export const DESKS: Record<DeskId, DeskPalette> = {
  oak: { top: 0xa5713d, side: 0x8a5b2e, leg: 0x6b4423 },
  walnut: { top: 0x6e4a2c, side: 0x593a20, leg: 0x452c17 },
  white: { top: 0xd8d5cd, side: 0xbcb8ae, leg: 0x8f8c84 },
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

export const DESK_OPTIONS: VariantOption<DeskId>[] = [
  { id: 'oak', label: '오크 책상', swatch: '#a5713d' },
  { id: 'walnut', label: '월넛 책상', swatch: '#6e4a2c' },
  { id: 'white', label: '화이트 책상', swatch: '#d8d5cd' },
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
