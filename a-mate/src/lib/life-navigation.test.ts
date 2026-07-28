import { describe, expect, it } from 'vitest';
import { lifeDestinations } from './life-navigation';

const rooms = [
  { life_id: 'mine', owner_name: '준녕', occupants: 1 },
  { life_id: 'life-a', owner_name: '유연', occupants: 2 },
  { life_id: 'life-b', owner_name: '성문', occupants: 0 },
];

describe('lifeDestinations', () => {
  it('excludes my room and the current room while preserving server order', () => {
    expect(lifeDestinations(rooms, 'mine', 'mine')).toEqual([
      { lifeId: 'life-a', label: '유연의 방', occupants: 2, kind: 'life' },
      { lifeId: 'life-b', label: '성문의 방', occupants: 0, kind: 'life' },
    ]);
  });

  it('adds home first while visiting and excludes the visited room', () => {
    expect(lifeDestinations(rooms, 'mine', 'life-a')).toEqual([
      { lifeId: 'mine', label: '내 방으로 돌아가기', occupants: 1, kind: 'home' },
      { lifeId: 'life-b', label: '성문의 방', occupants: 0, kind: 'life' },
    ]);
  });

  it('returns no destinations until identity and position are known', () => {
    expect(lifeDestinations(rooms, '', 'mine')).toEqual([]);
    expect(lifeDestinations(rooms, 'mine', '')).toEqual([]);
  });
});
