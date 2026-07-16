"""SQLite 영속화 — `ROOM_SERVER_DB` 설정 시 등록(토큰)·방·위치·디자인이 재시작을 견딘다.

모든 호출은 RoomService의 전역 락 안에서 일어난다(단일 프로세스). DB는 내구성만
담당하고, 원자성·겹침 금지는 여전히 서비스 락이 보장한다 — hub의 SPACE_A_DB와
같은 opt-in 패턴(미설정이면 인메모리 그대로).
"""

import os
import sqlite3

from .rooms import Room, RoomAgent, RoomDesign, RoomObject

_SCHEMA = """
CREATE TABLE IF NOT EXISTS rooms (
  room_id        TEXT PRIMARY KEY,
  owner_agent_id TEXT NOT NULL,
  owner_name     TEXT NOT NULL,
  wallpaper      TEXT NOT NULL,
  floor          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS room_objects (
  room_id TEXT NOT NULL,
  kind    TEXT NOT NULL,
  x       INTEGER NOT NULL,
  y       INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS agents (
  agent_id    TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  room_id     TEXT NOT NULL,
  at_room     TEXT NOT NULL,
  x           INTEGER NOT NULL,
  y           INTEGER NOT NULL,
  mascot_seed TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS tokens (
  token    TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL
);
"""


class SqliteStore:
    def __init__(self, path: str) -> None:
        # check_same_thread=False: uvicorn 워커 스레드에서 호출되지만
        # 접근은 전부 RoomService 락 직렬화 하에 있다.
        self._conn = sqlite3.connect(path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.executescript(_SCHEMA)
        self._conn.commit()

    @classmethod
    def from_env(cls) -> "SqliteStore | None":
        path = os.environ.get("ROOM_SERVER_DB", "").strip()
        return cls(path) if path else None

    # --- 시작 시 전체 로드 ---

    def load(self) -> tuple[dict[str, Room], dict[str, RoomAgent], dict[str, str]]:
        c = self._conn
        objects: dict[str, list[RoomObject]] = {}
        for room_id, kind, x, y in c.execute("SELECT room_id, kind, x, y FROM room_objects"):
            objects.setdefault(room_id, []).append(RoomObject(kind=kind, cell=(x, y)))
        rooms = {
            room_id: Room(
                id=room_id,
                owner_agent_id=owner_agent_id,
                owner_name=owner_name,
                design=RoomDesign(wallpaper=wallpaper, floor=floor, objects=objects.get(room_id, [])),
            )
            for room_id, owner_agent_id, owner_name, wallpaper, floor in c.execute(
                "SELECT room_id, owner_agent_id, owner_name, wallpaper, floor FROM rooms"
            )
        }
        agents = {
            agent_id: RoomAgent(
                agent_id=agent_id, name=name, room_id=room_id, at_room=at_room,
                cell=(x, y), mascot_seed=mascot_seed,
            )
            for agent_id, name, room_id, at_room, x, y, mascot_seed in c.execute(
                "SELECT agent_id, name, room_id, at_room, x, y, mascot_seed FROM agents"
            )
        }
        tokens = dict(c.execute("SELECT token, agent_id FROM tokens"))
        return rooms, agents, tokens

    # --- 변이별 반영 (호출자 = RoomService, 락 보유) ---

    def save_registration(self, agent: RoomAgent, token: str, room: Room) -> None:
        self._conn.execute(
            "INSERT INTO rooms VALUES (?, ?, ?, ?, ?)",
            (room.id, room.owner_agent_id, room.owner_name, room.design.wallpaper, room.design.floor),
        )
        self._save_agent_row(agent)
        self._conn.execute("INSERT INTO tokens VALUES (?, ?)", (token, agent.agent_id))
        self._conn.commit()

    def save_agent(self, agent: RoomAgent) -> None:
        self._save_agent_row(agent)
        self._conn.commit()

    def save_owner_name(self, room: Room) -> None:
        self._conn.execute(
            "UPDATE rooms SET owner_name = ? WHERE room_id = ?", (room.owner_name, room.id)
        )
        self._conn.commit()

    def save_design(self, room: Room) -> None:
        self._conn.execute(
            "UPDATE rooms SET wallpaper = ?, floor = ? WHERE room_id = ?",
            (room.design.wallpaper, room.design.floor, room.id),
        )
        self._conn.execute("DELETE FROM room_objects WHERE room_id = ?", (room.id,))
        self._conn.executemany(
            "INSERT INTO room_objects VALUES (?, ?, ?, ?)",
            [(room.id, o.kind, o.cell[0], o.cell[1]) for o in room.design.objects],
        )
        self._conn.commit()

    def _save_agent_row(self, agent: RoomAgent) -> None:
        self._conn.execute(
            "INSERT INTO agents VALUES (?, ?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(agent_id) DO UPDATE SET "
            "name = excluded.name, at_room = excluded.at_room, x = excluded.x, y = excluded.y",
            (
                agent.agent_id, agent.name, agent.room_id, agent.at_room,
                agent.cell[0], agent.cell[1], agent.mascot_seed,
            ),
        )
