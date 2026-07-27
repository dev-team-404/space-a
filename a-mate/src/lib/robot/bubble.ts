export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter';

export interface Bubble {
  kind: BubbleKind;
  text: string;
  tab: 'home' | 'diary' | 'coach';
  /** 딥링크 대상 — tab 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD). 없으면 탭 이동만. */
  target?: string;
}

const RULE_LINE: Record<string, string> = {
  R1: '안 쓰는 MCP가 상주 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '같은 파일을 반복해서 읽고 있어요',
  R7: '단순 작업에 Opus는 과해요',
  R9: '웹 검색을 너무 많이 돌렸어요',
};

export function findingBubble(
  rows: { rule_id: string; est_tokens_saved: number; severity: string; dedup_key: string }[],
  honorific: string,
): Bubble {
  const top = [...rows].sort((a, b) => b.est_tokens_saved - a.est_tokens_saved)[0];
  const line = RULE_LINE[top.rule_id] ?? '아낄 수 있는 게 보여요';
  const more = rows.length > 1 ? ` 외 ${rows.length - 1}건` : '';
  return {
    kind: 'finding',
    tab: 'coach',
    target: top.dedup_key,
    text: `${honorific}, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`,
  };
}

export function diaryBubble(date: string): Bubble {
  return { kind: 'diary', tab: 'diary', target: date, text: `${date} 일기 다 썼어요! 보러 올래요?` };
}

export function occasionBubble(labels: string[]): Bubble {
  return { kind: 'occasion', tab: 'home', text: `오늘 ${labels[0]}이래요! 🎉` };
}

const CHATTER: ((n: number | null, h: string) => string)[] = [
  (n, h) => (n === null ? `오늘도 화이팅이에요, ${h}!` : `오늘 벌써 ${n}세션이나 돌렸어요`),
  () => '토큰은 아끼라고 있는 거예요',
  () => '스킬로 만들면 편할 텐데…',
  (_n, h) => `${h}, 물 한 잔 마시고 해요`,
  (n) => (n === null ? '심심해요…' : `${n}세션째… 저 좀 굴리는데요?`),
  () => '캐시 히트가 곧 절약이에요',
  () => '커밋은 자주, 후회는 짧게',
  () => '오늘 일기 기대해 주세요',
  () => '레지스트리에 새 스킬 구경 갈까요',
  () => 'zzz… 아 깨어있어요!',
  (_n, h) => `${h}, 오늘도 제가 응원해요. 조용히, 근데 진심으로`,
  () => '막히면 잠깐 산책 — 코드는 도망 안 가요',
  () => '어제보다 한 커밋만 더. 그게 성장이에요',
  (n, h) => (n === null ? `오늘의 ${h}도 응원합니다!` : `${n}세션째 달리는 ${h}, 존경해요`),
  () => '실패한 시도도 데이터예요. 제가 다 보고 있었어요',
];

/** realtime_advice 옵트인: 스캔 후 최상위 활성 advice를 말풍선으로 (스펙 §6).
 *  target(dedup_key)은 코칭 카드 딥링크 대상 — 같은 조언 반복 방지는 호출측이 dedup_key로 수행. */
export function adviceBubble(f: { dedup_key: string; detail: string }, honorific: string): Bubble {
  return { kind: 'finding', tab: 'coach', text: `${honorific}, ${f.detail}`, target: f.dedup_key };
}

/** 잡담 후보 — LLM 풀(사용기록 연계)이 있으면 풀에서만 pick, 비면 정적 큐레이션 폴백.
 *  빈 풀 판정은 백엔드 몫 — 오프라인/엔진 미설정/활동 0건/캐시 지문 불일치(당일 중단·실패). */
export function chatterCandidates(
  pool: string[],
  summary: { session_count: number } | null,
  honorific: string,
): string[] {
  return pool.length ? [...pool] : CHATTER.map((f) => f(summary?.session_count ?? null, honorific));
}

/** 잡담 pick — 후보에서 최근 표시분(recent)을 제외하고 균등 랜덤.
 *  제외 후 후보가 비면 recent를 무시하고 전체에서 pick(기아 방지). rand는 [0,1) 주입. */
export function pickChatter(
  pool: string[],
  summary: { session_count: number } | null,
  recent: string[],
  rand: () => number,
  honorific: string,
): Bubble {
  const all = chatterCandidates(pool, summary, honorific);
  const fresh = all.filter((t) => !recent.includes(t));
  const candidates = fresh.length ? fresh : all;
  return { kind: 'chatter', tab: 'home', text: candidates[Math.floor(rand() * candidates.length)] };
}
