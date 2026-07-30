---
status: done
archived: 2026-07-30
---

# 대문 아웃바운드 게시 구현 계획 (O1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 방문객이 남의 방에 들어갔을 때 그 방 주인의 대문사진과 오늘의 한마디를 같은 자리에서 보게 한다.

**Architecture:** 주인 앱이 자기 화면에 걸린 것(PNG 1장 + 문장 1개)을 life 서버에 게시하고, 방문객은 기존 2초 폴링(`life_state`)이 주는 방 레벨 필드로 그것을 렌더한다. 공개범위 게이트는 두지 않는다(상주 말풍선과 같은 수위). 서버는 판정을 하지 않고 값을 보관·반환만 한다 — 캡션 우선·날짜 만료 같은 해석은 전부 주인 기계 안에서 끝난다.

**Tech Stack:** FastAPI + SQLite (a-hub/life) · Rust/Tauri v2 + `ureq` (a-mate `src-tauri`, `crates/core`) · Svelte 5 + Vitest (a-mate `src`)

**스펙:** [2026-07-30-front-door-outbound-design.md](../specs/2026-07-30-front-door-outbound-design.md)

## Global Constraints

- **불변식**: 방문객이 보는 대문 = 그 방 주인이 자기 화면에서 보는 대문. 게시 단위는 **사진 1장 + 문장 1개**.
- **공개범위 게이트를 만들지 않는다.** `content_visibility`의 `feature != "diary"` 하드 거부는 그대로 둔다.
- **대문 데이터는 방(life) 레벨로 노출한다** — 주인이 남의 방에 놀러 가 있어도 대문은 걸려 있어야 한다.
- 한마디 자수 상한 **120자**(`strip()` 후). PNG 상한 **5 MiB** + PNG 시그니처 검증. 둘 다 말풍선·마스코트 이미지와 같은 값.
- **`pipeline.rs`·`sprite.rs` 미접촉** — 이 작업은 게시와 렌더만 한다.
- **`contracts/` 미접촉** — life 계약 정본은 `docs/design/life-visit.md §4`.
- 주석은 **한국어**, 기존 파일 스타일에 맞춘다. 이모지·과잉 주석 금지.
- 커밋은 Conventional Commits **영어**. scope: a-hub=`backend`, a-mate=`agent`, 문서=`docs`.
  푸터 `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- 실행 위치: a-hub는 `a-hub/life/`에서 `.venv\Scripts\python.exe -m pytest -q`,
  a-mate는 **네이티브 Windows PowerShell**의 `a-mate/`에서 (WSL 금지).

## Task 0: 베이스라인 확인 (구현 전 필수)

새 워크트리는 main이 깨져 있을 수 있다는 전제로 시작한다(#125 실제 사례).

- [ ] **Step 1: life 서버 베이스라인**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest -q`
Expected: `81 passed` (2026-07-30 실측값). 실패하면 **구현 시작 전에** 원인을 보고할 것.

- [ ] **Step 2: a-mate 베이스라인**

Run (PowerShell, `a-mate/`에서):
```powershell
cargo test
npm test
```
Expected: 둘 다 통과. `npm test`는 189건 근방(묶음 ④ 시점 실측 189/189). 실패하면 보고 후 중단.

커밋 없음 — 확인만 한다.

---

## Task 1: ADR 0026 + 계약 문서 (문서 우선)

**Files:**
- Create: `docs/adr/0026-front-door-outbound-publication.md`
- Modify: `docs/design/life-visit.md:56` 뒤 (§4 표에 3행 추가)

**Interfaces:**
- Consumes: 없음
- Produces: 이후 모든 태스크가 구현할 엔드포인트 계약 3행 —
  `PATCH /life/me/daily-line` · `PUT /life/me/daily-cut` · `GET /life/agents/{id}/daily-cut`

- [ ] **Step 1: ADR 0026 작성**

`docs/adr/0026-front-door-outbound-publication.md`:

```markdown
# ADR 0026: 로컬 생성 대문(사진·한마디)을 공개범위 게이트 없이 life 서버에 게시한다

- 상태: 채택
- 날짜: 2026-07-30
- 대상: a-mate가 생성한 대문사진·오늘의 한마디의 외부 게시 경계
- 관련: [ADR 0019](0019-owner-memory-transmission-boundary.md)(주인 메모리),
  [ADR 0024](0024-image-engine-material-boundary.md)(이미지 엔진),
  [ADR 0025](0025-neighbour-content-transmission-boundary.md)(이웃 일기),
  [설계 스펙](../design/a-mate/specs/2026-07-30-front-door-outbound-design.md)

## 배경

ADR 0019·0024·0025는 모두 소재가 **LLM 엔진으로** 흐르는 수위를 정한 결정이었다. 엔진은
사내 on-prem이 기본이고, 전송된 내용을 다른 사람이 읽지는 않는다.

대문사진(`daily_cut.png`)·오늘의 한마디는 a-mate 로컬 생성물이라 life 서버에 없고, 그래서
방문객에게 보이지 않는다(`App.svelte`가 `!visiting`으로 카드를 숨긴다). 이를 해결하려면
로컬 생성물을 **다른 사람이 읽는 서버**로 내보내야 한다 — 전송 대상이 처음으로 엔진이 아닌
새 범주이므로 ADR로 남긴다.

## 결정

- **공개범위 게이트를 두지 않는다.** `content_visibility`(다이어리 공개 범위)를 재사용하지도,
  새 범위를 만들지도 않는다. 상주 말풍선(`PATCH /life/me/bubble`)과 같은 수위다.
- **게시 값은 주인 화면의 표시 문장·그림 그대로.** 서버는 판정하지 않고 보관·반환만 한다.
  캡션 우선 규칙과 "오늘" 판정은 전부 주인 기계 안에 남는다 — 서버는 주인의 시간대를 모른다.
- **저장은 agent 키, 노출은 방(life) 레벨.** 주인이 남의 방에 가 있어도 대문은 걸려 있어야
  하므로 `occupants[].bubble`처럼 agent 레벨에 둘 수 없다(`owner_mascot_image_sha256` 선례).
- **무인증 노출을 수용한다.** `GET /life/{id}`는 무인증이므로 한마디 텍스트는 토큰 없이 읽힌다.
  지금도 `bubble`·`owner_mascot_image_sha256`이 같은 조건으로 노출된다. 이미지 본문 GET은
  기존 마스코트 이미지와 같은 Bearer 수위로 둔다.
- **삭제 API를 두지 않는다.** 로컬에서 자동 생성을 꺼도 로컬 컷은 계속 걸려 있고 주인 화면도
  계속 보여주므로, 서버가 유지하는 쪽이 불변식과 일치한다.

## 대안

- **`content_visibility`에 새 feature 키 추가**(`home` 1개 또는 `daily_cut`/`daily_line` 2개):
  스펙(2026-07-22 Life 소셜)이 "다이어리는 첫 적용 대상일 뿐"이라 예고한 설계된 확장 지점이고
  서버 스키마 변경도 없다. 기각 — 대문은 미니홈피의 가장 공개적인 표면이라는 판단.
- **기존 `diary` 키 편승**: 새 서버 표면이 0이지만 "일기를 공개하면 대문도 공개된다"는 묶임이
  생기고, 오늘의 한마디는 일기 파생이 아니라 활동 무드 파생이어서 의미가 어긋난다. 기각.

## 결과

- 대문사진·오늘의 한마디가 life 서버에 저장되고 방문객(및 무인증 조회자, 텍스트에 한해)에게
  노출된다.
- **대문사진은 생성 자체가 옵트인**(`daily_cut_enabled` 기본 off)이지만 **오늘의 한마디는
  생성 토글이 없다** — 엔진만 설정돼 있으면 무조건 생성되므로, 사전 동의 없이 게시되는 첫
  자동 생성물이 된다. 이 성질을 명시적으로 수용한다.
- 공개를 멈추려면 지금은 Life 연결 종료가 유일한 수단이다. 사용자 통제 수단 추가는 후속 과제.
- 트랜스크립트 원문 비전송 원칙은 그대로 유지된다(게시물은 40자 이내 파생 문장과 추상 그림뿐).
```

- [ ] **Step 2: `life-visit.md §4` 표에 3행 추가**

`docs/design/life-visit.md`의 56행(`GET /life/me/visits...`) **바로 뒤**에 삽입:

