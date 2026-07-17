// 월 달력 그리드 — UTC 기반 계산으로 타임존 무관 결정성.
export interface CalCell {
  date: string; // YYYY-MM-DD
  day: number;
  inMonth: boolean;
}

const iso = (d: Date) => d.toISOString().slice(0, 10);

/** 일요일 시작, 항상 6주(42칸). month는 1~12. */
export function monthGrid(year: number, month: number): CalCell[] {
  const first = new Date(Date.UTC(year, month - 1, 1));
  const start = new Date(first);
  start.setUTCDate(1 - first.getUTCDay()); // 그 주 일요일로
  const cells: CalCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start);
    d.setUTCDate(start.getUTCDate() + i);
    cells.push({ date: iso(d), day: d.getUTCDate(), inMonth: d.getUTCMonth() === month - 1 });
  }
  return cells;
}

export function shiftMonth(year: number, month: number, delta: number): [number, number] {
  const d = new Date(Date.UTC(year, month - 1 + delta, 1));
  return [d.getUTCFullYear(), d.getUTCMonth() + 1];
}
