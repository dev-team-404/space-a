"""스토어 선택 (환경변수 기반).

- `SPACE_A_TABLE` → DynamoDB (서버리스)
- `SPACE_A_DB`    → SQLite (파일 영속)
- 없으면          → 인메모리
"""

import os

from ..core.ports import Store
from .store_memory import InMemoryStore
from .store_sqlite import SqliteStore


def make_store() -> Store:
    if os.environ.get("SPACE_A_TABLE"):
        # boto3는 서버리스(DynamoDB)에서만 필요하므로 지연 import
        from .store_dynamodb import DynamoDBStore

        return DynamoDBStore()
    db = os.environ.get("SPACE_A_DB")
    return SqliteStore(db) if db else InMemoryStore()
