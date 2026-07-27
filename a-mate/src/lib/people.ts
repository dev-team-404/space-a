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
