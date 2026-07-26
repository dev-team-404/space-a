---
status: done
archived: 2026-07-26
---

# 캐릭터 이미지 엔드포인트 "연결 테스트" — 설계

- **날짜**: 2026-07-26
- **상태**: 승인 대기 (브레인스토밍 완료)
- **컴포넌트**: a-mate (`crates/core/src/sprite.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`, `src/lib/ui/settings/ConnectionGroup.svelte`)
- **결정 요지**: 설정 → 연결 → "캐릭터 이미지" 섹션에 **무과금 "연결 테스트" 버튼**을 추가한다. 저장·재생성 전에 URL·키·모델 오설정을 사람이 읽는 메시지로 잡아내는 피드백 루프. 위 "LLM 엔진"의 `연결 테스트`(`engine_test`)를 그대로 미러링한다.
- **관련**: PR #103 [`fix/sprite-parse-image-from-content`](../../../../a-mate/crates/core/src/sprite.rs) (이미지 파서 — 이 브랜치 위에서 분기), 후속 PR2 = 나 탭 재생성 IA 개편(별도 브레인스토밍)

## 1. 배경 — 관측된 문제

직전 세션에서 사용자가 설정한 이미지 엔드포인트로 `캐릭터 재생성`이 계속 실패했다. 원인은 두 겹:

1. **오설정** — 이미지 URL 칸에 텍스트 엔진 URL을 넣고, API 키를 `ssk-`로 오타냈다.
2. **파서 버그** — 앱이 OpenRouter 전용 응답 포맷만 읽어 로컬 LiteLLM의 정상 응답을 못 알아봤다. → **PR #103에서 `extract_image_bytes` 관용적 파서로 이미 해결.**

핵심 교훈: 텍스트 엔진에는 "연결 테스트" 버튼이 있는데 **이미지 엔드포인트에는 검증 수단이 전혀 없어서**, 사용자가 뭘 잘못 넣었는지 알 방법이 없었다. 이 스펙은 그 공백을 메운다.

## 2. 실측한 사실 (설계 근거)

- 사용자 환경: 로컬 LiteLLM(`http://localhost:4444/v1`), 모델 `gemini/gemini-2.5-flash-image`.
- 이 chat-image 모델은 `/v1/images/generations`에 대해 빈 `data: []`를 준다 → **표준 이미지 엔드포인트로는 검증 불가.** `/model/info`의 `mode`도 이 모델은 `chat`이라 신뢰 못 함.
- 이미지 출력 과금은 **크기와 무관하게 한 장당 고정 1290 토큰(≈ $0.039)** — 작게 요청해도 절감 없음(1024² 이하는 동일 블록). → **"작은 이미지로 싸게 테스트"는 이 모델에서 불가.**
- 따라서 비용을 줄이는 지점은 크기가 아니라 **무과금 프로브**다.

## 3. 결정 — 2단계 검증, 이번 PR은 1단계(무과금)만

| 단계 | 수단 | 비용 | 잡는 것 |
|------|------|------|---------|
| **1 (이번 PR, 신규)** | **연결 테스트** = `GET {url}/models` + 키 | **$0** | URL·포트 오류, 키 오타(401), 모델명 오타 → **지난 세션 버그를 전부 잡음** |
| 2 (기존, 그대로) | 캐릭터 재생성 = 실제 `/chat/completions` 생성 | ~$0.04 | 이미지 실제 반환·포맷까지 확정(결과가 마스코트에 바로 보임) |

무과금 프로브가 지난 세션 실패를 모두 커버한다: 텍스트 URL을 넣으면 그 서버 `/models`에 `gemini/…-image` 모델이 없어 걸리고, `ssk-` 키는 401로 걸린다. 프로브는 "이미지를 실제로 그리는지"까지는 보장 못 하므로(그건 실제 생성뿐), **성공 메시지가 "실제 그림은 '캐릭터 재생성'으로 확인하세요"라고 안내**해 기대치를 정직하게 표현한다.

## 4. 컴포넌트 & 데이터 흐름

### 4.1 백엔드 — 신규 `image_test` 커맨드 (`engine_test` 미러링)

