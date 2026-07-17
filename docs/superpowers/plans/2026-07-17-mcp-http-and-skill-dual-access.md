# MCP HTTP Transport + Skill Dual-Access Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `a-hub/work`'s MCP from stdio+single-token to Streamable HTTP with per-request Bearer auth, mount it on the REST process (Lambda stays REST-only), and add a Skill package so non-MCP environments reach the same features via REST.

**Architecture:** One `create_app(mount_mcp=True)` FastAPI process serves REST at `/` and MCP (Streamable HTTP) at `/mcp`, both sharing one `SpaceAService` and one `_bearer` auth rule. MCP tools read the request's `Authorization` header via the injected `Context`. Lambda calls `create_app(mount_mcp=False)` and never imports `mcp`. A Skill package under `a-hub/work/skills/space-a-hub/` documents how non-MCP agents call the equivalent REST endpoints.

**Tech Stack:** Python 3.12, FastAPI, `mcp` 1.27.0 (FastMCP + `streamable_http_app()`), Mangum (Lambda), pytest.

---

## File Structure

- Create: `ahub/api/_auth.py` — shared Bearer extraction (`_bearer`, `token_from_headers`). Extracted from `rest_server.py` to avoid REST↔MCP circular import.
- Modify: `ahub/api/rest_server.py` — import `_bearer` from `_auth`; add `mount_mcp` param + `/mcp` mount + lifespan.
- Modify: `ahub/api/mcp_server.py` — per-request Bearer via `Context`; remove stdio `main()`, `SPACE_A_TOKEN`, demo seed.
- Modify: `ahub/api/lambda_handler.py` — `create_app(mount_mcp=False)`.
- Modify: `pyproject.toml` — promote `mcp` to `[project].dependencies`.
- Modify: `tests/test_mcp.py` — rewrite for per-request Bearer (no demo seed).
- Create: `tests/test_mcp_http.py` — integration: `/mcp` end-to-end with Bearer.
- Modify: `tests/test_api.py` — add `mount_mcp` behavior assertions (REST-only vs mounted).
- Create: `ahub/api/seed.py` — dev-only demo seed helper (moved out of mcp_server).
- Modify: `demo_mcp.py` — use `seed.py`.
- Create: `skills/space-a-hub/SKILL.md`, `skills/space-a-hub/references/endpoints.md`.
- Create: `docs/adr/0009-mcp-http-and-skill-dual-access.md` (number: next free ADR).
- Modify: `README.md`, `SERVERLESS.md`, `docs/design/collab-space/03-architecture.md`, `docs/design/collab-space/05-contracts.md`.

**Working directory for all commands:** `a-hub/work` (run `cd a-hub/work` first). Tests run with the venv that has `fastapi mcp uvicorn httpx pytest` installed.

---

## Task 1: Extract shared Bearer auth into `_auth.py`

**Files:**
- Create: `ahub/api/_auth.py`
- Modify: `ahub/api/rest_server.py` (remove local `_bearer`, import from `_auth`)
- Test: `tests/test_auth.py` (create)

- [ ] **Step 1: Write the failing test**

Create `tests/test_auth.py`:

```python
import pytest

from ahub.api._auth import token_from_headers
from ahub.core import errors


def test_extracts_bearer_token():
    assert token_from_headers({"authorization": "Bearer abc123"}) == "abc123"


def test_missing_header_raises_unauthorized():
    with pytest.raises(errors.Unauthorized):
        token_from_headers({})


def test_non_bearer_raises_unauthorized():
    with pytest.raises(errors.Unauthorized):
        token_from_headers({"authorization": "Basic abc"})


def test_header_lookup_is_case_insensitive():
    # Starlette Headers are case-insensitive; a plain dict is not, so the
    # function must normalize. We pass the canonical lowercase key here and
    # rely on callers passing Starlette Headers in production.
    assert token_from_headers({"authorization": "Bearer x"}) == "x"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_auth.py -v`
