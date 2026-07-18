import type { FootprintCell, Rotation } from './catalog';

export type WorldCell = [number, number];
export const ROOM_GRID = { w: 20, h: 20 } as const;

export interface GeometryObject {
  cell: WorldCell;
  size: WorldCell;
  footprint?: FootprintCell[];
  rotation: Rotation;
}

export function rotatedSize(size: WorldCell, rotation: Rotation): WorldCell {
  return rotation === 90 || rotation === 270 ? [size[1], size[0]] : size;
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

export function placementOrigin(target: WorldCell, size: WorldCell, rotation: Rotation, grid = ROOM_GRID): WorldCell {
  const [w, h] = rotatedSize(size, rotation);
  const clamp = (value: number, max: number) => Math.max(0, Math.min(max, value));
  return [clamp(target[0] - Math.floor(w / 2), grid.w - w), clamp(target[1] - Math.floor(h / 2), grid.h - h)];
}

export function rotatedOrigin(object: GeometryObject, rotation: Rotation, grid = ROOM_GRID): WorldCell {
  const [anchorX, anchorY] = groundAnchor(object);
  const [w, h] = rotatedSize(object.size, rotation);
  const clamp = (value: number, max: number) => Math.max(0, Math.min(max, value));
  return [
    clamp(Math.round(anchorX - w / 2), grid.w - w),
    clamp(Math.round(anchorY - h / 2), grid.h - h),
  ];
}

export function wallPlacementOrigin(targetIndex: number, span: number, wallLength = ROOM_GRID.w): number {
  return Math.max(0, Math.min(wallLength - span, targetIndex - Math.floor(span / 2)));
}

export function wallOccupiedIndices(origin: number, span: number): number[] {
  return Array.from({ length: span }, (_, index) => origin + index);
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

export function isInsideRoom(object: GeometryObject, grid = ROOM_GRID): boolean {
  return occupiedWorldCells(object).every(([x, y]) => x >= 0 && y >= 0 && x < grid.w && y < grid.h);
}
