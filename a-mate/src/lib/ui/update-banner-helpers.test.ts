import { describe, expect, it } from 'vitest';
import {
  shouldShow,
  bannerText,
  progressPercent,
  type UpdatePhase,
} from './update-banner-helpers';

describe('shouldShow', () => {
  it('idle는 항상 숨김', () => {
    expect(shouldShow({ kind: 'idle' })).toBe(false);
  });
  it('available/downloading는 항상 표시', () => {
    expect(shouldShow({ kind: 'available', version: '0.2.0', notes: null })).toBe(true);
    expect(shouldShow({ kind: 'downloading', downloaded: 1, total: 2 })).toBe(true);
  });
  it('checking/uptodate/error는 수동일 때만 표시', () => {
    expect(shouldShow({ kind: 'checking', manual: false })).toBe(false);
    expect(shouldShow({ kind: 'checking', manual: true })).toBe(true);
    expect(shouldShow({ kind: 'uptodate', manual: false })).toBe(false);
    expect(shouldShow({ kind: 'uptodate', manual: true })).toBe(true);
    expect(shouldShow({ kind: 'error', manual: false, message: 'x' })).toBe(false);
    expect(shouldShow({ kind: 'error', manual: true, message: 'x' })).toBe(true);
  });
});

describe('bannerText', () => {
  it('available은 버전 포함', () => {
    expect(bannerText({ kind: 'available', version: '0.2.0', notes: null })).toBe('새 버전 0.2.0 있음');
  });
  it('downloading은 진행률(%) 표시, total 없으면 % 생략', () => {
    expect(bannerText({ kind: 'downloading', downloaded: 50, total: 200 })).toBe('업데이트 다운로드 중… 25%');
    expect(bannerText({ kind: 'downloading', downloaded: 50, total: null })).toBe('업데이트 다운로드 중…');
  });
  it('uptodate/error 문구', () => {
    expect(bannerText({ kind: 'uptodate', manual: true })).toBe('최신 버전입니다');
    expect(bannerText({ kind: 'error', manual: true, message: '네트워크' })).toBe('업데이트 확인 실패: 네트워크');
  });
});

describe('progressPercent', () => {
  it('total이 null/0이면 null', () => {
    expect(progressPercent(10, null)).toBe(null);
    expect(progressPercent(10, 0)).toBe(null);
  });
  it('정상 비율 계산 + 0~100 clamp', () => {
    expect(progressPercent(50, 200)).toBe(25);
    expect(progressPercent(300, 200)).toBe(100);
    expect(progressPercent(-5, 200)).toBe(0);
  });
});
