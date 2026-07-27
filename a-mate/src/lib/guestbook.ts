import type { GuestbookEntry } from './api';

export interface GuestbookThread { entry: GuestbookEntry; replies: GuestbookEntry[] }

/** 평면 방명록 목록(서버: 최신순) → 원글 스레드 목록.
 *  원글은 서버 순서(최신순) 유지, 답글은 오래된 순(대화 흐름).
 *  부모가 목록에 없는 답글은 방어적으로 top-level 취급(정상 흐름엔 없음 — 서버가 cascade 삭제). */
export function groupGuestbook(entries: GuestbookEntry[]): GuestbookThread[] {
  const ids = new Set(entries.map((e) => e.entry_id));
  const byParent = new Map<string, GuestbookEntry[]>();
  for (const e of entries) {
    if (e.parent_id && ids.has(e.parent_id)) {
      byParent.set(e.parent_id, [...(byParent.get(e.parent_id) ?? []), e]);
    }
  }
  return entries
    .filter((e) => !e.parent_id || !ids.has(e.parent_id))
    .map((entry) => ({ entry, replies: [...(byParent.get(entry.entry_id) ?? [])].reverse() }));
}

/** G5 — 이 항목에 주인 마스코트 아바타를 보일지: 주인이 자기 홈에서 보는 자기 봇/자기 작성 항목만. */
export function showOwnerAvatar(entry: GuestbookEntry, meId: string, isOwner: boolean): boolean {
  return isOwner && entry.author_agent_id === meId;
}
