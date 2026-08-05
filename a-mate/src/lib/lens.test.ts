import { describe, expect, it } from 'vitest';

import { LENS_DEFAULT_SPACE, lensRoomUrl, lensSpaceLabel } from './lens';

describe('lensRoomUrl', () => {
  it('주소를 설정하지 않으면 링크를 숨긴다', () => {
    expect(lensRoomUrl({})).toBe('');
  });

  it('공간만 설정해도 주소가 없으면 링크를 만들지 않는다', () => {
    expect(lensRoomUrl({ knowledge_hub_space_id: 'sw-custom' })).toBe('');
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
    expect(lensRoomUrl({ a_lens_url: 'https://lens.example', knowledge_hub_space_id: 'a b/c' })).toBe(
      'https://lens.example/#life/a%20b%2Fc',
    );
  });
});

describe('lensSpaceLabel', () => {
  it('설정한 공간을, 없으면 기본 공간을 보여준다', () => {
    expect(lensSpaceLabel({ knowledge_hub_space_id: 'sw-custom' })).toBe('sw-custom');
    expect(lensSpaceLabel({})).toBe(LENS_DEFAULT_SPACE);
  });
});
