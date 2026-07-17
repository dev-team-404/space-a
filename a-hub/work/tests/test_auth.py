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
