import { describe, expect, it } from 'vitest';
import { adviceBubble, chatterCandidates, diaryBubble, findingBubble, occasionBubble, pickChatter } from './bubble';

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
  it('diary는 diary 탭, occasion은 첫 라벨', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
  });
});

describe('pickChatter', () => {
  it('rand 주입으로 결정적 — LLM 풀 항목이 후보에 포함된다', () => {
    const b = pickChatter(['풀A', '풀B'], null, [], () => 0);
    expect(b).toEqual({ kind: 'chatter', tab: 'home', text: '풀A' });
  });
  it('최근 표시분은 제외한다', () => {
    const b = pickChatter(['풀A', '풀B'], null, ['풀A'], () => 0);
    expect(b.text).toBe('풀B');
  });
  it('빈 풀이면 정적 후보만으로 pick — 수치 삽입 유지', () => {
    const b = pickChatter([], { session_count: 7 }, [], () => 0);
    expect(b.text).toContain('7'); // CHATTER[0]이 세션 수 삽입
  });
  it('전 후보가 recent면 recent를 무시한다(기아 방지)', () => {
    const all = chatterCandidates(['풀A'], null);
    const b = pickChatter(['풀A'], null, all, () => 0);
    expect(all).toContain(b.text);
  });
});
