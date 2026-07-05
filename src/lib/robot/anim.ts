import { EYES_BLINK, EYES_HAPPY, EYES_SLEEP, type Px } from './parts';
import type { BubbleKind } from './bubble';

export type MascotState = 'idle' | 'talk' | 'happy' | 'alert' | 'sleep';
export type { BubbleKind };

export interface Frame {
  offsetY: number;
  eyesOverride?: Px[];
  antennaBlink: boolean;
}

export function resolveState(input: { bubbleKind: BubbleKind | null; hour: number }): MascotState {
  switch (input.bubbleKind) {
    case 'finding': return 'alert';
    case 'diary':
    case 'occasion': return 'happy';
    case 'chatter':
    case 'scan': return 'talk';
    default: return input.hour >= 1 && input.hour < 7 ? 'sleep' : 'idle';
  }
}

/** 결정적 프레임 계산 — 렌더 루프가 t만 넘긴다. */
export function frameAt(state: MascotState, tMs: number): Frame {
  const bounce = Math.round(Math.sin((tMs / 1600) * Math.PI * 2)); // -1..1
  switch (state) {
    case 'sleep':
      return { offsetY: 0, eyesOverride: EYES_SLEEP, antennaBlink: false };
    case 'alert':
      return { offsetY: bounce, antennaBlink: Math.floor(tMs / 250) % 2 === 0 };
    case 'happy':
      return { offsetY: Math.floor(tMs / 200) % 2 === 0 ? -2 : 0, eyesOverride: EYES_HAPPY, antennaBlink: false };
    case 'talk':
      return { offsetY: bounce, ...(Math.floor(tMs / 350) % 2 === 0 ? {} : { eyesOverride: EYES_BLINK }), antennaBlink: false };
    default: {
      // idle: 4초 주기 중 200ms 깜빡임
      const blink = tMs % 4000 < 200;
      return { offsetY: bounce, ...(blink ? { eyesOverride: EYES_BLINK } : {}), antennaBlink: false };
    }
  }
}
