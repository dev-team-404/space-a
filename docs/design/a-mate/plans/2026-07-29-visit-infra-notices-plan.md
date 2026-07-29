# 방문 인프라 (P4+N1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a-hub life 서버에 인바운드 방문 추적을 신설하고, a-mate가 스캔 편승 폴링으로 방문·방명록 소식을 감지해 마스코트 말풍선("○○님 다녀갔어요")·홈 최근 알림·다이어리/방명록 탭 뱃지로 표면한다.

**Architecture:** 서버가 `enter()`에서 방문을 자동 기록(30분 세션화, 방당 100행)하고 `GET /life/me/visits`로 준다. a-mate 파이프라인의 신규 함수 `maybe_poll_inbound`가 settings 커서 기준 diff 후 `life:visit`/`guestbook:new`를 emit — Mascot 창은 말풍선, App 창은 알림·뱃지로 소비한다. 읽음(뱃지) 상태는 프론트 localStorage.

**Tech Stack:** Python 3(FastAPI, sqlite3, pytest) · Rust(ureq, serde_json, cargo test) · Svelte 5 + TS(Vitest)

**Spec:** [2026-07-29-visit-infra-notices-design.md](../specs/2026-07-29-visit-infra-notices-design.md) — 요구·결정 테이블·엣지 표 전부 여기 있음. 막히면 스펙이 정답.

## Global Constraints

- **작업 위치**: 워크트리 `D:\Project\space-a\.claude\worktrees\feat+visit-infra-notices`, 브랜치 `feat/visit-infra-notices` (이미 존재 — 스펙 커밋 + 핫픽스 커밋 `1542d0e` 포함).
- **시작 전 리베이스**: PR #128(main 핫픽스 `fix(agent): thread hub user id through life rename`)이 머지됐으면 `git fetch origin; git rebase origin/main` — 커밋 `1542d0e`는 자동 dedupe된다. 미머지면 그대로 진행(이 브랜치엔 이미 픽스 포함, cargo 녹색).
- **a-mate 명령은 전부 네이티브 Windows PowerShell**, `a-mate/` 디렉터리에서 (**WSL 금지** — a-mate/CLAUDE.md).
- **a-hub life 명령**은 `a-hub/life/`에서, venv 이미 세팅됨: `.venv\Scripts\python.exe -m pytest -q`.
- **커밋**: Conventional Commits **영어**(scope: a-hub=`backend`, a-mate=`agent`), 푸터 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- **contracts/ 미접촉** — life 서버 계약의 정본은 `docs/design/life-visit.md §4` (스펙 §1 조사 결론).
- **주석은 한국어**, 각 파일의 기존 스타일에 맞춤. 이모지·과잉 주석 금지.
- **타임스탬프 규약**: 서버가 `datetime.now(timezone.utc).isoformat()`(예: `2026-07-29T08:00:00.123456+00:00`)으로 발급 — 동일 서식이므로 커서 비교는 **문자열 사전순**으로 충분(Python·Rust·TS 모두).
- `docs/archive/`는 읽지 말 것 (레포 규칙 — 기본 탐색 제외).

## File Structure

| 파일 | 역할 |
|---|---|
| `a-hub/life/life_server/store.py` | `visits` 테이블 + load/save/delete (수정) |
| `a-hub/life/life_server/life.py` | 방문 기록·세션화·prune·조회 로직 (수정) |
| `a-hub/life/life_server/api.py` | `GET /life/me/visits` 라우트 (수정) |
| `a-hub/life/tests/test_visits.py` | 방문 추적 pytest (신규) |
| `docs/design/life-visit.md` | §4 계약 표에 엔드포인트 행 (수정) |
| `a-mate/crates/core/src/inbound.rs` | 순수 diff 판정 `select_new_visits`/`select_new_guestbook` (신규) |
| `a-mate/crates/core/src/lib.rs` | `pub mod inbound;` 등록 (수정) |
| `a-mate/crates/core/src/life_client.rs` | `visits()` 메서드 (수정) |
| `a-mate/src-tauri/src/pipeline.rs` | `maybe_poll_inbound` 글루 + 호출 배선 (수정) |
| `a-mate/src/lib/api.ts` | `LifeVisit` 타입 + `onLifeVisit`/`onGuestbookNew` (수정) |
| `a-mate/src/lib/robot/bubble.ts` + `bubble.test.ts` | `visitBubble` (수정) |
| `a-mate/src/lib/notices.ts` + `notices.test.ts` | `visitNotice`/`guestbookNotice` + kind·dest 확장 (수정) |
| `a-mate/src/lib/unseen.ts` + `unseen.test.ts` | 뱃지 unseen 순수 로직 + localStorage 래퍼 (신규) |
| `a-mate/src/lib/ui/home/NoticeLog.svelte` | ICON 맵에 visit/guestbook (수정) |
| `a-mate/src/Mascot.svelte` | `onLifeVisit` → 말풍선 (수정) |
| `a-mate/src/App.svelte` | 알림 기록 + 탭 뱃지 + 부트스트랩 + 클리어 (수정) |

커밋 구조 (스펙 §8 — 항목당 분리): Task 1–2 → 커밋① backend / Task 3–5 → 커밋② P4 클라 / Task 6–8 → 커밋③ N1 / Task 9 → 검증·PR.

---

### Task 1: a-hub — visits 저장·기록·세션화·조회 (life.py + store.py)

**Files:**
- Modify: `a-hub/life/life_server/store.py` (`_SCHEMA` 끝 + 메서드 3개)
- Modify: `a-hub/life/life_server/life.py` (상수, `__init__`, `enter()`, 신규 메서드 3개)
- Test: `a-hub/life/tests/test_visits.py` (신규)

**Interfaces:**
- Consumes: 기존 `LifeService.register/enter/me/_authed`, `SqliteStore` write-through 패턴.
- Produces: `LifeService.visits(token, since=None, limit=50) -> list[dict]` — dict 키 `visit_id, life_id, visitor_agent_id, visitor_name, first_at, last_at, present`. `SqliteStore.load_visits() -> list[dict]`, `save_visit(row)`, `delete_visit(visit_id)`. 상수 `VISIT_SESSION_WINDOW_SECS`, `VISITS_MAX_PER_LIFE`. (Task 2의 API 라우트가 `visits()`를 그대로 노출.)

- [ ] **Step 1: 실패하는 테스트 작성** — `a-hub/life/tests/test_visits.py` 신규:

```python
"""인바운드 방문 추적 (P4) — enter 자동 기록·세션화·prune·조회.
설계: docs/design/a-mate/specs/2026-07-29-visit-infra-notices-design.md §3"""

import pytest

from life_server import errors
from life_server.life import VISIT_SESSION_WINDOW_SECS, VISITS_MAX_PER_LIFE, LifeService
from life_server.store import SqliteStore


@pytest.fixture
def life() -> LifeService:
    return LifeService()


def _register_two(life):
    _, token_a, life_a = life.register("A")
    _, token_b, life_b = life.register("B")
    return token_a, life_a, token_b, life_b


def _age_latest_visit(life, iso: str) -> None:
    """세션 창을 지난 방문으로 노화 — 시계 주입 없이 저장 행을 직접 조작(테스트 전용)."""
    life._visits[-1]["first_at"] = iso
    life._visits[-1]["last_at"] = iso


def test_enter_records_inbound_visit(life):
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    rows = life.visits(token_a)
    assert len(rows) == 1
    assert rows[0]["visitor_name"] == "B"
    assert rows[0]["first_at"] == rows[0]["last_at"]
    assert rows[0]["present"] is True  # 아직 방에 있음


def test_present_false_after_visitor_leaves(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.enter(token_b, life_b.id, cell=None)  # 자기 방으로 귀가
    assert life.visits(token_a)[0]["present"] is False


def test_own_room_entry_not_recorded(life):
    _, token_a, life_a = life.register("A")
    life.enter(token_a, life_a.id, cell=None)
    assert life.visits(token_a) == []


def test_visits_scope_is_my_room_only(life):
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    assert life.visits(token_b) == []  # B의 방엔 인바운드 방문 없음


def test_visits_requires_valid_token(life):
    with pytest.raises(errors.Unauthorized):
        life.visits("no-such-token")


def test_revisit_within_window_extends_session(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 30분 이내 재방문
    rows = life.visits(token_a)
    assert len(rows) == 1  # 새 행이 아니라 세션 연장
    assert rows[0]["last_at"] >= rows[0]["first_at"]


def test_revisit_after_window_creates_new_row(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    _age_latest_visit(life, "2020-01-01T00:00:00+00:00")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)
    assert len(life.visits(token_a)) == 2


def test_session_extension_resnapshots_visitor_name(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.rename(token_b, "B2")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 세션 연장 → 이름 재스냅샷
    assert life.visits(token_a)[0]["visitor_name"] == "B2"


def test_visits_since_filters_and_sorts_desc(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    _age_latest_visit(life, "2020-01-01T00:00:00+00:00")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 두 번째 세션(현재 시각)
    rows = life.visits(token_a)
    assert [r["last_at"] for r in rows] == sorted((r["last_at"] for r in rows), reverse=True)
    fresh = life.visits(token_a, since="2021-01-01T00:00:00+00:00")
    assert len(fresh) == 1


def test_visits_limit_clamped(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    for i in range(3):
        life.enter(token_b, life_a.id, cell=None)
        _age_latest_visit(life, f"2020-01-01T00:0{i}:00+00:00")
        life.enter(token_b, life_b.id, cell=None)
    assert len(life.visits(token_a, limit=2)) == 2
    assert len(life.visits(token_a, limit=0)) == 1  # 최소 1로 클램프


def test_prune_keeps_latest_per_life(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    for i in range(VISITS_MAX_PER_LIFE + 5):
        life.enter(token_b, life_a.id, cell=None)
        _age_latest_visit(life, f"2020-01-01T{i // 60:02d}:{i % 60:02d}:00+00:00")
        life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 최신(현재 시각) 방문
    rows = life.visits(token_a, limit=100)
    assert len(rows) == VISITS_MAX_PER_LIFE
    assert rows[0]["last_at"] > "2021-01-01"  # 최신은 살아 있음
    assert all(r["last_at"] != "2020-01-01T00:00:00+00:00" for r in rows)  # 가장 오래된 것 prune


def test_sqlite_persistence_survives_restart(tmp_path):
    db = str(tmp_path / "life.db")
    life = LifeService(store=SqliteStore(db))
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    revived = LifeService(store=SqliteStore(db))  # 재시작 흉내
    assert len(revived._visits) == 1
    assert revived._visits[0]["visitor_name"] == "B"
    assert set(revived._visits[0]) == {
        "visit_id", "life_id", "visitor_agent_id", "visitor_name", "first_at", "last_at",
    }
```

- [ ] **Step 2: 실패 확인**

Run (`a-hub/life`에서): `.venv\Scripts\python.exe -m pytest tests/test_visits.py -q`
Expected: FAIL — `ImportError: cannot import name 'VISIT_SESSION_WINDOW_SECS'`

- [ ] **Step 3: store.py 구현** — `_SCHEMA` 문자열 끝(guestbook 테이블 다음, `"""` 닫기 직전)에 추가:

```sql
CREATE TABLE IF NOT EXISTS visits (
  visit_id         TEXT PRIMARY KEY,
  life_id          TEXT NOT NULL,
  visitor_agent_id TEXT NOT NULL,
  visitor_name     TEXT NOT NULL,
  first_at         TEXT NOT NULL,
  last_at          TEXT NOT NULL
);
```

그리고 `save_guestbook_entry`/`delete_guestbook_entry` 메서드 아래에 추가 (신규 테이블이라 ALTER 마이그레이션 불필요):

```python
    def load_visits(self) -> list[dict]:
        return [
            {"visit_id": visit_id, "life_id": life_id, "visitor_agent_id": visitor_id,
             "visitor_name": visitor_name, "first_at": first_at, "last_at": last_at}
            for visit_id, life_id, visitor_id, visitor_name, first_at, last_at in self._conn.execute(
                "SELECT visit_id, life_id, visitor_agent_id, visitor_name, first_at, last_at "
                "FROM visits ORDER BY last_at"
            )
        ]

    def save_visit(self, row: dict) -> None:
        # 세션 연장(last_at·visitor_name 갱신)과 신규 행을 upsert 하나로 처리
        self._conn.execute(
            "INSERT INTO visits VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(visit_id) DO UPDATE SET "
            "last_at = excluded.last_at, visitor_name = excluded.visitor_name",
            (row["visit_id"], row["life_id"], row["visitor_agent_id"], row["visitor_name"],
             row["first_at"], row["last_at"]),
        )
        self._conn.commit()

    def delete_visit(self, visit_id: str) -> None:
        self._conn.execute("DELETE FROM visits WHERE visit_id = ?", (visit_id,))
        self._conn.commit()
```

- [ ] **Step 4: life.py 구현** — 4곳 수정.

(a) 모듈 상수 — `WINDOW_ROTATION_BY_WALL = ...` 줄 아래에:

```python
# P4 인바운드 방문 추적 — 같은 방문자 연속 재입장 세션화 창·방당 보존 상한 (스펙 §3)
VISIT_SESSION_WINDOW_SECS = 30 * 60
VISITS_MAX_PER_LIFE = 100
```

(b) `__init__` — `self._mascot_image_hashes: dict[str, str] = {}` 줄 아래에 `self._visits: list[dict] = []`, 그리고 `if store is not None:` 블록의 `self._mascot_image_hashes = store.load_mascot_image_hashes()` 줄 아래에 `self._visits = store.load_visits()`.

(c) `enter()` — 락 블록을 다음으로 교체 (변경점: `life =` 바인딩 추가 + 방문 기록 한 줄):

```python
        with self._lock:
            if life_id not in self._life:
                raise errors.NotFound(f"life '{life_id}' not found")
            life = self._life[life_id]
            target = cell if cell is not None else self._free_cell_locked(life_id, for_agent=agent.agent_id)
            if self._occupied_locked(life_id, target, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({target[0]},{target[1]}) 이미 점유됨")
            # 이전 방 자동 퇴장 = at_life/cell 원자 교체
            agent.at_life = life_id
            agent.cell = target
            agent.connected = True
            if self._store:
                self._store.save_agent(agent)
            # P4: 인바운드 방문 자동 기록 — 방문자≠주인일 때만 (같은 락 안)
            self._record_visit_locked(life, agent)
        return self.me(token)
```

(d) 신규 메서드 — `# --- 위치 변이 ...` 섹션 주석 바로 위(= `disconnect` 아래)에:

```python
    # --- 인바운드 방문 추적 (P4) ---

    def _record_visit_locked(self, life: Life, agent: LifeAgent) -> None:
        """방문 자동 기록. 자기 방 입장은 기록하지 않는다. 같은 (방, 방문자)의 최신 방문이
        세션 창(30분) 이내면 새 행 대신 last_at 연장 — 들락날락 도배 억제 (스펙 §1)."""
        if life.owner_agent_id == agent.agent_id:
            return
        now = datetime.now(timezone.utc)
        latest = next((v for v in reversed(self._visits)
                       if v["life_id"] == life.id and v["visitor_agent_id"] == agent.agent_id), None)
        if latest is not None:
            last = datetime.fromisoformat(latest["last_at"])
            if (now - last).total_seconds() <= VISIT_SESSION_WINDOW_SECS:
                latest["last_at"] = now.isoformat()
                latest["visitor_name"] = agent.name  # 개명 반영 (이력 행은 당시 이름 유지)
                if self._store:
                    self._store.save_visit(latest)
                return
        row = {"visit_id": f"vst_{uuid.uuid4().hex[:12]}", "life_id": life.id,
               "visitor_agent_id": agent.agent_id, "visitor_name": agent.name,
               "first_at": now.isoformat(), "last_at": now.isoformat()}
        self._visits.append(row)
        if self._store:
            self._store.save_visit(row)
        self._prune_visits_locked(life.id)

    def _prune_visits_locked(self, life_id: str) -> None:
        rows = [v for v in self._visits if v["life_id"] == life_id]
        if len(rows) <= VISITS_MAX_PER_LIFE:
            return
        rows.sort(key=lambda v: v["last_at"])
        for stale in rows[: len(rows) - VISITS_MAX_PER_LIFE]:
            self._visits.remove(stale)
            if self._store:
                self._store.delete_visit(stale["visit_id"])

    def visits(self, token: str | None, since: str | None = None, limit: int = 50) -> list[dict]:
        """내 방 인바운드 방문 조회 — Bearer 본인 방 전용(타인 방 기록은 구조적으로 불가).
        since: last_at 초과 필터(RFC3339 사전순). last_at 내림차순, limit 1~100 클램프.
        present = 그 방문자가 지금도 내 방에 있는지 (문구 현재형/과거형 분기용, 스펙 §1)."""
        me = self._authed(token)
        limit = max(1, min(int(limit), 100))
        with self._lock:
            rows = [v for v in self._visits if v["life_id"] == me.life_id]
            if since:
                rows = [v for v in rows if v["last_at"] > since]
            rows.sort(key=lambda v: v["last_at"], reverse=True)
            out = []
            for v in rows[:limit]:
                visitor = self._agents.get(v["visitor_agent_id"])
                out.append({**v, "present": bool(visitor and visitor.at_life == me.life_id)})
            return out
```

- [ ] **Step 5: 통과 확인**

Run: `.venv\Scripts\python.exe -m pytest tests/test_visits.py -q`
Expected: 12 passed

- [ ] **Step 6: 기존 테스트 회귀 확인**

Run: `.venv\Scripts\python.exe -m pytest -q`
Expected: 전부 passed (베이스라인 기준 기존 테스트 + 신규 12)

---

### Task 2: a-hub — API 라우트 + 계약 문서, 커밋 ①

**Files:**
- Modify: `a-hub/life/life_server/api.py` (`life_me` 라우트 바로 아래)
- Modify: `docs/design/life-visit.md` (§4 표)

**Interfaces:**
- Consumes: Task 1의 `LifeService.visits`.
- Produces: `GET /life/me/visits?since=&limit=` → `{"visits": [...]}` — Task 4의 `LifeClient::visits`가 호출.

- [ ] **Step 1: 라우트 추가** — `api.py`의 `life_me` 함수 아래에:

```python
    @app.get("/life/me/visits")
    def life_visits(since: str | None = None, limit: int = 50,
                    authorization: str | None = Header(default=None)):
        # P4 인바운드 방문 — enter가 자동 기록(방문자≠주인), 본인 방 전용 (스펙 §3)
        return {"visits": life.visits(_bearer(authorization), since, limit)}
```

- [ ] **Step 2: 계약 문서 갱신** — `docs/design/life-visit.md` §4 표의 `GET /life/me` 행 아래에 행 추가:

```markdown
| GET | `/life/me/visits?since=&limit=` | 내 방 인바운드 방문 목록 `{visits: [{visit_id, visitor_agent_id, visitor_name, first_at, last_at, present}]}`. enter가 자동 기록(방문자≠주인, 같은 방문자 30분 세션화, 방당 100행 보존). `since`=last_at 초과 필터, `limit` 기본 50·최대 100, last_at 내림차순 (P4) |
```

- [ ] **Step 3: 전체 pytest 재확인**

Run (`a-hub/life`): `.venv\Scripts\python.exe -m pytest -q`
Expected: 전부 passed

- [ ] **Step 4: 커밋 ①**

```powershell
git add a-hub/life/life_server/store.py a-hub/life/life_server/life.py a-hub/life/life_server/api.py a-hub/life/tests/test_visits.py docs/design/life-visit.md
git commit -m "feat(backend): add inbound visit tracking to life server`n`nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(PowerShell에서 여러 줄 메시지는 위처럼 백틱-n 대신 here-string도 가능: `git commit -m @'...'@` — 닫는 `'@`는 반드시 0열.)

---

### Task 3: core — `inbound.rs` diff 판정 (visits 파트, TDD)

**Files:**
- Create: `a-mate/crates/core/src/inbound.rs`
- Modify: `a-mate/crates/core/src/lib.rs` (모듈 등록)

**Interfaces:**
- Produces: `agent_mentor::inbound::select_new_visits(rows: &[Value], cursor: Option<&str>) -> (Vec<Value>, Option<String>)` — Task 4의 파이프라인 글루가 사용. 반환 0번 = emit할 행(서버 순서 유지 = last_at 내림차순), 1번 = 저장할 새 커서(관측 first_at 최댓값, 기존 커서 포함 단조 증가).

- [ ] **Step 1: 모듈 파일을 테스트와 함께 작성** — `a-mate/crates/core/src/inbound.rs` 신규:

```rust
//! 인바운드 소식(방문·방명록) diff — 스캔 편승 폴링의 순수 판정부.
//! 스펙: docs/design/a-mate/specs/2026-07-29-visit-infra-notices-design.md §4
//!
//! 커서는 서버 발급 RFC3339(UTC, 동일 서식) 문자열 — 사전순 비교로 충분하다.
//! 파싱 불가·필드 누락 행은 방어적으로 무시한다.

use serde_json::Value;

/// 방문 diff: `first_at > cursor`인 행만 emit 대상. 세션 연장(last_at만 갱신)된 행은
/// first_at이 그대로라 재-emit되지 않는다 — 도배 억제가 커서 기준에서 완성된다 (스펙 §2).
/// cursor가 None(첫 실행)이면 emit 없이 커서만 초기화 — 설치 직후 과거분 도배 방지.
/// 반환 커서는 관측한 first_at 최댓값과 기존 커서 중 큰 쪽 — 항상 단조 증가.
pub fn select_new_visits(rows: &[Value], cursor: Option<&str>) -> (Vec<Value>, Option<String>) {
    let next = rows
        .iter()
        .filter_map(|r| r.get("first_at").and_then(|v| v.as_str()))
        .chain(cursor)
        .max()
        .map(str::to_string);
    let Some(cur) = cursor else { return (Vec::new(), next) };
    let fresh = rows
        .iter()
        .filter(|r| r.get("first_at").and_then(|v| v.as_str()).is_some_and(|f| f > cur))
        .cloned()
        .collect();
    (fresh, next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn visit(first_at: &str, last_at: &str) -> Value {
        json!({"visit_id": "v", "visitor_name": "B", "first_at": first_at, "last_at": last_at, "present": false})
    }

    #[test]
    fn first_run_initializes_cursor_without_emitting() {
        let rows = vec![visit("2026-07-29T01:00:00+00:00", "2026-07-29T01:00:00+00:00")];
        let (fresh, cur) = select_new_visits(&rows, None);
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn first_run_with_no_rows_keeps_cursor_unset() {
        let (fresh, cur) = select_new_visits(&[], None);
        assert!(fresh.is_empty());
        assert_eq!(cur, None); // 다음 스캔도 첫 실행 취급 — 과거분 도배 없음
    }

    #[test]
    fn emits_only_rows_newer_than_cursor() {
        let rows = vec![
            visit("2026-07-29T03:00:00+00:00", "2026-07-29T03:00:00+00:00"),
            visit("2026-07-29T01:00:00+00:00", "2026-07-29T01:00:00+00:00"),
        ];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["first_at"], "2026-07-29T03:00:00+00:00");
        assert_eq!(cur.as_deref(), Some("2026-07-29T03:00:00+00:00"));
    }

    #[test]
    fn session_extension_is_not_reemitted() {
        // last_at만 갱신된 행(first_at ≤ 커서) — since 필터에 걸려 내려와도 emit 제외
        let rows = vec![visit("2026-07-29T01:00:00+00:00", "2026-07-29T05:00:00+00:00")];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T01:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn cursor_never_regresses_when_rows_pruned() {
        let (fresh, cur) = select_new_visits(&[], Some("2026-07-29T09:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T09:00:00+00:00"));
    }

    #[test]
    fn malformed_rows_are_ignored() {
        let rows = vec![json!({"visit_id": "broken"})];
        let (fresh, cur) = select_new_visits(&rows, Some("2026-07-29T01:00:00+00:00"));
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }
}
```

