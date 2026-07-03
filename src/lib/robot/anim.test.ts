import { describe, expect, it } from 'vitest';
import { frameAt, resolveState } from './anim';

describe('resolveState', () => {
  it('말풍선이 최우선이다', () => {
    expect(resolveState({ bubbleKind: 'finding', hour: 3 })).toBe('alert');
    expect(resolveState({ bubbleKind: 'diary', hour: 3 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'occasion', hour: 12 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'chatter', hour: 12 })).toBe('talk');
  });
  it('01~07시 무풍선이면 sleep, 그 외 idle', () => {
    expect(resolveState({ bubbleKind: null, hour: 1 })).toBe('sleep');
    expect(resolveState({ bubbleKind: null, hour: 6 })).toBe('sleep');
    expect(resolveState({ bubbleKind: null, hour: 7 })).toBe('idle');
    expect(resolveState({ bubbleKind: null, hour: 12 })).toBe('idle');
  });
});

describe('frameAt', () => {
  it('결정적이다', () => {
    expect(frameAt('idle', 1234)).toEqual(frameAt('idle', 1234));
  });
  it('idle은 바운스가 -1..1 안에서 주기적으로 변한다', () => {
    const ys = [0, 400, 800, 1200].map((t) => frameAt('idle', t).offsetY);
    for (const y of ys) { expect(y).toBeGreaterThanOrEqual(-1); expect(y).toBeLessThanOrEqual(1); }
    expect(new Set(ys).size).toBeGreaterThan(1);
  });
  it('sleep은 눈 감김, alert는 안테나 점멸 토글', () => {
    expect(frameAt('sleep', 0).eyesOverride).toBeDefined();
    const a = frameAt('alert', 0).antennaBlink;
    const b = frameAt('alert', 300).antennaBlink;
    expect(a).not.toBe(b);
  });
});
