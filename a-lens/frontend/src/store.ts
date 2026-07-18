// 만든 방 저장소 — 지금은 localStorage. 방 공유가 필요해지면 backend 저장으로 승격한다.

import { resolveCharacterId, resolveDeskId, resolveRoomId } from './room/catalog'
import type { CharacterId, RoomConfig } from './room/types'

const KEY = 'a-lens.rooms.v1'
const CHAR_KEY = 'a-lens.character.v1'

/** 내 캐릭터 — 전역 설정(방과 무관). 어느 방에 들어가든 에이전트 자리를 이 스프라이트로 그린다. */
export function loadCharacter(): CharacterId {
  return resolveCharacterId(localStorage.getItem(CHAR_KEY) ?? undefined)
}

export function saveCharacter(id: CharacterId): void {
  localStorage.setItem(CHAR_KEY, id)
}

export function loadRooms(): RoomConfig[] {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    // 구버전 저장분 마이그레이션 (desk 구 id → 시트 기반 새 id, room 프리셋 정규화)
    return (parsed as RoomConfig[]).map((r) => ({
      ...r,
      desk: r.desk === 'mix' ? 'mix' : resolveDeskId(String(r.desk)),
      room: resolveRoomId(r.room),
    }))
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
