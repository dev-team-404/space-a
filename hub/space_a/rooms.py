"""방 방문(Room Visit) — 개인 방·에이전트 위치·방 디자인.

설계: docs/design/room-visit.md
- 유저당 방 1개, 30×16 정사각 셀 격자. 에이전트 점유 = 논리 1셀.
- 위치의 단일 원천은 서버. 겹침 금지는 전역 락 안에서 "빈 셀일 때만 점유"로 원자 처리.
- 방 디자인(벽지·바닥·가구)은 방문자에게 보여주는 공개 표면이므로 서버가 가진다.
  사적 내용(다이어리 등)은 클라이언트에만 있다 — 여기 없음이 설계다.
- 기존 Space/Page 도메인과 별개 모듈. MVP는 인메모리(재시작 시 초기화).
"""

import secrets
import threading
import uuid
from dataclasses import dataclass, field

from .core import errors

GRID_W = 30
GRID_H = 16

Cell = tuple[int, int]


class CellTaken(errors.SpaceAError):
    code = "cell_taken"


@dataclass
class RoomObject:
    kind: str  # 클라이언트가 해석하는 가구 식별자 (예: "plant", "rug")
    cell: Cell


@dataclass
class RoomDesign:
    wallpaper: str = "lavender"
    floor: str = "cream"
    objects: list[RoomObject] = field(default_factory=list)


@dataclass
class RoomAgent:
    agent_id: str
    name: str
    room_id: str  # 자기 방 (소유)
    at_room: str  # 현재 있는 방
    cell: Cell
    mascot_seed: str = ""  # 클라이언트 마스코트 시드 — 어느 방에서든 같은 로봇으로 보이게


@dataclass
class Room:
    id: str
    owner_agent_id: str
    owner_name: str
    design: RoomDesign = field(default_factory=RoomDesign)


def _validate_cell(cell: Cell) -> None:
    x, y = cell
    if not (0 <= x < GRID_W and 0 <= y < GRID_H):
        raise errors.InvalidRequest(f"셀 범위 밖: ({x},{y}) — 0~{GRID_W - 1} × 0~{GRID_H - 1}")


