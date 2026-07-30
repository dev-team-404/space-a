"""SQLite 영속화 — 재시작(새 LifeService + 같은 DB) 후 토큰·방·위치·디자인 복원."""

import sqlite3

from life_server.life import FLOOR_DEPTH, LifeService
from life_server.store import SqliteStore


def test_mascot_png_survives_restart(tmp_path):
    db = str(tmp_path / "life-mascot.db")
    service = LifeService(store=SqliteStore(db))
    agent, token, _ = service.register("mascot-owner", mascot_seed="seed")
    png = b"\x89PNG\r\n\x1a\noriginal-image"

    saved = service.set_mascot_image(token, png)
    restarted = LifeService(store=SqliteStore(db))

    assert saved["size"] == len(png)
    assert restarted.mascot_image(token, agent.agent_id)[0] == png


def test_profile_org_uuid_survive_restart(tmp_path):
    db = str(tmp_path / "life-profile.db")
    s1 = LifeService(store=SqliteStore(db))
    agent, _, _ = s1.register("owner", org="S/W 혁신팀", agent_uuid="uuid-xyz")

    # 재시작 흉내 — 같은 DB로 새 서비스가 로드
    s2 = LifeService(store=SqliteStore(db))
    loaded = s2._agents[agent.agent_id]
    assert loaded.org == "S/W 혁신팀"
    assert loaded.agent_uuid == "uuid-xyz"


def test_state_survives_restart(tmp_path):
    db = str(tmp_path / "life.db")
    s1 = LifeService(store=SqliteStore(db))
    _, token_a, life_a = s1.register("A", mascot_seed="seed-a")
    _, token_b, life_b = s1.register("B")
    s1.enter(token_a, life_b.id, cell=(3, 8))
    s1.set_design(token_b, life_b.id, {"wallpaper": "mint", "objects": [{"asset_id": "window.mint", "category": "window", "cell": [1, 0], "size": [3, 2], "rotation": 180, "wall": "north"}]})
    s1.rename(token_b, "B2")

    # 재시작 흉내 — 같은 DB로 새 서비스
    s2 = LifeService(store=SqliteStore(db))
    me = s2.me(token_a)  # 기존 토큰이 그대로 유효
    assert me["life_id"] == life_b.id
    assert me["cell"] == [3, 8]
    state = s2.life_state(life_b.id)
    assert state["owner_name"] == "B2"
    assert state["design"]["wallpaper"] == "mint"
    assert {"asset_id": "window.mint", "category": "window", "cell": [1, 0], "size": [3, 2], "rotation": 180, "wall": "north", "footprint": None} in state["design"]["objects"]
    assert s2.life_state(life_a.id)["owner_mascot_seed"] == "seed-a"


def test_same_name_reconnect_token_survives_restart(tmp_path):
    db = str(tmp_path / "life-reconnect.db")
    s1 = LifeService(store=SqliteStore(db))
    first_agent, _, first_life = s1.register("A", mascot_seed="seed-old")
    second_agent, reconnect_token, second_life = s1.register(" A ", mascot_seed="seed-new")

    assert second_agent.agent_id == first_agent.agent_id
    assert second_life.id == first_life.id

    s2 = LifeService(store=SqliteStore(db))
    assert s2.me(reconnect_token)["my_life_id"] == first_life.id
    assert s2.life_state(first_life.id)["owner_mascot_seed"] == "seed-new"
    assert len(s2.list_life()) == 1


def test_moves_persist(tmp_path):
    db = str(tmp_path / "life.db")
    s1 = LifeService(store=SqliteStore(db))
    _, token, _ = s1.register("A")
    s1.move(token, (7, 8))

    s2 = LifeService(store=SqliteStore(db))
    assert s2.me(token)["cell"] == [7, 8]


def test_custom_footprint_persists(tmp_path):
    db = str(tmp_path / "life-mask.db")
    s1 = LifeService(store=SqliteStore(db))
    _, token, life = s1.register("A")
    s1.move(token, (18, 0))
    mask = [[0, 0], [1, 0], [2, 0], [0, 1], [0, 2]]
    s1.set_design(token, life.id, {"objects": [{
        "asset_id": "sofa.corner", "category": "sofa", "cell": [1, 1],
        "size": [3, 3], "footprint": mask, "rotation": 0,
    }]})
    state = LifeService(store=SqliteStore(db)).life_state(life.id)
    assert state["design"]["objects"][0]["footprint"] == mask


