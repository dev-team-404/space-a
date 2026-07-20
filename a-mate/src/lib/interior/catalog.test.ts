import { describe, expect, it } from 'vitest';
import { DESK_SUPPORT_CONTACTS_BY_ASSET, DESK_SUPPORT_PADDING, FLOOR_PROJECTION_EXEMPT_BY_ASSET, FLOOR_SOURCE_AXES_BY_ASSET, FURNITURE_BY_ID, MIRRORED_DIRECTION, OPPOSITE_DIRECTION, ROTATIONS, SPRITE_DIRECTION_BY_ROTATION, WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET, WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION, canonicalizeFurnitureGeometry, wallRotation, type Rotation, type SpriteSource } from './catalog';
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
        const projection = window.render.projections[rotation];
        expect(projection.shearY + projection.scaleY * effectiveSource).toBeCloseTo(WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION[rotation]!, 6);
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
      ['computer', [3, 2]],
      ['wood-writing', [3, 2]],
      ['pastel-vanity', [3, 2]],
      ['metal-workstation', [3, 2]],
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

  it('registers multiple visible support contacts for both source images', () => {
    for (const contacts of Object.values(DESK_SUPPORT_CONTACTS_BY_ASSET)) {
      expect(contacts.sw.length).toBeGreaterThanOrEqual(3);
      expect(contacts.ne.length).toBeGreaterThanOrEqual(3);
    }
  });

  it('keeps every registered desk support inside all four footprint boundaries', () => {
    const metrics = assetMetricsJson as Record<string, { width: number; height: number }>;
    for (const [assetId, sourceContacts] of Object.entries(DESK_SUPPORT_CONTACTS_BY_ASSET)) {
      const desk = FURNITURE_BY_ID.get(assetId)!;
      const id = assetId.slice('desk.'.length);
      for (const rotation of ROTATIONS) {
        const source = desk.render.sources[rotation];
        const metric = metrics[`desk/${id}/${source}`];
        const mirrorX = desk.render.mirrorX[rotation];
        const projection = desk.render.projections[rotation];
        const [anchorX, anchorY] = desk.render.anchors[rotation];
        const scale = 2 * desk.render.widthTiles[rotation] / metric.width;
        const [width, height] = rotatedSize(desk.size, rotation);
        for (const [sourceX, sourceY] of sourceContacts[source]) {
          const x = mirrorX ? metric.width - sourceX : sourceX;
          const y = projection.shearY * x + projection.scaleY * sourceY;
          const screenX = scale * (x - anchorX * metric.width);
          const screenY = scale * (y - anchorY * metric.height);
          const u = (screenX + 2 * screenY) / 2;
          const v = (2 * screenY - screenX) / 2;
          expect(u, `${assetId} ${rotation}° contact ${sourceX},${sourceY} u`).toBeGreaterThanOrEqual(-width + DESK_SUPPORT_PADDING - 1e-6);
          expect(u, `${assetId} ${rotation}° contact ${sourceX},${sourceY} u`).toBeLessThanOrEqual(-DESK_SUPPORT_PADDING + 1e-6);
          expect(v, `${assetId} ${rotation}° contact ${sourceX},${sourceY} v`).toBeGreaterThanOrEqual(-height + DESK_SUPPORT_PADDING - 1e-6);
          expect(v, `${assetId} ${rotation}° contact ${sourceX},${sourceY} v`).toBeLessThanOrEqual(-DESK_SUPPORT_PADDING + 1e-6);
        }
      }
    }
  });

  it('maps every registered floor source axis onto the room grid', () => {
    for (const [assetId, sourceAxes] of Object.entries(FLOOR_SOURCE_AXES_BY_ASSET)) {
      const item = FURNITURE_BY_ID.get(assetId)!;
      for (const rotation of ROTATIONS) {
        const source = item.render.sources[rotation];
        const axes = sourceAxes[source];
        if (!axes) continue;
        const [sourcePositive, sourceNegative] = axes;
        const mirrored = item.render.mirrorX[rotation];
        const effectivePositive = mirrored ? -sourceNegative : sourcePositive;
        const effectiveNegative = mirrored ? -sourcePositive : sourceNegative;
        const projection = item.render.projections[rotation];
        expect(projection.shearY + projection.scaleY * effectivePositive, `${assetId} ${rotation}° positive`).toBeCloseTo(0.5, 6);
        expect(projection.shearY + projection.scaleY * effectiveNegative, `${assetId} ${rotation}° negative`).toBeCloseTo(-0.5, 6);
        expect(projection.scaleY, `${assetId} ${rotation}° scale`).toBeGreaterThan(0.85);
        expect(projection.scaleY, `${assetId} ${rotation}° scale`).toBeLessThan(1.15);
        expect(Math.abs(projection.shearY), `${assetId} ${rotation}° shear`).toBeLessThan(0.15);
      }
    }
  });

  it('anchors every window at the transformed midpoint of its bottom frame', () => {
    const metrics = assetMetricsJson as Record<string, { width: number; height: number; ground: [number, number] }>;
    for (const [assetId, sourceSlope] of Object.entries(WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET)) {
      const window = FURNITURE_BY_ID.get(assetId)!;
      const id = assetId.slice('window.'.length);
      const metric = metrics[`window/${id}/ne`];
      const centerSourceY = metric.ground[1]
        + sourceSlope * (0.5 - metric.ground[0]) * metric.width / metric.height;
      for (const rotation of [90, 180] as Rotation[]) {
        const projection = window.render.projections[rotation];
        const expectedY = projection.scaleY * centerSourceY
          + projection.shearY * 0.5 * metric.width / metric.height;
        expect(window.render.anchors[rotation][0], `${assetId} ${rotation}° x`).toBe(0.5);
        expect(window.render.anchors[rotation][1], `${assetId} ${rotation}° y`).toBeCloseTo(expectedY, 6);
      }
    }
  });

  it('classifies every floor asset as normalized or explicitly exempt', () => {
    for (const item of FURNITURE_BY_ID.values()) {
      if (item.category === 'window') continue;
      const axes = FLOOR_SOURCE_AXES_BY_ASSET[item.id];
      const exemption = FLOOR_PROJECTION_EXEMPT_BY_ASSET[item.id];
      expect(Boolean(axes) !== Boolean(exemption), item.id).toBe(true);
      if (!exemption) continue;
      for (const rotation of ROTATIONS) {
        expect(item.render.projections[rotation], `${item.id} ${rotation}°`).toEqual({ scaleY: 1, shearY: 0 });
      }
    }
  });

  it('migrates stored geometry to the current catalog contract', () => {
    const migrated = canonicalizeFurnitureGeometry({
      asset_id: 'desk.computer', category: 'desk', cell: [4, 5], size: [2, 4],
      footprint: [[0, 0]], rotation: 0,
    });
    expect(migrated.size).toEqual([3, 2]);
    expect(migrated.footprint).toHaveLength(6);
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
      for (const rotation of ROTATIONS) {
        expect(item.render.widthTiles[rotation]).toBeGreaterThanOrEqual(0.7);
        expect(item.render.widthTiles[rotation]).toBeLessThanOrEqual(0.9);
      }
    }
  });
});
