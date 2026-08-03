import { describe, expect, it } from 'vitest';
import { announcementNotice, diaryNotice, findingNotice, guestbookNotice, noticeDest, noticeStamp, occasionNotice, pushNotice, visitNotice, type Notice } from './notices';

const n = (text: string): Notice => ({ ts: '2026-07-05T10:00:00Z', kind: 'finding', text });

describe('pushNotice', () => {
  it('최신이 앞, 최대 20건 유지', () => {
    let list: Notice[] = [];
    for (let i = 0; i < 25; i++) list = pushNotice(list, n(`알림 ${i}`));
    expect(list.length).toBe(20);
    expect(list[0].text).toBe('알림 24');
    expect(list[19].text).toBe('알림 5');
  });
  it('원본 배열을 변형하지 않는다', () => {
    const orig: Notice[] = [n('a')];
    const next = pushNotice(orig, n('b'));
    expect(orig.length).toBe(1);
    expect(next[0].text).toBe('b');
  });
});

describe('notice 팩토리', () => {
  it('finding: N건 문구 + 절약량 1위 dedup_key가 target', () => {
    const n = findingNotice(
      [
        { dedup_key: 'k-low', est_tokens_saved: 10 },
        { dedup_key: 'k-top', est_tokens_saved: 500 },
      ],
      '2026-07-12T10:00:00Z',
    );
    expect(n.kind).toBe('finding');
    expect(n.text).toBe('코칭 지적 2건이 도착했어요');
    expect(n.target).toBe('k-top');
    expect(n.ts).toBe('2026-07-12T10:00:00Z');
  });
  it('diary: 날짜 문구 + 날짜가 target', () => {
    const n = diaryNotice('2026-07-11', '2026-07-12T07:00:00Z');
    expect(n.kind).toBe('diary');
    expect(n.text).toBe('2026-07-11 일기가 나왔어요');
    expect(n.target).toBe('2026-07-11');
  });
  it('occasion: 첫 라벨 문구, target 없음', () => {
    const n = occasionNotice(['크리스마스', '함께한 지 100일'], '2026-07-12T00:00:00Z');
    expect(n.kind).toBe('occasion');
    expect(n.text).toBe('오늘은 크리스마스!');
    expect(n.target).toBeUndefined();
  });
  it('visit: 단수·복수 문구, target 없음(클릭 불가)', () => {
    const one = visitNotice([{ visitor_name: '준녕' }], '2026-07-29T10:00:00Z');
    expect(one.kind).toBe('visit');
    expect(one.text).toBe('준녕님이 방에 다녀갔어요');
    expect(one.target).toBeUndefined();
    expect(noticeDest(one)).toBeNull();
    const many = visitNotice([{ visitor_name: '가' }, { visitor_name: '나' }], '2026-07-29T10:00:00Z');
    expect(many.text).toBe('가님 외 1명이 방에 다녀갔어요');
  });
  it('announcement: 새 공지 문구 + 첫 공지 id가 target + 코칭 탭 dest', () => {
    const one = announcementNotice(
      [{ id: 'cc-announce-fable', title: 'Fable 5 팀 플랜 포함' }],
      '2026-08-03T10:00:00Z',
    );
    expect(one.kind).toBe('announcement');
    expect(one.text).toBe('새 소식 — Fable 5 팀 플랜 포함');
    expect(noticeDest(one)).toEqual({ tab: 'coach', target: 'cc-announce-fable' });
    const many = announcementNotice(
      [{ id: 'a', title: '가' }, { id: 'b', title: '나' }],
      '2026-08-03T10:00:00Z',
    );
    expect(many.text).toBe('새 소식 2건 — 가 외');
    expect(many.target).toBe('a');
  });
  it('guestbook: N건 문구 + 최신 entry_id target + 방명록 탭 dest', () => {
    const one = guestbookNotice([{ entry_id: 'e1', author_name: '준녕' }], '2026-07-29T10:00:00Z');
    expect(one.kind).toBe('guestbook');
    expect(one.text).toBe('방명록에 새 글 — 준녕님');
    expect(noticeDest(one)).toEqual({ tab: 'guestbook', target: 'e1' });
    const many = guestbookNotice(
      [{ entry_id: 'e2', author_name: '가' }, { entry_id: 'e1', author_name: '나' }],
      '2026-07-29T10:00:00Z',
    );
    expect(many.text).toBe('방명록에 새 글 2건 — 가님 외');
    expect(many.target).toBe('e2');
  });
});

describe('noticeStamp (단일 스트림 스펙 §5 C)', () => {
  // 로컬 시각 기준 — 테스트도 로컬 생성자로 만들어 타임존에 흔들리지 않게 한다.
  const at = (y: number, m: number, d: number, h = 0, min = 0) =>
    new Date(y, m - 1, d, h, min).toISOString();
  const now = new Date(2026, 7, 3, 14, 30); // 2026-08-03 14:30 로컬

  it('오늘 알림은 HH:MM', () => {
    expect(noticeStamp(at(2026, 8, 3, 9, 5), now).label).toBe('09:05');
  });
  it('어제 알림은 MM-DD — 오늘과 구분된다', () => {
    expect(noticeStamp(at(2026, 8, 2, 23, 59), now).label).toBe('08-02');
  });
  it('해가 바뀐 과거도 MM-DD', () => {
    expect(noticeStamp(at(2025, 12, 31, 12, 0), now).label).toBe('12-31');
  });
  it('full은 두 경우 모두 YYYY-MM-DD HH:MM', () => {
    expect(noticeStamp(at(2026, 8, 3, 9, 5), now).full).toBe('2026-08-03 09:05');
    expect(noticeStamp(at(2025, 12, 31, 12, 0), now).full).toBe('2025-12-31 12:00');
  });
  it('자정 경계에서 날짜가 정확히 갈린다', () => {
    // 00:00은 오늘의 시작 — HH:MM. 그 1분 전은 어제라 MM-DD.
    expect(noticeStamp(at(2026, 8, 3, 0, 0), now).label).toBe('00:00');
    expect(noticeStamp(at(2026, 8, 2, 23, 59), now).label).toBe('08-02');
    // now가 자정 직후여도 같은 판정이어야 한다 (now를 인자로 받는 이유)
    const justAfterMidnight = new Date(2026, 7, 3, 0, 0);
    expect(noticeStamp(at(2026, 8, 3, 0, 0), justAfterMidnight).label).toBe('00:00');
    expect(noticeStamp(at(2026, 8, 2, 23, 59), justAfterMidnight).label).toBe('08-02');
  });
});

describe('noticeDest', () => {
  it('finding→coach, diary→diary로 매핑', () => {
    expect(noticeDest({ ts: 't', kind: 'finding', text: '', target: 'k1' })).toEqual({
      tab: 'coach',
      target: 'k1',
    });
    expect(noticeDest({ ts: 't', kind: 'diary', text: '', target: '2026-07-11' })).toEqual({
      tab: 'diary',
      target: '2026-07-11',
    });
  });
  it('target 없는 알림(occasion·구버전 저장분)은 null — 클릭 불가', () => {
    expect(noticeDest({ ts: 't', kind: 'occasion', text: '오늘은 X!' })).toBeNull();
    expect(noticeDest({ ts: 't', kind: 'finding', text: '구버전' })).toBeNull();
  });
});
