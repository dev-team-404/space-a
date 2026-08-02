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

/** evidence에서 유한한 숫자 키를 안전하게 꺼낸다. 없거나 숫자가 아니면 null. */
function numberOf(evidence: unknown, key: string): number | null {
  if (typeof evidence !== 'object' || evidence === null) return null;
  const v = (evidence as Record<string, unknown>)[key];
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

/** 카드 헤더의 근거 칩 — "근거가 얼마나 굳어졌나"를 룰별 evidence 실수치로 보여준다.
 * 절약 토큰(est_tokens_saved)은 등록된 룰이 전부 0이라 표시하지 않는다.
 * occurrences·last_seen은 스캔마다 갱신되는 **스캔 지표**라 여기 쓰지 않는다.
 * 재료가 없으면 null — 0이나 추정치를 지어내지 않는다. */
export function evidenceChip(ruleId: string, evidence: unknown): string | null {
  const count = (key: string, suffix: string): string | null => {
    const v = numberOf(evidence, key);
    return v !== null && v > 0 ? `${v}${suffix}` : null;
  };
  switch (ruleId) {
    case 'R6':
      return count('session_count', '개 세션');
    case 'R7':
      return count('total_sessions', '개 세션');
    case 'R8':
      return count('large_result_count', '회 대형 결과');
    default:
      return null;
  }
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

// ops::registered_rules()에 등록된 룰만 — 은퇴 룰(R1·R2·R5·R9·R10·R11·R12·R24)은
// 매 스캔 purge되어 카드가 뜨지 않으므로 제목도 두지 않는다.
const COACH_TITLE: Record<string, string> = {
  R6: '같은 지시를 여러 세션에서 반복하고 있어요',
  R7: '이 프로젝트, 가벼운 작업엔 시작 모델을 낮춰보세요',
  R8: '큰 MCP 결과가 매번 컨텍스트를 잡아먹어요',
};

/** 두 번째 인자는 룰별 evidence 분기 자리 — 지금은 분기가 없지만 호출부(CoachTab)
 * 시그니처를 유지해 R5 같은 subtype 분기가 다시 생길 때 흔들리지 않게 둔다. */
export function coachTitle(ruleId: string, _evidence: unknown): string {
  return COACH_TITLE[ruleId] ?? '아낄 수 있는 게 보여요';
}

/** 코치 탭 '숨긴 항목'에 보일 상태 — 사용자가 직접 처분한 것만.
 * pending(판정 대기)·rejected(판정 탈락)는 내부 상태라 노출하지 않는다 (fail-safe 침묵). */
export function isHiddenFinding(status: string): boolean {
  return status === 'resolved' || status === 'dismissed';
}
