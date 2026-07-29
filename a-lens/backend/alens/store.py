"""번역 캐시 영속화 — SQLite (파생 캐시라 정본 아님, 언제든 재생성 가능).

a-hub Page를 분류·요약·서사로 번역한 결과를 page_id로 저장한다. `updated_at`으로
증분 판정 — a-hub에서 **바뀐/새 문서만 LLM에 태우기** 위한 게이트다.

교체 지점: 규모가 정말 커지면 이 store만 Postgres 등으로 갈아끼운다(collector는 안 바뀜).
현재 워크로드는 단일 프로세스·단일 writer·read-mostly라 SQLite(WAL)가 최적 —
설계 근거는 docs/design/a-lens/05-narrative-summarization.md §4.

DB 경로는 settings(`db_path`)에서 읽는다 — 설정 창에서 비우면 캐시 비활성(get_store()가 None).
"""

import sqlite3
import threading
from pathlib import Path

from . import settings

_SCHEMA = """
CREATE TABLE IF NOT EXISTS knowledge_translation (
  page_id       TEXT PRIMARY KEY,
  space_id      TEXT,
  updated_at    TEXT,   -- 번역 당시 a-hub Page.updated_at (증분 판정 키)
  source_hash   TEXT,   -- 본문 sha256 (무결성 확인용)
  body          TEXT,   -- 원문 본문 (허브 재조회 스킵용 — updated_at 그대로면 재fetch 안 함)
  visibility    TEXT,
  author_agent  TEXT,
  author_agent_id TEXT, -- 작성자 원본 agent_id — 사람별 작업 기록을 묶는 키(표시 이름과 분리)
  title         TEXT,   -- LLM이 생성한 명사형 제목 (원문 제목이 길거나 깨졌을 때 대체 표시)
  category      TEXT,   -- ① 분류
  summary       TEXT,   -- ② 요약
  narrative     TEXT,   -- ③ 서사
  model         TEXT,
  translated_at TEXT
);
"""

# 기존 DB(구 스키마)에 추가된 컬럼 — 있으면 건너뛰고 없으면 ALTER로 붙인다.
_ADDED_COLUMNS = ("body", "visibility", "author_agent", "author_agent_id", "title")
_ISSUE_ADDED_COLUMNS = ("gen_title",)

# 이슈 번역 캐시 — 제목(title)이 번역 대상. 상태(open→resolved)가 바뀌어도 제목 그대로면 재번역 안 함.
_ISSUE_SCHEMA = """
CREATE TABLE IF NOT EXISTS issue_translation (
  issue_id      TEXT PRIMARY KEY,
  title         TEXT,   -- 원문 이슈 제목 (재번역 판정 키)
  gen_title     TEXT,   -- LLM이 생성한 명사형 제목 (표시용)
  category      TEXT,
  summary       TEXT,
  narrative     TEXT,
  model         TEXT,
  translated_at TEXT
);
"""


class TranslationStore:
    """스레드 안전한 얇은 SQLite 래퍼. 호출량이 낮아 연결은 호출마다 연다(WAL이라 안전)."""

    def __init__(self, path: str):
        self._path = path
        self._lock = threading.Lock()
        Path(path).expanduser().parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as c:
            c.executescript(_SCHEMA)
            c.executescript(_ISSUE_SCHEMA)
            self._migrate(c)

    @staticmethod
    def _migrate(c: sqlite3.Connection) -> None:
        def add(table: str, columns: tuple) -> None:
            cols = {r["name"] for r in c.execute(f"PRAGMA table_info({table})")}
            for col in columns:
                if col not in cols:
                    c.execute(f"ALTER TABLE {table} ADD COLUMN {col} TEXT")

        add("knowledge_translation", _ADDED_COLUMNS)
        add("issue_translation", _ISSUE_ADDED_COLUMNS)

    def _connect(self) -> sqlite3.Connection:
        c = sqlite3.connect(self._path, timeout=5)
        c.execute("PRAGMA journal_mode=WAL")     # 읽기(웹 응답)와 쓰기(번역)가 안 부딪히게
        c.execute("PRAGMA synchronous=NORMAL")
        c.row_factory = sqlite3.Row
        return c

    def get(self, page_id: str) -> dict | None:
        with self._connect() as c:
            r = c.execute(
                "SELECT * FROM knowledge_translation WHERE page_id=?", (page_id,)
            ).fetchone()
            return dict(r) if r else None

    def upsert(self, row: dict) -> None:
        with self._lock, self._connect() as c:
            c.execute(
                """
                INSERT INTO knowledge_translation
                  (page_id, space_id, updated_at, source_hash, body, visibility,
                   author_agent, author_agent_id, title, category, summary, narrative,
                   model, translated_at)
                VALUES
                  (:page_id, :space_id, :updated_at, :source_hash, :body, :visibility,
                   :author_agent, :author_agent_id, :title, :category, :summary, :narrative,
                   :model, :translated_at)
                ON CONFLICT(page_id) DO UPDATE SET
                  space_id=excluded.space_id, updated_at=excluded.updated_at,
                  source_hash=excluded.source_hash, body=excluded.body,
                  visibility=excluded.visibility, author_agent=excluded.author_agent,
                  author_agent_id=excluded.author_agent_id,
                  title=excluded.title, category=excluded.category, summary=excluded.summary,
                  narrative=excluded.narrative, model=excluded.model,
                  translated_at=excluded.translated_at
                """,
                row,
            )

    def clear_translations(self) -> None:
        """모든 번역 캐시 삭제 — 요약 스타일 변경 등으로 전체 재번역이 필요할 때."""
        with self._lock, self._connect() as c:
            c.execute("DELETE FROM knowledge_translation")
            c.execute("DELETE FROM issue_translation")

    def get_issue(self, issue_id: str) -> dict | None:
        with self._connect() as c:
            r = c.execute(
                "SELECT * FROM issue_translation WHERE issue_id=?", (issue_id,)
            ).fetchone()
            return dict(r) if r else None

    def upsert_issue(self, row: dict) -> None:
        with self._lock, self._connect() as c:
            c.execute(
                """
                INSERT INTO issue_translation
                  (issue_id, title, gen_title, category, summary, narrative, model, translated_at)
                VALUES
                  (:issue_id, :title, :gen_title, :category, :summary, :narrative, :model, :translated_at)
                ON CONFLICT(issue_id) DO UPDATE SET
                  title=excluded.title, gen_title=excluded.gen_title, category=excluded.category,
                  summary=excluded.summary, narrative=excluded.narrative, model=excluded.model,
                  translated_at=excluded.translated_at
                """,
                row,
            )


_store: TranslationStore | None = None
_store_path: str | None = None
_init_lock = threading.Lock()


def get_store() -> TranslationStore | None:
    """settings.db_path가 있으면 싱글턴 store, 비면 None(캐시 비활성).

    경로가 설정 창에서 바뀌면 새 경로로 재생성한다."""
    global _store, _store_path
    path = (settings.get().get("db_path") or "").strip()
    with _init_lock:
        if not path:
            _store, _store_path = None, None
        elif _store is None or path != _store_path:
            _store = TranslationStore(path)
            _store_path = path
    return _store
