import { PALETTES, PARTS, type Px } from './parts';

export interface RobotSpec {
  antenna: number; head: number; eyes: number; body: number; arms: number; palette: number;
}

/** 조립: body→arms→head→eyes→antenna. 같은 좌표는 나중 파츠가 덮는다. */
export function buildRobotPixels(spec: RobotSpec, eyesOverride?: Px[]): Px[] {
  const layers: Px[][] = [
    PARTS.body[spec.body],
    PARTS.arms[spec.arms],
    PARTS.head[spec.head],
    eyesOverride ?? PARTS.eyes[spec.eyes],
    PARTS.antenna[spec.antenna],
  ];
  const grid = new Map<string, Px>();
  for (const layer of layers) for (const px of layer) grid.set(`${px[0]},${px[1]}`, px);
  return [...grid.values()].sort((a, b) => a[1] - b[1] || a[0] - b[0]);
}

export function drawRobot(
  ctx: CanvasRenderingContext2D,
  spec: RobotSpec,
  opts: { eyesOverride?: Px[]; offsetY?: number } = {},
): void {
  const palette = PALETTES[spec.palette];
  ctx.clearRect(0, 0, 16, 16);
  for (const [x, y, c] of buildRobotPixels(spec, opts.eyesOverride)) {
    ctx.fillStyle = palette[c];
    ctx.fillRect(x, y + (opts.offsetY ?? 0), 1, 1);
  }
}
