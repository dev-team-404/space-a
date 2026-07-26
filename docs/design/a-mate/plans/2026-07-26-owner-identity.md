# G1 주인 신원 체계 (Owner Identity) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 주인 풀네임(실명) 설정을 신설해 Life register/rename payload에 forward 필드로 싣고, 사람이 남기는 방명록 작성자를 풀네임으로 서명한다.

**Architecture:** 클라이언트(a-mate) 조립 — a-hub는 방명록 POST에 optional `author_name`을 받아 검증·저장만 하고(미제공 시 현행 봇 이름 fallback), 이름 조립·설정은 전부 a-mate 커맨드 층에서. payload의 `owner_full_name`은 `owner_os_user`와 같은 "빈값 생략·서버는 모르는 필드 무시" 규약.

**Tech Stack:** Rust(Tauri v2 커맨드 + `crates/core` ureq 클라이언트), Python(FastAPI + pydantic v2), Svelte 5 + TypeScript.

**스펙:** [2026-07-26-owner-identity-design.md](../specs/2026-07-26-owner-identity-design.md) · **ADR:** [0020](../../../adr/0020-owner-fullname-to-hub.md)

## Global Constraints

- **권위 테스트 환경은 Windows PowerShell** (`a-mate/`에서 `cargo test`(워크스페이스)·`npm test`). **WSL 금지.**
- **macOS(현 세션) 부분 검증 매트릭스** — 각 태스크의 "Run" 명령은 이것을 따른다:
  - Rust core: `cargo test -p agent-mentor` — 단, `hosts::tests::host_source_derives_sibling_paths` 1건은 Windows 경로를 기대하는 **기존 macOS 노이즈**(무시, 고치지 말 것).
  - Rust src-tauri(`agent-mentor-app`): macOS에서 테스트 바이너리 **링크 불가**(기존 이슈 — `libresource.a`). 로컬은 `cargo check -p agent-mentor-app`으로 컴파일만 검증하고, 테스트 실행은 Windows에서.
  - a-hub life: `a-hub/life/`에서 `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/ -q` (베이스라인 37 passed). `uv.lock`/`.venv`를 커밋하지 말 것.
  - 프론트: `a-mate/`에서 `npm test` (베이스라인 148 passed).
- 커밋은 **영어 Conventional Commits**: a-mate 변경 scope는 `agent`, a-hub 변경 scope는 `backend`. 커밋 푸터에 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- 신규 의존성 금지. 기존 코드 스타일(한국어 주석·컴팩트 포맷) 유지.
- 설정 키는 `owner_full_name` 하나. **빈값 = 미설정(옵트인)** — 기본값 없음(`owner_title`의 "주인" 기본값과 다름에 주의).
- `author_name` 한도 **80자**(trim 후), 초과는 `InvalidRequest`(HTTP 400 — `api.py:19` 매핑 확인됨).
- 방 헤더·GuestbookTab 표시·점유자 라벨은 **건드리지 않는다**(스펙 비목표).

---

### Task 1: a-hub — 방명록 optional `author_name` (service + REST)

**Files:**
- Modify: `a-hub/life/life_server/life.py` (`add_guestbook`, 383행 부근)
- Modify: `a-hub/life/life_server/api.py` (body 모델 80행 부근 + 엔드포인트 201행 부근)
- Test: `a-hub/life/tests/test_life.py`, `a-hub/life/tests/test_api.py`

**Interfaces:**
- Consumes: 없음 (독립 — 서버가 먼저 필드를 받도록 준비)
- Produces: `POST /life/{life_id}/guestbook` body `{ "body": str, "author_name": str | null }`. 규칙: `author_name` 미제공/trim 빈값 → 등록된 agent name(현행) / 1~80자 → 그대로 저장 / 80자 초과 → 400. Task 3의 Rust 클라이언트가 이 계약을 소비한다.

- [ ] **Step 1: 실패하는 서비스 테스트 작성** — `tests/test_life.py` 끝에 추가 (`import pytest`·`from life_server import errors`는 파일 상단에 이미 있음):

