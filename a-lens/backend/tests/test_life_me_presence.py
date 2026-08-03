"""`/life/people`은 호출자 자신을 빼고 준다 — 그 빈자리를 `/life/me`로 채울 때
프레즌스(`connected`·`last_seen`)까지 함께 실어야 한다.

배경(2026-08-03): 이 두 필드가 빠져 있어서, **a-lens가 Life 인증에 쓰는 계정만** 프레즌스
판정이 허브 write 시각으로 강등됐다. a-mate를 켜고 쓰는 중에도(Life last_seen 1분 전) 3일 전
write가 기준이 되어 혼자 오프라인으로 보였다. 남들은 `/life/people`로 오므로 멀쩡했다.
"""

import httpx
import pytest

from alens import life_client, settings

ME = {
    "agent_id": "ragt_me",
    "name": "kimmy",
    "my_life_id": "life_1",
    "identity": {},
    "connected": True,
    "last_seen": "2026-08-03T01:03:15+00:00",
}
OTHER = {
    "agent_id": "ragt_other",
    "name": "돌쇠",
    "connected": True,
    "last_seen": "2026-08-02T08:06:44+00:00",
}


class _FakeClient:
    """`/life/people`과 `/life/me`만 답하는 최소 스텁."""

    def __init__(self, me: dict, **_kw):
        self._me = me

    def __enter__(self):
        return self

    def __exit__(self, *_a):
        return False

    def get(self, url: str):
        body = {"people": [OTHER]} if url.endswith("/life/people") else self._me
        return httpx.Response(200, json=body, request=httpx.Request("GET", url))


@pytest.fixture
def life_on(monkeypatch):
    """Life 연동을 켜고 사람 목록 캐시를 비운 상태로 시작한다."""
    monkeypatch.setattr(settings, "get", lambda: {"life_url": "http://life", "life_token": "t", "life_api_key": ""})
    life_client.clear_cache()
    yield
    life_client.clear_cache()


def _me_row(me: dict) -> dict:
    return next(p for p in life_client.people(force=True) if p.get("is_me"))


def test_me_row_carries_presence(life_on, monkeypatch):
    monkeypatch.setattr(life_client.httpx, "Client", lambda **kw: _FakeClient(ME, **kw))
    row = _me_row(ME)
    assert row["connected"] is True
    assert row["last_seen"] == ME["last_seen"]


def test_me_row_omits_presence_on_old_life_server(life_on, monkeypatch):
    """구버전 Life 서버는 이 필드를 안 준다 — 그때는 **키를 만들지 않아야** 한다.

    `_presence_status`가 `last_seen` 키의 부재로 구버전을 알아보고 허브 write 방식으로
    강등하기 때문. None으로 채우면 그 판정을 가로채 구버전에서 전원이 idle이 된다."""
    old = {k: v for k, v in ME.items() if k not in ("connected", "last_seen")}
    monkeypatch.setattr(life_client.httpx, "Client", lambda **kw: _FakeClient(old, **kw))
    row = _me_row(old)
    assert "last_seen" not in row
    assert "connected" not in row


def test_others_still_come_through_untouched(life_on, monkeypatch):
    monkeypatch.setattr(life_client.httpx, "Client", lambda **kw: _FakeClient(ME, **kw))
    rows = life_client.people(force=True)
    assert [r["agent_id"] for r in rows] == ["ragt_other", "ragt_me"]
    assert rows[0]["last_seen"] == OTHER["last_seen"]
