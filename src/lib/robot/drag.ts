/** pointerdown→현재 좌표 이동 거리가 임계값을 넘으면 드래그로 판정 (스펙 §6). */
export function isDrag(x0: number, y0: number, x1: number, y1: number, threshold = 4): boolean {
  return Math.hypot(x1 - x0, y1 - y0) > threshold;
}
