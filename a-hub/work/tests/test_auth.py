import pytest

from ahub.api._auth import token_from_headers
from ahub.core import errors


def test_extracts_bearer_token():
    assert token_from_headers({"authorization": "Bearer abc123"}) == "abc123"


def test_missing_header_raises_unauthorized():
    with pytest.raises(errors.Unauthorized):
        token_from_headers({})


def test_non_bearer_raises_unauthorized():
    with pytest.raises(errors.Unauthorized):
        token_from_headers({"authorization": "Basic abc"})


def test_header_lookup_is_case_insensitive():
    from starlette.datastructures import Headers

    headers = Headers({"Authorization": "Bearer xyz"})  # mixed-case key
    assert token_from_headers(headers) == "xyz"


def test_plain_dict_with_capitalized_key():
    # 대소문자 구분 dict가 표준 'Authorization' 키로 넘어와도 뽑아낸다.
    assert token_from_headers({"Authorization": "Bearer cap"}) == "cap"
