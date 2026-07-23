import { describe, expect, it } from 'vitest';
import { coachTitle, ctxLine, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
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

describe('coachTitle', () => {
  it('R5 cross_session subtype → CLAUDE.md 카피', () => {
    expect(coachTitle('R5', { subtype: 'cross_session_claude_md' })).toContain('CLAUDE.md');
  });
  it('R5 context_drift subtype → 기존 다시읽기 제목', () => {
    expect(coachTitle('R5', { subtype: 'within_session_context_drift' })).toContain('다시 읽었어요');
  });
  it('R1은 기존 제목', () => {
    expect(coachTitle('R1', null)).toContain('MCP');
  });
  it('R6은 반복 지시 제목', () => {
    expect(coachTitle('R6', {})).toContain('같은 지시');
  });
  it('R24 컨텍스트 위생 카드 제목', () => {
    expect(coachTitle('R24', {})).toBe('작업을 바꿀 때 컨텍스트를 끊으면 더 좋아요');
  });
  it('알 수 없는 rule → fallback', () => {
    expect(coachTitle('RX', {})).toBe('아낄 수 있는 게 보여요');
  });
});

describe('isHiddenFinding', () => {
  it('사용자 처분 상태(resolved/dismissed)만 숨긴 항목', () => {
    expect(isHiddenFinding('resolved')).toBe(true);
    expect(isHiddenFinding('dismissed')).toBe(true);
  });
  it('내부 상태(new/pending/rejected)는 숨긴 항목이 아니다', () => {
    expect(isHiddenFinding('new')).toBe(false);
    expect(isHiddenFinding('pending')).toBe(false);
    expect(isHiddenFinding('rejected')).toBe(false);
  });
});
