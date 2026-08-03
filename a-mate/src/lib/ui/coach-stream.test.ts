import { describe, expect, it } from 'vitest';
import {
  activeCoachKeys, buildCoachStream, compareFirstSeen, isAnnouncementLive, liveContent,
  localDateString, parseSeenCoachKeys, unseenCoachCount,
} from './coach-stream';
import { CONTENT_CARD_LIMIT, pinnedNewsItem } from './coach-helpers';
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

/** 로컬 공지 하나. content 팩토리의 기본 태그가 personal이라 반드시 덮어쓴다. */
const ann = (over: Partial<ContentItem> = {}): ContentItem =>
  content({
    id: 'cc-announce-x', kind: 'news', dimension: null,
    trigger_tags: ['announcement'], title: '공지', body: '본문', ...over,
  });

const TODAY = '2026-08-10';
const NOW_MS = Date.parse('2026-08-10T00:00:00Z');
const daysBefore = (n: number) => new Date(NOW_MS - n * 24 * 60 * 60 * 1000).toISOString();

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

  // 아래 넷은 `partitionCoachItems`가 지키던 주장을 이어받은 것이다(그 함수는 이 PR에서
  // 호출부를 잃고 삭제됐다). 성질이 사라진 게 아니라 주장하는 자리가 옮겨왔다.
  it('처분된 항목이 상한 자리를 잡아먹지 않는다 — 거르기가 자르기보다 먼저다', () => {
    const rows = [
      // 처분 항목이 **더 최신**이라 자르기가 먼저면 상위 세 자리를 먹는다
      ...Array.from({ length: 3 }, (_, i) => tip(`D-${i}`, '2026-08-09T00:00:00Z', { status: 'dismissed' })),
      ...Array.from({ length: CONTENT_CARD_LIMIT }, (_, i) => tip(`T-${i}`, '2026-08-08T00:00:00Z')),
    ];
    const stream = buildCoachStream([], rows);
    expect(stream).toHaveLength(CONTENT_CARD_LIMIT);
    expect(stream.every((c) => c.key.startsWith('content:T-'))).toBe(true);
  });

  it('코칭 레슨도 콘텐츠 상한을 함께 먹는다 — 분류별로 따로 세지 않는다', () => {
    const rows = [
      ...Array.from({ length: 2 }, (_, i) => content({ id: `L${i}`, first_seen: '2026-08-09T00:00:00Z' })),
      ...Array.from({ length: CONTENT_CARD_LIMIT }, (_, i) => tip(`T-${i}`, '2026-08-08T00:00:00Z')),
    ];
    expect(buildCoachStream([], rows)).toHaveLength(CONTENT_CARD_LIMIT);
  });

  it('상한은 6이다 — ⑥에서 소스(로컬 공지)가 하나 늘어 4에서 올렸다', () => {
    expect(CONTENT_CARD_LIMIT).toBe(6);
  });

  it('빈 입력이면 빈 스트림', () => {
    expect(buildCoachStream([], [])).toEqual([]);
  });

  it('first_seen이 같으면 키로 안정 정렬한다 — 새로고침마다 순서가 튀지 않게', () => {
    const a = buildCoachStream([], [tip('b', '2026-08-01T00:00:00Z'), tip('a', '2026-08-01T00:00:00Z')]);
    const b = buildCoachStream([], [tip('a', '2026-08-01T00:00:00Z'), tip('b', '2026-08-01T00:00:00Z')]);
    expect(a.map((c) => c.key)).toEqual(b.map((c) => c.key));
  });
});

describe('고정 슬롯과 카드 상한', () => {
  // 상한은 한 곳(buildCoachStream)에서만 자른다. 고정 슬롯은 그 밖의 별도 1칸이라,
  // 호출부(CoachTab)가 고정된 항목을 빼고 넘긴다 (§2.2 파이프라인).
  it('고정된 항목을 제외하고 넘기면 스트림에 중복되지 않는다', () => {
    const rows = [
      ann({ id: 'pin', deadline: '2026-08-31', first_seen: '2026-08-09T00:00:00Z' }),
      tip('T1', '2026-08-08T00:00:00Z'),
    ];
    const pinned = pinnedNewsItem(rows, [], TODAY);
    expect(pinned?.id).toBe('pin');
    const stream = buildCoachStream([], rows.filter((r) => r.id !== pinned?.id));
    expect(stream.map((c) => c.key)).toEqual(['content:T1']);
  });
});