```
image_test(url, key, model) -> Result<String, String>      [commands.rs]
  ├ url 공백 → 즉시 Err("엔드포인트 URL을 입력하세요")
  ├ SpriteConfig 직접 구성 (trim, trailing '/' 제거, 모델 빈값이면 DEFAULT_IMAGE_MODEL)
  │    ← env 개입 없이 폼값 그대로 검증 (SpriteConfig::resolve의 env 폴백은 안 씀)
  ├ sprite::probe_endpoint(&cfg) -> ProbeVerdict            [sprite.rs, 네트워크]
  │    GET {base_url}/models  (키 있으면 Authorization: Bearer, 10s 타임아웃)
  │      ├ 200            → classify_models_body(body, model)   [순수·테스트 대상]
  │      ├ 401 / 403      → AuthFailed(code)
  │      ├ 429            → RateLimited
  │      ├ 그 외 status    → HttpError(code)
  │      └ 전송 실패        → Connection(msg)
  └ probe_result_message(verdict, model) -> Result<String,String>  [순수·테스트 대상]
```

`ProbeVerdict` (sprite.rs):

```rust
pub enum ProbeVerdict {
    Ok,                          // 200 + 모델이 /models 목록에 있음
    ModelMissing(Vec<String>),   // 200 이나 모델이 목록에 없음 (샘플 id 최대 5개)
    NotOpenAiCompat,             // 200 이나 data[] 배열이 없음 (OpenAI 호환 아님)
    AuthFailed(u16),             // 401 / 403
    RateLimited,                 // 429
    HttpError(u16),              // 기타 non-2xx
    Connection(String),          // 전송 실패
}
```

`classify_models_body(body, model)` (순수):
- `body["data"]`가 배열 아님 → `NotOpenAiCompat`
- `data[].id`를 모아 트림 비교, `model`(트림) 포함 → `Ok`
- 없음 → `ModelMissing(샘플 id)`

### 4.2 판정 → 사람이 읽는 결과 (`probe_result_message`, 순수)

| 판정 | 메시지 | Status |
|------|--------|--------|
| `Ok` | "엔드포인트·모델 확인됨 — 실제 그림은 '캐릭터 재생성'으로 확인하세요" | ok(초록) |
| `ModelMissing(ids)` | "연결·인증은 OK인데 '`{model}`' 모델이 목록에 없어요. 모델명을 확인하세요 (목록: {ids…})" | err(빨강) |
| `NotOpenAiCompat` | "이 URL은 모델 목록(/models)을 주지 않아요 — 엔드포인트가 OpenAI 호환 /v1 인지 확인하세요" | err |
| `AuthFailed(c)` | "인증 실패 ({c}) — API 키를 확인하세요" | err |
| `RateLimited` | "레이트 리밋 (429) — 잠시 후 다시 시도하세요" | err |
| `HttpError(c)` | "엔드포인트 오류 (HTTP {c}) — URL을 확인하세요" | err |
| `Connection(t)` | "연결 실패 — URL·포트를 확인하세요: {t}" | err |

`ModelMissing`는 연결·인증이 OK여도 사용자의 목표("이 모델이 동작")가 미달이므로 err(빨강)로 돌려, 메시지가 원인(모델명)을 명확히 한다.

**Ok 메시지는 키 유효성을 주장하지 않는다** (코드리뷰 반영). 공개 `/models`(예: OpenRouter)는 잘못된 키로도 200을 주므로, `Ok` 도달이 키 유효를 보장하지 않는다. 따라서 성공 메시지는 엔드포인트·모델 확인까지만 말하고, 키·실제 생성 확정은 "캐릭터 재생성"에 맡긴다. (사내 LiteLLM처럼 키 인증을 강제하는 게이트웨이는 잘못된 키를 `AuthFailed`로 먼저 걸러낸다.)

**빈 키 인증 동작 통일** (코드리뷰 반영). `sprite::with_bearer(req, key)` 헬퍼로 프로브와 `generate`가 동일하게 인증한다 — 키가 있으면 `Authorization: Bearer <key>`, 비면 헤더 **생략**. (이전엔 프로브는 생략·`generate`는 빈 Bearer를 보내, "no header 허용/빈 Bearer 거부" 게이트웨이에서 테스트 통과·재생성 실패의 위양성이 가능했다.)

### 4.3 등록 (`lib.rs`)

`invoke_handler`의 `generate_handler![...]` 목록에 `commands::image_test`를 추가한다 (이미지 커맨드들 근처, `image_settings_set` 다음 줄 등).

### 4.4 프론트

