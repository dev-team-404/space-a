import { describe, expect, it } from 'vitest';
import { adviceBubble, chatterBubble, diaryBubble, findingBubble, occasionBubble } from './bubble';

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
  it('advice: detail을 싣고 dedup_key를 반복 방지 키로', () => {
    const b = adviceBubble({ dedup_key: 'k1', detail: '`playwright`가 상주하는데 호출 0회' });
    expect(b.kind).toBe('finding');
    expect(b.tab).toBe('coach');
    expect(b.key).toBe('k1');
    expect(b.text).toContain('playwright');
  });
  it('diary는 diary 탭, occasion은 첫 라벨, chatter는 수치 삽입', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
    expect(chatterBubble(0, { session_count: 7 }).text).toContain('7');
    expect(chatterBubble(3, null).tab).toBe('home');
  });
});
