import { describe, expect, it } from 'vitest';
import { isDrag } from './drag';

describe('isDrag — 클릭 vs 드래그 판별 (스펙 §6, 4px 임계값)', () => {
  it('임계값 이내는 클릭', () => {
    expect(isDrag(100, 100, 100, 100)).toBe(false);
    expect(isDrag(100, 100, 103, 102)).toBe(false); // √13 ≈ 3.6
  });
  it('임계값 초과는 드래그', () => {
    expect(isDrag(100, 100, 105, 100)).toBe(true); // 5 > 4
    expect(isDrag(100, 100, 97, 97)).toBe(true); // √18 ≈ 4.2
  });
  it('커스텀 임계값', () => {
    expect(isDrag(0, 0, 5, 0, 6)).toBe(false);
  });
});
