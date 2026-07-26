import { describe, expect, it } from 'vitest';
import { groupGuestbook } from './guestbook';
import type { GuestbookEntry } from './api';

const e = (entry_id: string, parent_id: string | null = null): GuestbookEntry => ({
  entry_id, life_id: 'l1', author_agent_id: 'a1', author_name: '이름',
  body: '내용', parent_id, created_at: '2026-07-26T00:00:00Z',
});

describe('groupGuestbook', () => {
  it('원글은 서버 순서(최신순) 유지, 답글은 오래된 순으로 부모 아래 묶인다', () => {
    // 서버 응답은 최신순 — gb_r2가 gb_r1보다 최신
    const entries = [e('gb_r2', 'gb_p1'), e('gb_p2'), e('gb_r1', 'gb_p1'), e('gb_p1')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_p2', 'gb_p1']);
    expect(threads[1].replies.map((r) => r.entry_id)).toEqual(['gb_r1', 'gb_r2']);
  });

  it('parent_id 없는 레거시 행은 전부 top-level', () => {
    const entries = [e('gb_b'), e('gb_a')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_b', 'gb_a']);
    expect(threads.every((t) => t.replies.length === 0)).toBe(true);
  });

  it('부모가 목록에 없는 답글은 top-level로 폴백', () => {
    const threads = groupGuestbook([e('gb_orphan', 'gb_gone')]);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_orphan']);
    expect(threads[0].replies).toEqual([]);
  });
});
