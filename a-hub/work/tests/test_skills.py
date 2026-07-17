"""Skill 후보 — 반복 해결 패턴 탐지."""


def _resolve_n(service, token, space_id, summary, n):
    for _ in range(n):
        issue = service.open_issue(token, "seed", space_id)
        service.resolve_issue(token, issue.id, summary)


def test_repeated_solution_becomes_candidate(service):
    service.create_space("sw-innov", "S/W")
    _, tok = service.register_agent("bot", "bot", "sw-innov")
    _resolve_n(service, tok, "sw-innov", "DS 인증서 갱신", 3)

    cands = service.get_skill_candidates(tok, min_occurrences=3)

    assert len(cands) == 1
    assert cands[0].pattern == "DS 인증서 갱신"
    assert cands[0].occurrences == 3
    assert len(cands[0].page_ids) == 3


def test_below_threshold_is_not_candidate(service):
    service.create_space("sw-innov", "S/W")
    _, tok = service.register_agent("bot", "bot", "sw-innov")
    _resolve_n(service, tok, "sw-innov", "가끔 나는 문제", 2)

    assert service.get_skill_candidates(tok, min_occurrences=3) == []
