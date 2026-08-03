// 코칭 탭의 **한 줄 스트림** 조립 (스펙 §2.2·§2.3).
//
// `coach-helpers`가 "한 항목을 어떻게 그릴까"라면 여기는 "여러 항목을 어떤 순서로 몇 개나"다.
// 배지 키 계산은 CoachTab이 아니라 App.svelte(셸)가 쓰므로 소비자도 다르다.
import type { CoachFinding, ContentItem } from '../api';
import {
  CONTENT_CARD_LIMIT, contentKind, toLearnCardView, toLessonCardView, toLogCardView,
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