describe('isAnnouncementLive — 공지 수명 (§2.6)', () => {
  it('공지가 아닌 항목은 이 규칙과 무관하다', () => {
    expect(isAnnouncementLive(tip('T1', daysBefore(400)), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(content({ first_seen: daysBefore(400) }), TODAY, NOW_MS)).toBe(true);
  });

  it('기한이 있으면 그 날까지 — 당일은 살아 있고 지나면 내린다', () => {
    expect(isAnnouncementLive(ann({ deadline: '2026-08-10', first_seen: daysBefore(1) }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ deadline: '2026-08-09', first_seen: daysBefore(1) }), TODAY, NOW_MS)).toBe(false);
  });

  it('기한이 일수 규칙을 이긴다 — 「명시한 기간 동안만」이 우선이다', () => {
    // 처음 본 지 30일이 지났지만 공지가 명시한 기한이 아직 남았다
    const long = ann({ deadline: '2026-09-01', first_seen: daysBefore(30) });
    expect(isAnnouncementLive(long, TODAY, NOW_MS)).toBe(true);
  });

  it('기한이 없으면 일반 공지는 7일', () => {
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(6) }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(8) }), TODAY, NOW_MS)).toBe(false);
  });

  it('기한이 없으면 장애 공지는 2일 — 「마지막으로 표시한」 스냅샷이라 만료를 알 수 없다', () => {
    const emerg = (fs: string) => ann({ trigger_tags: ['announcement', 'notice'], first_seen: fs });
    expect(isAnnouncementLive(emerg(daysBefore(1)), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(emerg(daysBefore(3)), TODAY, NOW_MS)).toBe(false);
    // 같은 나이라도 일반 공지는 아직 살아 있다 — 둘을 가르는 것이 이 규칙의 핵심이다
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(3) }), TODAY, NOW_MS)).toBe(true);
  });

  it('깨진 기한은 무시하고 일수 규칙으로 — LLM 오추출 방어', () => {
    for (const bad of ['2026-13-40', '곧', '', '  ']) {
      expect(isAnnouncementLive(ann({ deadline: bad, first_seen: daysBefore(8) }), TODAY, NOW_MS), bad).toBe(false);
      expect(isAnnouncementLive(ann({ deadline: bad, first_seen: daysBefore(1) }), TODAY, NOW_MS), bad).toBe(true);
    }
  });

  it('처음 본 시각을 모르면 보이는 쪽 — 시각이 없다고 카드를 뺏지 않는다', () => {
    expect(isAnnouncementLive(ann({ first_seen: null }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ first_seen: '언젠가' }), TODAY, NOW_MS)).toBe(true);
  });
});

describe('liveContent', () => {
  it('수명이 끝난 공지만 걷어내고 나머지는 그대로 둔다', () => {
    const rows = [
      ann({ id: 'a-live', first_seen: daysBefore(1) }),
      ann({ id: 'a-dead', first_seen: daysBefore(9) }),
      tip('T-old', daysBefore(400)),
    ];
    expect(liveContent(rows, TODAY, NOW_MS).map((c) => c.id)).toEqual(['a-live', 'T-old']);
  });

  it('순서를 보존한다 — pinnedNewsItem이 score 내림차순을 전제한다', () => {
    const rows = [ann({ id: 'p3', score: 325 }), ann({ id: 'p2', score: 321 }), ann({ id: 'p1', score: 320 })]
      .map((c) => ({ ...c, first_seen: daysBefore(1) }));
    expect(liveContent(rows, TODAY, NOW_MS).map((c) => c.id)).toEqual(['p3', 'p2', 'p1']);
  });
});

describe('localDateString', () => {
  it('로컬 날짜를 YYYY-MM-DD로 — deadline과 같은 축으로 비교하기 위해', () => {
    expect(localDateString(new Date(2026, 7, 3))).toBe('2026-08-03');
    expect(localDateString(new Date(2026, 11, 25))).toBe('2026-12-25');
  });
});

