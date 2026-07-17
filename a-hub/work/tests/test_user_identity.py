"""사용자 기반 정체성 — register가 사용자 지정 user_id로 계정을 안정 식별한다.

같은 user_id로 다시 register하면 새 계정이 생기지 않고 재사용(upsert)되며,
매번 새 토큰을 발급하되 기존 토큰도 유효하다. 한 사람이 봇을 여러 개 돌려도
같은 user_id면 created_by가 하나로 뭉친다.
"""

import pytest

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.rest_server import create_app
from ahub.core import errors
from ahub.core.services import SpaceAService

from fastapi.testclient import TestClient


def _space(service: SpaceAService):
    service.create_space("s1", "Space One", purpose="p")


# --- 서비스 레벨 (memory·sqlite 양쪽) ---


def test_agent_id_equals_user_id(service: SpaceAService):
    _space(service)
    agent, _ = service.register_agent("salt", "salt의 봇", "s1")
    assert agent.id == "salt"


def test_reregister_reuses_account_and_updates_name(service: SpaceAService):
    _space(service)
    a1, _ = service.register_agent("salt", "old name", "s1")
    a2, _ = service.register_agent("salt", "new name", "s1")
    assert a2.id == a1.id == "salt"
    assert a2.name == "new name"
    # 새 계정이 생기지 않고 하나만 존재
    assert len(service.store.all_agents()) == 1


def test_reregister_issues_new_token_but_old_still_valid(service: SpaceAService):
    _space(service)
    _, t1 = service.register_agent("salt", "n", "s1")
    _, t2 = service.register_agent("salt", "n", "s1")
    assert t1 != t2
    # 두 토큰 모두 같은 계정으로 인증된다
    assert service.store.agent_for_token(t1).id == "salt"
    assert service.store.agent_for_token(t2).id == "salt"


def test_reregister_other_space_merges_membership(service: SpaceAService):
    service.create_space("s1", "S1")
    service.create_space("s2", "S2")
    service.register_agent("salt", "n", "s1")
    agent, _ = service.register_agent("salt", "n", "s2")
    assert set(agent.spaces) == {"s1", "s2"}


def test_same_user_two_agents_share_author(service: SpaceAService):
    """한 사람(user_id)이 봇 두 번 등록 → 각 토큰으로 쓴 page의 created_by가 동일."""
    _space(service)
    _, t1 = service.register_agent("salt", "botA", "s1")
    _, t2 = service.register_agent("salt", "botB", "s1")
    p1 = service.create_page(t1, "s1", "A")
    p2 = service.create_page(t2, "s1", "B")
    assert p1.created_by == p2.created_by == "salt"


@pytest.mark.parametrize(
    "bad",
    [
        "",  # 빈 값
        "   ",  # 공백
        "a/b",  # 경로 구분자 → URL 라우팅 파손
        "a b",  # 공백 포함
        "a?b",  # 쿼리 구분자
        "a#b",  # 프래그먼트
        "../x",  # 경로 탐색류
        "a" * 101,  # 길이 초과
    ],
)
def test_invalid_user_id_rejected(service: SpaceAService, bad):
    """user_id는 agent.id·URL 경로 파라미터로 쓰이므로 안전한 문자·길이만 허용한다."""
    _space(service)
    with pytest.raises(errors.InvalidRequest):
        service.register_agent(bad, "n", "s1")


@pytest.mark.parametrize("ok", ["salt", "salt.jeong", "bot-1", "team_a", "A1.b-c_2"])
def test_valid_user_id_accepted(service: SpaceAService, ok):
    _space(service)
    agent, _ = service.register_agent(ok, "n", "s1")
    assert agent.id == ok


# --- REST 레벨 ---


def test_rest_register_returns_user_id_as_agent_id():
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "S", purpose="p")
    c = TestClient(create_app(svc, mount_mcp=False))
    r = c.post(
        "/agents/register",
        json={"user_id": "salt", "name": "봇", "space_id": "s1"},
    )
    assert r.status_code == 201
    assert r.json()["agent_id"] == "salt"
