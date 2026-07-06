// 집계 finding evidence에서 세션 목록을 안전하게 추출 (코칭 v2 스펙 §5.2)
import type { SessionCtxItem } from '../api';

export function sessionIdsOf(evidence: unknown): string[] {
  if (typeof evidence !== 'object' || evidence === null) return [];
  const ids = (evidence as Record<string, unknown>).session_ids;
  return Array.isArray(ids) ? ids.filter((x): x is string => typeof x === 'string') : [];
}

export function totalSessionsOf(evidence: unknown, fallback: number): number {
  if (typeof evidence !== 'object' || evidence === null) return fallback;
  const t = (evidence as Record<string, unknown>).total_sessions;
  return typeof t === 'number' ? t : fallback;
}

export function ctxLine(s: SessionCtxItem): string {
  if (!s.first_ts) return s.project_id;
  const d = new Date(s.first_ts);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${s.project_id} · ${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())} 세션`;
}
