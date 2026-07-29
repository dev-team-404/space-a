import { describe, expect, it, vi } from 'vitest';
import { diaryVisibility, syncAllSharedDiaries, syncSharedDiary } from './diary-sharing';

function api(bodies: Record<string, string | null>) {
  return {
    getDiary: vi.fn(async (date: string) => bodies[date] ?? null),
    listDiaryDates: vi.fn(async () => Object.keys(bodies)),
    shareDiary: vi.fn(async () => undefined),
  };
}

describe('diary sharing', () => {
  it('treats missing and unknown visibility as private', () => {
    expect(diaryVisibility(null)).toBe('private');
    expect(diaryVisibility('unknown')).toBe('private');
    expect(diaryVisibility('friends')).toBe('friends');
    expect(diaryVisibility('public')).toBe('public');
  });

  it('does not read or upload a private diary', async () => {
    const client = api({ '2026-07-29': 'body' });
    await syncSharedDiary('2026-07-29', 'private', client);
    expect(client.getDiary).not.toHaveBeenCalled();
    expect(client.shareDiary).not.toHaveBeenCalled();
  });

  it('uploads a newly generated shared diary', async () => {
    const client = api({ '2026-07-29': 'body' });
    await syncSharedDiary('2026-07-29', 'friends', client);
    expect(client.shareDiary).toHaveBeenCalledWith('2026-07-29', 'body', 'friends');
  });

  it('catches up every existing diary after connecting', async () => {
    const client = api({
      '2026-07-27': 'first',
      '2026-07-28': null,
      '2026-07-29': 'latest',
    });
    await syncAllSharedDiaries('public', client);
    expect(client.shareDiary.mock.calls).toEqual([
      ['2026-07-27', 'first', 'public'],
      ['2026-07-29', 'latest', 'public'],
    ]);
  });
});
