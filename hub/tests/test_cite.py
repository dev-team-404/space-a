"""재사용 루프 — cite_knowledge가 ReuseEvent를 만든다."""

import pytest

from space_a.core import errors


def _doc_in(service, token, space_id, summary, visibility="org"):
    issue = service.open_issue(token, "seed", space_id)
    _, doc = service.resolve_issue(token, issue.id, summary, visibility=visibility)
    return doc


def test_cite_creates_reuse_event_and_links_issue(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "sw-innov")
    doc = _doc_in(service, token, "sw-innov", "DS 인증서 갱신")
    issue = service.open_issue(token, "같은 문제", "sw-innov")

    event, updated = service.cite_knowledge(token, issue.id, doc.id)

    assert event.id.startswith("reuse_")
    assert event.doc_id == doc.id
    assert updated.status == "knowledge_linked"


def test_cite_marks_cross_team_for_other_space_doc(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "ds")
    doc = _doc_in(service, b_token, "ds", "공용 인증서 가이드")  # org
    _, a_token = service.register_agent("a-bot", "sw-innov")
    issue = service.open_issue(a_token, "우리도 같은 문제", "sw-innov")

    event, _ = service.cite_knowledge(a_token, issue.id, doc.id)

    assert event.cross_team is True


def test_cite_unknown_doc_is_not_found(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "sw-innov")
    issue = service.open_issue(token, "t", "sw-innov")

    with pytest.raises(errors.NotFound):
        service.cite_knowledge(token, issue.id, "doc_nope")


def test_cite_invisible_doc_is_forbidden(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "ds")
    secret = _doc_in(service, b_token, "ds", "비밀", visibility="space")
    _, a_token = service.register_agent("a-bot", "sw-innov")
    issue_a = service.open_issue(a_token, "t", "sw-innov")

    with pytest.raises(errors.Forbidden):
        service.cite_knowledge(a_token, issue_a.id, secret.id)
