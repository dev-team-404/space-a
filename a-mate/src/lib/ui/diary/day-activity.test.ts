import { describe, expect, it } from 'vitest';
import { compactTokens, hhmm, toolBars } from './day-activity';

describe('compactTokens', () => {
  it('백만 미만은 천 단위 구분만 — 좁은 폭에서도 읽히는 자릿수', () => {
    expect(compactTokens(0)).toBe('0');
    expect(compactTokens(3367)).toBe('3,367');
    expect(compactTokens(429577)).toBe('429,577');
  });
  it('백만 이상은 M으로 줄인다 — 240px 컬럼에 9자리가 안 들어간다', () => {
    expect(compactTokens(1_200_000)).toBe('1.2M');
    expect(compactTokens(12_345_678)).toBe('12.3M');
  });
});

describe('hhmm', () => {
  it('ISO 시각에서 시:분만 뽑는다', () => {
    expect(hhmm('2026-07-30T09:12:34+09:00')).toBe('09:12');
  });
  it('파싱 불가면 빈 문자열 — 라벨이 "Invalid Date"가 되지 않게', () => {
    expect(hhmm('nope')).toBe('');
  });
});

describe('toolBars', () => {
  const kinds: [string, number][] = [
    ['bash', 210], ['edit', 142], ['read', 98], ['write', 55], ['skill', 12], ['mcp_call', 3],
  ];
  it('상위 N개만, 최대값 기준 폭 비율', () => {
    const bars = toolBars(kinds, 5);
    expect(bars.map((b) => b.kind)).toEqual(['bash', 'edit', 'read', 'write', 'skill']);
    expect(bars[0].pct).toBe(100);
    expect(bars[1].pct).toBe(Math.round((142 / 210) * 100));
  });
  it('빈 입력은 빈 배열 — 0 나눗셈 없음', () => {
    expect(toolBars([], 5)).toEqual([]);
  });
  it('전부 0이어도 NaN이 되지 않는다', () => {
    expect(toolBars([['bash', 0]], 5)).toEqual([{ kind: 'bash', count: 0, pct: 0 }]);
  });
});
