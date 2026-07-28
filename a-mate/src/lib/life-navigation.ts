import type { LifeListEntry } from './api';

export interface LifeDestination {
  lifeId: string;
  label: string;
  occupants: number | null;
  kind: 'home' | 'life';
}

/** 마스코트와 미니홈피가 공유하는 이동 가능 목적지 목록. */
export function lifeDestinations(
  entries: LifeListEntry[],
  myLifeId: string,
  currentLifeId: string,
): LifeDestination[] {
  if (!myLifeId || !currentLifeId) return [];
  const destinations: LifeDestination[] = [];
  if (currentLifeId !== myLifeId) {
    const myLife = entries.find((entry) => entry.life_id === myLifeId);
    destinations.push({
      lifeId: myLifeId,
      label: '내 방으로 돌아가기',
      occupants: myLife?.occupants ?? null,
      kind: 'home',
    });
  }
  for (const entry of entries) {
    if (entry.life_id === myLifeId || entry.life_id === currentLifeId) continue;
    destinations.push({
      lifeId: entry.life_id,
      label: `${entry.owner_name}의 방`,
      occupants: entry.occupants,
      kind: 'life',
    });
  }
  return destinations;
}
