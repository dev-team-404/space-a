"""방 방문(rooms) — 등록·입장·이동·겹침 금지·디자인. 설계: docs/design/room-visit.md"""

import pytest

from space_a.rooms import GRID_H, GRID_W, CellTaken, RoomService
from space_a.core import errors


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
    rooms.enter(token_b, room_a.id, cell=(5, 5))
    with pytest.raises(CellTaken):
        rooms.move(token_a, (5, 5))
    with pytest.raises(CellTaken):
        rooms.enter(token_a, room_a.id, cell=(5, 5))


def test_move_within_room_and_bounds(rooms):
    _, token, _ = rooms.register("A")
    me = rooms.move(token, (0, 0))
    assert me["cell"] == [0, 0]
    with pytest.raises(errors.InvalidRequest):
        rooms.move(token, (GRID_W, 0))
    with pytest.raises(errors.InvalidRequest):
        rooms.move(token, (0, -1))


def test_design_owner_only_and_furniture_blocks(rooms):
    _, token_a, room_a = rooms.register("A")
    _, token_b, _ = rooms.register("B")
    with pytest.raises(errors.Forbidden):
        rooms.set_design(token_b, room_a.id, {"objects": []})
    rooms.move(token_a, (0, 0))
    state = rooms.set_design(
        token_a, room_a.id, {"wallpaper": "mint", "objects": [{"kind": "plant", "cell": [3, 3]}]}
    )
    assert state["design"]["wallpaper"] == "mint"
    # 가구 셀은 이동 불가
    with pytest.raises(CellTaken):
        rooms.move(token_a, (3, 3))
    # 에이전트가 서 있는 셀에는 가구를 못 놓음
    with pytest.raises(CellTaken):
        rooms.set_design(token_a, room_a.id, {"objects": [{"kind": "rug", "cell": [0, 0]}]})


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
