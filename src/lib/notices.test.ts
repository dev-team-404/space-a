import { describe, expect, it } from 'vitest';
import { pushNotice, type Notice } from './notices';

const n = (text: string): Notice => ({ ts: '2026-07-05T10:00:00Z', kind: 'finding', text });

describe('pushNotice', () => {
  it('최신이 앞, 최대 20건 유지', () => {
    let list: Notice[] = [];
    for (let i = 0; i < 25; i++) list = pushNotice(list, n(`알림 ${i}`));
    expect(list.length).toBe(20);
    expect(list[0].text).toBe('알림 24');
    expect(list[19].text).toBe('알림 5');
  });
  it('원본 배열을 변형하지 않는다', () => {
    const orig: Notice[] = [n('a')];
    const next = pushNotice(orig, n('b'));
    expect(orig.length).toBe(1);
    expect(next[0].text).toBe('b');
  });
});