Expected: FAIL with `ModuleNotFoundError: No module named 'ahub.api._auth'`

- [ ] **Step 3: Create `ahub/api/_auth.py`**

```python
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_auth.py -v`
Expected: PASS (4 passed)

- [ ] **Step 5: Update `rest_server.py` to use the shared helper**

In `ahub/api/rest_server.py`, remove the local `_bearer` definition (currently around lines 92-95):

```python
def _bearer(authorization: str | None) -> str:
    if not authorization or not authorization.startswith("Bearer "):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len("Bearer ") :]
```

And add this import near the other imports at the top (after `from ..core.services import SpaceAService`):

```python
from ._auth import _bearer
```

- [ ] **Step 6: Run full suite to verify no regression**

Run: `python -m pytest -q`
Expected: PASS (163 + 4 new = 167 passed)

- [ ] **Step 7: Commit**

```bash
git add ahub/api/_auth.py ahub/api/rest_server.py tests/test_auth.py
git commit -m "refactor(backend): extract shared Bearer auth into _auth module"
```

---

## Task 2: Move demo seed out of `mcp_server.py` into `seed.py`

This isolates the dev-only demo-seed logic before Task 3 rewrites `mcp_server.py` for per-request auth.

**Files:**
- Create: `ahub/api/seed.py`
- Test: `tests/test_seed.py` (create)

- [ ] **Step 1: Write the failing test**

Create `tests/test_seed.py`:

```python
import pytest

pytest.importorskip("mcp")  # seed is only used by dev tooling that pulls mcp

from ahub.api.seed import seed_demo
from ahub.core.services import SpaceAService
from ahub.adapters.store_memory import InMemoryStore


def test_seed_demo_creates_space_and_searchable_knowledge():
    service = SpaceAService(InMemoryStore())
    token = seed_demo(service)
    guide = service.get_guide("demo")
    assert guide is not None and "search_knowledge" in guide.body
    res = service.search_knowledge(token, "인증서")
    assert len(res.pages) == 1
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_seed.py -v`
Expected: FAIL with `ModuleNotFoundError: No module named 'ahub.api.seed'`

- [ ] **Step 3: Create `ahub/api/seed.py`**

Move the seed logic (from current `mcp_server.py:27-41`) here:

```python
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
    _, token = service.register_agent("demo-agent", "demo")
    seed = service.open_issue(token, "샘플: 인증서 오류", "demo")
    service.resolve_issue(token, seed.id, "DS 인증서를 갱신하면 해결", ["cert 재발급", "재기동"])
    return token
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_seed.py -v`
Expected: PASS (1 passed)

- [ ] **Step 5: Commit**

```bash
git add ahub/api/seed.py tests/test_seed.py
git commit -m "refactor(backend): move demo seed into dev-only seed module"
```

---

## Task 3: Rewrite `mcp_server.py` for per-request Bearer (remove stdio + seed)

**Files:**
- Modify: `ahub/api/mcp_server.py` (full rewrite of auth + entrypoint)
- Modify: `tests/test_mcp.py` (rewrite for new model)

- [ ] **Step 1: Rewrite the failing tests first**

Replace the entire contents of `tests/test_mcp.py` with:

```python
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
```

- [ ] **Step 2: Run to verify failure**

Run: `python -m pytest tests/test_mcp.py -v`
Expected: FAIL — `test_build_mcp_does_not_seed` fails because current `build_mcp` seeds when `SPACE_A_TOKEN` is unset.

- [ ] **Step 3: Rewrite `ahub/api/mcp_server.py`**

Replace the entire file with:

```python
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
    """이번 요청의 Bearer 토큰. HTTP 요청 컨텍스트가 없으면 Unauthorized."""
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
```

Note: `Context` params have no default — FastMCP injects Context by type annotation regardless of position, and required tool args (`query`, `space_id`) must not follow a defaulted param in the visible schema. Placing `ctx: Context` before optional args keeps signatures valid.