def test_retro_tv_six_cell_footprint_persists(tmp_path):
    db = str(tmp_path / "life-tv.db")
    s1 = LifeService(store=SqliteStore(db))
    _, token, life = s1.register("A")
    s1.move(token, (18, 0))
    footprint = [[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]]
    s1.set_design(token, life.id, {"objects": [{
        "asset_id": "appliance.retro-tv", "category": "appliance", "cell": [1, 1],
        "size": [3, 2], "footprint": footprint, "rotation": 0,
    }]})

    saved = LifeService(store=SqliteStore(db)).life_state(life.id)["design"]["objects"][0]
    assert saved["size"] == [3, 2]
    assert saved["footprint"] == footprint


def test_legacy_window_rotation_is_migrated_from_wall(tmp_path):
    db = str(tmp_path / "life-window-v2.db")
    store = SqliteStore(db)
    s1 = LifeService(store=store)
    _, token, life = s1.register("A")
    s1.set_design(token, life.id, {"objects": [{
        "asset_id": "window.mint", "category": "window", "cell": [1, 0],
        "size": [3, 2], "rotation": 180, "wall": "north",
    }]})
    # v2 DB에 남아 있던 잘못된 자유 회전값을 재현한다.
    s1._life[life.id].design.objects[0].rotation = 0
    store.save_design(s1._life[life.id])

    state = LifeService(store=SqliteStore(db)).life_state(life.id)
    assert state["design"]["objects"][0]["rotation"] == 180


def test_half_depth_restart_drops_invalid_furniture_and_relocates_agent(tmp_path):
    db = str(tmp_path / "life-half-depth-v3.db")
    store = SqliteStore(db)
    s1 = LifeService(store=store)
    agent, token, life = s1.register("owner")
    # protocol 3 DB에 존재할 수 있었던 앞쪽 좌표와 가구를 직접 재현한다.
    store._conn.execute(
        "UPDATE agents SET x=19, y=19 WHERE agent_id=?",
        (agent.agent_id,),
    )
    store._conn.execute(
        "INSERT INTO life_objects "
        "(life_id,kind,x,y,category,w,h,rotation,wall,footprint) "
        "VALUES (?,?,?,?,?,?,?,?,?,?)",
        (life.id, "sofa.legacy-front", 17, 17, "sofa", 2, 2, 0, None, None),
    )
    store._conn.execute(
        "INSERT INTO life_objects "
        "(life_id,kind,x,y,category,w,h,rotation,wall,footprint) "
        "VALUES (?,?,?,?,?,?,?,?,?,?)",
        (life.id, "window.kept", 1, 0, "window", 3, 2, 180, "north", None),
    )
    store._conn.commit()

    restarted = LifeService(store=SqliteStore(db))
    state = restarted.life_state(life.id)
    assert [obj["asset_id"] for obj in state["design"]["objects"]] == ["window.kept"]
    assert sum(restarted.me(token)["cell"]) < FLOOR_DEPTH

    # 정리 결과가 영속되고 재실행해도 더 바뀌지 않는다.
    first_cell = restarted.me(token)["cell"]
    again = LifeService(store=SqliteStore(db))
    assert again.me(token)["cell"] == first_cell
    assert [obj["asset_id"] for obj in again.life_state(life.id)["design"]["objects"]] == ["window.kept"]


def test_legacy_combined_dining_assets_are_dropped(tmp_path):
    db = str(tmp_path / "life-dining-v1.db")
    s1 = LifeService(store=SqliteStore(db))
    _, token, life = s1.register("A")
    s1.move(token, (18, 0))
    s1.set_design(token, life.id, {"objects": [{
        "asset_id": "dining.square-two", "category": "dining", "cell": [1, 1],
        "size": [3, 4], "rotation": 0,
    }]})

    state = LifeService(store=SqliteStore(db)).life_state(life.id)
    assert state["design"]["objects"] == []


