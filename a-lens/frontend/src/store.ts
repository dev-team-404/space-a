// 만든 방 저장소 — 지금은 localStorage. 방 공유가 필요해지면 backend 저장으로 승격한다.

import { resolveDeskId, resolveLifeId } from './life/catalog'
import type { LifeConfig } from './life/types'

const KEY = 'a-lens.life.v1'

export function loadLife(): LifeConfig[] {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    // 구버전 저장분 마이그레이션 (desk 구 id → 시트 기반 새 id, life 프리셋 정규화)
    return (parsed as LifeConfig[]).map((r) => ({
      ...r,
      desk: r.desk === 'mix' ? 'mix' : resolveDeskId(String(r.desk)),
      life: resolveLifeId(r.life),
    }))
  } catch {
    return []
  }
}

export function getLife(spaceId: string): LifeConfig | undefined {
  return loadLife().find((r) => r.space_id === spaceId)
}

/** 같은 space_id의 방은 하나 — 다시 만들면 덮어쓴다. */
export function saveLife(config: LifeConfig): void {
  const life = loadLife().filter((r) => r.space_id !== config.space_id)
  life.push(config)
  localStorage.setItem(KEY, JSON.stringify(life))
}

export function deleteLife(spaceId: string): void {
  localStorage.setItem(KEY, JSON.stringify(loadLife().filter((r) => r.space_id !== spaceId)))
}
