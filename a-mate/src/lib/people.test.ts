import { describe, it, expect } from 'vitest';
import { resolveVisitTarget, type VisitCtx } from './people';
import type { LifePerson } from './api';

const P = (agent_id: string, life_id: string): LifePerson => ({ agent_id, name: 'x', life_id, is_friend: false });
const ctx = (over: Partial<VisitCtx> = {}): VisitCtx => ({
  meId: 'me', myLifeId: 'life-me', currentLifeId: 'life-cur',
  people: [P('a1', 'life-a1'), P('owner', 'life-cur')],
  ...over,
});

describe('resolveVisitTarget', () => {
  it('내 방에서 내 이름 → null (규칙 1)', () => {
    expect(resolveVisitTarget('me', '돌쇠', ctx({ currentLifeId: 'life-me' }))).toBeNull();
  });
  it('방문 중 내 이름 → 내 방으로 돌아가기 (규칙 2)', () => {
    expect(resolveVisitTarget('me', '돌쇠', ctx())).toEqual({ lifeId: 'life-me', label: '내 방으로 돌아가기' });
  });
  it('people에 없는 agent(탈퇴·미등록) → null (규칙 3)', () => {
    expect(resolveVisitTarget('ghost', '유령', ctx())).toBeNull();
  });
  it('현재 방 주인(그 방 봇 답글 행 포함) → null (규칙 4)', () => {
    expect(resolveVisitTarget('owner', '주인봇', ctx())).toBeNull();
  });
  it('다른 방 사람 → 그 방 타겟 + 표시 이름 라벨 (규칙 5)', () => {
    expect(resolveVisitTarget('a1', '김민지', ctx())).toEqual({ lifeId: 'life-a1', label: '김민지네 놀러가기' });
  });
  it('컨텍스트 미비(로딩 전 빈 문자열) → null', () => {
    expect(resolveVisitTarget('a1', '김민지', ctx({ meId: '' }))).toBeNull();
    expect(resolveVisitTarget('a1', '김민지', ctx({ myLifeId: '' }))).toBeNull();
    expect(resolveVisitTarget('a1', '김민지', ctx({ currentLifeId: '' }))).toBeNull();
    expect(resolveVisitTarget('', '김민지', ctx())).toBeNull();
  });
});
