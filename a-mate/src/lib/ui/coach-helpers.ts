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
  let base: string;
  if (!s.first_ts) {
    base = s.project_id;
  } else {
    const d = new Date(s.first_ts);
    const p = (n: number) => String(n).padStart(2, '0');
    base = `${s.project_id} · ${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())} 세션`;
  }
  return s.first_prompt ? `${base} — “${s.first_prompt}”` : base;
}

const COACH_TITLE: Record<string, string> = {
  R1: '안 쓰는 MCP 서버가 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '세션 안에서 같은 파일을 여러 번 다시 읽었어요',
  R7: '이 프로젝트, 가벼운 작업엔 시작 모델을 낮춰보세요',
  R9: '웹 검색이 너무 잦아요',
  R10: '자동화 파이프라인이 Opus로 돌고 있어요',
  R6: '같은 지시를 여러 세션에서 반복하고 있어요',
  R8: '큰 MCP 결과가 매번 컨텍스트를 잡아먹어요',
  R11: '거부한 뒤 결국 허용한 도구가 있어요',
  R12: '설치해둔 스킬이 놀고 있어요',
};

function subtypeOf(evidence: unknown): string | null {
  if (typeof evidence !== 'object' || evidence === null) return null;
  const s = (evidence as Record<string, unknown>).subtype;
  return typeof s === 'string' ? s : null;
}

export function coachTitle(ruleId: string, evidence: unknown): string {
  if (ruleId === 'R5' && subtypeOf(evidence) === 'cross_session_claude_md') {
    return '여러 세션에서 반복해 읽는 파일 — CLAUDE.md에 넣어두면 좋겠어요';
  }
  return COACH_TITLE[ruleId] ?? '아낄 수 있는 게 보여요';
}
