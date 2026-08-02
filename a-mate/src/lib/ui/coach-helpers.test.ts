import { describe, expect, it } from 'vitest';
import {
  coachTitle, ctxLine, evidenceChip, isHiddenFinding, partitionCoachItems, sessionIdsOf,
  splitLessonBody, toLearnCardView, toLessonCardView, toLogCardView, totalSessionsOf,
} from './coach-helpers';
import type { CoachFinding, ContentItem, SessionCtxItem } from '../api';

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

describe('splitLessonBody', () => {
  it('원리·이렇게 마커로 세 조각을 나눈다', () => {
    const r = splitLessonBody(
      '당신 로그: 최근 세션에서 PowerShell 3회 오류가 났다가 회복했어요.\n' +
      '• 원리: 계획 없이 바로 손대면 헤매요.\n' +
      '• 이렇게: Shift+Tab으로 계획부터 세우세요.'
    );
    expect(r.evidence).toBe('당신 로그: 최근 세션에서 PowerShell 3회 오류가 났다가 회복했어요.');
    expect(r.principle).toBe('계획 없이 바로 손대면 헤매요.');
    expect(r.action).toBe('Shift+Tab으로 계획부터 세우세요.');
  });
  it('여러 줄에 걸친 조각을 이어 붙인다', () => {
    const r = splitLessonBody('앞줄\n• 원리: 첫 줄\n이어지는 줄\n• 이렇게: 행동');
    expect(r.principle).toBe('첫 줄 이어지는 줄');
    expect(r.action).toBe('행동');
  });
  it('👉 첫걸음도 행동 마커로 인정한다 (lesson-frontier)', () => {
    const r = splitLessonBody('당신 로그: 남은 다음 단계는 스킬 재사용이에요.\n👉 첫걸음: SKILL.md를 하나 만들어 보세요.');
    expect(r.evidence).toBe('당신 로그: 남은 다음 단계는 스킬 재사용이에요.');
    expect(r.principle).toBeNull();
    expect(r.action).toBe('SKILL.md를 하나 만들어 보세요.');
  });
  it('마커가 없으면 전부 근거로 — 본문을 잃지 않는다', () => {
    const body = '당신 로그: 최근 세션 5개가 UI 작업이었어요.\n설치된 플러그인이 쓰이지 않았어요.';
    const r = splitLessonBody(body);
    expect(r.evidence).toBe('당신 로그: 최근 세션 5개가 UI 작업이었어요. 설치된 플러그인이 쓰이지 않았어요.');
    expect(r.principle).toBeNull();
    expect(r.action).toBeNull();
  });
  it('빈 본문은 전부 null', () => {
    expect(splitLessonBody('')).toEqual({ evidence: null, principle: null, action: null });
    expect(splitLessonBody('   \n  ')).toEqual({ evidence: null, principle: null, action: null });
  });
});

const finding = (over: Partial<CoachFinding> = {}): CoachFinding => ({
  rule_id: 'R6', severity: 'warn', scope_host: 'Windows', scope_project: null,
  scope_kind: 'pattern', scope_ref: 'pattern:ab', evidence: { session_count: 4 },
  est_tokens_saved: 0, prescription: null, dedup_key: 'R6|Windows|ab',
  last_seen: null, occurrences: 1, status: 'new',
  detail: '같은 지시를 4개 세션에서 반복했어요', suggested_action: '스킬로 묶으세요',
  fix_command: null, session: null, ...over,
});

const content = (over: Partial<ContentItem> = {}): ContentItem => ({
  id: 'lesson-struggle', kind: 'tip', dimension: null, title: '도구 오류 3회',
  body: '당신 로그: 오류가 났어요.\n• 원리: 계획 없이 손대면 헤매요.\n• 이렇게: 플랜 모드를 쓰세요.',
  source_url: 'https://example.test/guide', trigger_tags: ['personal', 'plan'],
  score: 550, status: 'new', personal: null, ...over,
});

