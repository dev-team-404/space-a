/** 설정 탭 서브 그룹. id는 chat:goto-tab의 target으로도 쓰인다(트레이·마스코트 딥링크). */
export type SettingsGroup = 'conn' | 'me' | 'privacy' | 'look';

export const SETTINGS_GROUPS: { id: SettingsGroup; label: string }[] = [
  { id: 'conn', label: '연결' },
  { id: 'me', label: '나' },
  { id: 'privacy', label: '공개' },
  { id: 'look', label: '모양' },
];

/** 처음 쓸 때 가장 먼저 필요한 설정이 연결(Life Server·엔진)이다. */
export const DEFAULT_GROUP: SettingsGroup = 'conn';

/** 딥링크 target 정규화 — 백엔드는 target 내용을 해석하지 않으므로 여기서 방어한다. */
export function normalizeGroup(raw: string | undefined | null): SettingsGroup {
  return SETTINGS_GROUPS.some((g) => g.id === raw) ? (raw as SettingsGroup) : DEFAULT_GROUP;
}
