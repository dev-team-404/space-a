import { describe, expect, it } from 'vitest';

import { LENS_DEFAULT_SPACE, LENS_DEFAULT_URL, lensRoomUrl, lensSpaceLabel } from './lens';

describe('lensRoomUrl', () => {
  it('설정이 비어 있으면 팀 기본 주소·기본 공간으로 조립한다', () => {
    expect(lensRoomUrl({})).toBe(`${LENS_DEFAULT_URL}/#life/${LENS_DEFAULT_SPACE}`);
  });

  it('사람마다 다른 공간을 따른다 — 링크가 개인의 팀 방을 가리켜야 한다', () => {
    expect(lensRoomUrl({ knowledge_hub_space_id: 'sw-custom' })).toBe(
      `${LENS_DEFAULT_URL}/#life/sw-custom`,
    );
  });

  it('주소 끝 슬래시는 정리한다', () => {
    expect(lensRoomUrl({ a_lens_url: 'http://host:8600///' })).toBe(
      `http://host:8600/#life/${LENS_DEFAULT_SPACE}`,
    );
  });

  it('off면 링크를 숨긴다(빈 문자열)', () => {
    expect(lensRoomUrl({ a_lens_url: 'off' })).toBe('');
    expect(lensRoomUrl({ a_lens_url: 'OFF' })).toBe('');
  });

  it('http(s)가 아니면 죽은 버튼을 만들지 않는다', () => {
    expect(lensRoomUrl({ a_lens_url: '10.0.0.1:8600' })).toBe('');
    expect(lensRoomUrl({ a_lens_url: 'javascript:alert(1)' })).toBe('');
  });

  it('공간 이름에 특수문자가 있어도 URL로 안전하게 만든다', () => {
    expect(lensRoomUrl({ knowledge_hub_space_id: 'a b/c' })).toBe(
      `${LENS_DEFAULT_URL}/#life/a%20b%2Fc`,
    );
  });
});

describe('lensSpaceLabel', () => {
  it('설정한 공간을, 없으면 기본 공간을 보여준다', () => {
    expect(lensSpaceLabel({ knowledge_hub_space_id: 'sw-custom' })).toBe('sw-custom');
    expect(lensSpaceLabel({})).toBe(LENS_DEFAULT_SPACE);
  });
});
