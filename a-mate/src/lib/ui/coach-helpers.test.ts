import { describe, expect, it } from 'vitest';
import { coachTitle, ctxLine, evidenceChip, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
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
  it('등록된 룰만 고유 제목을 갖는다', () => {
    expect(coachTitle('R6', {})).toContain('같은 지시');
    expect(coachTitle('R7', {})).toContain('모델');
    expect(coachTitle('R8', {})).toContain('MCP');
  });
  it('은퇴한 룰은 폴백 제목 — 죽은 매핑을 남기지 않는다', () => {
    for (const retired of ['R1', 'R2', 'R5', 'R9', 'R10', 'R11', 'R12', 'R24']) {
      expect(coachTitle(retired, {}), retired).toBe('아낄 수 있는 게 보여요');
    }
  });
  it('알 수 없는 rule → 폴백', () => {
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

describe('evidenceChip', () => {
  it('R6은 세션 수를 센다', () => {
    expect(evidenceChip('R6', { session_count: 4 })).toBe('4개 세션');
  });
  it('R7은 집계된 세션 수를 센다', () => {
    expect(evidenceChip('R7', { total_sessions: 5 })).toBe('5개 세션');
  });
  it('R8은 대형 결과 횟수를 센다', () => {
    expect(evidenceChip('R8', { large_result_count: 12 })).toBe('12회 대형 결과');
  });
  it('occurrences·last_seen은 스캔 지표라 칩 재료가 아니다', () => {
    // 룰별 evidence 키가 없으면 다른 값이 있어도 칩은 없다
    expect(evidenceChip('R6', { occurrences: 99, last_seen: '2026-08-02' })).toBeNull();
  });
  it('키가 없거나 숫자가 아니거나 0이면 칩을 생략한다', () => {
    expect(evidenceChip('R6', {})).toBeNull();
    expect(evidenceChip('R6', { session_count: '4' })).toBeNull();
    expect(evidenceChip('R6', { session_count: 0 })).toBeNull();
    expect(evidenceChip('R7', { total_sessions: Number.NaN })).toBeNull();
  });
  it('evidence가 객체가 아니면 칩을 생략한다', () => {
    expect(evidenceChip('R6', null)).toBeNull();
    expect(evidenceChip('R6', 'junk')).toBeNull();
    expect(evidenceChip('R6', undefined)).toBeNull();
  });
  it('칩 규칙이 없는 룰은 생략한다', () => {
    expect(evidenceChip('RX', { session_count: 4 })).toBeNull();
  });
});
