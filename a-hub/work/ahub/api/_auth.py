"""공용 인증 헬퍼 — REST와 MCP가 같은 Bearer 규칙을 쓴다 (계약 §3.1).

신원은 전송 계층(Authorization 헤더)에서만 온다. 순환 import를 피해
rest_server가 아니라 여기에 둔다.
"""

from __future__ import annotations

import os
from typing import Mapping

from ..core import errors

_PREFIX = "Bearer "

# x-api-key 관문을 면제할 경로 (로드밸런서·모니터링용).
API_KEY_EXEMPT_PATHS = {"/healthz", "/readyz"}


def api_key_ok(path: str, provided: str | None) -> bool:
    """고정 공유키 x-api-key 검증.

    SPACE_A_API_KEY 환경변수가 없으면(미설정) 검사를 건너뛴다 — 로컬·테스트 편의.
    설정된 배포에서는 면제 경로를 제외한 모든 요청에서 키 일치를 요구한다.
    """
    expected = os.environ.get("SPACE_A_API_KEY")
    if not expected:
        return True
    if path in API_KEY_EXEMPT_PATHS:
        return True
    return provided == expected


def _bearer(authorization: str | None) -> str:
    """`Authorization: Bearer <token>` 헤더 값에서 토큰을 뽑는다."""
    if not authorization or not authorization.startswith(_PREFIX):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len(_PREFIX) :]


def token_from_headers(headers: Mapping[str, str]) -> str:
    """헤더 매핑에서 Bearer 토큰을 뽑는다 (Starlette Headers 또는 dict).

    프로덕션 경로는 Starlette Headers(대소문자 무시)라 소문자 조회로 충분하다.
    다만 시그니처가 평범한 dict도 허용하므로, 대소문자 구분 dict가 대문자
    'Authorization' 키로 넘어와도 안전하도록 두 형태 모두 조회한다.
    """
    return _bearer(headers.get("authorization") or headers.get("Authorization"))
