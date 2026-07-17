"""에이전트 온보딩 등록."""

import pytest

from ahub.core import errors


def test_register_agent_issues_id_token_and_joins_space(service):
    service.create_space("sw-innov", "S/W 혁신팀")

    agent, token = service.register_agent("build-bot", "build-bot", "sw-innov")

    assert agent.id == "build-bot"  # agent.id == user_id
    assert agent.spaces == ["sw-innov"]
    assert token


def test_two_agents_get_distinct_ids_and_tokens(service):
    service.create_space("sw-innov", "S/W 혁신팀")

    a1, t1 = service.register_agent("bot-1", "bot-1", "sw-innov")
    a2, t2 = service.register_agent("bot-2", "bot-2", "sw-innov")

    assert a1.id != a2.id
    assert t1 != t2


def test_register_into_missing_space_is_rejected(service):
    with pytest.raises(errors.NotFound):
        service.register_agent("build-bot", "build-bot", "ghost-team")
