"""REST API 통합 테스트: 관리 API + 지식 생애주기를 HTTP로."""

import pytest
from fastapi.testclient import TestClient

from space_a.api.rest_server import create_app


@pytest.fixture
def client() -> TestClient:
    return TestClient(create_app())


def _register(client, space="sw-innov"):
    client.post("/spaces", json={"id": space, "name": space})
    token = client.post(
        "/agents/register", json={"name": "bot", "space_id": space}
    ).json()["token"]
    return {"Authorization": f"Bearer {token}"}


def test_full_flow_register_open_resolve(client):
    r = client.post("/spaces", json={"id": "sw-innov", "name": "S/W 혁신팀"})
    assert r.status_code == 201

    r = client.post("/agents/register", json={"name": "build-bot", "space_id": "sw-innov"})
    assert r.status_code == 201
    body = r.json()
    assert body["agent_id"].startswith("agt_")
    assert body["spaces"] == ["sw-innov"]
    token = body["token"]

    auth = {"Authorization": f"Bearer {token}"}
    r = client.post("/issues", json={"title": "DS 인증서 오류", "space_id": "sw-innov"}, headers=auth)
    assert r.status_code == 201
    issue_id = r.json()["issue_id"]

    r = client.post(
        f"/issues/{issue_id}/resolve",
        json={"summary": "DS 인증서 갱신", "steps": ["재발급"]},
        headers=auth,
    )
    assert r.status_code == 200
    assert r.json()["status"] == "resolved"
    assert r.json()["page_id"].startswith("page_")