```python
def test_guestbook_author_name_override_and_delete_permission():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    visitor, visitor_token, _ = service.register("visitor-bot")

    # 전달 시 그대로 저장 (G1: 사람 작성 = 풀네임)
    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감", author_name="홍길동")
    assert entry["author_name"] == "홍길동"
    assert service.guestbook(owner_life.id)[0]["author_name"] == "홍길동"
    # author_agent_id는 토큰 주체 그대로 — 삭제 권한 판정 불변
    assert entry["author_agent_id"] == visitor.agent_id
    service.delete_guestbook(visitor_token, entry["entry_id"])
    assert service.guestbook(owner_life.id) == []


def test_guestbook_author_name_fallbacks_and_length_limit():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    # 미제공/공백 → 등록된 agent name (현행 동작·구클라 하위호환)
    assert service.add_guestbook(visitor_token, owner_life.id, "기본")["author_name"] == "visitor-bot"
    assert service.add_guestbook(visitor_token, owner_life.id, "공백", author_name="   ")["author_name"] == "visitor-bot"

    # 경계: trim 후 80자 허용, 81자 거부
    assert service.add_guestbook(visitor_token, owner_life.id, "한도", author_name="a" * 80)["author_name"] == "a" * 80
    with pytest.raises(errors.InvalidRequest):
        service.add_guestbook(visitor_token, owner_life.id, "초과", author_name="a" * 81)
```

- [ ] **Step 2: 실패 확인**

Run (`a-hub/life/`): `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/test_life.py -q -k author_name`
Expected: FAIL — `TypeError: ... add_guestbook() got an unexpected keyword argument 'author_name'`

- [ ] **Step 3: 서비스 구현** — `life.py`의 `add_guestbook`을 다음으로 교체:

```python
    def add_guestbook(self, token: str | None, life_id: str, body: str,
                      author_name: str | None = None) -> dict:
        author = self._authed(token)
        body = body.strip()
        if not body or len(body) > 500:
            raise errors.InvalidRequest("방명록은 1~500자여야 함")
        # G1: 클라이언트가 작성자 표기를 조립해 보낸다(사람=풀네임, 봇="{호칭}님의 {봇이름}").
        # 미제공이면 현행대로 등록된 agent name — 구클라 하위호환.
        name = (author_name or "").strip()
        if len(name) > 80:
            raise errors.InvalidRequest("작성자 이름은 80자 이하여야 함")
        if not name:
            name = author.name
        with self._lock:
            if life_id not in self._life:
                raise errors.NotFound(f"life '{life_id}' not found")
            row = {"entry_id": f"gb_{uuid.uuid4().hex[:12]}", "life_id": life_id,
                   "author_agent_id": author.agent_id, "author_name": name, "body": body,
                   "created_at": datetime.now(timezone.utc).isoformat()}
            self._guestbook.append(row)
            if self._store:
                self._store.save_guestbook_entry(row)
            return dict(row)
```

- [ ] **Step 4: 서비스 테스트 통과 확인**

Run: `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/test_life.py -q -k author_name`
Expected: `2 passed`

- [ ] **Step 5: 실패하는 API 테스트 작성** — `tests/test_api.py` 끝에 추가 (`client`·`_register` 픽스처는 파일 상단에 이미 있음):

```python
def test_guestbook_add_accepts_optional_author_name(client):
    a = _register(client, "A")
    b = _register(client, "B")
    hb = {"Authorization": f"Bearer {b['token']}"}

    # author_name 전달 → 그대로 저장·반환
    r = client.post(f"/life/{a['life_id']}/guestbook",
                    json={"body": "왔다감", "author_name": "홍길동"}, headers=hb)
    assert r.status_code == 201
    assert r.json()["author_name"] == "홍길동"

    # 미전달 → 등록된 agent name (구클라 하위호환)
    r = client.post(f"/life/{a['life_id']}/guestbook", json={"body": "기본"}, headers=hb)
    assert r.status_code == 201
    assert r.json()["author_name"] == "B"

    # 80자 초과 → 400 (InvalidRequest 매핑)
    r = client.post(f"/life/{a['life_id']}/guestbook",
                    json={"body": "초과", "author_name": "a" * 81}, headers=hb)
    assert r.status_code == 400
```