- [ ] **Step 2: 모듈 등록** — `a-mate/crates/core/src/lib.rs`의 `pub mod hub;` 줄 아래에 (알파벳 순서 무관 — 기존 나열 순서에 맞춰):

```rust
pub mod inbound;
```

- [ ] **Step 3: 테스트 실행**

Run (`a-mate/`에서): `cargo test -p agent-mentor inbound`
Expected: 6 passed

---

### Task 4: a-mate 백엔드 — `LifeClient::visits` + `maybe_poll_inbound`(visits 파트)

**Files:**
- Modify: `a-mate/crates/core/src/life_client.rs` (`me()` 아래)
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`maybe_reply_guestbook` 아래 + `run_pipeline_once` 호출부)

**Interfaces:**
- Consumes: Task 3 `select_new_visits`, 기존 settings 키 `hub_url/hub_token/hub_api_key/hub_life_id/hub_agent_id`, `store.get_setting/set_setting`.
- Produces: Tauri 이벤트 `life:visit` (payload = 방문 행 JSON 배열, last_at 내림차순 — Task 5가 구독). settings 키 `inbound_visits_cursor`. `LifeClient::visits(since: Option<&str>, limit: u32) -> Result<Value>`.

- [ ] **Step 1: `visits()` 메서드** — `life_client.rs`의 `me()` 아래에:

```rust
    /// 내 방 인바운드 방문 목록 (P4). since = 서버 발급 last_at 커서 — 초과분만 받는다.
    /// 구서버(엔드포인트 미배포)는 404 — 호출자가 이번 실행 동안 폴링을 비활성한다.
    pub fn visits(&self, since: Option<&str>, limit: u32) -> Result<Value> {
        let mut req = self.req("GET", "/life/me/visits").query("limit", &limit.to_string());
        if let Some(s) = since.map(str::trim).filter(|s| !s.is_empty()) {
            req = req.query("since", s); // RFC3339의 '+'가 query 인코딩으로 보존된다
        }
        req.call().map_err(err_of)?.into_json().map_err(Into::into)
    }
```

- [ ] **Step 2: 컴파일 확인**

Run (`a-mate/`): `cargo test -p agent-mentor --lib life_client`
Expected: 기존 body 빌더 테스트 passed, 경고 없음

- [ ] **Step 3: 파이프라인 글루** — `pipeline.rs` runtime 모듈의 `maybe_reply_guestbook` 함수 **아래**에 신규 함수 (이번 Task에서는 ② 방문 파트만 — ③ 방명록 파트는 Task 7이 추가):

```rust
    /// P4+N1 — 인바운드 소식 폴링 (스캔 편승, 스펙 §4). 내 방 방문을 settings 커서
    /// 기준 diff 후 life:visit emit. hub 미연결이면 no-op, 모든 실패는 warn 후 다음
    /// 스캔 재시도. 커서 저장은 emit 성공 후 — 실패 시 커서 미갱신으로 재-emit된다.
    /// 구서버(visits 404)는 이번 실행 동안 방문 폴링만 비활성(maybe_reply_guestbook
    /// INCOMPATIBLE 선례). 기존 maybe_reply_guestbook은 건드리지 않는다(묶음 ② 충돌 억제).
    fn maybe_poll_inbound(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static VISITS_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

        // ① 락: 설정·커서 스냅샷 → 즉시 해제 (maybe_reply_guestbook 선례)
        let (url, token, api_key, life_id, agent_id, visits_cursor) = match store_mutex.lock() {
            Ok(store) => {
                let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                let cursor = store.get_setting("inbound_visits_cursor").ok().flatten()
                    .filter(|v| !v.is_empty());
                (get("hub_url"), get("hub_token"), get("hub_api_key"),
                 get("hub_life_id"), get("hub_agent_id"), cursor)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if url.trim().is_empty() || token.is_empty() || life_id.is_empty() || agent_id.is_empty() {
            return;
        }
        let client = agent_mentor::life_client::LifeClient {
            base_url: url,
            token,
            api_key: { let k = api_key.trim(); (!k.is_empty()).then(|| k.to_string()) },
        };
        let save_cursor = |key: &str, val: &Option<String>| {
            let Some(v) = val.as_deref() else { return };
            match store_mutex.lock() {
                Ok(store) => {
                    if let Err(e) = store.set_setting(key, v) { log::warn!("{key} 저장 실패: {e}"); }
                }
                Err(e) => log::warn!("store lock poisoned: {e}"),
            }
        };

        // ② 락 없이 네트워크: 방문 diff → life:visit
        if !VISITS_UNSUPPORTED.load(Ordering::SeqCst) {
            match client.visits(visits_cursor.as_deref(), 100) {
                Ok(v) => {
                    let rows = v.get("visits").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let (fresh, next) =
                        agent_mentor::inbound::select_new_visits(&rows, visits_cursor.as_deref());
                    if fresh.is_empty() || app.emit("life:visit", &fresh).is_ok() {
                        save_cursor("inbound_visits_cursor", &next);
                    } else {
                        log::warn!("life:visit emit 실패 — 다음 스캔 재시도");
                    }
                }
                // err_of가 "(HTTP 404)"를 접미한다 — 구서버 판별
                Err(e) if e.to_string().contains("(HTTP 404)") => {
                    log::warn!("방문 폴링: 서버가 visits 미지원(구서버) — 이번 실행 동안 비활성");
                    VISITS_UNSUPPORTED.store(true, Ordering::SeqCst);
                }
                Err(e) => log::warn!("방문 폴링: 조회 실패(다음 스캔 재시도): {e}"),
            }
        }
    }
```

- [ ] **Step 4: 호출 배선** — `run_pipeline_once`의 `maybe_reply_guestbook(&state.store);` 줄 바로 아래에:

```rust
                // P4+N1 인바운드 소식 — 내 방 방문·방명록 diff를 emit (hub 미연결·구서버 no-op)
                maybe_poll_inbound(app, &state.store);
```

- [ ] **Step 5: 전체 cargo 확인**

Run (`a-mate/`): `cargo test`
Expected: 전부 passed (agent-mentor-app 포함 컴파일 성공, `_ = &agent_id` 덕에 unused 경고 없음)

---

### Task 5: 프론트 P4 — 말풍선 + 방문 알림, 커밋 ②

