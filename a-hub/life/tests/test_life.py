"""방 방문(life) — 등록·입장·이동·겹침 금지·디자인. 설계: docs/design/life-visit.md"""

import pytest

from life_server import errors
from life_server.errors import CellTaken
from life_server.life import FLOOR_DEPTH, FLOOR_Y, GRID_H, GRID_W, SPAWN_X, SPAWN_Y, LifeService


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


def test_register_stores_org_and_uuid(life):
    agent, _, _ = life.register("준녕", org="S/W 혁신팀", agent_uuid="uuid-123")
    assert agent.org == "S/W 혁신팀"
    assert agent.agent_uuid == "uuid-123"
    # 빈 값으로 재등록(재연결)해도 기존 프로필은 유지된다
    again, _, _ = life.register("준녕")
    assert again.org == "S/W 혁신팀"
    assert again.agent_uuid == "uuid-123"


def test_register_same_normalized_name_reuses_life(life):
    first_agent, first_token, first_life = life.register("  Same Name  ", mascot_seed="seed-old")
    second_agent, second_token, second_life = life.register("Same Name", mascot_seed="seed-new")

    assert second_agent.agent_id == first_agent.agent_id
    assert second_life.id == first_life.id
    assert second_token != first_token
    assert life.me(second_token)["my_life_id"] == first_life.id
    assert life.life_state(first_life.id)["owner_mascot_seed"] == "seed-new"
    assert len(life.list_life()) == 1


def test_auto_spawn_at_spawn_point_then_buffered(life):
    _, token, created_life = life.register("A")
    # 빈 방의 첫 스폰 = 스폰 지점 (구석 아님) — 회귀 가드
    assert life.me(token)["cell"] == [SPAWN_X, SPAWN_Y]
    # 스폰 지점에 주인이 있으면 방문자는 버퍼(체비셰프 거리 3)만큼 떨어져 배정
    _, token_b, _ = life.register("B")
    life.enter(token_b, created_life.id, cell=None)
    bx, by = life.me(token_b)["cell"]
    assert max(abs(bx - SPAWN_X), abs(by - SPAWN_Y)) == 3


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
    assert life.move(token, (10, 9))["cell"] == [10, 9]
    with pytest.raises(errors.InvalidRequest):
        life.move(token, (10, 10))


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
        {"asset_id": "sofa.mint", "category": "sofa", "cell": [8, 5], "size": [4, 2], "rotation": 90},
    ]})
    # 4x2 소파를 90도 회전하면 2x4 footprint 전체가 막힌다.
    with pytest.raises(CellTaken):
        life.move(token, (9, 8))
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
    life.move(token, (18, 0))
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


def test_design_rejects_furniture_crossing_half_depth_boundary(life):
    _, token, created_life = life.register("A")
    life.move(token, (0, 0))
    with pytest.raises(errors.InvalidRequest):
        life.set_design(token, created_life.id, {"objects": [{
            "asset_id": "sofa.edge", "category": "sofa", "cell": [17, 1],
            "size": [2, 2], "rotation": 0,
        }]})


def test_all_auto_spawn_cells_stay_inside_half_depth_floor(life):
    _, _, host = life.register("host")
    tokens = [life.register(f"guest-{i}")[1] for i in range(20)]
    for token in tokens:
        life.enter(token, host.id, None)
    assert all(
        0 <= occupant["cell"][0] < GRID_W
        and 0 <= occupant["cell"][1] < GRID_H
        and sum(occupant["cell"]) < FLOOR_DEPTH
        for occupant in life.life_state(host.id)["occupants"]
    )


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


def test_guestbook_author_name_override_and_delete_permission():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    visitor, visitor_token, _ = service.register("visitor-bot")

    # 전달 시 그대로 저장 (G1: 사람 작성 = 풀네임)
    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감", author_name="홍길동")
    assert entry["author_name"] == "홍길동"
    assert service.guestbook(owner_life.id)[0]["author_name"] == "홍길동"
    # author_agent_id는 토큰 주체 그대로 — 삭제 권한 판정 불변
    assert entry["author_agent_id"] == visitor.agent_id
    service.delete_guestbook(visitor_token, entry["entry_id"])
    assert service.guestbook(owner_life.id) == []


