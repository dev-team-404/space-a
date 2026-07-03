export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter';

export interface Bubble {
  kind: BubbleKind;
  text: string;
  tab: 'home' | 'diary' | 'coach';
}

const MAX_QUEUE = 5;

export class BubbleQueue {
  private items: Bubble[] = [];
  push(b: Bubble): void {
    this.items.push(b);
    if (this.items.length > MAX_QUEUE) this.items.shift();
  }
  next(): Bubble | null {
    return this.items.shift() ?? null;
  }
  get size(): number {
    return this.items.length;
  }
}

const RULE_LINE: Record<string, string> = {
  R1: '안 쓰는 MCP가 상주 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '같은 파일을 반복해서 읽고 있어요',
  R7: '단순 작업에 Opus는 과해요',
  R9: '웹 검색을 너무 많이 돌렸어요',
};

export function findingBubble(
  rows: { rule_id: string; est_tokens_saved: number; severity: string }[],
): Bubble {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  const line = RULE_LINE[top.rule_id] ?? '아낄 수 있는 게 보여요';
  const more = rows.length > 1 ? ` 외 ${rows.length - 1}건` : '';
  return {
    kind: 'finding',
    tab: 'coach',
    text: `주인, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`,
  };
}

export function diaryBubble(date: string): Bubble {
  return { kind: 'diary', tab: 'diary', text: `${date} 일기 다 썼어요! 보러 올래요?` };
}

export function occasionBubble(labels: string[]): Bubble {
  return { kind: 'occasion', tab: 'home', text: `오늘 ${labels[0]}이래요! 🎉` };
}

const CHATTER: ((n: number | null) => string)[] = [
  (n) => (n === null ? '오늘도 화이팅이에요, 주인!' : `오늘 벌써 ${n}세션이나 돌렸어요`),
  () => '토큰은 아끼라고 있는 거예요',
  () => '스킬로 만들면 편할 텐데…',
  () => '주인, 물 한 잔 마시고 해요',
  (n) => (n === null ? '심심해요…' : `${n}세션째… 저 좀 굴리는데요?`),
  () => '캐시 히트가 곧 절약이에요',
  () => '커밋은 자주, 후회는 짧게',
  () => '오늘 일기 기대해 주세요',
  () => '레지스트리에 새 스킬 구경 갈까요',
  () => 'zzz… 아 깨어있어요!',
];

export function chatterBubble(pick: number, summary: { session_count: number } | null): Bubble {
  const f = CHATTER[((pick % CHATTER.length) + CHATTER.length) % CHATTER.length];
  return { kind: 'chatter', tab: 'home', text: f(summary?.session_count ?? null) };
}
