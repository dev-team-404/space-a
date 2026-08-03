import { describe, expect, it } from 'vitest';
import { buildCoachStream, compareFirstSeen } from './coach-stream';
import { CONTENT_CARD_LIMIT } from './coach-helpers';
import type { CoachFinding, ContentItem } from '../api';

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

/** 학습 카드 하나 — first_seen과 id만 다르게 찍는다 */
const tip = (id: string, firstSeen: string | null, over: Partial<ContentItem> = {}): ContentItem =>
  content({ id, trigger_tags: [], dimension: 'automation', first_seen: firstSeen, ...over });

describe('compareFirstSeen', () => {
  it('최신이 앞 — 내림차순이다', () => {
    expect(compareFirstSeen('2026-08-03T00:00:00Z', '2026-08-01T00:00:00Z')).toBeLessThan(0);
    expect(compareFirstSeen('2026-08-01T00:00:00Z', '2026-08-03T00:00:00Z')).toBeGreaterThan(0);
    expect(compareFirstSeen('2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')).toBe(0);
  });

  it('시각을 모르는 항목은 맨 뒤 — 최신인 척하지 않는다', () => {
    expect(compareFirstSeen(null, '2020-01-01T00:00:00Z')).toBeGreaterThan(0);
    expect(compareFirstSeen('2020-01-01T00:00:00Z', null)).toBeLessThan(0);
    expect(compareFirstSeen(null, null)).toBe(0);
    expect(compareFirstSeen(undefined, null)).toBe(0);
  });
});

describe('buildCoachStream', () => {
  it('분류와 무관하게 first_seen 내림차순 한 줄로 깐다 (§2.2)', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F1', first_seen: '2026-08-02T00:00:00Z' })],
      [
        tip('T-new', '2026-08-03T00:00:00Z'),
        tip('T-old', '2026-08-01T00:00:00Z'),
      ],
    );
    expect(stream.map((c) => c.key)).toEqual(['content:T-new', 'finding:F1', 'content:T-old']);
  });

  it('행동 가능한 코칭이 소식 아래로 밀리는 것을 의도적으로 수용한다', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F1', first_seen: '2026-08-01T00:00:00Z' })],
      [tip('N1', '2026-08-05T00:00:00Z', { trigger_tags: ['changelog'], dimension: null })],
    );
    expect(stream[0].kind).toBe('news');
    expect(stream[1].kind).toBe('coaching');
  });

  it('키는 이름공간을 붙인다 — dedup_key와 콘텐츠 id가 충돌할 수 있다', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'X', first_seen: '2026-08-02T00:00:00Z' })],
      [tip('X', '2026-08-01T00:00:00Z')],
    );
    expect(new Set(stream.map((c) => c.key)).size).toBe(2);
  });

  it('personal 레슨은 코칭 분류로 문법 A 뷰모델을 싣는다', () => {
    const [card] = buildCoachStream([], [content({ first_seen: '2026-08-01T00:00:00Z' })]);
    expect(card.kind).toBe('coaching');
    expect(card.kind === 'coaching' && card.log.source).toBe('lesson');
  });

  it('활성만 스트림에 오른다 — 처분 항목은 아래 별도 줄이다 (§2.5)', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F-done', status: 'resolved', first_seen: '2026-08-09T00:00:00Z' }),
       finding({ dedup_key: 'F-pend', status: 'pending', first_seen: '2026-08-09T00:00:00Z' })],
      [tip('T-gone', '2026-08-09T00:00:00Z', { status: 'dismissed' })],
    );
    expect(stream).toEqual([]);
  });

  it('상한은 콘텐츠에만, 그리고 first_seen 기준으로 자른다 (§4.2)', () => {
    // 오래된 고득점 팁이 상한을 채우고 방금 온 항목이 뒤에 있는 배치.
    // score 기준으로 자르면 최신 항목이 통째로 사라진다.
    const older = Array.from({ length: CONTENT_CARD_LIMIT }, (_, i) =>
      tip(`old-${i}`, '2026-08-01T00:00:00Z', { score: 500 }));
    const fresh = tip('fresh', '2026-08-09T00:00:00Z', { score: 300 });
    const stream = buildCoachStream([], [...older, fresh]);
    expect(stream).toHaveLength(CONTENT_CARD_LIMIT);
    expect(stream[0].key).toBe('content:fresh');
  });

  it('룰 finding은 상한 대상이 아니다 — 콘텐츠가 상한을 채워도 전부 남는다', () => {
    const many = Array.from({ length: CONTENT_CARD_LIMIT + 3 }, (_, i) =>
      tip(`t-${i}`, `2026-08-0${(i % 9) + 1}T00:00:00Z`));
    const findings = Array.from({ length: 5 }, (_, i) =>
      finding({ dedup_key: `F${i}`, first_seen: '2026-08-02T00:00:00Z' }));
    const stream = buildCoachStream(findings, many);
    expect(stream.filter((c) => c.key.startsWith('finding:'))).toHaveLength(5);
    expect(stream.filter((c) => c.key.startsWith('content:'))).toHaveLength(CONTENT_CARD_LIMIT);
  });

  it('first_seen이 같으면 키로 안정 정렬한다 — 새로고침마다 순서가 튀지 않게', () => {
    const a = buildCoachStream([], [tip('b', '2026-08-01T00:00:00Z'), tip('a', '2026-08-01T00:00:00Z')]);
    const b = buildCoachStream([], [tip('a', '2026-08-01T00:00:00Z'), tip('b', '2026-08-01T00:00:00Z')]);
    expect(a.map((c) => c.key)).toEqual(b.map((c) => c.key));
  });
});
