// 만든 방 저장소 — 백엔드 공유 저장(/api/life). 모든 뷰어가 같은 방을 본다.
// 앱 시작 시 loadRooms()로 서버 목록을 한 번 받아 캐시하고, 이후 조회는 캐시(동기)로 한다.
// 쓰기는 캐시를 즉시 갱신(낙관적)하고 서버에 반영한다. (구 localStorage 저장에서 승격)

import { resolveDeskId, resolveLifeId } from './life/catalog'
import type { LifeConfig } from './life/types'

const LEGACY_KEY = 'a-lens.life.v1' // 구 localStorage 저장분 — 서버로 1회 이관 후 제거
let _rooms: LifeConfig[] = []

// desk 구 id → 시트 기반 새 id, life 프리셋 정규화 (구버전 저장분 호환)
function normalize(rooms: LifeConfig[]): LifeConfig[] {
  return rooms.map((r) => ({
    ...r,
    desk: r.desk === 'mix' ? 'mix' : resolveDeskId(String(r.desk)),
    life: resolveLifeId(r.life),
  }))
}

async function postRoom(config: LifeConfig): Promise<void> {
  await fetch('/api/life', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  })
}

/** 앱 시작 시 1회: (구)localStorage 방을 서버로 이관하고, 서버 목록을 캐시에 채운다. */
export async function loadRooms(): Promise<void> {
  // 1) 이 브라우저의 로컬 방을 서버로 이관 (있을 때만, 1회)
  try {
    const raw = localStorage.getItem(LEGACY_KEY)
    if (raw) {
      const legacy = JSON.parse(raw)
      if (Array.isArray(legacy)) {
        for (const c of legacy) await postRoom(c as LifeConfig).catch(() => {})
      }
      localStorage.removeItem(LEGACY_KEY) // 중복 이관 방지
    }
  } catch {
    /* localStorage 접근 불가 환경은 무시 */
  }
  // 2) 서버 목록을 캐시에 로드
  try {
    const res = await fetch('/api/life')
    if (res.ok) {
      const data = await res.json()
      const rooms = Array.isArray(data) ? data : (data.rooms ?? [])
      _rooms = normalize(rooms as LifeConfig[])
    }
  } catch {
    /* 서버 실패 시 빈 목록 유지 */
  }
}

export function loadLife(): LifeConfig[] {
  return _rooms
}

export function getLife(spaceId: string): LifeConfig | undefined {
  return _rooms.find((r) => r.space_id === spaceId)
}

/** 같은 space_id의 방은 하나 — 캐시를 즉시 갱신(낙관적)하고 서버에 저장한다. */
export function saveLife(config: LifeConfig): void {
  _rooms = [..._rooms.filter((r) => r.space_id !== config.space_id), config]
  void postRoom(config).catch(() => {})
}

export function deleteLife(spaceId: string): void {
  _rooms = _rooms.filter((r) => r.space_id !== spaceId)
  void fetch(`/api/life/${encodeURIComponent(spaceId)}`, { method: 'DELETE' }).catch(() => {})
}
