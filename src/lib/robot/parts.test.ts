import { describe, expect, it } from 'vitest';
import { EYES_BLINK, EYES_HAPPY, EYES_SLEEP, PALETTES, PARTS, VARIANTS } from './parts';
import { buildRobotPixels, type RobotSpec } from './render';

const SLOTS = ['antenna', 'head', 'eyes', 'body', 'arms'] as const;

describe('parts 불변식', () => {
  it('변형 개수가 Rust 상수와 일치한다', () => {
    for (const s of SLOTS) expect(PARTS[s]).toHaveLength(VARIANTS[s]);
    expect(PALETTES).toHaveLength(VARIANTS.palette);
  });

  it('모든 픽셀이 16×16 경계·색인덱스 0..3 안이다', () => {
    const all = [...SLOTS.flatMap((s) => PARTS[s].flat()), ...EYES_BLINK, ...EYES_HAPPY, ...EYES_SLEEP];
    for (const [x, y, c] of all) {
      expect(x).toBeGreaterThanOrEqual(0); expect(x).toBeLessThan(16);
      expect(y).toBeGreaterThanOrEqual(0); expect(y).toBeLessThan(16);
      expect(c).toBeGreaterThanOrEqual(0); expect(c).toBeLessThan(4);
    }
  });

  it('변형은 비어있지 않고 같은 슬롯 안에서 서로 다르다', () => {
    for (const s of SLOTS) {
      const seen = new Set<string>();
      for (const v of PARTS[s]) {
        expect(v.length).toBeGreaterThan(0);
        const key = JSON.stringify([...v].sort());
        expect(seen.has(key)).toBe(false);
        seen.add(key);
      }
    }
  });

  it('팔레트는 4색 hex다', () => {
    for (const p of PALETTES) {
      expect(p).toHaveLength(4);
      for (const c of p) expect(c).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe('buildRobotPixels', () => {
  const spec: RobotSpec = { antenna: 1, head: 2, eyes: 3, body: 4, arms: 5, palette: 6 };

  it('결정적이다', () => {
    expect(buildRobotPixels(spec)).toEqual(buildRobotPixels(spec));
  });

  it('eyesOverride가 기본 눈을 대체한다', () => {
    const a = buildRobotPixels(spec);
    const b = buildRobotPixels(spec, EYES_SLEEP);
    expect(a).not.toEqual(b);
  });

  it('스펙이 다르면 결과가 다르다 (샘플 페어)', () => {
    const other: RobotSpec = { antenna: 0, head: 0, eyes: 0, body: 0, arms: 0, palette: 0 };
    expect(buildRobotPixels(spec)).not.toEqual(buildRobotPixels(other));
  });
});
