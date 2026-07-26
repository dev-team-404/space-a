---
status: done
archived: 2026-07-27
---

# G2 방명록 답글 (1단계) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a-hub life 방명록에 방 주인 전용 1단계 답글(중첩 불가)을 추가하고 a-mate에 답글 UI를 붙인다.

**Architecture:** 기존 `POST /life/{life_id}/guestbook`에 optional `parent_id`를 확장(신규 엔드포인트 없음), GET은 평면 목록 그대로 행에 `parent_id`를 노출하고 그룹핑은 클라이언트(`groupGuestbook`)가 한다. 원글 삭제 시 답글 cascade. 스펙: [2026-07-26-guestbook-replies-design.md](../specs/2026-07-26-guestbook-replies-design.md), 의미론 결정: [ADR 0021](../../../../adr/0021-guestbook-replies-one-depth.md).

**Tech Stack:** a-hub life = FastAPI + SQLite(opt-in, 인메모리 원본) + pytest. a-mate = Tauri v2(Rust: ureq/serde_json) + Svelte 5 + Vitest.

## Global Constraints

- **작업 디렉터리**: worktree `/Users/jibin/Work/space-a/.claude/worktrees/feat+guestbook-replies`, 브랜치 `feat/guestbook-replies`. 모든 경로는 worktree 루트 기준.
- **답글 규칙 (ADR 0021, 정확히 이 값)**: 답글은 방 주인만(위반 403 `forbidden`) / 1단계만 — parent가 이미 답글이면 400 `invalid_request` / parent 미존재·타 방이면 404 `not_found` / 원글 삭제 시 답글 cascade / 원글당 답글 수 무제한 / top-level 원글 작성은 기존과 완전 동일(추가 검사 없음).
- **기존 검증 재사용**: 본문 1~500자, `author_name` ≤80자(G1) — 답글 전용 규칙 신설 금지.
- **커밋**: Conventional Commits 영어. a-hub 변경 = `feat(backend): ...`, a-mate 변경 = `feat(agent): ...`. 각 커밋 끝에 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- **a-hub 테스트 (macOS에서 실행 가능, 권위)**: worktree 루트에서
  `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q`
  (`--with-editable .` 필수 — 없으면 fastapi 미설치로 collection 에러. `uv.lock`·`.venv` 커밋 금지.)
- **a-mate 테스트**: 권위 실행은 **Windows PowerShell**(`a-mate/`에서 `cargo test`·`npm test`, WSL 금지). macOS 부분 신호: `cargo test -p agent-mentor`(기존 노이즈 1건 — `hosts::tests::host_source_derives_sibling_paths`는 Windows 경로 기대라 macOS에서 원래 실패, 무시), `cargo check -p agent-mentor-app --tests`(링크 불가라 컴파일만), `npm test`, `npm run build`.
- **프론트 테스트 관행**: Vitest는 순수 `.ts` 모듈만 — Svelte 컴포넌트 렌더 테스트 금지.
- **Life REST는 contracts/ 범위 밖** (ADR 0020 전례) — contracts/ 수정 금지.
- **배포 순서 규범**: 서버 먼저 (구서버는 `parent_id` 무시 → 답글이 원글로 저장되므로). 이번 PR 범위에 배포 자체는 없음.

---

### Task 1: a-hub 도메인 — 답글 검증·저장·cascade (인메모리)

**Files:**
- Modify: `a-hub/life/life_server/life.py:383-419` (`add_guestbook`, `delete_guestbook`)
- Test: `a-hub/life/tests/test_life.py` (파일 끝에 추가)

**Interfaces:**
- Consumes: 기존 `LifeService.register/add_guestbook/guestbook/delete_guestbook`, `errors.InvalidRequest/Forbidden/NotFound`.
- Produces: `add_guestbook(token, life_id, body, author_name=None, parent_id=None)` — 반환 행에 `"parent_id"` 키(top-level은 `None`); `delete_guestbook`이 인메모리에서 답글 cascade. Task 2·3이 이 시그니처에 의존.

- [ ] **Step 1: 실패하는 테스트 작성** — `a-hub/life/tests/test_life.py` 끝에 추가:

```python
def test_guestbook_reply_one_depth_owner_only():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    assert entry["parent_id"] is None

    reply = service.add_guestbook(owner_token, owner_life.id, "고마워요", parent_id=entry["entry_id"])
    assert reply["parent_id"] == entry["entry_id"]

    # 방 주인이 아니면 답글 불가
    with pytest.raises(errors.Forbidden):
        service.add_guestbook(visitor_token, owner_life.id, "저도요", parent_id=entry["entry_id"])

    # 답글의 답글(2단계) 불가
    with pytest.raises(errors.InvalidRequest):
        service.add_guestbook(owner_token, owner_life.id, "중첩", parent_id=reply["entry_id"])


def test_guestbook_reply_parent_must_exist_in_same_life():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, _, other_life = service.register("other-bot")

    with pytest.raises(errors.NotFound):
        service.add_guestbook(owner_token, owner_life.id, "고아", parent_id="gb_missing")

    # 다른 방의 원글을 부모로 지정할 수 없다
    foreign = service.add_guestbook(owner_token, other_life.id, "남의 방 글")
    with pytest.raises(errors.NotFound):
        service.add_guestbook(owner_token, owner_life.id, "크로스", parent_id=foreign["entry_id"])


def test_guestbook_multiple_replies_and_cascade_delete():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    r1 = service.add_guestbook(owner_token, owner_life.id, "첫 답글", parent_id=entry["entry_id"])
    r2 = service.add_guestbook(owner_token, owner_life.id, "둘째 답글", parent_id=entry["entry_id"])
    assert {r["entry_id"] for r in service.guestbook(owner_life.id)} == {
        entry["entry_id"], r1["entry_id"], r2["entry_id"]}

    # 원글 삭제(작성자) → 답글도 함께(cascade)
    service.delete_guestbook(visitor_token, entry["entry_id"])
    assert service.guestbook(owner_life.id) == []


def test_guestbook_reply_deletes_alone_without_touching_parent():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    reply = service.add_guestbook(owner_token, owner_life.id, "답글", parent_id=entry["entry_id"])
    service.delete_guestbook(owner_token, reply["entry_id"])
    assert [r["entry_id"] for r in service.guestbook(owner_life.id)] == [entry["entry_id"]]
```

- [ ] **Step 2: 실패 확인**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/test_life.py -q -k "guestbook_reply or multiple_replies"`
Expected: FAIL — `TypeError: add_guestbook() got an unexpected keyword argument 'parent_id'`

- [ ] **Step 3: 구현** — `a-hub/life/life_server/life.py`의 `add_guestbook`/`delete_guestbook`을 다음으로 교체:

```python
    def add_guestbook(self, token: str | None, life_id: str, body: str,
                      author_name: str | None = None, parent_id: str | None = None) -> dict:
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
        parent_id = (parent_id or "").strip() or None
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            if parent_id is not None:
                # G2(ADR 0021): 답글은 방 주인만, 1단계만 — 부모는 같은 방의 top-level 원글.
                parent = next((r for r in self._guestbook if r["entry_id"] == parent_id), None)
                if parent is None or parent["life_id"] != life_id:
                    raise errors.NotFound("부모 방명록 항목을 찾을 수 없음")
                if parent.get("parent_id"):
                    raise errors.InvalidRequest("답글에는 답글을 달 수 없음")
                if author.agent_id != life.owner_agent_id:
                    raise errors.Forbidden("방 주인만 답글을 달 수 있음")
            row = {"entry_id": f"gb_{uuid.uuid4().hex[:12]}", "life_id": life_id,
                   "author_agent_id": author.agent_id, "author_name": name, "body": body,
                   "parent_id": parent_id,
                   "created_at": datetime.now(timezone.utc).isoformat()}
            self._guestbook.append(row)
            if self._store:
                self._store.save_guestbook_entry(row)
            return dict(row)

    def delete_guestbook(self, token: str | None, entry_id: str) -> dict:
        actor = self._authed(token)
        with self._lock:
            row = next((r for r in self._guestbook if r["entry_id"] == entry_id), None)
            if row is None:
                raise errors.NotFound("방명록 항목을 찾을 수 없음")
            life = self._life[row["life_id"]]
            if actor.agent_id not in (row["author_agent_id"], life.owner_agent_id):
                raise errors.Forbidden("작성자 또는 방 주인만 삭제할 수 있음")
            # G2(ADR 0021): 원글 삭제 시 답글도 함께(cascade) — 고아 행 금지
            self._guestbook = [r for r in self._guestbook
                               if r["entry_id"] != entry_id and r.get("parent_id") != entry_id]
            if self._store:
                self._store.delete_guestbook_entry(entry_id)
        return {"entry_id": entry_id, "deleted": True}
```

(변경점: ① `parent_id` 파라미터 + 빈문자열 정규화 ② `life_id not in self._life` 멤버십 체크를 `life` 객체 조회로 바꿔 주인 비교에 사용 ③ 답글 검증 3종 ④ 행에 `parent_id` 키 ⑤ 삭제를 `remove(row)`에서 cascade 필터로.)

- [ ] **Step 4: 통과 확인 + 전체 무회귀**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q`
Expected: 전부 PASS (베이스라인 40 + 신규 4)