- [ ] **Step 6: 실패 확인**

Run: `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/test_api.py -q -k author_name`
Expected: FAIL — 첫 단언에서 `author_name`이 `"홍길동"`이 아니라 `"B"` (엔드포인트가 아직 필드를 안 넘김. `TextBody`는 pydantic 기본으로 모르는 필드를 버린다)

- [ ] **Step 7: API 구현** — `api.py` 80행 부근 `TextBody` 아래에 전용 모델 추가 (`TextBody`는 말풍선 엔드포인트와 공유라 건드리지 않는다):

```python
class GuestbookAddBody(BaseModel):
    body: str = ""
    author_name: str | None = None
```

201행 부근 엔드포인트를 다음으로 교체:

```python
    @app.post("/life/{life_id}/guestbook", status_code=201)
    def life_guestbook_add(life_id: str, body: GuestbookAddBody, authorization: str | None = Header(default=None)):
        return life.add_guestbook(_bearer(authorization), life_id, body.body, body.author_name)
```

- [ ] **Step 8: 전체 a-hub 테스트 통과 확인**

Run: `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/ -q`
Expected: `40 passed` (기존 37 + 신규 3)

- [ ] **Step 9: Commit**

```bash
git add a-hub/life/life_server/life.py a-hub/life/life_server/api.py a-hub/life/tests/test_life.py a-hub/life/tests/test_api.py
git commit -m "feat(backend): accept optional author_name on guestbook add

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: a-mate — `owner_full_name` 설정 헬퍼

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (헬퍼는 `owner_title` fn 1560행 부근, 테스트는 `owner_title_defaults_and_roundtrips` 1408행 부근)

**Interfaces:**
- Consumes: `SqliteStore::get_setting` (기존)
- Produces: `pub(crate) fn owner_full_name(store: &SqliteStore) -> String` — trim된 실명, 미설정/공백이면 `""`. Task 3·4가 호출.

- [ ] **Step 1: 실패하는 테스트 작성** — `commands.rs` 테스트 모듈의 `owner_title_defaults_and_roundtrips` 아래에 추가:

```rust
    #[test]
    fn owner_full_name_roundtrips_and_trims() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(owner_full_name(&store), ""); // 미설정 → 빈값 (옵트인, 기본값 없음)
        store.set_setting("owner_full_name", "  홍길동  ").unwrap();
        assert_eq!(owner_full_name(&store), "홍길동");
        store.set_setting("owner_full_name", "   ").unwrap(); // 공백 → 미설정과 동일
        assert_eq!(owner_full_name(&store), "");
    }