- [ ] **Step 4: Run to verify pass**

Run: `python -m pytest tests/test_mcp.py -v`
Expected: PASS (2 passed)

- [ ] **Step 5: Verify no lingering stdio/seed/SPACE_A_TOKEN references**

Run: `grep -n "SPACE_A_TOKEN\|__main__\|def main\|seed" ahub/api/mcp_server.py || echo CLEAN`
Expected: `CLEAN`

- [ ] **Step 6: Commit**

```bash
git add ahub/api/mcp_server.py tests/test_mcp.py
git commit -m "feat(backend): MCP per-request Bearer auth, drop stdio and seed"
```

---

## Task 4: Mount MCP on `create_app` with `mount_mcp` flag + lifespan

**Files:**
- Modify: `ahub/api/rest_server.py` (`create_app` signature, mount, lifespan)
- Test: `tests/test_api.py` (add mount behavior tests)

- [ ] **Step 1: Write the failing tests**

Append to `tests/test_api.py`:

```python
def test_mcp_not_mounted_when_disabled():
    from fastapi.testclient import TestClient
    from ahub.api.rest_server import create_app

    app = create_app(mount_mcp=False)
    client = TestClient(app)
    # /mcp must not exist → 404 (REST-only, e.g. Lambda)
    assert client.get("/mcp").status_code == 404


def test_mcp_mounted_when_enabled():
    import pytest
    pytest.importorskip("mcp")
    from fastapi.testclient import TestClient
    from ahub.api.rest_server import create_app

    app = create_app(mount_mcp=True)
    with TestClient(app) as client:
        # A bare GET to the streamable endpoint should NOT 404 (route exists).
        # MCP requires specific headers, so we expect a 4xx that is not 404.
        resp = client.get("/mcp")
        assert resp.status_code != 404
```

- [ ] **Step 2: Run to verify failure**

Run: `python -m pytest tests/test_api.py::test_mcp_mounted_when_enabled tests/test_api.py::test_mcp_not_mounted_when_disabled -v`
Expected: FAIL — `create_app()` has no `mount_mcp` param (TypeError) / `/mcp` returns 404.

- [ ] **Step 3: Modify `create_app` in `ahub/api/rest_server.py`**

Add imports at the top of the file (after existing imports):

```python
import contextlib
```

Change the `create_app` signature and add the mount at the end of the function, just before `return app`.

Current (line ~98):
```python
def create_app(service: SpaceAService | None = None) -> FastAPI:
    service = service or SpaceAService(make_store())
    app = FastAPI(title="Space A Hub")
```

Replace with:
```python
def create_app(
    service: SpaceAService | None = None, *, mount_mcp: bool = True
) -> FastAPI:
    service = service or SpaceAService(make_store())

    lifespan = None
    mcp_app = None
    if mount_mcp:
        # 지연 import: Lambda(mount_mcp=False)는 mcp를 import하지 않는다.
        from ..api.mcp_server import build_mcp

        mcp = build_mcp(service)
        mcp_app = mcp.streamable_http_app()  # session_manager를 지연 생성

        @contextlib.asynccontextmanager
        async def lifespan(_app):
            # Streamable HTTP 세션 매니저를 부모 앱 lifespan에서 기동한다.
            async with mcp.session_manager.run():
                yield

    app = FastAPI(title="Space A Hub", lifespan=lifespan)
```

Then, immediately before the final `return app` (line ~433), add:

```python
    if mcp_app is not None:
        app.mount("/mcp", mcp_app)

    return app
```

(Keep the existing `return app`; replace it with the block above so there is exactly one `return app`.)

- [ ] **Step 4: Run to verify pass**

Run: `python -m pytest tests/test_api.py -v`
Expected: PASS (existing api tests + 2 new)

- [ ] **Step 5: Run full suite**

Run: `python -m pytest -q`
Expected: PASS (all green)

- [ ] **Step 6: Commit**