def test_guestbook_author_name_fallbacks_and_length_limit():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    # 미제공/공백 → 등록된 agent name (현행 동작·구클라 하위호환)
    assert service.add_guestbook(visitor_token, owner_life.id, "기본")["author_name"] == "visitor-bot"
    assert service.add_guestbook(visitor_token, owner_life.id, "공백", author_name="   ")["author_name"] == "visitor-bot"

    # 경계: trim 후 80자 허용, 81자 거부
    assert service.add_guestbook(visitor_token, owner_life.id, "한도", author_name="a" * 80)["author_name"] == "a" * 80
    with pytest.raises(errors.InvalidRequest):
        service.add_guestbook(visitor_token, owner_life.id, "초과", author_name="a" * 81)


def test_guestbook_reply_one_depth_owner_only():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    assert entry["parent_id"] is None

    reply = service.add_guestbook(owner_token, owner_life.id, "고마워요", parent_id=entry["entry_id"])
    assert reply["parent_id"] == entry["entry_id"]

    # 방 주인이 아니면 답글 불가
    with pytest.raises(errors.Forbidden):
        service.add_guestbook(visitor_token, owner_life.id, "저도요", parent_id=entry["entry_id"])

    # 답글의 답글(2단계) 불가
    with pytest.raises(errors.InvalidRequest):
        service.add_guestbook(owner_token, owner_life.id, "중첩", parent_id=reply["entry_id"])


def test_guestbook_reply_parent_must_exist_in_same_life():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, _, other_life = service.register("other-bot")

    with pytest.raises(errors.NotFound):
        service.add_guestbook(owner_token, owner_life.id, "고아", parent_id="gb_missing")

    # 다른 방의 원글을 부모로 지정할 수 없다
    foreign = service.add_guestbook(owner_token, other_life.id, "남의 방 글")
    with pytest.raises(errors.NotFound):
        service.add_guestbook(owner_token, owner_life.id, "크로스", parent_id=foreign["entry_id"])


def test_guestbook_multiple_replies_and_cascade_delete():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    r1 = service.add_guestbook(owner_token, owner_life.id, "첫 답글", parent_id=entry["entry_id"])
    r2 = service.add_guestbook(owner_token, owner_life.id, "둘째 답글", parent_id=entry["entry_id"])
    assert {r["entry_id"] for r in service.guestbook(owner_life.id)} == {
        entry["entry_id"], r1["entry_id"], r2["entry_id"]}

    # 원글 삭제(작성자) → 답글도 함께(cascade)
    service.delete_guestbook(visitor_token, entry["entry_id"])
    assert service.guestbook(owner_life.id) == []


def test_guestbook_reply_deletes_alone_without_touching_parent():
    service = LifeService()
    _, owner_token, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    entry = service.add_guestbook(visitor_token, owner_life.id, "왔다감")
    reply = service.add_guestbook(owner_token, owner_life.id, "답글", parent_id=entry["entry_id"])
    service.delete_guestbook(owner_token, reply["entry_id"])
    assert [r["entry_id"] for r in service.guestbook(owner_life.id)] == [entry["entry_id"]]


def test_visit_spawn_keeps_buffer_between_agents(life):
    _, _, owner_life = life.register("owner")
    _, token_b, _ = life.register("B")
    _, token_c, _ = life.register("C")
    life.enter(token_b, owner_life.id, cell=None)
    life.enter(token_c, owner_life.id, cell=None)
    cells = [tuple(o["cell"]) for o in life.life_state(owner_life.id)["occupants"]]
    assert len(cells) == 3  # 주인 + 방문자 2
    for i in range(len(cells)):
        for j in range(i + 1, len(cells)):
            (x1, y1), (x2, y2) = cells[i], cells[j]
            assert max(abs(x1 - x2), abs(y1 - y2)) >= 3


