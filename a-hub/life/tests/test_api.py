"""REST 바인딩 — 인증·상태코드 매핑이 계약(life-visit.md §4)대로인지."""

import pytest
from fastapi.testclient import TestClient

from life_server.api import create_app


@pytest.fixture
def client() -> TestClient:
    return TestClient(create_app())


def _register(client, name):
    r = client.post("/life/register", json={"name": name})
    assert r.status_code == 201
    return r.json()


def test_capabilities_expose_orientation_contract(client):
    assert client.get("/capabilities").json() == {
        "life_protocol": 4,
        "grid": {"w": 20, "h": 20},
        "floor": {"shape": "half-depth", "max_xy_exclusive": 20},
        "floor_min_y": 0,
        "footprint_mask": True,
        "wall_objects": True,
    }


def test_register_and_me(client):
    a = _register(client, "A")
    me = client.get("/life/me", headers={"Authorization": f"Bearer {a['token']}"})
    assert me.status_code == 200
    assert me.json()["my_life_id"] == a["life_id"]


def test_register_same_name_returns_existing_life(client):
    first = _register(client, "A")
    second = _register(client, " A ")

    assert second["agent_id"] == first["agent_id"]
    assert second["life_id"] == first["life_id"]
    assert second["token"] != first["token"]
    assert len(client.get("/life").json()["life"]) == 1


def test_missing_token_is_401(client):
    r = client.get("/life/me")
    assert r.status_code == 401
    assert r.json()["error"]["code"] == "unauthorized"


def test_enter_conflict_is_409_cell_taken(client):
    a = _register(client, "A")
    b = _register(client, "B")
    hb = {"Authorization": f"Bearer {b['token']}"}
    ha = {"Authorization": f"Bearer {a['token']}"}
    assert client.post(f"/life/{a['life_id']}/enter", json={"cell": [5, 6]}, headers=hb).status_code == 200
    r = client.post(f"/life/{a['life_id']}/move", json={"cell": [5, 6]}, headers=ha)
    assert r.status_code == 409
    assert r.json()["error"]["code"] == "cell_taken"


def test_unknown_life_is_404(client):
    a = _register(client, "A")
    r = client.post("/life/life_none/enter", json={}, headers={"Authorization": f"Bearer {a['token']}"})
    assert r.status_code == 404


def test_design_forbidden_for_visitor(client):
    a = _register(client, "A")
    b = _register(client, "B")
    r = client.put(
        f"/life/{a['life_id']}/design",
        json={"wallpaper": "mint"},
        headers={"Authorization": f"Bearer {b['token']}"},
    )
    assert r.status_code == 403


# --- x-api-key 관문 (LIFE_SERVER_API_KEY) ---


def test_api_key_gate_blocks_without_key(monkeypatch):
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/capabilities").status_code == 401
    assert c.get("/capabilities").json()["error"]["code"] == "unauthorized"


def test_api_key_gate_allows_with_matching_key(monkeypatch):
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/capabilities", headers={"x-api-key": "secret"}).status_code == 200


def test_api_key_gate_exempts_health(monkeypatch):
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/healthz").status_code == 200
    assert c.get("/readyz").status_code == 200


def test_api_key_gate_exempts_docs(monkeypatch):
    # 관문을 켜도 브라우저로 API 문서를 열 수 있어야 한다 (헤더를 못 실으므로)
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/docs").status_code == 200
    assert c.get("/openapi.json").status_code == 200


def test_api_key_gate_rejects_wrong_key(monkeypatch):
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "secret")
    c = TestClient(create_app())
    assert c.get("/capabilities", headers={"x-api-key": "wrong"}).status_code == 401


def test_api_key_gate_off_when_unset(client):
    # LIFE_SERVER_API_KEY 미설정이면 x-api-key 없이도 통과 (로컬·테스트 기본)
    assert client.get("/capabilities").status_code == 200


def test_guestbook_add_accepts_optional_author_name(client):
    a = _register(client, "A")
    b = _register(client, "B")
    hb = {"Authorization": f"Bearer {b['token']}"}

    # author_name 전달 → 그대로 저장·반환
    r = client.post(f"/life/{a['life_id']}/guestbook",
                    json={"body": "왔다감", "author_name": "홍길동"}, headers=hb)
    assert r.status_code == 201
    assert r.json()["author_name"] == "홍길동"

    # 미전달 → 등록된 agent name (구클라 하위호환)
    r = client.post(f"/life/{a['life_id']}/guestbook", json={"body": "기본"}, headers=hb)
    assert r.status_code == 201
    assert r.json()["author_name"] == "B"

    # 80자 초과 → 400 (InvalidRequest 매핑)
    r = client.post(f"/life/{a['life_id']}/guestbook",
                    json={"body": "초과", "author_name": "a" * 81}, headers=hb)
    assert r.status_code == 400


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