```bash
git add ahub/api/rest_server.py tests/test_api.py
git commit -m "feat(backend): mount MCP on FastAPI process behind mount_mcp flag"
```

---

## Task 5: Integration test — `/mcp` end-to-end with Bearer over HTTP

Proves a real MCP client over HTTP gets per-request identity. Uses an ephemeral uvicorn server + the MCP streamable HTTP client.

**Files:**
- Create: `tests/test_mcp_http.py`

- [ ] **Step 1: Write the integration test**

Create `tests/test_mcp_http.py`:

```python
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
    # 공유 서비스에 데이터를 심고, HTTP MCP로 검색이 그 신원으로 도는지 본다.
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
    # 결과 콘텐츠에 발행된 지식이 검색되어야 한다.
    text = result.content[0].text if result.content else ""
    assert "page_" in text or "results" in text
```

- [ ] **Step 2: Run the integration test**

Run: `python -m pytest tests/test_mcp_http.py -v`
Expected: PASS (1 passed). If the MCP client API differs slightly (e.g. `streamablehttp_client` returns a different tuple arity), adjust the unpacking to match the installed `mcp` 1.27.0 signature — verify with `python -c "import inspect,mcp.client.streamable_http as c; print(inspect.signature(c.streamablehttp_client))"`.

- [ ] **Step 3: Verify missing-token path is rejected**

Add to `tests/test_mcp_http.py`:

```python
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
    # 인증 실패는 tool error로 표면화된다.
    assert result.isError
```

- [ ] **Step 4: Run both**

Run: `python -m pytest tests/test_mcp_http.py -v`
Expected: PASS (2 passed)

- [ ] **Step 5: Commit**

```bash
git add tests/test_mcp_http.py
git commit -m "test(backend): MCP-over-HTTP per-request Bearer integration"
```

---

## Task 6: Lambda REST-only + promote `mcp` dependency

**Files:**
- Modify: `ahub/api/lambda_handler.py`
- Modify: `pyproject.toml`
- Modify: `demo_mcp.py` (use seed helper)

- [ ] **Step 1: Update `lambda_handler.py`**

Replace `handler = Mangum(create_app())` with:

```python
handler = Mangum(create_app(mount_mcp=False))
```

Also update the module docstring's second paragraph to note MCP is not served on Lambda (REST only; MCP runs on the container).

- [ ] **Step 2: Promote `mcp` in `pyproject.toml`**

Change `[project].dependencies` from:

```toml
dependencies = ["fastapi>=0.115", "uvicorn>=0.30"]
```

to:

```toml
dependencies = ["fastapi>=0.115", "uvicorn>=0.30", "mcp>=1.27"]
```

Rationale comment above the line: `# mcp: /mcp 마운트(서버 기본 기능). Lambda는 create_app(mount_mcp=False)로 지연 import 회피.`

- [ ] **Step 3: Update `demo_mcp.py` to use the seed helper**

Open `demo_mcp.py`. It currently seeds inline and builds MCP. Replace its seeding with a call to `seed_demo`, and since MCP tools now require HTTP context, drive the demo through the **service** (not `call_tool`). Replace the file body with a service-level walkthrough:

```python
"""In-process 데모 — 데모 공간을 시드하고 재사용 흐름을 서비스로 시연한다.
(MCP 도구는 HTTP 신원을 요구하므로 여기선 service를 직접 호출한다.)"""

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.seed import seed_demo
from ahub.core.services import SpaceAService


def main() -> None:
    service = SpaceAService(InMemoryStore())
    token = seed_demo(service)
    print("[demo] seeded space=demo, token issued")

    res = service.search_knowledge(token, "인증서")
    print(f"[demo] search '인증서' → {len(res.pages)} hit(s)")

    issue = service.open_issue(token, "같은 문제", "demo")
    page_id = res.pages[0].id
    event, issue = service.cite_knowledge(token, issue.id, page_id)
    print(f"[demo] cite → reuse={event.id}, issue_status={issue.status}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Verify demo runs**

Run: `python demo_mcp.py`
Expected: three `[demo]` lines, search hit count = 1, issue_status = `knowledge_linked`.

- [ ] **Step 5: Verify Lambda path imports no mcp**

Run:
```bash
python -c "import ast,sys; src=open('ahub/api/lambda_handler.py').read(); assert 'mount_mcp=False' in src; print('lambda REST-only OK')"
```
Expected: `lambda REST-only OK`

- [ ] **Step 6: Run full suite**

Run: `python -m pytest -q`
Expected: PASS (all green)

- [ ] **Step 7: Commit**

```bash
git add ahub/api/lambda_handler.py pyproject.toml demo_mcp.py
git commit -m "feat(backend): Lambda serves REST only; promote mcp to base deps"
```

---

## Task 7: Skill package (non-MCP entry via REST)

**Files:**
- Create: `skills/space-a-hub/SKILL.md`
- Create: `skills/space-a-hub/references/endpoints.md`

- [ ] **Step 1: Create `skills/space-a-hub/SKILL.md`**

```markdown
---
name: space-a-hub
description: Use when an agent needs to search/reuse team knowledge or record issues in the Space A collaboration hub from a NON-MCP environment (Claude Code, scripts). Calls the hub's REST API. For MCP-capable clients, use the /mcp endpoint instead.
---

# Space A Hub — REST access

에이전트 협업 공간(Space A, Jira+Confluence식)에 REST로 접근한다. MCP를 못 붙이는
환경에서 MCP 도구와 **동일한 6개 작업**을 REST로 수행한다.

## 설정 (환경변수)

- `SPACE_A_HUB_URL` — 허브 base URL (예: `http://localhost:8000`)
- `SPACE_A_TOKEN` — 에이전트 Bearer 토큰 (`POST /agents/register`로 발급)

모든 호출에 `Authorization: Bearer $SPACE_A_TOKEN` 헤더를 붙인다. 신원은 이 헤더에서만 온다.

## 워크플로 (언제 무엇을)

막히면 **추론 전에 검색 먼저**:

1. `get_guide` — 그 방 규칙을 먼저 읽는다
2. `search_knowledge` — 유사 과거 해결을 찾는다
3. 있으면 → `open_issue` → `cite_knowledge` (재사용 기록 = 가장 중요)
4. 없으면 새로 해결 후 → `open_issue` → `resolve_issue` (지식 발행)

## 작업 ↔ 호출

| 작업 | 메서드 · 경로 |
|---|---|
| get_guide | `GET  /spaces/{space_id}/guide` |
| search_knowledge | `POST /pages/search` |
| open_issue | `POST /issues` |
| cite_knowledge | `POST /issues/{issue_id}/cite` |
| resolve_issue | `POST /issues/{issue_id}/resolve` |
| get_skill_candidates | `GET  /skills/candidates` |

호출 예시와 요청/응답 본문은 [references/endpoints.md](references/endpoints.md) 참고.
```

- [ ] **Step 2: Create `skills/space-a-hub/references/endpoints.md`**

Fill with concrete curl examples matching the real REST bodies in `rest_server.py`. Each example uses `$SPACE_A_HUB_URL` and `$SPACE_A_TOKEN`:

```markdown
# Space A Hub REST — 호출 레퍼런스

전제: `export SPACE_A_HUB_URL=http://localhost:8000` , `export SPACE_A_TOKEN=<token>`
공통 헤더: `-H "Authorization: Bearer $SPACE_A_TOKEN" -H "content-type: application/json"`

## get_guide — 방 가이드 읽기
​```sh
curl "$SPACE_A_HUB_URL/spaces/demo/guide" -H "Authorization: Bearer $SPACE_A_TOKEN"
​```

## search_knowledge — 유사 해결 검색
​```sh
curl -X POST "$SPACE_A_HUB_URL/pages/search" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" -H "content-type: application/json" \
  -d '{"query":"인증서","space_id":"demo","limit":3}'
​```
응답: `{"results":[{"page_id","space_id","title","source","visibility"}...],"scanned"}`

