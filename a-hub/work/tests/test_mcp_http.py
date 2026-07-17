"""MCP over HTTP 통합 — 실제 Streamable HTTP 클라이언트가 per-request Bearer로
신원을 얻는지. 임시 uvicorn 서버를 띄운다. mcp/uvicorn 없으면 skip."""

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
    """port=0으로 바인딩하고 기동 후 실제 포트를 읽어와 TOCTOU 경합을 피한다."""

    def __init__(self, app):
        # timeout_graceful_shutdown: MCP Streamable HTTP는 SSE GET 연결을 열어두므로,
        # 종료 시 uvicorn이 그 연결을 무한정 기다리다 스레드가 멈추는 경우가 있다.
        # 유예 시간을 짧게 줘서 남은 연결을 강제로 닫고 확실히 종료시킨다.
        cfg = uvicorn.Config(
            app,
            host="127.0.0.1",
            port=0,
            log_level="warning",
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
        assert not self.thread.is_alive(), "uvicorn server thread did not stop"


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
    _, page = service.resolve_issue(token, issue.id, "인증서 갱신", ["재발급"])

    app = create_app(service, mount_mcp=True)
    with _Server(app) as server:
        url = f"http://127.0.0.1:{server.port}/mcp"
        result = asyncio.run(_call_search(url, token))

    assert not result.isError
    payload = json.loads(result.content[0].text)
    assert len(payload["results"]) == 1
    assert payload["results"][0]["page_id"] == page.id


def test_mcp_http_without_bearer_is_unauthorized():
    service = SpaceAService(InMemoryStore())
    app = create_app(service, mount_mcp=True)

    async def _call_no_auth(url):
        async with streamablehttp_client(url) as (read, write, _):
            async with ClientSession(read, write) as session:
                await session.initialize()
                return await session.call_tool("search_knowledge", {"query": "x"})

    with _Server(app) as server:
        url = f"http://127.0.0.1:{server.port}/mcp"
        result = asyncio.run(_call_no_auth(url))
    assert result.isError
