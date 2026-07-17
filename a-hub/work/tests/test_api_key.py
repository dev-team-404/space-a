"""x-api-key 전역 관문 — 고정 공유키를 미들웨어에서 검사한다.

SPACE_A_API_KEY 환경변수가 설정된 배포에서만 활성. 미설정이면 검사 비활성
(로컬·테스트). /healthz·/readyz는 항상 면제.
"""

from fastapi.testclient import TestClient

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.rest_server import create_app
from ahub.core.services import SpaceAService

KEY = "secret-shared-key"


def _client() -> TestClient:
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "S", purpose="p")
    return TestClient(create_app(svc, mount_mcp=False))


def test_no_env_means_check_disabled(monkeypatch):
    monkeypatch.delenv("SPACE_A_API_KEY", raising=False)
    c = _client()
    assert c.get("/spaces").status_code == 200  # 키 없이 통과


def test_correct_key_passes(monkeypatch):
    monkeypatch.setenv("SPACE_A_API_KEY", KEY)
    c = _client()
    r = c.get("/spaces", headers={"x-api-key": KEY})
    assert r.status_code == 200


def test_missing_key_rejected(monkeypatch):
    monkeypatch.setenv("SPACE_A_API_KEY", KEY)
    c = _client()
    r = c.get("/spaces")
    assert r.status_code == 401
    assert r.json()["error"]["code"] == "unauthorized"


def test_wrong_key_rejected(monkeypatch):
    monkeypatch.setenv("SPACE_A_API_KEY", KEY)
    c = _client()
    r = c.get("/spaces", headers={"x-api-key": "nope"})
    assert r.status_code == 401


def test_healthz_readyz_exempt_even_with_key_set(monkeypatch):
    monkeypatch.setenv("SPACE_A_API_KEY", KEY)
    c = _client()
    assert c.get("/healthz").status_code == 200
    assert c.get("/readyz").status_code == 200


def test_public_endpoints_still_require_key(monkeypatch):
    """키가 설정되면 인증 없는 공개 엔드포인트(discovery)도 x-api-key를 요구한다."""
    monkeypatch.setenv("SPACE_A_API_KEY", KEY)
    c = _client()
    assert c.get("/").status_code == 401
    assert c.get("/", headers={"x-api-key": KEY}).status_code == 200
