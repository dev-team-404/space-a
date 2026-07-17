"""지식 검색 — 권한 범위 안에서만."""


def _resolve_page(service, token, space_id, summary, visibility="org"):
    issue = service.open_issue(token, "seed", space_id)
    _, page = service.resolve_issue(token, issue.id, summary, visibility=visibility)
    return page


def test_search_returns_matching_visible_page(service):
    service.create_space("sw-innov", "S/W")
    _, token = service.register_agent("bot", "sw-innov")
    page = _resolve_page(service, token, "sw-innov", "DS 인증서 갱신")

    result = service.search_knowledge(token, "인증서")

    assert page.id in [p.id for p in result.pages]
    assert result.scanned >= 1


def test_search_includes_org_page_from_other_space(service):
    # B팀이 org 가시성 문서를 남기면 A팀 에이전트도 검색된다 (지식은 팀 경계를 넘는다)
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "ds")
    page = _resolve_page(service, b_token, "ds", "공용 인증서 가이드")  # visibility=org(기본)
    _, a_token = service.register_agent("a-bot", "sw-innov")

    result = service.search_knowledge(a_token, "인증서")

    assert page.id in [p.id for p in result.pages]


def test_search_excludes_space_only_page_from_other_space(service):
    service.create_space("sw-innov", "S/W")
    service.create_space("ds", "DS")
    _, b_token = service.register_agent("b-bot", "ds")
    page = _resolve_page(service, b_token, "ds", "비밀 인증서", visibility="space")
    _, a_token = service.register_agent("a-bot", "sw-innov")

    result = service.search_knowledge(a_token, "인증서")

    assert page.id not in [p.id for p in result.pages]
