import type { LifeMe, LifeState } from '../api';

export interface LifeViewPayload {
  me: LifeMe;
  life: LifeState;
}

export interface LifeRenderSignatures {
  me: string;
  meta: string;
  design: string;
  occupants: string;
}

export function lifeRenderSignatures(view: LifeViewPayload): LifeRenderSignatures {
  const { design, occupants, ...meta } = view.life;
  return {
    me: JSON.stringify(view.me),
    meta: JSON.stringify(meta),
    design: JSON.stringify(design),
    occupants: JSON.stringify(occupants),
  };
}
