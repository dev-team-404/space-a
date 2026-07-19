# a-mate: room server x-api-key 지원 — 설계

- 날짜: 2026-07-19
- 상태: 승인됨 (구현 대기)

## 배경

`a-hub/life` room server에 `x-api-key` 관문(`ROOM_SERVER_API_KEY`)을 추가하고 OCI VM에
배포했다. 관문이 켜진 서버는 `/healthz`·`/readyz`·`/docs`·`/openapi.json`를 제외한 모든
요청에 `x-api-key` 헤더를 요구한다.

room server의 실제 클라이언트는 **a-mate**(Agent Mentor, Tauri 데스크톱)다:

- `crates/core/src/rooms_client.rs` — HTTP 래퍼 (`RoomsClient` + 자유 함수 `register`/`list_rooms`)
- `src-tauri/src/commands.rs` — Tauri 커맨드가 설정을 저장하고 `RoomsClient`를 만든다
- `src/Settings.svelte` — "Space A 서버" 섹션에서 URL·이름 입력

현재 a-mate는 `Authorization: Bearer <token>`만 보내고 `x-api-key`는 전혀 보내지 않는다.
따라서 관문이 켜진 VM(`http://158.179.194.42:8001`)에 붙으면 모든 요청이 401이 된다.

## 목표

a-mate가 관문이 켜진 room server에 연결할 수 있도록 `x-api-key` 헤더 전송을 지원한다.
키가 비어 있으면(관문 없는 서버) 지금과 동일하게 동작한다(하위호환).

## 설계

x-api-key는 등록으로 얻는 `token`과 성격이 다르다 — **base_url과 짝을 이루는 서버 접속
정보**다. 따라서 base_url처럼 클라이언트 전반에 흐르게 하고, 별도 설정 키로 저장한다.

### 1. `rooms_client.rs`

- `RoomsClient`에 `api_key: Option<String>` 필드 추가.
- `req()`에서 `api_key`가 `Some`이고 공백이 아니면 `.set("x-api-key", key)` 첨부.
- 자유 함수 시그니처 확장:
  - `register(base_url, name, seed)` → `register(base_url, api_key, name, seed)`
  - `list_rooms(base_url)` → `list_rooms(base_url, api_key)`
  - 내부 공통 헬퍼로 "요청에 x-api-key를 조건부 첨부"를 한 곳에 둔다.
- 빈 문자열/None이면 헤더 미첨부.

### 2. `commands.rs`

- 설정 키 `hub_api_key` 추가 (기존 `hub_url`·`hub_user`·`hub_token`… 옆에).
- `hub_client()`: `RoomsClient { base_url, token, api_key: Some(get("hub_api_key")) }`.
- `hub_connect(url, user)` → `hub_connect(url, user, api_key)`:
  - `hub_api_key`를 설정에 저장.
  - `register`/`rename` 호출에 api_key 전달.
- `HubSettings`에 `api_key: String` 필드 추가, `hub_settings_get`이 반환.

### 3. `Settings.svelte`

- `HubSettings` 인터페이스에 `api_key: string`, 상태 `hubApiKey` 추가.
- "Space A 서버" 섹션에 API 키 입력 필드 추가 — LLM 엔진의 key 필드와 동일하게
  `type="password"`, placeholder "선택 — 비우면 인증 없이".
- `hub_connect` 호출에 `apiKey: hubApiKey` 전달.

## 에러 처리

`err_of()`가 이미 `{error:{code,message}}`를 파싱한다. 잘못된/누락된 키는
`unauthorized: invalid or missing x-api-key (HTTP 401)`로 표시되므로 추가 처리 불필요.

## 테스트

- `rooms_client.rs` 단위 테스트: `RoomsClient`가 x-api-key를 조건부로 첨부하는지 검증
  (키 있을 때 첨부 / 빈 키일 때 미첨부). 네트워크 없이 헤더 조립 로직을 검증할 수 있는
  범위로 한정한다.

## 하위호환

관문 없는 서버(로컬 `docker compose` 기본, `ROOM_SERVER_API_KEY` 미설정)는 키를 비워두면
헤더가 붙지 않아 현재와 100% 동일하게 동작한다.

## 범위 밖 (YAGNI)

- `.env` 기반 hub 설정 — hub는 현재 `.env` 경로가 없다. LLM 엔진만 `.env` 폴백을 가진다.
- 키 회전·만료 등 관리 기능. 고정 공유키 관문의 테스트 용도에 맞춘다.