def test_guestbook_replies_survive_restart_and_cascade(tmp_path):
    db = str(tmp_path / "life-gb.db")
    s1 = LifeService(store=SqliteStore(db))
    _, owner_token, owner_life = s1.register("owner-bot")
    _, visitor_token, _ = s1.register("visitor-bot")
    entry = s1.add_guestbook(visitor_token, owner_life.id, "왔다감")
    reply = s1.add_guestbook(owner_token, owner_life.id, "고마워요", parent_id=entry["entry_id"])

    # 재시작 — parent_id 복원
    s2 = LifeService(store=SqliteStore(db))
    rows = {r["entry_id"]: r for r in s2.guestbook(owner_life.id)}
    assert rows[reply["entry_id"]]["parent_id"] == entry["entry_id"]
    assert rows[entry["entry_id"]]["parent_id"] is None

    # 원글 삭제 → DB에서도 답글 cascade — 재시작 후에도 비어 있다
    s2.delete_guestbook(visitor_token, entry["entry_id"])
    s3 = LifeService(store=SqliteStore(db))
    assert s3.guestbook(owner_life.id) == []


def test_legacy_guestbook_db_gains_parent_id_column(tmp_path):
    db = str(tmp_path / "life-legacy.db")
    # parent_id 없는 구 스키마 DB를 직접 구성
    conn = sqlite3.connect(db)
    conn.execute(
        "CREATE TABLE guestbook (entry_id TEXT PRIMARY KEY, life_id TEXT NOT NULL,"
        " author_agent_id TEXT NOT NULL, author_name TEXT NOT NULL, body TEXT NOT NULL,"
        " created_at TEXT NOT NULL)"
    )
    conn.execute("INSERT INTO guestbook VALUES ('gb_legacy', 'l1', 'a1', '옛손님', '옛글',"
                 " '2026-01-01T00:00:00+00:00')")
    conn.commit()
    conn.close()

    _, _, _, guestbook = SqliteStore(db).load_social()
    assert guestbook == [{"entry_id": "gb_legacy", "life_id": "l1", "author_agent_id": "a1",
                          "author_name": "옛손님", "body": "옛글", "parent_id": None,
                          "author_kind": None,
                          "created_at": "2026-01-01T00:00:00+00:00"}]


def test_guestbook_author_kind_survives_restart(tmp_path):
    db = str(tmp_path / "life-kind.db")
    s1 = LifeService(store=SqliteStore(db))
    _, _, owner_life = s1.register("owner-bot")
    _, visitor_token, _ = s1.register("visitor-bot")
    s1.add_guestbook(visitor_token, owner_life.id, "봇글", author_kind="bot")

    s2 = LifeService(store=SqliteStore(db))
    assert s2.guestbook(owner_life.id)[0]["author_kind"] == "bot"


def test_shared_identity_survives_restart(tmp_path):
    """공통 신원(주인 계정·풀네임·hub 연결)이 재시작 후에도 남는다."""
    db = str(tmp_path / "life-identity.db")
    s1 = LifeService(store=SqliteStore(db))
    agent, token, _ = s1.register(
        "소금맛",
        agent_uuid="uuid-1",
        owner_os_user="saltjeong",
        owner_full_name="정소금",
        hub_user_id="salt.jeong",
    )

    s2 = LifeService(store=SqliteStore(db))
    me = s2.me(token)

    assert me["identity"]["owner_os_user"] == "saltjeong"
    assert me["identity"]["owner_full_name"] == "정소금"
    assert me["identity"]["hub_user_id"] == "salt.jeong"
    assert me["agent_id"] == agent.agent_id


