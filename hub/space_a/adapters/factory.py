"""스토어 선택. `SPACE_A_DB`가 설정돼 있으면 SQLite(영속), 아니면 인메모리."""

import os

from ..core.ports import Store
from .store_memory import InMemoryStore
from .store_sqlite import SqliteStore


def make_store() -> Store:
    db = os.environ.get("SPACE_A_DB")
    return SqliteStore(db) if db else InMemoryStore()
