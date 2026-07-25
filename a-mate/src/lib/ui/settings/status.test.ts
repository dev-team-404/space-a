import { describe, it, expect } from 'vitest';
import { IDLE, busy, ok, err } from './status';

describe('Status 생성자', () => {
  it('IDLE은 빈 텍스트 — StatusLine이 아무것도 렌더하지 않는 상태', () => {
    expect(IDLE).toEqual({ kind: 'idle', text: '' });
  });
  it('busy/ok는 kind와 문구를 그대로 담는다', () => {
    expect(busy('저장 중…')).toEqual({ kind: 'busy', text: '저장 중…' });
    expect(ok('저장했어요.')).toEqual({ kind: 'ok', text: '저장했어요.' });
  });
  it('err는 무엇이든 문자열로 — catch(e)의 e 타입이 unknown이라서', () => {
    expect(err(new Error('boom'))).toEqual({ kind: 'err', text: 'Error: boom' });
    expect(err('네트워크 오류')).toEqual({ kind: 'err', text: '네트워크 오류' });
    expect(err(404)).toEqual({ kind: 'err', text: '404' });
  });
});