describe('partitionCoachItems', () => {
  it('룰 finding과 personal 레슨은 로그 섹션, 나머지는 배움 섹션', () => {
    const { log, learn } = partitionCoachItems(
      [finding()],
      [content(), content({ id: 'T-L1-1', trigger_tags: [], dimension: 'context_hygiene', title: '내장 팁' })],
    );
    expect(log.map((v) => v.key)).toEqual(['R6|Windows|ab', 'lesson-struggle']);
    expect(learn.map((v) => v.id)).toEqual(['T-L1-1']);
  });
  it('finding이 레슨보다 앞에 온다', () => {
    const { log } = partitionCoachItems([finding()], [content()]);
    expect(log[0].source).toBe('finding');
    expect(log[1].source).toBe('lesson');
  });
  it('한쪽이 비어도 동작한다', () => {
    expect(partitionCoachItems([], []).log).toEqual([]);
    expect(partitionCoachItems([], []).learn).toEqual([]);
  });
  // list_content는 LIMIT 없이 score>=0인 행을 전부 준다. 옛 「오늘의 배움」 컨테이너가
  // items[0]+slice(1,4)로 4건만 보였으므로, 그 상한을 여기서 유지한다.
  it('콘텐츠는 상위 4건까지만 카드가 된다', () => {
    const many = Array.from({ length: 7 }, (_, i) => content({ id: `T-${i}`, trigger_tags: [] }));
    const { log, learn } = partitionCoachItems([finding()], many);
    expect(learn.map((v) => v.id)).toEqual(['T-0', 'T-1', 'T-2', 'T-3']);
    expect(log).toHaveLength(1); // 룰 finding은 상한과 무관
  });
  it('상한은 두 섹션 합계에 적용된다', () => {
    const { log, learn } = partitionCoachItems([], [
      content({ id: 'L1' }),
      content({ id: 'L2' }),
      content({ id: 'T1', trigger_tags: [] }),
      content({ id: 'T2', trigger_tags: [] }),
      content({ id: 'T3', trigger_tags: [] }),
    ]);
    expect(log.map((v) => v.key)).toEqual(['L1', 'L2']);
    expect(learn.map((v) => v.id)).toEqual(['T1', 'T2']);
  });
});

describe('toLogCardView', () => {
  it('finding을 문법 A 슬롯으로 정규화한다', () => {
    const v = toLogCardView(finding({ judgment: { reason: '재사용 가치 높음' } }));
    expect(v.source).toBe('finding');
    expect(v.icon).toBe('⚠');
    expect(v.title).toContain('같은 지시');
    expect(v.evidence).toBe('같은 지시를 4개 세션에서 반복했어요');
    expect(v.chip).toBe('4개 세션');
    expect(v.reason).toBe('재사용 가치 높음');
    expect(v.principle).toBeNull();      // 원리는 레슨 전용
    expect(v.action).toBe('스킬로 묶으세요');
  });
  it('severity에 따라 아이콘이 갈린다', () => {
    expect(toLogCardView(finding({ severity: 'suggest' })).icon).toBe('💡');
    expect(toLogCardView(finding({ severity: 'info' })).icon).toBe('ℹ');
  });
  it('레슨은 본문을 쪼개 담고 칩·판정이 없다', () => {
    const v = toLessonCardView(content());
    expect(v.source).toBe('lesson');
    expect(v.evidence).toBe('당신 로그: 오류가 났어요.');
    expect(v.principle).toBe('계획 없이 손대면 헤매요.');
    expect(v.action).toBe('플랜 모드를 쓰세요.');
    expect(v.chip).toBeNull();
    expect(v.reason).toBeNull();
    expect(v.sourceUrl).toBe('https://example.test/guide');
  });
  // enrich_personal은 태그로 붙으므로 레슨 본문과 다른 얘기일 수 있다 —
  // lesson-cache(태그 subagent)의 personal은 캐시가 아니라 서브에이전트 수치다.
  // 하나를 버리면 제목·원리·행동과 어긋난 근거만 남으므로 둘 다 남긴다.
  it('personal이 있으면 본문 근거보다 앞에 오되 본문을 버리지 않는다', () => {
    const v = toLessonCardView(content({ personal: '당신 로그: 서브에이전트 3건 사용 중' }));
    expect(v.evidence).toBe('당신 로그: 서브에이전트 3건 사용 중\n당신 로그: 오류가 났어요.');
  });
  it('둘이 같은 문장이면 한 번만 보인다', () => {
    const v = toLessonCardView(content({ personal: '당신 로그: 오류가 났어요.' }));
    expect(v.evidence).toBe('당신 로그: 오류가 났어요.');
  });
  it('본문에 근거 슬롯이 없으면 personal만', () => {
    const v = toLessonCardView(content({ body: '• 이렇게: 플랜 모드', personal: '당신 로그: 실측 한 줄' }));
    expect(v.evidence).toBe('당신 로그: 실측 한 줄');
  });
});

describe('toLearnCardView', () => {
  it('출처별 배지', () => {
    expect(toLearnCardView(content({ kind: 'news', trigger_tags: ['changelog'] })).badge).toBe('소식');
    expect(toLearnCardView(content({ trigger_tags: ['boris'] })).badge).toBe('Boris');
    expect(toLearnCardView(content({ trigger_tags: ['team'] })).badge).toBe('팀');
    expect(toLearnCardView(content({ trigger_tags: [], dimension: 'skill_reuse' })).badge).toBe('스킬로 반복 줄이기');
    expect(toLearnCardView(content({ trigger_tags: [], dimension: null })).badge).toBe('배움');
  });
  it('본문이 비면 summary는 null', () => {
    expect(toLearnCardView(content({ trigger_tags: [], body: '  ' })).summary).toBeNull();
  });
});
