# 캐릭터 이미지 엔드포인트 "연결 테스트" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 설정 → 연결 → "캐릭터 이미지" 섹션에 무과금 "연결 테스트" 버튼을 추가해, 저장·재생성 전에 이미지 엔드포인트 URL·키·모델 오설정을 사람이 읽는 메시지로 잡아낸다.

**Architecture:** 백엔드에 `image_test` Tauri 커맨드를 추가한다(`engine_test` 미러링). 커맨드는 폼 값으로 `SpriteConfig`를 구성해 `sprite::probe_endpoint`(무과금 `GET {url}/models`)를 호출하고, 순수 함수 `classify_models_body`·`probe_result_message`로 판정→한국어 메시지를 만든다. 프론트는 위 "LLM 엔진"의 `연결 테스트` 버튼 UX를 그대로 복제한다. **순수 추가** — 기존 "캐릭터 재생성" 등은 건드리지 않는다.

**Tech Stack:** Rust (Tauri v2 커맨드, `ureq` 2.x, `serde_json`), Svelte 5 (runes), TypeScript. Windows 네이티브 빌드(PowerShell), WSL 금지.

## Global Constraints

- 플랫폼: Tauri v2 + Svelte 5(runes) + Windows 전용. 빌드/실행은 네이티브 PowerShell(`cargo test`, `npm run tauri dev`). WSL 금지. Tauri v1 API 금지.
- 커밋: 영어 Conventional Commits, scope `agent`.
- 브랜치: `feat/image-endpoint-connection-test` (이미 생성됨, #103 파서 포함·main 대비 클린 베이스).
- TDD 필수 — 실패 테스트 먼저.
- 순수 추가 스코프: 나 탭 IA 개편·재생성 에러 개선·저장 시점 경고는 **범위 밖(PR2)**.
- `ureq` 2.12.1 — 에러는 `ureq::Error::Status(u16, Response)` / `ureq::Error::Transport(_)`.
- `SpriteConfig` 필드(`base_url`·`api_key`·`model`)는 `pub` — 직접 구성 가능.
- `DEFAULT_IMAGE_MODEL = "google/gemini-2.5-flash-image"` (sprite.rs).

---

### Task 1: sprite.rs — 프로브 코어 (`ProbeVerdict` + `classify_models_body` + `probe_endpoint`)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (파일 끝의 `#[cfg(test)] mod tests` 바로 앞에 프로덕션 코드 추가; 테스트는 그 tests 모듈 안에 추가)

**Interfaces:**
- Produces:
  - `pub enum ProbeVerdict { Ok, ModelMissing(Vec<String>), NotOpenAiCompat, AuthFailed(u16), RateLimited, HttpError(u16), Connection(String) }` (derives `Debug, Clone, PartialEq, Eq`)
  - `pub fn classify_models_body(body: &serde_json::Value, model: &str) -> ProbeVerdict`
  - `pub fn probe_endpoint(cfg: &SpriteConfig) -> ProbeVerdict`

- [ ] **Step 1: Write the failing tests**

`a-mate/crates/core/src/sprite.rs`의 `mod tests { ... }` 안(기존 마지막 테스트 뒤, 닫는 `}` 앞)에 추가:

```rust
    #[test]
    fn classify_models_body_ok_when_model_listed() {
        let body = serde_json::json!({"data":[
            {"id":"gpt-4o"},
            {"id":"gemini/gemini-2.5-flash-image"}
        ]});
        assert_eq!(
            classify_models_body(&body, "gemini/gemini-2.5-flash-image"),
            ProbeVerdict::Ok
        );
    }

    #[test]
    fn classify_models_body_missing_when_model_absent() {
        let body = serde_json::json!({"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]});
        match classify_models_body(&body, "gemini/gemini-2.5-flash-image") {
            ProbeVerdict::ModelMissing(ids) => {
                assert!(ids.contains(&"gpt-4o".to_string()), "샘플 id 포함: {ids:?}");
                assert!(ids.len() <= 5, "샘플은 최대 5개");
            }
            v => panic!("ModelMissing 기대, got {v:?}"),
        }
    }

    #[test]
    fn classify_models_body_not_openai_when_no_data_array() {
        // data[] 배열이 없으면(예: 에러 오브젝트/HTML) OpenAI 호환이 아니다.
        let body = serde_json::json!({"error":"not found"});
        assert_eq!(classify_models_body(&body, "x"), ProbeVerdict::NotOpenAiCompat);
    }

    #[test]
    fn classify_models_body_trims_model_before_compare() {
        let body = serde_json::json!({"data":[{"id":"some/model"}]});
        assert_eq!(classify_models_body(&body, "  some/model  "), ProbeVerdict::Ok);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-mentor --lib sprite::tests::classify_models_body`
Expected: FAIL — `cannot find function classify_models_body` / `cannot find type ProbeVerdict`.

(크레이트명이 `agent-mentor`가 아니면 `cargo test -p agent_mentor` 또는 워크스페이스 루트에서 `cargo test classify_models_body`. 크레이트명은 `a-mate/crates/core/Cargo.toml`의 `[package] name` 확인.)

- [ ] **Step 3: Write minimal implementation**

`a-mate/crates/core/src/sprite.rs`에서 `#[cfg(test)] mod tests {` 바로 **앞**에 추가:

```rust
/// 이미지 엔드포인트 프로브 결과 — 무과금 `GET /models` 기반.
/// 실제 이미지 생성 능력까지는 확인하지 못한다(그건 `generate`뿐).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// 200 + 설정 모델이 /models 목록에 있음.
    Ok,
    /// 200 이나 설정 모델이 목록에 없음 (샘플 id 최대 5개).
    ModelMissing(Vec<String>),
    /// 200 이나 `data[]` 배열이 없음 — OpenAI 호환 /models 아님.
    NotOpenAiCompat,
    /// 401 / 403.
    AuthFailed(u16),
    /// 429.
    RateLimited,
    /// 기타 non-2xx.
    HttpError(u16),
    /// 전송 실패(연결 불가·DNS 등).
    Connection(String),
}

/// `GET /models` 200 응답 본문 + 설정 모델명 → 판정 (순수).
/// OpenAI 규격: `{"data":[{"id":"..."}, ...]}`.
pub fn classify_models_body(body: &serde_json::Value, model: &str) -> ProbeVerdict {
    let Some(list) = body.get("data").and_then(|d| d.as_array()) else {
        return ProbeVerdict::NotOpenAiCompat;
    };
    let ids: Vec<String> = list
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();
    let want = model.trim();
    if ids.iter().any(|id| id.trim() == want) {
        ProbeVerdict::Ok
    } else {
        ProbeVerdict::ModelMissing(ids.into_iter().take(5).collect())
    }
}

/// 무과금 프로브 — `GET {base_url}/models`로 엔드포인트·키·모델을 확인한다.
/// (네트워크 — 단위테스트 제외, 로컬 LiteLLM 수동 확인.)
pub fn probe_endpoint(cfg: &SpriteConfig) -> ProbeVerdict {
    let url = format!("{}/models", cfg.base_url);
    let mut req = ureq::get(&url).timeout(std::time::Duration::from_secs(10));
    if !cfg.api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", cfg.api_key));
    }
    match req.call() {
        Ok(resp) => {
            let body: serde_json::Value = resp.into_json().unwrap_or(serde_json::Value::Null);
            classify_models_body(&body, &cfg.model)
        }
        Err(ureq::Error::Status(code, _)) => match code {
            401 | 403 => ProbeVerdict::AuthFailed(code),
            429 => ProbeVerdict::RateLimited,
            _ => ProbeVerdict::HttpError(code),
        },
        Err(ureq::Error::Transport(t)) => ProbeVerdict::Connection(t.to_string()),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor --lib sprite::tests::classify_models_body`
Expected: PASS (4 tests).

- [ ] **Step 5: Verify the whole crate still compiles**

Run: `cargo test -p agent-mentor --lib sprite`
Expected: 기존 sprite 테스트 + 신규 4개 모두 PASS.

- [ ] **Step 6: Commit**

```bash
git add a-mate/crates/core/src/sprite.rs
git commit -m "feat(agent): add image endpoint probe (models list check)"
```

---

### Task 2: commands.rs — `image_test` 커맨드 + `probe_result_message`, lib.rs 등록

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`image_settings_set`(~L1467) 뒤에 커맨드 추가; 파일 안에 새 `#[cfg(test)] mod probe_message_tests` 추가)
- Modify: `a-mate/src-tauri/src/lib.rs:373` (invoke_handler 목록에 한 줄 추가)

