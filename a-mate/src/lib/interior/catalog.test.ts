import { describe, expect, it } from 'vitest';
import { FURNITURE_BY_ID, MIRRORED_DIRECTION, OPPOSITE_DIRECTION, ROTATIONS, SPRITE_DIRECTION_BY_ROTATION, WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET, WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION, canonicalizeFurnitureGeometry, wallRotation, type Rotation, type SpriteSource } from './catalog';
import { rotatedSize } from './geometry';
import assetMetricsJson from './asset-metrics.json';

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

  it('shears every window frame onto the exact isometric wall slope', () => {
    for (const [id, sourceSlope] of Object.entries(WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET)) {
      const window = FURNITURE_BY_ID.get(id)!;
      for (const rotation of [90, 180] as Rotation[]) {
        const effectiveSource = window.render.mirrorX[rotation] ? -sourceSlope : sourceSlope;
        expect(effectiveSource + window.render.shearY[rotation]).toBeCloseTo(WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION[rotation]!, 6);
      }
    }
  });

  it('keeps each window wall span proportional to its asset width', () => {
    expect(['mint-square', 'cream-wood', 'coral-arch', 'lavender-bay', 'navy-wide'].map((id) =>
      FURNITURE_BY_ID.get(`window.${id}`)!.size[0],
    )).toEqual([3, 3, 3, 5, 5]);
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
      ['compact-study', [3, 2]],
      ['computer', [4, 2]],
      ['wood-writing', [3, 2]],
      ['pastel-vanity', [3, 2]],
      ['metal-workstation', [4, 2]],
    ] as const);
    for (const [id, size] of expected) {
      const desk = FURNITURE_BY_ID.get(`desk.${id}`)!;
      expect(desk.size).toEqual(size);
      expect(desk.footprint).toHaveLength(size[0] * size[1]);
      expect(ROTATIONS.map((rotation) => rotatedSize(desk.size, rotation))).toEqual([
        [...size], [size[1], size[0]], [...size], [size[1], size[0]],
      ]);
    }
  });

  it('anchors every desk view to its measured source contact pixel', () => {
    for (const id of ['compact-study', 'computer', 'wood-writing', 'pastel-vanity', 'metal-workstation']) {
      const desk = FURNITURE_BY_ID.get(`desk.${id}`)!;
      for (const rotation of ROTATIONS) {
        const source = desk.render.sources[rotation];
        const metric = (assetMetricsJson as Record<string, { ground: [number, number] }>)[`desk/${id}/${source}`];
        const expectedX = desk.render.mirrorX[rotation] ? 1 - metric.ground[0] : metric.ground[0];
        expect(desk.render.anchors[rotation][0], `${desk.id} ${rotation}°`).toBeCloseTo(expectedX, 6);
      }
    }
  });

  it('migrates stored geometry to the current catalog contract', () => {
    const migrated = canonicalizeFurnitureGeometry({
      asset_id: 'desk.computer', category: 'desk', cell: [4, 5], size: [2, 4],
      footprint: [[0, 0]], rotation: 0,
    });
    expect(migrated.size).toEqual([4, 2]);
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

  it('anchors the round pedestal table at the front edge of its centered base', () => {
    expect(FURNITURE_BY_ID.get('table.round-cafe')!.render.footprintAnchor).toEqual([0.75, 0.75]);
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
