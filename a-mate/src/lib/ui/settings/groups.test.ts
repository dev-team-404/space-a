import { describe, it, expect } from 'vitest';
import { SETTINGS_GROUPS, DEFAULT_GROUP, normalizeGroup } from './groups';

describe('SETTINGS_GROUPS', () => {
  it('연결·나·공개·모양 4그룹이 이 순서로', () => {
    expect(SETTINGS_GROUPS.map((g) => g.id)).toEqual(['conn', 'me', 'privacy', 'look']);
    expect(SETTINGS_GROUPS.map((g) => g.label)).toEqual(['연결', '나', '공개', '모양']);
  });
  it('기본 그룹은 연결 — 처음 쓸 때 가장 먼저 필요한 설정', () => {
    expect(DEFAULT_GROUP).toBe('conn');
  });
});

describe('normalizeGroup', () => {
  it('유효한 그룹 id는 그대로', () => {
    expect(normalizeGroup('conn')).toBe('conn');
    expect(normalizeGroup('me')).toBe('me');
    expect(normalizeGroup('privacy')).toBe('privacy');
    expect(normalizeGroup('look')).toBe('look');
  });
  it('모르는 값·빈값·undefined·null은 기본 그룹으로', () => {
    expect(normalizeGroup('bogus')).toBe('conn');
    expect(normalizeGroup('')).toBe('conn');
    expect(normalizeGroup(undefined)).toBe('conn');
    expect(normalizeGroup(null)).toBe('conn');
  });
});