```markdown
| PATCH | `/life/me/daily-line` | 대문에 걸린 오늘의 한마디 게시. `{body}` — `strip()` 후 120자 이하, 빈 문자열 = 지움. 응답 `{daily_line}`. 노출은 `GET /life/{id}`의 `owner_daily_line`(방 레벨 — 주인이 자리를 비워도 걸려 있다) (O1) |
| PUT | `/life/me/daily-cut` | 대문사진 게시. raw PNG 바디(`Content-Type: image/png`), PNG 시그니처 + 5 MiB 검증, sha256이 같으면 쓰기 생략. 응답 `{sha256, size}`. 노출은 `GET /life/{id}`의 `owner_daily_cut_sha256` (O1) |
| GET | `/life/agents/{agent_id}/daily-cut` | 대문사진 본문. Bearer 필요. `image/png` + `ETag` + `Cache-Control: private, max-age=300`. 없으면 404 (O1) |
```

- [ ] **Step 3: 링크 확인**

Run: `git diff --stat`
Expected: `docs/adr/0026-front-door-outbound-publication.md` 신규 + `docs/design/life-visit.md` 수정.
ADR 안의 상대 링크는 `docs/adr/`에서 출발하므로 `0019-...md`(같은 폴더)와
`../design/a-mate/specs/...`(한 단계 위로)가 맞는지 눈으로 확인한다.

- [ ] **Step 4: 커밋**

```bash
git add docs/adr/0026-front-door-outbound-publication.md docs/design/life-visit.md
git commit -m "docs(docs): record ADR 0026 and contract rows for front-door publication"
```

---

## Task 2: life 서버 — 대문 한마디 (텍스트)

**Files:**
- Modify: `a-hub/life/life_server/store.py` (`_SCHEMA` agents 테이블 · `__init__` 마이그레이션 · `load()` · `_save_agent_row`)
- Modify: `a-hub/life/life_server/life.py` (`LifeAgent` · `life_state` · `set_daily_line` 신규)
- Modify: `a-hub/life/life_server/api.py` (`PATCH /life/me/daily-line`)
- Test: `a-hub/life/tests/test_life.py` · `a-hub/life/tests/test_api.py` · `a-hub/life/tests/test_store.py`

**Interfaces:**
- Consumes: Task 1의 계약 3행 중 `PATCH /life/me/daily-line`
- Produces:
  - `LifeService.set_daily_line(token: str | None, body: str) -> dict` — `{"daily_line": str}`
  - `life_state()` 응답에 `"owner_daily_line": str` (미설정이면 `""`)
  - `LifeAgent.daily_line: str`
  - `SqliteStore`: `agents.daily_line` 컬럼 (가산 마이그레이션)

- [ ] **Step 1: 실패하는 테스트를 쓴다 (서비스)**

`a-hub/life/tests/test_life.py` 맨 끝에 추가:

```python
def test_daily_line_is_room_level_and_survives_owner_leaving():
    """O1 — 대문 한마디는 방 레벨이다. 주인이 남의 방에 가 있어도 그 방 대문에 걸려 있어야 한다."""
    service = LifeService()
    _, owner_token, owner_life = service.register("owner")
    _, visitor_token, visitor_life = service.register("visitor")

    assert service.life_state(owner_life.id)["owner_daily_line"] == ""

    assert service.set_daily_line(owner_token, "  밤샘 끝, 뿌듯  ") == {"daily_line": "밤샘 끝, 뿌듯"}
    assert service.life_state(owner_life.id)["owner_daily_line"] == "밤샘 끝, 뿌듯"

    # 주인이 방문자의 방으로 이동 — 자기 방 대문은 그대로 걸려 있다
    service.enter(owner_token, visitor_life.id, None)
    assert service.life_state(owner_life.id)["owner_daily_line"] == "밤샘 끝, 뿌듯"

    # 빈 문자열은 지움
    assert service.set_daily_line(owner_token, "   ") == {"daily_line": ""}
    assert service.life_state(owner_life.id)["owner_daily_line"] == ""

    with pytest.raises(errors.InvalidRequest):
        service.set_daily_line(owner_token, "가" * 121)
    # 남의 대문은 건드릴 수 없다 — 토큰이 곧 대상이므로 방문자 토큰은 자기 것만 바꾼다
    service.set_daily_line(visitor_token, "나도 한마디")
    assert service.life_state(owner_life.id)["owner_daily_line"] == ""
    assert service.life_state(visitor_life.id)["owner_daily_line"] == "나도 한마디"
```

- [ ] **Step 2: 실패를 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_life.py::test_daily_line_is_room_level_and_survives_owner_leaving -q`
Expected: FAIL — `AttributeError: 'LifeService' object has no attribute 'set_daily_line'`

- [ ] **Step 3: `LifeAgent`에 필드를 추가한다**

`a-hub/life/life_server/life.py`의 `LifeAgent` 데이터클래스에서 `bubble: str = ""` **바로 뒤**:

```python
    bubble: str = ""
    # O1 대문 아웃바운드 — 대문에 걸린 오늘의 한마디. 저장은 agent 키, 노출은 방 레벨
    # (owner_mascot_image_sha256 선례 — 주인이 자리를 비워도 대문은 걸려 있어야 한다).
    daily_line: str = ""
    connected: bool = True
```

- [ ] **Step 4: `set_daily_line`을 추가한다**

`life.py`의 `set_bubble` **바로 뒤**:

```python
    def set_daily_line(self, token: str | None, body: str) -> dict:
        """O1 — 대문에 걸린 오늘의 한마디. 빈 문자열은 지움 (set_bubble과 동일 규율).

        값의 해석(캡션 우선·"오늘" 판정)은 전부 클라이언트가 한다 — 서버는 주인의 시간대를
        모르므로 문자열을 보관·반환만 하고, 방문객은 주인 화면과 같은 문장을 본다."""
        agent = self._authed(token)
        body = body.strip()
        if len(body) > 120:
            raise errors.InvalidRequest("대문 한마디는 120자 이하여야 함")
        with self._lock:
            agent.daily_line = body
            if self._store:
                self._store.save_agent(agent)
        return {"daily_line": body}
```

- [ ] **Step 5: `life_state`에 노출 필드를 추가한다**

`life.py`의 `life_state` 반환 dict에서 `"owner_mascot_image_sha256": ...` **바로 뒤**:

```python
                "owner_mascot_image_sha256": self._mascot_image_hashes.get(life.owner_agent_id),
                # O1 대문 — 방 레벨. 주인이 남의 방에 가 있어도 대문은 이 방에 걸려 있다.
                "owner_daily_line": owner.daily_line if owner else "",
```

- [ ] **Step 6: 테스트가 통과하는지 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_life.py::test_daily_line_is_room_level_and_survives_owner_leaving -q`
Expected: PASS

- [ ] **Step 7: 영속화 테스트를 쓴다**

`a-hub/life/tests/test_store.py` 맨 끝에 추가:

```python
def test_daily_line_survives_restart(tmp_path):
    """O1 — 대문 한마디는 agents 컬럼에 영속된다."""
    db = str(tmp_path / "life-daily-line.db")
    service = LifeService(store=SqliteStore(db))
    _, token, created_life = service.register("front-door-owner")
    service.set_daily_line(token, "오늘도 묵묵히")

    restarted = LifeService(store=SqliteStore(db))
    assert restarted.life_state(created_life.id)["owner_daily_line"] == "오늘도 묵묵히"


def test_legacy_agents_table_gains_daily_line_column(tmp_path):
    """O1 — daily_line 컬럼이 없는 기존 DB를 열어도 가산 마이그레이션되어 로드된다."""
    db = str(tmp_path / "legacy-agents.db")
    con = sqlite3.connect(db)
    con.execute(
        "CREATE TABLE agents (agent_id TEXT PRIMARY KEY, name TEXT NOT NULL, life_id TEXT NOT NULL, "
        "at_life TEXT NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, "
        "mascot_seed TEXT NOT NULL DEFAULT '')"
    )
    con.execute("INSERT INTO agents VALUES ('ragt_old', '옛사람', 'life_old', 'life_old', 3, 4, 'seed')")
    con.commit()
    con.close()

    store = SqliteStore(db)
    columns = {row[1] for row in store._conn.execute("PRAGMA table_info(agents)")}
    assert "daily_line" in columns
    _, agents, _ = store.load()
    assert agents["ragt_old"].daily_line == ""
```

- [ ] **Step 8: 실패를 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_store.py -q -k daily_line`
Expected: FAIL — 첫 테스트는 재시작 후 `""`, 두 번째는 `assert "daily_line" in columns` 실패.

- [ ] **Step 9: `store.py` 4곳을 고친다**

**(a)** `_SCHEMA`의 `agents` 테이블에서 `bubble TEXT NOT NULL DEFAULT '',` 뒤에 한 줄:

```sql
  bubble      TEXT NOT NULL DEFAULT '',
  daily_line  TEXT NOT NULL DEFAULT '',
  connected   INTEGER NOT NULL DEFAULT 1
