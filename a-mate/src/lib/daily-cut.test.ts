import { describe, it, expect } from 'vitest';
import { dailyCutEnabled } from './daily-cut';

describe('dailyCutEnabled', () => {
  it('기본 off — 미설정·빈값·false는 비활성 (visit_guestbook과 달리 옵트인)', () => {
    expect(dailyCutEnabled({})).toBe(false);
    expect(dailyCutEnabled({ daily_cut_enabled: '' })).toBe(false);
    expect(dailyCutEnabled({ daily_cut_enabled: 'false' })).toBe(false);
  });
  it("'true'일 때만 활성", () => {
    expect(dailyCutEnabled({ daily_cut_enabled: 'true' })).toBe(true);
  });
});
