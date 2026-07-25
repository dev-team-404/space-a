/** 미니홈피 창 탭. App.svelte와 트레이 딥링크(chat:goto-tab)가 공유한다. */
export type Tab = 'home' | 'diary' | 'coach' | 'chat' | 'guestbook' | 'settings';

const TAB_IDS: readonly string[] = ['home', 'diary', 'coach', 'chat', 'guestbook', 'settings'];

/** chat:goto-tab의 tab 문자열이 아는 탭인지. 모르면 무시한다. */
export function isTab(raw: string): raw is Tab {
  return TAB_IDS.includes(raw);
}

/**
 * 방이 바뀌면 홈으로 리셋한다(방문 컨텍스트 전환 — 기존 동작).
 * 단 pendingTab이 있으면 그 의도가 이긴다: 트레이 "설정"이 남의 방 방문 중에 눌려
 * 내 방으로 돌아오는 중인 경우, 리셋이 설정 탭 의도를 덮어써선 안 된다.
 */
export function resolveTabAfterLifeChange(
  lifeChanged: boolean,
  pendingTab: Tab | null,
  currentTab: Tab,
): { tab: Tab; pendingTab: Tab | null } {
  if (!lifeChanged) return { tab: currentTab, pendingTab };
  if (pendingTab) return { tab: pendingTab, pendingTab: null };
  return { tab: 'home', pendingTab: null };
}
