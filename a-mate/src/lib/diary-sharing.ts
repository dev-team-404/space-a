import { getDiary, lifeSetDiaryVisibility, listDiaryDates } from './api';

export type DiaryVisibility = 'private' | 'friends' | 'public';

type DiarySharingApi = {
  getDiary: (date: string) => Promise<string | null>;
  listDiaryDates: () => Promise<string[]>;
  shareDiary: (date: string, body: string, visibility: 'friends' | 'public') => Promise<unknown>;
};

const defaultApi: DiarySharingApi = {
  getDiary,
  listDiaryDates,
  shareDiary: (date, body, visibility) => lifeSetDiaryVisibility(date, body, visibility),
};

export function diaryVisibility(value: string | null): DiaryVisibility {
  return value === 'friends' || value === 'public' ? value : 'private';
}

export async function syncSharedDiary(
  date: string,
  visibility: DiaryVisibility,
  api: DiarySharingApi = defaultApi,
): Promise<void> {
  if (visibility === 'private') return;
  const body = await api.getDiary(date);
  if (body) await api.shareDiary(date, body, visibility);
}

export async function syncAllSharedDiaries(
  visibility: DiaryVisibility,
  api: DiarySharingApi = defaultApi,
): Promise<void> {
  if (visibility === 'private') return;
  const dates = await api.listDiaryDates();
  for (const date of dates) await syncSharedDiary(date, visibility, api);
}
