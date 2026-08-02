// 집계 finding evidence에서 세션 목록을 안전하게 추출 (코칭 v2 스펙 §5.2)
import type { CoachFinding, ContentItem, SessionCtxItem } from '../api';

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

/** 개인 실전 레슨 본문을 문법 A의 슬롯으로 쪼갠다 (스펙 §4.1).
 * 마커(`• 원리:` / `• 이렇게:` / `👉 첫걸음:`)가 없는 레슨도 있으므로
 * 그때는 전부 근거로 넘긴다 — 본문을 잃지 않는 것이 우선이다. */
export interface LessonBody {
  evidence: string | null;
  principle: string | null;
  action: string | null;
}

const PRINCIPLE_MARK = '• 원리:';
const ACTION_MARKS = ['• 이렇게:', '👉 첫걸음:'];

export function splitLessonBody(body: string): LessonBody {
  const buckets: Record<keyof LessonBody, string[]> = { evidence: [], principle: [], action: [] };
  let current: keyof LessonBody = 'evidence';
  for (const raw of body.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    const actionMark = ACTION_MARKS.find((m) => line.startsWith(m));
    if (line.startsWith(PRINCIPLE_MARK)) {
      current = 'principle';
      buckets.principle.push(line.slice(PRINCIPLE_MARK.length).trim());
    } else if (actionMark) {
      current = 'action';
      buckets.action.push(line.slice(actionMark.length).trim());
    } else {
      buckets[current].push(line);
    }
  }
  const join = (parts: string[]): string | null => {
    const s = parts.filter(Boolean).join(' ').trim();
    return s === '' ? null : s;
  };
  return { evidence: join(buckets.evidence), principle: join(buckets.principle), action: join(buckets.action) };
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

/** 역량 사다리 축 → 한국어 배지 (옛 「오늘의 배움」 컨테이너에서 이관) */
const DIM_LABEL: Record<string, string> = {
  model_literacy: '모델 고르기',
  context_hygiene: '컨텍스트 정리',
  skill_reuse: '스킬로 반복 줄이기',
  automation: '워크플로 자동화',
  orchestration: '작업 위임',
};

export interface LogCardView {
  key: string;
  source: 'finding' | 'lesson';
  icon: string;
  title: string;
  evidence: string | null;
  chip: string | null;
  reason: string | null;
  principle: string | null;
  action: string | null;
  sourceUrl: string | null;
}

export interface LearnCardView {
  id: string;
  badge: string;
  title: string;
  summary: string | null;
  sourceUrl: string | null;
}

const severityIcon = (s: CoachFinding['severity']): string =>
  s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ';

const orNull = (s: string | null | undefined): string | null => {
  const t = (s ?? '').trim();
  return t === '' ? null : t;
};

/** 룰 finding → 문법 A. 원리는 레슨 전용 슬롯이라 항상 null. */
export function toLogCardView(f: CoachFinding): LogCardView {
  return {
    key: f.dedup_key,
    source: 'finding',
    icon: severityIcon(f.severity),
    title: coachTitle(f.rule_id, f.evidence),
    evidence: orNull(f.detail),
    chip: evidenceChip(f.rule_id, f.evidence),
    reason: orNull(f.judgment?.reason),
    principle: null,
    action: orNull(f.suggested_action),
    sourceUrl: null,
  };
}

/** 개인 실전 레슨 → 문법 A. 칩·판정은 룰 전용이라 null.
 * `personal`(store의 enrich_personal)은 **태그로** 붙어서 레슨 본문과 다른 주제일 수 있다 —
 * 예: `lesson-cache`는 `subagent` 태그가 있어 캐시 재읽기 레슨에 서브에이전트 수치가 달린다.
 * 하나만 남기면 제목·원리·행동과 어긋난 근거만 보이므로 둘 다 싣는다(같으면 한 번만).
 * `.evidence`가 `white-space: pre-line`이라 개행이 그대로 두 줄로 렌더된다. */
export function toLessonCardView(c: ContentItem): LogCardView {
  const parts = splitLessonBody(c.body);
  const lines = [...new Set([orNull(c.personal), parts.evidence].filter((s): s is string => s !== null))];
  return {
    key: c.id,
    source: 'lesson',
    icon: '💡',
    title: c.title,
    evidence: lines.length === 0 ? null : lines.join('\n'),
    chip: null,
    reason: null,
    principle: parts.principle,
    action: parts.action,
    sourceUrl: orNull(c.source_url),
  };
}

/** 커리큘럼·외부 팁·소식 → 문법 B(카드뉴스). */
export function toLearnCardView(c: ContentItem): LearnCardView {
  const has = (t: string) => c.trigger_tags?.includes(t) ?? false;
  const badge = c.kind === 'news' || has('changelog')
    ? '소식'
    : has('boris')
      ? 'Boris'
      : has('team')
        ? '팀'
        : c.dimension
          ? (DIM_LABEL[c.dimension] ?? c.dimension)
          : '배움';
  return { id: c.id, badge, title: c.title, summary: orNull(c.body), sourceUrl: orNull(c.source_url) };
}

/** 큐레이션 콘텐츠가 카드가 되는 상한.
 * `store.list_content`는 LIMIT 없이 `score>=0`인 행을 전부 주고, 옛 「오늘의 배움」 컨테이너가
 * `items[0]` + `items.slice(1, 4)`로 4건만 보여줬다. 컨테이너를 해체하면서 그 상한까지
 * 같이 없애면 카드가 수십 장으로 불어난다 — 상한은 여기서 유지한다. 룰 finding은 대상이 아니다. */
export const CONTENT_CARD_LIMIT = 4;

/** 근거 출처로 두 섹션을 가른다 (스펙 §3).
 * 「내 로그에서」 = 룰 finding + `personal` 태그 콘텐츠. finding을 앞에 둔다 —
 * 처방·행동 버튼이 붙어 바로 실행 가능한 쪽이기 때문.
 * 상한은 score 순 상위 N건에 먼저 적용한 뒤 가른다(두 섹션 합계 기준 — 옛 컨테이너와 동일). */
export function partitionCoachItems(
  findings: CoachFinding[],
  content: ContentItem[],
): { log: LogCardView[]; learn: LearnCardView[] } {
  const isPersonal = (c: ContentItem) => c.trigger_tags?.includes('personal') ?? false;
  const top = content.slice(0, CONTENT_CARD_LIMIT);
  return {
    log: [...findings.map(toLogCardView), ...top.filter(isPersonal).map(toLessonCardView)],
    learn: top.filter((c) => !isPersonal(c)).map(toLearnCardView),
  };
}
