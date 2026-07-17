"""저작 페이지 · 트리 — 에이전트가 공간 안에 문서를 트리로 쓴다."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "sw-innov")
    return service, token


def test_create_page_at_root(ctx):
    service, token = ctx
    page = service.create_page(token, "sw-innov", "온보딩 가이드", body="이렇게 시작")
    assert page.id.startswith("page_")
    assert page.source == "authored"
    assert page.parent_id is None
    assert page.space_id == "sw-innov"


def test_create_child_page(ctx):
    service, token = ctx
    parent = service.create_page(token, "sw-innov", "가이드")
    child = service.create_page(token, "sw-innov", "세부", parent_id=parent.id)
    assert child.parent_id == parent.id


def test_create_page_in_foreign_space_is_forbidden(ctx):
    service, token = ctx
    service.create_space("ds", "DS")
    with pytest.raises(errors.Forbidden):
        service.create_page(token, "ds", "남의 방 글")


def test_create_with_unknown_parent_is_not_found(ctx):
    service, token = ctx
    with pytest.raises(errors.NotFound):
        service.create_page(token, "sw-innov", "고아", parent_id="page_nope")


def test_move_page_reparents(ctx):
    service, token = ctx
    p1 = service.create_page(token, "sw-innov", "P1")
    p2 = service.create_page(token, "sw-innov", "P2")
    moved = service.move_page(token, p2.id, p1.id)
    assert moved.parent_id == p1.id


def test_move_page_cycle_is_rejected(ctx):
    service, token = ctx
    p1 = service.create_page(token, "sw-innov", "P1")
    p2 = service.create_page(token, "sw-innov", "P2", parent_id=p1.id)
    with pytest.raises(errors.InvalidRequest):
        service.move_page(token, p1.id, p2.id)  # 자기 자손 아래로 → 사이클


def test_list_pages_returns_space_pages(ctx):
    service, token = ctx
    service.create_page(token, "sw-innov", "A")
    service.create_page(token, "sw-innov", "B")
    assert len(service.list_pages(token, "sw-innov")) == 2


def test_get_page_returns_visible(ctx):
    service, token = ctx
    p = service.create_page(token, "sw-innov", "가이드")
    assert service.get_page(token, p.id).id == p.id
