import { describe, expect, it } from 'vitest';
import { adviceBubble, chatterCandidates, diaryBubble, findingBubble, occasionBubble, pickChatter } from './bubble';

describe('bubble 팩토리', () => {
  it('finding: 최대 절약 1건 + 외 N건, coach 탭, top dedup_key가 target', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest', dedup_key: 'k-r5' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn', dedup_key: 'k-r1' },
    ], '주인');
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
    expect(b.target).toBe('k-r1');
  });
  it('advice: detail을 싣고 dedup_key를 딥링크 target으로', () => {
    const b = adviceBubble({ dedup_key: 'k1', detail: '`playwright`가 상주하는데 호출 0회' }, '주인');
    expect(b.kind).toBe('finding');
    expect(b.tab).toBe('coach');
    expect(b.target).toBe('k1');
    expect(b.text).toContain('playwright');
  });
  it('커스텀 호칭이 finding·advice·chatter 대사에 반영된다', () => {
    const f = findingBubble([{ rule_id: 'R1', est_tokens_saved: 100, severity: 'warn', dedup_key: 'k' }], '대장');
    expect(f.text.startsWith('대장,')).toBe(true);
    const a = adviceBubble({ dedup_key: 'k', detail: '뭐가 있어요' }, '대장');
    expect(a.text.startsWith('대장,')).toBe(true);
    // 세션 null일 때 CHATTER[0]이 호칭을 부른다
    expect(pickChatter([], null, [], () => 0, '대장').text).toContain('대장');
  });
  it('diary는 diary 탭 + 날짜가 target, occasion은 첫 라벨', () => {
    const d = diaryBubble('2026-07-02');
    expect(d.tab).toBe('diary');
    expect(d.target).toBe('2026-07-02');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
  });
  it('occasion·chatter는 target 없음(탭 이동만)', () => {
    expect(occasionBubble(['크리스마스']).target).toBeUndefined();
    expect(pickChatter(['풀A'], null, [], () => 0, '주인').target).toBeUndefined();
  });
});

describe('pickChatter', () => {
  it('rand 주입으로 결정적 — LLM 풀 항목이 후보에 포함된다', () => {
    const b = pickChatter(['풀A', '풀B'], null, [], () => 0, '주인');
    expect(b).toEqual({ kind: 'chatter', tab: 'home', text: '풀A' });
  });
  it('최근 표시분은 제외한다', () => {
    const b = pickChatter(['풀A', '풀B'], null, ['풀A'], () => 0, '주인');
    expect(b.text).toBe('풀B');
  });
  it('빈 풀이면 정적 후보만으로 pick — 수치 삽입 유지', () => {
    const b = pickChatter([], { session_count: 7 }, [], () => 0, '주인');
    expect(b.text).toContain('7'); // CHATTER[0]이 세션 수 삽입
  });
  it('전 후보가 recent면 recent를 무시한다(기아 방지)', () => {
    const all = chatterCandidates(['풀A'], null, '주인');
    const b = pickChatter(['풀A'], null, all, () => 0, '주인');
    expect(all).toContain(b.text);
  });
});
