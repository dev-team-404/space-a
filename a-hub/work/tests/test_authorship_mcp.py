"""MCP search_knowledge 응답에 created_by(작성자 agent_id)가 포함되는지.

실제 Streamable HTTP 클라이언트로 도구를 호출한다(test_mcp_http.py와 동일 패턴).
mcp/uvicorn 없으면 skip.
"""

import asyncio
import json
import threading
import time

import pytest

pytest.importorskip("mcp")
pytest.importorskip("uvicorn")

import uvicorn  # noqa: E402
from mcp import ClientSession  # noqa: E402
from mcp.client.streamable_http import streamablehttp_client  # noqa: E402

from ahub.adapters.store_memory import InMemoryStore  # noqa: E402
from ahub.api.rest_server import create_app  # noqa: E402
from ahub.core.services import SpaceAService  # noqa: E402


class _Server:
    def __init__(self, app):
        cfg = uvicorn.Config(
            app, host="127.0.0.1", port=0, log_level="warning",
            timeout_graceful_shutdown=1,
        )
        self.server = uvicorn.Server(cfg)
        self.thread = threading.Thread(target=self.server.run, daemon=True)
        self.port = None

    def __enter__(self):
        self.thread.start()
        for _ in range(200):
            if self.server.started:
                self.port = self.server.servers[0].sockets[0].getsockname()[1]
                return self
            time.sleep(0.05)
        raise RuntimeError("server did not start")

    def __exit__(self, *exc):
        self.server.should_exit = True
        self.thread.join(timeout=5)


async def _call_search(url, token):
    headers = {"Authorization": f"Bearer {token}"}
    async with streamablehttp_client(url, headers=headers) as (read, write, _):
        async with ClientSession(read, write) as session:
            await session.initialize()
            return await session.call_tool("search_knowledge", {"query": "인증서"})


def test_mcp_search_includes_created_by():
    service = SpaceAService(InMemoryStore())
    service.create_space("demo", "데모", guidelines="g")
    agent, token = service.register_agent("a", "a", "demo")
    issue = service.open_issue(token, "인증서 오류", "demo")
    _, page = service.resolve_issue(token, issue.id, "인증서 갱신", ["재발급"])

    app = create_app(service, mount_mcp=True)
    with _Server(app) as server:
        url = f"http://127.0.0.1:{server.port}/mcp"
        result = asyncio.run(_call_search(url, token))

    assert not result.isError
    payload = json.loads(result.content[0].text)
    assert payload["results"][0]["created_by"] == agent.id
