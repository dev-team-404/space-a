/** N1 탭 뱃지의 읽음 상태 (스펙 §5) — localStorage 영속은 lastSeen(방명록)·날짜 set(다이어리)만.
 *  방명록 unseen 카운트는 세션 내 entry_id set으로 재계산(부트스트랩+이벤트 dedup — 중복 카운트 구조적 차단). */
export interface UnseenState {
  diaryDates: string[]; // 아직 안 본 신규 일기 날짜들 — 일기는 앱 실행 중에만 생성되므로 이벤트 누적으로 완결
  guestbookLastSeen: string | null; // 마지막 확인 시각(서버 created_at 서식). null = 아직 시드 안 됨
}

const KEY = 'agent-mentor.tab-unseen';

/** 저장 원문 파싱 — 없음·손상은 **미시드(null)**. 클라이언트 시계로 워터마크를 만들면
 *  서버 created_at과의 사전순 비교가 시계 오차만큼 어긋난다(Q1). 시드는 첫 서버 조회가 한다.
 *  저장된 null은 **명시적으로 허용**해야 한다 — 무효 분기로 뭉개면 diaryDates까지 함께 버려진다. */
export function parseUnseen(raw: string | null): UnseenState {
  try {
    const v = JSON.parse(raw ?? 'null');
    if (
      v && Array.isArray(v.diaryDates) && v.diaryDates.every((d: unknown) => typeof d === 'string')
      && (v.guestbookLastSeen === null || typeof v.guestbookLastSeen === 'string')
    ) {
      return { diaryDates: v.diaryDates, guestbookLastSeen: v.guestbookLastSeen };
    }
  } catch {
    /* 손상 → 초기화 */
  }
  return { diaryDates: [], guestbookLastSeen: null };
}

export function loadUnseen(): UnseenState {
  return parseUnseen(localStorage.getItem(KEY));
}

export function saveUnseen(s: UnseenState): void {
  localStorage.setItem(KEY, JSON.stringify(s));
}

export function addDiaryDate(s: UnseenState, date: string): UnseenState {
  return s.diaryDates.includes(date) ? s : { ...s, diaryDates: [...s.diaryDates, date] };
}

export function clearDiaryDates(s: UnseenState): UnseenState {
  return { ...s, diaryDates: [] };
}

/** 읽음 워터마크 갱신. `atIso`는 **서버 발급 created_at**이어야 한다 — 클라이언트 시계를 쓰면
 *  오차만큼 새 글이 숨거나(시계 빠름) 본 글이 되살아난다(느림). 후보는 maxCreatedAt로 뽑는다. */
export function clearGuestbookSeen(s: UnseenState, atIso: string): UnseenState {
  return { ...s, guestbookLastSeen: atIso };
}

/** 부트스트랩(첫 서버 조회) 전용 시드 — **성공한 조회는 반드시 시드한다.**
 *  글이 있으면 최신 서버 시각("이미 있는 건 다 본 것"), 없으면 `''`(최소 워터마크).
 *  빈 방명록에서 미시드로 남기면 이후 `newGuestbookIds`가 계속 0건을 반환해
 *  **첫 새 글이 조용히 읽음 처리된다** — `''`면 모든 ISO 시각이 그보다 커서 정상적으로 신규가 된다.
 *  이벤트 경로에서 부르면 안 된다: 그 글 자신의 시각으로 시드해 자기를 읽음 처리한다. */
export function seedGuestbookSeen(
  s: UnseenState,
  entries: { author_agent_id: string; created_at: string }[],
  myAgentId: string,
): UnseenState {
  if (s.guestbookLastSeen !== null) return s;
  return { ...s, guestbookLastSeen: maxCreatedAt(entries, myAgentId) ?? '' };
}

/** 관측한 타인 글의 최신 created_at (없으면 null) — 워터마크를 서버 시각으로 올리기 위한 후보. */
export function maxCreatedAt(
  entries: { author_agent_id: string; created_at: string }[],
  myAgentId: string,
): string | null {
  const times = entries
    .filter((e) => e.author_agent_id !== myAgentId)
    .map((e) => e.created_at);
  return times.length ? times.reduce((a, b) => (b > a ? b : a)) : null;
}

/** lastSeen 이후의 타인 글 entry_id — 부트스트랩(서버 조회)과 이벤트 payload 양쪽에 같은 판정.
 *  `lastSeenIso`가 null(미시드)이면 기존 글은 전부 "이미 본 것"으로 보고 아무것도 세지 않는다. */
export function newGuestbookIds(
  entries: { entry_id: string; author_agent_id: string; created_at: string }[],
  myAgentId: string,
  lastSeenIso: string | null,
): string[] {
  if (lastSeenIso === null) return [];
  return entries
    .filter((e) => e.author_agent_id !== myAgentId && e.created_at > lastSeenIso)
    .map((e) => e.entry_id);
}
