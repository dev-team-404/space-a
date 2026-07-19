"""REST 바인딩 — 인증·상태코드 매핑이 계약(room-visit.md §4)대로인지."""

import pytest
from fastapi.testclient import TestClient

from room_server.api import create_app


@pytest.fixture
def client() -> TestClient:
    return TestClient(create_app())


def _register(client, name):
    r = client.post("/rooms/register", json={"name": name})
    assert r.status_code == 201
    return r.json()


def test_capabilities_expose_orientation_contract(client):
    assert client.get("/capabilities").json() == {
        "room_protocol": 3,
        "grid": {"w": 20, "h": 20},
        "floor_min_y": 0,
        "footprint_mask": True,
        "wall_objects": True,
    }


def test_register_and_me(client):
    a = _register(client, "A")
    me = client.get("/rooms/me", headers={"Authorization": f"Bearer {a['token']}"})
    assert me.status_code == 200
    assert me.json()["my_room_id"] == a["room_id"]


def test_missing_token_is_401(client):
    r = client.get("/rooms/me")
    assert r.status_code == 401
    assert r.json()["error"]["code"] == "unauthorized"


def test_enter_conflict_is_409_cell_taken(client):
    a = _register(client, "A")
    b = _register(client, "B")
    hb = {"Authorization": f"Bearer {b['token']}"}
    ha = {"Authorization": f"Bearer {a['token']}"}
    assert client.post(f"/rooms/{a['room_id']}/enter", json={"cell": [5, 6]}, headers=hb).status_code == 200
    r = client.post(f"/rooms/{a['room_id']}/move", json={"cell": [5, 6]}, headers=ha)
    assert r.status_code == 409
    assert r.json()["error"]["code"] == "cell_taken"


def test_unknown_room_is_404(client):
    a = _register(client, "A")
    r = client.post("/rooms/room_none/enter", json={}, headers={"Authorization": f"Bearer {a['token']}"})
    assert r.status_code == 404


def test_design_forbidden_for_visitor(client):
    a = _register(client, "A")
    b = _register(client, "B")
    r = client.put(
        f"/rooms/{a['room_id']}/design",
        json={"wallpaper": "mint"},
        headers={"Authorization": f"Bearer {b['token']}"},
    )
    assert r.status_code == 403


# --- x-api-key 관문 (ROOM_SERVER_API_KEY) ---


def test_api_key_gate_blocks_without_key(monkeypatch):
    monkeypatch.setenv("ROOM_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/capabilities").status_code == 401
    assert c.get("/capabilities").json()["error"]["code"] == "unauthorized"


def test_api_key_gate_allows_with_matching_key(monkeypatch):
    monkeypatch.setenv("ROOM_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/capabilities", headers={"x-api-key": "secret"}).status_code == 200


def test_api_key_gate_exempts_health(monkeypatch):
    monkeypatch.setenv("ROOM_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/healthz").status_code == 200
    assert c.get("/readyz").status_code == 200


def test_api_key_gate_off_when_unset(client):
    # ROOM_SERVER_API_KEY 미설정이면 x-api-key 없이도 통과 (로컬·테스트 기본)
    assert client.get("/capabilities").status_code == 200
