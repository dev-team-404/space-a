// 부품 카탈로그 — 방 배경 프리셋 5종 + 그 위에 얹는 책상 9종.
// 방(벽·바닥·책장·칠판)은 통짜 배경 이미지가 그리므로, 남은 선택지는 배경과 책상뿐이다.

import type { CharacterId, DeskChoice, DeskId, RoomPresetId, VariantOption } from './types'

/** 내 캐릭터 15종 — char-cN.png. 방 안 에이전트 자리를 이 스프라이트로 그린다. */
export const CHARACTER_IDS: CharacterId[] = [
  'c1', 'c2', 'c3', 'c4', 'c5', 'c6', 'c7', 'c8', 'c9', 'c10', 'c11', 'c12', 'c13', 'c14', 'c15',
]

export function resolveCharacterId(v: string | undefined): CharacterId {
  return v && CHARACTER_IDS.includes(v as CharacterId) ? (v as CharacterId) : 'c1'
}

/** 에이전트별 캐릭터를 결정적으로 뽑는다 — 같은 seed는 늘 같은 캐릭터(리렌더에도 안정),
 *  방 안에서는 15종이 골고루 섞여 다양하게 보인다. seed = agent_id 등 고유 문자열. */
export function characterForSeed(seed: string): CharacterId {
  let h = 0
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0
  return CHARACTER_IDS[Math.abs(h) % CHARACTER_IDS.length]
}


/** 방 배경 프리셋 5종 — 통짜 이미지(벽·바닥·책장·칠판 포함). 이 위에 책상만 얹는다. */
export const ROOM_PRESET_IDS: RoomPresetId[] = ['r1', 'r2', 'r3', 'r4', 'r5']

export function resolveRoomId(v: string | undefined): RoomPresetId {
  return v && ROOM_PRESET_IDS.includes(v as RoomPresetId) ? (v as RoomPresetId) : 'r1'
}

const roomThumb = (id: RoomPresetId) => `/assets/kit/room-preset-${id.slice(1)}.png`
export const ROOM_OPTIONS: VariantOption<RoomPresetId>[] = [
  { id: 'r1', label: '벽돌 · 다크우드', swatch: '#8a4438', thumb: roomThumb('r1') },
  { id: 'r2', label: '세이지 · 라이트우드', swatch: '#9aa889', thumb: roomThumb('r2') },
  { id: 'r3', label: '네이비 · 나이트', swatch: '#2e3a4e', thumb: roomThumb('r3') },
  { id: 'r4', label: '크림 · 데이라이트', swatch: '#d8c9a8', thumb: roomThumb('r4') },
  { id: 'r5', label: '그레이 · 이브닝', swatch: '#8a7d6e', thumb: roomThumb('r5') },
]

export type DeskPalette = { top: number; side: number; leg: number }

// 책상 킷 스프라이트가 못 뜰 때의 Graphics 폴백 팔레트 — 시트가 전부 우드톤이라 공통
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
