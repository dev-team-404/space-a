import { describe, expect, it } from 'vitest';
import {
  addDiaryDate, clearDiaryDates, clearGuestbookSeen, maxCreatedAt, newGuestbookIds, parseUnseen,
} from './unseen';

const NOW = '2026-07-29T10:00:00+00:00';

describe('parseUnseen', () => {
  it('정상 저장분 복원', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
    expect(parseUnseen(raw)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
  });
  it('없음·손상은 미시드(null) — 클라이언트 시계로 워터마크를 만들지 않는다', () => {
    expect(parseUnseen(null)).toEqual({ diaryDates: [], guestbookLastSeen: null });
    expect(parseUnseen('{broken').guestbookLastSeen).toBeNull();
    expect(parseUnseen('{"diaryDates":"x"}').diaryDates).toEqual([]);
  });
  it('저장된 null을 복원하며 diaryDates를 보존한다', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: null });
    expect(parseUnseen(raw)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: null });
  });
});

describe('diary unseen', () => {
  it('날짜 set 누적 — 같은 날짜 재생성은 1회', () => {
    let s = parseUnseen(null);
    s = addDiaryDate(s, '2026-07-28');
    s = addDiaryDate(s, '2026-07-28');
    s = addDiaryDate(s, '2026-07-29');
    expect(s.diaryDates).toEqual(['2026-07-28', '2026-07-29']);
    expect(clearDiaryDates(s).diaryDates).toEqual([]);
  });
});

describe('guestbook unseen', () => {
  const e = (id: string, author: string, at: string) => ({ entry_id: id, author_agent_id: author, created_at: at });
  it('lastSeen 이후 타인 글만 — 내 글·과거 글 제외', () => {
    const ids = newGuestbookIds(
      [e('new', 'other', '2026-07-29T12:00:00+00:00'), e('mine', 'me', '2026-07-29T12:00:00+00:00'), e('old', 'other', '2026-07-01T00:00:00+00:00')],
      'me', NOW,
    );
    expect(ids).toEqual(['new']);
  });
  it('미시드(null)면 기존 글을 신규로 세지 않는다 — 이미 있는 건 다 본 것', () => {
    const ids = newGuestbookIds(
      [e('a', 'other', '2026-07-01T00:00:00+00:00'), e('b', 'other', '2026-07-29T12:00:00+00:00')],
      'me', null,
    );
    expect(ids).toEqual([]);
  });
  it('클리어 = lastSeen 갱신', () => {
    const s = clearGuestbookSeen(parseUnseen(null), '2026-07-30T00:00:00+00:00');
    expect(s.guestbookLastSeen).toBe('2026-07-30T00:00:00+00:00');
  });
  it('maxCreatedAt: 타인 글의 최신 서버 시각 — 내 글 제외, 없으면 null', () => {
    expect(maxCreatedAt(
      [e('a', 'other', '2026-07-01T00:00:00+00:00'), e('b', 'other', '2026-07-29T00:00:00+00:00'),
       e('c', 'me', '2026-07-30T00:00:00+00:00')],
      'me',
    )).toBe('2026-07-29T00:00:00+00:00');
    expect(maxCreatedAt([e('c', 'me', '2026-07-30T00:00:00+00:00')], 'me')).toBeNull();
    expect(maxCreatedAt([], 'me')).toBeNull();
  });
});
