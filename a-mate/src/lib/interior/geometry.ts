import type { FootprintCell, Rotation } from './catalog';

export type WorldCell = [number, number];
export const LIFE_GRID = { w: 20, h: 20 } as const;
export const LIFE_FLOOR = { maxXYExclusive: 20 } as const;

export interface GeometryObject {
  cell: WorldCell;
  size: WorldCell;
  footprint?: FootprintCell[];
  rotation: Rotation;
}

export function rotatedSize(size: WorldCell, rotation: Rotation): WorldCell {
  return rotation === 90 || rotation === 270 ? [size[1], size[0]] : size;
}

export function isFloorCell(
  [x, y]: WorldCell,
  grid = LIFE_GRID,
  maxXYExclusive = LIFE_FLOOR.maxXYExclusive,
): boolean {
  return x >= 0 && y >= 0 && x < grid.w && y < grid.h && x + y < maxXYExclusive;
}

export function rectangleFootprint(size: WorldCell): FootprintCell[] {
  return Array.from({ length: size[0] * size[1] }, (_, i) => [i % size[0], Math.floor(i / size[0])]);
}

export function rotatedOffsets(object: Pick<GeometryObject, 'size' | 'footprint' | 'rotation'>): FootprintCell[] {
  const source = object.footprint?.length ? object.footprint : rectangleFootprint(object.size);
  const [w, h] = object.size;
  return source.map(([x, y]) => {
    if (object.rotation === 90) return [h - 1 - y, x];
    if (object.rotation === 180) return [w - 1 - x, h - 1 - y];
    if (object.rotation === 270) return [y, w - 1 - x];
    return [x, y];
  });
}

function nearestValidOrigin(
  desired: WorldCell,
  size: WorldCell,
  rotation: Rotation,
  footprint: FootprintCell[] | undefined,
  grid = LIFE_GRID,
): WorldCell {
  const [w, h] = rotatedSize(size, rotation);
  const candidates: { cell: WorldCell; manhattan: number; chebyshev: number }[] = [];
  for (let y = 0; y <= grid.h - h; y += 1) {
    for (let x = 0; x <= grid.w - w; x += 1) {
      const cell: WorldCell = [x, y];
      if (!isInsideLife({ cell, size, rotation, footprint }, grid)) continue;
      const dx = Math.abs(x - desired[0]), dy = Math.abs(y - desired[1]);
      candidates.push({ cell, manhattan: dx + dy, chebyshev: Math.max(dx, dy) });
    }
  }
  candidates.sort((a, b) =>
    a.manhattan - b.manhattan
    || a.chebyshev - b.chebyshev
    || a.cell[1] - b.cell[1]
    || a.cell[0] - b.cell[0]
  );
  return candidates[0]?.cell ?? [0, 0];
}

export function placementOrigin(
  target: WorldCell,
  size: WorldCell,
  rotation: Rotation,
  grid = LIFE_GRID,
  footprint?: FootprintCell[],
): WorldCell {
  const [w, h] = rotatedSize(size, rotation);
  const desired: WorldCell = [
    Math.max(0, Math.min(grid.w - w, target[0] - Math.floor(w / 2))),
    Math.max(0, Math.min(grid.h - h, target[1] - Math.floor(h / 2))),
  ];
  return nearestValidOrigin(desired, size, rotation, footprint, grid);
}

export function rotatedOrigin(object: GeometryObject, rotation: Rotation, grid = LIFE_GRID): WorldCell {
  const [anchorX, anchorY] = groundAnchor(object);
  const [w, h] = rotatedSize(object.size, rotation);
  const desired: WorldCell = [
    Math.max(0, Math.min(grid.w - w, Math.round(anchorX - w / 2))),
    Math.max(0, Math.min(grid.h - h, Math.round(anchorY - h / 2))),
  ];
  return nearestValidOrigin(desired, object.size, rotation, object.footprint, grid);
}

export function wallPlacementOrigin(targetIndex: number, span: number, wallLength = LIFE_GRID.w): number {
  return Math.max(0, Math.min(wallLength - span, targetIndex - Math.floor(span / 2)));
}

export function wallOccupiedIndices(origin: number, span: number): number[] {
  return Array.from({ length: span }, (_, index) => origin + index);
}

export function wallSpanScreenWidth(span: number, tileWidth: number): number {
  return span * tileWidth / 2;
}

export function occupiedWorldCells(object: GeometryObject): WorldCell[] {
  return rotatedOffsets(object).map(([x, y]) => [object.cell[0] + x, object.cell[1] + y]);
}

export function groundAnchor(object: GeometryObject): [number, number] {
  const [w, h] = rotatedSize(object.size, object.rotation);
  return [object.cell[0] + w / 2, object.cell[1] + h / 2];
}

export function spriteGroundAnchor(object: GeometryObject, footprintAnchor: WorldCell = [1, 1]): [number, number] {
  const [w, h] = rotatedSize(object.size, object.rotation);
  return [object.cell[0] + w * footprintAnchor[0], object.cell[1] + h * footprintAnchor[1]];
}

export function isInsideLife(object: GeometryObject, grid = LIFE_GRID): boolean {
  return occupiedWorldCells(object).every((cell) => isFloorCell(cell, grid));
}
