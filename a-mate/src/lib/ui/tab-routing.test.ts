import { describe, it, expect } from 'vitest';
import { isTab, resolveTabAfterLifeChange } from './tab-routing';

describe('isTab', () => {
  it('아는 탭은 통과', () => {
    for (const t of ['home', 'diary', 'coach', 'chat', 'guestbook', 'settings']) {
      expect(isTab(t)).toBe(true);
    }
  });
  it('모르는 탭은 거부 — 백엔드가 보낸 낯선 값을 무시해야 한다', () => {
    expect(isTab('bogus')).toBe(false);
    expect(isTab('')).toBe(false);
  });
});

describe('resolveTabAfterLifeChange', () => {
  it('방이 안 바뀌면 현재 탭 유지 (pendingTab도 유지)', () => {
    expect(resolveTabAfterLifeChange(false, null, 'coach')).toEqual({ tab: 'coach', pendingTab: null });
    expect(resolveTabAfterLifeChange(false, 'settings', 'home')).toEqual({ tab: 'home', pendingTab: 'settings' });
  });
  it('방이 바뀌면 홈으로 리셋 — 방문 컨텍스트 전환 기존 동작', () => {
    expect(resolveTabAfterLifeChange(true, null, 'coach')).toEqual({ tab: 'home', pendingTab: null });
  });
  it('pendingTab이 있으면 홈 리셋을 이긴다 — 트레이 설정이 방문 중에 눌린 경우', () => {
    expect(resolveTabAfterLifeChange(true, 'settings', 'home')).toEqual({ tab: 'settings', pendingTab: null });
  });
});
