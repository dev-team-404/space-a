import { describe, expect, it } from 'vitest';
import { pctLabel, shapeMix } from './model-mix-helpers';

const e = (tier: string, tokens: number) => ({ tier, tokens });

describe('shapeMix', () => {
  it('상위 3개 + 나머지는 기타로 합산(항목별 정확한 % 보존)', () => {
    const s = shapeMix([e('a', 800), e('b', 150), e('c', 30), e('d', 15), e('e', 5)]);
    expect(s.top.map((m) => m.tier)).toEqual(['a', 'b', 'c']);
    expect(s.top[0].pct).toBeCloseTo(80);
    expect(s.other?.pct).toBeCloseTo(2);
    expect(s.other?.items.map((m) => m.tier)).toEqual(['d', 'e']);
    expect(s.other?.items[0].pct).toBeCloseTo(1.5);
    expect(s.other?.items[1].pct).toBeCloseTo(0.5);
  });

  it('3개 이하면 기타 없음', () => {
    const s = shapeMix([e('a', 60), e('b', 40)]);
    expect(s.top).toHaveLength(2);
    expect(s.other).toBeNull();
    expect(s.top[1].pct).toBeCloseTo(40);
  });

  it('빈 입력은 빈 결과', () => {
    expect(shapeMix([])).toEqual({ top: [], other: null });
  });
});

describe('pctLabel', () => {
  it('1% 미만은 <1%', () => {
    expect(pctLabel(0.3)).toBe('<1%');
    expect(pctLabel(0.94)).toBe('<1%');
  });
  it('그 외는 반올림 정수', () => {
    expect(pctLabel(1.2)).toBe('1%');
    expect(pctLabel(80.19)).toBe('80%');
    expect(pctLabel(17.5)).toBe('18%');
  });
});