- [ ] **Step 5: Commit**

```bash
git add a-hub/life/life_server/life.py a-hub/life/tests/test_life.py
git commit -m "feat(backend): add one-depth owner replies to life guestbook domain

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: a-hub store — parent_id 영속·마이그레이션·cascade

**Files:**
- Modify: `a-hub/life/life_server/store.py` (`_SCHEMA`의 guestbook, `__init__` 마이그레이션, `load_social`, `save_guestbook_entry`, `delete_guestbook_entry`)
- Test: `a-hub/life/tests/test_store.py` (파일 끝에 추가; 상단에 `import sqlite3` 필요)

**Interfaces:**
- Consumes: Task 1의 `add_guestbook(..., parent_id=)`·cascade `delete_guestbook`.
- Produces: SQLite에 `parent_id` 컬럼(레거시 DB는 자동 `ALTER TABLE`), `load_social()` 방명록 행에 `parent_id` 키(레거시 행 `None`), `delete_guestbook_entry`가 DB에서 cascade.

- [ ] **Step 1: 실패하는 테스트 작성** — `a-hub/life/tests/test_store.py` 상단 import에 `import sqlite3` 추가 후, 파일 끝에:

```python
def test_guestbook_replies_survive_restart_and_cascade(tmp_path):
    db = str(tmp_path / "life-gb.db")
    s1 = LifeService(store=SqliteStore(db))
    _, owner_token, owner_life = s1.register("owner-bot")
    _, visitor_token, _ = s1.register("visitor-bot")
    entry = s1.add_guestbook(visitor_token, owner_life.id, "왔다감")
    reply = s1.add_guestbook(owner_token, owner_life.id, "고마워요", parent_id=entry["entry_id"])

    # 재시작 — parent_id 복원
    s2 = LifeService(store=SqliteStore(db))
    rows = {r["entry_id"]: r for r in s2.guestbook(owner_life.id)}
    assert rows[reply["entry_id"]]["parent_id"] == entry["entry_id"]
    assert rows[entry["entry_id"]]["parent_id"] is None

    # 원글 삭제 → DB에서도 답글 cascade — 재시작 후에도 비어 있다
    s2.delete_guestbook(visitor_token, entry["entry_id"])
    s3 = LifeService(store=SqliteStore(db))
    assert s3.guestbook(owner_life.id) == []


def test_legacy_guestbook_db_gains_parent_id_column(tmp_path):
    db = str(tmp_path / "life-legacy.db")
    # parent_id 없는 구 스키마 DB를 직접 구성
    conn = sqlite3.connect(db)
    conn.execute(
        "CREATE TABLE guestbook (entry_id TEXT PRIMARY KEY, life_id TEXT NOT NULL,"
        " author_agent_id TEXT NOT NULL, author_name TEXT NOT NULL, body TEXT NOT NULL,"
        " created_at TEXT NOT NULL)"
    )
    conn.execute("INSERT INTO guestbook VALUES ('gb_legacy', 'l1', 'a1', '옛손님', '옛글',"
                 " '2026-01-01T00:00:00+00:00')")
    conn.commit()
    conn.close()

    _, _, _, guestbook = SqliteStore(db).load_social()
    assert guestbook == [{"entry_id": "gb_legacy", "life_id": "l1", "author_agent_id": "a1",
                          "author_name": "옛손님", "body": "옛글", "parent_id": None,
                          "created_at": "2026-01-01T00:00:00+00:00"}]
```

- [ ] **Step 2: 실패 확인**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/test_store.py -q`
Expected: 신규 2건 FAIL — restart 테스트는 `parent_id`가 복원 행에 없음(`KeyError` 또는 `None` 비교 실패), legacy 테스트는 행 dict에 `parent_id` 키 부재로 불일치. (Task 1만 반영된 상태에서는 `save_guestbook_entry`가 `parent_id`를 저장하지 않음.)

- [ ] **Step 3: 구현** — `a-hub/life/life_server/store.py`:

① `_SCHEMA`의 guestbook 테이블을 다음으로 교체:

```sql
CREATE TABLE IF NOT EXISTS guestbook (
  entry_id        TEXT PRIMARY KEY,
  life_id         TEXT NOT NULL,
  author_agent_id TEXT NOT NULL,
  author_name     TEXT NOT NULL,
  body            TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  parent_id       TEXT
);
```

② `__init__`에서 agents 마이그레이션 블록 뒤(`self._conn.commit()` 앞)에 추가:

```python
        guestbook_columns = {row[1] for row in self._conn.execute("PRAGMA table_info(guestbook)")}
        if "parent_id" not in guestbook_columns:
            self._conn.execute("ALTER TABLE guestbook ADD COLUMN parent_id TEXT")
```

③ `load_social`의 guestbook 조회를 교체:

```python
        guestbook = [
            {"entry_id": entry_id, "life_id": life_id, "author_agent_id": author_id,
             "author_name": author_name, "body": body, "parent_id": parent_id,
             "created_at": created_at}
            for entry_id, life_id, author_id, author_name, body, parent_id, created_at in self._conn.execute(
                "SELECT entry_id, life_id, author_agent_id, author_name, body, parent_id, created_at FROM guestbook"
            )
        ]
```

④ `save_guestbook_entry`·`delete_guestbook_entry`를 교체 (positional INSERT → 명시 컬럼, cascade):

```python
    def save_guestbook_entry(self, entry: dict) -> None:
        self._conn.execute(
            "INSERT INTO guestbook (entry_id, life_id, author_agent_id, author_name, body, parent_id, created_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (entry["entry_id"], entry["life_id"], entry["author_agent_id"], entry["author_name"],
             entry["body"], entry.get("parent_id"), entry["created_at"]),
        )
        self._conn.commit()

    def delete_guestbook_entry(self, entry_id: str) -> None:
        # G2(ADR 0021): 원글 삭제 시 답글도 cascade — 1-depth라 재귀 불필요
        self._conn.execute("DELETE FROM guestbook WHERE entry_id = ? OR parent_id = ?",
                           (entry_id, entry_id))
        self._conn.commit()
```

- [ ] **Step 4: 통과 확인 + 전체 무회귀**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q`
Expected: 전부 PASS

- [ ] **Step 5: Commit**

```bash
git add a-hub/life/life_server/store.py a-hub/life/tests/test_store.py
git commit -m "feat(backend): persist guestbook reply parent_id and cascade delete

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: a-hub API — POST body에 parent_id

**Files:**
- Modify: `a-hub/life/life_server/api.py:84-86` (`GuestbookAddBody`), `:206-208` (`life_guestbook_add`)
- Test: `a-hub/life/tests/test_api.py` (파일 끝에 추가)

**Interfaces:**
- Consumes: Task 1의 `add_guestbook(..., parent_id=)`.
- Produces: `POST /life/{life_id}/guestbook` body `{body, author_name?, parent_id?}` — a-mate(Task 4)가 이 규약에 의존.

- [ ] **Step 1: 실패하는 테스트 작성** — `a-hub/life/tests/test_api.py` 끝에 추가:

```python
def test_guestbook_reply_roundtrip_and_error_codes(client):
    owner = _register(client, "owner-bot")
    visitor = _register(client, "visitor-bot")
    ho = {"Authorization": f"Bearer {owner['token']}"}
    hv = {"Authorization": f"Bearer {visitor['token']}"}

    entry = client.post(f"/life/{owner['life_id']}/guestbook",
                        json={"body": "왔다감"}, headers=hv).json()

    r = client.post(f"/life/{owner['life_id']}/guestbook",
                    json={"body": "고마워요", "parent_id": entry["entry_id"]}, headers=ho)
    assert r.status_code == 201
    reply = r.json()
    assert reply["parent_id"] == entry["entry_id"]

    # GET 평면 목록의 행에 parent_id가 노출된다
    entries = client.get(f"/life/{owner['life_id']}/guestbook").json()["entries"]
    assert {e["entry_id"]: e["parent_id"] for e in entries} == {
        entry["entry_id"]: None, reply["entry_id"]: entry["entry_id"]}

    # 비주인 403 / 답글의 답글 400 / 부모 미존재 404
    assert client.post(f"/life/{owner['life_id']}/guestbook",
                       json={"body": "저도", "parent_id": entry["entry_id"]},
                       headers=hv).status_code == 403
    assert client.post(f"/life/{owner['life_id']}/guestbook",
                       json={"body": "중첩", "parent_id": reply["entry_id"]},
                       headers=ho).status_code == 400
    assert client.post(f"/life/{owner['life_id']}/guestbook",
                       json={"body": "고아", "parent_id": "gb_missing"},
                       headers=ho).status_code == 404
```

- [ ] **Step 2: 실패 확인**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/test_api.py -q`
Expected: 신규 1건 FAIL — `GuestbookAddBody`가 `parent_id`를 무시해 답글이 top-level로 저장 → `reply["parent_id"]`가 `None` (assert 실패)

- [ ] **Step 3: 구현** — `a-hub/life/life_server/api.py`:

```python
class GuestbookAddBody(BaseModel):
    body: str = ""
    author_name: str | None = None
    parent_id: str | None = None