**Interfaces:**
- Consumes (Task 1): `agent_mentor::sprite::{ProbeVerdict, probe_endpoint}`, 기존 `agent_mentor::sprite::{SpriteConfig, DEFAULT_IMAGE_MODEL}`
- Produces:
  - `fn probe_result_message(verdict: agent_mentor::sprite::ProbeVerdict, model: &str) -> Result<String, String>` (모듈 private, 테스트는 `use super::*`)
  - `#[tauri::command(async)] pub fn image_test(url: String, key: String, model: String) -> Result<String, String>`

- [ ] **Step 1: Write the failing tests**

`a-mate/src-tauri/src/commands.rs` 파일 **끝**에 새 테스트 모듈 추가:

```rust
#[cfg(test)]
mod probe_message_tests {
    use super::*;
    use agent_mentor::sprite::ProbeVerdict as V;

    #[test]
    fn ok_verdict_is_green_and_points_to_regenerate() {
        let msg = probe_result_message(V::Ok, "m").expect("Ok → Ok(초록)");
        assert!(msg.contains("확인됨"), "성공 문구: {msg}");
        assert!(msg.contains("캐릭터 재생성"), "재생성 안내 포함: {msg}");
    }

    #[test]
    fn model_missing_is_error_with_model_and_list() {
        let e = probe_result_message(V::ModelMissing(vec!["gpt-4o".into()]), "gemini/x")
            .expect_err("ModelMissing → Err(빨강)");
        assert!(e.contains("gemini/x"), "설정 모델명 포함: {e}");
        assert!(e.contains("gpt-4o"), "목록 샘플 포함: {e}");
    }

    #[test]
    fn auth_failed_mentions_code_and_key() {
        let e = probe_result_message(V::AuthFailed(401), "m").expect_err("401 → Err");
        assert!(e.contains("401") && e.contains("API 키"), "{e}");
    }

    #[test]
    fn connection_mentions_url() {
        let e = probe_result_message(V::Connection("dns error".into()), "m")
            .expect_err("Connection → Err");
        assert!(e.contains("연결 실패"), "{e}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-mentor-app probe_message_tests`
