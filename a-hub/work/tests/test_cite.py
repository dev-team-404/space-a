"""재사용 루프 — cite_knowledge가 ReuseEvent를 만든다."""

import pytest

from ahub.core import errors


def _page_in(service, token, space_id, summary, visibility="org"):
    issue = service.open_issue(token, "seed", space_id)
    _, page = service.resolve_issue(token, issue.id, summary, visibility=visibility)
    return page


def test_cite_creates_reuse_event_and_links_issue(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "bot", "sw-innov")
    page = _page_in(service, token, "sw-innov", "DS 인증서 갱신")
    issue = service.open_issue(token, "같은 문제", "sw-innov")

    event, updated = service.cite_knowledge(token, issue.id, page.id)

    assert event.id.startswith("reuse_")
    assert event.page_id == page.id
    assert updated.status == "knowledge_linked"


def test_cite_marks_cross_team_for_other_space_page(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "b-bot", "ds")
    page = _page_in(service, b_token, "ds", "공용 인증서 가이드")  # org
    _, a_token = service.register_agent("a-bot", "a-bot", "sw-innov")
    issue = service.open_issue(a_token, "우리도 같은 문제", "sw-innov")

    event, _ = service.cite_knowledge(a_token, issue.id, page.id)

    assert event.cross_team is True


def test_cite_unknown_page_is_not_found(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "bot", "sw-innov")
    issue = service.open_issue(token, "t", "sw-innov")

    with pytest.raises(errors.NotFound):
        service.cite_knowledge(token, issue.id, "page_nope")


def test_reuse_events_are_readable_after_citing(service):
    """북극성 지표 조회 — 인용이 쓰기 전용 싱크로 사라지지 않아야 한다."""
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "bot", "sw-innov")
    page = _page_in(service, token, "sw-innov", "DS 인증서 갱신")
    issue = service.open_issue(token, "같은 문제", "sw-innov")
    event, _ = service.cite_knowledge(token, issue.id, page.id)

    events = service.list_reuse_events(token)

    assert [e.id for e in events] == [event.id]
    assert events[0].page_id == page.id
    assert events[0].created_at, "a-lens 최신순 정렬을 위해 시각이 필요하다"


def test_reuse_events_are_scoped_to_my_spaces(service):
    """남의 space에서 일어난 인용은 보이지 않는다 (list_issues와 같은 규칙)."""
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "b-bot", "ds")
    b_page = _page_in(service, b_token, "ds", "공용 가이드")
    b_issue = service.open_issue(b_token, "ds 내부 문제", "ds")
    service.cite_knowledge(b_token, b_issue.id, b_page.id)

    _, a_token = service.register_agent("a-bot", "a-bot", "sw-innov")

    assert service.list_reuse_events(a_token) == []

    # a가 같은 지식을 인용하면 cross_team 이벤트가 a에게 보인다 — README 시나리오 그대로
    a_issue = service.open_issue(a_token, "우리도 같은 문제", "sw-innov")
    service.cite_knowledge(a_token, a_issue.id, b_page.id)
    mine = service.list_reuse_events(a_token)
    assert len(mine) == 1
    assert mine[0].cross_team is True


def test_author_sees_reuse_of_own_knowledge_across_teams(service):
    """내 지식이 다른 팀에서 재사용된 기록은 작성자에게 보인다 (인정 루프의 전제).

    인용은 인용한 팀의 space에서 일어나므로, space 구성원 규칙만으로는
    **원 작성자만 자기 지식의 재사용을 영영 못 본다**. 그러면 개인의 발견이
    조직에서 낸 가치가 개인에게 돌아오지 않는다.
    """
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    # 작성자 b — ds팀에만 속한다
    _, b_token = service.register_agent("b-bot", "b-bot", "ds")
    b_page = _page_in(service, b_token, "ds", "공용 인증서 가이드")
    # 인용자 a — sw-innov팀. b의 지식을 자기 팀 이슈에 인용한다
    _, a_token = service.register_agent("a-bot", "a-bot", "sw-innov")
    a_issue = service.open_issue(a_token, "우리도 같은 문제", "sw-innov")
    service.cite_knowledge(a_token, a_issue.id, b_page.id)

    seen = service.list_reuse_events(b_token)

    assert len(seen) == 1, "작성자는 sw-innov 소속이 아니어도 자기 지식의 재사용을 본다"
    assert seen[0].page_id == b_page.id
    assert seen[0].cross_team is True


def test_author_visibility_does_not_leak_others_reuse(service):
    """작성자 규칙이 남의 재사용까지 보여주면 안 된다 — 내 페이지 건만."""
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "b-bot", "ds")
    _, c_token = service.register_agent("c-bot", "c-bot", "ds")
    c_page = _page_in(service, c_token, "ds", "c가 쓴 가이드")
    _, a_token = service.register_agent("a-bot", "a-bot", "sw-innov")
    a_issue = service.open_issue(a_token, "문제", "sw-innov")
    service.cite_knowledge(a_token, a_issue.id, c_page.id)

    # b는 작성자도 아니고 sw-innov 소속도 아니다 → 안 보인다
    assert service.list_reuse_events(b_token) == []
    # c는 작성자다 → 보인다
    assert len(service.list_reuse_events(c_token)) == 1


def test_cite_invisible_page_is_forbidden(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "b-bot", "ds")
    secret = _page_in(service, b_token, "ds", "비밀", visibility="space")
    _, a_token = service.register_agent("a-bot", "a-bot", "sw-innov")
    issue_a = service.open_issue(a_token, "t", "sw-innov")

    with pytest.raises(errors.Forbidden):
        service.cite_knowledge(a_token, issue_a.id, secret.id)
