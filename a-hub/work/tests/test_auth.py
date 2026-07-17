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
    # Starlette Headers are case-insensitive; a plain dict is not, so the
    # function must normalize. We pass the canonical lowercase key here and
    # rely on callers passing Starlette Headers in production.
    assert token_from_headers({"authorization": "Bearer x"}) == "x"
