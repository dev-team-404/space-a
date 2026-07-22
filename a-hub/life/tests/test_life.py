"""방 방문(life) — 등록·입장·이동·겹침 금지·디자인. 설계: docs/design/life-visit.md"""

import pytest

from life_server import errors
from life_server.errors import CellTaken
from life_server.life import FLOOR_Y, GRID_H, GRID_W, SPAWN_X, SPAWN_Y, LifeService


@pytest.fixture
def life() -> LifeService:
    return LifeService()


def test_register_creates_life_and_auto_enters(life):
    agent, token, created_life = life.register("준녕")
    me = life.me(token)
    assert me["my_life_id"] == created_life.id
    assert me["life_id"] == created_life.id  # 자기 방에 자동 입장
    state = life.life_state(created_life.id)
    assert state["grid"] == {"w": GRID_W, "h": GRID_H}
    assert [o["agent_id"] for o in state["occupants"]] == [agent.agent_id]
    assert state["occupants"][0]["is_owner"] is True


def test_register_same_normalized_name_reuses_life(life):
    first_agent, first_token, first_life = life.register("  Same Name  ", mascot_seed="seed-old")
    second_agent, second_token, second_life = life.register("Same Name", mascot_seed="seed-new")

    assert second_agent.agent_id == first_agent.agent_id
    assert second_life.id == first_life.id
    assert second_token != first_token
    assert life.me(second_token)["my_life_id"] == first_life.id
    assert life.life_state(first_life.id)["owner_mascot_seed"] == "seed-new"
    assert len(life.list_life()) == 1


def test_auto_spawn_at_spawn_point_then_nearby(life):
    _, token, created_life = life.register("A")
    # 빈 방의 첫 스폰 = 스폰 지점 (구석 아님)
    assert life.me(token)["cell"] == [SPAWN_X, SPAWN_Y]
    # 스폰 지점이 차 있으면 그 근처(체비셰프 거리 1 이내)에 배정
    _, token_b, _ = life.register("B")
    life.enter(token_b, created_life.id, cell=None)
    bx, by = life.me(token_b)["cell"]
    assert max(abs(bx - SPAWN_X), abs(by - SPAWN_Y)) == 1


def test_life_state_exposes_owner_seed_even_when_owner_away(life):
    _, token_a, life_a = life.register("A", mascot_seed="seed-a")
    _, _, life_b = life.register("B")
    life.enter(token_a, life_b.id, cell=None)  # 주인 A가 자기 방을 비움
    assert life.life_state(life_a.id)["owner_mascot_seed"] == "seed-a"


def test_visit_other_life_leaves_previous(life):
    _, token_a, life_a = life.register("A")
    _, _, life_b = life.register("B")
    life.enter(token_a, life_b.id, cell=None)
    assert life.me(token_a)["life_id"] == life_b.id
    # 이전 방에서는 사라짐
    assert all(o["name"] != "A" for o in life.life_state(life_a.id)["occupants"])
    # 방 목록의 인원 집계 반영
    counts = {r["life_id"]: r["occupants"] for r in life.list_life()}
    assert counts[life_a.id] == 0 and counts[life_b.id] == 2


def test_cell_conflict_is_atomic_reject(life):
    _, token_a, life_a = life.register("A")
    _, token_b, _ = life.register("B")
    life.enter(token_b, life_a.id, cell=(5, FLOOR_Y))
    with pytest.raises(CellTaken):
        life.move(token_a, (5, FLOOR_Y))
    with pytest.raises(CellTaken):
        life.enter(token_a, life_a.id, cell=(5, FLOOR_Y))


def test_move_within_life_and_bounds(life):
    _, token, _ = life.register("A")
    me = life.move(token, (0, FLOOR_Y))
    assert me["cell"] == [0, FLOOR_Y]
    with pytest.raises(errors.InvalidRequest):
        life.move(token, (GRID_W, 0))
    with pytest.raises(errors.InvalidRequest):
        life.move(token, (0, -1))


def test_design_owner_only_and_furniture_blocks(life):
    _, token_a, life_a = life.register("A")
    _, token_b, _ = life.register("B")
    with pytest.raises(errors.Forbidden):
        life.set_design(token_b, life_a.id, {"objects": []})
    life.move(token_a, (0, FLOOR_Y))
    state = life.set_design(
        token_a, life_a.id, {"wallpaper": "mint", "objects": [{"asset_id": "sofa.mint", "category": "sofa", "cell": [3, 8], "size": [3, 2], "rotation": 0}]}
    )
    assert state["design"]["wallpaper"] == "mint"
    # 가구 셀은 이동 불가
    with pytest.raises(CellTaken):
        life.move(token_a, (4, 9))
    # 에이전트가 서 있는 셀에는 가구를 못 놓음
    with pytest.raises(CellTaken):
        life.set_design(token_a, life_a.id, {"objects": [{"asset_id": "desk.one", "category": "desk", "cell": [0, FLOOR_Y], "size": [2, 2], "rotation": 0}]})