def test_old_db_without_identity_columns_migrates(tmp_path):
    """신원 컬럼이 없던 DB(팀 서버 현행)를 열어도 마이그레이션되고 기존 행이 살아 있다."""
    db = str(tmp_path / "life-old.db")
    conn = sqlite3.connect(db)
    conn.executescript(
        """
        CREATE TABLE life (life_id TEXT PRIMARY KEY, owner_agent_id TEXT NOT NULL,
          owner_name TEXT NOT NULL, wallpaper TEXT NOT NULL DEFAULT '', floor TEXT NOT NULL DEFAULT '');
        CREATE TABLE agents (agent_id TEXT PRIMARY KEY, name TEXT NOT NULL, life_id TEXT NOT NULL,
          at_life TEXT NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL,
          mascot_seed TEXT NOT NULL DEFAULT '', org TEXT NOT NULL DEFAULT '',
          agent_uuid TEXT NOT NULL DEFAULT '', bubble TEXT NOT NULL DEFAULT '',
          connected INTEGER NOT NULL DEFAULT 1);
        CREATE TABLE tokens (token TEXT PRIMARY KEY, agent_id TEXT NOT NULL);
        INSERT INTO life VALUES ('life_old', 'ragt_old', '돌쇠', '', '');
        INSERT INTO agents VALUES ('ragt_old', '돌쇠', 'life_old', 'life_old', 3, 4, 'seed', '', 'uuid-old', '', 1);
        INSERT INTO tokens VALUES ('tok-old', 'ragt_old');
        """
    )
    conn.commit()
    conn.close()

    service = LifeService(store=SqliteStore(db))
    me = service.me("tok-old")

    assert me["name"] == "돌쇠"
    assert me["identity"]["agent_uuid"] == "uuid-old"
    assert me["identity"]["hub_user_id"] == ""  # 새 컬럼은 빈 값으로 시작
    # 마이그레이션 후 지정도 된다
    service.set_hub_user("tok-old", "ragt_old", "palen")
    assert LifeService(store=SqliteStore(db)).me("tok-old")["identity"]["hub_user_id"] == "palen"


def test_daily_line_survives_restart(tmp_path):
    """O1 — 대문 한마디는 agents 컬럼에 영속된다."""
    db = str(tmp_path / "life-daily-line.db")
    service = LifeService(store=SqliteStore(db))
    _, token, created_life = service.register("front-door-owner")
    service.set_daily_line(token, "오늘도 묵묵히")

    restarted = LifeService(store=SqliteStore(db))
    assert restarted.life_state(created_life.id)["owner_daily_line"] == "오늘도 묵묵히"


def test_old_db_without_daily_line_column_migrates(tmp_path):
    """O1 — daily_line 컬럼이 없던 DB(팀 서버 현행)를 열어도 가산 마이그레이션되고 게시가 된다."""
    db = str(tmp_path / "life-no-daily-line.db")
    conn = sqlite3.connect(db)
    conn.executescript(
        """
        CREATE TABLE life (life_id TEXT PRIMARY KEY, owner_agent_id TEXT NOT NULL,
          owner_name TEXT NOT NULL, wallpaper TEXT NOT NULL DEFAULT '', floor TEXT NOT NULL DEFAULT '');
        CREATE TABLE agents (agent_id TEXT PRIMARY KEY, name TEXT NOT NULL, life_id TEXT NOT NULL,
          at_life TEXT NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL,
          mascot_seed TEXT NOT NULL DEFAULT '', org TEXT NOT NULL DEFAULT '',
          agent_uuid TEXT NOT NULL DEFAULT '', bubble TEXT NOT NULL DEFAULT '',
          connected INTEGER NOT NULL DEFAULT 1);
        CREATE TABLE tokens (token TEXT PRIMARY KEY, agent_id TEXT NOT NULL);
        INSERT INTO life VALUES ('life_old', 'ragt_old', '돌쇠', '', '');
        INSERT INTO agents VALUES ('ragt_old', '돌쇠', 'life_old', 'life_old', 3, 4, 'seed', '', 'uuid-old', '', 1);
        INSERT INTO tokens VALUES ('tok-old', 'ragt_old');
        """
    )
    conn.commit()
    conn.close()

    service = LifeService(store=SqliteStore(db))
    assert service.life_state("life_old")["owner_daily_line"] == ""  # 새 컬럼은 빈 값으로 시작
    service.set_daily_line("tok-old", "마이그레이션 후에도 걸린다")
    assert LifeService(store=SqliteStore(db)).life_state("life_old")["owner_daily_line"] == (
        "마이그레이션 후에도 걸린다"
    )


def test_daily_cut_png_survives_restart(tmp_path):
    """O1 — 대문사진 PNG와 방 레벨 해시가 재시작을 견딘다."""
    db = str(tmp_path / "life-daily-cut.db")
    service = LifeService(store=SqliteStore(db))
    agent, token, created_life = service.register("cut-owner")
    png = b"\x89PNG\r\n\x1a\nrestart-me"
    saved = service.set_daily_cut(token, png)

    restarted = LifeService(store=SqliteStore(db))
    assert restarted.daily_cut(token, agent.agent_id)[0] == png
    assert restarted.life_state(created_life.id)["owner_daily_cut_sha256"] == saved["sha256"]
