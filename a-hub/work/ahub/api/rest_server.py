"""REST API 어댑터 (FastAPI).

관리 API(C4): 공간 생성, 에이전트 등록.
지식 생애주기(C1의 MVP REST 바인딩): 이슈 열기·해결·검색·인용.
도메인 에러를 HTTP 상태로 매핑한다.
"""

import contextlib

from fastapi import FastAPI, Header, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from ..adapters.factory import make_store
from ..core import errors
from ..core.services import SpaceAService
from ._auth import _bearer


class UTF8JSONResponse(JSONResponse):
    """`application/json; charset=utf-8`을 명시하는 JSON 응답.

    기본 JSONResponse는 `application/json`만 보내므로, charset이 없으면
    CP949 기본 클라이언트(한국 Windows)가 UTF-8 바이트를 CP949로 오해석해
    한글이 mojibake(占…)로 깨진다. charset을 명시해 이를 막는다.
    """

    media_type = "application/json; charset=utf-8"


class CreateSpaceBody(BaseModel):
    id: str
    name: str
    purpose: str = ""
    guidelines: str = ""


class RegisterAgentBody(BaseModel):
    user_id: str
    name: str
    space_id: str


class OpenIssueBody(BaseModel):
    title: str
    space_id: str


class ResolveIssueBody(BaseModel):
    summary: str
    steps: list[str] | None = None
    publish_knowledge: bool = True
    visibility: str = "org"


class SearchBody(BaseModel):
    query: str
    space_id: str | None = None
    limit: int = 3


class CiteBody(BaseModel):
    page_id: str
    note: str | None = None


class CreatePageBody(BaseModel):
    title: str
    body: str = ""
    parent_id: str | None = None
    visibility: str = "org"


class MovePageBody(BaseModel):
    new_parent_id: str | None = None


class UpdateSpaceBody(BaseModel):
    name: str


class AddMemberBody(BaseModel):
    agent_id: str


class EditPageBody(BaseModel):
    title: str | None = None
    body: str | None = None


class VisibilityBody(BaseModel):
    visibility: str


class SupersedeBody(BaseModel):
    by: str


_STATUS = {
    errors.InvalidRequest: 400,
    errors.Unauthorized: 401,
    errors.Forbidden: 403,
    errors.NotFound: 404,
}


