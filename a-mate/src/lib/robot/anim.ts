import type { Expression } from './render';
import type { BubbleKind } from './bubble';

export type MascotState = 'idle' | 'talk' | 'happy' | 'alert' | 'sleep';
export type { BubbleKind };

export interface Frame {
  offsetY: number;
  expression: Expression;
  antennaBlink: boolean;
}

export function resolveState(input: { bubbleKind: BubbleKind | null; hour: number }): MascotState {
  switch (input.bubbleKind) {
    case 'finding': return 'alert';
    case 'diary':
    case 'occasion':
    case 'visit': return 'happy';
    case 'chatter': return 'talk';
    default: return input.hour >= 1 && input.hour < 7 ? 'sleep' : 'idle';
  }
}

/** Deterministic frame calculation -- render loop only passes t. */
export function frameAt(state: MascotState, tMs: number): Frame {
  const bounce = Math.round(Math.sin((tMs / 1600) * Math.PI * 2)); // -1..1
  switch (state) {
    case 'sleep':
      return { offsetY: 0, expression: 'sleep', antennaBlink: false };
    case 'alert':
      return { offsetY: bounce, expression: 'normal', antennaBlink: Math.floor(tMs / 250) % 2 === 0 };
    case 'happy':
      return { offsetY: Math.floor(tMs / 200) % 2 === 0 ? -2 : 0, expression: 'happy', antennaBlink: false };
    case 'talk': {
      const expression: Expression = Math.floor(tMs / 350) % 2 === 0 ? 'blink' : 'normal';
      return { offsetY: bounce, expression, antennaBlink: false };
    }
    default: {
      // idle: blink for 200ms in each 4s cycle
      const expression: Expression = tMs % 4000 < 200 ? 'blink' : 'normal';
      return { offsetY: bounce, expression, antennaBlink: false };
    }
  }
}
