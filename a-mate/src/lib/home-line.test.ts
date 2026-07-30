import { describe, expect, it } from 'vitest';
import { resolveHomeLine } from './home-line';

describe('resolveHomeLine', () => {
  it('내 방에서는 컷 캡션이 오늘의 한마디를 이긴다', () => {
    expect(resolveHomeLine({
      visiting: false, ownerLine: '', cutCaption: '밤샘 끝, 뿌듯', dailyLine: '월요일부터 달린다',
    })).toBe('밤샘 끝, 뿌듯');
  });

  it('컷 캡션이 없으면 오늘의 한마디를 쓴다', () => {
    expect(resolveHomeLine({
      visiting: false, ownerLine: '', cutCaption: null, dailyLine: '월요일부터 달린다',
    })).toBe('월요일부터 달린다');
  });

  it('내 방에서 둘 다 없으면 null (카드를 그리지 않는다)', () => {
    expect(resolveHomeLine({ visiting: false, ownerLine: '', cutCaption: null, dailyLine: null })).toBeNull();
    expect(resolveHomeLine({ visiting: false, ownerLine: '', cutCaption: '', dailyLine: '  ' })).toBeNull();
  });

  it('방문 중에는 주인 문장을 쓴다', () => {
    expect(resolveHomeLine({
      visiting: true, ownerLine: '주인이 쓴 한마디', cutCaption: null, dailyLine: null,
    })).toBe('주인이 쓴 한마디');
  });

  it('방문 중에는 내 캡션·한마디를 무시한다 (잔상 방지)', () => {
    expect(resolveHomeLine({
      visiting: true, ownerLine: '', cutCaption: '내 캡션', dailyLine: '내 한마디',
    })).toBeNull();
  });

  it('방문 중 주인 문장이 공백뿐이면 null', () => {
    expect(resolveHomeLine({ visiting: true, ownerLine: '   ', cutCaption: null, dailyLine: null })).toBeNull();
  });
});