Expected: FAIL — `cannot find function probe_result_message`.

- [ ] **Step 3: Write minimal implementation**

`a-mate/src-tauri/src/commands.rs`에서 `image_settings_set` 커맨드(끝 `}`, ~L1467) **뒤**에 추가:

```rust
/// 프로브 판정 → 사람이 읽는 결과. Ok(초록)/Err(빨강)로 StatusLine에 표시된다. (순수)
fn probe_result_message(
    verdict: agent_mentor::sprite::ProbeVerdict,
    model: &str,
) -> Result<String, String> {
    use agent_mentor::sprite::ProbeVerdict as V;
    match verdict {
        V::Ok => Ok("확인됨 — 엔드포인트·키·모델 OK. 실제 그림은 '캐릭터 재생성'으로 확인하세요".into()),
        V::ModelMissing(ids) => {
            let sample = if ids.is_empty() {
                String::new()
            } else {
                format!(" (목록: {})", ids.join(", "))
            };
            Err(format!("연결·인증은 OK인데 '{model}' 모델이 목록에 없어요. 모델명을 확인하세요{sample}"))
        }
        V::NotOpenAiCompat => {
            Err("이 URL은 모델 목록(/models)을 주지 않아요 — 엔드포인트가 OpenAI 호환 /v1 인지 확인하세요".into())
        }
        V::AuthFailed(c) => Err(format!("인증 실패 ({c}) — API 키를 확인하세요")),
        V::RateLimited => Err("레이트 리밋 (429) — 잠시 후 다시 시도하세요".into()),
        V::HttpError(c) => Err(format!("엔드포인트 오류 (HTTP {c}) — URL을 확인하세요")),
        V::Connection(t) => Err(format!("연결 실패 — URL·포트를 확인하세요: {t}")),
    }
}

/// 저장 전 값으로 이미지 엔드포인트를 검증한다 (무과금 — `GET /models`).
/// 폼 값을 그대로 받아 실제 생성 없이 URL·키·모델을 확인한다. `engine_test` 미러링.
#[tauri::command(async)]
pub fn image_test(url: String, key: String, model: String) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("엔드포인트 URL을 입력하세요".into());
    }
    let model_in = model.trim();
    // env 폴백 없이 폼 값 그대로 검증 (SpriteConfig::resolve의 env 경로를 쓰지 않는다).
    let cfg = agent_mentor::sprite::SpriteConfig {
        base_url: url.trim_end_matches('/').to_string(),
        api_key: key.trim().to_string(),
        model: if model_in.is_empty() {
            agent_mentor::sprite::DEFAULT_IMAGE_MODEL.to_string()
        } else {
            model_in.to_string()
        },
    };
    let verdict = agent_mentor::sprite::probe_endpoint(&cfg);
    probe_result_message(verdict, &cfg.model)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor-app probe_message_tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Register the command in lib.rs**

`a-mate/src-tauri/src/lib.rs:373` — `commands::image_settings_set,` 다음 줄에 추가:

```rust
                commands::image_settings_set,
                commands::image_test,
