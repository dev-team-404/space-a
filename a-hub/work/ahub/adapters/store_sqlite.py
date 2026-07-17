"""SQLite 영속 Store 어댑터.

stdlib `sqlite3`만 쓴다(추가 의존성 없음). 파일 경로를 주면 영속되고,
`:memory:`면 프로세스 수명 동안만 유지된다. 포트&어댑터라 core는 그대로다.

`check_same_thread=False`로 하나의 연결을 여러 스레드가 공유하므로(FastAPI 스레드풀),
**읽기·쓰기 모두 `self._lock`으로 직렬화**한다 (`_execute`/`_write`).
id는 접두어별 순번(agt_1, page_1, ...)으로 발급해 InMemoryStore와 동작이 같다.
"""

import json
import sqlite3
import threading

from ..core.models import Agent, Issue, Page, ReuseEvent, Space
from ..core.ports import Store

_SCHEMA = """
CREATE TABLE IF NOT EXISTS seq (prefix TEXT PRIMARY KEY, n INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS spaces (
    id TEXT PRIMARY KEY, name TEXT, status TEXT,
    purpose TEXT, guidelines TEXT, guide_page_id TEXT
);
CREATE TABLE IF NOT EXISTS agents (id TEXT PRIMARY KEY, name TEXT, spaces TEXT);
CREATE TABLE IF NOT EXISTS tokens (token TEXT PRIMARY KEY, agent_id TEXT);
CREATE TABLE IF NOT EXISTS issues (
    id TEXT PRIMARY KEY, space_id TEXT, title TEXT, status TEXT, opened_by TEXT
);
CREATE TABLE IF NOT EXISTS pages (
    id TEXT PRIMARY KEY, space_id TEXT, title TEXT, body TEXT, source TEXT,
    parent_id TEXT, status TEXT, visibility TEXT, issue_id TEXT,
    steps TEXT, superseded_by TEXT, flags INTEGER, created_by TEXT
);
CREATE TABLE IF NOT EXISTS reuse_events (
    id TEXT PRIMARY KEY, issue_id TEXT, page_id TEXT, agent_id TEXT, cross_team INTEGER
);
"""


