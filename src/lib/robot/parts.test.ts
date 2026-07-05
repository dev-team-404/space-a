import { describe, expect, it } from 'vitest';
import { EYES_BLINK, EYES_HAPPY, EYES_SLEEP, GRID, PALETTES, PARTS, VARIANTS } from './parts';
import { buildRobotPixels, type RobotSpec } from './render';

const SLOTS = ['antenna', 'head', 'eyes', 'body', 'arms'] as const;

describe('parts 불변식', () => {
  it('변형 개수가 Rust 상수와 일치한다', () => {
    for (const s of SLOTS) expect(PARTS[s]).toHaveLength(VARIANTS[s]);
    expect(PALETTES).toHaveLength(VARIANTS.palette);
  });

  it('모든 픽셀이 32×32 경계·색인덱스 0..7 안이다', () => {
    const all = [...SLOTS.flatMap((s) => PARTS[s].flat()), ...EYES_BLINK, ...EYES_HAPPY, ...EYES_SLEEP];
    for (const [x, y, c] of all) {
      expect(x).toBeGreaterThanOrEqual(0); expect(x).toBeLessThan(GRID);
      expect(y).toBeGreaterThanOrEqual(0); expect(y).toBeLessThan(GRID);
      expect(c).toBeGreaterThanOrEqual(0); expect(c).toBeLessThan(8);
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

  it('팔레트는 8색 hex다', () => {
    for (const p of PALETTES) {
      expect(p).toHaveLength(8);
      for (const c of p) expect(c).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it('눈 파츠(변형·표정 전부)는 눈 존(x 10–21, y 9–15) 안이다', () => {
    const allEyes = [...PARTS.eyes.flat(), ...EYES_BLINK, ...EYES_HAPPY, ...EYES_SLEEP];
    for (const [x, y] of allEyes) {
      expect(x).toBeGreaterThanOrEqual(10); expect(x).toBeLessThanOrEqual(21);
      expect(y).toBeGreaterThanOrEqual(9); expect(y).toBeLessThanOrEqual(15);
    }
  });

  it('모든 파츠 변형은 음영(1)과 하이라이트(6)를 사용한다 — 입체감 강제', () => {
    for (const v of [...PARTS.head, ...PARTS.body]) {
      const colors = new Set(v.map(([, , c]) => c));
      expect(colors.has(1) || colors.has(3)).toBe(true); // shade 계열
      expect(colors.has(6)).toBe(true); // highlight
    }
  });

  it('파츠 픽셀 밀도 — 32×32 리메이크가 실제로 조밀한지 (head·body 변형당 최소 60px)', () => {
    for (const v of [...PARTS.head, ...PARTS.body]) expect(v.length).toBeGreaterThanOrEqual(60);
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
