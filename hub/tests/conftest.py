import pytest

from space_a.adapters.store_memory import InMemoryStore
from space_a.core.services import SpaceAService


@pytest.fixture
def service() -> SpaceAService:
    return SpaceAService(InMemoryStore())