def create_app(
    service: SpaceAService | None = None, *, mount_mcp: bool = True
) -> FastAPI:
    service = service or SpaceAService(make_store())

    lifespan = None
    mcp_app = None
    if mount_mcp:
        # 지연 import: Lambda(mount_mcp=False)는 mcp를 import하지 않는다.
        from .mcp_server import build_mcp

        mcp = build_mcp(service)
        mcp_app = mcp.streamable_http_app()  # session_manager를 지연 생성

        @contextlib.asynccontextmanager
        async def lifespan(_app):
            # Streamable HTTP 세션 매니저를 부모 앱 lifespan에서 기동한다.
            async with mcp.session_manager.run():
                yield

    app = FastAPI(
        title="Space A Hub",
        lifespan=lifespan,
        default_response_class=UTF8JSONResponse,
    )

    @app.exception_handler(errors.SpaceAError)
    async def _handle(_: Request, exc: errors.SpaceAError):
        status = _STATUS.get(type(exc), 400)
        return UTF8JSONResponse(
            status_code=status,
            content={"error": {"code": exc.code, "message": str(exc)}},
        )

    @app.get("/")
    def discover():
        return {
            "service": "space-a-hub",
            "description": "에이전트들의 협업 공간 (Jira+Confluence). 두 타입: Issue(문제) + Page(문서).",
            "start_here": [
                "GET /spaces — 방 목록과 목적(purpose)을 본다",
                "GET /spaces/{id}/guide — 그 방의 작성 가이드를 먼저 읽는다",
                "막히면: search 먼저 → open_issue → cite → resolve",
                "문서 저작: POST /spaces/{id}/pages 로 트리에 쓴다",
            ],
            "auth": "Authorization: Bearer <token> (등록: POST /agents/register)",
            "openapi": "/docs",
        }

    @app.post("/spaces", status_code=201)
    def create_space(body: CreateSpaceBody):
        s = service.create_space(
            body.id, body.name, purpose=body.purpose, guidelines=body.guidelines
        )
        return {
            "id": s.id,
            "name": s.name,
            "status": s.status,
            "purpose": s.purpose,
            "guide_page_id": s.guide_page_id,
        }

    @app.post("/agents/register", status_code=201)
    def register_agent(body: RegisterAgentBody):
        agent, token = service.register_agent(body.user_id, body.name, body.space_id)
        return {"agent_id": agent.id, "spaces": agent.spaces, "token": token}

    @app.post("/issues", status_code=201)
    def open_issue(body: OpenIssueBody, authorization: str | None = Header(default=None)):
        issue = service.open_issue(_bearer(authorization), body.title, body.space_id)
        return {"issue_id": issue.id, "status": issue.status}

    @app.post("/issues/{issue_id}/resolve")
    def resolve_issue(
        issue_id: str,
        body: ResolveIssueBody,
        authorization: str | None = Header(default=None),
    ):
        issue, page = service.resolve_issue(
            _bearer(authorization),
            issue_id,
            body.summary,
            steps=body.steps,
            publish_knowledge=body.publish_knowledge,
            visibility=body.visibility,
        )
        resp = {"issue_id": issue.id, "status": issue.status}
        if page is not None:
            resp["page_id"] = page.id
        return resp

    @app.post("/pages/search")
    def search_pages(body: SearchBody, authorization: str | None = Header(default=None)):
        res = service.search_knowledge(
            _bearer(authorization), body.query, space_id=body.space_id, limit=body.limit
        )
        return {
            "results": [
                {
                    "page_id": p.id,
                    "space_id": p.space_id,
                    "title": p.title,
                    "source": p.source,
                    "visibility": p.visibility,
                    "created_by": p.created_by,
                }
                for p in res.pages
            ],
            "scanned": res.scanned,
        }

    @app.post("/issues/{issue_id}/cite")
    def cite(issue_id: str, body: CiteBody, authorization: str | None = Header(default=None)):
        event, issue = service.cite_knowledge(
            _bearer(authorization), issue_id, body.page_id, note=body.note
        )
        return {
            "reuse_id": event.id,
            "page_id": event.page_id,
            "cross_team": event.cross_team,
            "issue_status": issue.status,
        }

    @app.post("/spaces/{space_id}/pages", status_code=201)
    def create_page(
        space_id: str, body: CreatePageBody, authorization: str | None = Header(default=None)
    ):
        p = service.create_page(
            _bearer(authorization),
            space_id,
            body.title,
            body=body.body,
            parent_id=body.parent_id,
            visibility=body.visibility,
        )
        return {
            "page_id": p.id,
            "space_id": p.space_id,
            "title": p.title,
            "parent_id": p.parent_id,
            "source": p.source,
            "created_by": p.created_by,
        }

    @app.get("/pages/{page_id}")
    def get_page(page_id: str, authorization: str | None = Header(default=None)):
        p = service.get_page(_bearer(authorization), page_id)
        return {
            "page_id": p.id,
            "space_id": p.space_id,
            "title": p.title,
            "body": p.body,
            "parent_id": p.parent_id,
            "source": p.source,
            "visibility": p.visibility,
            "created_by": p.created_by,
        }

    @app.post("/pages/{page_id}/move")
    def move_page(
        page_id: str, body: MovePageBody, authorization: str | None = Header(default=None)
    ):
        p = service.move_page(_bearer(authorization), page_id, body.new_parent_id)
        return {"page_id": p.id, "parent_id": p.parent_id}

    @app.patch("/pages/{page_id}")
    def edit_page(
        page_id: str, body: EditPageBody, authorization: str | None = Header(default=None)
    ):
        p = service.edit_page(_bearer(authorization), page_id, title=body.title, body=body.body)
        return {"page_id": p.id, "title": p.title, "body": p.body}

    @app.patch("/pages/{page_id}/visibility")
    def set_visibility(
        page_id: str, body: VisibilityBody, authorization: str | None = Header(default=None)
    ):
        p = service.set_visibility(_bearer(authorization), page_id, body.visibility)
        return {"page_id": p.id, "visibility": p.visibility}

    @app.post("/pages/{page_id}/archive")
    def archive_page(page_id: str, authorization: str | None = Header(default=None)):
        p = service.archive_page(_bearer(authorization), page_id)
        return {"page_id": p.id, "status": p.status}

    @app.post("/pages/{page_id}/supersede")
    def supersede_page(
        page_id: str, body: SupersedeBody, authorization: str | None = Header(default=None)
    ):
        p = service.supersede_page(_bearer(authorization), page_id, body.by)
        return {"page_id": p.id, "status": p.status, "superseded_by": p.superseded_by}

    @app.post("/pages/{page_id}/quarantine")
    def quarantine_page(page_id: str, authorization: str | None = Header(default=None)):
        p = service.quarantine_page(_bearer(authorization), page_id)
        return {"page_id": p.id, "status": p.status}

    @app.post("/pages/{page_id}/flag")
    def flag_page(page_id: str, authorization: str | None = Header(default=None)):
        p = service.flag_page(_bearer(authorization), page_id)
        return {"page_id": p.id, "flags": p.flags}

    @app.get("/spaces/{space_id}/tree")
    def space_tree(space_id: str, authorization: str | None = Header(default=None)):
        pages = service.list_pages(_bearer(authorization), space_id)
        by_parent: dict[str | None, list] = {}
        for p in pages:
            by_parent.setdefault(p.parent_id, []).append(p)

        def node(p):
            return {
                "page_id": p.id,
                "title": p.title,
                "created_by": p.created_by,
                "children": [node(c) for c in by_parent.get(p.id, [])],
            }

        return {"tree": [node(r) for r in by_parent.get(None, [])]}

    @app.get("/spaces")
    def list_spaces():
        return {
            "spaces": [
                {"id": s.id, "name": s.name, "status": s.status, "purpose": s.purpose}
                for s in service.list_spaces()
            ]
        }

    @app.get("/spaces/{space_id}")
    def get_space(space_id: str):
        s = service.get_space(space_id)
        return {
            "id": s.id,
            "name": s.name,
            "status": s.status,
            "purpose": s.purpose,
            "guidelines": s.guidelines,
            "guide_page_id": s.guide_page_id,
        }

    @app.get("/spaces/{space_id}/guide")
    def get_guide(space_id: str):
        page = service.get_guide(space_id)
        if page is None:
            raise errors.NotFound(f"space '{space_id}' has no guide")
        return {"page_id": page.id, "title": page.title, "body": page.body}

    @app.get("/issues")
    def list_issues(
        space_id: str | None = None,
        status: str | None = None,
        mine: bool = False,
        authorization: str | None = Header(default=None),
    ):
        issues = service.list_issues(
            _bearer(authorization), space_id=space_id, status=status, mine=mine
        )
        return {
            "issues": [
                {
                    "issue_id": i.id,
                    "space_id": i.space_id,
                    "title": i.title,
                    "status": i.status,
                    "opened_by": i.opened_by,
                }
                for i in issues
            ]
        }

    @app.get("/issues/{issue_id}")
    def get_issue(issue_id: str, authorization: str | None = Header(default=None)):
        i = service.get_issue(_bearer(authorization), issue_id)
        return {
            "issue_id": i.id,
            "space_id": i.space_id,
            "title": i.title,
            "status": i.status,
            "opened_by": i.opened_by,
        }

    @app.patch("/spaces/{space_id}")
    def update_space(space_id: str, body: UpdateSpaceBody):
        s = service.update_space(space_id, body.name)
        return {"id": s.id, "name": s.name, "status": s.status}

    @app.post("/spaces/{space_id}/archive")
    def archive_space(space_id: str):
        s = service.archive_space(space_id)
        return {"id": s.id, "status": s.status}

    @app.get("/spaces/{space_id}/members")
    def list_members(space_id: str, authorization: str | None = Header(default=None)):
        members = service.list_members(_bearer(authorization), space_id)
        return {
            "members": [
                {"agent_id": a.id, "name": a.name, "spaces": a.spaces} for a in members
            ]
        }

    @app.post("/spaces/{space_id}/members", status_code=201)
    def add_member(
        space_id: str, body: AddMemberBody, authorization: str | None = Header(default=None)
    ):
        a = service.add_member(_bearer(authorization), space_id, body.agent_id)
        return {"agent_id": a.id, "spaces": a.spaces}

    @app.delete("/spaces/{space_id}/members/{agent_id}")
    def remove_member(
        space_id: str, agent_id: str, authorization: str | None = Header(default=None)
    ):
        service.remove_member(_bearer(authorization), space_id, agent_id)
        return {"removed": agent_id}

    @app.get("/agents")
    def list_agents(authorization: str | None = Header(default=None)):
        agents = service.list_agents(_bearer(authorization))
        return {
            "agents": [
                {"agent_id": a.id, "name": a.name, "spaces": a.spaces} for a in agents
            ]
        }

    @app.get("/agents/{agent_id}")
    def get_agent(agent_id: str, authorization: str | None = Header(default=None)):
        a = service.get_agent(_bearer(authorization), agent_id)
        return {"agent_id": a.id, "name": a.name, "spaces": a.spaces}

    @app.delete("/agents/{agent_id}")
    def revoke_agent(agent_id: str, authorization: str | None = Header(default=None)):
        service.revoke_agent(_bearer(authorization), agent_id)
        return {"revoked": agent_id}

    @app.post("/agents/{agent_id}/rotate-token")
    def rotate_token(agent_id: str, authorization: str | None = Header(default=None)):
        new = service.rotate_token(_bearer(authorization), agent_id)
        return {"agent_id": agent_id, "token": new}

    @app.get("/skills/candidates")
    def skill_candidates(
        space_id: str | None = None,
        min_occurrences: int = 3,
        authorization: str | None = Header(default=None),
    ):
        cands = service.get_skill_candidates(
            _bearer(authorization), space_id=space_id, min_occurrences=min_occurrences
        )
        return {
            "candidates": [
                {"pattern": c.pattern, "occurrences": c.occurrences, "page_ids": c.page_ids}
                for c in cands
            ]
        }

    @app.get("/healthz")
    def healthz():
        return {"status": "ok"}

    @app.get("/readyz")
    def readyz():
        return {"status": "ready"}

    if mcp_app is not None:
        # pairs with streamable_http_path="/" in build_mcp — mount prefix supplies /mcp
        app.mount("/mcp", mcp_app)

    return app
