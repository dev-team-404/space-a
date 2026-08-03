export interface Notice {
  ts: string;
  kind: 'finding' | 'diary' | 'occasion' | 'visit' | 'guestbook' | 'reuse';
  text: string;
  /** 딥링크 대상 — finding=dedup_key, diary=YYYY-MM-DD, guestbook=entry_id. 없으면 클릭 불가. */
  target?: string;
}

const MAX = 20;
const KEY = 'agent-mentor.notices';

export function pushNotice(list: Notice[], n: Notice): Notice[] {
  return [n, ...list].slice(0, MAX);
}

export function loadNotices(): Notice[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '[]') as Notice[];
  } catch {
    return [];
  }
}

export function saveNotices(list: Notice[]): void {
  localStorage.setItem(KEY, JSON.stringify(list));
}

/** 알림 클릭 착지. target 없으면 null(클릭 불가) — 구버전 저장분·occasion이 자연 비활성. */
export interface NoticeDest {
  tab: 'coach' | 'diary' | 'guestbook';
  target: string;
}

/** finding 알림 — target은 절약량 1위 finding의 dedup_key (말풍선 findingBubble의 top과 동일 기준) */
export function findingNotice(
  rows: { dedup_key: string; est_tokens_saved: number }[],
  ts: string,
): Notice {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  return { ts, kind: 'finding', text: `코칭 지적 ${rows.length}건이 도착했어요`, target: top.dedup_key };
}

export function diaryNotice(date: string, ts: string): Notice {
  return { ts, kind: 'diary', text: `${date} 일기가 나왔어요`, target: date };
}

export function occasionNotice(labels: string[], ts: string): Notice {
  return { ts, kind: 'occasion', text: `오늘은 ${labels[0]}!` };
}

/** P4 방문 알림 — target 없음(클릭 불가, occasion 선례). visits[0] = 가장 최근 방문자. */
export function visitNotice(visits: { visitor_name: string }[], ts: string): Notice {
  const more = visits.length > 1 ? ` 외 ${visits.length - 1}명` : '';
  return { ts, kind: 'visit', text: `${visits[0].visitor_name}님${more}이 방에 다녀갔어요` };
}

/** N1 방명록 알림 — entries는 최신이 앞(백엔드가 서버 순서 유지). target=최신 entry_id. */
export function guestbookNotice(entries: { entry_id: string; author_name: string }[], ts: string): Notice {
  const who = entries[0].author_name;
  const text = entries.length > 1
    ? `방명록에 새 글 ${entries.length}건 — ${who}님 외`
    : `방명록에 새 글 — ${who}님`;
  return { ts, kind: 'guestbook', text, target: entries[0].entry_id };
}

/** 인정 루프 알림 — 내가 올린 지식을 남이 재사용했다. target 없음(클릭 불가, visit 선례).
 *  프라이버시: 팀까지만 말하고 개인명·상대 이슈 제목은 넣지 않는다 (백엔드가 이미 그렇게 준다). */
export function reuseNotice(notes: { space: string; cross_team: boolean }[], ts: string): Notice {
  const where = notes[0].cross_team ? `${notes[0].space} 팀` : '같은 팀';
  const text = notes.length > 1
    ? `내 지식이 ${notes.length}곳에서 재사용됐어요 — ${where} 포함`
    : `내 지식을 ${where}에서 가져다 썼어요`;
  return { ts, kind: 'reuse', text };
}

export function noticeDest(n: Notice): NoticeDest | null {
  if (!n.target) return null;
  if (n.kind === 'finding') return { tab: 'coach', target: n.target };
  if (n.kind === 'diary') return { tab: 'diary', target: n.target };
  if (n.kind === 'guestbook') return { tab: 'guestbook', target: n.target };
  return null;
}
