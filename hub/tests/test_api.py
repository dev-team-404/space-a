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
