"""REST API 어댑터 (FastAPI) — 방 방문 서버.

hub(Space/Page)와 별개의 프로세스. 엔드포인트 계약: docs/design/life-visit.md §4.
도메인 에러를 HTTP 상태로 매핑한다.
"""

import os
import secrets

from fastapi import Body, FastAPI, Header, Request
from fastapi.responses import JSONResponse, Response
from pydantic import BaseModel

from . import errors
from .life import LifeService
from .store import SqliteStore

_STATUS = {
    errors.InvalidRequest: 400,
    errors.Unauthorized: 401,
    errors.Forbidden: 403,
    errors.NotFound: 404,
    errors.CellTaken: 409,
}

# x-api-key 관문을 면제할 경로. healthz/readyz는 로드밸런서·모니터링용, docs/openapi.json은
# 브라우저로 API 문서를 열람할 수 있게(관문을 켜면 헤더를 못 실으므로) 면제한다.
_API_KEY_EXEMPT_PATHS = {"/healthz", "/readyz", "/docs", "/openapi.json"}


def _api_key_ok(path: str, provided: str | None) -> bool:
    """고정 공유키 x-api-key 검증 — hub의 SPACE_A_API_KEY와 같은 opt-in 패턴.

    LIFE_SERVER_API_KEY 환경변수가 없으면(미설정) 검사를 건너뛴다 — 로컬·테스트 편의.
    설정된 배포에서는 면제 경로를 제외한 모든 요청에서 키 일치를 요구한다.
    타이밍 공격을 피해 secrets.compare_digest로 상수 시간 비교한다.
    """
    expected = os.environ.get("LIFE_SERVER_API_KEY")
    if not expected:
        return True
    if path in _API_KEY_EXEMPT_PATHS:
        return True
    return provided is not None and secrets.compare_digest(provided, expected)


class LifeRegisterBody(BaseModel):
    name: str
    mascot_seed: str = ""


class LifeEnterBody(BaseModel):
    cell: list[int] | None = None  # [x, y] — 생략 시 서버가 빈 셀 배정


class LifeMoveBody(BaseModel):
    cell: list[int]


class LifeDesignBody(BaseModel):
    wallpaper: str | None = None
    floor: str | None = None
    objects: list[dict] = []


class FriendBody(BaseModel):
    enabled: bool


class DiaryShareBody(BaseModel):
    body: str
    visibility: str


class VisibilityBody(BaseModel):
    visibility: str


class TextBody(BaseModel):
    body: str = ""


def _bearer(authorization: str | None) -> str:
    if not authorization or not authorization.startswith("Bearer "):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len("Bearer ") :]