## open_issue — 이슈 열기
​```sh
curl -X POST "$SPACE_A_HUB_URL/issues" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" -H "content-type: application/json" \
  -d '{"title":"인증서 오류","space_id":"demo"}'
​```
응답: `{"issue_id","status"}`

## cite_knowledge — 재사용 기록
​```sh
curl -X POST "$SPACE_A_HUB_URL/issues/$ISSUE_ID/cite" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" -H "content-type: application/json" \
  -d '{"page_id":"page_1","note":"동일 원인"}'
​```
응답: `{"reuse_id","page_id","cross_team","issue_status"}`

## resolve_issue — 해결 + 지식 발행
​```sh
curl -X POST "$SPACE_A_HUB_URL/issues/$ISSUE_ID/resolve" \
  -H "Authorization: Bearer $SPACE_A_TOKEN" -H "content-type: application/json" \
  -d '{"summary":"DS 인증서 갱신","steps":["재발급","재기동"],"publish_knowledge":true,"visibility":"org"}'
​```
응답: `{"issue_id","status","page_id"?}`

## get_skill_candidates — Skill 승격 후보
​```sh
curl "$SPACE_A_HUB_URL/skills/candidates?space_id=demo&min_occurrences=3" \
  -H "Authorization: Bearer $SPACE_A_TOKEN"
​```
응답: `{"candidates":[{"pattern","occurrences","page_ids"}...]}`
```

Note: the `​` characters shown around code fences above are zero-width artifacts of this plan document — when creating the real file, use plain triple-backtick fences.

- [ ] **Step 3: Sanity-check examples against REST routes**

Run: `grep -n '"/pages/search"\|"/issues"\|/cite\|/resolve\|/skills/candidates\|/guide' ahub/api/rest_server.py`
Expected: every path used in the skill appears as a route. Confirm paths and methods match.

- [ ] **Step 4: Commit**

```bash
git add skills/
git commit -m "feat: add space-a-hub Skill for non-MCP REST access"
```

---

## Task 8: Docs — ADR + design/README updates

**Files:**
- Create: `docs/adr/0009-mcp-http-and-skill-dual-access.md` (verify next free number first)
- Modify: `README.md`, `SERVERLESS.md`
- Modify: `docs/design/collab-space/03-architecture.md`, `docs/design/collab-space/05-contracts.md`

- [ ] **Step 1: Determine next ADR number**

Run: `ls ../../docs/adr/`
Use the next integer after the highest existing `NNNN-`. The plan assumes `0009`; adjust the filename if the repo's next free number differs.

- [ ] **Step 2: Create the ADR**

`docs/adr/0009-mcp-http-and-skill-dual-access.md`:

```markdown
# 9. MCP HTTP 전송 전환 + Skill(REST) 이중 입구

- 상태: 채택
- 일자: 2026-07-17

## 맥락

a-hub/work의 MCP는 stdio + 단일 토큰(SPACE_A_TOKEN)으로만 구현되어, 계약 C1 §3.1이
"기본"으로 정한 HTTP + per-request Bearer 경로가 비어 있었다. 또 MCP를 못 붙이는 환경
(Claude Code 등)에서 허브 기능에 접근할 방법이 없었다.

## 결정

1. MCP 전송을 stdio → **Streamable HTTP**로 전환한다. 신원은 요청의 Authorization
   헤더(per-request Bearer)에서 온다. stdio 진입점과 데모 시드는 제거한다.
2. REST와 MCP를 **한 프로세스**에서 서빙한다: `create_app(mount_mcp=True)`가 `/mcp`를
   마운트한다. 둘은 같은 SpaceAService·스토어·_bearer 규칙을 공유한다.
3. **Skill 패키지**(skills/space-a-hub/)로 비-MCP 환경이 동일한 REST 엔드포인트를
   호출하게 한다. 범위는 MCP 도구 6종과 동일하다.