```

**(b)** `__init__`의 마이그레이션 블록 — `hub_user_id` for 루프 **바로 뒤**:

```python
        # O1 대문 아웃바운드 — 대문에 걸린 오늘의 한마디 (스펙 §1.1)
        if "daily_line" not in agent_columns:
            self._conn.execute("ALTER TABLE agents ADD COLUMN daily_line TEXT NOT NULL DEFAULT ''")
```

**(c)** `load()`의 agents 딕셔너리 — kwarg·언팩·SELECT 세 곳을 함께 고친다:

```python
        agents = {
            agent_id: LifeAgent(
                agent_id=agent_id, name=name, life_id=life_id, at_life=at_life,
                cell=(x, y), mascot_seed=mascot_seed, org=org, agent_uuid=agent_uuid,
                owner_os_user=owner_os_user, owner_full_name=owner_full_name,
                hub_user_id=hub_user_id, bubble=bubble, daily_line=daily_line,
                connected=bool(connected),
            )
            for (agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid,
                 owner_os_user, owner_full_name, hub_user_id, bubble, daily_line, connected) in c.execute(
                "SELECT agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid, "
                "owner_os_user, owner_full_name, hub_user_id, bubble, daily_line, connected FROM agents"
            )
        }
```

**(d)** `_save_agent_row` — 컬럼 목록·placeholder 개수·`DO UPDATE SET`·파라미터 튜플 네 곳 모두:

```python
    def _save_agent_row(self, agent: LifeAgent) -> None:
        self._conn.execute(
            "INSERT INTO agents (agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid, "
            "owner_os_user, owner_full_name, hub_user_id, bubble, daily_line, connected) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(agent_id) DO UPDATE SET "
            "name = excluded.name, at_life = excluded.at_life, x = excluded.x, y = excluded.y, "
            "mascot_seed = excluded.mascot_seed, org = excluded.org, agent_uuid = excluded.agent_uuid, "
            "owner_os_user = excluded.owner_os_user, owner_full_name = excluded.owner_full_name, "
            "hub_user_id = excluded.hub_user_id, "
            "bubble = excluded.bubble, daily_line = excluded.daily_line, connected = excluded.connected",
            (
                agent.agent_id, agent.name, agent.life_id, agent.at_life,
                agent.cell[0], agent.cell[1], agent.mascot_seed, agent.org, agent.agent_uuid,
                agent.owner_os_user, agent.owner_full_name, agent.hub_user_id,
                agent.bubble, agent.daily_line, int(agent.connected),
            ),
```

> ⚠️ placeholder는 **15개**다. `VALUES` 괄호 안의 `?` 개수와 파라미터 튜플 길이가 컬럼 수와
> 모두 일치하는지 세어 볼 것 — 어긋나면 `sqlite3.ProgrammingError`가 난다.

- [ ] **Step 10: 영속화 테스트가 통과하는지 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_store.py -q -k daily_line`
Expected: PASS (2 passed)

- [ ] **Step 11: REST 테스트를 쓴다**

`a-hub/life/tests/test_api.py` 맨 끝에 추가:

```python
def test_daily_line_patch_echoes_trims_and_limits(client):
    """O1 — PATCH /life/me/daily-line: 에코 + strip + 120자 상한 + 무토큰 401."""
    a = _register(client, "front-door")
    h = {"Authorization": f"Bearer {a['token']}"}

    r = client.patch("/life/me/daily-line", json={"body": "  오늘도 묵묵히  "}, headers=h)
    assert r.status_code == 200
    assert r.json() == {"daily_line": "오늘도 묵묵히"}
    assert client.get(f"/life/{a['life_id']}").json()["owner_daily_line"] == "오늘도 묵묵히"

    over = client.patch("/life/me/daily-line", json={"body": "가" * 121}, headers=h)
    assert over.status_code == 400
    assert over.json()["error"]["code"] == "invalid_request"

    assert client.patch("/life/me/daily-line", json={"body": "무토큰"}).status_code == 401
```

- [ ] **Step 12: 실패를 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_api.py::test_daily_line_patch_echoes_trims_and_limits -q`
Expected: FAIL — 405 또는 404 (라우트 없음)

- [ ] **Step 13: 라우트를 추가한다**

`a-hub/life/life_server/api.py`의 `life_bubble`(`@app.patch("/life/me/bubble")`) **바로 뒤**:

```python
    @app.patch("/life/me/daily-line")
    def life_daily_line(body: TextBody, authorization: str | None = Header(default=None)):
        # O1 대문 — 노출은 GET /life/{id}의 owner_daily_line (방 레벨)
        return life.set_daily_line(_bearer(authorization), body.body)
```

- [ ] **Step 14: 전체 테스트를 돌린다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest -q`
Expected: 기존 81건 + 신규 4건 = `85 passed`

- [ ] **Step 15: 커밋**

```bash
git add a-hub/life/life_server/store.py a-hub/life/life_server/life.py a-hub/life/life_server/api.py a-hub/life/tests/test_life.py a-hub/life/tests/test_api.py a-hub/life/tests/test_store.py
git commit -m "feat(backend): publish front-door daily line at room level"
```

---

## Task 3: life 서버 — 대문사진 (이미지)

**Files:**
- Modify: `a-hub/life/life_server/store.py` (`_SCHEMA`에 `daily_cuts` 테이블 + 3개 메서드)
- Modify: `a-hub/life/life_server/life.py` (`_daily_cut_hashes` · `set_daily_cut` · `daily_cut` · `life_state`)
- Modify: `a-hub/life/life_server/api.py` (`PUT /life/me/daily-cut` · `GET /life/agents/{id}/daily-cut`)
- Test: `a-hub/life/tests/test_life.py` · `a-hub/life/tests/test_api.py` · `a-hub/life/tests/test_store.py`

**Interfaces:**
- Consumes: Task 1의 계약 3행 중 PUT·GET 두 행. Task 2의 `life_state` 확장(같은 dict에 필드를 하나 더 얹는다)
- Produces:
  - `LifeService.set_daily_cut(token: str | None, png: bytes) -> dict` — `{"sha256": str, "size": int}`
  - `LifeService.daily_cut(token: str | None, agent_id: str) -> tuple[bytes, str]` — `(png, sha256)`
  - `life_state()` 응답에 `"owner_daily_cut_sha256": str | None`
  - `SqliteStore`: `save_daily_cut(agent_id, png, sha256, updated_at)` · `daily_cut(agent_id) -> tuple[bytes,str] | None` · `load_daily_cut_hashes() -> dict[str,str]`

- [ ] **Step 1: 실패하는 테스트를 쓴다 (서비스)**

`a-hub/life/tests/test_life.py` 맨 끝에 추가:

```python
def test_daily_cut_requires_png_and_dedups(tmp_path):
    """O1 — 대문사진: PNG 시그니처·5 MiB 검증, 같은 sha면 쓰기 생략, 방 레벨 해시 노출."""
    from life_server.store import SqliteStore

    service = LifeService(store=SqliteStore(str(tmp_path / "cut.db")))
    agent, token, created_life = service.register("cut-owner")
    _, other_token, _ = service.register("other")
    png = b"\x89PNG\r\n\x1a\nfront-door-cut"

    assert service.life_state(created_life.id)["owner_daily_cut_sha256"] is None

    saved = service.set_daily_cut(token, png)
    assert saved["size"] == len(png)
    assert service.life_state(created_life.id)["owner_daily_cut_sha256"] == saved["sha256"]
    assert service.daily_cut(other_token, agent.agent_id) == (png, saved["sha256"])

    # 같은 바이트 재업로드는 같은 해시 (쓰기 생략 경로도 응답은 동일)
    assert service.set_daily_cut(token, png)["sha256"] == saved["sha256"]

    with pytest.raises(errors.InvalidRequest):
        service.set_daily_cut(token, b"not-a-png")
    with pytest.raises(errors.InvalidRequest):
        service.set_daily_cut(token, b"\x89PNG\r\n\x1a\n" + b"x" * (5 * 1024 * 1024))
    with pytest.raises(errors.NotFound):
        service.daily_cut(token, "ragt_nope")


def test_daily_cut_missing_is_not_found():
    """O1 — 컷을 안 올린 사람의 대문사진 조회는 404 (클라이언트는 마스코트 이미지로 폴백)."""
    service = LifeService()
    agent, token, _ = service.register("no-cut")
    with pytest.raises(errors.NotFound):
        service.daily_cut(token, agent.agent_id)
```

- [ ] **Step 2: 실패를 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_life.py -q -k daily_cut`
Expected: FAIL — `AttributeError: 'LifeService' object has no attribute 'set_daily_cut'`

- [ ] **Step 3: `store.py`에 테이블과 메서드를 추가한다**

**(a)** `_SCHEMA`의 `mascot_images` 테이블 정의 **바로 뒤**:

```sql
CREATE TABLE IF NOT EXISTS daily_cuts (
  agent_id    TEXT PRIMARY KEY,
  png         BLOB NOT NULL,
  sha256      TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
```

**(b)** `load_mascot_image_hashes` **바로 뒤**:

```python
    # O1 대문사진 — mascot_images와 동형(BLOB은 별 테이블). 노출은 방 레벨 해시.
    def save_daily_cut(self, agent_id: str, png: bytes, sha256: str, updated_at: str) -> None:
        self._conn.execute(
            "INSERT INTO daily_cuts VALUES (?, ?, ?, ?) ON CONFLICT(agent_id) DO UPDATE SET "
            "png = excluded.png, sha256 = excluded.sha256, updated_at = excluded.updated_at",
            (agent_id, png, sha256, updated_at),
        )
        self._conn.commit()

    def daily_cut(self, agent_id: str) -> tuple[bytes, str] | None:
        row = self._conn.execute("SELECT png, sha256 FROM daily_cuts WHERE agent_id = ?", (agent_id,)).fetchone()
        return (bytes(row[0]), row[1]) if row else None

    def load_daily_cut_hashes(self) -> dict[str, str]:
        return dict(self._conn.execute("SELECT agent_id, sha256 FROM daily_cuts"))
```

- [ ] **Step 4: `life.py`에 상태·메서드·노출을 추가한다**

**(a)** `LifeService.__init__`의 `self._mascot_image_hashes: dict[str, str] = {}` **바로 뒤**:

```python
        self._mascot_image_hashes: dict[str, str] = {}
        self._daily_cut_hashes: dict[str, str] = {}
```

**(b)** 같은 `__init__`의 store 복원 블록에서 `self._mascot_image_hashes = store.load_mascot_image_hashes()` **바로 뒤**:

```python
            self._mascot_image_hashes = store.load_mascot_image_hashes()
            self._daily_cut_hashes = store.load_daily_cut_hashes()
```

**(c)** `life_state` 반환 dict의 `"owner_daily_line": ...` **바로 뒤**:

```python
                "owner_daily_line": owner.daily_line if owner else "",
                "owner_daily_cut_sha256": self._daily_cut_hashes.get(life.owner_agent_id),
```

**(d)** `mascot_image` 메서드 **바로 뒤**에 두 메서드:

```python
    def set_daily_cut(self, token: str | None, png: bytes) -> dict:
        """O1 — 대문사진 게시 (set_mascot_image와 동일 규율). 같은 sha면 쓰기를 생략한다.

        마스코트 이미지와 **다른 슬롯**이어야 한다 — mascot_images는 방 안 점유자 로봇 렌더에도
        쓰이므로, 컷을 그 슬롯에 넣으면 방 안 로봇이 컷 그림으로 바뀐다."""
        me = self._authed(token)
        if not png.startswith(b"\x89PNG\r\n\x1a\n"):
            raise errors.InvalidRequest("daily cut must be a PNG")
        if len(png) > 5 * 1024 * 1024:
            raise errors.InvalidRequest("daily cut exceeds 5 MiB")
        digest = hashlib.sha256(png).hexdigest()
        with self._lock:
            if not self._store:
                raise errors.InvalidRequest("daily cut storage is unavailable")
            current = self._store.daily_cut(me.agent_id)
            if current is None or current[1] != digest:
                self._store.save_daily_cut(me.agent_id, png, digest, datetime.now(timezone.utc).isoformat())
                self._daily_cut_hashes[me.agent_id] = digest
        return {"sha256": digest, "size": len(png)}

    def daily_cut(self, token: str | None, agent_id: str) -> tuple[bytes, str]:
        """O1 — 방문객이 방 주인의 대문사진을 가져온다. 없으면 404 (클라이언트가 폴백)."""
        self._authed(token)
        with self._lock:
            if agent_id not in self._agents:
                raise errors.NotFound(f"agent '{agent_id}' not found")
            row = self._store.daily_cut(agent_id) if self._store else None
            if row is None:
                raise errors.NotFound("daily cut not found")
            return row
```

- [ ] **Step 5: 서비스 테스트가 통과하는지 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_life.py -q -k daily_cut`
Expected: PASS (2 passed)

- [ ] **Step 6: 영속화 테스트를 쓴다**

`a-hub/life/tests/test_store.py` 맨 끝에 추가:

```python
def test_daily_cut_png_survives_restart(tmp_path):
    """O1 — 대문사진 PNG와 방 레벨 해시가 재시작을 견딘다."""
    db = str(tmp_path / "life-daily-cut.db")
    service = LifeService(store=SqliteStore(db))
    agent, token, created_life = service.register("cut-owner")
    png = b"\x89PNG\r\n\x1a\nrestart-me"
    saved = service.set_daily_cut(token, png)

    restarted = LifeService(store=SqliteStore(db))
    assert restarted.daily_cut(token, agent.agent_id)[0] == png
    assert restarted.life_state(created_life.id)["owner_daily_cut_sha256"] == saved["sha256"]
```

- [ ] **Step 7: 실패를 확인하고, 통과시킨다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_store.py::test_daily_cut_png_survives_restart -q`
Expected: **PASS 즉시** — Step 3·4에서 `load_daily_cut_hashes` 복원까지 이미 구현했다.
FAIL이 나면 `__init__`의 store 복원 블록에 `load_daily_cut_hashes()` 한 줄이 빠진 것이다(Step 4b).

- [ ] **Step 8: REST 테스트를 쓴다**

`a-hub/life/tests/test_api.py`의 **import 블록**에 두 줄을 더한다:

```python
from life_server.api import create_app
from life_server.life import LifeService
from life_server.store import SqliteStore
```

`client` 픽스처 **바로 뒤**에 store 있는 픽스처를 추가한다:

```python
@pytest.fixture
def db_client(tmp_path) -> TestClient:
    """PNG 저장이 필요한 엔드포인트용 — store 없는 인메모리 서비스는 400을 낸다."""
    return TestClient(create_app(LifeService(store=SqliteStore(str(tmp_path / "api.db")))))
```

파일 맨 끝에 테스트를 추가한다:

```python
def test_daily_cut_put_and_get_with_etag(db_client):
    """O1 — PUT은 raw PNG, GET은 image/png + ETag. 무토큰 GET은 401, 미업로드는 404."""
    a = _register(db_client, "cut-a")
    b = _register(db_client, "cut-b")
    ha = {"Authorization": f"Bearer {a['token']}"}
    hb = {"Authorization": f"Bearer {b['token']}"}
    png = b"\x89PNG\r\n\x1a\napi-cut"

    put = db_client.put("/life/me/daily-cut", content=png,
                        headers={**ha, "Content-Type": "image/png"})
    assert put.status_code == 200
    assert put.json()["size"] == len(png)
    sha = put.json()["sha256"]
    assert db_client.get(f"/life/{a['life_id']}").json()["owner_daily_cut_sha256"] == sha

    # 방문객(b)이 주인(a)의 대문사진을 가져온다
    got = db_client.get(f"/life/agents/{a['agent_id']}/daily-cut", headers=hb)
    assert got.status_code == 200
    assert got.headers["content-type"] == "image/png"
    assert got.headers["etag"] == f'"{sha}"'
    assert got.content == png

    assert db_client.get(f"/life/agents/{a['agent_id']}/daily-cut").status_code == 401
    assert db_client.get(f"/life/agents/{b['agent_id']}/daily-cut", headers=ha).status_code == 404

    bad = db_client.put("/life/me/daily-cut", content=b"nope",
                        headers={**ha, "Content-Type": "image/png"})
    assert bad.status_code == 400
```

- [ ] **Step 9: 실패를 확인한다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest tests/test_api.py::test_daily_cut_put_and_get_with_etag -q`
Expected: FAIL — 405 또는 404 (라우트 없음)

- [ ] **Step 10: 라우트 2개를 추가한다**

`a-hub/life/life_server/api.py`의 `life_mascot_image_get`(`@app.get("/life/agents/{agent_id}/mascot-image")`) **바로 뒤**:

```python
    @app.put("/life/me/daily-cut")
    def life_daily_cut_put(png: bytes = Body(media_type="image/png"), authorization: str | None = Header(default=None)):
        return life.set_daily_cut(_bearer(authorization), png)

    @app.get("/life/agents/{agent_id}/daily-cut")
    def life_daily_cut_get(agent_id: str, authorization: str | None = Header(default=None)):
        png, digest = life.daily_cut(_bearer(authorization), agent_id)
        return Response(content=png, media_type="image/png", headers={"ETag": f'"{digest}"', "Cache-Control": "private, max-age=300"})
```

> 위치가 중요하다 — `@app.get("/life/{life_id}")`보다 **앞**에 등록해야 한다(마스코트 이미지가
> 같은 이유로 그 앞에 있다).

- [ ] **Step 11: 전체 테스트를 돌린다**

Run: `cd a-hub/life && .venv\Scripts\python.exe -m pytest -q`
Expected: Task 2 종료 시점 85건 + 신규 4건 = `89 passed`

- [ ] **Step 12: 커밋**

```bash
git add a-hub/life/life_server/store.py a-hub/life/life_server/life.py a-hub/life/life_server/api.py a-hub/life/tests/test_life.py a-hub/life/tests/test_api.py a-hub/life/tests/test_store.py
git commit -m "feat(backend): store and serve front-door daily cut images"
```

---

## Task 4: a-mate Rust — HTTP 래퍼와 커맨드

**Files:**
- Modify: `a-mate/crates/core/src/life_client.rs` (`mascot_image` 뒤에 3개 메서드)
- Modify: `a-mate/src-tauri/src/commands.rs` (`upload_cached_mascot` 뒤 헬퍼 1개 + `life_mascot_image` 뒤 커맨드 3개)
- Modify: `a-mate/src-tauri/src/lib.rs:439` (`commands::life_mascot_image` 뒤 등록 3줄)

**Interfaces:**
- Consumes: Task 2·3의 엔드포인트 3개
- Produces (프론트가 `invoke`로 부르는 이름과 시그니처):
  - `life_set_daily_line(body: String) -> Result<bool, String>`
  - `life_sync_daily_cut() -> Result<bool, String>`
  - `life_daily_cut(agent_id: String) -> Result<Option<String>, String>` (base64 PNG)

> **단위 테스트를 만들지 않는다.** `life_client.rs`의 기존 테스트는 순수 바디 빌더만 검증하고
> `set_bubble`·`upload_mascot_image`도 테스트가 없다. HTTP 래퍼의 실질 검증은 Task 2·3의
> pytest와 Task 8의 실환경 스모크가 담당한다. 이 태스크의 게이트는 **컴파일 통과 + 회귀 없음**이다.

- [ ] **Step 1: `life_client.rs`에 메서드 3개를 추가한다**

`mascot_image` 메서드 **바로 뒤**(즉 `disconnect` 앞):

```rust
    /// O1 — 대문에 걸린 오늘의 한마디 게시. 빈 문자열은 지움 (set_bubble 동형).
    pub fn set_daily_line(&self, body: &str) -> Result<Value> {
        self.req("PATCH", "/life/me/daily-line")
            .send_json(json!({"body": body})).map_err(err_of)?.into_json().map_err(Into::into)
    }

    /// O1 — 대문사진(PNG) 게시. 서버가 sha256이 같으면 쓰기를 생략한다 (upload_mascot_image 동형).
    pub fn upload_daily_cut(&self, png: &[u8]) -> Result<Value> {
        self.req("PUT", "/life/me/daily-cut")
            .set("Content-Type", "image/png")
            .send_bytes(png).map_err(err_of)?.into_json().map_err(Into::into)
    }

    /// O1 — 방문 중인 방 주인의 대문사진. 없으면 Ok(None) — 구서버의 404도 같게 처리해
    /// 클라이언트가 마스코트 이미지로 폴백한다 (mascot_image 동형).
    pub fn daily_cut(&self, agent_id: &str) -> Result<Option<Vec<u8>>> {
        use std::io::Read as _;
        match self.req("GET", &format!("/life/agents/{agent_id}/daily-cut")).call() {
            Ok(response) => {
                let mut bytes = Vec::new();
                response.into_reader().read_to_end(&mut bytes)?;
                Ok(Some(bytes))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(error) => Err(err_of(error)),
        }
    }
```

- [ ] **Step 2: `commands.rs`에 업로드 헬퍼를 추가한다**

`upload_cached_mascot` **바로 뒤**:

```rust
/// O1 — 캐시된 대문사진을 서버에 게시. 컷이 아직 없으면 Ok(false)(업로드할 게 없음 = 실패 아님).
/// PNG 바이트를 프론트로 왕복시키지 않기 위해 Rust가 파일을 직접 읽는다.
fn upload_cached_daily_cut(app: &tauri::AppHandle, client: &LifeClient) -> Result<bool, String> {
    use tauri::Manager as _;
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("daily_cut.png");
    let Ok(png) = std::fs::read(path) else { return Ok(false) };
    client.upload_daily_cut(&png).map(|_| true).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: `commands.rs`에 커맨드 3개를 추가한다**

`life_mascot_image` 커맨드 **바로 뒤**:

```rust
/// O1 — 대문 한마디 게시. **자동 경로**이므로 hub 미연결이면 조용히 Ok(false)
/// (life_sync_mascot_image 선례 — 사용자 개시 커맨드처럼 Err("hub_not_connected")를 던지지 않는다).
#[tauri::command]
pub async fn life_set_daily_line(state: State<'_, AppState>, body: String) -> Result<bool, String> {
    let Some(client) = hub_client(&state)? else { return Ok(false) };
    run_life_http("life_set_daily_line", move || {
        client.set_daily_line(&body).map(|_| true).map_err(|e| e.to_string())
    })
    .await
}

/// O1 — 캐시된 대문사진을 서버에 게시. 컷·연결이 없으면 Ok(false).
#[tauri::command]
pub async fn life_sync_daily_cut(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let Some(client) = hub_client(&state)? else { return Ok(false) };
    run_life_http("life_sync_daily_cut", move || upload_cached_daily_cut(&app, &client)).await
}

/// O1 — 방문 중인 방 주인의 대문사진 base64. 없으면 None(마스코트 이미지로 폴백).
#[tauri::command]
pub async fn life_daily_cut(state: State<'_, AppState>, agent_id: String) -> Result<Option<String>, String> {
    use base64::Engine as _;
    let Some(client) = hub_client(&state)? else { return Ok(None) };
    run_life_http("life_daily_cut", move || {
        client.daily_cut(&agent_id)
            .map(|value| value.map(|png| base64::engine::general_purpose::STANDARD.encode(png)))
            .map_err(|e| e.to_string())
    })
    .await
}
```

- [ ] **Step 4: `lib.rs`에 등록한다**

`a-mate/src-tauri/src/lib.rs`의 `commands::life_mascot_image,`(439행) **바로 뒤**:

```rust
                commands::life_mascot_image,
                commands::life_set_daily_line,
                commands::life_sync_daily_cut,
                commands::life_daily_cut,
```

- [ ] **Step 5: 컴파일과 회귀를 확인한다**

Run (PowerShell, `a-mate/`에서):
```powershell
cargo check --all-targets
cargo test
```
Expected: `cargo check` 경고 없이 통과, `cargo test` 기존 건수 그대로 통과.

> `cargo test`는 `pipeline.rs`의 `mod runtime`을 컴파일하지 않는다(`#[cfg(not(test))]`).
> 이번엔 `pipeline.rs`를 안 건드리지만 `cargo check --all-targets`를 **반드시** 함께 돌린다.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/crates/core/src/life_client.rs a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs
git commit -m "feat(agent): add life commands for front-door publication"
```

---

## Task 5: a-mate 프론트 — `resolveHomeLine` 순수 함수

**Files:**
- Create: `a-mate/src/lib/home-line.ts`
- Test: `a-mate/src/lib/home-line.test.ts`

**Interfaces:**
- Consumes: 없음 (순수 함수)
- Produces: `resolveHomeLine(input: { visiting: boolean; ownerLine: string; cutCaption: string | null; dailyLine: string | null }): string | null`

이 함수를 빼는 이유: 이 레포엔 타입 체크 단계가 없어 `.svelte`의 오류가 `npm run build`(transpile-only)·
`npm test` 어디에도 걸리지 않는다(로드맵 Q3, 실측). 판정 로직을 `.svelte` 밖으로 빼는 것이
현재 유일한 안전망이다.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/home-line.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { resolveHomeLine } from './home-line';

describe('resolveHomeLine', () => {
  it('내 방에서는 컷 캡션이 오늘의 한마디를 이긴다', () => {
    expect(resolveHomeLine({
      visiting: false, ownerLine: '', cutCaption: '밤샘 끝, 뿌듯', dailyLine: '월요일부터 달린다',
    })).toBe('밤샘 끝, 뿌듯');
  });

  it('컷 캡션이 없으면 오늘의 한마디를 쓴다', () => {
    expect(resolveHomeLine({
      visiting: false, ownerLine: '', cutCaption: null, dailyLine: '월요일부터 달린다',
    })).toBe('월요일부터 달린다');
  });

  it('내 방에서 둘 다 없으면 null (카드를 그리지 않는다)', () => {
    expect(resolveHomeLine({ visiting: false, ownerLine: '', cutCaption: null, dailyLine: null })).toBeNull();
    expect(resolveHomeLine({ visiting: false, ownerLine: '', cutCaption: '', dailyLine: '  ' })).toBeNull();
  });

  it('방문 중에는 주인 문장을 쓴다', () => {
    expect(resolveHomeLine({
      visiting: true, ownerLine: '주인이 쓴 한마디', cutCaption: null, dailyLine: null,
    })).toBe('주인이 쓴 한마디');
  });

  it('방문 중에는 내 캡션·한마디를 무시한다 (잔상 방지)', () => {
    expect(resolveHomeLine({
      visiting: true, ownerLine: '', cutCaption: '내 캡션', dailyLine: '내 한마디',
    })).toBeNull();
  });

  it('방문 중 주인 문장이 공백뿐이면 null', () => {
    expect(resolveHomeLine({ visiting: true, ownerLine: '   ', cutCaption: null, dailyLine: null })).toBeNull();
  });
});
```

- [ ] **Step 2: 실패를 확인한다**

Run (PowerShell, `a-mate/`에서): `npm test -- home-line`
Expected: FAIL — `Failed to resolve import "./home-line"`

- [ ] **Step 3: 최소 구현을 쓴다**

`a-mate/src/lib/home-line.ts`:

```ts
/** O1 — 대문 한마디 카드에 그릴 문장. 없으면 null(카드를 그리지 않는다).
 *
 *  방문 중이면 그 방 주인이 게시한 문장, 내 방이면 컷 캡션 우선(그림을 아는 텍스트가 이긴다).
 *  방문 중에 내 캡션·한마디를 참조하면 남의 방에 내 문장이 남는다 — 그래서 분기가 배타적이다. */
export function resolveHomeLine(input: {
  visiting: boolean;
  ownerLine: string;
  cutCaption: string | null;
  dailyLine: string | null;
}): string | null {
  const pick = input.visiting ? input.ownerLine : input.cutCaption || input.dailyLine || '';
  return pick.trim() ? pick : null;
}
```

- [ ] **Step 4: 통과를 확인한다**

Run: `npm test -- home-line`
Expected: PASS (6 tests)

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/home-line.ts a-mate/src/lib/home-line.test.ts
git commit -m "feat(agent): resolve home card line for own room and visits"
```

---

## Task 6: a-mate 프론트 — 게시 배선

**Files:**
- Modify: `a-mate/src/lib/api.ts` (`lifeMascotImage`(237행) 뒤에 3개 + `LifeState`(152-161행)에 2개 필드)
- Modify: `a-mate/src/App.svelte` (import · `homeLoaded` · `publishLine` `$effect` · 폴링 tick의 컷 캐치업 · `onDailyCutReady`)

**Interfaces:**
- Consumes: Task 4의 커맨드 3개
- Produces:
  - `lifeSetDailyLine(body: string): Promise<boolean>` · `lifeSyncDailyCut(): Promise<boolean>` · `lifeDailyCut(agentId: string): Promise<string | null>`
  - `LifeState.owner_daily_line?: string` · `LifeState.owner_daily_cut_sha256?: string | null`
  - `App.svelte`의 `homeLoaded` 플래그 (Task 7이 아니라 이 태스크가 만든다)

- [ ] **Step 1: `api.ts`에 타입과 래퍼를 추가한다**

**(a)** `LifeState` 인터페이스에 두 필드 (`owner_mascot_image_sha256` 뒤):

```ts
  owner_mascot_image_sha256?: string | null;
  // O1 대문 — 구서버는 두 필드를 보내지 않으므로 optional. 없으면 방문 시 현행(숨김)으로 폴백.
  owner_daily_line?: string;
  owner_daily_cut_sha256?: string | null;
```

**(b)** `lifeMascotImage` **바로 뒤**:

```ts
/** O1 — 대문에 걸린 오늘의 한마디 게시. 자동 경로라 미연결·구서버에서도 reject하지 않고 false. */
export const lifeSetDailyLine = (body: string) => invoke<boolean>('life_set_daily_line', { body });
/** O1 — 캐시된 대문사진을 서버에 게시. 컷·연결이 없으면 false. */
export const lifeSyncDailyCut = () => invoke<boolean>('life_sync_daily_cut');
/** O1 — 방문 중인 방 주인의 대문사진 base64. 없으면 null(마스코트 이미지로 폴백). */
export const lifeDailyCut = (agentId: string) => invoke<string | null>('life_daily_cut', { agentId });
```

- [ ] **Step 2: `App.svelte`의 import를 늘린다**

14-19행의 `./lib/api` import 목록에 `lifeSetDailyLine`, `lifeSyncDailyCut`를 추가한다
(알파벳 순서 없음 — 기존 나열 스타일에 맞춰 `lifeGuestbook, lifeView` 옆에 붙인다):

```ts
    onLifeVisit, onGuestbookNew, noticesReady, lifeContentAccess, lifeGoto, lifeGuestbook,
    lifeSetDailyLine, lifeSyncDailyCut, lifeView,
    type Summary,
  } from './lib/api';
```

- [ ] **Step 3: `refresh()`가 게시 게이트를 연다**

`refresh()`(135-140행)를 이렇게 고친다:

```ts
  // O1 — 첫 조회가 끝나기 전에는 대문을 게시하지 않는다 (초기값 null → 빈 문자열 게시 = 서버 값 삭제)
  let homeLoaded = $state(false);

  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
    dailyLine = await getDailyLine().catch(() => null);
    cutCaption = (await getDailyCut())?.caption || null;
    homeLoaded = true;
  }