**Files:**
- Modify: `a-mate/src/lib/api.ts` (타입 + listen 래퍼)
- Modify: `a-mate/src/lib/robot/bubble.ts` / Test: `a-mate/src/lib/robot/bubble.test.ts`
- Modify: `a-mate/src/lib/notices.ts` / Test: `a-mate/src/lib/notices.test.ts`
- Modify: `a-mate/src/lib/ui/home/NoticeLog.svelte` (ICON)
- Modify: `a-mate/src/Mascot.svelte`, `a-mate/src/App.svelte` (구독)

**Interfaces:**
- Consumes: Task 4의 `life:visit` 이벤트.
- Produces: `LifeVisit` 타입, `onLifeVisit`, `visitBubble(visits): Bubble`, `visitNotice(visits, ts): Notice`(kind `visit`, target 없음 — 클릭 불가). Task 8이 `Notice['kind']` 유니언에 의존.

- [ ] **Step 1: 실패하는 테스트 — bubble** — `bubble.test.ts`에 추가 (기존 import에 `visitBubble` 추가):

```typescript
describe('visitBubble', () => {
  const v = (name: string, present = false) => ({ visitor_name: name, present });
  it('단수 과거형: ○○님 다녀갔어요', () => {
    const b = visitBubble([v('준녕')]);
    expect(b).toEqual({ kind: 'visit', tab: 'home', text: '준녕님 다녀갔어요' });
  });
  it('단수 현재형: present면 놀러왔어요', () => {
    expect(visitBubble([v('준녕', true)]).text).toBe('준녕님이 놀러왔어요!');
  });
  it('복수: 최근 방문자 + 외 N명, 하나라도 present면 현재형', () => {
    expect(visitBubble([v('가'), v('나')]).text).toBe('가님 외 1명 다녀갔어요');
    expect(visitBubble([v('가'), v('나', true)]).text).toBe('가님 외 1명이 놀러왔어요!');
  });
});
```

- [ ] **Step 2: 실패하는 테스트 — notices** — `notices.test.ts`의 `notice 팩토리` describe에 추가 (import에 `visitNotice` 추가):

```typescript
  it('visit: 단수·복수 문구, target 없음(클릭 불가)', () => {
    const one = visitNotice([{ visitor_name: '준녕' }], '2026-07-29T10:00:00Z');
    expect(one.kind).toBe('visit');
    expect(one.text).toBe('준녕님이 방에 다녀갔어요');
    expect(one.target).toBeUndefined();
    expect(noticeDest(one)).toBeNull();
    const many = visitNotice([{ visitor_name: '가' }, { visitor_name: '나' }], '2026-07-29T10:00:00Z');
    expect(many.text).toBe('가님 외 1명이 방에 다녀갔어요');
  });
```

- [ ] **Step 3: 실패 확인**

Run (`a-mate/`): `npm test -- --run`
Expected: FAIL — `visitBubble is not a function` / `visitNotice is not a function`

- [ ] **Step 4: bubble.ts 구현** — `BubbleKind`를 교체하고 `occasionBubble` 아래에 함수 추가:

```typescript
export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter' | 'visit';
```

```typescript
/** P4 방문 소식 — 정적 템플릿(LLM 불필요: 즉시성·실패 무해). visits는 last_at 내림차순
 *  (백엔드가 서버 순서 유지) — [0]이 가장 최근 방문자. present면 현재형 (스펙 §5). */
export function visitBubble(visits: { visitor_name: string; present: boolean }[]): Bubble {
  const name = visits[0].visitor_name;
  const more = visits.length > 1 ? ` 외 ${visits.length - 1}명` : '';
  const text = visits.some((v) => v.present)
    ? `${name}님${more}이 놀러왔어요!`
    : `${name}님${more} 다녀갔어요`;
  return { kind: 'visit', tab: 'home', text };
}
```

- [ ] **Step 5: notices.ts 구현** — `Notice.kind`에 `'visit'` 추가:

```typescript
  kind: 'finding' | 'diary' | 'occasion' | 'visit';
```

`occasionNotice` 아래에:

```typescript
/** P4 방문 알림 — target 없음(클릭 불가, occasion 선례). visits[0] = 가장 최근 방문자. */
export function visitNotice(visits: { visitor_name: string }[], ts: string): Notice {
  const more = visits.length > 1 ? ` 외 ${visits.length - 1}명` : '';
  return { ts, kind: 'visit', text: `${visits[0].visitor_name}님${more}이 방에 다녀갔어요` };
}
```

- [ ] **Step 6: api.ts** — `GuestbookEntry` 인터페이스 아래에 타입, `onContentReady` 근처에 래퍼 추가:

```typescript
export interface LifeVisit { visit_id: string; visitor_agent_id: string; visitor_name: string; first_at: string; last_at: string; present: boolean }
```

```typescript
export const onLifeVisit = (cb: (rows: LifeVisit[]) => void): Promise<UnlistenFn> =>
  listen<LifeVisit[]>('life:visit', (e) => cb(e.payload));
```

- [ ] **Step 7: NoticeLog ICON** — `NoticeLog.svelte`의 ICON 맵 교체 (`Record<Notice['kind'], string>` 타입이라 kind 추가 시 컴파일이 강제함):

```typescript
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉', visit: '👋' };
```

- [ ] **Step 8: Mascot.svelte 구독** — import 2곳 수정: api import에 `onLifeVisit` 추가, bubble import에 `visitBubble` 추가. `$effect`의 subs 배열에서 `onDiaryReady(...)` 줄 아래에:

```typescript
      onLifeVisit((rows) => rows.length && showBubble(visitBubble(rows))),
```

- [ ] **Step 9: App.svelte 구독** — import 2곳: api import에 `onLifeVisit`, notices import에 `visitNotice`. 알림 subs 배열의 `onOccasionToday(...)` 줄 아래에:

```typescript
      onLifeVisit((rows) => rows.length && record(visitNotice(rows, new Date().toISOString()))),
```

- [ ] **Step 10: 테스트·빌드 확인**

Run (`a-mate/`): `npm test -- --run` 그리고 `npm run build`
Expected: 테스트 전부 passed, 빌드 성공 (svelte-check 포함 시 타입 에러 0)

- [ ] **Step 11: 커밋 ②**