```

- [ ] **Step 2: 실패 확인 (macOS는 컴파일 에러로 확인)**

Run: `cargo check -p agent-mentor-app 2>&1 | grep owner_full_name | head -3`
Expected: `cannot find function 'owner_full_name' in this scope`
(Windows에서는 `cargo test -p agent-mentor-app owner_full_name` → 컴파일 실패)

- [ ] **Step 3: 구현** — `owner_title` fn 바로 아래에 추가:

```rust
/// 주인 풀네임(실명) 설정 — 미설정/공백이면 빈 문자열(옵트인, G1 스펙 §A).
pub(crate) fn owner_full_name(store: &SqliteStore) -> String {
    store.get_setting("owner_full_name").ok().flatten()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
```

- [ ] **Step 4: 컴파일 통과 확인**

Run: `cargo check -p agent-mentor-app`
Expected: 에러 없음 (경고 무방). 테스트 실행은 Windows에서: `cargo test -p agent-mentor-app owner_full_name` → PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/src-tauri/src/commands.rs
git commit -m "feat(agent): add owner_full_name setting helper

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: a-mate core — life_client body 빌더 + 시그니처 + 콜사이트

**Files:**
- Modify: `a-mate/crates/core/src/life_client.rs` (`register_profile` 62행 부근, `rename` 116행 부근, `add_guestbook` 207행 부근, 테스트 모듈 246행 부근)
- Modify: `a-mate/src-tauri/src/commands.rs` (콜사이트: `hub_connect` 725·740·770행 부근, `profile_set` 1623행 부근, `life_add_guestbook` 953행 부근)

**Interfaces:**
- Consumes: Task 2의 `owner_full_name(&SqliteStore) -> String`; Task 1의 서버 계약(`author_name` optional).
- Produces (Task 4·5가 의존):
  - `pub fn register_profile(base_url, api_key, name, mascot_seed, org, agent_uuid, owner_os_user, owner_full_name: &str) -> Result<Value>`
  - `LifeClient::rename(&self, name: &str, owner_os_user: &str, owner_full_name: &str) -> Result<Value>`
  - `LifeClient::add_guestbook(&self, life_id: &str, body: &str, author_name: Option<&str>) -> Result<Value>`

- [ ] **Step 1: 실패하는 body 빌더 테스트 작성** — `life_client.rs` 테스트 모듈(`use super::*;` 있음) 끝에 추가:

```rust
    #[test]
    fn register_body_includes_owner_full_name_when_set() {
        let b = register_body("둘쇠", "seed", "조직", "uuid-1", "jibin", "홍길동");
        assert_eq!(b["owner_full_name"], serde_json::json!("홍길동"));
        // 풀네임이 os_user를 대체하지 않는다 (병행, G1 결정)
        assert_eq!(b["owner_os_user"], serde_json::json!("jibin"));
    }

    #[test]
    fn register_body_omits_blank_owner_full_name() {
        let b = register_body("둘쇠", "seed", "", "", "", "   ");
        assert!(b.get("owner_full_name").is_none());
    }

    #[test]
    fn rename_body_includes_owner_full_name_when_set() {
        let b = rename_body("둘쇠", "jibin", "홍길동");
        assert_eq!(b["owner_full_name"], serde_json::json!("홍길동"));
        assert_eq!(b["owner_os_user"], serde_json::json!("jibin"));
    }

    #[test]
    fn rename_body_omits_blank_fields() {
        let b = rename_body("둘쇠", "", "");
        assert!(b.get("owner_os_user").is_none());
        assert!(b.get("owner_full_name").is_none());
    }

    #[test]
    fn guestbook_body_includes_author_name_when_some() {
        let b = guestbook_body("왔다감", Some("홍길동"));
        assert_eq!(b["body"], serde_json::json!("왔다감"));
        assert_eq!(b["author_name"], serde_json::json!("홍길동"));
    }

    #[test]
    fn guestbook_body_omits_author_name_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None).get("author_name").is_none());
        assert!(guestbook_body("왔다감", Some("  ")).get("author_name").is_none());
    }
