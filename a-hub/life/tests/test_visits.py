"""인바운드 방문 추적 (P4) — enter 자동 기록·세션화·prune·조회.
설계: docs/design/a-mate/specs/2026-07-29-visit-infra-notices-design.md §3"""

import pytest

from life_server import errors
from life_server.life import VISIT_SESSION_WINDOW_SECS, VISITS_MAX_PER_LIFE, LifeService
from life_server.store import SqliteStore


@pytest.fixture
def life() -> LifeService:
    return LifeService()


def _register_two(life):
    _, token_a, life_a = life.register("A")
    _, token_b, life_b = life.register("B")
    return token_a, life_a, token_b, life_b


def _age_latest_visit(life, iso: str) -> None:
    """세션 창을 지난 방문으로 노화 — 시계 주입 없이 저장 행을 직접 조작(테스트 전용)."""
    life._visits[-1]["first_at"] = iso
    life._visits[-1]["last_at"] = iso


def test_enter_records_inbound_visit(life):
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    rows = life.visits(token_a)
    assert len(rows) == 1
    assert rows[0]["visitor_name"] == "B"
    assert rows[0]["first_at"] == rows[0]["last_at"]
    assert rows[0]["present"] is True  # 아직 방에 있음


def test_present_false_after_visitor_leaves(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.enter(token_b, life_b.id, cell=None)  # 자기 방으로 귀가
    assert life.visits(token_a)[0]["present"] is False


def test_own_room_entry_not_recorded(life):
    _, token_a, life_a = life.register("A")
    life.enter(token_a, life_a.id, cell=None)
    assert life.visits(token_a) == []


def test_visits_scope_is_my_room_only(life):
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    assert life.visits(token_b) == []  # B의 방엔 인바운드 방문 없음


def test_visits_requires_valid_token(life):
    with pytest.raises(errors.Unauthorized):
        life.visits("no-such-token")


def test_revisit_within_window_extends_session(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 30분 이내 재방문
    rows = life.visits(token_a)
    assert len(rows) == 1  # 새 행이 아니라 세션 연장
    assert rows[0]["last_at"] >= rows[0]["first_at"]


def test_revisit_after_window_creates_new_row(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    _age_latest_visit(life, "2020-01-01T00:00:00+00:00")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)
    assert len(life.visits(token_a)) == 2


def test_session_extension_resnapshots_visitor_name(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    life.rename(token_b, "B2")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 세션 연장 → 이름 재스냅샷
    assert life.visits(token_a)[0]["visitor_name"] == "B2"


def test_visits_since_filters_and_sorts_desc(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    _age_latest_visit(life, "2020-01-01T00:00:00+00:00")
    life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 두 번째 세션(현재 시각)
    rows = life.visits(token_a)
    assert [r["last_at"] for r in rows] == sorted((r["last_at"] for r in rows), reverse=True)
    fresh = life.visits(token_a, since="2021-01-01T00:00:00+00:00")
    assert len(fresh) == 1


def test_visits_limit_clamped(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    for i in range(3):
        life.enter(token_b, life_a.id, cell=None)
        _age_latest_visit(life, f"2020-01-01T00:0{i}:00+00:00")
        life.enter(token_b, life_b.id, cell=None)
    assert len(life.visits(token_a, limit=2)) == 2
    assert len(life.visits(token_a, limit=0)) == 1  # 최소 1로 클램프


def test_prune_keeps_latest_per_life(life):
    token_a, life_a, token_b, life_b = _register_two(life)
    for i in range(VISITS_MAX_PER_LIFE + 5):
        life.enter(token_b, life_a.id, cell=None)
        _age_latest_visit(life, f"2020-01-01T{i // 60:02d}:{i % 60:02d}:00+00:00")
        life.enter(token_b, life_b.id, cell=None)
    life.enter(token_b, life_a.id, cell=None)  # 최신(현재 시각) 방문
    rows = life.visits(token_a, limit=100)
    assert len(rows) == VISITS_MAX_PER_LIFE
    assert rows[0]["last_at"] > "2021-01-01"  # 최신은 살아 있음
    assert all(r["last_at"] != "2020-01-01T00:00:00+00:00" for r in rows)  # 가장 오래된 것 prune


def test_sqlite_persistence_survives_restart(tmp_path):
    db = str(tmp_path / "life.db")
    life = LifeService(store=SqliteStore(db))
    token_a, life_a, token_b, _ = _register_two(life)
    life.enter(token_b, life_a.id, cell=None)
    revived = LifeService(store=SqliteStore(db))  # 재시작 흉내
    assert len(revived._visits) == 1
    assert revived._visits[0]["visitor_name"] == "B"
    assert set(revived._visits[0]) == {
        "visit_id", "life_id", "visitor_agent_id", "visitor_name", "first_at", "last_at",
    }
