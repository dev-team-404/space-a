import { describe, expect, it } from 'vitest';
import { isPermanentLifeError } from './life-sync';

describe('isPermanentLifeError', () => {
  it('구서버(라우트 없음)는 영구 거부로 본다', () => {
    // Rust err_of가 만드는 실제 형태
    expect(isPermanentLifeError('http_error:  (HTTP 404)')).toBe(true);
    expect(isPermanentLifeError('http_error:  (HTTP 405)')).toBe(true);
  });

  it('요청을 받지 않는 서버 상태(저장소 없음 등)도 영구 거부', () => {
    expect(isPermanentLifeError('invalid_request: daily cut storage is unavailable (HTTP 400)')).toBe(true);
    expect(isPermanentLifeError('unauthorized: invalid or missing token (HTTP 401)')).toBe(true);
  });

  it('서버 장애는 일시적이라 재시도한다', () => {
    expect(isPermanentLifeError('http_error:  (HTTP 500)')).toBe(false);
    expect(isPermanentLifeError('http_error:  (HTTP 503)')).toBe(false);
  });

  it('네트워크 오류는 코드가 없으므로 일시적으로 본다', () => {
    expect(isPermanentLifeError('hub 연결 실패: Transport(Transport)')).toBe(false);
    expect(isPermanentLifeError('store lock poisoned')).toBe(false);
  });

  it('문자열이 아닌 값도 안전하게 처리한다', () => {
    expect(isPermanentLifeError(undefined)).toBe(false);
    expect(isPermanentLifeError(null)).toBe(false);
    expect(isPermanentLifeError(new Error('boom (HTTP 404)'))).toBe(true);
  });
});
