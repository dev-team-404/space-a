"""멤버십 · 공간 관리."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    x, x_tok = service.register_agent("x-bot", "x-bot", "sw-innov")
    y, y_tok = service.register_agent("y-bot", "y-bot", "ds")
    return service, x, x_tok, y, y_tok


def test_update_space_renames(ctx):
    service, *_ = ctx
    assert service.update_space("sw-innov", "S/W 혁신팀").name == "S/W 혁신팀"


def test_archive_space(ctx):
    service, *_ = ctx
    assert service.archive_space("sw-innov").status == "archived"


def test_update_unknown_space_is_not_found(ctx):
    service, *_ = ctx
    with pytest.raises(errors.NotFound):
        service.update_space("ghost", "x")


def test_add_member_grants_space(ctx):
    service, x, x_tok, y, y_tok = ctx
    service.add_member(x_tok, "sw-innov", y.id)  # x(멤버)가 y를 sw로 초대
    assert "sw-innov" in service.store.get_agent(y.id).spaces


def test_add_member_by_non_member_is_forbidden(ctx):
    service, x, x_tok, y, y_tok = ctx
    with pytest.raises(errors.Forbidden):
        service.add_member(x_tok, "ds", y.id)  # x는 ds 멤버가 아님


def test_add_unknown_agent_is_not_found(ctx):
    service, x, x_tok, *_ = ctx
    with pytest.raises(errors.NotFound):
        service.add_member(x_tok, "sw-innov", "agt_nope")


def test_remove_member(ctx):
    service, x, x_tok, y, y_tok = ctx
    service.add_member(x_tok, "sw-innov", y.id)
    service.remove_member(x_tok, "sw-innov", y.id)
    assert "sw-innov" not in service.store.get_agent(y.id).spaces


def test_list_members(ctx):
    service, x, x_tok, y, y_tok = ctx
    service.add_member(x_tok, "sw-innov", y.id)
    assert {a.id for a in service.list_members(x_tok, "sw-innov")} == {x.id, y.id}