```

```python
    @app.post("/life/{life_id}/guestbook", status_code=201)
    def life_guestbook_add(life_id: str, body: GuestbookAddBody, authorization: str | None = Header(default=None)):
        return life.add_guestbook(_bearer(authorization), life_id, body.body, body.author_name, body.parent_id)
```

- [ ] **Step 4: 통과 확인 + 전체 무회귀**

Run: `cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q`
Expected: 전부 PASS

- [ ] **Step 5: Commit**

```bash
git add a-hub/life/life_server/api.py a-hub/life/tests/test_api.py
git commit -m "feat(backend): accept parent_id in guestbook add API

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: a-mate Rust — life_client·커맨드에 parent_id 배선

**Files:**
- Modify: `a-mate/crates/core/src/life_client.rs:111-118` (`guestbook_body`), `:233-236` (`add_guestbook`), `:327-338` (기존 테스트 시그니처 갱신 + 신규 테스트)
- Modify: `a-mate/src-tauri/src/commands.rs:954-964` (`life_add_guestbook`)

**Interfaces:**
- Consumes: Task 3의 REST 규약 (`parent_id` optional body 필드).
- Produces: `LifeClient::add_guestbook(&self, life_id: &str, body: &str, author_name: Option<&str>, parent_id: Option<&str>) -> Result<Value>`; Tauri 커맨드 `life_add_guestbook(state, life_id: String, body: String, parent_id: Option<String>)` — 프론트(Task 5)가 `parentId` camelCase 인자로 호출.

- [ ] **Step 1: 실패하는 테스트 작성** — `a-mate/crates/core/src/life_client.rs`의 tests 모듈:

① 기존 2개 테스트의 호출을 3-인자 시그니처로 갱신:

```rust
    #[test]
    fn guestbook_body_includes_author_name_when_some() {
        let b = guestbook_body("왔다감", Some("홍길동"), None);
        assert_eq!(b["body"], serde_json::json!("왔다감"));
        assert_eq!(b["author_name"], serde_json::json!("홍길동"));
    }

    #[test]
    fn guestbook_body_omits_author_name_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None, None).get("author_name").is_none());
        assert!(guestbook_body("왔다감", Some("  "), None).get("author_name").is_none());
    }
```

② 신규 테스트 추가 (같은 tests 모듈 끝):

```rust
    #[test]
    fn guestbook_body_includes_parent_id_when_some() {
        let b = guestbook_body("고마워요", Some("홍길동"), Some("gb_abc123"));
        assert_eq!(b["parent_id"], serde_json::json!("gb_abc123"));
    }

    #[test]
    fn guestbook_body_omits_parent_id_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None, None).get("parent_id").is_none());
        assert!(guestbook_body("왔다감", None, Some("  ")).get("parent_id").is_none());
    }
```

- [ ] **Step 2: 실패(컴파일 에러) 확인**

Run: `cd a-mate && cargo test -p agent-mentor guestbook_body`
Expected: COMPILE FAIL — `guestbook_body`가 2개 인자만 받음 (`this function takes 2 arguments but 3 arguments were supplied`)

- [ ] **Step 3: 구현** — `a-mate/crates/core/src/life_client.rs`:

```rust
/// 방명록 body — author_name은 있고 비어있지 않을 때만 실린다(G1: 사람 작성 = 풀네임 서명).
/// parent_id는 답글일 때만 실린다(G2/ADR 0021: 방 주인 전용 1단계 답글, 빈값 생략 규약).
fn guestbook_body(body: &str, author_name: Option<&str>, parent_id: Option<&str>) -> Value {
    let mut v = json!({ "body": body });
    if let Some(name) = author_name.map(str::trim).filter(|n| !n.is_empty()) {
        v["author_name"] = json!(name);
    }
    if let Some(parent) = parent_id.map(str::trim).filter(|p| !p.is_empty()) {
        v["parent_id"] = json!(parent);
    }
    v
}
```

```rust
    pub fn add_guestbook(&self, life_id: &str, body: &str, author_name: Option<&str>, parent_id: Option<&str>) -> Result<Value> {
        self.req("POST", &format!("/life/{life_id}/guestbook"))
            .send_json(guestbook_body(body, author_name, parent_id)).map_err(err_of)?.into_json().map_err(Into::into)
    }
```

호출자 갱신 — `a-mate/src-tauri/src/commands.rs`의 `life_add_guestbook`:

```rust
#[tauri::command]
pub async fn life_add_guestbook(state: State<'_, AppState>, life_id: String, body: String, parent_id: Option<String>) -> Result<serde_json::Value, String> {
    let Some(client) = hub_client(&state)? else { return Err("hub_not_connected".into()) };
    // 사람 작성 경로 = 풀네임 서명 (G1 스펙 §C). 미설정이면 미전달 → 서버가 봇 이름 fallback.
    let full_name = { let guard = lock(&state)?; owner_full_name(&guard) };
    run_life_http("life_add_guestbook", move || {
        let author = (!full_name.is_empty()).then_some(full_name.as_str());
        client.add_guestbook(&life_id, &body, author, parent_id.as_deref()).map_err(|e| e.to_string())
    })
    .await
}
```

다른 호출자가 없는지 확인: `grep -rn "add_guestbook" a-mate/crates a-mate/src-tauri` → `life_client.rs`(정의·body 헬퍼)와 `commands.rs:961` 하나뿐이어야 함. 다른 호출자가 나오면 같은 방식(`None` 전달)으로 갱신.

- [ ] **Step 4: 통과 확인**

Run: `cd a-mate && cargo test -p agent-mentor life_client && cargo check -p agent-mentor-app --tests`
Expected: life_client 테스트 전부 PASS(신규 2 포함), src-tauri 컴파일 OK. (macOS: 워크스페이스 전체 `cargo test`는 돌리지 않아도 됨 — Task 7에서 일괄.)

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/life_client.rs a-mate/src-tauri/src/commands.rs
git commit -m "feat(agent): thread guestbook reply parent_id through life client

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: a-mate 프론트 — api.ts 확장 + groupGuestbook 순수 모듈

**Files:**
- Modify: `a-mate/src/lib/api.ts:213` (`GuestbookEntry`), `:225` (`lifeAddGuestbook`)
- Create: `a-mate/src/lib/guestbook.ts`
- Test: `a-mate/src/lib/guestbook.test.ts` (Vitest 기본 include가 자동 수집)

**Interfaces:**
- Consumes: Task 4의 커맨드 인자(`parentId` → Rust `parent_id`; Tauri v2가 camelCase→snake_case 자동 매핑 — 기존 `lifeId` 전례).
- Produces: `lifeAddGuestbook(lifeId: string, body: string, parentId?: string)`; `groupGuestbook(entries: GuestbookEntry[]): GuestbookThread[]`, `GuestbookThread = { entry: GuestbookEntry; replies: GuestbookEntry[] }` — Task 6이 의존.

- [ ] **Step 1: 실패하는 테스트 작성** — `a-mate/src/lib/guestbook.test.ts` 신규:

```ts
import { describe, expect, it } from 'vitest';
import { groupGuestbook } from './guestbook';
import type { GuestbookEntry } from './api';

const e = (entry_id: string, parent_id: string | null = null): GuestbookEntry => ({
  entry_id, life_id: 'l1', author_agent_id: 'a1', author_name: '이름',
  body: '내용', parent_id, created_at: '2026-07-26T00:00:00Z',
});

describe('groupGuestbook', () => {
  it('원글은 서버 순서(최신순) 유지, 답글은 오래된 순으로 부모 아래 묶인다', () => {
    // 서버 응답은 최신순 — gb_r2가 gb_r1보다 최신
    const entries = [e('gb_r2', 'gb_p1'), e('gb_p2'), e('gb_r1', 'gb_p1'), e('gb_p1')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_p2', 'gb_p1']);
    expect(threads[1].replies.map((r) => r.entry_id)).toEqual(['gb_r1', 'gb_r2']);
  });

  it('parent_id 없는 레거시 행은 전부 top-level', () => {
    const entries = [e('gb_b'), e('gb_a')];
    const threads = groupGuestbook(entries);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_b', 'gb_a']);
    expect(threads.every((t) => t.replies.length === 0)).toBe(true);
  });

  it('부모가 목록에 없는 답글은 top-level로 폴백', () => {
    const threads = groupGuestbook([e('gb_orphan', 'gb_gone')]);
    expect(threads.map((t) => t.entry.entry_id)).toEqual(['gb_orphan']);
    expect(threads[0].replies).toEqual([]);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `cd a-mate && npm test`
Expected: FAIL — `Cannot find module './guestbook'` (또는 resolve 에러). 기존 148개는 PASS 유지.

- [ ] **Step 3: 구현**

`a-mate/src/lib/api.ts` — 두 줄 교체:

```ts
export interface GuestbookEntry { entry_id: string; life_id: string; author_agent_id: string; author_name: string; body: string; parent_id?: string | null; created_at: string }
```

```ts
export const lifeAddGuestbook = (lifeId: string, body: string, parentId?: string) => invoke<GuestbookEntry>('life_add_guestbook', { lifeId, body, parentId });
```

(`parentId`가 `undefined`면 JSON 직렬화에서 키가 빠져 Rust `Option<String>`이 `None`이 된다 — 기존 optional 인자 전례와 동일.)

`a-mate/src/lib/guestbook.ts` 신규:

```ts
import type { GuestbookEntry } from './api';

