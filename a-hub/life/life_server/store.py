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
  mascot_seed TEXT NOT NULL DEFAULT '',
  org         TEXT NOT NULL DEFAULT '',
  agent_uuid  TEXT NOT NULL DEFAULT '',
  bubble      TEXT NOT NULL DEFAULT '',
  connected   INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS tokens (
  token    TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS friends (
  owner_agent_id  TEXT NOT NULL,
  friend_agent_id TEXT NOT NULL,
  PRIMARY KEY (owner_agent_id, friend_agent_id)
);
CREATE TABLE IF NOT EXISTS content_visibility (
  owner_agent_id TEXT NOT NULL,
  feature TEXT NOT NULL,
  visibility TEXT NOT NULL,
  PRIMARY KEY (owner_agent_id, feature)
);
CREATE TABLE IF NOT EXISTS mascot_images (
  agent_id    TEXT PRIMARY KEY,
  png         BLOB NOT NULL,
  sha256      TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS shared_diaries (
  life_id    TEXT NOT NULL,
  diary_date TEXT NOT NULL,
  body       TEXT NOT NULL,
  visibility TEXT NOT NULL,
  PRIMARY KEY (life_id, diary_date)
);
CREATE TABLE IF NOT EXISTS guestbook (
  entry_id        TEXT PRIMARY KEY,
  life_id         TEXT NOT NULL,
  author_agent_id TEXT NOT NULL,
  author_name     TEXT NOT NULL,
  body            TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  parent_id       TEXT,
  author_kind     TEXT
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
        agent_columns = {row[1] for row in self._conn.execute("PRAGMA table_info(agents)")}
        if "bubble" not in agent_columns:
            self._conn.execute("ALTER TABLE agents ADD COLUMN bubble TEXT NOT NULL DEFAULT ''")
        if "connected" not in agent_columns:
            self._conn.execute("ALTER TABLE agents ADD COLUMN connected INTEGER NOT NULL DEFAULT 1")
        if "org" not in agent_columns:
            self._conn.execute("ALTER TABLE agents ADD COLUMN org TEXT NOT NULL DEFAULT ''")
        if "agent_uuid" not in agent_columns:
            self._conn.execute("ALTER TABLE agents ADD COLUMN agent_uuid TEXT NOT NULL DEFAULT ''")
        guestbook_columns = {row[1] for row in self._conn.execute("PRAGMA table_info(guestbook)")}
        if "parent_id" not in guestbook_columns:
            self._conn.execute("ALTER TABLE guestbook ADD COLUMN parent_id TEXT")
        if "author_kind" not in guestbook_columns:
            self._conn.execute("ALTER TABLE guestbook ADD COLUMN author_kind TEXT")
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
                cell=(x, y), mascot_seed=mascot_seed, org=org, agent_uuid=agent_uuid,
                bubble=bubble, connected=bool(connected),
            )
            for agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid, bubble, connected in c.execute(
                "SELECT agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid, bubble, connected FROM agents"
            )
        }
        tokens = dict(c.execute("SELECT token, agent_id FROM tokens"))
        return life, agents, tokens

    def load_social(self) -> tuple[set[tuple[str, str]], dict[tuple[str, str], str], dict[tuple[str, str], dict], list[dict]]:
        friends = set(self._conn.execute("SELECT owner_agent_id, friend_agent_id FROM friends"))
        visibility = {(owner, feature): value for owner, feature, value in self._conn.execute(
            "SELECT owner_agent_id, feature, visibility FROM content_visibility"
        )}
        diaries = {
            (life_id, diary_date): {"date": diary_date, "body": body, "visibility": visibility}
            for life_id, diary_date, body, visibility in self._conn.execute(
                "SELECT life_id, diary_date, body, visibility FROM shared_diaries"
            )
        }
        guestbook = [
            {"entry_id": entry_id, "life_id": life_id, "author_agent_id": author_id,
             "author_name": author_name, "body": body, "parent_id": parent_id,
             "author_kind": author_kind, "created_at": created_at}
            for entry_id, life_id, author_id, author_name, body, parent_id, author_kind, created_at in self._conn.execute(
                "SELECT entry_id, life_id, author_agent_id, author_name, body, parent_id, author_kind, created_at FROM guestbook"
            )
        ]
        return friends, visibility, diaries, guestbook

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

    def save_token(self, token: str, agent_id: str) -> None:
        self._conn.execute("INSERT INTO tokens VALUES (?, ?)", (token, agent_id))
        self._conn.commit()

    def set_friend(self, owner_agent_id: str, friend_agent_id: str, enabled: bool) -> None:
        if enabled:
            self._conn.execute("INSERT OR IGNORE INTO friends VALUES (?, ?)", (owner_agent_id, friend_agent_id))
        else:
            self._conn.execute("DELETE FROM friends WHERE owner_agent_id = ? AND friend_agent_id = ?", (owner_agent_id, friend_agent_id))
        self._conn.commit()

    def set_content_visibility(self, owner_agent_id: str, feature: str, visibility: str) -> None:
        self._conn.execute(
            "INSERT INTO content_visibility VALUES (?, ?, ?) ON CONFLICT(owner_agent_id, feature) "
            "DO UPDATE SET visibility = excluded.visibility",
            (owner_agent_id, feature, visibility),
        )
        self._conn.commit()

    def save_mascot_image(self, agent_id: str, png: bytes, sha256: str, updated_at: str) -> None:
        self._conn.execute(
            "INSERT INTO mascot_images VALUES (?, ?, ?, ?) ON CONFLICT(agent_id) DO UPDATE SET "
            "png = excluded.png, sha256 = excluded.sha256, updated_at = excluded.updated_at",
            (agent_id, png, sha256, updated_at),
        )
        self._conn.commit()

    def mascot_image(self, agent_id: str) -> tuple[bytes, str] | None:
        row = self._conn.execute("SELECT png, sha256 FROM mascot_images WHERE agent_id = ?", (agent_id,)).fetchone()
        return (bytes(row[0]), row[1]) if row else None

    def mascot_image_hash(self, agent_id: str) -> str | None:
        row = self._conn.execute("SELECT sha256 FROM mascot_images WHERE agent_id = ?", (agent_id,)).fetchone()
        return row[0] if row else None

    def load_mascot_image_hashes(self) -> dict[str, str]:
        return dict(self._conn.execute("SELECT agent_id, sha256 FROM mascot_images"))

    def save_shared_diary(self, life_id: str, date: str, body: str, visibility: str) -> None:
        self._conn.execute(
            "INSERT INTO shared_diaries VALUES (?, ?, ?, ?) ON CONFLICT(life_id, diary_date) "
            "DO UPDATE SET body = excluded.body, visibility = excluded.visibility",
            (life_id, date, body, visibility),
        )
        self._conn.commit()

    def delete_shared_diary(self, life_id: str, date: str) -> None:
        self._conn.execute("DELETE FROM shared_diaries WHERE life_id = ? AND diary_date = ?", (life_id, date))
        self._conn.commit()

    def clear_shared_diaries(self, life_id: str) -> None:
        self._conn.execute("DELETE FROM shared_diaries WHERE life_id = ?", (life_id,))
        self._conn.commit()

    def save_guestbook_entry(self, entry: dict) -> None:
        self._conn.execute(
            "INSERT INTO guestbook (entry_id, life_id, author_agent_id, author_name, body, parent_id, author_kind, created_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (entry["entry_id"], entry["life_id"], entry["author_agent_id"], entry["author_name"],
             entry["body"], entry.get("parent_id"), entry.get("author_kind"), entry["created_at"]),
        )
        self._conn.commit()

    def delete_guestbook_entry(self, entry_id: str) -> None:
        # G2(ADR 0021): 원글 삭제 시 답글도 cascade — 1-depth라 재귀 불필요
        self._conn.execute("DELETE FROM guestbook WHERE entry_id = ? OR parent_id = ?",
                           (entry_id, entry_id))
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
            "INSERT INTO agents (agent_id, name, life_id, at_life, x, y, mascot_seed, org, agent_uuid, bubble, connected) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(agent_id) DO UPDATE SET "
            "name = excluded.name, at_life = excluded.at_life, x = excluded.x, y = excluded.y, "
            "mascot_seed = excluded.mascot_seed, org = excluded.org, agent_uuid = excluded.agent_uuid, "
            "bubble = excluded.bubble, connected = excluded.connected",
            (
                agent.agent_id, agent.name, agent.life_id, agent.at_life,
                agent.cell[0], agent.cell[1], agent.mascot_seed, agent.org, agent.agent_uuid,
                agent.bubble, int(agent.connected),
            ),
        )