class RoomService:
    """인메모리 방 서비스. 모든 변이는 self._lock 안 — 겹침 금지의 원자성 보장."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._rooms: dict[str, Room] = {}
        self._agents: dict[str, RoomAgent] = {}
        self._tokens: dict[str, str] = {}  # token -> agent_id

    # --- 신원 ---

    def register(self, name: str, mascot_seed: str = "") -> tuple[RoomAgent, str, Room]:
        """유저 등록 + 개인 방 생성. 에이전트는 자기 방에 자동 입장."""
        name = name.strip()
        if not name:
            raise errors.InvalidRequest("이름이 비어 있음")
        with self._lock:
            agent_id = f"ragt_{uuid.uuid4().hex[:8]}"
            room_id = f"room_{uuid.uuid4().hex[:8]}"
            room = Room(id=room_id, owner_agent_id=agent_id, owner_name=name)
            self._rooms[room_id] = room
            agent = RoomAgent(
                agent_id=agent_id, name=name, room_id=room_id, at_room=room_id,
                cell=self._free_cell_locked(room_id), mascot_seed=mascot_seed,
            )
            self._agents[agent_id] = agent
            token = secrets.token_urlsafe(24)
            self._tokens[token] = agent_id
            return agent, token, room

    def _authed(self, token: str | None) -> RoomAgent:
        agent_id = self._tokens.get(token or "")
        if agent_id is None:
            raise errors.Unauthorized("invalid or missing token")
        return self._agents[agent_id]

    def rename(self, token: str | None, name: str) -> dict:
        """유저 이름 변경 — 에이전트 이름과 자기 방의 주인 이름을 함께 바꾼다."""
        name = name.strip()
        if not name:
            raise errors.InvalidRequest("이름이 비어 있음")
        agent = self._authed(token)
        with self._lock:
            agent.name = name
            self._rooms[agent.room_id].owner_name = name
        return self.me(token)

    # --- 조회 ---

    def list_rooms(self) -> list[dict]:
        with self._lock:
            return [
                {
                    "room_id": r.id,
                    "owner_name": r.owner_name,
                    "occupants": sum(1 for a in self._agents.values() if a.at_room == r.id),
                }
                for r in self._rooms.values()
            ]

    def room_state(self, room_id: str) -> dict:
        with self._lock:
            room = self._rooms.get(room_id)
            if room is None:
                raise errors.NotFound(f"room '{room_id}' not found")
            return {
                "room_id": room.id,
                "owner_name": room.owner_name,
                "grid": {"w": GRID_W, "h": GRID_H},
                "design": {
                    "wallpaper": room.design.wallpaper,
                    "floor": room.design.floor,
                    "objects": [{"kind": o.kind, "cell": list(o.cell)} for o in room.design.objects],
                },
                "occupants": [
                    {
                        "agent_id": a.agent_id,
                        "name": a.name,
                        "cell": list(a.cell),
                        "is_owner": a.agent_id == room.owner_agent_id,
                        "mascot_seed": a.mascot_seed,
                    }
                    for a in self._agents.values()
                    if a.at_room == room.id
                ],
            }

    def me(self, token: str | None) -> dict:
        agent = self._authed(token)
        with self._lock:
            return {
                "agent_id": agent.agent_id,
                "name": agent.name,
                "my_room_id": agent.room_id,
                "room_id": agent.at_room,
                "cell": list(agent.cell),
            }

    # --- 위치 변이 (전부 락 안에서 원자 처리) ---

    def enter(self, token: str | None, room_id: str, cell: Cell | None) -> dict:
        agent = self._authed(token)
        if cell is not None:
            _validate_cell(cell)
        with self._lock:
            if room_id not in self._rooms:
                raise errors.NotFound(f"room '{room_id}' not found")
            target = cell if cell is not None else self._free_cell_locked(room_id)
            if self._occupied_locked(room_id, target, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({target[0]},{target[1]}) 이미 점유됨")
            # 이전 방 자동 퇴장 = at_room/cell 원자 교체
            agent.at_room = room_id
            agent.cell = target
        return self.me(token)

    def move(self, token: str | None, cell: Cell) -> dict:
        agent = self._authed(token)
        _validate_cell(cell)
        with self._lock:
            if self._occupied_locked(agent.at_room, cell, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({cell[0]},{cell[1]}) 이미 점유됨")
            agent.cell = cell
        return self.me(token)

    # --- 방 디자인 (주인만) ---

    def set_design(self, token: str | None, room_id: str, design: dict) -> dict:
        agent = self._authed(token)
        with self._lock:
            room = self._rooms.get(room_id)
            if room is None:
                raise errors.NotFound(f"room '{room_id}' not found")
            if room.owner_agent_id != agent.agent_id:
                raise errors.Forbidden("방 주인만 디자인을 바꿀 수 있음")
            objects: list[RoomObject] = []
            for o in design.get("objects", []):
                cell = (int(o["cell"][0]), int(o["cell"][1]))
                _validate_cell(cell)
                # 에이전트가 서 있는 셀에는 가구를 못 놓는다
                for a in self._agents.values():
                    if a.at_room == room_id and a.cell == cell:
                        raise CellTaken(f"셀 ({cell[0]},{cell[1]})에 에이전트가 있음")
                objects.append(RoomObject(kind=str(o["kind"]), cell=cell))
            room.design = RoomDesign(
                wallpaper=str(design.get("wallpaper", room.design.wallpaper)),
                floor=str(design.get("floor", room.design.floor)),
                objects=objects,
            )
        return self.room_state(room_id)

    # --- 내부 (호출자가 락 보유) ---

    def _occupied_locked(self, room_id: str, cell: Cell, except_agent: str) -> bool:
        for a in self._agents.values():
            if a.agent_id != except_agent and a.at_room == room_id and a.cell == cell:
                return True
        room = self._rooms.get(room_id)
        if room and any(o.cell == cell for o in room.design.objects):
            return True
        return False

    def _free_cell_locked(self, room_id: str) -> Cell:
        """자율 입장용 빈 셀 배정 — 아래줄부터 훑어 첫 빈 셀."""
        for y in range(GRID_H - 1, -1, -1):
            for x in range(GRID_W):
                if not self._occupied_locked(room_id, (x, y), except_agent=""):
                    return (x, y)
        raise CellTaken("방이 가득 참")
