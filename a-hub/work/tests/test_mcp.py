"""MCP 어댑터 — FastMCP 도구가 core를 per-request Bearer로 감싸는지.

도구는 요청 헤더에서 신원을 얻으므로, 직접 call_tool은 request context가 없어
Unauthorized가 난다. HTTP 경유 신원 검증은 tests/test_mcp_http.py 참고.
mcp SDK가 없는 환경에서는 skip.
"""

import asyncio

import pytest

pytest.importorskip("mcp")

from ahub.api.mcp_server import build_mcp  # noqa: E402


def test_tools_registered():
    mcp = build_mcp()
    names = {t.name for t in asyncio.run(mcp.list_tools())}
    assert {
        "get_guide",
        "search_knowledge",
        "open_issue",
        "cite_knowledge",
        "resolve_issue",
        "get_skill_candidates",
    } <= names


def test_build_mcp_does_not_seed():
    # 프로덕션 경로는 시드하지 않는다: build_mcp 자체가 데모 공간을 만들면 안 된다.
    from ahub.adapters.store_memory import InMemoryStore
    from ahub.core.services import SpaceAService

    service = SpaceAService(InMemoryStore())
    build_mcp(service)
    assert service.get_guide("demo") is None