def create_app(life: LifeService | None = None) -> FastAPI:
    # LIFE_SERVER_DB 설정 시 SQLite 영속(볼륨), 미설정이면 인메모리 — hub의 SPACE_A_DB와 동일 패턴
    life = life or LifeService(store=SqliteStore.from_env())
    app = FastAPI(title="Space A Life Server")

    @app.middleware("http")
    async def _api_key_gate(request: Request, call_next):
        # LIFE_SERVER_API_KEY 설정 시 x-api-key 헤더를 요구 (면제 경로 제외). 미설정이면 무관.
        if not _api_key_ok(request.url.path, request.headers.get("x-api-key")):
            return JSONResponse(
                status_code=401,
                content={"error": {"code": "unauthorized", "message": "invalid or missing x-api-key"}},
            )
        return await call_next(request)

    @app.exception_handler(errors.LifeServerError)
    async def _handle(_: Request, exc: errors.LifeServerError):
        status = _STATUS.get(type(exc), 400)
        return JSONResponse(
            status_code=status,
            content={"error": {"code": exc.code, "message": str(exc)}},
        )

    @app.get("/")
    def discover():
        return {
            "service": "space-a-life-server",
            "description": "방 방문 서버 — 개인 방·에이전트 위치·방 디자인 (docs/design/life-visit.md)",
            "auth": "Authorization: Bearer <token> (등록: POST /life/register)",
            # LIFE_SERVER_API_KEY 설정 시 모든 요청에 x-api-key 헤더 필요 (healthz/readyz 제외)
            "api_key_required": bool(os.environ.get("LIFE_SERVER_API_KEY")),
            "openapi": "/docs",
            "life_protocol": 3,
        }

    @app.get("/capabilities")
    def capabilities():
        return {"life_protocol": 3, "grid": {"w": 20, "h": 20}, "floor_min_y": 0, "footprint_mask": True, "wall_objects": True}

    @app.post("/life/register", status_code=201)
    def life_register(body: LifeRegisterBody):
        agent, token, created_life = life.register(body.name, body.mascot_seed)
        return {"agent_id": agent.agent_id, "token": token, "life_id": created_life.id}

    @app.get("/life")
    def life_list():
        return {"life": life.list_life()}

    @app.get("/life/me")
    def life_me(authorization: str | None = Header(default=None)):
        return life.me(_bearer(authorization))

    @app.patch("/life/me")
    def life_rename(body: LifeRegisterBody, authorization: str | None = Header(default=None)):
        return life.rename(_bearer(authorization), body.name)

    @app.get("/life/people")
    def life_people(authorization: str | None = Header(default=None)):
        return {"people": life.people(_bearer(authorization))}

    @app.put("/life/friends/{agent_id}")
    def life_friend(agent_id: str, body: FriendBody, authorization: str | None = Header(default=None)):
        return life.set_friend(_bearer(authorization), agent_id, body.enabled)

    @app.put("/life/me/diaries/{date}")
    def life_share_diary(date: str, body: DiaryShareBody, authorization: str | None = Header(default=None)):
        return life.share_diary(_bearer(authorization), date, body.body, body.visibility)

    @app.put("/life/me/content-visibility/{feature}")
    def life_content_visibility(feature: str, body: VisibilityBody, authorization: str | None = Header(default=None)):
        return life.set_content_visibility(_bearer(authorization), feature, body.visibility)

    @app.delete("/life/me/diaries/{date}")
    def life_unshare_diary(date: str, authorization: str | None = Header(default=None)):
        return life.unshare_diary(_bearer(authorization), date)

    @app.patch("/life/me/bubble")
    def life_bubble(body: TextBody, authorization: str | None = Header(default=None)):
        return life.set_bubble(_bearer(authorization), body.body)

    @app.put("/life/me/mascot-image")
    def life_mascot_image_put(png: bytes = Body(media_type="image/png"), authorization: str | None = Header(default=None)):
        return life.set_mascot_image(_bearer(authorization), png)

    @app.get("/life/agents/{agent_id}/mascot-image")
    def life_mascot_image_get(agent_id: str, authorization: str | None = Header(default=None)):
        png, digest = life.mascot_image(_bearer(authorization), agent_id)
        return Response(content=png, media_type="image/png", headers={"ETag": f'"{digest}"', "Cache-Control": "private, max-age=300"})

    @app.post("/life/me/disconnect")
    def life_disconnect(authorization: str | None = Header(default=None)):
        return life.disconnect(_bearer(authorization))

    @app.get("/life/{life_id}")
    def life_state(life_id: str):
        return life.life_state(life_id)

    @app.get("/life/{life_id}/diaries")
    def life_diaries(life_id: str, authorization: str | None = Header(default=None)):
        return {"diaries": life.shared_diaries(_bearer(authorization), life_id)}

    @app.get("/life/{life_id}/content-access")
    def life_content_access(life_id: str, authorization: str | None = Header(default=None)):
        return life.content_access(_bearer(authorization), life_id)

    @app.get("/life/{life_id}/guestbook")
    def life_guestbook(life_id: str):
        return {"entries": life.guestbook(life_id)}

    @app.post("/life/{life_id}/guestbook", status_code=201)
    def life_guestbook_add(life_id: str, body: TextBody, authorization: str | None = Header(default=None)):
        return life.add_guestbook(_bearer(authorization), life_id, body.body)

    @app.delete("/life/guestbook/{entry_id}")
    def life_guestbook_delete(entry_id: str, authorization: str | None = Header(default=None)):
        return life.delete_guestbook(_bearer(authorization), entry_id)

    @app.post("/life/{life_id}/enter")
    def life_enter(life_id: str, body: LifeEnterBody, authorization: str | None = Header(default=None)):
        cell = (body.cell[0], body.cell[1]) if body.cell else None
        return life.enter(_bearer(authorization), life_id, cell)

    @app.post("/life/{life_id}/move")
    def life_move(life_id: str, body: LifeMoveBody, authorization: str | None = Header(default=None)):
        me = life.me(_bearer(authorization))
        if me["life_id"] != life_id:
            raise errors.InvalidRequest("그 방에 있지 않음 — 먼저 입장하세요")
        return life.move(_bearer(authorization), (body.cell[0], body.cell[1]))

    @app.put("/life/{life_id}/design")
    def life_design(life_id: str, body: LifeDesignBody, authorization: str | None = Header(default=None)):
        design: dict = {"objects": body.objects}
        if body.wallpaper is not None:
            design["wallpaper"] = body.wallpaper
        if body.floor is not None:
            design["floor"] = body.floor
        return life.set_design(_bearer(authorization), life_id, design)

    @app.get("/healthz")
    def healthz():
        return {"status": "ok"}

    @app.get("/readyz")
    def readyz():
        return {"status": "ready"}

    return app
