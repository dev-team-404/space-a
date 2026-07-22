import { describe, expect, it } from 'vitest';
import { lifeRenderSignatures, type LifeViewPayload } from './life-render-state';

function view(): LifeViewPayload {
  return {
    me: { agent_id: 'a', name: 'A', my_life_id: 'life-a', life_id: 'life-b', cell: [1, 2] },
    life: {
      life_id: 'life-b', owner_agent_id: 'b', owner_name: 'B', owner_mascot_seed: 'b', grid: { w: 24, h: 20 },
      design: { wallpaper: 'w1', floor: 'f1', objects: [] },
      occupants: [{ agent_id: 'a', name: 'A', cell: [1, 2], is_owner: false, mascot_seed: 'a' }],
    },
  };
}

describe('lifeRenderSignatures', () => {
  it('keeps the design signature stable when only an occupant moves', () => {
    const before = view();
    const after = view();
    after.life.occupants[0].cell = [2, 3];
    const a = lifeRenderSignatures(before);
    const b = lifeRenderSignatures(after);
    expect(b.design).toBe(a.design);
    expect(b.occupants).not.toBe(a.occupants);
  });

  it('keeps the occupant signature stable when the owner changes the design', () => {
    const before = view();
    const after = view();
    after.life.design.wallpaper = 'w2';
    const a = lifeRenderSignatures(before);
    const b = lifeRenderSignatures(after);
    expect(b.design).not.toBe(a.design);
    expect(b.occupants).toBe(a.occupants);
  });
});