```powershell
git add a-mate/crates/core/src/inbound.rs a-mate/crates/core/src/lib.rs a-mate/crates/core/src/life_client.rs a-mate/src-tauri/src/pipeline.rs a-mate/src/lib/api.ts a-mate/src/lib/robot/bubble.ts a-mate/src/lib/robot/bubble.test.ts a-mate/src/lib/notices.ts a-mate/src/lib/notices.test.ts a-mate/src/lib/ui/home/NoticeLog.svelte a-mate/src/Mascot.svelte a-mate/src/App.svelte
git commit -m "feat(agent): poll inbound visits and surface mascot bubble`n`nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: core — `select_new_guestbook` (TDD)

**Files:**
- Modify: `a-mate/crates/core/src/inbound.rs`

**Interfaces:**
- Produces: `select_new_guestbook(entries: &[Value], my_agent_id: &str, cursor: Option<&str>) -> (Vec<Value>, Option<String>)` — Task 7이 사용. 타인 글(원글+답글)만 대상, 내 에이전트 작성분 제외 (스펙 §1 뱃지 정의와 동일 모집단).

- [ ] **Step 1: 함수 + 테스트 추가** — `inbound.rs`의 `select_new_visits` 아래에:

```rust
/// 방명록 diff: 타인 글(author_agent_id ≠ 나)만 대상 — 내 봇 답글·수동 글은 소식이 아니다.
/// created_at > cursor인 항목만 emit. 커서 규약은 select_new_visits와 동일
/// (None=첫 실행 초기화, 관측 최댓값으로 단조 증가).
pub fn select_new_guestbook(
    entries: &[Value],
    my_agent_id: &str,
    cursor: Option<&str>,
) -> (Vec<Value>, Option<String>) {
    let others: Vec<&Value> = entries
        .iter()
        .filter(|e| {
            e.get("author_agent_id").and_then(|v| v.as_str()).is_some_and(|a| a != my_agent_id)
        })
        .collect();
    let next = others
        .iter()
        .filter_map(|e| e.get("created_at").and_then(|v| v.as_str()))
        .chain(cursor)
        .max()
        .map(str::to_string);
    let Some(cur) = cursor else { return (Vec::new(), next) };
    let fresh = others
        .into_iter()
        .filter(|e| e.get("created_at").and_then(|v| v.as_str()).is_some_and(|c| c > cur))
        .cloned()
        .collect();
    (fresh, next)
}
```

tests 모듈에 추가:

```rust
    fn entry(id: &str, author: &str, created_at: &str, parent: Option<&str>) -> Value {
        json!({"entry_id": id, "author_agent_id": author, "author_name": author,
               "body": "글", "parent_id": parent, "created_at": created_at})
    }

    #[test]
    fn guestbook_first_run_initializes_without_emitting() {
        let entries = vec![entry("e1", "other", "2026-07-29T01:00:00+00:00", None)];
        let (fresh, cur) = select_new_guestbook(&entries, "me", None);
        assert!(fresh.is_empty());
        assert_eq!(cur.as_deref(), Some("2026-07-29T01:00:00+00:00"));
    }

    #[test]
    fn guestbook_excludes_my_own_entries_everywhere() {
        // 내 글은 emit 대상도, 커서 후보도 아니다 — 내 답글로 커서가 앞서가 타인 글을 놓치면 안 됨
        let entries = vec![
            entry("mine", "me", "2026-07-29T05:00:00+00:00", Some("e1")),
            entry("e1", "other", "2026-07-29T03:00:00+00:00", None),
        ];
        let (fresh, cur) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["entry_id"], "e1");
        assert_eq!(cur.as_deref(), Some("2026-07-29T03:00:00+00:00"));
    }

    #[test]
    fn guestbook_counts_replies_from_others_too() {
        let entries = vec![entry("r1", "other", "2026-07-29T03:00:00+00:00", Some("mine-post"))];
        let (fresh, _) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert_eq!(fresh.len(), 1); // parent 유무 무관 — 타인 글 전부 (스펙 §1)
    }

    #[test]
    fn guestbook_cursor_boundary_is_exclusive() {
        let entries = vec![entry("e1", "other", "2026-07-29T02:00:00+00:00", None)];
        let (fresh, _) = select_new_guestbook(&entries, "me", Some("2026-07-29T02:00:00+00:00"));
        assert!(fresh.is_empty()); // 커서와 같은 시각 = 이미 본 것
    }
