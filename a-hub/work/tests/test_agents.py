"""에이전트 lifecycle — 목록·조회·폐기·토큰 회전."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    x, x_tok = service.register_agent("x", "sw-innov")
    y, y_tok = service.register_agent("y", "sw-innov")
    z, z_tok = service.register_agent("z", "ds")
    return service, (x, x_tok), (y, y_tok), (z, z_tok)


def test_list_agents_sharing_a_space(ctx):
    service, (x, x_tok), (y, _), (z, _) = ctx
    ids = {a.id for a in service.list_agents(x_tok)}
    assert x.id in ids and y.id in ids
    assert z.id not in ids  # 다른 space


def test_get_agent(ctx):
    service, (x, x_tok), (y, _), _ = ctx
    assert service.get_agent(x_tok, y.id).name == "y"


def test_get_unknown_agent_is_not_found(ctx):
    service, (x, x_tok), *_ = ctx
    with pytest.raises(errors.NotFound):
        service.get_agent(x_tok, "agt_nope")


def test_revoke_self_invalidates_token(ctx):
    service, (x, x_tok), *_ = ctx
    service.revoke_agent(x_tok, x.id)
    with pytest.raises(errors.Unauthorized):
        service.list_agents(x_tok)


def test_revoke_other_is_forbidden(ctx):
    service, (x, x_tok), (y, _), _ = ctx
    with pytest.raises(errors.Forbidden):
        service.revoke_agent(x_tok, y.id)


def test_rotate_token_issues_new_and_invalidates_old(ctx):
    service, (x, x_tok), *_ = ctx
    new_tok = service.rotate_token(x_tok, x.id)
    assert new_tok != x_tok
    with pytest.raises(errors.Unauthorized):
        service.list_agents(x_tok)
    assert any(a.id == x.id for a in service.list_agents(new_tok))
