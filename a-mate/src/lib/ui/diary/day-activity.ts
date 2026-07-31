/** 다이어리 일별 활동 패널의 포맷·계산 — 컴포넌트에서 분리해 테스트 가능하게 둔다. */

/** 폭 240px 컬럼용 토큰 표기. 백만 이상만 줄인다(그 아래는 자릿수가 들어간다). */
export function compactTokens(n: number): string {
  if (!Number.isFinite(n)) return '0';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  return n.toLocaleString();
}

/** ISO 시각 → "09:12". 파싱 불가면 빈 문자열(라벨에 "Invalid Date"를 흘리지 않는다). */
export function hhmm(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

export interface ToolBar {
  kind: string;
  count: number;
  pct: number;
}

/** 상위 `limit`종만, 최대값을 100%로 한 막대 폭. 빈 입력·전부 0에서도 NaN이 없다.
 *  `byKind`는 Rust에서 이미 count 내림차순이라(diary/mod.rs의 ORDER BY COUNT(*) DESC) slice로 충분하다. */
export function toolBars(byKind: [string, number][], limit: number): ToolBar[] {
  const top = byKind.slice(0, limit);
  const max = Math.max(...top.map(([, n]) => n), 0);
  return top.map(([kind, count]) => ({
    kind,
    count,
    pct: max > 0 ? Math.round((count / max) * 100) : 0,
  }));
}