class SqliteStore(Store):
    def __init__(self, path: str = "ahub.db") -> None:
        self._conn = sqlite3.connect(path, check_same_thread=False)
        self._conn.row_factory = sqlite3.Row
        self._lock = threading.Lock()  # 공유 연결 → 모든 접근 직렬화
        with self._lock:
            self._conn.executescript(_SCHEMA)
            self._migrate()
            self._conn.commit()

    def _migrate(self) -> None:
        """기존 파일 하위호환: CREATE TABLE IF NOT EXISTS는 컬럼을 추가하지 못하므로,
        나중에 도입된 컬럼을 idempotent하게 채운다 (이미 있으면 무시)."""
        cols = {r["name"] for r in self._conn.execute("PRAGMA table_info(pages)")}
        if "created_by" not in cols:
            self._conn.execute("ALTER TABLE pages ADD COLUMN created_by TEXT")

    def _execute(self, sql: str, params: tuple = ()) -> list[sqlite3.Row]:
        with self._lock:
            return self._conn.execute(sql, params).fetchall()

    def _write(self, sql: str, params: tuple) -> None:
        with self._lock:
            self._conn.execute(sql, params)
            self._conn.commit()

    # --- id / token ---

    def new_id(self, prefix: str) -> str:
        with self._lock:
            self._conn.execute(
                "INSERT INTO seq(prefix, n) VALUES(?, 0) ON CONFLICT(prefix) DO NOTHING",
                (prefix,),
            )
            self._conn.execute("UPDATE seq SET n = n + 1 WHERE prefix = ?", (prefix,))
            n = self._conn.execute(
                "SELECT n FROM seq WHERE prefix = ?", (prefix,)
            ).fetchone()[0]
            self._conn.commit()
        return f"{prefix}_{n}"

    def new_token(self) -> str:
        return self.new_id("tok")

    # --- spaces ---

    def add_space(self, space: Space) -> None:
        self._write(
            "INSERT OR REPLACE INTO spaces(id,name,status,purpose,guidelines,guide_page_id)"
            " VALUES(?,?,?,?,?,?)",
            (space.id, space.name, space.status, space.purpose, space.guidelines, space.guide_page_id),
        )

    def get_space(self, space_id: str) -> Space | None:
        rows = self._execute("SELECT * FROM spaces WHERE id=?", (space_id,))
        return self._space(rows[0]) if rows else None

    def all_spaces(self) -> list[Space]:
        return [self._space(r) for r in self._execute("SELECT * FROM spaces")]

    # --- agents / tokens ---

    def add_agent(self, agent: Agent) -> None:
        self._write(
            "INSERT OR REPLACE INTO agents(id,name,spaces) VALUES(?,?,?)",
            (agent.id, agent.name, json.dumps(agent.spaces)),
        )

    def bind_token(self, token: str, agent_id: str) -> None:
        self._write("INSERT OR REPLACE INTO tokens(token,agent_id) VALUES(?,?)", (token, agent_id))

    def agent_for_token(self, token: str) -> Agent | None:
        rows = self._execute("SELECT agent_id FROM tokens WHERE token=?", (token,))
        return self.get_agent(rows[0]["agent_id"]) if rows else None

    def get_agent(self, agent_id: str) -> Agent | None:
        rows = self._execute("SELECT * FROM agents WHERE id=?", (agent_id,))
        return self._agent(rows[0]) if rows else None

    def save_agent(self, agent: Agent) -> None:
        self.add_agent(agent)

    def all_agents(self) -> list[Agent]:
        return [self._agent(r) for r in self._execute("SELECT * FROM agents")]

    def revoke_tokens(self, agent_id: str) -> None:
        self._write("DELETE FROM tokens WHERE agent_id=?", (agent_id,))

    # --- issues ---

    def add_issue(self, issue: Issue) -> None:
        self._write(
            "INSERT OR REPLACE INTO issues(id,space_id,title,status,opened_by) VALUES(?,?,?,?,?)",
            (issue.id, issue.space_id, issue.title, issue.status, issue.opened_by),
        )

    def get_issue(self, issue_id: str) -> Issue | None:
        rows = self._execute("SELECT * FROM issues WHERE id=?", (issue_id,))
        return self._issue(rows[0]) if rows else None

    def save_issue(self, issue: Issue) -> None:
        self.add_issue(issue)

    def all_issues(self) -> list[Issue]:
        return [self._issue(r) for r in self._execute("SELECT * FROM issues")]

    # --- pages ---

    def add_page(self, page: Page) -> None:
        self._write(
            "INSERT OR REPLACE INTO pages"
            "(id,space_id,title,body,source,parent_id,status,visibility,issue_id,steps,superseded_by,flags,created_by)"
            " VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                page.id, page.space_id, page.title, page.body, page.source, page.parent_id,
                page.status, page.visibility, page.issue_id, json.dumps(page.steps),
                page.superseded_by, page.flags, page.created_by,
            ),
        )

    def get_page(self, page_id: str) -> Page | None:
        rows = self._execute("SELECT * FROM pages WHERE id=?", (page_id,))
        return self._page(rows[0]) if rows else None

    def save_page(self, page: Page) -> None:
        self.add_page(page)

    def all_pages(self) -> list[Page]:
        return [self._page(r) for r in self._execute("SELECT * FROM pages")]

    def pages_in_space(self, space_id: str) -> list[Page]:
        return [
            self._page(r)
            for r in self._execute("SELECT * FROM pages WHERE space_id=?", (space_id,))
        ]

    # --- reuse events ---

    def add_reuse_event(self, event: ReuseEvent) -> None:
        self._write(
            "INSERT OR REPLACE INTO reuse_events(id,issue_id,page_id,agent_id,cross_team)"
            " VALUES(?,?,?,?,?)",
            (event.id, event.issue_id, event.page_id, event.agent_id, int(event.cross_team)),
        )

    # --- row → model ---

    @staticmethod
    def _space(r: sqlite3.Row) -> Space:
        return Space(
            id=r["id"], name=r["name"], status=r["status"],
            purpose=r["purpose"] or "", guidelines=r["guidelines"] or "",
            guide_page_id=r["guide_page_id"],
        )

    @staticmethod
    def _agent(r: sqlite3.Row) -> Agent:
        return Agent(id=r["id"], name=r["name"], spaces=json.loads(r["spaces"]))

    @staticmethod
    def _issue(r: sqlite3.Row) -> Issue:
        return Issue(
            id=r["id"], space_id=r["space_id"], title=r["title"],
            status=r["status"], opened_by=r["opened_by"],
        )

    @staticmethod
    def _page(r: sqlite3.Row) -> Page:
        return Page(
            id=r["id"], space_id=r["space_id"], title=r["title"], body=r["body"],
            source=r["source"], parent_id=r["parent_id"], status=r["status"],
            visibility=r["visibility"], issue_id=r["issue_id"],
            steps=json.loads(r["steps"]), superseded_by=r["superseded_by"], flags=r["flags"],
            created_by=r["created_by"],
        )
