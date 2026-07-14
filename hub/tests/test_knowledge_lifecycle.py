"""지식 등록: open_issue -> resolve_issue -> knowledge doc."""

import pytest


@pytest.fixture
def registered(service):
    service.create_space("sw-innov", "S/W 혁신팀")
    _, token = service.register_agent("build-bot", "sw-innov")
    return service, token


def test_open_issue_returns_open_status(registered):
    service, token = registered

    issue = service.open_issue(token, "DS 인증서 오류", "sw-innov")

    assert issue.id.startswith("iss_")
    assert issue.status == "open"


def test_resolve_issue_publishes_knowledge_doc(registered):
    service, token = registered
    issue = service.open_issue(token, "DS 인증서 오류", "sw-innov")

    resolved, doc = service.resolve_issue(
        token, issue.id, "DS 인증서를 갱신한다", ["cert 재발급", "재기동"]
    )

    assert resolved.status == "resolved"
    assert doc is not None
    assert doc.id.startswith("doc_")
    assert doc.summary == "DS 인증서를 갱신한다"
    assert doc.space_id == "sw-innov"


def test_resolve_without_publish_creates_no_doc(registered):
    service, token = registered
    issue = service.open_issue(token, "DS 인증서 오류", "sw-innov")

    resolved, doc = service.resolve_issue(
        token, issue.id, "임시 해결", publish_knowledge=False
    )

    assert resolved.status == "resolved"
    assert doc is None