def test_wall_floor_and_rotation_footprint_rules(life):
    _, token, created_life = life.register("A")
    life.set_design(token, created_life.id, {"objects": [
        {"asset_id": "window.mint", "category": "window", "cell": [2, 0], "size": [3, 2], "rotation": 180, "wall": "north"},
        {"asset_id": "sofa.mint", "category": "sofa", "cell": [10, 8], "size": [4, 2], "rotation": 90},
    ]})
    # 4x2 소파를 90도 회전하면 2x4 footprint 전체가 막힌다.
    with pytest.raises(CellTaken):
        life.move(token, (11, 11))
    with pytest.raises(errors.InvalidRequest):
        life.move(token, (1, FLOOR_Y - 1))
    with pytest.raises(errors.InvalidRequest):
        life.set_design(token, created_life.id, {"objects": [
            {"asset_id": "window.bad", "category": "window", "cell": [0, 0], "size": [2, 2], "rotation": 0, "wall": "ceiling"},
        ]})
    with pytest.raises(errors.InvalidRequest):
        life.set_design(token, created_life.id, {"objects": [
            {"asset_id": "window.wrong-way", "category": "window", "cell": [0, 0], "size": [2, 2], "rotation": 90, "wall": "north"},
        ]})


def test_auto_cell_assignment_no_overlap(life):
    _, token_a, life_a = life.register("A")
    tokens = [life.register(f"G{i}")[1] for i in range(5)]
    for t in tokens:
        life.enter(t, life_a.id, cell=None)
    cells = [tuple(o["cell"]) for o in life.life_state(life_a.id)["occupants"]]
    assert len(cells) == len(set(cells)) == 6  # 주인 + 방문자 5, 전부 다른 셀


def test_unknown_token_rejected(life):
    with pytest.raises(errors.Unauthorized):
        life.me("no-such-token")


def test_shape_footprint_allows_interlocking_empty_cells(life):
    _, token, created_life = life.register("A")
    life.move(token, (19, 19))
    l_mask = [[0, 0], [1, 0], [2, 0], [0, 1], [0, 2]]
    state = life.set_design(token, created_life.id, {"objects": [
        {"asset_id": "sofa.corner", "category": "sofa", "cell": [1, 1], "size": [3, 3], "footprint": l_mask, "rotation": 0},
        {"asset_id": "lighting.lamp", "category": "lighting", "cell": [2, 2], "size": [1, 1], "footprint": [[0, 0]], "rotation": 0},
    ]})
    assert state["design"]["objects"][0]["footprint"] == l_mask
    with pytest.raises(CellTaken):
        life.move(token, (1, 2))
    with pytest.raises(CellTaken):
        life.move(token, (2, 2))


def test_rename_updates_agent_and_life(life):
    _, token, created_life = life.register("옛이름")
    me = life.rename(token, "새이름")
    assert me["name"] == "새이름"
    state = life.life_state(created_life.id)
    assert state["owner_name"] == "새이름"
    with pytest.raises(errors.InvalidRequest):
        life.rename(token, "  ")
def test_social_visibility_guestbook_and_bubble():
    service = LifeService()
    owner, owner_token, owner_life = service.register("owner")
    friend, friend_token, _ = service.register("friend")
    stranger, stranger_token, _ = service.register("stranger")

    assert service.content_access(stranger_token, owner_life.id)["features"]["diary"] == {
        "visibility": "private", "can_view": False,
    }
    assert service.content_access(owner_token, owner_life.id)["features"]["diary"]["can_view"] is True
    service.set_content_visibility(owner_token, "diary", "friends")
    service.share_diary(owner_token, "2026-07-22", "friends only", "friends")
    assert service.shared_diaries(stranger_token, owner_life.id) == []
    service.set_friend(owner_token, friend.agent_id, True)
    assert service.shared_diaries(friend_token, owner_life.id)[0]["body"] == "friends only"
    service.set_content_visibility(owner_token, "diary", "public")
    service.share_diary(owner_token, "2026-07-23", "everyone", "public")
    assert {row["date"] for row in service.shared_diaries(stranger_token, owner_life.id)} == {"2026-07-22", "2026-07-23"}
    service.set_content_visibility(owner_token, "diary", "private")
    assert service.shared_diaries(stranger_token, owner_life.id) == []

    entry = service.add_guestbook(friend_token, owner_life.id, "다녀갑니다")
    assert service.guestbook(owner_life.id)[0]["body"] == "다녀갑니다"
    service.delete_guestbook(owner_token, entry["entry_id"])
    assert service.guestbook(owner_life.id) == []

    service.set_bubble(owner_token, "집중 중")
    assert service.life_state(owner_life.id)["occupants"][0]["bubble"] == "집중 중"
    service.disconnect(owner_token)
    assert all(row["agent_id"] != owner.agent_id for row in service.life_state(owner_life.id)["occupants"])
    service.register("owner")
    assert any(row["agent_id"] == owner.agent_id for row in service.life_state(owner_life.id)["occupants"])
