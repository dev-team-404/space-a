"""REST API 어댑터 (FastAPI).

관리 API(C4): 공간 생성, 에이전트 등록.
지식 생애주기(C1의 MVP REST 바인딩): 이슈 열기·해결.
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
        issue, doc = service.resolve_issue(
            _bearer(authorization),
            issue_id,
            body.summary,
            steps=body.steps,
            publish_knowledge=body.publish_knowledge,
            visibility=body.visibility,
        )
        resp = {"issue_id": issue.id, "status": issue.status}
        if doc is not None:
            resp["doc_id"] = doc.id
        return resp

    return app
