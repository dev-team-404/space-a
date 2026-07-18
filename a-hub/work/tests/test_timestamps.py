"""Page·Issue 생성/수정 시각 (created_at·updated_at) 검증.

시각은 서비스 레이어에서 주입 가능한 clock으로 찍는다. 테스트는 고정 clock을
주입해 결정적으로 검증한다. created_at은 생성 시 한 번, updated_at은 상태 변경마다.
"""

import pytest

from ahub.adapters.store_memory import InMemoryStore
from ahub.adapters.store_sqlite import SqliteStore
from ahub.api.rest_server import create_app
from ahub.core.models import Page
from ahub.core.services import SpaceAService

from fastapi.testclient import TestClient

T1 = "2026-01-01T00:00:00+00:00"
T2 = "2026-02-02T00:00:00+00:00"


class Clock:
    """호출 시 self.value를 반환하는 가변 clock — 시간 경과를 흉내낸다."""

    def __init__(self, value):
        self.value = value

    def __call__(self):
        return self.value


@pytest.fixture(params=["memory", "sqlite"])
def store(request):
    return InMemoryStore() if request.param == "memory" else SqliteStore(":memory:")


def _svc(store, clock):
    svc = SpaceAService(store, now=clock)
    svc.create_space("s1", "S", purpose="p")
    return svc


def test_create_page_stamps_created_and_updated(store):
    svc = _svc(store, Clock(T1))
    _, tok = svc.register_agent("u", "u", "s1")
    p = svc.create_page(tok, "s1", "t")
    assert p.created_at == T1
    assert p.updated_at == T1


def test_open_issue_stamps_timestamps(store):
    svc = _svc(store, Clock(T1))
    _, tok = svc.register_agent("u", "u", "s1")
    iss = svc.open_issue(tok, "title", "s1")
    assert iss.created_at == T1
    assert iss.updated_at == T1


def test_edit_page_updates_only_updated_at(store):
    clock = Clock(T1)
    svc = _svc(store, clock)
    _, tok = svc.register_agent("u", "u", "s1")
    p = svc.create_page(tok, "s1", "t")
    clock.value = T2  # 시간 경과
    edited = svc.edit_page(tok, p.id, title="new")
    assert edited.created_at == T1  # 생성 시각은 보존
    assert edited.updated_at == T2  # 수정 시각만 갱신


def test_resolve_issue_updates_issue_updated_at(store):
    clock = Clock(T1)
    svc = _svc(store, clock)
    _, tok = svc.register_agent("u", "u", "s1")
    iss = svc.open_issue(tok, "title", "s1")
    clock.value = T2
    resolved, _ = svc.resolve_issue(tok, iss.id, "done", publish_knowledge=False)
    assert resolved.created_at == T1
    assert resolved.updated_at == T2


def test_resolve_issue_shares_timestamp_with_published_page(store):
    """한 트랜잭션: 이슈 updated_at과 발행 페이지 created_at/updated_at이 동일 시각.

    기본(실제) clock을 써서, resolve_issue가 now()를 두 번 불러 값이 갈리면
    실패하도록 한다 — 단일 _ts 공유를 실제로 검증.
    """
    svc = SpaceAService(store)  # 실제 UTC clock
    svc.create_space("s1", "S", purpose="p")
    _, tok = svc.register_agent("u", "u", "s1")
    iss = svc.open_issue(tok, "title", "s1")
    resolved, page = svc.resolve_issue(tok, iss.id, "done", publish_knowledge=True)
    assert page is not None
    assert resolved.updated_at == page.created_at == page.updated_at


def test_legacy_record_without_timestamps_loads_as_none(store):
    svc = _svc(store, Clock(T1))
    # created_at 지정 없이 직접 저장 → 기존 데이터 시뮬레이션
    page = Page(id=store.new_id("page"), space_id="s1", title="old")
    store.add_page(page)
    loaded = store.get_page(page.id)
    assert loaded.created_at is None
    assert loaded.updated_at is None


def test_sqlite_migration_adds_timestamp_columns(tmp_path):
    """created_at/updated_at 컬럼이 없는 레거시 SQLite 파일을 열면 마이그레이션된다."""
    import sqlite3

    db = str(tmp_path / "legacy.db")
    conn = sqlite3.connect(db)
    conn.execute(
        "CREATE TABLE pages (id TEXT PRIMARY KEY, space_id TEXT, title TEXT, body TEXT, "
        "source TEXT, parent_id TEXT, status TEXT, visibility TEXT, issue_id TEXT, "
        "steps TEXT, superseded_by TEXT, flags INTEGER, created_by TEXT)"
    )
    conn.execute(
        "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ("legacy_1", "s1", "old", "", "authored", None, "active", "org", None, "[]", None, 0, None),
    )
    conn.commit()
    conn.close()

    s = SqliteStore(db)  # __init__ → _migrate()
    legacy = s.get_page("legacy_1")
    assert legacy is not None
    assert legacy.created_at is None
    assert legacy.updated_at is None
    s.add_page(Page(id="new_1", space_id="s1", title="n", created_at=T1, updated_at=T1))
    got = s.get_page("new_1")
    assert got.created_at == T1


# --- REST ---


def test_rest_page_response_includes_timestamps():
    store = InMemoryStore()
    svc = SpaceAService(store, now=Clock(T1))
    svc.create_space("s1", "S", purpose="p")
    c = TestClient(create_app(svc, mount_mcp=False))
    tok = c.post("/agents/register", json={"user_id": "u", "name": "u", "space_id": "s1"}).json()["token"]
    r = c.post("/spaces/s1/pages", json={"title": "t"}, headers={"authorization": f"Bearer {tok}"})
    body = r.json()
    assert body["created_at"] == T1
    assert body["updated_at"] == T1
    got = c.get(f"/pages/{body['page_id']}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["created_at"] == T1
    assert got["updated_at"] == T1
