// 코칭 탭의 **한 줄 스트림** 조립 (스펙 §2.2·§2.3).
//
// `coach-helpers`가 "한 항목을 어떻게 그릴까"라면 여기는 "여러 항목을 어떤 순서로 몇 개나"다.
// 배지 키 계산은 CoachTab이 아니라 App.svelte(셸)가 쓰므로 소비자도 다르다.
import type { CoachFinding, ContentItem } from '../api';
import {
  CONTENT_CARD_LIMIT, contentKind, toLearnCardView, toLessonCardView, toLogCardView, validDeadline,
  type LearnCardView, type LogCardView,
} from './coach-helpers';

/** 스트림의 한 칸. 분류에 따라 문법 A(log) 또는 문법 B(learn) 뷰모델을 싣는다.
 * `key`는 **이름공간이 붙은** 키다 — Svelte `{#each}` 키와 배지 집합에 쓴다.
 * 카드의 `data-key`(딥링크용)는 접두사 없는 원래 키라서 서로 다르다. */
export type StreamCard =
  | { kind: 'coaching'; key: string; firstSeen: string | null; log: LogCardView }
  | { kind: 'learning' | 'news'; key: string; firstSeen: string | null; learn: LearnCardView };

/** `first_seen` 내림차순 비교자. 값이 없는 항목은 **맨 뒤**로 보낸다 —
 * 시각을 모르는 카드가 최신인 척하며 상단을 차지하면 상한에도 먼저 들어간다. */
export function compareFirstSeen(
  a: string | null | undefined,
  b: string | null | undefined,
): number {
  const x = a ?? '';
  const y = b ?? '';
  if (x === y) return 0;
  if (x === '') return 1;
  if (y === '') return -1;
  return x < y ? 1 : -1;
}

/** 활성 finding + 활성 콘텐츠를 한 줄로 조립한다.
 *
 * **상한은 콘텐츠에만, `first_seen` 기준으로** 적용한다(§4.2). score 기준으로 자르면
 * 자르는 축과 보여주는 축이 어긋나 "맨 위가 1시간 전인데 30분 전 항목이 없다"가 생긴다.
 * 룰 finding은 지금도 상한 대상이 아니다.
 *
 * 고정 슬롯 항목은 호출부가 **미리 빼고** 넘긴다 — 그 항목은 스트림 밖 별도 칸이고(§2.5),
 * `pinnedNewsItem`이 score 순 배열을 전제하므로 정렬보다 먼저 골라내야 한다. */
export function buildCoachStream(
  findings: CoachFinding[],
  content: ContentItem[],
): StreamCard[] {
  const capped = content
    .filter((c) => c.status === 'new')
    .slice()
    .sort((a, b) => compareFirstSeen(a.first_seen, b.first_seen) || a.id.localeCompare(b.id))
    .slice(0, CONTENT_CARD_LIMIT);

  const cards: StreamCard[] = [];
  for (const f of findings.filter((f) => f.status === 'new')) {
    cards.push({
      kind: 'coaching',
      key: `finding:${f.dedup_key}`,
      firstSeen: f.first_seen ?? null,
      log: toLogCardView(f),
    });
  }
  for (const c of capped) {
    const kind = contentKind(c);
    const key = `content:${c.id}`;
    const firstSeen = c.first_seen ?? null;
    if (kind === 'coaching') {
      cards.push({ kind, key, firstSeen, log: toLessonCardView(c) });
    } else {
      cards.push({ kind, key, firstSeen, learn: toLearnCardView(c) });
    }
  }
  return cards.sort(
    (a, b) => compareFirstSeen(a.firstSeen, b.firstSeen) || a.key.localeCompare(b.key),
  );
}

// ── 공지 수명 (§2.6) ───────────────────────────────────────────────────────
//
// 공지는 시한부인데 **데이터에 발행 시각이 없다** — GrowthBook 캐시에도
// `lastShownEmergencyTip`에도 날짜 필드가 없다. 우리가 가진 유일한 시각은
// `first_seen`("우리가 처음 본 때")이라 그것으로 잰다.

/** 기한 없는 일반 공지(프로모션·릴리스)가 화면에 남는 기간. */
export const ANNOUNCEMENT_TTL_DAYS = 7;
/** 기한 없는 장애 공지가 남는 기간. `lastShownEmergencyTip`은 「마지막으로 표시한」
 * 스냅샷이지 「지금 유효한」 값이 아니고 Claude Code가 그 키를 지우지 않는다 —
 * 해소된 장애가 영원히 최고 점수(priority 5 → 325)로 남는다. 만료 신호가 없으므로
 * "장애는 길어야 하루이틀"이라는 성질로 대신한다. */
export const EMERGENCY_TTL_DAYS = 2;

const DAY_MS = 24 * 60 * 60 * 1000;
/** Rust `announcements::TAG_ANNOUNCEMENT`·`TAG_NOTICE`와 짝이다. */
const TAG_ANNOUNCEMENT = 'announcement';
const TAG_NOTICE = 'notice';

const hasTag = (c: ContentItem, t: string) => c.trigger_tags?.includes(t) ?? false;

/** 이 공지를 아직 보여줄까. 공지가 아닌 항목은 언제나 `true`(이 규칙의 대상이 아니다).
 *
 * 기한이 **일수 규칙을 이긴다** — 사용자가 정한 것은 "명시한 기간 동안만"이다.
 * 기한을 모르거나 깨졌으면 일수로 떨어진다. 시각조차 모르면 보이는 쪽으로 기운다
 * (`isDisposedVisible`과 같은 규약 — 잴 수 없다고 카드를 뺏지 않는다). */
export function isAnnouncementLive(c: ContentItem, today: string, nowMs: number): boolean {
  if (!hasTag(c, TAG_ANNOUNCEMENT)) return true;
  const d = c.deadline?.trim();
  if (d && validDeadline(d)) return d >= today;
  if (!c.first_seen) return true;
  const ts = Date.parse(c.first_seen);
  if (Number.isNaN(ts)) return true;
  const ttl = hasTag(c, TAG_NOTICE) ? EMERGENCY_TTL_DAYS : ANNOUNCEMENT_TTL_DAYS;
  return nowMs - ts < ttl * DAY_MS;
}

/** 수명이 끝난 공지를 걷어낸다. **`status` 필터는 하지 않는다** — 백엔드
 * `listContent(false)`가 이미 걸렀고 스트림·배지가 각자 한 번 더 본다.
 * `filter`라 **순서가 보존**되므로 `pinnedNewsItem`이 전제하는 score 내림차순이 깨지지 않는다. */
export function liveContent(content: ContentItem[], today: string, nowMs: number): ContentItem[] {
  return content.filter((c) => isAnnouncementLive(c, today, nowMs));
}

/** 로컬 날짜 `YYYY-MM-DD`. `deadline`이 로컬 달력 날짜라 UTC로 비교하면 하루가 어긋난다. */
export function localDateString(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
