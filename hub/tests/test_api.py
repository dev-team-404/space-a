"""REST API 통합 테스트: 관리 API + 지식 등록을 HTTP로."""

import pytest
from fastapi.testclient import TestClient

from space_a.api.rest_server import create_app


@pytest.fixture
def client() -> TestClient:
    return TestClient(create_app())


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
    assert r.json()["doc_id"].startswith("doc_")


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


def _register(client, space="sw-innov"):
    client.post("/spaces", json={"id": space, "name": space})
    token = client.post("/agents/register", json={"name": "bot", "space_id": space}).json()["token"]
    return {"Authorization": f"Bearer {token}"}


def test_search_over_http(client):
    auth = _register(client)
    iss = client.post("/issues", json={"title": "t", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    client.post(f"/issues/{iss}/resolve", json={"summary": "DS 인증서 갱신"}, headers=auth)

    r = client.post("/pages/search", json={"query": "인증서"}, headers=auth)
    assert r.status_code == 200
    body = r.json()
    assert len(body["results"]) == 1
    assert body["results"][0]["summary"] == "DS 인증서 갱신"


def test_cite_over_http_creates_reuse_and_links(client):
    auth = _register(client)
    iss1 = client.post("/issues", json={"title": "seed", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    doc_id = client.post(f"/issues/{iss1}/resolve", json={"summary": "DS 인증서 갱신"}, headers=auth).json()["doc_id"]
    iss2 = client.post("/issues", json={"title": "again", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]

    r = client.post(f"/issues/{iss2}/cite", json={"doc_id": doc_id}, headers=auth)
    assert r.status_code == 200
    body = r.json()
    assert body["reuse_id"].startswith("reuse_")
    assert body["issue_status"] == "knowledge_linked"


def test_cite_unknown_doc_over_http_is_404(client):
    auth = _register(client)
    iss = client.post("/issues", json={"title": "t", "space_id": "sw-innov"}, headers=auth).json()["issue_id"]
    r = client.post(f"/issues/{iss}/cite", json={"doc_id": "doc_nope"}, headers=auth)
    assert r.status_code == 404
