import { describe, expect, it } from 'vitest';
import { monthGrid, shiftMonth } from './calendar';

describe('monthGrid', () => {
  it('일요일 시작 6주(42칸), 2026-07은 수요일 시작', () => {
    const g = monthGrid(2026, 7); // 2026-07-01 = 수요일
    expect(g.length).toBe(42);
    expect(g[0]).toEqual({ date: '2026-06-28', day: 28, inMonth: false }); // 앞 일요일
    expect(g[3]).toEqual({ date: '2026-07-01', day: 1, inMonth: true });
    expect(g[33]).toEqual({ date: '2026-07-31', day: 31, inMonth: true });
    expect(g[34].inMonth).toBe(false); // 8월 칸
  });
  it('연도 경계 패딩', () => {
    const g = monthGrid(2026, 1); // 2026-01-01 = 목요일
    expect(g[4]).toEqual({ date: '2026-01-01', day: 1, inMonth: true });
    expect(g[0].date).toBe('2025-12-28');
  });
});

describe('shiftMonth', () => {
  it('연도 넘김', () => {
    expect(shiftMonth(2026, 1, -1)).toEqual([2025, 12]);
    expect(shiftMonth(2026, 12, 1)).toEqual([2027, 1]);
    expect(shiftMonth(2026, 7, -1)).toEqual([2026, 6]);
  });
});
