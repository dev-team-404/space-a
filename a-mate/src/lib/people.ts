import type { LifePerson } from './api';

// G4 — 이름 클릭 → 홈 이동 (스펙 §4). 순수 모듈: api.ts는 type-only import만.

export interface VisitTarget { lifeId: string; label: string }
export interface VisitCtx {
  meId: string;
  myLifeId: string;
  currentLifeId: string;
  people: Pick<LifePerson, 'agent_id' | 'life_id'>[];
}

/** 이름 클릭 대상 판별. null = 이동 불가(일반 텍스트 렌더). */
export function resolveVisitTarget(agentId: string, displayName: string, ctx: VisitCtx): VisitTarget | null {
  if (!agentId || !ctx.meId || !ctx.myLifeId || !ctx.currentLifeId) return null;
  if (agentId === ctx.meId) {
    if (ctx.myLifeId === ctx.currentLifeId) return null;
    return { lifeId: ctx.myLifeId, label: '내 방으로 돌아가기' };
  }
  const person = ctx.people.find((p) => p.agent_id === agentId);
  if (!person || person.life_id === ctx.currentLifeId) return null;
  return { lifeId: person.life_id, label: `${displayName}네 놀러가기` };
}

export type PeopleFetcher = () => Promise<{ people: LifePerson[] }>;

const TTL_MS = 10_000; // 등록자 목록은 저빈도 변경 — lifeViewCache(500ms) 선례를 완화
let cache: { at: number; value: LifePerson[] } | null = null;
let inFlight: Promise<LifePerson[]> | null = null;

/** lifePeople 래퍼 — TTL 캐시 + in-flight 공유. 실패 시 직전 성공값(없으면 []) — 순단에 기능이 꺼지지 않게. */
export function getPeople(fetcher: PeopleFetcher): Promise<LifePerson[]> {
  if (cache && Date.now() - cache.at < TTL_MS) return Promise.resolve(cache.value);
  if (inFlight) return inFlight;
  inFlight = fetcher()
    .then((r) => { cache = { at: Date.now(), value: r.people }; return r.people; })
    .catch(() => cache?.value ?? ([] as LifePerson[]))
    .finally(() => { inFlight = null; });
  return inFlight;
}

/** 테스트 전용 — 모듈 캐시 초기화. */
export function resetPeopleCache(): void { cache = null; inFlight = null; }
