"""REST API 어댑터 (FastAPI) — 방 방문 서버.

hub(Space/Page)와 별개의 프로세스. 엔드포인트 계약: docs/design/room-visit.md §4.
도메인 에러를 HTTP 상태로 매핑한다.
"""

from fastapi import FastAPI, Header, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from . import errors
from .rooms import RoomService
from .store import SqliteStore

_STATUS = {
    errors.InvalidRequest: 400,
    errors.Unauthorized: 401,
    errors.Forbidden: 403,
    errors.NotFound: 404,
    errors.CellTaken: 409,
}


class RoomRegisterBody(BaseModel):
    name: str
    mascot_seed: str = ""


class RoomEnterBody(BaseModel):
    cell: list[int] | None = None  # [x, y] — 생략 시 서버가 빈 셀 배정


class RoomMoveBody(BaseModel):
    cell: list[int]


class RoomDesignBody(BaseModel):
    wallpaper: str | None = None
    floor: str | None = None
    objects: list[dict] = []


def _bearer(authorization: str | None) -> str:
    if not authorization or not authorization.startswith("Bearer "):
        raise errors.Unauthorized("missing bearer token")
    return authorization[len("Bearer ") :]


def create_app(rooms: RoomService | None = None) -> FastAPI:
    # ROOM_SERVER_DB 설정 시 SQLite 영속(볼륨), 미설정이면 인메모리 — hub의 SPACE_A_DB와 동일 패턴
    rooms = rooms or RoomService(store=SqliteStore.from_env())
    app = FastAPI(title="Space A Room Server")

    @app.exception_handler(errors.RoomServerError)
    async def _handle(_: Request, exc: errors.RoomServerError):
        status = _STATUS.get(type(exc), 400)
        return JSONResponse(
            status_code=status,
            content={"error": {"code": exc.code, "message": str(exc)}},
        )

    @app.get("/")
    def discover():
        return {
            "service": "space-a-room-server",
            "description": "방 방문 서버 — 개인 방·에이전트 위치·방 디자인 (docs/design/room-visit.md)",
            "auth": "Authorization: Bearer <token> (등록: POST /rooms/register)",
            "openapi": "/docs",
        }

    @app.post("/rooms/register", status_code=201)
    def room_register(body: RoomRegisterBody):
        agent, token, room = rooms.register(body.name, body.mascot_seed)
        return {"agent_id": agent.agent_id, "token": token, "room_id": room.id}

    @app.get("/rooms")
    def room_list():
        return {"rooms": rooms.list_rooms()}

    @app.get("/rooms/me")
    def room_me(authorization: str | None = Header(default=None)):
        return rooms.me(_bearer(authorization))

    @app.patch("/rooms/me")
    def room_rename(body: RoomRegisterBody, authorization: str | None = Header(default=None)):
        return rooms.rename(_bearer(authorization), body.name)

    @app.get("/rooms/{room_id}")
    def room_state(room_id: str):
        return rooms.room_state(room_id)

    @app.post("/rooms/{room_id}/enter")
    def room_enter(room_id: str, body: RoomEnterBody, authorization: str | None = Header(default=None)):
        cell = (body.cell[0], body.cell[1]) if body.cell else None
        return rooms.enter(_bearer(authorization), room_id, cell)

    @app.post("/rooms/{room_id}/move")
    def room_move(room_id: str, body: RoomMoveBody, authorization: str | None = Header(default=None)):
        me = rooms.me(_bearer(authorization))
        if me["room_id"] != room_id:
            raise errors.InvalidRequest("그 방에 있지 않음 — 먼저 입장하세요")
        return rooms.move(_bearer(authorization), (body.cell[0], body.cell[1]))

    @app.put("/rooms/{room_id}/design")
    def room_design(room_id: str, body: RoomDesignBody, authorization: str | None = Header(default=None)):
        design: dict = {"objects": body.objects}
        if body.wallpaper is not None:
            design["wallpaper"] = body.wallpaper
        if body.floor is not None:
            design["floor"] = body.floor
        return rooms.set_design(_bearer(authorization), room_id, design)

    @app.get("/healthz")
    def healthz():
        return {"status": "ok"}

    @app.get("/readyz")
    def readyz():
        return {"status": "ready"}

    return app
