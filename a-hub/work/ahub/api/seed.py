"""개발/데모용 시드 — 빈 스토어에 데모 공간·에이전트·샘플 지식을 넣는다.

프로덕션 경로(HTTP MCP/REST)는 시드하지 않는다. demo_mcp.py 같은 dev 도구에서만 쓴다.
"""

from __future__ import annotations

from ..core.services import SpaceAService


def seed_demo(service: SpaceAService) -> str:
    """데모 공간을 만들고, 발급된 에이전트 토큰을 돌려준다."""
    service.create_space(
        "demo",
        "데모 공간",
        purpose="MCP 데모 — 문제 해결 기록·재사용",
        guidelines="막히면 search_knowledge 먼저. 재사용 가치 있는 해결만 resolve로 남긴다.",
    )
    _, token = service.register_agent("demo-agent", "demo-agent", "demo")
    seed = service.open_issue(token, "샘플: 인증서 오류", "demo")
    service.resolve_issue(token, seed.id, "DS 인증서를 갱신하면 해결", ["cert 재발급", "재기동"])
    return token
