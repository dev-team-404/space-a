// 방 = 데이터. 이미지 좌표 캘리브레이션 없이 격자 + variant 선택만으로 방을 그린다 (#44 4번).

/** 벽 4종 — floor-wall.png 시트에서 추출 (코너 스프라이트) */
export type WallpaperId = 'w1' | 'w2' | 'w3' | 'w4'
/** 바닥 4종 — floor-wall.png 시트에서 추출 (육각 슬래브 스프라이트) */
export type FloorId = 'f1' | 'f2' | 'f3' | 'f4'
/** 책상 9종 — desk.png 시트에서 추출한 스프라이트 (assets/kit/desk-d*.png) */
export type DeskId = 'd1' | 'd2' | 'd3' | 'd4' | 'd5' | 'd6' | 'd7' | 'd8' | 'd9'
/** 'mix'면 책상마다 d1~d9를 돌아가며 배치 */
export type DeskChoice = DeskId | 'mix'
/** 책장 4종 — book.png 시트에서 추출, 전부 왼쪽 벽 원근.
 *  s1 높은 · s7 낮은 가로 · s9 중간 높이 · s11 좁은. 왼벽을 따라 여러 개 이어 붙일 수 있다. */
export type ShelfId = 's1' | 's7' | 's9' | 's11'
export type BoardId = 'chalk-green' | 'chalk-black' | 'glass'
export type DecoId = 'plant' | 'water-cooler' | 'rug' | 'string-lights'
/** 완성된 방 배경 5종 — room-preset-N.png 통짜 이미지. 벽·바닥·책장·칠판이 다 그려져 있다. */
export type RoomPresetId = 'r1' | 'r2' | 'r3' | 'r4' | 'r5'

/** 사용자가 방 만들기에서 고른 것 전부. localStorage에 이대로 저장된다. */
export type RoomConfig = {
  /** a-hub 스페이스 id — 이 방에 어느 스페이스 데이터를 표시할지 */
  space_id: string
  /** 표시용 스페이스 이름 (빌드 시점 스냅숏, 렌더 시 최신값으로 덮어씀) */
  space_name: string
  /** 방 배경 프리셋 — 이 위에 책상만 얹는다. 없으면(구버전 저장분) 'r1' */
  room?: RoomPresetId
  desk: DeskChoice
  /** 책상 수 — 'auto'면 에이전트 수를 따라감. 이전 버전 저장분에는 없을 수 있다. */
  desks?: 'auto' | number
  // ── 구버전 저장분 하위호환 (더 이상 선택 UI 없음, 렌더에도 안 씀) ──
  wallpaper?: WallpaperId
  floor?: FloorId
  shelves?: ShelfId[]
  board?: BoardId
  deco?: DecoId[]
  size?: number
  created_at: string
}

export type VariantOption<Id extends string> = {
  id: Id
  label: string
  /** 선택 UI 스와치 색 */
  swatch: string
  /** 스프라이트 썸네일 URL (있으면 스와치 대신 이미지 표시) */
  thumb?: string
}
