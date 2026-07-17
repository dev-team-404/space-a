"""Page·Issue 생성자(작성자) 저장·조회 검증.

토큰으로 인증한 에이전트를 Page.created_by(신규)·Issue.opened_by(기존)에 기록하고
조회 응답에 함께 반환하는지 확인한다. agent_id만 다룬다(name 생략).
"""

from fastapi.testclient import TestClient

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.rest_server import create_app
from ahub.core.models import Page
from ahub.core.services import SpaceAService


# --- 서비스 레벨 (memory·sqlite 양쪽: conftest의 service 픽스처) ---


def _register(service: SpaceAService):
    service.create_space("s1", "Space One", purpose="p")
    agent, token = service.register_agent("bot", "bot", "s1")
    return agent, token


def test_create_page_records_author(service: SpaceAService):
    agent, token = _register(service)
    page = service.create_page(token, "s1", "제목", body="본문")
    assert page.created_by == agent.id


def test_resolved_page_records_resolver(service: SpaceAService):
    agent, token = _register(service)
    issue = service.open_issue(token, "문제", "s1")
    _, page = service.resolve_issue(token, issue.id, "해결 요약", ["step"])
    assert page is not None
    assert page.created_by == agent.id


def test_different_agents_have_distinct_authors(service: SpaceAService):
    a1, t1 = _register(service)
    a2, t2 = service.register_agent("bot2", "bot2", "s1")
    p1 = service.create_page(t1, "s1", "A")
    p2 = service.create_page(t2, "s1", "B")
    assert p1.created_by == a1.id
    assert p2.created_by == a2.id
    assert p1.created_by != p2.created_by


def test_legacy_page_without_author_loads_as_none(service: SpaceAService):
    """created_by 없이 저장된 기존 Page도 None으로 안전하게 읽힌다 (하위호환)."""
    _register(service)
    # created_by를 지정하지 않고 직접 저장 → 기존 데이터 시뮬레이션
    page = Page(id=service.store.new_id("page"), space_id="s1", title="옛문서")
    service.store.add_page(page)
    loaded = service.store.get_page(page.id)
    assert loaded is not None
    assert loaded.created_by is None


def test_sqlite_migration_adds_column_to_legacy_file(tmp_path):
    """created_by 컬럼이 없는 실제 레거시 SQLite 파일을 열면 _migrate()가 컬럼을
    추가하고, 기존 행은 None으로 로드되며 새 write는 값을 보존한다."""
    import sqlite3

    from ahub.adapters.store_sqlite import SqliteStore

    db_path = str(tmp_path / "legacy.db")
    conn = sqlite3.connect(db_path)
    conn.execute(
        "CREATE TABLE pages ("
        "id TEXT PRIMARY KEY, space_id TEXT, title TEXT, body TEXT, source TEXT, "
        "parent_id TEXT, status TEXT, visibility TEXT, issue_id TEXT, "
        "steps TEXT, superseded_by TEXT, flags INTEGER)"
    )
    conn.execute(
        "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        ("legacy_1", "s1", "옛문서", "본문", "authored", None, "active", "org", None, "[]", None, 0),
    )
    conn.commit()
    conn.close()

    store = SqliteStore(db_path)  # __init__ → _migrate() runs ALTER TABLE
    legacy = store.get_page("legacy_1")
    assert legacy is not None
    assert legacy.created_by is None

    store.add_page(Page(id="new_1", space_id="s1", title="새문서", created_by="agt_1"))
    assert store.get_page("new_1").created_by == "agt_1"


# --- REST 레벨 ---


def _client() -> tuple[TestClient, str, str]:
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "Space One", purpose="p")
    c = TestClient(create_app(svc, mount_mcp=False))
    reg = c.post("/agents/register", json={"user_id": "bot", "name": "bot", "space_id": "s1"}).json()
    return c, reg["token"], reg["agent_id"]


def test_create_page_response_includes_created_by():
    c, tok, aid = _client()
    r = c.post(
        "/spaces/s1/pages",
        json={"title": "T", "body": "B"},
        headers={"authorization": f"Bearer {tok}"},
    )
    assert r.status_code == 201
    assert r.json()["created_by"] == aid


def test_get_page_response_includes_created_by():
    c, tok, aid = _client()
    pid = c.post(
        "/spaces/s1/pages",
        json={"title": "T"},
        headers={"authorization": f"Bearer {tok}"},
    ).json()["page_id"]
    got = c.get(f"/pages/{pid}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["created_by"] == aid


def test_search_response_includes_created_by():
    c, tok, aid = _client()
    c.post(
        "/spaces/s1/pages",
        json={"title": "findme unique", "body": "x"},
        headers={"authorization": f"Bearer {tok}"},
    )
    res = c.post(
        "/pages/search",
        json={"query": "findme", "space_id": "s1"},
        headers={"authorization": f"Bearer {tok}"},
    ).json()
    assert res["results"]
    assert res["results"][0]["created_by"] == aid


def test_tree_response_includes_created_by():
    c, tok, aid = _client()
    c.post(
        "/spaces/s1/pages",
        json={"title": "root"},
        headers={"authorization": f"Bearer {tok}"},
    )
    tree = c.get("/spaces/s1/tree", headers={"authorization": f"Bearer {tok}"}).json()
    assert tree["tree"]
    assert tree["tree"][0]["created_by"] == aid
