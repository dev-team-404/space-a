"""권한 모델: 소속은 토큰에서 유도. 남의 Space는 지정해도 안 열린다."""

import pytest

from ahub.core import errors


def test_open_issue_in_foreign_space_is_forbidden(service):
    service.create_space("sw-innov", "S/W 혁신팀")
    service.create_space("ds-platform", "데이터플랫폼팀")
    _, token = service.register_agent("build-bot", "build-bot", "sw-innov")

    with pytest.raises(errors.Forbidden):
        service.open_issue(token, "남의 방", "ds-platform")


def test_open_issue_with_unknown_token_is_unauthorized(service):
    service.create_space("sw-innov", "S/W 혁신팀")

    with pytest.raises(errors.Unauthorized):
        service.open_issue("bad-token", "무단", "sw-innov")
