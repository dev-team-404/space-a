import { describe, expect, it } from 'vitest';
import { groundAnchor, occupiedWorldCells, placementOrigin, rotatedOffsets, rotatedOrigin, spriteGroundAnchor, wallOccupiedIndices, wallPlacementOrigin, wallSpanScreenWidth } from './geometry';

describe('placement geometry v2', () => {
  it.each([
    [[0, 0], [0, 0]], [[19, 0], [16, 0]], [[0, 19], [0, 18]], [[19, 19], [16, 18]],
  ] as const)('keeps a 4x2 sofa inside at target %j', (target, expected) => {
    expect(placementOrigin([...target], [4, 2], 0)).toEqual(expected);
  });

  it('rotates an L footprint with its bounding box', () => {
    const footprint = [[0,0],[1,0],[2,0],[0,1],[0,2]] as [number,number][];
    expect(rotatedOffsets({ size:[3,3], footprint, rotation:90 })).toEqual([[2,0],[2,1],[2,2],[1,0],[0,0]]);
  });

  it('allows another object in the empty corner of an L footprint', () => {
    const sofa = { cell:[1,1] as [number,number], size:[3,3] as [number,number], footprint:[[0,0],[1,0],[2,0],[0,1],[0,2]] as [number,number][], rotation:0 as const };
    expect(occupiedWorldCells(sofa)).not.toContainEqual([3,3]);
  });

  it('uses one stable ground anchor derived from the stored origin', () => {
    expect(groundAnchor({ cell:[0,18], size:[4,2], rotation:0 })).toEqual([2,19]);
    expect(groundAnchor({ cell:[18,16], size:[4,2], rotation:90 })).toEqual([19,18]);
  });

  it('binds a floor sprite to the front corner instead of the footprint center', () => {
    expect(spriteGroundAnchor({ cell:[3,4], size:[2,4], rotation:0 })).toEqual([5,8]);
    expect(spriteGroundAnchor({ cell:[3,4], size:[2,4], rotation:90 })).toEqual([7,6]);
  });

  it('supports an interior world contact for pedestal furniture', () => {
    expect(spriteGroundAnchor({ cell:[3,4], size:[2,2], rotation:0 }, [0.5,0.5])).toEqual([4,5]);
    expect(spriteGroundAnchor({ cell:[3,4], size:[2,2], rotation:90 }, [0.5,0.5])).toEqual([4,5]);
    expect(spriteGroundAnchor({ cell:[3,4], size:[2,2], rotation:0 }, [0.75,0.75])).toEqual([4.5,5.5]);
  });

  it('projects a wall span onto the same horizontal axis as the isometric grid', () => {
    expect(wallSpanScreenWidth(3, 18)).toBe(27);
    expect(wallSpanScreenWidth(5, 18)).toBe(45);
  });

  it('preserves the anchor when possible and moves inward at an edge', () => {
    const sofa = { cell:[0,18] as [number,number], size:[4,2] as [number,number], rotation:0 as const };
    expect(rotatedOrigin(sofa, 90)).toEqual([1,16]);
    expect(groundAnchor({ ...sofa, cell:rotatedOrigin(sofa, 90), rotation:90 })).toEqual([2,18]);
  });

  it('uses the same clamped wall origin for preview and placement', () => {
    expect(wallPlacementOrigin(0, 5)).toBe(0);
    expect(wallPlacementOrigin(10, 5)).toBe(8);
    expect(wallPlacementOrigin(19, 5)).toBe(15);
    expect(wallOccupiedIndices(4, 3)).toEqual([4, 5, 6]);
    expect(wallOccupiedIndices(8, 5)).toEqual([8, 9, 10, 11, 12]);
  });
});
