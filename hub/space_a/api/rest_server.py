"""REST API 어댑터 (FastAPI).

관리 API(C4): 공간 생성, 에이전트 등록.
지식 생애주기(C1의 MVP REST 바인딩): 이슈 열기·해결·검색·인용.
도메인 에러를 HTTP 상태로 매핑한다.
"""

from fastapi import FastAPI, Header, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from ..adapters.store_memory import InMemoryStore
from ..core import errors
from ..core.services import SpaceAService


class CreateSpaceBody(BaseModel):
    id: str
    name: str


class RegisterAgentBody(BaseModel):
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


_STATUS = {
    errors.InvalidRequest: 400,
    errors.Unauthorized: 401,
    errors.Forbidden: 403,
    errors.NotFound: 404,
}


def _bearer(authorization: str | None) -> str:
    if not authorization or not authorization.startswith("Bearer "):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len("Bearer ") :]


def create_app(service: SpaceAService | None = None) -> FastAPI:
    service = service or SpaceAService(InMemoryStore())
    app = FastAPI(title="Space A Hub")

    @app.exception_handler(errors.SpaceAError)
    async def _handle(_: Request, exc: errors.SpaceAError):
        status = _STATUS.get(type(exc), 400)
        return JSONResponse(
            status_code=status,
            content={"error": {"code": exc.code, "message": str(exc)}},
        )

    @app.post("/spaces", status_code=201)
    def create_space(body: CreateSpaceBody):
        s = service.create_space(body.id, body.name)
        return {"id": s.id, "name": s.name, "status": s.status}

    @app.post("/agents/register", status_code=201)
    def register_agent(body: RegisterAgentBody):
        agent, token = service.register_agent(body.name, body.space_id)
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
        }

    @app.post("/pages/{page_id}/move")
    def move_page(
        page_id: str, body: MovePageBody, authorization: str | None = Header(default=None)
    ):
        p = service.move_page(_bearer(authorization), page_id, body.new_parent_id)
        return {"page_id": p.id, "parent_id": p.parent_id}

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
                "children": [node(c) for c in by_parent.get(p.id, [])],
            }

        return {"tree": [node(r) for r in by_parent.get(None, [])]}

    @app.get("/spaces")
    def list_spaces():
        return {
            "spaces": [
                {"id": s.id, "name": s.name, "status": s.status}
                for s in service.list_spaces()
            ]
        }

    @app.get("/spaces/{space_id}")
    def get_space(space_id: str):
        s = service.get_space(space_id)
        return {"id": s.id, "name": s.name, "status": s.status}

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

    return app
