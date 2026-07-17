import pytest

pytest.importorskip("mcp")  # seed is only used by dev tooling that pulls mcp

from ahub.api.seed import seed_demo
from ahub.core.services import SpaceAService
from ahub.adapters.store_memory import InMemoryStore


def test_seed_demo_creates_space_and_searchable_knowledge():
    service = SpaceAService(InMemoryStore())
    token = seed_demo(service)
    guide = service.get_guide("demo")
    assert guide is not None and "search_knowledge" in guide.body
    res = service.search_knowledge(token, "인증서")
    assert len(res.pages) == 1
