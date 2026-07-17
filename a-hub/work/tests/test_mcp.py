"""MCP 어댑터 — FastMCP 도구가 core를 올바로 감싸는지.

mcp SDK가 없는 환경(시스템 python)에서는 skip된다.
"""

import asyncio
import json

import pytest

pytest.importorskip("mcp")

from space_a.api.mcp_server import build_mcp  # noqa: E402


def _call(mcp, name, args):
    res = asyncio.run(mcp.call_tool(name, args))
    return json.loads(res[0].text)


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


def test_demo_seed_exposes_guide_and_searchable_knowledge():
    mcp = build_mcp()
    guide = _call(mcp, "get_guide", {"space_id": "demo"})
    assert "search_knowledge" in guide["body"]
    found = _call(mcp, "search_knowledge", {"query": "인증서"})
    assert len(found["results"]) == 1


def test_reuse_loop_over_mcp():
    mcp = build_mcp()
    page_id = _call(mcp, "search_knowledge", {"query": "인증서"})["results"][0]["page_id"]
    issue = _call(mcp, "open_issue", {"title": "같은 문제", "space_id": "demo"})
    cite = _call(mcp, "cite_knowledge", {"issue_id": issue["issue_id"], "page_id": page_id})
    assert cite["reuse_id"].startswith("reuse_")
    assert cite["issue_status"] == "knowledge_linked"