```

- [ ] **Step 4: 문장 게시 `$effect`를 추가한다**

`refresh(); onScanDone(() => refresh());`(141-142행) **바로 뒤**:

```ts
  // O1 대문 게시 — 내 화면에 걸린 문장을 그대로 올린다(빈 문자열 = 지움).
  // visiting을 절대 참조하지 않는다 — 참조하면 남의 방에 들어간 순간 내 대문이 지워진다.
  const publishLine = $derived(cutCaption || dailyLine || '');
  $effect(() => {
    if (!homeLoaded) return; // 첫 refresh 전의 빈 문자열 게시 방지
    lifeSetDailyLine(publishLine).catch(() => {});
  });
```

- [ ] **Step 5: 대문사진 부팅 캐치업을 폴링 tick에 얹는다**

`App.svelte`의 `gbBootstrapped` 선언(58행) 옆에 플래그를 더한다:

```ts
  let gbBootstrapped = false;
  let cutSyncBootstrapped = false; // O1 — 대문사진 게시는 앱 실행당 1회 + 컷 생성 시마다
```

폴링 tick 안의 `gbBootstrapped` 블록(82-87행) **바로 뒤**에 추가한다:

```ts
        // O1 — 앱이 꺼진 동안의 미게시분·구서버→신서버 전환을 메운다. refresh()에 얹으면
        // scan:done마다 PNG를 올리게 되므로 일회성 가드로 둔다(실패 시 다음 tick 재시도).
        if (!cutSyncBootstrapped) {
          cutSyncBootstrapped = true;
          lifeSyncDailyCut().catch(() => { cutSyncBootstrapped = false; });
        }
