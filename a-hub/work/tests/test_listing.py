"""목록 · 조회 — 공간/이슈 브라우징."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, a = service.register_agent("a-bot", "a-bot", "sw-innov")
    _, b = service.register_agent("b-bot", "b-bot", "ds")
    return service, a, b


def test_list_spaces_returns_all(ctx):
    service, *_ = ctx
    assert {s.id for s in service.list_spaces()} == {"sw-innov", "ds"}


def test_get_space_returns_it(ctx):
    service, *_ = ctx
    assert service.get_space("sw-innov").name == "S/W"


def test_get_unknown_space_is_not_found(ctx):
    service, *_ = ctx
    with pytest.raises(errors.NotFound):
        service.get_space("ghost")


def test_list_issues_in_my_spaces(ctx):
    service, a, b = ctx
    service.open_issue(a, "i1", "sw-innov")
    service.open_issue(a, "i2", "sw-innov")
    assert len(service.list_issues(a)) == 2


def test_list_issues_excludes_other_space(ctx):
    service, a, b = ctx
    service.open_issue(b, "b-issue", "ds")
    assert service.list_issues(a) == []


def test_list_issues_filter_by_status(ctx):
    service, a, b = ctx
    i1 = service.open_issue(a, "open one", "sw-innov")
    i2 = service.open_issue(a, "to resolve", "sw-innov")
    service.resolve_issue(a, i2.id, "done", publish_knowledge=False)
    assert [i.id for i in service.list_issues(a, status="open")] == [i1.id]


def test_list_issues_mine(ctx):
    service, a, b = ctx
    _, a2 = service.register_agent("a2-bot", "a2-bot", "sw-innov")
    ia = service.open_issue(a, "a's", "sw-innov")
    service.open_issue(a2, "a2's", "sw-innov")
    assert [i.id for i in service.list_issues(a, mine=True)] == [ia.id]


def test_get_issue_in_my_space(ctx):
    service, a, b = ctx
    i = service.open_issue(a, "t", "sw-innov")
    assert service.get_issue(a, i.id).id == i.id


def test_get_issue_foreign_space_is_forbidden(ctx):
    service, a, b = ctx
    ib = service.open_issue(b, "t", "ds")
    with pytest.raises(errors.Forbidden):
        service.get_issue(a, ib.id)
