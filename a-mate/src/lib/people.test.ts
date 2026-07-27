import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getPeople, resetPeopleCache, resolveVisitTarget, type VisitCtx } from './people';
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

describe('getPeople', () => {
  beforeEach(() => { resetPeopleCache(); vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('TTL 내 재호출은 fetch 1회', async () => {
    const fetcher = vi.fn().mockResolvedValue({ people: [P('a', 'l')] });
    await getPeople(fetcher);
    await getPeople(fetcher);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('TTL(10초) 경과 후 재fetch', async () => {
    const fetcher = vi.fn().mockResolvedValue({ people: [] });
    await getPeople(fetcher);
    vi.advanceTimersByTime(10_001);
    await getPeople(fetcher);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it('동시 호출은 in-flight 공유', async () => {
    let resolve!: (v: { people: LifePerson[] }) => void;
    const fetcher = vi.fn(() => new Promise<{ people: LifePerson[] }>((r) => { resolve = r; }));
    const p1 = getPeople(fetcher);
    const p2 = getPeople(fetcher);
    resolve({ people: [P('a', 'l')] });
    expect(await p1).toEqual(await p2);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('실패 시 빈 배열 (기능만 조용히 비활성)', async () => {
    const fetcher = vi.fn().mockRejectedValue(new Error('down'));
    await expect(getPeople(fetcher)).resolves.toEqual([]);
  });
  it('TTL 경과 후 순단이면 직전 성공값 유지 (스테일 허용)', async () => {
    const ok = vi.fn().mockResolvedValue({ people: [P('a', 'l')] });
    await getPeople(ok);
    vi.advanceTimersByTime(10_001);
    const bad = vi.fn().mockRejectedValue(new Error('down'));
    await expect(getPeople(bad)).resolves.toEqual([P('a', 'l')]);
    expect(bad).toHaveBeenCalledTimes(1);
  });
});
