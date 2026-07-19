import { describe, expect, it } from 'vitest';
import { GRID, PALETTES, VARIANTS } from './parts';
import { buildRobotShapes, type RobotSpec, type Frame } from './render';

const baseSpec: RobotSpec = { antenna: 0, head: 0, eyes: 0, body: 0, arms: 0, palette: 0 };
const baseFrame: Frame = { offsetY: 0, expression: 'normal', antennaBlink: false };

describe('VARIANTS', () => {
  it('각 슬롯이 6종, 팔레트 8종', () => {
    expect(VARIANTS.antenna).toBe(6);
    expect(VARIANTS.head).toBe(6);
    expect(VARIANTS.eyes).toBe(6);
    expect(VARIANTS.body).toBe(6);
    expect(VARIANTS.arms).toBe(6);
    expect(VARIANTS.palette).toBe(8);
  });
});

describe('PALETTES', () => {
  it('8팔레트 × 9색 hex (v5: outfit/shade/accent/pants/hair/outline/skin/cheek/white)', () => {
    expect(PALETTES).toHaveLength(8);
    for (const p of PALETTES) {
      expect(p).toHaveLength(9);
      for (const c of p) expect(c).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe('GRID', () => {
  it('128', () => { expect(GRID).toBe(128); });
});

describe('buildRobotShapes', () => {
  it('결정적이다 — 같은 입력은 deep equal 결과', () => {
    expect(buildRobotShapes(baseSpec, baseFrame)).toEqual(buildRobotShapes(baseSpec, baseFrame));
  });

  it('도형 극값이 0..128 경계 안', () => {
    for (let a = 0; a < 6; a++) {
      for (let h = 0; h < 6; h++) {
        const spec = { ...baseSpec, antenna: a, head: h };
        for (const s of buildRobotShapes(spec, baseFrame)) {
          if (s.kind === 'ellipse' || s.kind === 'stroke-ellipse') {
            expect(s.cx - s.rx).toBeGreaterThanOrEqual(-1); // 1px leeway for outline
            expect(s.cx + s.rx).toBeLessThanOrEqual(129);
            expect(s.cy - s.ry).toBeGreaterThanOrEqual(-1);
            expect(s.cy + s.ry).toBeLessThanOrEqual(129);
          } else if (s.kind === 'rrect' || s.kind === 'stroke-rrect') {
            expect(s.x).toBeGreaterThanOrEqual(-1);
            expect(s.x + s.w).toBeLessThanOrEqual(129);
            expect(s.y).toBeGreaterThanOrEqual(-1);
            expect(s.y + s.h).toBeLessThanOrEqual(129);
          }
        }
      }
    }
  });

  it('color 인덱스가 0..8 범위', () => {
    for (const s of buildRobotShapes(baseSpec, baseFrame)) {
      expect(s.color).toBeGreaterThanOrEqual(0);
      expect(s.color).toBeLessThan(9);
    }
  });

  it('밀도 게이트 — shapes >= 22개 (미니미 개편으로 눈 단순화: 반짝 1개)', () => {
    const shapes = buildRobotShapes(baseSpec, baseFrame);
    expect(shapes.length).toBeGreaterThanOrEqual(22);
  });

  it('cheek(7)과 skin(6)이 각각 1개 이상', () => {
    const shapes = buildRobotShapes(baseSpec, baseFrame);
    expect(shapes.some(s => s.color === 7)).toBe(true);
    expect(shapes.some(s => s.color === 6)).toBe(true);
  });

  it('변형 상이 — antenna 슬롯', () => {
    const s0 = buildRobotShapes({ ...baseSpec, antenna: 0 }, baseFrame);
    const s1 = buildRobotShapes({ ...baseSpec, antenna: 1 }, baseFrame);
    expect(s0).not.toEqual(s1);
  });

  it('변형 상이 — head 슬롯', () => {
    const s0 = buildRobotShapes({ ...baseSpec, head: 0 }, baseFrame);
    const s1 = buildRobotShapes({ ...baseSpec, head: 1 }, baseFrame);
    expect(s0).not.toEqual(s1);
  });

  it('변형 상이 — eyes 슬롯', () => {
    const s0 = buildRobotShapes({ ...baseSpec, eyes: 0 }, baseFrame);
    const s1 = buildRobotShapes({ ...baseSpec, eyes: 1 }, baseFrame);
    expect(s0).not.toEqual(s1);
  });

  it('변형 상이 — body 슬롯', () => {
    const s0 = buildRobotShapes({ ...baseSpec, body: 0 }, baseFrame);
    const s1 = buildRobotShapes({ ...baseSpec, body: 1 }, baseFrame);
    expect(s0).not.toEqual(s1);
  });

  it('변형 상이 — arms 슬롯', () => {
    const s0 = buildRobotShapes({ ...baseSpec, arms: 0 }, baseFrame);
    const s1 = buildRobotShapes({ ...baseSpec, arms: 1 }, baseFrame);
    expect(s0).not.toEqual(s1);
  });

  it('표정 blink ≠ normal', () => {
    const normal = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'normal' });
    const blink  = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'blink' });
    expect(normal).not.toEqual(blink);
  });

  it('표정 sleep ≠ normal', () => {
    const normal = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'normal' });
    const sleep  = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'sleep' });
    expect(normal).not.toEqual(sleep);
  });

  it('표정 happy ≠ normal', () => {
    const normal = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'normal' });
    const happy  = buildRobotShapes(baseSpec, { ...baseFrame, expression: 'happy' });
    expect(normal).not.toEqual(happy);
  });

  it('antennaBlink true/false가 shapes를 변경', () => {
    const off = buildRobotShapes(baseSpec, { ...baseFrame, antennaBlink: false });
    const on  = buildRobotShapes(baseSpec, { ...baseFrame, antennaBlink: true });
    expect(off).not.toEqual(on);
  });
});
