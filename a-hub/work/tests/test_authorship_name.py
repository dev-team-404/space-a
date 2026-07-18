"""작성자(생성자) 정보에 사람이 읽을 name 병기 검증.

created_by(Page)·opened_by(Issue)는 agent_id다. 조회 응답에는 그 옆에
사람이 읽을 이름(created_by_name·opened_by_name)을 함께 실어야 한다
(README '알려진 이슈' ④). agent가 사라졌거나 저자가 없으면 name은 None이다.
"""

from fastapi.testclient import TestClient

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.rest_server import create_app
from ahub.core.services import SpaceAService


# --- 서비스 레벨: agent_name 리졸버 ---


def _register(service: SpaceAService):
    service.create_space("s1", "Space One", purpose="p")
    agent, token = service.register_agent("bot", "봇 하나", "s1")
    return agent, token


def test_agent_name_resolves_known_agent(service: SpaceAService):
    agent, _ = _register(service)
    assert service.agent_name(agent.id) == "봇 하나"


def test_agent_name_none_for_unknown_agent(service: SpaceAService):
    _register(service)
    assert service.agent_name("nope") is None


def test_agent_name_none_for_none_input(service: SpaceAService):
    _register(service)
    assert service.agent_name(None) is None


# --- REST 레벨 ---


def _client() -> tuple[TestClient, str, str, str]:
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "Space One", purpose="p")
    c = TestClient(create_app(svc, mount_mcp=False))
    reg = c.post(
        "/agents/register",
        json={"user_id": "bot", "name": "봇 하나", "space_id": "s1"},
    ).json()
    return c, reg["token"], reg["agent_id"], "봇 하나"


def test_create_page_response_includes_created_by_name():
    c, tok, aid, name = _client()
    r = c.post(
        "/spaces/s1/pages",
        json={"title": "T", "body": "B"},
        headers={"authorization": f"Bearer {tok}"},
    )
    assert r.status_code == 201
    body = r.json()
    assert body["created_by"] == aid
    assert body["created_by_name"] == name


def test_get_page_response_includes_created_by_name():
    c, tok, aid, name = _client()
    pid = c.post(
        "/spaces/s1/pages",
        json={"title": "T"},
        headers={"authorization": f"Bearer {tok}"},
    ).json()["page_id"]
    got = c.get(f"/pages/{pid}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["created_by"] == aid
    assert got["created_by_name"] == name


def test_search_response_includes_created_by_name():
    c, tok, aid, name = _client()
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
    assert res["results"][0]["created_by"] == aid
    assert res["results"][0]["created_by_name"] == name


def test_tree_response_includes_created_by_name():
    c, tok, aid, name = _client()
    c.post(
        "/spaces/s1/pages",
        json={"title": "root"},
        headers={"authorization": f"Bearer {tok}"},
    )
    tree = c.get("/spaces/s1/tree", headers={"authorization": f"Bearer {tok}"}).json()
    assert tree["tree"][0]["created_by"] == aid
    assert tree["tree"][0]["created_by_name"] == name


def test_list_issues_response_includes_opened_by_name():
    c, tok, aid, name = _client()
    c.post(
        "/issues",
        json={"title": "문제", "space_id": "s1"},
        headers={"authorization": f"Bearer {tok}"},
    )
    res = c.get("/issues", headers={"authorization": f"Bearer {tok}"}).json()
    assert res["issues"][0]["opened_by"] == aid
    assert res["issues"][0]["opened_by_name"] == name


def test_get_issue_response_includes_opened_by_name():
    c, tok, aid, name = _client()
    iid = c.post(
        "/issues",
        json={"title": "문제", "space_id": "s1"},
        headers={"authorization": f"Bearer {tok}"},
    ).json()["issue_id"]
    got = c.get(f"/issues/{iid}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["opened_by"] == aid
    assert got["opened_by_name"] == name


def test_author_name_is_none_when_author_missing():
    """저자 agent_id가 저장소에 없으면 name은 None (하위호환).

    작성자(author) 계정만 사라지고, 조회는 같은 방의 다른 유효한 계정(reader)이 한다.
    """
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "Space One")
    c = TestClient(create_app(svc, mount_mcp=False))
    author = c.post(
        "/agents/register", json={"user_id": "author", "name": "저자봇", "space_id": "s1"}
    ).json()
    reader = c.post(
        "/agents/register", json={"user_id": "reader", "name": "독자봇", "space_id": "s1"}
    ).json()
    pid = c.post(
        "/spaces/s1/pages",
        json={"title": "T"},
        headers={"authorization": f"Bearer {author['token']}"},
    ).json()["page_id"]
    # 작성자 계정만 저장소에서 제거해 '사라진 저자'를 시뮬레이션 (reader는 유효)
    svc.store._agents.pop(author["agent_id"], None)
    got = c.get(
        f"/pages/{pid}", headers={"authorization": f"Bearer {reader['token']}"}
    ).json()
    assert got["created_by"] == author["agent_id"]
    assert got["created_by_name"] is None
