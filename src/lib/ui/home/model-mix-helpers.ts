import type { ModelMixEntry } from '../../api';

export interface MixSlice {
  tier: string;
  tokens: number;
  pct: number; // 정확한 비율(%) — 표시 반올림은 pctLabel/toFixed 몫
}

export interface ShapedMix {
  top: MixSlice[];
  other: { tokens: number; pct: number; items: MixSlice[] } | null;
}

export const TOP_N = 3;

/** 내림차순 mix를 상위 TOP_N + 기타(나머지 합산)로 가공. legend 줄 수 고정용. */
export function shapeMix(mix: ModelMixEntry[], topN = TOP_N): ShapedMix {
  const total = mix.reduce((a, m) => a + m.tokens, 0);
  if (total <= 0) return { top: [], other: null };
  const slice = (m: ModelMixEntry): MixSlice => ({ tier: m.tier, tokens: m.tokens, pct: (m.tokens / total) * 100 });
  const top = mix.slice(0, topN).map(slice);
  const rest = mix.slice(topN).map(slice);
  if (rest.length === 0) return { top, other: null };
  const tokens = rest.reduce((a, m) => a + m.tokens, 0);
  return { top, other: { tokens, pct: (tokens / total) * 100, items: rest } };
}

/** 범례 % 라벨 — 반올림 정수, 1% 미만은 "0%" 대신 "<1%". */
export function pctLabel(pct: number): string {
  return pct < 1 ? '<1%' : `${Math.round(pct)}%`;
}