```

- [ ] **Step 2: 실패 확인**

Run (`a-mate/`): `cargo test -p agent-mentor life_client`
Expected: FAIL — `cannot find function 'register_body' in this scope` (컴파일 에러)

- [ ] **Step 3: 빌더 구현 + 시그니처 변경** — `life_client.rs`:

`register_profile`(62행 부근)을 다음으로 교체 — doc 주석의 필드 나열도 갱신:

```rust
/// 프로필 포함 등록 — org·agent_uuid·owner_os_user·owner_full_name을 함께 보낸다. 현재 서버는
/// 모르는 필드를 무시하므로 하위호환이며, 서버가 프로필을 저장하도록 확장되면 그대로 쓰인다.
/// 빈 값은 생략한다.
pub fn register_profile(
    base_url: &str,
    api_key: Option<&str>,
    name: &str,
    mascot_seed: &str,
    org: &str,
    agent_uuid: &str,
    owner_os_user: &str,
    owner_full_name: &str,
) -> Result<Value> {
    let req = ureq::post(&format!("{}/life/register", base(base_url)))
        .timeout(std::time::Duration::from_secs(10));
    with_api_key(req, api_key)
        .send_json(register_body(name, mascot_seed, org, agent_uuid, owner_os_user, owner_full_name))
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

fn register_body(name: &str, mascot_seed: &str, org: &str, agent_uuid: &str,
                 owner_os_user: &str, owner_full_name: &str) -> Value {
    let mut body = json!({ "name": name, "mascot_seed": mascot_seed });
    if !org.trim().is_empty() {
        body["org"] = json!(org);
    }
    if !agent_uuid.trim().is_empty() {
        body["agent_uuid"] = json!(agent_uuid);
    }
    if !owner_os_user.trim().is_empty() {
        body["owner_os_user"] = json!(owner_os_user);
    }
    if !owner_full_name.trim().is_empty() {
        body["owner_full_name"] = json!(owner_full_name);
    }
    body
}
```

`register`(56행 부근) 위임 호출에 인자 하나 추가:

```rust
pub fn register(base_url: &str, api_key: Option<&str>, name: &str, mascot_seed: &str) -> Result<Value> {
    register_profile(base_url, api_key, name, mascot_seed, "", "", "", "")
}
```

`rename`(116행 부근)을 다음으로 교체 — doc 주석에 풀네임 언급 추가:

```rust
    /// 이름 변경(에이전트 + 내 방 주인 이름). 주인 OS 계정(owner_os_user)·풀네임(owner_full_name)도
    /// 함께 실어 기존 연결도 §F 주인 식별자를 갱신한다 — register_profile과 같은 규약(빈 값은 생략,
    /// 서버는 모르는 필드를 무시하므로 하위호환. register·PATCH가 동일 body 모델).
    pub fn rename(&self, name: &str, owner_os_user: &str, owner_full_name: &str) -> Result<Value> {
        self.req("PATCH", "/life/me")
            .send_json(rename_body(name, owner_os_user, owner_full_name))
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }
```

`rename_body`·`guestbook_body`는 자유 함수로 `register_body` 아래에 추가:

```rust
fn rename_body(name: &str, owner_os_user: &str, owner_full_name: &str) -> Value {
    let mut body = json!({ "name": name });
    if !owner_os_user.trim().is_empty() {
        body["owner_os_user"] = json!(owner_os_user);
    }
    if !owner_full_name.trim().is_empty() {
        body["owner_full_name"] = json!(owner_full_name);
    }
    body
}

/// 방명록 body — author_name은 있고 비어있지 않을 때만 실린다(G1: 사람 작성 = 풀네임 서명).
fn guestbook_body(body: &str, author_name: Option<&str>) -> Value {
    let mut v = json!({ "body": body });
    if let Some(name) = author_name.map(str::trim).filter(|n| !n.is_empty()) {
        v["author_name"] = json!(name);
    }
    v
}
```

`add_guestbook`(207행 부근)을 다음으로 교체:

```rust
    pub fn add_guestbook(&self, life_id: &str, body: &str, author_name: Option<&str>) -> Result<Value> {
        self.req("POST", &format!("/life/{life_id}/guestbook"))
            .send_json(guestbook_body(body, author_name)).map_err(err_of)?.into_json().map_err(Into::into)
    }
```

- [ ] **Step 4: core 테스트 통과 확인**

Run: `cargo test -p agent-mentor life_client`
Expected: PASS (신규 6건 포함, 기존 api_key 4건 유지)

- [ ] **Step 5: commands.rs 콜사이트 갱신 (워크스페이스 컴파일 복구)**

`hub_connect`(725행 부근) — `user` 읽기 블록에서 풀네임도 함께 읽는다:

```rust
    let (user, full_name) = {
        let guard = lock(&state)?;
        let user =
            guard.get_setting("user_name").ok().flatten().unwrap_or_default().trim().to_string();
        (user, owner_full_name(&guard))
    };
```

같은 함수의 rename 호출(740행 부근): `match client.rename(&user, &owner_os_user()) {` → `match client.rename(&user, &owner_os_user(), &full_name) {`

같은 함수의 register 호출(770행 부근):

```rust
    let v = life_client::register_profile(&url, key_opt.as_deref(), &user, &uuid, &org, &uuid, &os_user, &full_name)
        .map_err(|e| e.to_string())?;
```

`profile_set`(1618행 부근) — `old_name` 블록에서 저장된 풀네임도 읽어 rename에 동봉 (파라미터 추가는 Task 4에서):

```rust
    let (old_name, stored_full_name) = {
        let guard = lock(&state)?;
        let old = guard.get_setting("user_name").ok().flatten().unwrap_or_default();
        (old, owner_full_name(&guard))
    };
    if name != old_name {
        if let Some(client) = hub_client(&state)? {
            client.rename(&name, &owner_os_user(), &stored_full_name)
                .map_err(|e| format!("Life 서버 이름 변경 실패: {e}"))?;
        }
    }
```

`life_add_guestbook`(953행 부근) — 설정에서 풀네임을 읽어 전달 (사람 작성 경로 = 풀네임 서명):

```rust
#[tauri::command]
pub async fn life_add_guestbook(state: State<'_, AppState>, life_id: String, body: String) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    let full_name = { let guard = lock(&state)?; owner_full_name(&guard) };
    run_life_http("life_add_guestbook", move || {
        let author = (!full_name.is_empty()).then_some(full_name.as_str());
        client.add_guestbook(&life_id, &body, author).map_err(|e| e.to_string())
    })
    .await
}
```

- [ ] **Step 6: 전체 컴파일 + core 테스트 확인**

Run: `cargo check -p agent-mentor-app && cargo test -p agent-mentor life_client`
Expected: check 에러 없음, life_client 테스트 PASS

- [ ] **Step 7: Commit**

```bash
git add a-mate/crates/core/src/life_client.rs a-mate/src-tauri/src/commands.rs
git commit -m "feat(agent): carry owner_full_name in life payloads and guestbook author

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: a-mate — 프로필 API (`Profile`·`profile_get`·`profile_set`)

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`Profile` 1538행 부근, `profile_get` 1584행 부근, `profile_set` 1597행 부근, 테스트 모듈)

**Interfaces:**
- Consumes: Task 2 `owner_full_name(store)`, Task 3 `rename(name, os_user, full_name)`
- Produces: `Profile { …, owner_full_name: String }` (serde snake_case 그대로 JS로 감), `profile_set(state, name, org, mbti, owner_title, owner_full_name)` — JS invoke 키는 `ownerFullName`. Task 5가 소비.

- [ ] **Step 1: 실패하는 테스트 작성** — 테스트 모듈에 추가 (JS 경계로 나가는 필드명 계약):

```rust
    #[test]
    fn profile_serializes_owner_full_name_snake_case() {
        let p = Profile {
            name: "둘쇠".into(), org: "S/W 혁신팀".into(), uuid: "u-1".into(),
            mbti: String::new(), owner_title: "주인".into(), owner_full_name: "홍길동".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["owner_full_name"], serde_json::json!("홍길동"));
    }
```

- [ ] **Step 2: 실패 확인 (macOS는 컴파일 에러로 확인)**

Run: `cargo check -p agent-mentor-app 2>&1 | grep -m1 "owner_full_name"`
Expected: `struct 'Profile' has no field named 'owner_full_name'` 계열 컴파일 에러
(Windows: `cargo test -p agent-mentor-app profile_serializes` → 컴파일 실패)

- [ ] **Step 3: 구현**

`Profile` 구조체(1538행 부근)에 필드 추가:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    pub org: String,
    pub uuid: String,
    pub mbti: String,
    pub owner_title: String,
    pub owner_full_name: String,
}
```

`profile_get`(1584행 부근) 반환에 추가:

```rust
    Ok(Profile {
        name: get("user_name"), org, uuid, mbti: get("user_mbti"),
        owner_title: owner_title(&guard), owner_full_name: owner_full_name(&guard),
    })
```

`profile_set`(1597행 부근) — 파라미터 추가·트리거 확장·저장. 주의: 파라미터 `owner_full_name`이
같은 이름의 헬퍼 fn을 가리므로 헬퍼 호출은 `self::owner_full_name(&guard)`로 경로를 명시한다
(기존 `owner_title` 파라미터는 헬퍼를 호출하지 않아서 이 문제가 없었음):

```rust
/// 개인정보 저장. uuid는 불변(여기서 안 받음). mbti는 빈값(미설정) 또는 유효 4글자만 허용.
#[tauri::command(async)]
pub fn profile_set(
    state: State<AppState>,
    name: String,
    org: String,
    mbti: String,
    owner_title: String,
    owner_full_name: String,
) -> Result<Profile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("이름을 입력하세요".into());
    }
    let mbti_norm = if mbti.trim().is_empty() {
        String::new()
    } else {
        agent_mentor::mascot::normalize_mbti(&mbti)
            .ok_or_else(|| "MBTI는 E/I·S/N·T/F·J/P 조합 4글자여야 해요 (예: INTJ)".to_string())?
    };
    let org = org.trim();
    let org = if org.is_empty() { DEFAULT_ORG } else { org };
    let title = owner_title.trim();
    let title = if title.is_empty() { agent_mentor::mascot::DEFAULT_OWNER_TITLE } else { title };
    let full_name = owner_full_name.trim().to_string();
    let (old_name, old_full_name) = {
        let guard = lock(&state)?;
        let old = guard.get_setting("user_name").ok().flatten().unwrap_or_default();
        (old, self::owner_full_name(&guard))
    };
    // 이름·풀네임 어느 쪽이 바뀌어도 rename으로 서버의 주인 식별자를 갱신한다 (G1 스펙 §B)
    if name != old_name || full_name != old_full_name {
        if let Some(client) = hub_client(&state)? {
            client.rename(&name, &owner_os_user(), &full_name)
                .map_err(|e| format!("Life 서버 이름 변경 실패: {e}"))?;
        }
    }
    {
        let guard = lock(&state)?;
        guard.set_setting("user_name", &name).map_err(|e| e.to_string())?;
        guard.set_setting("hub_user", &name).map_err(|e| e.to_string())?;
        guard.set_setting("user_org", org).map_err(|e| e.to_string())?;
        guard.set_setting("user_mbti", &mbti_norm).map_err(|e| e.to_string())?;
        guard.set_setting("owner_title", title).map_err(|e| e.to_string())?;
        guard.set_setting("owner_full_name", &full_name).map_err(|e| e.to_string())?;
    }
    profile_get(state)
}
```

(Task 3에서 넣은 `stored_full_name` 중간 형태는 이 교체로 사라진다.)

- [ ] **Step 4: 컴파일 통과 확인**

Run: `cargo check -p agent-mentor-app`
Expected: 에러 없음. Windows: `cargo test -p agent-mentor-app` → 전체 PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/src-tauri/src/commands.rs
git commit -m "feat(agent): expose owner_full_name via profile get/set

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 프론트 — `api.ts` + MeGroup "주인 이름" 필드

**Files:**
- Modify: `a-mate/src/lib/api.ts` (`Profile` 314행 부근, `profileSet` 316행 부근)
- Modify: `a-mate/src/lib/ui/settings/MeGroup.svelte` (11·19·67행 부근)

**Interfaces:**
- Consumes: Task 4의 `profile_set` 커맨드 (invoke 키 `ownerFullName`), `Profile.owner_full_name`
- Produces: 설정 UI에서 실명 입력·저장 (사용자 대면 완결)

- [ ] **Step 1: api.ts 갱신** (컴포넌트 렌더 테스트 인프라 없음 — 레포 관행대로 타입/바인딩만, 스펙 테스트 섹션 참조):

```ts
export interface Profile { name: string; org: string; uuid: string; mbti: string; owner_title: string; owner_full_name: string }
export const profileGet = () => invoke<Profile>('profile_get');
export const profileSet = (name: string, org: string, mbti: string, ownerTitle: string, ownerFullName: string) =>
  invoke<Profile>('profile_set', { name, org, mbti, ownerTitle, ownerFullName });