```

- [ ] **Step 6: Verify the tauri crate builds with the command registered**

Run: `cargo build -p agent-mentor-app`
Expected: 컴파일 성공 (경고 무방). `image_test`가 handler에 인식됨.

- [ ] **Step 7: Commit**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs
git commit -m "feat(agent): add image_test command for endpoint validation"
```

---

### Task 3: 프론트 — `api.ts` 바인딩 + `ConnectionGroup.svelte` "연결 테스트" 버튼

**Files:**
- Modify: `a-mate/src/lib/api.ts` (`regenerateSprite`(~L303) 뒤)
- Modify: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte` (import L6, 함수 ~L67, 버튼 L128-131)

**Interfaces:**
- Consumes (Task 2): Tauri 커맨드 `image_test(url, key, model) -> string`
- Produces: `imageTest(url, key, model): Promise<string>`; 캐릭터 이미지 섹션의 `연결 테스트` 버튼 → `testImage()`

- [ ] **Step 1: Add the API binding**

`a-mate/src/lib/api.ts` — `regenerateSprite` 함수(끝 `}`, ~L303) **뒤**에 추가:

```typescript
/** 이미지 엔드포인트 검증 (무과금 — GET /models). 성공/실패 모두 사람이 읽는 메시지. 실패는 reject. */
export const imageTest = (url: string, key: string, model: string) =>
  invoke<string>('image_test', { url, key, model });
```

- [ ] **Step 2: Import it in ConnectionGroup.svelte**

`a-mate/src/lib/ui/settings/ConnectionGroup.svelte:6` 을 다음으로 교체:

```
    imageSettingsGet, imageSettingsSet, regenerateSprite,
```
→
```
    imageSettingsGet, imageSettingsSet, imageTest, regenerateSprite,
```

- [ ] **Step 3: Add the `testImage` handler**

같은 파일에서 `regenerate()` 함수(끝 `}`, ~L67) **뒤**, `imgSourceLabel` 선언 앞에 추가:

```typescript
  async function testImage(){
    imgStatus = busy('연결 확인 중…');
    try { imgStatus = ok(await imageTest(img.url, img.key, img.model)); }
    catch(e){ imgStatus = err(e); }
  }
```

(주의: `image_test`가 실패 시 이미 완성된 한국어 메시지를 reject하므로, 위 텍스트 엔진 `testEngine`과 달리 `err(e)`를 **접두어 없이** 그대로 쓴다.)

- [ ] **Step 4: Add the button**

같은 파일, 캐릭터 이미지 섹션의 actions(L128-131)를 다음으로 교체:

```svelte
  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind==='busy'}>저장</button>
    <button onclick={regenerate} disabled={imgStatus.kind==='busy'}>캐릭터 재생성</button>
  </div>
```
→
```svelte
  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind==='busy'}>저장</button>
    <button onclick={testImage} disabled={imgStatus.kind==='busy'}>연결 테스트</button>
    <button onclick={regenerate} disabled={imgStatus.kind==='busy'}>캐릭터 재생성</button>
  </div>
```

- [ ] **Step 5: Build the frontend (컴파일·import 검증)**

Run (a-mate 디렉터리에서): `npm run build`
Expected: 빌드 성공. `imageTest` import·svelte 템플릿이 정상 컴파일됨(미정의 import·구문 오류면 실패).
(참고: `package.json`에 별도 `check`(svelte-check) 스크립트는 없다. 전체 타입·동작 검증은 Task 4의 `npm run tauri dev` 수동 확인으로 마무리.)

- [ ] **Step 6: Commit**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/settings/ConnectionGroup.svelte
git commit -m "feat(agent): add image endpoint connection test button"
```

