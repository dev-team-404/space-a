// 만든 방 저장소 — 지금은 localStorage. 방 공유가 필요해지면 backend 저장으로 승격한다.

import type { RoomConfig } from './room/types'

const KEY = 'a-lens.rooms.v1'

export function loadRooms(): RoomConfig[] {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? (parsed as RoomConfig[]) : []
  } catch {
    return []
  }
}

export function getRoom(spaceId: string): RoomConfig | undefined {
  return loadRooms().find((r) => r.space_id === spaceId)
}

/** 같은 space_id의 방은 하나 — 다시 만들면 덮어쓴다. */
export function saveRoom(config: RoomConfig): void {
  const rooms = loadRooms().filter((r) => r.space_id !== config.space_id)
  rooms.push(config)
  localStorage.setItem(KEY, JSON.stringify(rooms))
}

export function deleteRoom(spaceId: string): void {
  localStorage.setItem(KEY, JSON.stringify(loadRooms().filter((r) => r.space_id !== spaceId)))
}
