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

/** G7 — 방명록 작성자 아이콘 (스펙 §3.1).
 *  봇 = 서버에 게시된 마스코트 얼굴, 사람 = 이름에서 뽑은 이니셜 모노그램. */
export type AuthorIcon =
  | { kind: 'bot'; agentId: string }
  | { kind: 'monogram'; initial: string; hue: number };

/** 이름 첫 글자. 코드포인트 단위라 이모지(서로게이트 페어)가 반으로 쪼개지지 않는다.
 *  빈 이름·공백뿐이면 '?'. */
export function authorInitial(name: string): string {
  const trimmed = name.trim();
  return trimmed ? [...trimmed][0] : '?';
}

/** 이름 → 색상 hue(0~359). 결정론적이라 모든 관찰자가 같은 색을 본다. */
export function authorHue(name: string): number {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + (ch.codePointAt(0) ?? 0)) % 360;
  return hash;
}

/** 이 항목의 아이콘. **관찰자를 나타내는 인자가 없다** — 같은 엔트리는 내 방이든 남의 방이든,
 *  내가 쓴 글이든 남이 쓴 글이든 같은 아이콘으로 보인다(스펙 목표 2).
 *  봇 판별은 서버가 채우는 author_kind 하나로 한다: 미제공(구데이터)은 사람 간주. */
export function authorIcon(entry: GuestbookEntry): AuthorIcon {
  if (entry.author_kind === 'bot') {
    return { kind: 'bot', agentId: entry.author_agent_id };
  }
  return {
    kind: 'monogram',
    initial: authorInitial(entry.author_name),
    hue: authorHue(entry.author_name),
  };
}

/** P3 — 방문 시 봇 방명록 토글. 기본 on: 'false'로 저장된 경우에만 off (Rust 글루와 동일 규칙). */
export function visitGuestbookEnabled(settings: Record<string, string>): boolean {
  return settings['visit_guestbook_enabled'] !== 'false';
}

/** 묶음 ② — 자율 방문(쉬는 날 스스로 놀러가기) 토글. 기본 on: 'false'일 때만 off. */
export function autoVisitEnabled(settings: Record<string, string>): boolean {
  return settings['auto_visit_enabled'] !== 'false';
}
