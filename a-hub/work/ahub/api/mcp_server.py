"""MCP 어댑터 — Space A hub를 MCP 도구로 노출 (C1 계약의 실체).

전송: Streamable HTTP. 신원은 전송 계층에서 온다 (계약 §3.1) — 각 도구가 주입된
Context에서 요청의 Authorization 헤더를 읽어 per-request Bearer로 판별한다.
build_mcp()는 시드하지 않는다 (dev 시드는 ahub/api/seed.py + demo_mcp.py).
"""

from __future__ import annotations

from mcp.server.fastmcp import Context, FastMCP

from ..adapters.factory import make_store
from ..core import errors
from ..core.services import SpaceAService
from ._auth import token_from_headers


def _token(ctx: Context) -> str:
    """이번 요청의 Bearer 토큰. 요청에 HTTP request가 없으면 Unauthorized."""
    req = ctx.request_context.request  # Streamable HTTP → Starlette Request
    if req is None:
        raise errors.Unauthorized("no request context")
    return token_from_headers(req.headers)


def build_mcp(service: SpaceAService | None = None) -> FastMCP:
    service = service or SpaceAService(make_store())
    mcp = FastMCP("space-a-hub")

    @mcp.tool()
    def get_guide(space_id: str, ctx: Context) -> dict:
        """이 방(space)의 작성 가이드를 읽는다. 쓰기 전에 먼저 호출하라."""
        _token(ctx)  # 인증만 강제 (get_guide는 공개 정보지만 신원은 요구)
        page = service.get_guide(space_id)
        if page is None:
            return {"guide": None}
        return {"page_id": page.id, "title": page.title, "body": page.body}

    @mcp.tool()
    def search_knowledge(
        query: str, ctx: Context, space_id: str | None = None, limit: int = 3
    ) -> dict:
        """유사한 과거 해결책을 찾는다. 스스로 추론하기 전에 먼저 호출하라.
        권한 범위(org 공개 + 내가 속한 space) 안에서만 검색된다."""
        res = service.search_knowledge(_token(ctx), query, space_id=space_id, limit=limit)
        return {
            "results": [
                {"page_id": p.id, "space_id": p.space_id, "title": p.title, "source": p.source}
                for p in res.pages
            ],
            "scanned": res.scanned,
        }

    @mcp.tool()
    def open_issue(title: str, space_id: str, ctx: Context) -> dict:
        """문제가 생긴 시점에 이슈를 연다 (해결 후가 아니라)."""
        issue = service.open_issue(_token(ctx), title, space_id)
        return {"issue_id": issue.id, "status": issue.status}

    @mcp.tool()
    def cite_knowledge(
        issue_id: str, page_id: str, ctx: Context, note: str | None = None
    ) -> dict:
        """기존 문서를 이 이슈에 재사용으로 기록한다. 이 순간 ReuseEvent가 생긴다(가장 중요)."""
        event, issue = service.cite_knowledge(_token(ctx), issue_id, page_id, note=note)
        return {
            "reuse_id": event.id,
            "page_id": event.page_id,
            "cross_team": event.cross_team,
            "issue_status": issue.status,
        }

    @mcp.tool()
    def resolve_issue(
        issue_id: str,
        summary: str,
        ctx: Context,
        steps: list[str] | None = None,
        publish_knowledge: bool = True,
        visibility: str = "org",
    ) -> dict:
        """해결을 기록하고 이슈를 닫는다. publish_knowledge면 해결이 Page로 발행된다."""
        issue, page = service.resolve_issue(
            _token(ctx),
            issue_id,
            summary,
            steps=steps,
            publish_knowledge=publish_knowledge,
            visibility=visibility,
        )
        out = {"issue_id": issue.id, "status": issue.status}
        if page is not None:
            out["page_id"] = page.id
        return out

    @mcp.tool()
    def get_skill_candidates(
        ctx: Context, space_id: str | None = None, min_occurrences: int = 3
    ) -> dict:
        """3회 이상 반복된 해결 패턴(= Skill 승격 후보)을 조회한다."""
        cands = service.get_skill_candidates(
            _token(ctx), space_id=space_id, min_occurrences=min_occurrences
        )
        return {
            "candidates": [
                {"pattern": c.pattern, "occurrences": c.occurrences, "page_ids": c.page_ids}
                for c in cands
            ]
        }

    return mcp
