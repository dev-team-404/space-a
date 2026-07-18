// 방 = 데이터. 이미지 좌표 캘리브레이션 없이 격자 + variant 선택만으로 방을 그린다 (#44 4번).

/** 벽 4종 — floor-wall.png 시트에서 추출 (코너 스프라이트) */
export type WallpaperId = 'w1' | 'w2' | 'w3' | 'w4'
/** 바닥 4종 — floor-wall.png 시트에서 추출 (육각 슬래브 스프라이트) */
export type FloorId = 'f1' | 'f2' | 'f3' | 'f4'
/** 책상 9종 — desk.png 시트에서 추출한 스프라이트 (assets/kit/desk-d*.png) */
export type DeskId = 'd1' | 'd2' | 'd3' | 'd4' | 'd5' | 'd6' | 'd7' | 'd8' | 'd9'
/** 'mix'면 책상마다 d1~d9를 돌아가며 배치 */
export type DeskChoice = DeskId | 'mix'
/** 책장 11종 — book.png 시트에서 추출 (s1~s2 왼쪽 벽용, s3~s11 오른쪽 벽용) */
export type ShelfId = 's1' | 's2' | 's3' | 's4' | 's5' | 's6' | 's7' | 's8' | 's9' | 's10' | 's11'
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
  desk: DeskChoice
  /** 지식 책장 — 없으면(구버전 저장분) s1 */
  shelf?: ShelfId
  board: BoardId
  deco: DecoId[]
  /** 책상 수 — 'auto'면 에이전트 수를 따라감. 이전 버전 저장분에는 없을 수 있다. */
  desks?: 'auto' | number
  /** 방 크기 (격자 한 변 셀 수). 없으면 16. 책상이 더 필요하면 자동으로 늘어남 */
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