def test_open_issue_without_token_is_401(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    r = client.post("/issues", json={"title": "x", "space_id": "sw-innov"})
    assert r.status_code == 401


def test_open_issue_in_foreign_space_is_403(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    client.post("/spaces", json={"id": "ds-platform", "name": "DS"})
    token = client.post(
        "/agents/register", json={"name": "bot", "space_id": "sw-innov"}
    ).json()["token"]
    r = client.post(
        "/issues",
        json={"title": "x", "space_id": "ds-platform"},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert r.status_code == 403


def test_register_into_missing_space_is_404(client):
    r = client.post("/agents/register", json={"name": "bot", "space_id": "ghost"})
    assert r.status_code == 404


def test_search_over_http(client):
    auth = _register(client)
    iss = client.post("/issues", json={"title": "t", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    client.post(f"/issues/{iss}/resolve", json={"summary": "DS 인증서 갱신"}, headers=auth)

    r = client.post("/pages/search", json={"query": "인증서"}, headers=auth)
    assert r.status_code == 200
    body = r.json()
    assert len(body["results"]) == 1
    assert body["results"][0]["title"] == "DS 인증서 갱신"


def test_cite_over_http_creates_reuse_and_links(client):
    auth = _register(client)
    iss1 = client.post("/issues", json={"title": "seed", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    page_id = client.post(f"/issues/{iss1}/resolve", json={"summary": "DS 인증서 갱신"}, headers=auth).json()["page_id"]
    iss2 = client.post("/issues", json={"title": "again", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]

    r = client.post(f"/issues/{iss2}/cite", json={"page_id": page_id}, headers=auth)
    assert r.status_code == 200
    body = r.json()
    assert body["reuse_id"].startswith("reuse_")
    assert body["issue_status"] == "knowledge_linked"


def test_cite_unknown_page_over_http_is_404(client):
    auth = _register(client)
    iss = client.post("/issues", json={"title": "t", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    r = client.post(f"/issues/{iss}/cite", json={"page_id": "page_nope"}, headers=auth)
    assert r.status_code == 404


def test_create_and_get_page_over_http(client):
    auth = _register(client)
    r = client.post("/spaces/sw-innov/pages", json={"title": "온보딩", "body": "가이드"}, headers=auth)
    assert r.status_code == 201
    pid = r.json()["page_id"]
    assert pid.startswith("page_")
    assert r.json()["source"] == "authored"

    g = client.get(f"/pages/{pid}", headers=auth)
    assert g.status_code == 200
    assert g.json()["title"] == "온보딩"


def test_page_tree_over_http(client):
    auth = _register(client)
    parent = client.post("/spaces/sw-innov/pages", json={"title": "가이드"}, headers=auth).json()["page_id"]
    child = client.post(
        "/spaces/sw-innov/pages", json={"title": "세부", "parent_id": parent}, headers=auth
    ).json()["page_id"]

    r = client.get("/spaces/sw-innov/tree", headers=auth)
    assert r.status_code == 200
    tree = r.json()["tree"]
    assert len(tree) == 1
    assert tree[0]["page_id"] == parent
    assert tree[0]["children"][0]["page_id"] == child


def test_move_page_over_http(client):
    auth = _register(client)
    p1 = client.post("/spaces/sw-innov/pages", json={"title": "P1"}, headers=auth).json()["page_id"]
    p2 = client.post("/spaces/sw-innov/pages", json={"title": "P2"}, headers=auth).json()["page_id"]
    r = client.post(f"/pages/{p2}/move", json={"new_parent_id": p1}, headers=auth)
    assert r.status_code == 200
    assert r.json()["parent_id"] == p1


def test_list_and_get_space_over_http(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    client.post("/spaces", json={"id": "ds", "name": "DS"})

    r = client.get("/spaces")
    assert r.status_code == 200
    assert {s["id"] for s in r.json()["spaces"]} == {"sw-innov", "ds"}

    g = client.get("/spaces/sw-innov")
    assert g.status_code == 200
    assert g.json()["name"] == "S/W"


def test_list_and_get_issue_over_http(client):
    auth = _register(client)
    iid = client.post("/issues", json={"title": "t", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    client.post("/issues", json={"title": "u", "space_id": "sw-innov"}, headers=auth)

    r = client.get("/issues", headers=auth)
    assert r.status_code == 200
    assert len(r.json()["issues"]) == 2

    g = client.get(f"/issues/{iid}", headers=auth)
    assert g.status_code == 200
    assert g.json()["issue_id"] == iid


def test_update_and_archive_space_over_http(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})

    r = client.patch("/spaces/sw-innov", json={"name": "S/W 혁신팀"})
    assert r.status_code == 200
    assert r.json()["name"] == "S/W 혁신팀"

    a = client.post("/spaces/sw-innov/archive")
    assert a.status_code == 200
    assert a.json()["status"] == "archived"


def test_membership_over_http(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    client.post("/spaces", json={"id": "ds", "name": "DS"})
    x_tok = client.post("/agents/register", json={"name": "x", "space_id": "sw-innov"}).json()["token"]
    y = client.post("/agents/register", json={"name": "y", "space_id": "ds"}).json()
    xh = {"Authorization": f"Bearer {x_tok}"}

    r = client.post("/spaces/sw-innov/members", json={"agent_id": y["agent_id"]}, headers=xh)
    assert r.status_code == 201
    assert "sw-innov" in r.json()["spaces"]

    m = client.get("/spaces/sw-innov/members", headers=xh)
    assert len(m.json()["members"]) == 2

    d = client.delete(f"/spaces/sw-innov/members/{y['agent_id']}", headers=xh)
    assert d.status_code == 200


def test_agent_lifecycle_over_http(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    reg = client.post("/agents/register", json={"name": "x", "space_id": "sw-innov"}).json()
    tok, aid = reg["token"], reg["agent_id"]
    h = {"Authorization": f"Bearer {tok}"}

    assert client.get("/agents", headers=h).status_code == 200
    assert client.get(f"/agents/{aid}", headers=h).json()["name"] == "x"

    rot = client.post(f"/agents/{aid}/rotate-token", headers=h)
    assert rot.status_code == 200
    new = rot.json()["token"]
    assert new != tok
    assert client.get("/agents", headers=h).status_code == 401  # 옛 토큰 무효
    assert client.get("/agents", headers={"Authorization": f"Bearer {new}"}).status_code == 200


def test_health_endpoints(client):
    assert client.get("/healthz").json()["status"] == "ok"
    assert client.get("/readyz").json()["status"] == "ready"


def test_page_edit_and_archive_over_http(client):
    auth = _register(client)
    pid = client.post("/spaces/sw-innov/pages", json={"title": "가이드"}, headers=auth).json()["page_id"]

    e = client.patch(f"/pages/{pid}", json={"title": "새 제목"}, headers=auth)
    assert e.status_code == 200
    assert e.json()["title"] == "새 제목"

    a = client.post(f"/pages/{pid}/archive", headers=auth)
    assert a.json()["status"] == "archived"

    s = client.post("/pages/search", json={"query": "제목"}, headers=auth)
    assert all(r["page_id"] != pid for r in s.json()["results"])  # archived → 검색 제외


def test_page_flag_over_http(client):
    auth = _register(client)
    pid = client.post("/spaces/sw-innov/pages", json={"title": "의심"}, headers=auth).json()["page_id"]
    r = client.post(f"/pages/{pid}/flag", headers=auth)
    assert r.status_code == 200
    assert r.json()["flags"] == 1


def test_skill_candidates_over_http(client):
    auth = _register(client)
    for _ in range(3):
        iid = client.post("/issues", json={"title": "err", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
        client.post(f"/issues/{iid}/resolve", json={"summary": "인증서 갱신"}, headers=auth)
    r = client.get("/skills/candidates?min_occurrences=3", headers=auth)
    assert r.status_code == 200
    cands = r.json()["candidates"]
    assert len(cands) == 1
    assert cands[0]["occurrences"] == 3


def test_discovery_root(client):
    r = client.get("/")
    assert r.status_code == 200
    assert r.json()["service"] == "space-a-hub"
    assert "start_here" in r.json()


def test_space_purpose_and_guide_over_http(client):
    client.post(
        "/spaces",
        json={"id": "sw-innov", "name": "S/W", "purpose": "AI 협업", "guidelines": "문제만 기록"},
    )
    g = client.get("/spaces/sw-innov")
    assert g.json()["purpose"] == "AI 협업"

    guide = client.get("/spaces/sw-innov/guide")
    assert guide.status_code == 200
    assert guide.json()["body"] == "문제만 기록"


def test_no_guide_returns_404(client):
    client.post("/spaces", json={"id": "sw-innov", "name": "S/W"})
    assert client.get("/spaces/sw-innov/guide").status_code == 404
