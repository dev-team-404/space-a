"""SQLite 영속화 — 재시작(새 RoomService + 같은 DB) 후 토큰·방·위치·디자인 복원."""

from room_server.rooms import RoomService
from room_server.store import SqliteStore


def test_state_survives_restart(tmp_path):
    db = str(tmp_path / "rooms.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token_a, room_a = s1.register("A", mascot_seed="seed-a")
    _, token_b, room_b = s1.register("B")
    s1.enter(token_a, room_b.id, cell=(3, 8))
    s1.set_design(token_b, room_b.id, {"wallpaper": "mint", "objects": [{"asset_id": "window.mint", "category": "window", "cell": [1, 0], "size": [3, 2], "rotation": 180, "wall": "north"}]})
    s1.rename(token_b, "B2")

    # 재시작 흉내 — 같은 DB로 새 서비스
    s2 = RoomService(store=SqliteStore(db))
    me = s2.me(token_a)  # 기존 토큰이 그대로 유효
    assert me["room_id"] == room_b.id
    assert me["cell"] == [3, 8]
    state = s2.room_state(room_b.id)
    assert state["owner_name"] == "B2"
    assert state["design"]["wallpaper"] == "mint"
    assert {"asset_id": "window.mint", "category": "window", "cell": [1, 0], "size": [3, 2], "rotation": 180, "wall": "north", "footprint": None} in state["design"]["objects"]
    assert s2.room_state(room_a.id)["owner_mascot_seed"] == "seed-a"


def test_moves_persist(tmp_path):
    db = str(tmp_path / "rooms.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token, _ = s1.register("A")
    s1.move(token, (7, 8))

    s2 = RoomService(store=SqliteStore(db))
    assert s2.me(token)["cell"] == [7, 8]


def test_custom_footprint_persists(tmp_path):
    db = str(tmp_path / "rooms-mask.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token, room = s1.register("A")
    s1.move(token, (19, 19))
    mask = [[0, 0], [1, 0], [2, 0], [0, 1], [0, 2]]
    s1.set_design(token, room.id, {"objects": [{
        "asset_id": "sofa.corner", "category": "sofa", "cell": [1, 1],
        "size": [3, 3], "footprint": mask, "rotation": 0,
    }]})
    state = RoomService(store=SqliteStore(db)).room_state(room.id)
    assert state["design"]["objects"][0]["footprint"] == mask


def test_retro_tv_six_cell_footprint_persists(tmp_path):
    db = str(tmp_path / "rooms-tv.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token, room = s1.register("A")
    s1.move(token, (19, 19))
    footprint = [[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]]
    s1.set_design(token, room.id, {"objects": [{
        "asset_id": "appliance.retro-tv", "category": "appliance", "cell": [1, 1],
        "size": [3, 2], "footprint": footprint, "rotation": 0,
    }]})

    saved = RoomService(store=SqliteStore(db)).room_state(room.id)["design"]["objects"][0]
    assert saved["size"] == [3, 2]
    assert saved["footprint"] == footprint


def test_legacy_window_rotation_is_migrated_from_wall(tmp_path):
    db = str(tmp_path / "rooms-window-v2.db")
    store = SqliteStore(db)
    s1 = RoomService(store=store)
    _, token, room = s1.register("A")
    s1.set_design(token, room.id, {"objects": [{
        "asset_id": "window.mint", "category": "window", "cell": [1, 0],
        "size": [3, 2], "rotation": 180, "wall": "north",
    }]})
    # v2 DB에 남아 있던 잘못된 자유 회전값을 재현한다.
    s1._rooms[room.id].design.objects[0].rotation = 0
    store.save_design(s1._rooms[room.id])

    state = RoomService(store=SqliteStore(db)).room_state(room.id)
    assert state["design"]["objects"][0]["rotation"] == 180


def test_legacy_combined_dining_assets_are_dropped(tmp_path):
    db = str(tmp_path / "rooms-dining-v1.db")
    s1 = RoomService(store=SqliteStore(db))
    _, token, room = s1.register("A")
    s1.move(token, (19, 19))
    s1.set_design(token, room.id, {"objects": [{
        "asset_id": "dining.square-two", "category": "dining", "cell": [1, 1],
        "size": [3, 4], "rotation": 0,
    }]})

    state = RoomService(store=SqliteStore(db)).room_state(room.id)
    assert state["design"]["objects"] == []
