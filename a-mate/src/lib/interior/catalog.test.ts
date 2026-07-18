import { describe, expect, it } from 'vitest';
import { FURNITURE_BY_ID, MIRRORED_DIRECTION, OPPOSITE_DIRECTION, ROTATIONS, SPRITE_DIRECTION_BY_ROTATION, canonicalizeFurnitureGeometry, wallRotation, type SpriteSource } from './catalog';

const expectViews = (id: string, sources: SpriteSource[], mirrors: boolean[]) => {
  const item = FURNITURE_BY_ID.get(id)!;
  expect(ROTATIONS.map((rotation) => item.render.sources[rotation])).toEqual(sources);
  expect(ROTATIONS.map((rotation) => item.render.mirrorX[rotation])).toEqual(mirrors);
};

describe('interior asset orientation contract', () => {
  it('maps world footprint rotations to the matching sprite axes', () => {
    expect(SPRITE_DIRECTION_BY_ROTATION).toEqual({ 0: 'ne', 90: 'se', 180: 'sw', 270: 'nw' });
  });

  it('derives the only valid window rotation from its wall', () => {
    expect(wallRotation('west')).toBe(90);
    expect(wallRotation('north')).toBe(180);
    const upRightSources = ['mint-square', 'cream-wood'];
    const downRightSources = ['coral-arch', 'lavender-bay', 'navy-wide'];
    for (const id of upRightSources) {
      const window = FURNITURE_BY_ID.get(`window.${id}`)!;
      expect(window.sprites[90]).toBe(window.sprites[180]);
      expect([window.render.mirrorX[90], window.render.mirrorX[180]]).toEqual([false, true]);
    }
    for (const id of downRightSources) {
      const window = FURNITURE_BY_ID.get(`window.${id}`)!;
      expect(window.sprites[90]).toBe(window.sprites[180]);
      expect([window.render.mirrorX[90], window.render.mirrorX[180]]).toEqual([true, false]);
    }
  });

  it('keeps tables and chairs as independently placeable assets', () => {
    const table = FURNITURE_BY_ID.get('table.dark-modern')!;
    const chair = FURNITURE_BY_ID.get('chair.dark-modern')!;
    expect(table.category).toBe('table');
    expect(table.size).toEqual([5, 2]);
    expect(table.footprint).toHaveLength(10);
    expect(chair.category).toBe('chair');
    expect(chair.size).toEqual([1, 1]);
    expect(chair.footprint).toEqual([[0, 0]]);
    expect([...FURNITURE_BY_ID.keys()].some((id) => id.startsWith('dining.'))).toBe(false);
  });

  it('resolves every floor sprite from its declared actual facing', () => {
    for (const item of FURNITURE_BY_ID.values()) {
      if (item.category === 'window') continue;
      const spec = item.turnaround!;
      for (const rotation of ROTATIONS) {
        if (spec.orientation === 'invariant') continue;
        const source = item.render.sources[rotation];
        const declared = spec.sources[source]!;
        const actual = item.render.mirrorX[rotation] ? MIRRORED_DIRECTION[declared] : declared;
        const target = SPRITE_DIRECTION_BY_ROTATION[rotation];
        expect(
          actual === target || (spec.orientation === 'axial' && actual === OPPOSITE_DIRECTION[target]),
          `${item.id} ${rotation}° resolves to ${actual}, expected ${target}`,
        ).toBe(true);
      }
    }
  });

  it('uses the calibrated sofa directions reported by the editor', () => {
    for (const id of ['mint-loveseat', 'coral-two-seat', 'lavender-sectional', 'wood-frame', 'navy-modern']) {
      expectViews(`sofa.${id}`, ['sw', 'ne', 'ne', 'sw'], [true, false, true, false]);
    }
  });

  it('uses the calibrated desk directions reported by the editor', () => {
    for (const id of ['compact-study', 'computer', 'wood-writing', 'pastel-vanity', 'metal-workstation']) {
      expectViews(`desk.${id}`, ['sw', 'ne', 'ne', 'sw'], [false, false, true, true]);
    }
  });

  it('uses full rectangular desk footprints on the sprite-aligned axis', () => {
    const expected = new Map([
      ['compact-study', [2, 3]],
      ['computer', [2, 4]],
      ['wood-writing', [2, 3]],
      ['pastel-vanity', [2, 3]],
      ['metal-workstation', [2, 4]],
    ] as const);
    for (const [id, size] of expected) {
      const desk = FURNITURE_BY_ID.get(`desk.${id}`)!;
      expect(desk.size).toEqual(size);
      expect(desk.footprint).toHaveLength(size[0] * size[1]);
    }
  });

  it('migrates stored geometry to the current catalog contract', () => {
    const migrated = canonicalizeFurnitureGeometry({
      asset_id: 'desk.computer', category: 'desk', cell: [4, 5], size: [4, 2],
      footprint: [[0, 0]], rotation: 0,
    });
    expect(migrated.size).toEqual([2, 4]);
    expect(migrated.footprint).toHaveLength(8);
    expect(migrated.cell).toEqual([4, 5]);
  });

  it('treats standalone tables as axial and swaps the two chair directions', () => {
    for (const id of ['round-cafe', 'square-two', 'wood-four', 'pastel-breakfast', 'dark-modern']) {
      expectViews(`table.${id}`, ['sw', 'sw', 'sw', 'sw'], [false, true, false, true]);
    }
    for (const id of ['mint-cafe', 'coral-compact', 'warm-wood', 'pastel-cream', 'dark-modern']) {
      expectViews(`chair.${id}`, ['sw', 'ne', 'ne', 'sw'], [false, true, false, true]);
    }
  });

  it('anchors the round pedestal table at the footprint center', () => {
    expect(FURNITURE_BY_ID.get('table.round-cafe')!.render.footprintAnchor).toEqual([0.5, 0.5]);
  });

  it('calibrates the three appliance source-facing groups independently', () => {
    for (const id of ['retro-tv', 'compact-fridge']) {
      expectViews(`appliance.${id}`, ['sw', 'ne', 'ne', 'sw'], [true, false, true, false]);
    }
    expectViews('appliance.washer', ['sw', 'ne', 'ne', 'sw'], [false, false, true, true]);
    for (const id of ['stereo', 'desktop']) {
      expectViews(`appliance.${id}`, ['sw', 'ne', 'ne', 'sw'], [false, true, false, true]);
    }
  });

  it('gives the retro TV its full rectangular six-cell footprint', () => {
    const tv = FURNITURE_BY_ID.get('appliance.retro-tv')!;
    expect(tv.size).toEqual([3, 2]);
    expect(tv.footprint).toEqual([[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]]);
  });

  it('normalizes one-cell lighting widths instead of scaling by source pixels', () => {
    for (const id of ['warm-floor', 'pastel-table', 'retro-stand', 'paper-lantern']) {
      const item = FURNITURE_BY_ID.get(`lighting.${id}`)!;
      expect(item.size).toEqual([1, 1]);
      expect(item.render.widthTiles).toBeGreaterThanOrEqual(0.7);
      expect(item.render.widthTiles).toBeLessThanOrEqual(0.9);
    }
  });
});
