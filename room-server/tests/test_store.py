"""SQLite 영속화 — 재시작(새 RoomService + 같은 DB) 후 토큰·방·위치·디자인 복원."""

from room_server.rooms import RoomService
from room_server.store import SqliteStore


def test_state_survives_restart(tmp_path):
    db = str(tmp_path / "rooms.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token_a, room_a = s1.register("A", mascot_seed="seed-a")
    _, token_b, room_b = s1.register("B")
    s1.enter(token_a, room_b.id, cell=(3, 3))
    s1.set_design(token_b, room_b.id, {"wallpaper": "mint", "objects": [{"kind": "plant", "cell": [1, 1]}]})
    s1.rename(token_b, "B2")

    # 재시작 흉내 — 같은 DB로 새 서비스
    s2 = RoomService(store=SqliteStore(db))
    me = s2.me(token_a)  # 기존 토큰이 그대로 유효
    assert me["room_id"] == room_b.id
    assert me["cell"] == [3, 3]
    state = s2.room_state(room_b.id)
    assert state["owner_name"] == "B2"
    assert state["design"]["wallpaper"] == "mint"
    assert {"kind": "plant", "cell": [1, 1]} in state["design"]["objects"]
    assert s2.room_state(room_a.id)["owner_mascot_seed"] == "seed-a"


def test_moves_persist(tmp_path):
    db = str(tmp_path / "rooms.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token, _ = s1.register("A")
    s1.move(token, (7, 2))

    s2 = RoomService(store=SqliteStore(db))
    assert s2.me(token)["cell"] == [7, 2]
