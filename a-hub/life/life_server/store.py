"""SQLite 영속화 — `LIFE_SERVER_DB` 설정 시 등록(토큰)·방·위치·디자인이 재시작을 견딘다.

모든 호출은 LifeService의 전역 락 안에서 일어난다(단일 프로세스). DB는 내구성만
담당하고, 원자성·겹침 금지는 여전히 서비스 락이 보장한다 — hub의 SPACE_A_DB와
같은 opt-in 패턴(미설정이면 인메모리 그대로).
"""

import os
import json
import sqlite3

from .life import Life, LifeAgent, LifeDesign, LifeObject

_SCHEMA = """
CREATE TABLE IF NOT EXISTS life (
  life_id        TEXT PRIMARY KEY,
  owner_agent_id TEXT NOT NULL,
  owner_name     TEXT NOT NULL,
  wallpaper      TEXT NOT NULL,
  floor          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS life_objects (
  life_id TEXT NOT NULL,
  kind    TEXT NOT NULL,
  x       INTEGER NOT NULL,
  y       INTEGER NOT NULL,
  category TEXT NOT NULL DEFAULT 'legacy',
  w INTEGER NOT NULL DEFAULT 1,
  h INTEGER NOT NULL DEFAULT 1,
  rotation INTEGER NOT NULL DEFAULT 0,
  wall TEXT,
  footprint TEXT
);
CREATE TABLE IF NOT EXISTS agents (
  agent_id    TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  life_id     TEXT NOT NULL,
  at_life     TEXT NOT NULL,
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
        # 접근은 전부 LifeService 락 직렬화 하에 있다.
        self._conn = sqlite3.connect(path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.executescript(_SCHEMA)
        self._migrate_life_objects()
        self._conn.commit()

    def _migrate_life_objects(self) -> None:
        columns = {row[1] for row in self._conn.execute("PRAGMA table_info(life_objects)")}
        for name, ddl in [
            ("category", "TEXT NOT NULL DEFAULT 'legacy'"),
            ("w", "INTEGER NOT NULL DEFAULT 1"),
            ("h", "INTEGER NOT NULL DEFAULT 1"),
            ("rotation", "INTEGER NOT NULL DEFAULT 0"),
            ("wall", "TEXT"),
            ("footprint", "TEXT"),
        ]:
            if name not in columns:
                self._conn.execute(f"ALTER TABLE life_objects ADD COLUMN {name} {ddl}")
        # ADR 0005 intentionally drops the old combined table+chair assets. They cannot be
        # migrated without preserving the inconsistent chair counts and collision masks.
        self._conn.execute("DELETE FROM life_objects WHERE category = 'dining'")

    @classmethod
    def from_env(cls) -> "SqliteStore | None":
        path = os.environ.get("LIFE_SERVER_DB", "").strip()
        return cls(path) if path else None

    # --- 시작 시 전체 로드 ---

    def load(self) -> tuple[dict[str, Life], dict[str, LifeAgent], dict[str, str]]:
        c = self._conn
        objects: dict[str, list[LifeObject]] = {}
        for life_id, asset_id, x, y, category, w, h, rotation, wall, footprint in c.execute(
            "SELECT life_id, kind, x, y, category, w, h, rotation, wall, footprint FROM life_objects"
        ):
            objects.setdefault(life_id, []).append(
                LifeObject(
                    asset_id=asset_id,
                    category=category,
                    cell=(x, y),
                    size=(w, h),
                    rotation=rotation,
                    wall=wall,
                    footprint=tuple(tuple(cell) for cell in json.loads(footprint)) if footprint else None,
                )
            )
        life = {
            life_id: Life(
                id=life_id,
                owner_agent_id=owner_agent_id,
                owner_name=owner_name,
                design=LifeDesign(wallpaper=wallpaper, floor=floor, objects=objects.get(life_id, [])),
            )
            for life_id, owner_agent_id, owner_name, wallpaper, floor in c.execute(
                "SELECT life_id, owner_agent_id, owner_name, wallpaper, floor FROM life"
            )
        }
        agents = {
            agent_id: LifeAgent(
                agent_id=agent_id, name=name, life_id=life_id, at_life=at_life,
                cell=(x, y), mascot_seed=mascot_seed,
            )
            for agent_id, name, life_id, at_life, x, y, mascot_seed in c.execute(
                "SELECT agent_id, name, life_id, at_life, x, y, mascot_seed FROM agents"
            )
        }
        tokens = dict(c.execute("SELECT token, agent_id FROM tokens"))
        return life, agents, tokens

    # --- 변이별 반영 (호출자 = LifeService, 락 보유) ---

    def save_registration(self, agent: LifeAgent, token: str, life: Life) -> None:
        self._conn.execute(
            "INSERT INTO life VALUES (?, ?, ?, ?, ?)",
            (life.id, life.owner_agent_id, life.owner_name, life.design.wallpaper, life.design.floor),
        )
        self._save_agent_row(agent)
        self._conn.execute("INSERT INTO tokens VALUES (?, ?)", (token, agent.agent_id))
        self._conn.commit()

    def save_agent(self, agent: LifeAgent) -> None:
        self._save_agent_row(agent)
        self._conn.commit()

    def save_owner_name(self, life: Life) -> None:
        self._conn.execute(
            "UPDATE life SET owner_name = ? WHERE life_id = ?", (life.owner_name, life.id)
        )
        self._conn.commit()

    def save_design(self, life: Life) -> None:
        self._conn.execute(
            "UPDATE life SET wallpaper = ?, floor = ? WHERE life_id = ?",
            (life.design.wallpaper, life.design.floor, life.id),
        )
        self._conn.execute("DELETE FROM life_objects WHERE life_id = ?", (life.id,))
        self._conn.executemany(
            "INSERT INTO life_objects (life_id, kind, x, y, category, w, h, rotation, wall, footprint) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                (
                    life.id, o.asset_id, o.cell[0], o.cell[1], o.category,
                    o.size[0], o.size[1], o.rotation, o.wall,
                    json.dumps(o.footprint) if o.footprint else None,
                )
                for o in life.design.objects
            ],
        )
        self._conn.commit()

    def _save_agent_row(self, agent: LifeAgent) -> None:
        self._conn.execute(
            "INSERT INTO agents VALUES (?, ?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(agent_id) DO UPDATE SET "
            "name = excluded.name, at_life = excluded.at_life, x = excluded.x, y = excluded.y",
            (
                agent.agent_id, agent.name, agent.life_id, agent.at_life,
                agent.cell[0], agent.cell[1], agent.mascot_seed,
            ),
        )
