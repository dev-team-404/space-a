import pytest

from space_a.adapters.store_memory import InMemoryStore
from space_a.adapters.store_sqlite import SqliteStore
from space_a.core.services import SpaceAService


@pytest.fixture(params=["memory", "sqlite"])
def service(request) -> SpaceAService:
    """모든 서비스 테스트를 두 스토어에 대해 돌린다 — 동작 동일성 증명."""
    store = InMemoryStore() if request.param == "memory" else SqliteStore(":memory:")
    return SpaceAService(store)