def test_visit_spawn_is_deterministic_for_same_agent(life):
    _, _, owner_life = life.register("owner")
    _, token_b, b_life = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    first = life.me(token_b)["cell"]
    life.enter(token_b, b_life.id, cell=None)      # 집으로 돌아감
    life.enter(token_b, owner_life.id, cell=None)  # 같은 방 상태에서 재방문
    assert life.me(token_b)["cell"] == first


def test_visit_spawn_scatters_across_agents(life):
    _, _, owner_life = life.register("owner")
    cells = set()
    for i in range(5):
        _, token, guest_life = life.register(f"G{i}")
        life.enter(token, owner_life.id, cell=None)
        cells.add(tuple(life.me(token)["cell"]))
        life.enter(token, guest_life.id, cell=None)  # 다음 프로브를 위해 집으로
    # 매번 '주인만 있는 방'이라는 같은 조건 — 에이전트별 해시로 자리가 전부 같지는 않다
    assert len(cells) >= 2


def test_visit_spawn_avoids_furniture_footprint(life):
    _, owner_token, owner_life = life.register("owner")
    # 앵커 위쪽 거리 3 링 일부를 소파(7x2)로 덮어도 가구 위에는 스폰되지 않는다
    life.set_design(owner_token, owner_life.id, {"objects": [
        {"asset_id": "sofa.big", "category": "sofa", "cell": [SPAWN_X - 3, SPAWN_Y - 3],
         "size": [7, 2], "rotation": 0},
    ]})
    _, token_b, _ = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    bx, by = life.me(token_b)["cell"]
    blocked = {(SPAWN_X - 3 + dx, SPAWN_Y - 3 + dy) for dx in range(7) for dy in range(2)}
    assert (bx, by) not in blocked


def _fill_grid_agents(life, owner_life_id):
    """반깊이 바닥의 4셀 간격 격자점에 에이전트 배치.

    유효 삼각형 어디서든 거리 3짜리 빈 칸이 존재하지 않게 한다.
    """
    for gx in (1, 5, 9, 13, 17):
        for gy in (1, 5, 9, 13, 17):
            if gx + gy >= FLOOR_DEPTH:
                continue
            if (gx, gy) == (SPAWN_X, SPAWN_Y):
                continue
            _, token, _ = life.register(f"grid-{gx}-{gy}")
            life.enter(token, owner_life_id, cell=(gx, gy))


def _cover_floor_except(life, owner_token, life_id, holes):
    """바닥 전체를 가구 footprint로 덮되 holes만 비운다. 주인이 선 칸은 holes에 포함해야 한다."""
    objects = []
    for bx in (0, 8, 16):
        for by in (0, 8, 16):
            w, h = min(8, GRID_W - bx), min(8, GRID_H - by)
            cells = [
                [x, y] for y in range(h) for x in range(w)
                if bx + x + by + y < FLOOR_DEPTH and (bx + x, by + y) not in holes
            ]
            if cells:
                objects.append({"asset_id": f"block-{bx}-{by}", "category": "block",
                                "cell": [bx, by], "size": [w, h], "footprint": cells, "rotation": 0})
    life.set_design(owner_token, life_id, {"objects": objects})


def test_spawn_buffer_relaxes_to_two_when_three_impossible(life):
    _, _, owner_life = life.register("owner")
    _fill_grid_agents(life, owner_life.id)
    _, token_v, _ = life.register("visitor")
    life.enter(token_v, owner_life.id, cell=None)
    vx, vy = life.me(token_v)["cell"]
    dists = [max(abs(vx - o["cell"][0]), abs(vy - o["cell"][1]))
             for o in life.life_state(owner_life.id)["occupants"] if o["name"] != "visitor"]
    # 격자 간격 4라 어떤 칸도 거리 3 불가·2는 가능 — 사다리가 2로 완화한 자리여야 한다
    assert min(dists) == 2