```

- [ ] **Step 2: MeGroup.svelte 갱신**

11행 초기 상태:

```ts
  let prof = $state<Profile>({ name: '', org: '', uuid: '', mbti: '', owner_title: '주인', owner_full_name: '' });
```

19행 저장 호출:

```ts
      prof = await profileSet(prof.name, prof.org, profMbti, prof.owner_title, prof.owner_full_name);
```

67행 호칭 필드 **바로 아래**에 필드 추가 (maxlength=80은 서버 방명록 한도와 정합 — 스펙 §A/§D):

```svelte
    <label class="field"><span>주인 이름 <em>(실명 — 방명록 서명·신원 확인, 선택)</em></span><input type="text" bind:value={prof.owner_full_name} placeholder="홍길동" maxlength="80" spellcheck="false"/></label>
```

- [ ] **Step 3: 프론트 무회귀 + 빌드 확인**

Run (`a-mate/`): `npm test && npx vite build`
Expected: `148 passed` (기존 스위트 그대로) + 빌드 성공 (TS 타입 에러 없음 확인용)

- [ ] **Step 4: Commit**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/settings/MeGroup.svelte
git commit -m "feat(agent): add owner full name field to bot tab settings

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 최종 검증 + PR + 문서 DoD

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (G1 항목 69행 부근)
- Move (docs-archive 스킬이 수행): 본 계획 + `docs/design/a-mate/specs/2026-07-26-owner-identity-design.md` → `docs/archive/design/a-mate/…` (ADR 0020은 `docs/adr/`에 **남는다** — ADR은 아카이브 대상 아님)

**Interfaces:**
- Consumes: Task 1~5 완료 커밋 전부
- Produces: PR + 아카이브된 작업 문서 (DoD, ADR 0013)

- [ ] **Step 1: 전 스위트 로컬 검증**

Run (`a-mate/`): `cargo test -p agent-mentor && cargo check -p agent-mentor-app && npm test`
Run (`a-hub/life/`): `uv run --no-project --with "pytest>=8" --with "httpx>=0.27" python -m pytest tests/ -q`
Expected: core는 `hosts` 1건(기존 macOS 노이즈) 외 전부 PASS · check 클린 · 프론트 148 · a-hub 40

- [ ] **Step 2: 푸시 + PR 생성**

```bash
git push -u origin feat/amate-owner-identity
gh pr create --base main --title "feat(agent): owner identity — full name setting, life payload, guestbook author (G1)" --body "$(cat <<'EOF'
## Summary
- G1(주인 신원 체계): `owner_full_name` 설정 신설 — 봇 탭 "주인 이름" 필드(옵트인)
- Life register/rename payload에 `owner_full_name` forward 필드 동봉 (`owner_os_user` 병행 유지)
- a-hub 방명록 POST가 optional `author_name` 수용(80자 한도, 미제공 시 현행 fallback) — 사람 작성 방명록이 풀네임으로 서명됨
- 봇 작성 포맷 `{owner_title}님의 {봇이름}`은 규범만 확정 (구현은 P3/G3)