```

- [ ] **Step 6: 새 컷이 생기면 서버에도 올린다**

`onDailyCutReady` 구독(186행)을 고친다:

```ts
      onDailyCutReady(() => {
        getDailyCut().then((c) => (cutCaption = c?.caption || null));
        lifeSyncDailyCut().catch(() => {}); // O1 — 새 컷을 서버 대문에도 게시
      }),
```

- [ ] **Step 7: 회귀를 확인한다**

Run (PowerShell, `a-mate/`에서):
```powershell
npm test
npm run build
```
Expected: 기존 테스트 + Task 5의 6건 모두 통과, 빌드 성공.

> `npm run build`는 transpile-only라 `.svelte`의 타입 오류를 잡지 못한다. `publishLine`·
> `homeLoaded`·`cutSyncBootstrapped`가 실제로 선언됐는지, import 목록에 두 함수가 들어갔는지
> **눈으로** 확인한다.

- [ ] **Step 8: 커밋**

```bash
git add a-mate/src/lib/api.ts a-mate/src/App.svelte
git commit -m "feat(agent): publish own front-door line and cut to life server"
```

---

## Task 7: a-mate 프론트 — 방문객 렌더

**Files:**
- Modify: `a-mate/src/lib/ui/RobotPortrait.svelte` (props 2개 · `visitCut` 상태 · `cutPng` derived · 방문 컷 `$effect` · 템플릿)
- Modify: `a-mate/src/App.svelte` (`ownerDailyLine`·`ownerCutVersion` 상태 · 폴링 tick 대입 · `homeLine` derived · 카드 조건 · `RobotPortrait` props)

**Interfaces:**
- Consumes: Task 5의 `resolveHomeLine`, Task 6의 `lifeDailyCut`·`LifeState` 두 필드
- Produces: 최종 사용자 동작 — B의 방에 들어가면 B의 대문사진과 B의 문장이 보인다

- [ ] **Step 1: `RobotPortrait.svelte`를 고친다**

파일 전체를 아래로 교체한다(기존 구조 유지 + 방문 컷 경로 추가):

```svelte
<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getDailyCut, getMascotSeed, getSprite, lifeDailyCut, lifeMascotImage, robotSpecForSeed, type DailyCut } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  // seed 지정 시 그 시드의 로봇(예: 방문 중인 미니홈피 주인), 미지정이면 내 마스코트
  let { seed = null, agentId = null, imageVersion = null, cutAgentId = null, cutVersion = null }: {
    seed?: string | null; agentId?: string | null; imageVersion?: string | null;
    cutAgentId?: string | null; cutVersion?: string | null;
  } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null);
  // AI 스프라이트(내 캐릭터 전용 캐시) — 있으면 이미지, 없으면 절차 생성 폴백
  let sprite = $state<string | null>(null);
  // H2 — 오늘의 컷 (내 화면 전용). 있으면 sprite/canvas 대신 컷 프레임.
  let cut = $state<DailyCut | null>(null);
  // O1 — 방문 중인 방 주인의 대문사진 base64. 서버 게시분이라 캡션은 App의 카드가 담당한다.
  let visitCut = $state<string | null>(null);
  const cutPng = $derived(seed ? visitCut : cut?.png ?? null);

  $effect(() => {
    if (seed) { cut = null; return; } // 방문 초상 — 내 컷 잔상 제거 (seed 토글 시 필수)
    let un: (() => void) | null = null;
    let stale = false; // 방문 전환 뒤 도착하는 인플라이트 응답 무시
    getDailyCut().then((c) => { if (!stale) cut = c; });
    listen('daily_cut:ready', () => getDailyCut().then((c) => { if (!stale) cut = c; })).then((u) => (un = u));
    return () => { stale = true; un?.(); };
  });

  // O1 — 방문 컷. cutVersion(sha256)이 무효화 키 (마스코트의 imageVersion과 같은 역할).
  // 없으면 아무것도 세팅하지 않고 마스코트 이미지 → 시드 절차 생성으로 폴백한다.
  $effect(() => {
    const id = cutAgentId, version = cutVersion;
    if (!seed) { visitCut = null; return; }
    visitCut = null;
    if (!id || !version) return;
    let stale = false;
    lifeDailyCut(id).then((png) => { if (!stale) visitCut = png; }).catch(() => {});
    return () => { stale = true; };
  });

  $effect(() => {
    const id = agentId, version = imageVersion;
    if (!seed) return;
    sprite = null;
    if (!id || !version) return;
    lifeMascotImage(id).then((s) => (sprite = s)).catch(() => (sprite = null));
  });

  $effect(() => {
    if (seed) return; // 남의 초상은 시드 절차 생성만
    let un: (() => void) | null = null;
    getSprite().then((s) => (sprite = s));
    listen('sprite:ready', () => getSprite().then((s) => (sprite = s))).then((u) => (un = u));
    return () => un?.();
  });

  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    const spec = seed ? robotSpecForSeed(seed) : getMascotSeed();
    spec.then((s: RobotSpec) => drawRobot(ctx, s, frameAt('idle', 300)));
  });