export interface GuestbookThread { entry: GuestbookEntry; replies: GuestbookEntry[] }

/** 평면 방명록 목록(서버: 최신순) → 원글 스레드 목록.
 *  원글은 서버 순서(최신순) 유지, 답글은 오래된 순(대화 흐름).
 *  부모가 목록에 없는 답글은 방어적으로 top-level 취급(정상 흐름엔 없음 — 서버가 cascade 삭제). */
export function groupGuestbook(entries: GuestbookEntry[]): GuestbookThread[] {
  const ids = new Set(entries.map((e) => e.entry_id));
  const byParent = new Map<string, GuestbookEntry[]>();
  for (const e of entries) {
    if (e.parent_id && ids.has(e.parent_id)) {
      byParent.set(e.parent_id, [...(byParent.get(e.parent_id) ?? []), e]);
    }
  }
  return entries
    .filter((e) => !e.parent_id || !ids.has(e.parent_id))
    .map((entry) => ({ entry, replies: [...(byParent.get(entry.entry_id) ?? [])].reverse() }));
}
```

- [ ] **Step 4: 통과 확인**

Run: `cd a-mate && npm test`
Expected: 전부 PASS (기존 148 + 신규 3)

- [ ] **Step 5: Commit**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/guestbook.ts a-mate/src/lib/guestbook.test.ts
git commit -m "feat(agent): add guestbook thread grouping for reply rendering

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: a-mate 프론트 — GuestbookTab 답글 UI

**Files:**
- Modify: `a-mate/src/lib/ui/GuestbookTab.svelte` (전체 교체 — 현재 10줄 컴포넌트)

**Interfaces:**
- Consumes: Task 5의 `groupGuestbook`·`lifeAddGuestbook(lifeId, body, parentId?)`, 기존 props `{lifeId, meId, isOwner}`.
- Produces: 없음 (리프 컴포넌트). 렌더 테스트는 두지 않는다(레포 관행) — 로직은 Task 5의 Vitest가, 값 배선은 Task 4의 cargo 테스트가 커버.

- [ ] **Step 1: 구현** — `a-mate/src/lib/ui/GuestbookTab.svelte` 전체를 다음으로 교체
  (기존 압축 스타일 유지; 답글 버튼은 `isOwner` 원글에만, 답글엔 없음 — 1-depth):

```svelte
<script lang="ts">
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, type GuestbookEntry } from '../api';
  import { groupGuestbook } from '../guestbook';
  let { lifeId, meId, isOwner=false }: {lifeId:string;meId:string;isOwner?:boolean}=$props();
  let entries=$state<GuestbookEntry[]>([]),text=$state(''),busy=$state(false);
  let replyTo=$state<string|null>(null),replyText=$state('');
  let threads=$derived(groupGuestbook(entries));
  async function load(){entries=(await lifeGuestbook(lifeId)).entries}load();
  async function add(){if(!text.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,text);text='';await load()}finally{busy=false}}
  async function addReply(parentId:string){if(!replyText.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,replyText,parentId);replyText='';replyTo=null;await load()}finally{busy=false}}
  function toggleReply(id:string){replyTo=replyTo===id?null:id;replyText=''}
  async function remove(id:string){await lifeDeleteGuestbook(id);await load()}
