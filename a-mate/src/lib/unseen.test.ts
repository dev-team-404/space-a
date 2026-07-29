import { describe, expect, it } from 'vitest';
import {
  addDiaryDate, clearDiaryDates, clearGuestbookSeen, newGuestbookIds, parseUnseen,
} from './unseen';

const NOW = '2026-07-29T10:00:00+00:00';

describe('parseUnseen', () => {
  it('정상 저장분 복원', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
    expect(parseUnseen(raw, NOW)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
  });
  it('없음·손상은 now 기준 초기 상태 — 과거 전체가 뱃지로 쏟아지지 않게', () => {
    expect(parseUnseen(null, NOW)).toEqual({ diaryDates: [], guestbookLastSeen: NOW });
    expect(parseUnseen('{broken', NOW).guestbookLastSeen).toBe(NOW);
    expect(parseUnseen('{"diaryDates":"x"}', NOW).diaryDates).toEqual([]);
  });
});

describe('diary unseen', () => {
  it('날짜 set 누적 — 같은 날짜 재생성은 1회', () => {
    let s = parseUnseen(null, NOW);
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
  it('클리어 = lastSeen 갱신', () => {
    const s = clearGuestbookSeen(parseUnseen(null, NOW), '2026-07-30T00:00:00+00:00');
    expect(s.guestbookLastSeen).toBe('2026-07-30T00:00:00+00:00');
  });
});