def test_spawn_buffer_fully_relaxes_before_full(life):
    _, owner_token, owner_life = life.register("owner")
    _cover_floor_except(life, owner_token, owner_life.id,
                        holes={(SPAWN_X, SPAWN_Y), (SPAWN_X, SPAWN_Y + 1)})
    _, token_v, _ = life.register("visitor")
    life.enter(token_v, owner_life.id, cell=None)
    # 남은 빈 칸이 주인 옆칸뿐 — 409가 아니라 버퍼 없이 배정된다
    assert life.me(token_v)["cell"] == [SPAWN_X, SPAWN_Y + 1]


def test_spawn_full_room_still_rejects(life):
    _, owner_token, owner_life = life.register("owner")
    _cover_floor_except(life, owner_token, owner_life.id, holes={(SPAWN_X, SPAWN_Y)})
    _, token_v, _ = life.register("visitor")
    with pytest.raises(CellTaken):
        life.enter(token_v, owner_life.id, cell=None)


def test_reenter_same_room_keeps_own_cell(life):
    _, _, owner_life = life.register("owner")
    _, token_b, _ = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    first = life.me(token_b)["cell"]
    # 같은 방 자동 재입장(재연결 경로) — 자기 옛 자리가 점유·버퍼로 잡히면 자리가 튄다
    life.enter(token_b, owner_life.id, cell=None)
    assert life.me(token_b)["cell"] == first


def test_guestbook_author_kind_roundtrip_and_default():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")

    # P3: 봇 작성 글은 author_kind="bot"으로 저장·에코
    entry = service.add_guestbook(visitor_token, owner_life.id, "봇이 왔다감", author_kind="bot")
    assert entry["author_kind"] == "bot"
    assert service.guestbook(owner_life.id)[0]["author_kind"] == "bot"

    # 미제공/공백 → None (human 간주는 소비자 몫 — 구클라 하위호환)
    assert service.add_guestbook(visitor_token, owner_life.id, "기본")["author_kind"] is None
    assert service.add_guestbook(visitor_token, owner_life.id, "공백", author_kind="  ")["author_kind"] is None
    assert service.add_guestbook(visitor_token, owner_life.id, "사람", author_kind="human")["author_kind"] == "human"


def test_guestbook_author_kind_rejects_unknown_value():
    service = LifeService()
    _, _, owner_life = service.register("owner-bot")
    _, visitor_token, _ = service.register("visitor-bot")
    with pytest.raises(errors.InvalidRequest):
        service.add_guestbook(visitor_token, owner_life.id, "이상값", author_kind="alien")


def test_daily_line_is_room_level_and_survives_owner_leaving():
    """O1 — 대문 한마디는 방 레벨이다. 주인이 남의 방에 가 있어도 그 방 대문에 걸려 있어야 한다."""
    service = LifeService()
    _, owner_token, owner_life = service.register("owner")
    _, visitor_token, visitor_life = service.register("visitor")

    assert service.life_state(owner_life.id)["owner_daily_line"] == ""

    assert service.set_daily_line(owner_token, "  밤샘 끝, 뿌듯  ") == {"daily_line": "밤샘 끝, 뿌듯"}
    assert service.life_state(owner_life.id)["owner_daily_line"] == "밤샘 끝, 뿌듯"

    # 주인이 방문자의 방으로 이동 — 자기 방 대문은 그대로 걸려 있다
    service.enter(owner_token, visitor_life.id, None)
    assert service.life_state(owner_life.id)["owner_daily_line"] == "밤샘 끝, 뿌듯"

    # 빈 문자열은 지움
    assert service.set_daily_line(owner_token, "   ") == {"daily_line": ""}
    assert service.life_state(owner_life.id)["owner_daily_line"] == ""

    with pytest.raises(errors.InvalidRequest):
        service.set_daily_line(owner_token, "가" * 121)
    # 남의 대문은 건드릴 수 없다 — 토큰이 곧 대상이므로 방문자 토큰은 자기 것만 바꾼다
    service.set_daily_line(visitor_token, "나도 한마디")
    assert service.life_state(owner_life.id)["owner_daily_line"] == ""
    assert service.life_state(visitor_life.id)["owner_daily_line"] == "나도 한마디"


