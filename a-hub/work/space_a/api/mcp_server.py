"""MCP 어댑터 — Space A hub를 MCP 도구로 노출 (C1 계약의 실체).

에이전트는 이 MCP 서버에 붙어 도구를 호출한다. **각 도구의 docstring이 곧
프로토콜 지침**이며, MCP 클라이언트가 에이전트 컨텍스트에 자동 주입한다.
(언제·무엇을 쓸지 같은 판단 지침은 운영자의 AGENTS.md 몫 — docs/agents.md.template.md)

MVP: 인메모리 저장소를 쓰는 dev/demo 서버.
- `SPACE_A_TOKEN`이 없으면 데모 공간·에이전트·샘플 지식을 시드하고 그 토큰을 쓴다(stderr 출력).
- 프로덕션은 영속 저장소 + SSO 토큰으로 교체 (ports & adapters라 core 불변).
"""

from __future__ import annotations

import os
import sys

from mcp.server.fastmcp import FastMCP

from ..adapters.factory import make_store
from ..core.services import SpaceAService


def build_mcp(service: SpaceAService | None = None) -> FastMCP:
    service = service or SpaceAService(make_store())
    token = os.environ.get("SPACE_A_TOKEN")

    if not token:
        service.create_space(
            "demo",
            "데모 공간",
            purpose="MCP 데모 — 문제 해결 기록·재사용",
            guidelines="막히면 search_knowledge 먼저. 재사용 가치 있는 해결만 resolve로 남긴다.",
        )
        agent, token = service.register_agent("demo-agent", "demo")
        seed = service.open_issue(token, "샘플: 인증서 오류", "demo")
        service.resolve_issue(token, seed.id, "DS 인증서를 갱신하면 해결", ["cert 재발급", "재기동"])
        print(
            f"[space-a-hub mcp] SPACE_A_TOKEN 미설정 → 데모 시드. "
            f"space=demo, agent={agent.id}, token={token}",
            file=sys.stderr,
        )

    mcp = FastMCP("space-a-hub")

    @mcp.tool()
    def get_guide(space_id: str) -> dict:
        """이 방(space)의 작성 가이드를 읽는다. 쓰기 전에 먼저 호출하라."""
        page = service.get_guide(space_id)
        if page is None:
            return {"guide": None}
        return {"page_id": page.id, "title": page.title, "body": page.body}

    @mcp.tool()
    def search_knowledge(query: str, space_id: str | None = None, limit: int = 3) -> dict:
        """유사한 과거 해결책을 찾는다. 스스로 추론하기 전에 먼저 호출하라.
        권한 범위(org 공개 + 내가 속한 space) 안에서만 검색된다."""
        res = service.search_knowledge(token, query, space_id=space_id, limit=limit)
        return {
            "results": [
                {"page_id": p.id, "space_id": p.space_id, "title": p.title, "source": p.source}
                for p in res.pages
            ],
            "scanned": res.scanned,
        }

    @mcp.tool()
    def open_issue(title: str, space_id: str) -> dict:
        """문제가 생긴 시점에 이슈를 연다 (해결 후가 아니라)."""
        issue = service.open_issue(token, title, space_id)
        return {"issue_id": issue.id, "status": issue.status}

    @mcp.tool()
    def cite_knowledge(issue_id: str, page_id: str, note: str | None = None) -> dict:
        """기존 문서를 이 이슈에 재사용으로 기록한다. 이 순간 ReuseEvent가 생긴다(가장 중요)."""
        event, issue = service.cite_knowledge(token, issue_id, page_id, note=note)
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
        steps: list[str] | None = None,
        publish_knowledge: bool = True,
        visibility: str = "org",
    ) -> dict:
        """해결을 기록하고 이슈를 닫는다. publish_knowledge면 해결이 Page로 발행된다."""
        issue, page = service.resolve_issue(
            token,
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
    def get_skill_candidates(space_id: str | None = None, min_occurrences: int = 3) -> dict:
        """3회 이상 반복된 해결 패턴(= Skill 승격 후보)을 조회한다."""
        cands = service.get_skill_candidates(
            token, space_id=space_id, min_occurrences=min_occurrences
        )
        return {
            "candidates": [
                {"pattern": c.pattern, "occurrences": c.occurrences, "page_ids": c.page_ids}
                for c in cands
            ]
        }

    return mcp


def main() -> None:
    build_mcp().run()  # stdio transport


if __name__ == "__main__":
    main()
