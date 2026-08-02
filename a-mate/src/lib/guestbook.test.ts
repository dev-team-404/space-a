import { describe, expect, it } from 'vitest';
import { authorHue, authorIcon, authorInitial, autoVisitEnabled, groupGuestbook, visitGuestbookEnabled } from './guestbook';
import type { GuestbookEntry } from './api';

const e = (entry_id: string, parent_id: string | null = null): GuestbookEntry => ({
  entry_id, life_id: 'l1', author_agent_id: 'a1', author_name: '이름',
  body: '내용', parent_id, created_at: '2026-07-26T00:00:00Z',
});

describe('groupGuestbook', () => {
  it('원글은 서버 순서(최신순) 유지, 답글은 오래된 순으로 부모 아래 묶인다', () => {
    // 서버 응답은 최신순 — gb_r2가 gb_r1보다 최신
    const entries = [e('gb_r2', 'gb_p1'), e('gb_p2'), e('gb_r1', 'gb_p1'), e('gb_p1')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_p2', 'gb_p1']);
    expect(threads[1].replies.map((r) => r.entry_id)).toEqual(['gb_r1', 'gb_r2']);
  });

  it('parent_id 없는 레거시 행은 전부 top-level', () => {
    const entries = [e('gb_b'), e('gb_a')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_b', 'gb_a']);
    expect(threads.every((t) => t.replies.length === 0)).toBe(true);
  });

  it('부모가 목록에 없는 답글은 top-level로 폴백', () => {
    const threads = groupGuestbook([e('gb_orphan', 'gb_gone')]);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_orphan']);
    expect(threads[0].replies).toEqual([]);
  });
});

describe('authorIcon', () => {
  const mk = (
    author_kind: 'human' | 'bot' | null | undefined,
    author_name = '홍길동',
  ): GuestbookEntry => ({
    entry_id: 'x', life_id: 'l1', author_agent_id: 'agent-1', author_name,
    body: 'b', author_kind, created_at: '2026-08-02T00:00:00Z',
  });

  it("author_kind='bot'이면 봇 아이콘 — agent_id로 서버 얼굴을 찾는다", () => {
    expect(authorIcon(mk('bot'))).toEqual({ kind: 'bot', agentId: 'agent-1' });
  });

  it("'human'과 구데이터(null·미제공)는 모두 모노그램 (서버 규약: 미제공=human 간주)", () => {
    expect(authorIcon(mk('human')).kind).toBe('monogram');
    expect(authorIcon(mk(null)).kind).toBe('monogram');
    expect(authorIcon(mk(undefined)).kind).toBe('monogram');
  });

  it('모노그램은 이름에서 글자와 색을 뽑는다', () => {
    expect(authorIcon(mk('human', '김영희'))).toEqual({
      kind: 'monogram', initial: '김', hue: authorHue('김영희'),
    });
  });

  it('관찰자 인자를 받지 않는다 — 같은 엔트리는 누가 보든 같은 아이콘 (스펙 목표 2)', () => {
    const entry = mk('human');
    expect(authorIcon(entry)).toEqual(authorIcon(entry));
    // 관찰자(meId·isOwner 등)를 인자로 추가하면 이 단언이 깨진다 — 회귀 고정
    expect(authorIcon.length).toBe(1);
  });
});

describe('authorInitial', () => {
  it('한글·영문 첫 글자', () => {
    expect(authorInitial('홍길동')).toBe('홍');
    expect(authorInitial('John Doe')).toBe('J');
  });

  it('이모지는 서로게이트 페어를 쪼개지 않는다', () => {
    expect(authorInitial('🤖봇')).toBe('🤖');
  });

  it('앞뒤 공백은 무시', () => {
    expect(authorInitial('  김철수 ')).toBe('김');
  });

  it('빈 이름·공백뿐이면 물음표', () => {
    expect(authorInitial('')).toBe('?');
    expect(authorInitial('   ')).toBe('?');
  });
});

describe('authorHue', () => {
  it('같은 이름은 항상 같은 hue — 모든 관찰자가 같은 색을 본다', () => {
    expect(authorHue('홍길동')).toBe(authorHue('홍길동'));
  });

  it('다른 이름은 다른 hue', () => {
    expect(authorHue('홍길동')).not.toBe(authorHue('김영희'));
  });

  it('어떤 입력에도 0~359 범위', () => {
    for (const name of ['홍길동', 'John', '', '🤖', 'a'.repeat(80)]) {
      expect(authorHue(name)).toBeGreaterThanOrEqual(0);
      expect(authorHue(name)).toBeLessThan(360);
    }
  });
});

describe('visitGuestbookEnabled', () => {
  it("기본 on — 미설정/빈값/true 전부 켜짐, 'false'만 꺼짐 (러스트 글루와 동일 규칙)", () => {
    expect(visitGuestbookEnabled({})).toBe(true);
    expect(visitGuestbookEnabled({ visit_guestbook_enabled: '' })).toBe(true);
    expect(visitGuestbookEnabled({ visit_guestbook_enabled: 'true' })).toBe(true);
    expect(visitGuestbookEnabled({ visit_guestbook_enabled: 'false' })).toBe(false);
  });
});

describe('autoVisitEnabled', () => {
  it("기본 on — 미설정/빈값/true 전부 켜짐, 'false'만 꺼짐 (러스트 글루와 동일 규칙)", () => {
    expect(autoVisitEnabled({})).toBe(true);
    expect(autoVisitEnabled({ auto_visit_enabled: '' })).toBe(true);
    expect(autoVisitEnabled({ auto_visit_enabled: 'true' })).toBe(true);
    expect(autoVisitEnabled({ auto_visit_enabled: 'false' })).toBe(false);
  });
});