## Docs
- Spec: docs/design/a-mate/specs/2026-07-26-owner-identity-design.md (아카이브 이동)
- ADR 0020: 실명 허브 전송·방명록 영구 스냅샷 경계

## Test
- a-hub pytest 40 passed / Rust core life_client·owner_full_name 신규 테스트 / 프론트 148 passed
- Windows PowerShell에서 `cargo test`(워크스페이스)·`npm test` 최종 확인 필요

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: 로드맵 G1 완료 표기** — PR 번호를 받은 뒤 roadmap 69행 G1 제목 끝에 표기:

```markdown
**G1 — 주인 신원 체계: 호칭 표시 + 풀네임 식별 *(foundational)*** (백로그 3·4) — ✅ 완료(2026-07-26, PR #NNN)
```

(NNN은 Step 2에서 생성된 실제 PR 번호로 치환)

- [ ] **Step 4: docs-archive 스킬 실행** — 본 계획 문서와 스펙을 `docs/archive/` 미러로 이동(DoD). 스킬 절차를 따르고, 로드맵은 활성 문서로 남긴다.

- [ ] **Step 5: 문서 커밋 + 푸시**

```bash
git add -A docs/
git commit -m "docs(archive): archive G1 owner identity plan and spec

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
git push
```

- [ ] **Step 6: 사용자에게 Windows 최종 검증 요청** — Windows PowerShell에서 `a-mate/` 기준 `cargo test`·`npm test` 실행을 요청하고(특히 `agent-mentor-app` 테스트는 macOS에서 실행 불가), 결과 확인 후 머지 진행.
