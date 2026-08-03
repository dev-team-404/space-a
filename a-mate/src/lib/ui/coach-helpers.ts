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
  /** 스트림 정렬축 (§2.2). 값이 없으면 null — 정렬에서 맨 뒤로 간다. */
  firstSeen: string | null;
}

export interface LearnCardView {
  id: string;
  badge: string;
  title: string;
  /** 📊 근거 — store의 `enrich_personal`이 채운 "당신 로그: …" 실측 한 줄. 없으면 null. */
  evidence: string | null;
  summary: string | null;
  sourceUrl: string | null;
  /** 유효 기한 `YYYY-MM-DD` — 고정 슬롯의 기한 칩 (⑥ §6.4). 없으면 null. */
  deadline: string | null;
  /** 스트림 정렬축 (§2.2). 값이 없으면 null — 정렬에서 맨 뒤로 간다. */
  firstSeen: string | null;
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
    firstSeen: f.first_seen ?? null,
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
    firstSeen: c.first_seen ?? null,
  };
}

/** 커리큘럼·외부 팁·소식 → 문법 B(카드뉴스).
 * 번역 캐시(`title_ko`·`summary_ko`, ⑥ §6.3)가 있으면 그것을 쓰고, 없으면 원문으로 폴백한다 —
 * 엔진 미설정 사용자에게도 영어 원문이 그대로 값을 남긴다(소식은 코칭이 아니라 정보 전달). */
export function toLearnCardView(c: ContentItem): LearnCardView {
  const has = (t: string) => c.trigger_tags?.includes(t) ?? false;
  const badge = has('notice')
    ? '공지' // lastShownEmergencyTip — 장애 안내라 프로모(「소식」)와 갈린다 (§6.1)
    : c.kind === 'news' || has('changelog')
      ? '소식'
      : has('boris')
        ? 'Boris'
        : has('team')
          ? '팀'
          : c.dimension
            ? (DIM_LABEL[c.dimension] ?? c.dimension)
            : '배움';
  return {
    id: c.id,
    badge,
    title: orNull(c.title_ko) ?? c.title,
    // 📊 근거 — 커리큘럼은 지도, 내 로그는 GPS. 문법 A와 같은 슬롯을 써서 두 카드가
    // "근거 → 내용" 구조로 수렴한다. 재료는 store가 이미 채워 보내는 결정론 수치라
    // LLM이 필요 없고 매 조회마다 최신이다 — 캐시가 굳지 않는다.
    // 재료가 없으면 줄을 생략한다(근거 칩과 같은 규칙: 0·추정치를 지어내지 않는다).
    evidence: orNull(c.personal),
    summary: orNull(c.summary_ko) ?? orNull(c.body),
    sourceUrl: orNull(c.source_url),
    deadline: orNull(c.deadline),
    firstSeen: c.first_seen ?? null,
  };
}

/** 기한 문자열이 실제 달력 날짜인가 — `2026-13-40`·`곧` 같은 오추출을 거른다. */
function validDeadline(d: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d)) return false;
  const parsed = new Date(`${d}T00:00:00`);
  return !Number.isNaN(parsed.getTime()) && d.endsWith(String(parsed.getDate()).padStart(2, '0'));
}

/** 최상단 고정 슬롯에 올릴 소식 1건 (⑥ §6.4). 없으면 null.
 *
 * 고정 조건은 **기한이 있고 · 유효하고 · 아직 확인하지 않았을 때**뿐이다. 셋 중 하나라도
 * 빠지면 일반 소식 카드로 남는다(기본 폐쇄). 특히 **기한 경과 자동 강등이 LLM 오추출의
 * 안전망**이다 — 잘못 뽑힌 날짜라도 그날이 지나면 스스로 내려온다.
 *
 * `items`는 점수 내림차순이므로 첫 후보가 곧 우선순위 1위다. 상한은 1건. */
export function pinnedNewsItem(
  items: ContentItem[],
  ackedIds: string[],
  today: string,
): ContentItem | null {
  return (
    items.find((c) => {
      const d = orNull(c.deadline);
      return d !== null && validDeadline(d) && d >= today && !ackedIds.includes(c.id);
    }) ?? null
  );
}

/** 고정 슬롯 `[확인]` 기록 — 표시 층 상태라 localStorage에 둔다(notices·unseen 선례).
 * 기한 경과 자동 강등이 안전망이라 이 기록이 없어져도 카드가 영원히 고정되지 않는다. */
const PIN_ACK_KEY = 'agent-mentor.newsPinAcked';