def test_daily_cut_requires_png_and_dedups(tmp_path):
    """O1 — 대문사진: PNG 시그니처·5 MiB 검증, 같은 sha면 쓰기 생략, 방 레벨 해시 노출."""
    from life_server.store import SqliteStore

    service = LifeService(store=SqliteStore(str(tmp_path / "cut.db")))
    agent, token, created_life = service.register("cut-owner")
    _, other_token, _ = service.register("other")
    png = b"\x89PNG\r\n\x1a\nfront-door-cut"

    assert service.life_state(created_life.id)["owner_daily_cut_sha256"] is None

    saved = service.set_daily_cut(token, png)
    assert saved["size"] == len(png)
    assert service.life_state(created_life.id)["owner_daily_cut_sha256"] == saved["sha256"]
    # 방문객이 주인의 대문사진을 가져온다
    assert service.daily_cut(other_token, agent.agent_id) == (png, saved["sha256"])

    # 같은 바이트 재업로드는 같은 해시 (쓰기 생략 경로도 응답은 동일)
    assert service.set_daily_cut(token, png)["sha256"] == saved["sha256"]

    with pytest.raises(errors.InvalidRequest):
        service.set_daily_cut(token, b"not-a-png")
    with pytest.raises(errors.InvalidRequest):
        service.set_daily_cut(token, b"\x89PNG\r\n\x1a\n" + b"x" * (5 * 1024 * 1024))
    with pytest.raises(errors.NotFound):
        service.daily_cut(token, "ragt_nope")

    # 마스코트 이미지와 다른 슬롯이다 — 컷을 올려도 방 안 로봇 렌더용 해시는 그대로 None
    assert service.life_state(created_life.id)["owner_mascot_image_sha256"] is None


def test_daily_cut_missing_is_not_found():
    """O1 — 컷을 안 올린 사람의 대문사진 조회는 404 (클라이언트는 마스코트 이미지로 폴백)."""
    service = LifeService()
    agent, token, _ = service.register("no-cut")
    with pytest.raises(errors.NotFound):
        service.daily_cut(token, agent.agent_id)


# ── 프레즌스 last_seen (2026-07-30) ─────────────────────────────
#
# `connected`만으로는 "연결을 설정해뒀나"밖에 알 수 없다 — register/enter에서 True가 되고
# 수동 disconnect로만 False가 된다. 신선도의 근거는 `last_seen` 하나뿐이다.


def test_authed_call_refreshes_last_seen_and_is_exposed():
    """인증된 호출이면 무엇이든(읽기 포함) last_seen이 갱신된다 — 전용 하트비트가 필요 없다."""
    service = LifeService()
    _, token, _ = service.register("presence-bot")

    first = service.me(token)["last_seen"]
    assert first is not None, "register 직후 인증 호출로 관측 이력이 생긴다"

    # 읽기 전용 호출도 하트비트다 (a-mate가 스캔마다 하는 게 바로 이 부류)
    service.people(token)
    second = service.me(token)["last_seen"]
    assert second >= first


def test_last_seen_visible_to_others_in_people():
    """남의 프레즌스도 보여야 한다 — a-lens가 팀 전체 온라인/오프라인을 그리는 재료."""
    service = LifeService()
    _, me_token, _ = service.register("viewer")
    other, other_token, _ = service.register("worker")
    service.me(other_token)  # worker가 활동

    row = next(p for p in service.people(me_token) if p["agent_id"] == other.agent_id)
    assert row["connected"] is True
    assert row["last_seen"] is not None


def test_disconnect_keeps_last_seen_but_clears_connected():
    """수동 연결 끊기는 connected만 내린다 — 마지막 관측 시각은 사실이므로 지우지 않는다."""
    service = LifeService()
    _, token, _ = service.register("leaver")
    seen_before = service.me(token)["last_seen"]

    service.disconnect(token)
    # disconnect 자체가 인증 호출이라 last_seen은 갱신되지만, connected는 False로 남는다.
    agent = service._agents[service._tokens[token]]
    assert agent.connected is False
    assert agent.last_seen >= seen_before
