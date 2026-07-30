"""프레즌스 판정 — Life의 `last_seen` 우선, 구버전 Life는 허브 write로 강등.

배경(2026-07-30): 원래 온라인 판정은 "마지막 허브 write(page·issue)가 판정 창 안인가"였다.
그런데 a-mate의 write는 사람당 하루 1~3건이고 전부 자정 직후에 몰려서, 창이 1시간이면
하루 23시간이 offline이고 정작 켜지는 1시간은 새벽이었다. Life 쪽은 a-mate가 스캔마다
인증 호출을 하므로 분 단위로 신선하다.
"""

from datetime import datetime, timedelta, timezone

from alens.collector import _presence_status

WINDOW = 3600.0


def _cutoff(now: datetime) -> float:
    return now.timestamp() - WINDOW


def _person(last_seen: datetime | None, *, connected: bool = True) -> dict:
    return {
        "agent_id": "ragt_x",
        "connected": connected,
        "last_seen": last_seen.isoformat() if last_seen else None,
    }


def _hub_write(at: datetime) -> dict:
    return {"at": at, "kind": "knowledge", "title": "t", "ref_id": "page_1"}


def test_fresh_life_last_seen_is_working():
    now = datetime.now(timezone.utc)
    person = _person(now - timedelta(minutes=2))
    assert _presence_status(person, None, _cutoff(now)) == "working"


def test_stale_life_last_seen_is_idle():
    now = datetime.now(timezone.utc)
    person = _person(now - timedelta(hours=3))
    assert _presence_status(person, None, _cutoff(now)) == "idle"


def test_life_wins_over_stale_hub_write():
    """핵심 회귀: 허브 write가 오래됐어도 접속 중이면 online이다.

    이게 고치려던 증상이다 — 낮에 8시간 작업해도 페이지를 안 쓰면 offline으로 보였다."""
    now = datetime.now(timezone.utc)
    person = _person(now - timedelta(minutes=1))
    old_write = _hub_write(now - timedelta(hours=12))
    assert _presence_status(person, old_write, _cutoff(now)) == "working"


def test_life_wins_over_fresh_hub_write():
    """반대 방향도 Life가 정답 — 자정에 회고를 발행했지만 지금 앱이 꺼져 있으면 offline이다."""
    now = datetime.now(timezone.utc)
    person = _person(now - timedelta(hours=5))
    fresh_write = _hub_write(now - timedelta(minutes=10))
    assert _presence_status(person, fresh_write, _cutoff(now)) == "idle"


def test_explicit_disconnect_is_idle_even_if_recently_seen():
    """수동 연결 끊기는 사용자의 명시적 의사 — last_seen이 신선해도 존중한다."""
    now = datetime.now(timezone.utc)
    person = _person(now, connected=False)
    assert _presence_status(person, None, _cutoff(now)) == "idle"


def test_never_observed_life_person_is_idle():
    """connected=True이지만 관측 이력이 없는 계정(등록만 해두고 떠난 사람) — online 아님."""
    now = datetime.now(timezone.utc)
    assert _presence_status(_person(None), None, _cutoff(now)) == "idle"


def test_old_life_server_without_last_seen_falls_back_to_hub_write():
    """`last_seen` 키가 없으면 구버전 Life다 — 조용히 종전 방식으로 강등한다."""
    now = datetime.now(timezone.utc)
    legacy = {"agent_id": "ragt_x", "name": "옛서버"}
    assert _presence_status(legacy, _hub_write(now - timedelta(minutes=5)), _cutoff(now)) == "working"
    assert _presence_status(legacy, _hub_write(now - timedelta(hours=9)), _cutoff(now)) == "idle"


def test_no_life_match_falls_back_to_hub_write():
    """Life에 못 이은 허브 계정 — 재료가 write뿐이므로 종전 판정을 그대로 쓴다."""
    now = datetime.now(timezone.utc)
    assert _presence_status(None, _hub_write(now - timedelta(minutes=5)), _cutoff(now)) == "working"
    assert _presence_status(None, None, _cutoff(now)) == "idle"