export function loadPinAcks(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(PIN_ACK_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

/** 최근 것부터 상한을 둬 무한히 자라지 않게 한다. 실제로 저장된 목록을 돌려주므로
 *  호출부의 상태와 localStorage가 어긋나지 않는다. */
export function savePinAcks(ids: string[]): string[] {
  const kept = ids.slice(-30);
  localStorage.setItem(PIN_ACK_KEY, JSON.stringify(kept));
  return kept;
}

/** 큐레이션 콘텐츠가 카드가 되는 상한.
 * `store.list_content`는 LIMIT 없이 `score>=0`인 행을 전부 주고, 옛 「오늘의 배움」 컨테이너가
 * `items[0]` + `items.slice(1, 4)`로 4건만 보여줬다. 컨테이너를 해체하면서 그 상한까지
 * 같이 없애면 카드가 수십 장으로 불어난다 — 상한은 여기서 유지한다. 룰 finding은 대상이 아니다.
 *
 * ⑥에서 4 → 6. 로컬 공지라는 소스가 하나 늘어, 4를 유지하면 공지 2건이 배움 카드를 전부
 * 밀어낸다. **자르는 지점은 여전히 여기 한 곳뿐이다**(백엔드엔 LIMIT이 없다).
 * 고정 슬롯(최대 1건)은 이 상한 밖의 별도 칸이라 호출부가 미리 빼고 넘긴다. */
export const CONTENT_CARD_LIMIT = 6;

/** 「내 로그에서」로 가는 콘텐츠 = 개인 실전 레슨. 문법 A라 처분·수명 규칙을 함께 받는다. */
const isPersonal = (c: ContentItem) => c.trigger_tags?.includes('personal') ?? false;

/** 근거 출처로 두 섹션을 가른다 (스펙 §3).
 * 「내 로그에서」 = 룰 finding + `personal` 태그 콘텐츠. finding을 앞에 둔다 —
 * 처방·행동 버튼이 붙어 바로 실행 가능한 쪽이기 때문.
 * 상한은 score 순 상위 N건에 먼저 적용한 뒤 가른다(두 섹션 합계 기준 — 옛 컨테이너와 동일). */
export function partitionCoachItems(
  findings: CoachFinding[],
  content: ContentItem[],
): { log: LogCardView[]; learn: LearnCardView[] } {
  // 처분된 항목은 카드가 아니라 하단 접힌 줄이다 — 4건 상한의 자리도 잡아먹지 않게 먼저 거른다.
  // 탭 신규 배지·알림 집계도 같은 기준(백엔드 status='new')을 쓴다.
  const top = content.filter((c) => c.status === 'new').slice(0, CONTENT_CARD_LIMIT);
  return {
    log: [
      ...findings.filter((f) => f.status === 'new').map(toLogCardView),
      ...top.filter(isPersonal).map(toLessonCardView),
    ],
    learn: top.filter((c) => !isPersonal(c)).map(toLearnCardView),
  };
}

// ── 처분 줄 (스펙 §5) ─────────────────────────────────────────────────────
//
// 두 처분은 뜻이 다르므로 수명도 다르다.
//   해결함 = "조치했다" → 7일간 접힌 줄로 남아 되돌릴 수 있고, 재발하면 활성 복귀(백엔드 §5.1)
//   무시   = "알지만 지금은 안 한다" → 영구히 접힌 줄. 같은 묶음까지 침묵
// 표시 **위치**는 잠정이다 — 단일 스트림 재설계가 섹션을 없애며 다시 정한다. 여기 있는 것은
// "어떤 줄이 어떤 라벨로 보이는가"의 판정뿐이라 위치가 바뀌어도 그대로 쓰인다.

/** 「해결함」 접힌 줄이 남는 기간 — 실수로 눌렀을 때의 복구 창. 이후엔 화면에서만 사라지고
 * DB 행은 재발 감지를 위해 보존된다(지우면 룰이 다음 스캔에 같은 카드를 새로 만든다). */
export const RESOLVED_WINDOW_DAYS = 7;

const DISPOSED_LABEL: Record<string, string> = {
  resolved: '✔ 해결함',
  dismissed: '◷ 무시',
};

/** 처분 라벨. 처분이 아닌 상태(new·pending·rejected)는 null — 줄을 만들지 않는다. */
export function disposedLabel(status: string): string | null {
  return DISPOSED_LABEL[status] ?? null;
}

/** 처분 줄을 지금 보여줄지. 무시는 영구, 해결함은 7일.
 * 처분 시각을 모르면(마이그레이션 이전 처분 · 파싱 불가) **보이는 쪽**으로 기운다 —
 * 만료를 계산할 수 없다고 실행취소까지 빼앗지 않는다. */
export function isDisposedVisible(
  status: string,
  statusTs: string | null | undefined,
  nowMs: number,
): boolean {
  if (!isHiddenFinding(status)) return false;
  if (status === 'dismissed') return true;
  if (!statusTs) return true;
  const ts = Date.parse(statusTs);
  if (Number.isNaN(ts)) return true;
  return nowMs - ts < RESOLVED_WINDOW_DAYS * 24 * 60 * 60 * 1000;
}

export interface DisposedRow {
  key: string;
  /** 실행취소가 어느 커맨드로 가는지 — finding=set_finding_status, lesson=set_content_status */
  source: 'finding' | 'lesson';
  label: string;
  title: string;
  detail: string | null;
}

/** 룰 카드와 개인 레슨을 하나의 처분 줄 목록으로 합친다 — 둘 다 문법 A라 처분 모델이 같다.
 * finding을 앞에 두는 것은 활성 카드 정렬과 같은 규약. */
export function toDisposedRows(
  findings: CoachFinding[],
  content: ContentItem[],
  nowMs: number,
): DisposedRow[] {
  const rows: DisposedRow[] = [];
  for (const f of findings) {
    const label = disposedLabel(f.status);
    if (label === null || !isDisposedVisible(f.status, f.status_ts, nowMs)) continue;
    rows.push({
      key: f.dedup_key,
      source: 'finding',
      label,
      title: coachTitle(f.rule_id, f.evidence),
      detail: orNull(f.detail),
    });
  }
  for (const c of content) {
    // 수명 규칙은 문법 A 공통이다 — 배움·소식(문법 B)은 `✕` 하나뿐이라 처분 줄을 만들지 않는다.
    if (!isPersonal(c)) continue;
    const label = disposedLabel(c.status);
    if (label === null || !isDisposedVisible(c.status, c.status_ts, nowMs)) continue;
    rows.push({
      key: c.id,
      source: 'lesson',
      label,
      title: c.title,
      detail: splitLessonBody(c.body).evidence,
    });
  }
  return rows;
}