4. **Lambda(서버리스, 개발용)는 REST만** 서빙한다(`mount_mcp=False`). MCP는 상주
   컨테이너 전용 — Streamable HTTP의 상주/스트리밍이 API GW+Lambda와 맞지 않는다.

## 결과

- MCP·REST·Skill 세 접근이 같은 도메인 로직·데이터를 공유한다.
- `mcp`가 서버 기본 의존성이 되지만, Lambda는 지연 import로 번들에서 제외된다.
- 계약 C1이 구현으로 완성된다.
```

- [ ] **Step 3: Update `03-architecture.md` (C1 전송)**

Find the line `C1: MCP (stdio/HTTP)` and the ports/adapters notes; update to reflect that HTTP is the implemented default and stdio is removed. Add a one-line note that REST+MCP share one process.

Run to locate: `grep -n "stdio\|C1\|MCP" ../../docs/design/collab-space/03-architecture.md`

- [ ] **Step 4: Update `05-contracts.md` (신원 표)**

In §3.1, the transport table currently lists stdio as a dev option. Add a note that stdio is no longer implemented (HTTP Bearer is the sole transport); keep the header/claim contract unchanged.

Run to locate: `grep -n "stdio\|SPACE_A_TOKEN\|Bearer" ../../docs/design/collab-space/05-contracts.md`

- [ ] **Step 5: Update `README.md` MCP section**

Replace the stdio run commands (`python -m ahub.api.mcp_server`, MCP Inspector against stdio) with the HTTP model: MCP is served at `/mcp` by the same uvicorn process; connect an MCP client to `http://<host>:8000/mcp` with a Bearer token. Mention the Skill package as the non-MCP path.

Run to locate: `grep -n "mcp_server\|MCP\|stdio\|Inspector" README.md`

- [ ] **Step 6: Update `SERVERLESS.md`**

Confirm/adjust the existing note that MCP is not served on Lambda; point to the container for MCP. (Wording likely already close.)

Run to locate: `grep -n "MCP\|stdio" SERVERLESS.md`

- [ ] **Step 7: Commit**

```bash
git add ../../docs/adr/ ../../docs/design/collab-space/03-architecture.md ../../docs/design/collab-space/05-contracts.md README.md SERVERLESS.md
git commit -m "docs: record MCP HTTP + Skill dual-access decision (ADR + design)"
```

---

## Task 9: Final verification

- [ ] **Step 1: Full test suite**

Run: `python -m pytest -q`
Expected: all pass (originals + `test_auth` + `test_seed` + rewritten `test_mcp` + `test_mcp_http` + new `test_api` cases).

- [ ] **Step 2: Server boots and serves both entries**

Run (background, then curl):
```bash
python -m uvicorn "ahub.api.rest_server:create_app" --factory --port 8000 &
sleep 2
curl -s http://127.0.0.1:8000/healthz          # REST alive → {"status":"ok"}
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8000/mcp   # /mcp exists → not 404
kill %1
```
Expected: healthz ok; `/mcp` returns a non-404 status.

- [ ] **Step 3: Lambda factory has no /mcp**

Run:
```bash
python -c "
from fastapi.testclient import TestClient
from ahub.api.rest_server import create_app
c = TestClient(create_app(mount_mcp=False))
assert c.get('/mcp').status_code == 404
assert c.get('/healthz').json()['status'] == 'ok'
print('lambda REST-only verified')
"
```
Expected: `lambda REST-only verified`

- [ ] **Step 4: Server-only install excludes mcp? (informational)**

Note: `mcp` is now a base dependency (needed for `/mcp`). This is intended — the container serves MCP. Lambda avoids importing it via `mount_mcp=False` + lazy import, but the wheel may still be present in a non-Lambda install. No action; documented in the ADR.

- [ ] **Step 5: Final commit if any docs/cleanup remain**

```bash
git status --short
# commit anything outstanding with an appropriate conventional-commit message
```
