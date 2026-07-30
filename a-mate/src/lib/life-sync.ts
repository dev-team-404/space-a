/** O1 — 대문 게시를 이 앱 실행 동안 포기할 오류인지 판정한다.
 *
 *  Rust `life_client::err_of`가 상태 코드를 `"(HTTP 404)"` 형태로 메시지에 담으므로 문자열로
 *  판정한다 (`commands.rs`의 `token_was_rejected`와 같은 방식).
 *
 *  **4xx = 서버가 이 요청을 받지 않는다** — 구서버(라우트 없음), 저장소 없는 인메모리 서버,
 *  거부된 토큰 등. 재시도해도 같은 답이 온다. 대문사진 게시는 방 폴링(2초)에 얹혀 재시도되므로
 *  이 판정이 없으면 거부된 PNG를 영구히 재전송한다.
 *  5xx·네트워크 오류는 일시적일 수 있으므로 재시도 대상으로 남긴다. */
export function isPermanentLifeError(error: unknown): boolean {
  const matched = /HTTP (\d{3})/.exec(String(error));
  return matched ? matched[1].startsWith('4') : false;
}
