import { describe, it, expect } from 'vitest';
import { resolveTheme, normalizeTheme, DEFAULT_THEME } from './theme';

describe('resolveTheme', () => {
  it('light/dark는 시스템과 무관하게 확정', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
  it('system은 OS 선호를 따름', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });
});

describe('normalizeTheme', () => {
  it('유효 값은 유지', () => {
    expect(normalizeTheme({ mode: 'dark', skin: 'peach' })).toEqual({ mode: 'dark', skin: 'peach' });
  });
  it('null/깨진 값은 기본값 폴백', () => {
    expect(normalizeTheme(null)).toEqual(DEFAULT_THEME);
    expect(normalizeTheme({ mode: 'x' as any, skin: 'y' as any })).toEqual(DEFAULT_THEME);
  });
});
