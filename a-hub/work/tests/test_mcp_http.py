"""MCP over HTTP 통합 — 실제 Streamable HTTP 클라이언트가 per-request Bearer로
신원을 얻는지. 임시 uvicorn 서버를 띄운다. mcp/uvicorn 없으면 skip."""

import asyncio
import socket
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


def _free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class _Server:
    def __init__(self, app, port):
        cfg = uvicorn.Config(app, host="127.0.0.1", port=port, log_level="warning")
        self.server = uvicorn.Server(cfg)
        self.thread = threading.Thread(target=self.server.run, daemon=True)

    def __enter__(self):
        self.thread.start()
        for _ in range(100):
            if self.server.started:
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


def test_mcp_http_uses_per_request_bearer():
    service = SpaceAService(InMemoryStore())
    service.create_space("demo", "데모", guidelines="g")
    _, token = service.register_agent("a", "demo")
    issue = service.open_issue(token, "인증서 오류", "demo")
    service.resolve_issue(token, issue.id, "갱신", ["재발급"])

    app = create_app(service, mount_mcp=True)
    port = _free_port()
    url = f"http://127.0.0.1:{port}/mcp"
    with _Server(app, port):
        result = asyncio.run(_call_search(url, token))
    text = result.content[0].text if result.content else ""
    assert "page_" in text or "results" in text


def test_mcp_http_without_bearer_is_unauthorized():
    service = SpaceAService(InMemoryStore())
    app = create_app(service, mount_mcp=True)
    port = _free_port()
    url = f"http://127.0.0.1:{port}/mcp"

    async def _call_no_auth():
        async with streamablehttp_client(url) as (read, write, _):
            async with ClientSession(read, write) as session:
                await session.initialize()
                return await session.call_tool("search_knowledge", {"query": "x"})

    with _Server(app, port):
        result = asyncio.run(_call_no_auth())
    assert result.isError
