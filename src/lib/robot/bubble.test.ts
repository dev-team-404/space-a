import { describe, expect, it } from 'vitest';
import { BubbleQueue, chatterBubble, diaryBubble, findingBubble, occasionBubble, scanBubble } from './bubble';

describe('BubbleQueue', () => {
  it('FIFO이고 최대 5건, 초과 시 oldest 드롭', () => {
    const q = new BubbleQueue();
    for (let i = 0; i < 7; i++) q.push(diaryBubble(`2026-07-0${i}`));
    expect(q.size).toBe(5);
    expect(q.next()!.text).toContain('2026-07-02'); // 0,1 드롭됨
  });
});

describe('bubble 팩토리', () => {
  it('finding: 최대 절약 1건 + 외 N건, coach 탭', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn' },
    ]);
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
  });
  it('diary는 diary 탭, occasion은 첫 라벨, chatter는 수치 삽입', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
    expect(chatterBubble(0, { session_count: 7 }).text).toContain('7');
    expect(chatterBubble(3, null).tab).toBe('home');
  });
  it('scanBubble — 수치 삽입·절약 후보 유무·null 폴백', () => {
    const b = scanBubble({ session_count: 5, est_tokens_saved_total: 12000 });
    expect(b.tab).toBe('home');
    expect(b.text).toContain('5세션');
    expect(b.text).toContain('12,000');
    expect(scanBubble({ session_count: 2, est_tokens_saved_total: 0 }).text).not.toContain('절약');
    expect(scanBubble(null).kind).toBe('scan');
  });
});