</script>

<!-- 컷 = 자체 배경을 가진 사진이라 풀블리드, 폴백 캐릭터(투명 배경)만 민트 여백 유지 -->
<div class="portrait" class:full={!!cutPng}>
  {#if cutPng}
    <!-- 캡션은 App의 "오늘의 한마디" 카드가 표시 — 초상은 이미지만 (공간 절약) -->
    <img class="cut" src={'data:image/png;base64,' + cutPng} alt="오늘의 대문사진" />
  {:else if sprite}
    <img class="sprite" src={'data:image/png;base64,' + sprite} alt="내 캐릭터" />
  {:else}
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  {/if}
</div>

<style>
  .portrait {
    background: var(--pastel-mint);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 10px;
    display: flex;
    justify-content: center;
  }
  canvas { width: 96px; height: 96px; image-rendering: pixelated; }
  .sprite { width: 96px; height: 96px; object-fit: contain; }
  /* H2 — 미니홈피 대문사진: 박스를 꽉 채우는 풀블리드 (감성 글귀는 "오늘의 한마디" 카드가 담당) */
  .portrait.full { padding: 0; overflow: hidden; }
  .cut { display: block; width: 100%; aspect-ratio: 1 / 1; height: auto; object-fit: cover; image-rendering: pixelated; }
</style>
```

- [ ] **Step 2: `App.svelte`에 주인 대문 상태를 추가한다**

`ownerAgentId`·`ownerImageVersion` 선언(54행) **바로 뒤**:

```ts
  let ownerAgentId = $state(''), ownerImageVersion = $state('');
  // O1 — 방문 중인 방 주인의 대문(문장·사진 버전). 기존 2초 폴링이 채우므로 추가 조회가 없다.
  let ownerDailyLine = $state(''), ownerCutVersion = $state('');
```

- [ ] **Step 3: 폴링 tick이 두 값을 채우게 한다**

tick 안의 `ownerImageVersion = v.life.owner_mascot_image_sha256 || '';`(73행) **바로 뒤**:

```ts
        ownerImageVersion = v.life.owner_mascot_image_sha256 || '';
        ownerDailyLine = v.life.owner_daily_line ?? '';
        ownerCutVersion = v.life.owner_daily_cut_sha256 || '';
```

같은 `$effect`의 `catch` 블록(94-98행)에서 `ownerSeed = '';` **바로 뒤**에 초기화를 더한다:

```ts
        ownerSeed = '';
        ownerDailyLine = '';
        ownerCutVersion = '';
```

- [ ] **Step 4: `resolveHomeLine`을 import하고 derived를 만든다**

`import { diaryVisibility, syncAllSharedDiaries, syncSharedDiary } from './lib/diary-sharing';`(30행) **바로 뒤**:

```ts
  import { resolveHomeLine } from './lib/home-line';
```

`cutCaption` 선언(113행) **바로 뒤**:

```ts
  let cutCaption = $state<string | null>(null);
  // O1 — 카드에 그릴 문장. 방문 중이면 주인 게시분, 내 방이면 캡션 우선.
  const homeLine = $derived(resolveHomeLine({ visiting, ownerLine: ownerDailyLine, cutCaption, dailyLine }));
```

- [ ] **Step 5: 초상과 카드를 고친다**

`App.svelte` 템플릿의 254-260행을 이렇게 교체한다:

```svelte
        <!-- 프로필 = 지금 보는 미니홈피의 주인. 방문 중이면 그 방 주인의 대문사진·로봇 -->
        <RobotPortrait seed={visiting ? ownerSeed : null} agentId={visiting ? ownerAgentId : null}
          imageVersion={visiting ? ownerImageVersion : null}
          cutAgentId={visiting ? ownerAgentId : null} cutVersion={visiting ? ownerCutVersion : null} />
        {#if homeLine}
          <div class="daily">
            <span class="cap">💬 오늘의 한마디</span>
            <span class="daily-line">{homeLine}</span>
          </div>
        {/if}
```

- [ ] **Step 6: 회귀를 확인한다**

Run (PowerShell, `a-mate/`에서):
```powershell
npm test
npm run build
cargo check --all-targets
```
Expected: 전부 통과.

> 타입 체크가 없으므로 다음 4개를 **눈으로** 확인한다: (1) `RobotPortrait`의 `$props()` 타입에
> `cutAgentId`·`cutVersion`이 있는지, (2) `App.svelte`가 그 두 props를 넘기는지,
> (3) `lifeDailyCut`가 `RobotPortrait`의 import에 있는지, (4) `resolveHomeLine` import 경로가
> `./lib/home-line`인지.

- [ ] **Step 7: 실제 앱으로 확인한다**

Run: `npm run tauri dev`

확인 항목:
1. 내 방 — 대문사진과 한마디가 **전과 똑같이** 보인다(회귀 없음).
2. 대문사진이 없는 상태(설정에서 자동 생성 off + 컷 파일 없음)에서도 한마디 카드가 보인다.
3. 방 목록에서 다른 방으로 이동 — 그 방 주인의 대문사진·문장이 보이거나, 주인이 게시한 게
   없으면 마스코트 이미지로 폴백하고 카드가 숨는다.
4. 내 방으로 돌아오면 내 대문이 그대로 있다(§Global Constraints의 함정 1 회귀 확인).

> 실환경 서버(`http://10.116.67.127:8001`)는 **재배포 전에는 새 엔드포인트가 없다.** 배포
> 전이면 3번은 확인할 수 없고, 대신 게시 호출이 404로 조용히 무시되며 앱이 정상 동작하는지
> (콘솔 에러로 죽지 않는지)를 확인한다.

- [ ] **Step 8: 커밋**

```bash
git add a-mate/src/lib/ui/RobotPortrait.svelte a-mate/src/App.svelte
git commit -m "feat(agent): show room owner front-door cut and line while visiting"
```

---

## Task 8: DoD — 아카이브와 로드맵 기록

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md:268` (O1 항목에 완료 표시)
- Move: `docs/design/a-mate/specs/2026-07-30-front-door-outbound-design.md` → `docs/archive/design/a-mate/specs/`
- Move: `docs/design/a-mate/plans/2026-07-30-front-door-outbound.md` → `docs/archive/design/a-mate/plans/`

**Interfaces:**
- Consumes: Task 1-7 전부 완료 + 테스트 녹색
- Produces: 없음 (문서 정리)

- [ ] **Step 1: 전체 검증을 다시 돌린다**

Run:
```powershell
cd a-hub/life; .venv\Scripts\python.exe -m pytest -q
cd ../../a-mate; cargo test; cargo check --all-targets; npm test
```
Expected: life `89 passed`, a-mate 전부 통과. **하나라도 실패하면 아카이브하지 않는다.**

- [ ] **Step 2: 로드맵 O1 항목에 완료를 기록한다**

`docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md`의 268행 제목을 고친다:

```markdown
**O1 — 대문사진(daily cut)·오늘의 한마디를 방문객에게도 표시** — ✅ 완료(2026-07-30, ADR 0026)
```

같은 항목 마지막(`- **의존·세션**:` 줄 뒤)에 결과 한 줄을 더한다:

```markdown
- **결과**: 공개범위 게이트 없이 게시(ADR 0026). 신규 표면 3개(`PATCH /life/me/daily-line`,
  `PUT /life/me/daily-cut`, `GET /life/agents/{id}/daily-cut`) + `life_state`에 `owner_daily_line`·
  `owner_daily_cut_sha256`. **실환경 스모크는 life 서버 재배포 후 별도** — 묶음 ②·④와 같은 대기 사유.
```

- [ ] **Step 3: `docs-archive` 스킬로 스펙·플랜을 옮긴다**

`docs-archive` 스킬을 호출해 이 두 파일을 `docs/archive/` 미러로 옮긴다:
- `docs/design/a-mate/specs/2026-07-30-front-door-outbound-design.md`
- `docs/design/a-mate/plans/2026-07-30-front-door-outbound.md`

옮긴 뒤 **밖으로 나가는 링크를 깊이에 맞게 재계산**한다(ADR 0013). 아카이브 위치에서:
- 스펙의 `../plans/...` → `../plans/...` (같은 미러 안이면 그대로)
- 스펙의 `../../life-visit.md` → `../../../../design/life-visit.md`
- 플랜의 `../specs/...` → `../specs/...` (같은 미러 안)
- ADR 0026이 스펙을 가리키는 링크(`../design/a-mate/specs/...`)는 **아카이브 경로로 갱신**해야 한다
  → `../archive/design/a-mate/specs/2026-07-30-front-door-outbound-design.md`
  (ADR 0025가 같은 방식으로 archive를 가리킨다 — 선례 확인 후 맞출 것)

- [ ] **Step 4: 링크가 깨지지 않았는지 확인한다**

Run: `git diff --stat` + 옮긴 파일들의 상대 링크를 `ls`로 실제 존재 확인:
```bash
ls docs/archive/design/a-mate/specs/2026-07-30-front-door-outbound-design.md
ls docs/design/life-visit.md
ls docs/adr/0026-front-door-outbound-publication.md
```
Expected: 3개 모두 존재.

- [ ] **Step 5: 커밋**

```bash
git add -A docs
git commit -m "docs(docs): archive front-door outbound working docs and stamp roadmap"
```

- [ ] **Step 6: PR을 만든다**

PR 본문은 파일로 쓴다(PowerShell here-string이 자주 깨진다):

```bash
gh pr create --title "feat: show room owner front-door cut and daily line to visitors" --body-file <path>
```

본문에 반드시 포함할 것:
- ADR 0026 결정 요약 + **수용한 결과**(오늘의 한마디는 생성 토글이 없어 사전 동의 없이 공개됨)
- 테스트 결과(life `89 passed`, a-mate `cargo test`/`npm test` 통과)
- **실환경 스모크 미완 + life 서버 재배포 선행 필요**를 명시

---

## Self-Review

**스펙 커버리지** — 스펙 각 절이 어느 태스크에 대응하는가:

| 스펙 절 | 태스크 |
|---|---|
| §1.1 데이터 모델 (`agents.daily_line`) | Task 2 Step 9 |
| §1.1 데이터 모델 (`daily_cuts` 테이블) | Task 3 Step 3 |
| §1.2 노출 (`owner_daily_line`) | Task 2 Step 5 |
| §1.2 노출 (`owner_daily_cut_sha256`) | Task 3 Step 4c |
| §1.3 엔드포인트 3개 | Task 2 Step 13, Task 3 Step 10 |
| §1.4 계약 문서 | Task 1 Step 2 |
| §2.1 게시 (문장·사진·부팅 캐치업·미연결) | Task 4 전체, Task 6 Step 3-6 |
| §2.2 방문객 렌더 (카드·초상) | Task 7 Step 1-5 |
| §2.3 `resolveHomeLine` | Task 5 |
| §2.4 함정 1 (visiting 미참조) | Task 6 Step 4 주석 + Task 5 테스트 "잔상 방지" |
| §2.4 함정 2 (`homeLoaded`) | Task 6 Step 3-4 |
| §2.4 함정 3 (캐치업을 refresh에 얹지 않기) | Task 6 Step 5 |
| §2.4 함정 4 (`cargo check --all-targets`) | Task 4 Step 5, Task 7 Step 6 |
| §2.5 신규 표면 요약 | Task 4·5·6 |
| §3 검증 | 각 태스크의 Run 스텝 + Task 8 Step 1 |
| §4 범위 밖 | 어떤 태스크도 a-lens·`contracts/`·`pipeline.rs`를 건드리지 않는다 |
| §5 ADR 0026 | Task 1 Step 1 |
| §6 백로그 | ADR·로드맵에 기록, 이 PR 범위 밖 |

갭 없음.

**타입 일관성 확인:**
- `set_daily_line`(Python) → `{"daily_line": str}` / `life_set_daily_line`(Rust) → `bool` /
  `lifeSetDailyLine`(TS) → `Promise<boolean>` — Rust가 응답 dict를 버리고 bool로 축약하는 것이
  의도다(프론트는 성공/실패만 쓴다).
- `daily_cut`이 세 레이어에 같은 이름으로 있다: `SqliteStore.daily_cut`(→ `tuple|None`),
  `LifeService.daily_cut`(→ `tuple`, 없으면 raise), `LifeClient::daily_cut`(→ `Option<Vec<u8>>`).
  마스코트 이미지가 정확히 같은 이름 중복을 이미 갖고 있어 선례와 일치한다.
- `cutVersion`(TS prop) ← `owner_daily_cut_sha256`(서버) — Task 6에서 타입 선언, Task 7에서 소비.
- `resolveHomeLine`의 `ownerLine: string`은 non-nullable이므로 호출자가 `?? ''`로 좁힌다
  (Task 7 Step 3에서 `ownerDailyLine`에 이미 좁혀 담는다).