</script>
<section>
<form onsubmit={e=>{e.preventDefault();add()}}><input maxlength="500" bind:value={text} placeholder="왔다 간 흔적을 남겨보세요"/><button disabled={busy}>남기기</button></form>
<div class="list">
{#each threads as t (t.entry.entry_id)}
  <article>
    <header><b>{t.entry.author_name}</b><time>{new Date(t.entry.created_at).toLocaleString()}</time>
      <span class="acts">
        {#if isOwner}<button onclick={()=>toggleReply(t.entry.entry_id)}>답글</button>{/if}
        {#if isOwner||t.entry.author_agent_id===meId}<button onclick={()=>remove(t.entry.entry_id)}>삭제</button>{/if}
      </span>
    </header>
    <p>{t.entry.body}</p>
    {#if t.replies.length||replyTo===t.entry.entry_id}
    <div class="replies">
      {#each t.replies as reply (reply.entry_id)}
        <article class="reply">
          <header><b>{reply.author_name}</b><time>{new Date(reply.created_at).toLocaleString()}</time>
            <span class="acts">{#if isOwner||reply.author_agent_id===meId}<button onclick={()=>remove(reply.entry_id)}>삭제</button>{/if}</span>
          </header>
          <p>{reply.body}</p>
        </article>
      {/each}
      {#if replyTo===t.entry.entry_id}
        <form onsubmit={e=>{e.preventDefault();addReply(t.entry.entry_id)}}><input maxlength="500" bind:value={replyText} placeholder="답글을 남겨보세요"/><button disabled={busy}>답글 달기</button></form>
      {/if}
    </div>
    {/if}
  </article>
{:else}<p>아직 방명록이 없어요.</p>{/each}
</div>
</section>
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--pastel-lav);border-radius:8px}button{border:0;border-radius:8px;padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--pastel-cream);border-radius:10px}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list time{color:var(--ink-soft)}.list .acts{margin-left:auto;display:flex;gap:4px}.list header button{padding:3px 7px;background:var(--pastel-lav);color:var(--ink)}.list p{margin:7px 0 0}.replies{margin-top:8px;display:grid;gap:6px;border-left:2px solid var(--pastel-lav);padding-left:10px}.list .reply{padding:8px 10px;background:transparent;border:1px solid var(--pastel-lav);border-radius:8px}</style>
```

(스타일 변경점: 기존 `.list header button{margin-left:auto;...}`의 `margin-left:auto`를 `.acts` 래퍼로 이동 — 버튼이 2개가 됐으므로. 나머지 기존 스타일 그대로 + `.replies`/`.reply` 추가, 기존 CSS 변수만 사용.)

- [ ] **Step 2: 컴파일·무회귀 확인**

Run: `cd a-mate && npm run build && npm test`
Expected: vite build OK (Svelte 템플릿 컴파일 통과), Vitest 전부 PASS

- [ ] **Step 3: Commit**

```bash
git add a-mate/src/lib/ui/GuestbookTab.svelte
git commit -m "feat(agent): add owner reply UI to guestbook tab

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 통합 검증 · PR · docs-archive (DoD)

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md:80` (G2 항목에 완료 표기 — G1 전례: `**G2 — 방명록 답글 (1단계, 중첩 불가)** (백로그 1) — ✅ 완료(2026-07-26, PR #NNN)` 형식, PR 번호는 생성 후 기입)
- 이동: 본 plan + 스펙 → `docs/archive/` (docs-archive 스킬이 수행)

**Interfaces:**
- Consumes: Task 1~6 전부 완료·커밋된 상태.
- Produces: 머지 가능한 PR + 아카이브된 작업 문서.

- [ ] **Step 1: 전체 스위트 (macOS 부분 신호)**

```bash
cd a-hub/life && uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q
cd ../../a-mate && cargo test -p agent-mentor ; cargo check -p agent-mentor-app --tests ; npm test ; npm run build
```

Expected: pytest 전부 PASS / cargo `hosts::tests::host_source_derives_sibling_paths` 1건만 실패(기존 macOS 노이즈, 무시) / cargo check OK / Vitest 전부 PASS / build OK

- [ ] **Step 2: 사용자에게 Windows 권위 실행 요청**

Windows PowerShell(`a-mate/`)에서 `cargo test`(워크스페이스 전체)·`npm test` 결과 확인을 요청하고 통과를 확인받는다 (메모리의 macOS 부분 검증 매트릭스 규칙).

- [ ] **Step 3: 로드맵 완료 표기 커밋**

로드맵 G2 항목 제목 줄에 `— ✅ 완료(2026-07-26, PR #NNN)` 추가 (PR 생성 후 번호 확정 시점에 수행해도 됨 — G1 전례).

- [ ] **Step 4: PR 생성**

superpowers:finishing-a-development-branch 스킬을 사용해 마무리. PR 본문에 스펙·ADR 0021 링크, 배포 순서(서버 먼저) 명시. base: `main`.

- [ ] **Step 5: docs-archive (DoD)**

같은 PR에서 `docs-archive` 스킬 실행 — 본 plan과 스펙(`2026-07-26-guestbook-replies-design.md`)을 `docs/archive/` 미러로 이동 (ADR은 아카이브 대상 아님 — `docs/adr/`에 존속). 규칙: ADR 0013.

```bash
git add -A docs
git commit -m "docs(archive): archive G2 guestbook replies plan and spec

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