---

### Task 4: 로컬 LiteLLM 수동 검증 (DoD) + 마무리

**Files:** (없음 — 실행 검증)

- [ ] **Step 1: Launch the app**

Run (a-mate 디렉터리, PowerShell): `npm run tauri dev`
Expected: 앱 실행, 설정 → 연결 탭 진입 가능.

- [ ] **Step 2: 정상 케이스**

설정 → 연결 → 캐릭터 이미지에 실제 값 입력 후 **연결 테스트**:
- URL `http://localhost:4444/v1`, 키(유효), 모델 `gemini/gemini-2.5-flash-image`
Expected: 초록 "확인됨 — 엔드포인트·키·모델 OK. 실제 그림은 '캐릭터 재생성'으로 확인하세요".

- [ ] **Step 3: 오타 키 케이스**

키를 `ssk-...`(오타)로 바꾸고 연결 테스트.
Expected: 빨강 "인증 실패 (401) — API 키를 확인하세요" (LiteLLM이 master key 인증을 걸었을 때). 401 대신 다른 코드/무인증 서버면 판정이 그에 맞게 나오는지 확인.

- [ ] **Step 4: 잘못된 URL/모델 케이스**

URL을 텍스트 엔진 URL로 바꾸거나 모델명을 오타내고 연결 테스트.
Expected: 빨강 "…'`<model>`' 모델이 목록에 없어요…" 또는 "연결 실패 — URL·포트…"/"모델 목록을 주지 않아요" 중 상황에 맞는 메시지.

- [ ] **Step 5: 기존 동작 회귀 확인**

`저장`·`캐릭터 재생성` 버튼이 이전과 동일하게 동작하는지(재생성 성공 시 마스코트 교체) 확인.

- [ ] **Step 6: 전체 테스트 스위트 그린**

Run: `cargo test` (워크스페이스 루트 또는 a-mate)
Expected: 전부 PASS.

- [ ] **Step 7: DoD 마무리 — docs-archive**

`docs-archive` 스킬을 실행해 이 스펙·플랜을 `docs/archive/` 미러로 옮긴다(구현 완료 시). PR을 열 경우 같은 PR에 포함.

---

## Self-Review

**Spec coverage (스펙 §3~§7 대조):**
- §4.1 `image_test` 커맨드 → Task 2 ✅
- §4.1 `probe_endpoint` + `classify_models_body` → Task 1 ✅
- §4.2 `probe_result_message` 7판정 매핑 → Task 2 (모든 variant 매치) ✅
- §4.3 lib.rs 등록 → Task 2 Step 5 ✅
- §4.4 api.ts `imageTest` + 버튼 → Task 3 ✅
- §5 에러 규율(URL 공백 즉시 Err·키 공백 헤더 생략·락 없음·10s) → Task 1/2 코드에 반영 ✅
- §6 TDD 테스트 목록(classify_models_body 4종·probe_result_message 4종) → Task 1/2 ✅
- §6 probe_endpoint 네트워크 수동 검증 → Task 4 ✅
- §7 범위 밖(재생성·나 탭·경고) → 어느 태스크도 건드리지 않음 ✅

**Placeholder scan:** 모든 스텝에 실제 코드/명령/기대출력 포함. TBD·"적절히 처리" 없음 ✅

**Type consistency:** `ProbeVerdict`(7 variant) — Task 1 정의 ↔ Task 2 매치 동일. `classify_models_body(&Value,&str)->ProbeVerdict`, `probe_endpoint(&SpriteConfig)->ProbeVerdict`, `probe_result_message(ProbeVerdict,&str)->Result<String,String>`, `imageTest(url,key,model)->Promise<string>` — 태스크 간 시그니처 일치 ✅

**크레이트명(확정):** core = `agent-mentor` (모듈 경로 `agent_mentor`), tauri = `agent-mentor-app`. `cargo test -p agent-mentor` / `cargo test -p agent-mentor-app`. frontend는 `check` 스크립트 없음 → `npm run build`로 컴파일 확인.
