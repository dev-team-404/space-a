"""에이전트 guidance — space 목적/지침 + 가이드 페이지."""


def test_create_space_stores_purpose_and_guidelines(service):
    s = service.create_space("sw-innov", "S/W", purpose="AI 협업 공간", guidelines="문제만 기록")
    assert s.purpose == "AI 협업 공간"
    assert s.guidelines == "문제만 기록"


def test_guidelines_seed_a_guide_page(service):
    service.create_space("sw-innov", "S/W", guidelines="여기 작성 규칙")
    guide = service.get_guide("sw-innov")
    assert guide is not None
    assert guide.body == "여기 작성 규칙"
    assert guide.source == "authored"


def test_no_guidelines_means_no_guide(service):
    service.create_space("sw-innov", "S/W")
    assert service.get_guide("sw-innov") is None
