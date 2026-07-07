import { describe, expect, it } from 'vitest';
import { ctxLine, sessionIdsOf, totalSessionsOf } from './coach-helpers';
import type { SessionCtxItem } from '../api';

const item = (over: Partial<SessionCtxItem> = {}): SessionCtxItem => ({
  session_id: 's1', project_id: 'd--proj', first_ts: null, cwd: null, first_prompt: null, ...over,
});

describe('coach-helpers', () => {
  it('sessionIdsOf extracts string array from evidence', () => {
    expect(sessionIdsOf({ session_ids: ['a', 'b'] })).toEqual(['a', 'b']);
    expect(sessionIdsOf({ session_ids: ['a', 42] })).toEqual(['a']); // 비문자열 제거
    expect(sessionIdsOf({})).toEqual([]);
    expect(sessionIdsOf(null)).toEqual([]);
    expect(sessionIdsOf('junk')).toEqual([]);
  });

  it('totalSessionsOf falls back to ids length', () => {
    expect(totalSessionsOf({ total_sessions: 81 }, 5)).toBe(81);
    expect(totalSessionsOf({}, 5)).toBe(5);
    expect(totalSessionsOf(null, 0)).toBe(0);
  });

  it('ctxLine formats project and timestamp', () => {
    const line = ctxLine(item({ first_ts: '2026-07-06T09:30:00Z' }));
    expect(line).toContain('d--proj');
    expect(line).toMatch(/\d{2}-\d{2} \d{2}:\d{2}/);
    expect(ctxLine(item())).toBe('d--proj');
  });

  it('ctxLine appends first_prompt snippet when present', () => {
    const line = ctxLine(item({ first_prompt: '커밋 요약해줘' }));
    expect(line).toContain('d--proj');
    expect(line).toContain('커밋 요약해줘');
  });
});