```

- [ ] **Step 2: 테스트 실행**

Run (`a-mate/`): `cargo test -p agent-mentor inbound`
Expected: 10 passed

---

### Task 7: 파이프라인 방명록 파트 + `onGuestbookNew`

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`maybe_poll_inbound` 끝에 ③ 추가)
- Modify: `a-mate/src/lib/api.ts`

**Interfaces:**
- Consumes: Task 6 `select_new_guestbook`, 기존 `client.guestbook(&life_id)`.
- Produces: 이벤트 `guestbook:new` (payload = `GuestbookEntry[]`, 신규 타인 글 — 최신이 앞), settings 키 `inbound_guestbook_cursor`, `onGuestbookNew` 래퍼 — Task 8이 구독.

- [ ] **Step 1: 파이프라인 ③ 추가** — `maybe_poll_inbound`에서 (a) 커서 스냅샷에 방명록 커서 추가 — 락 블록의 튜플을 다음으로 교체:

```rust
        let (url, token, api_key, life_id, agent_id, visits_cursor, gb_cursor) = match store_mutex.lock() {
            Ok(store) => {
                let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                let cursor = |k: &str| store.get_setting(k).ok().flatten().filter(|v: &String| !v.is_empty());
                (get("hub_url"), get("hub_token"), get("hub_api_key"),
                 get("hub_life_id"), get("hub_agent_id"),
                 cursor("inbound_visits_cursor"), cursor("inbound_guestbook_cursor"))
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
```

(b) 함수 끝(② 블록 뒤)에:

```rust
        // ③ 방명록 diff → guestbook:new (타인 글만 — 내 작성분 제외는 core 판정)
        match client.guestbook(&life_id) {
            Ok(v) => {
                let entries = v.get("entries").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let (fresh, next) =
                    agent_mentor::inbound::select_new_guestbook(&entries, &agent_id, gb_cursor.as_deref());
                if fresh.is_empty() || app.emit("guestbook:new", &fresh).is_ok() {
                    save_cursor("inbound_guestbook_cursor", &next);
                } else {
                    log::warn!("guestbook:new emit 실패 — 다음 스캔 재시도");
                }
            }
            Err(e) => log::warn!("방명록 신규 폴링: 조회 실패(다음 스캔 재시도): {e}"),
        }
```

- [ ] **Step 2: api.ts 래퍼** — `onLifeVisit` 아래에:

```typescript
export const onGuestbookNew = (cb: (rows: GuestbookEntry[]) => void): Promise<UnlistenFn> =>
  listen<GuestbookEntry[]>('guestbook:new', (e) => cb(e.payload));
```

- [ ] **Step 3: 컴파일·테스트 확인**

Run (`a-mate/`): `cargo test`
Expected: 전부 passed

---

### Task 8: 프론트 N1 — 방명록 알림 + 탭 뱃지, 커밋 ③

**Files:**
- Create: `a-mate/src/lib/unseen.ts` / Test: `a-mate/src/lib/unseen.test.ts`
- Modify: `a-mate/src/lib/notices.ts` / Test: `a-mate/src/lib/notices.test.ts`
- Modify: `a-mate/src/lib/ui/home/NoticeLog.svelte` (ICON)
- Modify: `a-mate/src/App.svelte`

**Interfaces:**
- Consumes: Task 7 `onGuestbookNew`, 기존 `lifeGuestbook(lifeId)`, 기존 `onDiaryReady`, App의 `meId/myLifeId/currentLifeId/visiting/tab/pendingTab` 상태.
- Produces: `guestbookNotice(entries, ts): Notice`(kind `guestbook`, target=최신 entry_id, dest `{tab:'guestbook'}`), `NoticeDest.tab`에 `'guestbook'`, `unseen.ts`의 `UnseenState/parseUnseen/loadUnseen/saveUnseen/addDiaryDate/clearDiaryDates/clearGuestbookSeen/newGuestbookIds`.

- [ ] **Step 1: 실패하는 테스트 — unseen** — `a-mate/src/lib/unseen.test.ts` 신규:

```typescript
import { describe, expect, it } from 'vitest';
import {
  addDiaryDate, clearDiaryDates, clearGuestbookSeen, newGuestbookIds, parseUnseen,
} from './unseen';

const NOW = '2026-07-29T10:00:00+00:00';

describe('parseUnseen', () => {
  it('정상 저장분 복원', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
    expect(parseUnseen(raw, NOW)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
  });
  it('없음·손상은 now 기준 초기 상태 — 과거 전체가 뱃지로 쏟아지지 않게', () => {
    expect(parseUnseen(null, NOW)).toEqual({ diaryDates: [], guestbookLastSeen: NOW });
    expect(parseUnseen('{broken', NOW).guestbookLastSeen).toBe(NOW);
    expect(parseUnseen('{"diaryDates":"x"}', NOW).diaryDates).toEqual([]);
  });
});

describe('diary unseen', () => {
  it('날짜 set 누적 — 같은 날짜 재생성은 1회', () => {
    let s = parseUnseen(null, NOW);
    s = addDiaryDate(s, '2026-07-28');
    s = addDiaryDate(s, '2026-07-28');
    s = addDiaryDate(s, '2026-07-29');
    expect(s.diaryDates).toEqual(['2026-07-28', '2026-07-29']);
    expect(clearDiaryDates(s).diaryDates).toEqual([]);
  });
});

describe('guestbook unseen', () => {
  const e = (id: string, author: string, at: string) => ({ entry_id: id, author_agent_id: author, created_at: at });
  it('lastSeen 이후 타인 글만 — 내 글·과거 글 제외', () => {
    const ids = newGuestbookIds(
      [e('new', 'other', '2026-07-29T12:00:00+00:00'), e('mine', 'me', '2026-07-29T12:00:00+00:00'), e('old', 'other', '2026-07-01T00:00:00+00:00')],
      'me', NOW,
    );
    expect(ids).toEqual(['new']);
  });
  it('클리어 = lastSeen 갱신', () => {
    const s = clearGuestbookSeen(parseUnseen(null, NOW), '2026-07-30T00:00:00+00:00');
    expect(s.guestbookLastSeen).toBe('2026-07-30T00:00:00+00:00');
  });
});
```

- [ ] **Step 2: 실패하는 테스트 — guestbookNotice** — `notices.test.ts`에 추가 (import에 `guestbookNotice` 추가):

```typescript
  it('guestbook: N건 문구 + 최신 entry_id target + 방명록 탭 dest', () => {
    const one = guestbookNotice([{ entry_id: 'e1', author_name: '준녕' }], '2026-07-29T10:00:00Z');
    expect(one.kind).toBe('guestbook');
    expect(one.text).toBe('방명록에 새 글 — 준녕님');
    expect(noticeDest(one)).toEqual({ tab: 'guestbook', target: 'e1' });
    const many = guestbookNotice(
      [{ entry_id: 'e2', author_name: '가' }, { entry_id: 'e1', author_name: '나' }],
      '2026-07-29T10:00:00Z',
    );
    expect(many.text).toBe('방명록에 새 글 2건 — 가님 외');
    expect(many.target).toBe('e2');
  });
```

- [ ] **Step 3: 실패 확인**

Run (`a-mate/`): `npm test -- --run`
Expected: FAIL — unseen 모듈 없음 / `guestbookNotice is not a function`

- [ ] **Step 4: unseen.ts 구현** — `a-mate/src/lib/unseen.ts` 신규:

```typescript
/** N1 탭 뱃지의 읽음 상태 (스펙 §5) — localStorage 영속은 lastSeen(방명록)·날짜 set(다이어리)만.
 *  방명록 unseen 카운트는 세션 내 entry_id set으로 재계산(부트스트랩+이벤트 dedup — 중복 카운트 구조적 차단). */
export interface UnseenState {
  diaryDates: string[]; // 아직 안 본 신규 일기 날짜들 — 일기는 앱 실행 중에만 생성되므로 이벤트 누적으로 완결
  guestbookLastSeen: string; // 방명록 탭을 내 방 문맥으로 마지막 확인한 시각 (서버 created_at와 같은 서식 비교)
}

const KEY = 'agent-mentor.tab-unseen';

/** 저장 원문 파싱 — 없음·손상은 now 기준 초기 상태 (과거 전체가 뱃지로 쏟아지는 것 방지). */
export function parseUnseen(raw: string | null, nowIso: string): UnseenState {
  try {
    const v = JSON.parse(raw ?? 'null');
    if (
      v && Array.isArray(v.diaryDates) && v.diaryDates.every((d: unknown) => typeof d === 'string')
      && typeof v.guestbookLastSeen === 'string'
    ) {
      return { diaryDates: v.diaryDates, guestbookLastSeen: v.guestbookLastSeen };
    }
  } catch {
    /* 손상 → 초기화 */
  }
  return { diaryDates: [], guestbookLastSeen: nowIso };
}

export function loadUnseen(nowIso: string): UnseenState {
  return parseUnseen(localStorage.getItem(KEY), nowIso);
}

export function saveUnseen(s: UnseenState): void {
  localStorage.setItem(KEY, JSON.stringify(s));
}

export function addDiaryDate(s: UnseenState, date: string): UnseenState {
  return s.diaryDates.includes(date) ? s : { ...s, diaryDates: [...s.diaryDates, date] };
}

export function clearDiaryDates(s: UnseenState): UnseenState {
  return { ...s, diaryDates: [] };
}

export function clearGuestbookSeen(s: UnseenState, nowIso: string): UnseenState {
  return { ...s, guestbookLastSeen: nowIso };
}

/** lastSeen 이후의 타인 글 entry_id — 부트스트랩(서버 조회)과 이벤트 payload 양쪽에 같은 판정. */
export function newGuestbookIds(
  entries: { entry_id: string; author_agent_id: string; created_at: string }[],
  myAgentId: string,
  lastSeenIso: string,
): string[] {
  return entries
    .filter((e) => e.author_agent_id !== myAgentId && e.created_at > lastSeenIso)
    .map((e) => e.entry_id);
}
```

- [ ] **Step 5: notices.ts 확장** — kind 유니언·NoticeDest·dest 세 곳 수정 + 헬퍼 추가:

```typescript
  kind: 'finding' | 'diary' | 'occasion' | 'visit' | 'guestbook';
```

`Notice.target`의 주석을 갱신: `/** 딥링크 대상 — finding=dedup_key, diary=YYYY-MM-DD, guestbook=entry_id. 없으면 클릭 불가. */`

```typescript
export interface NoticeDest {
  tab: 'coach' | 'diary' | 'guestbook';
  target: string;
}
```

`visitNotice` 아래에:

```typescript
/** N1 방명록 알림 — entries는 최신이 앞(백엔드가 서버 순서 유지). target=최신 entry_id. */
export function guestbookNotice(entries: { entry_id: string; author_name: string }[], ts: string): Notice {
  const who = entries[0].author_name;
  const text = entries.length > 1
    ? `방명록에 새 글 ${entries.length}건 — ${who}님 외`
    : `방명록에 새 글 — ${who}님`;
  return { ts, kind: 'guestbook', text, target: entries[0].entry_id };
}
```

`noticeDest`에 분기 추가 (`diary` 분기 아래):

```typescript
  if (n.kind === 'guestbook') return { tab: 'guestbook', target: n.target };
```

- [ ] **Step 6: NoticeLog ICON** — 맵에 `guestbook: '✍️'` 추가:

```typescript
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉', visit: '👋', guestbook: '✍️' };
```

- [ ] **Step 7: App.svelte 배선** — 변경 6곳. import 추가:

```typescript
  import { lifeGuestbook, onGuestbookNew } from './lib/api'; // 기존 api import 목록에 합류
  import { guestbookNotice } from './lib/notices'; // 기존 notices import 목록에 합류
  import {
    addDiaryDate, clearDiaryDates, clearGuestbookSeen, loadUnseen, newGuestbookIds, saveUnseen,
  } from './lib/unseen';
```

(a) 상태 — `let notices = $state<Notice[]>(loadNotices());` 아래에:

```typescript
  // N1 탭 뱃지 — 다이어리는 날짜 set(영속), 방명록은 entry_id set(세션) + lastSeen(영속)
  let unseen = $state(loadUnseen(new Date().toISOString()));
  saveUnseen(unseen); // 최초 실행: 초기 lastSeen을 고정해 재시작마다 리셋되지 않게
  let gbUnseenIds = $state(new Set<string>());
  const unseenDiary = $derived(unseen.diaryDates.length);
  const unseenGuestbook = $derived(gbUnseenIds.size);
```

(b) 구독 — 알림 subs 배열의 `onDiaryReady` 핸들러를 교체(뱃지 누적 추가)하고 `onLifeVisit` 줄 아래에 `onGuestbookNew` 추가:

```typescript
      onDiaryReady((date) => {
        record(diaryNotice(date, new Date().toISOString()));
        unseen = addDiaryDate(unseen, date);
        saveUnseen(unseen);
        syncSharedDiary(date, currentDiaryVisibility()).catch(() => { diaryCatchUpStarted = false; });
      }),
```

```typescript
      onGuestbookNew((rows) => {
        if (!rows.length) return;
        record(guestbookNotice(rows, new Date().toISOString()));
        const fresh = newGuestbookIds(rows, meId, unseen.guestbookLastSeen);
        if (fresh.length) gbUnseenIds = new Set([...gbUnseenIds, ...fresh]);
      }),
```

(c) 부트스트랩 — 알림 subs `$effect` 아래에 신규 `$effect` (앱 꺼진 동안 온 글 보완 — 스펙 §5):

```typescript
  // 방명록 뱃지 부트스트랩: 내 방 식별이 서면 1회 서버 조회 — entry_id dedup이라 이벤트와 중복 카운트 없음
  let gbBootstrapped = false;
  $effect(() => {
    if (gbBootstrapped || !myLifeId || !meId) return;
    gbBootstrapped = true;
    lifeGuestbook(myLifeId)
      .then(({ entries }) => {
        const fresh = newGuestbookIds(entries, meId, unseen.guestbookLastSeen);
        if (fresh.length) gbUnseenIds = new Set([...gbUnseenIds, ...fresh]);
      })
      .catch(() => { gbBootstrapped = false; }); // 실패 시 다음 tick 재시도
  });
```

(d) 클리어 — 부트스트랩 `$effect` 아래에:

```typescript
  // 탭 확인 시 뱃지 클리어 — 방명록은 내 방 문맥일 때만 (남의 방 방명록을 봐도 내 소식은 그대로)
  $effect(() => {
    if (tab === 'diary' && !visiting && unseen.diaryDates.length) {
      unseen = clearDiaryDates(unseen);
      saveUnseen(unseen);
    }
    if (tab === 'guestbook' && !visiting && currentLifeId === myLifeId && gbUnseenIds.size) {
      unseen = clearGuestbookSeen(unseen, new Date().toISOString());
      saveUnseen(unseen);
      gbUnseenIds = new Set();
    }
  });
```

(e) 알림 클릭 착지 — `gotoDest`를 교체 + 헬퍼 추가 (설정 딥링크의 pendingTab 선례):

```typescript
  // NoticeLog 클릭 착지 — dest.tab에 따라 코칭 카드/다이어리 날짜/방명록 탭으로
  function gotoDest(dest: NoticeDest) {
    if (dest.tab === 'coach') gotoCoach(dest.target);
    else if (dest.tab === 'guestbook') gotoGuestbook();
    else gotoDiary(dest.target);
  }

  // 방명록 알림은 내 방 문맥으로 착지 — 방문 중이면 내 방으로 돌아간 뒤 (설정 딥링크 선례)
  function gotoGuestbook() {
    if (visiting) { pendingTab = 'guestbook'; lifeGoto(myLifeId).catch(() => { pendingTab = null; }); }
    else tab = 'guestbook';
  }
```

(f) 탭 뱃지 렌더 — nav의 coach 뱃지 줄 아래에:

```svelte
            {#if t.id === 'diary' && unseenDiary > 0}<span class="badge">{unseenDiary}</span>{/if}
            {#if t.id === 'guestbook' && unseenGuestbook > 0}<span class="badge">{unseenGuestbook}</span>{/if}
```

- [ ] **Step 8: 테스트·빌드 확인**

Run (`a-mate/`): `npm test -- --run` 그리고 `npm run build`
Expected: 전부 passed(신규 unseen 8건·notices 1건 포함), 빌드 성공

- [ ] **Step 9: 커밋 ③**

```powershell
git add a-mate/crates/core/src/inbound.rs a-mate/src-tauri/src/pipeline.rs a-mate/src/lib/api.ts a-mate/src/lib/unseen.ts a-mate/src/lib/unseen.test.ts a-mate/src/lib/notices.ts a-mate/src/lib/notices.test.ts a-mate/src/lib/ui/home/NoticeLog.svelte a-mate/src/App.svelte
git commit -m "feat(agent): record guestbook notices and tab badges`n`nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 전체 검증 + PR

**Files:** 없음 (검증·PR만)

- [ ] **Step 1: 전체 테스트 3종**

```powershell
# a-mate (a-mate/에서)
cargo test          # Expected: 전부 passed
npm test -- --run   # Expected: 전부 passed
npm run build       # Expected: 성공
```

```powershell
# a-hub life (a-hub/life에서)
.venv\Scripts\python.exe -m pytest -q   # Expected: 전부 passed
```

- [ ] **Step 2: 리베이스·푸시** — PR #128이 머지됐다면 `git fetch origin; git rebase origin/main` (핫픽스 커밋 자동 dedupe, 충돌 시 우리 쪽은 docs·신규 파일 위주라 소충돌). 이후:

```powershell
git push -u origin feat/visit-infra-notices
```

- [ ] **Step 3: PR 생성** — 본문은 파일로 작성 후 `--body-file` 사용 (PowerShell 5.1은 인자 내 큰따옴표를 깨뜨림). 제목: `feat: inbound visit tracking, mascot bubble and tab badges (P4+N1)`. 본문에 포함할 것:
  - 요약: 스펙 링크 + 커밋 3개(백엔드/P4 클라/N1) 구성.
  - **실환경 체크리스트** (사용자 몫 — 스펙 §7):
    - [ ] life 서버 배포 후 2클라: B가 A 방 방문 → A 마스코트 말풍선("B님이 놀러왔어요!"/"다녀갔어요") + 홈 최근 알림 확인
    - [ ] 30분 내 재방문이 새 알림을 만들지 않는지 (세션화)
    - [ ] B가 A 방명록 작성 → A 방명록 탭 뱃지 증가 + 최근 알림 + 탭 확인 시 클리어
    - [ ] 일기 생성 → 다이어리 탭 뱃지 + 탭 확인 시 클리어
    - [ ] 구서버(visits 미배포)에서 방문 폴링만 조용히 비활성되는지 (로그 warn 1회)
  - 푸터: `🤖 Generated with [Claude Code](https://claude.com/claude-code)`

- [ ] **Step 4: DoD** — PR 머지 후 (별도 후속): `docs-archive` 스킬로 본 플랜+스펙을 `docs/archive/`로 이동, 로드맵 묶음 ④ 행에 완료 기록 (main 최신에서 — ADR 0013).

---

## 병렬 세션 주의 (묶음 ②: feat/diary-social-cluster)

- 겹침 예상: `pipeline.rs`(④=`maybe_poll_inbound` 신규 함수+호출 1줄 vs ②=diary 함수 — 머지 가능 수준), `api.ts`·`App.svelte`(④가 주로 수정, ②는 미접촉 예상), 로드맵 파일(나중 머지 쪽이 rebase 소충돌 처리).
- 기존 `maybe_reply_guestbook`은 **의도적으로 무수정** — 충돌 표면 최소화가 설계였다 (스펙 §2).
