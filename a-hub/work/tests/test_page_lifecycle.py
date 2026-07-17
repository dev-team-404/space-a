"""페이지 lifecycle — 편집·가시성·archive·supersede·quarantine·flag + 검색 제외."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    _, tok = service.register_agent("bot", "sw-innov")
    p = service.create_page(tok, "sw-innov", "가이드", body="원본")
    return service, tok, p


def _search_ids(service, tok, q="가이드"):
    return [x.id for x in service.search_knowledge(tok, q).pages]


def test_edit_page(ctx):
    service, tok, p = ctx
    edited = service.edit_page(tok, p.id, title="새 제목", body="새 내용")
    assert edited.title == "새 제목"
    assert edited.body == "새 내용"


def test_set_visibility(ctx):
    service, tok, p = ctx
    assert service.set_visibility(tok, p.id, "space").visibility == "space"


def test_archive_page_excluded_from_search(ctx):
    service, tok, p = ctx
    assert p.id in _search_ids(service, tok)
    service.archive_page(tok, p.id)
    assert p.id not in _search_ids(service, tok)


def test_supersede_marks_and_excludes(ctx):
    service, tok, p = ctx
    new = service.create_page(tok, "sw-innov", "가이드 v2")
    superseded = service.supersede_page(tok, p.id, new.id)
    assert superseded.status == "superseded"
    assert superseded.superseded_by == new.id
    assert p.id not in _search_ids(service, tok)


def test_quarantine_excludes_from_search(ctx):
    service, tok, p = ctx
    service.quarantine_page(tok, p.id)
    assert p.id not in _search_ids(service, tok)


def test_flag_increments(ctx):
    service, tok, p = ctx
    assert service.flag_page(tok, p.id).flags == 1
    assert service.flag_page(tok, p.id).flags == 2


def test_edit_foreign_space_is_forbidden(ctx):
    service, tok, p = ctx
    service.create_space("ds", "DS")
    _, other = service.register_agent("o", "ds")
    with pytest.raises(errors.Forbidden):
        service.edit_page(other, p.id, title="침입")
