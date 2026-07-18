import pytest

from ahub.adapters.store_memory import InMemoryStore
from ahub.adapters.store_sqlite import SqliteStore
from ahub.core.services import SpaceAService

# 서버 동작을 바꾸는 환경변수들. 개발/배포용으로 셸이나 ~/.claude/settings.json에
# 설정돼 있으면 테스트로 새어들어 오작동시킨다 (예: SPACE_A_API_KEY가 있으면
# x-api-key 미들웨어가 헤더 없는 테스트 요청을 401로 막는다).
_SERVER_ENV_VARS = ("SPACE_A_API_KEY", "SPACE_A_TABLE", "SPACE_A_DB")


@pytest.fixture(autouse=True)
def _isolate_server_env(monkeypatch):
    """모든 테스트는 이 환경변수들이 없는 상태에서 돈다 (셸/전역 설정 누수 차단)."""
    for var in _SERVER_ENV_VARS:
        monkeypatch.delenv(var, raising=False)


@pytest.fixture(params=["memory", "sqlite"])
def service(request) -> SpaceAService:
    """모든 서비스 테스트를 두 스토어에 대해 돌린다 — 동작 동일성 증명."""
    store = InMemoryStore() if request.param == "memory" else SqliteStore(":memory:")
    return SpaceAService(store)