describe('activeCoachKeys — 배지가 세는 대상 (§2.3)', () => {
  it('활성 finding과 활성 콘텐츠를 이름공간 붙인 키로 모은다', () => {
    const keys = activeCoachKeys(
      [finding({ dedup_key: 'F1' })],
      [tip('T1', '2026-08-01T00:00:00Z')],
    );
    expect(keys).toEqual(['finding:F1', 'content:T1']);
  });

  it('처분·판정중 항목은 세지 않는다 — ③이 지키라고 한 성질', () => {
    const keys = activeCoachKeys(
      [finding({ dedup_key: 'F-r', status: 'resolved' }),
       finding({ dedup_key: 'F-d', status: 'dismissed' }),
       finding({ dedup_key: 'F-p', status: 'pending' }),
       finding({ dedup_key: 'F-x', status: 'rejected' })],
      [tip('T-d', '2026-08-01T00:00:00Z', { status: 'dismissed' })],
    );
    expect(keys).toEqual([]);
  });

  it('상한을 적용하기 전의 활성 전부를 센다 (§2.3)', () => {
    const many = Array.from({ length: CONTENT_CARD_LIMIT + 4 }, (_, i) =>
      tip(`t-${i}`, '2026-08-01T00:00:00Z'));
    expect(activeCoachKeys([], many)).toHaveLength(CONTENT_CARD_LIMIT + 4);
  });
});

describe('unseenCoachCount — 안 본 개수', () => {
  it('본 적 없는 키만 센다', () => {
    expect(unseenCoachCount(['a', 'b', 'c'], ['a'])).toBe(2);
    expect(unseenCoachCount(['a', 'b'], ['a', 'b'])).toBe(0);
  });

  it('첫 실행이면 전부 안 본 것 — 처음 보는 게 맞다', () => {
    expect(unseenCoachCount(['a', 'b'], [])).toBe(2);
  });

  it('사라진 키가 집합에 남아 있어도 개수를 늘리지 않는다', () => {
    expect(unseenCoachCount(['a'], ['a', 'gone-1', 'gone-2'])).toBe(0);
  });

  it('resolved→new 복귀를 잡는다 — first_seen 비교가 놓치던 전이 (§4.5)', () => {
    // 어제 「해결함」을 눌러 활성 집합에서 빠졌고, 오늘 재발로 같은 키가 돌아왔다.
    // first_seen은 그대로 과거라 시각 비교로는 안 잡힌다.
    const revived = finding({ dedup_key: 'F1', status: 'new', first_seen: '2026-07-01T00:00:00Z' });
    const seenWhileResolved = activeCoachKeys([finding({ dedup_key: 'F1', status: 'resolved' })], []);
    expect(seenWhileResolved).toEqual([]);
    expect(unseenCoachCount(activeCoachKeys([revived], []), seenWhileResolved)).toBe(1);
  });

  it('pending→new 통과를 잡는다 — 판정이 며칠 걸려도', () => {
    const seenWhilePending = activeCoachKeys([finding({ dedup_key: 'F2', status: 'pending' })], []);
    const passed = finding({ dedup_key: 'F2', status: 'new', first_seen: '2026-07-01T00:00:00Z' });
    expect(unseenCoachCount(activeCoachKeys([passed], []), seenWhilePending)).toBe(1);
  });
});

describe('parseSeenCoachKeys — 저장값 해석 (localStorage 밖의 순수 부분)', () => {
  it('문자열 배열만 통과시킨다', () => {
    expect(parseSeenCoachKeys('["finding:F1","content:T1"]')).toEqual(['finding:F1', 'content:T1']);
    expect(parseSeenCoachKeys('["a",42,null,"b"]')).toEqual(['a', 'b']);
  });

  it('없거나 깨진 값은 빈 배열 — 배지 때문에 앱이 죽지 않는다', () => {
    expect(parseSeenCoachKeys(null)).toEqual([]);
    expect(parseSeenCoachKeys('{oops')).toEqual([]);
    expect(parseSeenCoachKeys('{"not":"an array"}')).toEqual([]);
    expect(parseSeenCoachKeys('"just a string"')).toEqual([]);
  });
});
