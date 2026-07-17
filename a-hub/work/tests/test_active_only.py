"""리뷰 반영 — 비활성(archived/quarantined/superseded) 페이지 일관 처리 + supersede/move 가드."""

import pytest

from ahub.core import errors


@pytest.fixture
def ctx(service):
    service.create_space("sw-innov", "S/W")
    _, tok = service.register_agent("bot", "sw-innov")
    return service, tok


def test_cite_non_active_page_is_rejected(ctx):
    service, tok = ctx
    p = service.create_page(tok, "sw-innov", "가이드")
    service.archive_page(tok, p.id)
    issue = service.open_issue(tok, "문제", "sw-innov")
    with pytest.raises(errors.InvalidRequest):
        service.cite_knowledge(tok, issue.id, p.id)


def test_create_under_archived_parent_is_rejected(ctx):
    service, tok = ctx
    parent = service.create_page(tok, "sw-innov", "부모")
    service.archive_page(tok, parent.id)
    with pytest.raises(errors.InvalidRequest):
        service.create_page(tok, "sw-innov", "자식", parent_id=parent.id)


def test_move_under_archived_parent_is_rejected(ctx):
    service, tok = ctx
    parent = service.create_page(tok, "sw-innov", "부모")
    child = service.create_page(tok, "sw-innov", "자식")
    service.archive_page(tok, parent.id)
    with pytest.raises(errors.InvalidRequest):
        service.move_page(tok, child.id, parent.id)


def test_supersede_self_is_rejected(ctx):
    service, tok = ctx
    p = service.create_page(tok, "sw-innov", "문서")
    with pytest.raises(errors.InvalidRequest):
        service.supersede_page(tok, p.id, p.id)


def test_supersede_cross_space_is_rejected(ctx):
    service, tok = ctx
    service.create_space("ds", "DS")
    _, dtok = service.register_agent("d", "ds")
    other = service.create_page(dtok, "ds", "남의 문서")
    p = service.create_page(tok, "sw-innov", "문서")
    with pytest.raises(errors.InvalidRequest):
        service.supersede_page(tok, p.id, other.id)


def test_list_pages_excludes_non_active(ctx):
    service, tok = ctx
    a = service.create_page(tok, "sw-innov", "A")
    b = service.create_page(tok, "sw-innov", "B")
    service.archive_page(tok, b.id)
    ids = [p.id for p in service.list_pages(tok, "sw-innov")]
    assert a.id in ids
    assert b.id not in ids


def test_skill_candidates_exclude_non_active(ctx):
    service, tok = ctx
    ids = []
    for _ in range(3):
        i = service.open_issue(tok, "seed", "sw-innov")
        _, pg = service.resolve_issue(tok, i.id, "반복 해결")
        ids.append(pg.id)
    service.quarantine_page(tok, ids[0])  # 3건 중 1건 격리 → 2건 → min 3 미달
    assert service.get_skill_candidates(tok, min_occurrences=3) == []
