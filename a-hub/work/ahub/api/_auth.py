"""공용 인증 헬퍼 — REST와 MCP가 같은 Bearer 규칙을 쓴다 (계약 §3.1).

신원은 전송 계층(Authorization 헤더)에서만 온다. 순환 import를 피해
rest_server가 아니라 여기에 둔다.
"""

from __future__ import annotations

from typing import Mapping

from ..core import errors

_PREFIX = "Bearer "


def _bearer(authorization: str | None) -> str:
    """`Authorization: Bearer <token>` 헤더 값에서 토큰을 뽑는다."""
    if not authorization or not authorization.startswith(_PREFIX):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len(_PREFIX) :]


def token_from_headers(headers: Mapping[str, str]) -> str:
    """헤더 매핑에서 Bearer 토큰을 뽑는다 (Starlette Headers 또는 dict).

    Starlette Headers는 대소문자 무시 조회를 제공한다. 평범한 dict를 넘길 때는
    소문자 'authorization' 키를 쓴다.
    """
    return _bearer(headers.get("authorization"))
