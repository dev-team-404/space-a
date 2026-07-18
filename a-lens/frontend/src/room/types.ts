// 방 = 데이터. 이미지 좌표 캘리브레이션 없이 격자 + variant 선택만으로 방을 그린다 (#44 4번).

export type WallpaperId = 'wood-night' | 'sage' | 'terracotta' | 'slate'
export type FloorId = 'plank' | 'checker' | 'stone'
export type DeskId = 'oak' | 'walnut' | 'white'
export type BoardId = 'chalk-green' | 'chalk-black' | 'glass'
export type DecoId = 'plant' | 'water-cooler' | 'rug' | 'string-lights'

/** 사용자가 방 만들기에서 고른 것 전부. localStorage에 이대로 저장된다. */
export type RoomConfig = {
  /** a-hub 스페이스 id — 이 방에 어느 스페이스 데이터를 표시할지 */
  space_id: string
  /** 표시용 스페이스 이름 (빌드 시점 스냅숏, 렌더 시 최신값으로 덮어씀) */
  space_name: string
  wallpaper: WallpaperId
  floor: FloorId
  desk: DeskId
  board: BoardId
  deco: DecoId[]
  created_at: string
}

export type VariantOption<Id extends string> = {
  id: Id
  label: string
  /** 선택 UI 스와치 색 */
  swatch: string
}
