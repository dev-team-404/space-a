/** N1 탭 뱃지의 읽음 상태 (스펙 §5) — localStorage 영속은 lastSeen(방명록)·날짜 set(다이어리)만.
 *  방명록 unseen 카운트는 세션 내 entry_id set으로 재계산(부트스트랩+이벤트 dedup — 중복 카운트 구조적 차단). */
export interface UnseenState {
  diaryDates: string[]; // 아직 안 본 신규 일기 날짜들 — 일기는 앱 실행 중에만 생성되므로 이벤트 누적으로 완결
  guestbookLastSeen: string; // 방명록 탭을 내 방 문맥으로 마지막 확인한 시각 (서버 created_at와 같은 서식 비교)
}

const KEY = 'agent-mentor.tab-unseen';

/** 저장 원문 파싱 — 없음·손상은 now 기준 초기 상태 (과거 전체가 뱃지로 쏟아지는 것 방지). */
export function parseUnseen(raw: string | null, nowIso: string): UnseenState {
  try {
    const v = JSON.parse(raw ?? 'null');
    if (
      v && Array.isArray(v.diaryDates) && v.diaryDates.every((d: unknown) => typeof d === 'string')
      && typeof v.guestbookLastSeen === 'string'
    ) {
      return { diaryDates: v.diaryDates, guestbookLastSeen: v.guestbookLastSeen };
    }
  } catch {
    /* 손상 → 초기화 */
  }
  return { diaryDates: [], guestbookLastSeen: nowIso };
}

export function loadUnseen(nowIso: string): UnseenState {
  return parseUnseen(localStorage.getItem(KEY), nowIso);
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

export function clearGuestbookSeen(s: UnseenState, nowIso: string): UnseenState {
  return { ...s, guestbookLastSeen: nowIso };
}

/** lastSeen 이후의 타인 글 entry_id — 부트스트랩(서버 조회)과 이벤트 payload 양쪽에 같은 판정. */
export function newGuestbookIds(
  entries: { entry_id: string; author_agent_id: string; created_at: string }[],
  myAgentId: string,
  lastSeenIso: string,
): string[] {
  return entries
    .filter((e) => e.author_agent_id !== myAgentId && e.created_at > lastSeenIso)
    .map((e) => e.entry_id);
}