- **`src/lib/api.ts`**: `export const imageTest = (url, key, model) => invoke<string>('image_test', { url, key, model });` (기존 `engineTest` 바로 아래).
- **`src/lib/ui/settings/ConnectionGroup.svelte`** — "캐릭터 이미지" 섹션에만 추가:
  - `import { imageTest }` 추가.
  - `async function testImage(){ imgStatus = busy('연결 확인 중…'); try { imgStatus = ok(await imageTest(img.url, img.key, img.model)); } catch(e){ imgStatus = err(e); } }` (위 `testEngine` 패턴 그대로).
  - actions에 `<button onclick={testImage} disabled={imgStatus.kind==='busy'}>연결 테스트</button>`를 `저장`과 `캐릭터 재생성` 사이에 삽입.
  - **재생성 버튼·`regenerate()`·`regenerateSprite` import은 그대로 둔다** (이 PR은 순수 추가).
  - 최종 버튼: `저장` · `연결 테스트`(신규) · `캐릭터 재생성`(유지) — 위 "LLM 엔진"(`저장`·`연결 테스트`)과 대칭.

## 5. 에러 처리 규율

- URL 공백 → 커맨드 진입 즉시 Err (네트워크 안 감).
- 키 공백 → `Authorization` 헤더 생략 (관문 없는 서버 허용 — `hub`·`sprite::generate` 규율과 동일).
- 네트워크는 커맨드 안에서만. 설정을 읽지/쓰지 않으므로 **DB 락을 잡지 않는다**(`engine_test`와 동일).
- 타임아웃 10초 (프로브는 빨라야 함 — `generate`의 120초와 구분).

## 6. TDD (실패 테스트 먼저)

Rust 단위테스트 (순수 함수 — 네트워크 없음):

- `sprite::classify_models_body`
  - 모델이 `data[].id`에 있음 → `Ok`
  - 모델이 없음 → `ModelMissing`, 샘플 id 포함
  - `data` 배열 없음(예: HTML/오브젝트) → `NotOpenAiCompat`
  - 모델명 앞뒤 공백 트림 후 비교
- `commands::probe_result_message`
  - 각 `ProbeVerdict` → 기대 한국어 문자열 + `Ok`/`Err` 분기
  - `ModelMissing`가 err이고 모델명·목록 힌트 포함

`sprite::probe_endpoint`(네트워크 경로)는 단위테스트 제외 → **DoD의 로컬 LiteLLM 수동 확인**으로 검증(정상 URL/키, 오타 키→401, 텍스트 엔진 URL→ModelMissing 각각).

## 7. 명시적 범위 밖 (이번 PR 아님)

- **나 탭 재생성 IA 개편** — 재생성 버튼을 나 탭 "내 캐릭터" 섹션으로 이동, 연결 탭에서 제거, MBTI 저장 시 자동 재생성 거취, 현재 스프라이트 썸네일. **PR2로 분리** (별도 브레인스토밍). 이번 PR은 기존 재생성을 건드리지 않아 흩어짐을 악화시키지 않는다.
- **재생성(`generate`) 에러 경로 개선** — 동작 중인 경로라 건드리지 않음. 프로브가 주 피드백 채널.
- **저장 시점 `image_url == engine_url` 경고** — 프로브가 기능적으로 커버(그 URL은 모델 목록에 image 모델이 없음).
- **유료 "테스트 생성" 별도 버튼** — 기존 "캐릭터 재생성"이 그 역할.

### 프로브의 한계 (문서화)

`GET /models`의 키 검증 신뢰도는 게이트웨이마다 다르다. 로컬 LiteLLM은 보통 master key 인증이 걸려 401을 제대로 주지만, OpenRouter의 `/models`는 공개라 키를 검증하지 않는다(별도 `/key` 엔드포인트 필요). 사용자 실측 환경(로컬 LiteLLM)에서는 프로브가 정확히 동작한다. OpenRouter에서도 모델명 검증은 되며, 키 오류는 실제 재생성이 잡는다.

## 8. 브랜치·커밋

- 브랜치: `fix/sprite-parse-image-from-content` 위에서 분기 (파서 #103가 아직 main에 없음 — 이번 PR의 "실제 생성" 2단계 설명이 그 파서에 의존).
- 커밋: 영어 Conventional Commits, scope `agent`. 예: `feat(agent): add image endpoint connection test`.
- DoD: 스펙 → 실패 테스트 → `image_test` + 프론트 버튼 → 테스트 녹색 → 로컬 LiteLLM 수동 확인 → 커밋 → (원하면) PR → `docs-archive`.
