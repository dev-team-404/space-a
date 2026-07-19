// 아이소메트릭 격자 수학 (2:1 다이아몬드). room-server의 cell 격자와 같은 좌표계를 쓴다.
// cell (gx, gy) → 화면 px. 원점은 방의 맨 위 꼭짓점.

export const TILE_W = 64
export const TILE_H = 32
export const WALL_H = 130

export function isoX(gx: number, gy: number): number {
  return ((gx - gy) * TILE_W) / 2
}

export function isoY(gx: number, gy: number): number {
  return ((gx + gy) * TILE_H) / 2
}

/** 화면 깊이 정렬 키 — 뒤(작음)에서 앞(큼) 순으로 그린다. */
export function depth(gx: number, gy: number): number {
  return gx + gy
}
