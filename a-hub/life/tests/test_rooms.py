"""방 방문(rooms) — 등록·입장·이동·겹침 금지·디자인. 설계: docs/design/room-visit.md"""

import pytest

from room_server import errors
from room_server.errors import CellTaken
from room_server.rooms import FLOOR_Y, GRID_H, GRID_W, SPAWN_X, SPAWN_Y, RoomService


@pytest.fixture
def rooms() -> RoomService:
    return RoomService()


def test_register_creates_room_and_auto_enters(rooms):
    agent, token, room = rooms.register("준녕")
    me = rooms.me(token)
    assert me["my_room_id"] == room.id
    assert me["room_id"] == room.id  # 자기 방에 자동 입장
    state = rooms.room_state(room.id)
    assert state["grid"] == {"w": GRID_W, "h": GRID_H}
    assert [o["agent_id"] for o in state["occupants"]] == [agent.agent_id]
    assert state["occupants"][0]["is_owner"] is True


def test_auto_spawn_at_spawn_point_then_nearby(rooms):
    _, token, room = rooms.register("A")
    # 빈 방의 첫 스폰 = 스폰 지점 (구석 아님)
    assert rooms.me(token)["cell"] == [SPAWN_X, SPAWN_Y]
    # 스폰 지점이 차 있으면 그 근처(체비셰프 거리 1 이내)에 배정
    _, token_b, _ = rooms.register("B")
    rooms.enter(token_b, room.id, cell=None)
    bx, by = rooms.me(token_b)["cell"]
    assert max(abs(bx - SPAWN_X), abs(by - SPAWN_Y)) == 1


def test_room_state_exposes_owner_seed_even_when_owner_away(rooms):
    _, token_a, room_a = rooms.register("A", mascot_seed="seed-a")
    _, _, room_b = rooms.register("B")
    rooms.enter(token_a, room_b.id, cell=None)  # 주인 A가 자기 방을 비움
    assert rooms.room_state(room_a.id)["owner_mascot_seed"] == "seed-a"


def test_visit_other_room_leaves_previous(rooms):
    _, token_a, room_a = rooms.register("A")
    _, _, room_b = rooms.register("B")
    rooms.enter(token_a, room_b.id, cell=None)
    assert rooms.me(token_a)["room_id"] == room_b.id
    # 이전 방에서는 사라짐
    assert all(o["name"] != "A" for o in rooms.room_state(room_a.id)["occupants"])
    # 방 목록의 인원 집계 반영
    counts = {r["room_id"]: r["occupants"] for r in rooms.list_rooms()}
    assert counts[room_a.id] == 0 and counts[room_b.id] == 2


def test_cell_conflict_is_atomic_reject(rooms):
    _, token_a, room_a = rooms.register("A")
    _, token_b, _ = rooms.register("B")
    rooms.enter(token_b, room_a.id, cell=(5, FLOOR_Y))
    with pytest.raises(CellTaken):
        rooms.move(token_a, (5, FLOOR_Y))
    with pytest.raises(CellTaken):
        rooms.enter(token_a, room_a.id, cell=(5, FLOOR_Y))


def test_move_within_room_and_bounds(rooms):
    _, token, _ = rooms.register("A")
    me = rooms.move(token, (0, FLOOR_Y))
    assert me["cell"] == [0, FLOOR_Y]
    with pytest.raises(errors.InvalidRequest):
        rooms.move(token, (GRID_W, 0))
    with pytest.raises(errors.InvalidRequest):
        rooms.move(token, (0, -1))


def test_design_owner_only_and_furniture_blocks(rooms):
    _, token_a, room_a = rooms.register("A")
    _, token_b, _ = rooms.register("B")
    with pytest.raises(errors.Forbidden):
        rooms.set_design(token_b, room_a.id, {"objects": []})
    rooms.move(token_a, (0, FLOOR_Y))
    state = rooms.set_design(
        token_a, room_a.id, {"wallpaper": "mint", "objects": [{"asset_id": "sofa.mint", "category": "sofa", "cell": [3, 8], "size": [3, 2], "rotation": 0}]}
    )
    assert state["design"]["wallpaper"] == "mint"
    # 가구 셀은 이동 불가
    with pytest.raises(CellTaken):
        rooms.move(token_a, (4, 9))
    # 에이전트가 서 있는 셀에는 가구를 못 놓음
    with pytest.raises(CellTaken):
        rooms.set_design(token_a, room_a.id, {"objects": [{"asset_id": "desk.one", "category": "desk", "cell": [0, FLOOR_Y], "size": [2, 2], "rotation": 0}]})


def test_wall_floor_and_rotation_footprint_rules(rooms):
    _, token, room = rooms.register("A")
    rooms.set_design(token, room.id, {"objects": [
        {"asset_id": "window.mint", "category": "window", "cell": [2, 0], "size": [3, 2], "rotation": 180, "wall": "north"},
        {"asset_id": "sofa.mint", "category": "sofa", "cell": [10, 8], "size": [4, 2], "rotation": 90},
    ]})
    # 4x2 소파를 90도 회전하면 2x4 footprint 전체가 막힌다.
    with pytest.raises(CellTaken):
        rooms.move(token, (11, 11))
    with pytest.raises(errors.InvalidRequest):
        rooms.move(token, (1, FLOOR_Y - 1))
    with pytest.raises(errors.InvalidRequest):
        rooms.set_design(token, room.id, {"objects": [
            {"asset_id": "window.bad", "category": "window", "cell": [0, 0], "size": [2, 2], "rotation": 0, "wall": "ceiling"},
        ]})
    with pytest.raises(errors.InvalidRequest):
        rooms.set_design(token, room.id, {"objects": [
            {"asset_id": "window.wrong-way", "category": "window", "cell": [0, 0], "size": [2, 2], "rotation": 90, "wall": "north"},
        ]})


def test_auto_cell_assignment_no_overlap(rooms):
    _, token_a, room_a = rooms.register("A")
    tokens = [rooms.register(f"G{i}")[1] for i in range(5)]
    for t in tokens:
        rooms.enter(t, room_a.id, cell=None)
    cells = [tuple(o["cell"]) for o in rooms.room_state(room_a.id)["occupants"]]
    assert len(cells) == len(set(cells)) == 6  # 주인 + 방문자 5, 전부 다른 셀


def test_unknown_token_rejected(rooms):
    with pytest.raises(errors.Unauthorized):
        rooms.me("no-such-token")


def test_shape_footprint_allows_interlocking_empty_cells(rooms):
    _, token, room = rooms.register("A")
    rooms.move(token, (19, 19))
    l_mask = [[0, 0], [1, 0], [2, 0], [0, 1], [0, 2]]
    state = rooms.set_design(token, room.id, {"objects": [
        {"asset_id": "sofa.corner", "category": "sofa", "cell": [1, 1], "size": [3, 3], "footprint": l_mask, "rotation": 0},
        {"asset_id": "lighting.lamp", "category": "lighting", "cell": [2, 2], "size": [1, 1], "footprint": [[0, 0]], "rotation": 0},
    ]})
    assert state["design"]["objects"][0]["footprint"] == l_mask
    with pytest.raises(CellTaken):
        rooms.move(token, (1, 2))
    with pytest.raises(CellTaken):
        rooms.move(token, (2, 2))


def test_rename_updates_agent_and_room(rooms):
    _, token, room = rooms.register("옛이름")
    me = rooms.rename(token, "새이름")
    assert me["name"] == "새이름"
    state = rooms.room_state(room.id)
    assert state["owner_name"] == "새이름"
    with pytest.raises(errors.InvalidRequest):
        rooms.rename(token, "  ")