# ── 공통 신원 (2026-07-29) — Life가 세 컴포넌트를 잇는 등록처 ──
# 스펙: docs/design/common/specs/2026-07-29-shared-identity-life-hub-lens.md


def test_register_keeps_owner_and_hub_identity(client):
    r = client.post(
        "/life/register",
        json={
            "name": "소금맛",
            "agent_uuid": "uuid-1",
            "org": "sw-innov",
            "owner_os_user": "saltjeong",
            "owner_full_name": "정소금",
            "hub_user_id": "salt.jeong",
        },
    )
    assert r.status_code == 201
    me = client.get("/life/me", headers={"Authorization": f"Bearer {r.json()['token']}"}).json()
    assert me["identity"] == {
        "agent_uuid": "uuid-1",
        "org": "sw-innov",
        "owner_os_user": "saltjeong",
        "owner_full_name": "정소금",
        "hub_user_id": "salt.jeong",
        "mascot_image_sha256": None,
    }


def test_reregister_without_identity_keeps_existing(client):
    """구버전 클라이언트가 재등록해도 사람이 지정해 둔 연결을 지우지 않는다."""
    first = client.post(
        "/life/register", json={"name": "돌쇠", "hub_user_id": "palen", "owner_os_user": "palen"}
    ).json()
    client.post("/life/register", json={"name": "돌쇠"})  # 신원 필드 없이 재등록
    me = client.get("/life/me", headers={"Authorization": f"Bearer {first['token']}"}).json()
    assert me["identity"]["hub_user_id"] == "palen"
    assert me["identity"]["owner_os_user"] == "palen"


def test_people_carries_identity(client):
    a = _register(client, "준냥헐")
    client.post("/life/register", json={"name": "palendy", "hub_user_id": "palendy"})
    people = client.get("/life/people", headers={"Authorization": f"Bearer {a['token']}"}).json()
    hub_ids = {p["name"]: p["identity"]["hub_user_id"] for p in people["people"]}
    assert hub_ids == {"palendy": "palendy"}


def test_set_hub_user_own_and_clear(client):
    a = _register(client, "kimmy")
    h = {"Authorization": f"Bearer {a['token']}"}
    r = client.patch(f"/life/agents/{a['agent_id']}/hub-user", json={"hub_user_id": "kimmy-claude"}, headers=h)
    assert r.status_code == 200 and r.json()["hub_user_id"] == "kimmy-claude"
    assert client.get("/life/me", headers=h).json()["identity"]["hub_user_id"] == "kimmy-claude"
    # 빈 문자열 = 연결 해제 (추론으로 복귀)
    client.patch(f"/life/agents/{a['agent_id']}/hub-user", json={"hub_user_id": ""}, headers=h)
    assert client.get("/life/me", headers=h).json()["identity"]["hub_user_id"] == ""


def test_set_hub_user_on_others_is_forbidden_without_admin_key(client):
    a = _register(client, "kimmy")
    b = _register(client, "돌쇠")
    r = client.patch(
        f"/life/agents/{b['agent_id']}/hub-user",
        json={"hub_user_id": "palen"},
        headers={"Authorization": f"Bearer {a['token']}"},
    )
    assert r.status_code == 403


def test_admin_key_can_link_others(monkeypatch):
    monkeypatch.setenv("LIFE_SERVER_API_KEY", "adminkey")
    c = TestClient(create_app())
    a = c.post("/life/register", json={"name": "kimmy"}, headers={"x-api-key": "adminkey"}).json()
    b = c.post("/life/register", json={"name": "돌쇠"}, headers={"x-api-key": "adminkey"}).json()
    r = c.patch(
        f"/life/agents/{b['agent_id']}/hub-user",
        json={"hub_user_id": "palen"},
        headers={"Authorization": f"Bearer {a['token']}", "x-api-key": "adminkey"},
    )
    assert r.status_code == 200 and r.json()["hub_user_id"] == "palen"
