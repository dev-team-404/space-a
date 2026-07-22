import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type ThemeMode = 'light' | 'dark' | 'system';
export type ThemeSkin = 'sky' | 'mint' | 'peach' | 'lavender';
export interface ThemeSettings { mode: ThemeMode; skin: ThemeSkin }

const MODES: ThemeMode[] = ['light', 'dark', 'system'];
const SKINS: ThemeSkin[] = ['sky', 'mint', 'peach', 'lavender'];
export const DEFAULT_THEME: ThemeSettings = { mode: 'system', skin: 'sky' };

/** system이면 OS 선호로, 아니면 그대로. 순수 함수. */
export function resolveTheme(mode: ThemeMode, systemPrefersDark: boolean): 'light' | 'dark' {
  if (mode === 'dark') return 'dark';
  if (mode === 'light') return 'light';
  return systemPrefersDark ? 'dark' : 'light';
}

/** 저장값이 깨졌을 때 안전한 기본값으로 정규화. 순수 함수. */
export function normalizeTheme(raw: Partial<ThemeSettings> | null | undefined): ThemeSettings {
  const mode = raw && MODES.includes(raw.mode as ThemeMode) ? (raw.mode as ThemeMode) : DEFAULT_THEME.mode;
  const skin = raw && SKINS.includes(raw.skin as ThemeSkin) ? (raw.skin as ThemeSkin) : DEFAULT_THEME.skin;
  return { mode, skin };
}

let current: ThemeSettings = { ...DEFAULT_THEME };
const darkMql = () => window.matchMedia('(prefers-color-scheme: dark)');

function paint(): void {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(current.mode, darkMql().matches);
  root.dataset.skin = current.skin;
}

/** 각 창 엔트리(chat/mascot/settings)에서 1회 호출. */
export async function initTheme(): Promise<void> {
  paint(); // 기본값으로 즉시 적용 (FOUC 방지)
  try {
    current = normalizeTheme(await invoke<ThemeSettings>('theme_get'));
    paint();
  } catch (e) {
    console.warn('theme_get 실패, 기본값 유지:', e);
  }
  darkMql().addEventListener('change', () => { if (current.mode === 'system') paint(); });
  await listen<ThemeSettings>('theme:changed', (ev) => { current = normalizeTheme(ev.payload); paint(); });
}

// 연속 변경(모드→스킨 빠른 클릭) 시 async 저장이 순서 뒤바뀌어 오래된 스냅샷이
// store/브로드캐스트를 덮어쓰지 않도록 저장을 직렬화한다 — 마지막 선택이 이긴다.
let saveChain: Promise<void> = Promise.resolve();

/** 설정 UI에서 호출. 낙관적 적용(동기) + 직렬화된 저장(서버가 theme:changed 브로드캐스트). */
export function setTheme(next: ThemeSettings): Promise<void> {
  current = normalizeTheme(next);
  paint();
  const snap = { mode: current.mode, skin: current.skin };
  saveChain = saveChain.then(() =>
    invoke('theme_set', snap).then(
      () => {},
      (e) => { console.warn('theme_set 실패:', e); },
    ),
  );
  return saveChain;
}

export function getTheme(): ThemeSettings { return { ...current }; }
