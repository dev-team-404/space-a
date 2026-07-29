import { describe, expect, it } from 'vitest';
import { diaryNotice, findingNotice, guestbookNotice, noticeDest, occasionNotice, pushNotice, visitNotice, type Notice } from './notices';

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
