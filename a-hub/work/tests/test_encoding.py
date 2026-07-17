"""한글(멀티바이트) 왕복 및 UTF-8 charset 명시 검증.

배포본(API Gateway HTTP API + Mangum)에서 한글 title/body가 mojibake(占…)로
깨지던 이슈의 회귀 방지. 근본 원인은 응답 Content-Type에 charset=utf-8이 없어
CP949 기본 클라이언트(한국 Windows)가 UTF-8 바이트를 CP949로 오해석한 것.
"""

from fastapi.testclient import TestClient

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.rest_server import create_app
from ahub.core.services import SpaceAService

KOREAN = "한글 제목 조회"


def _client() -> TestClient:
    svc = SpaceAService(InMemoryStore())
    svc.create_space("s1", "Space One", purpose="p")
    return TestClient(create_app(svc, mount_mcp=False))


def _token(c: TestClient) -> str:
    return c.post("/agents/register", json={"name": "a1", "space_id": "s1"}).json()[
        "token"
    ]


def test_json_response_declares_utf8_charset():
    """모든 JSON 응답은 charset=utf-8을 명시해야 한다 (CP949 오해석 방지)."""
    c = _client()
    tok = _token(c)
    r = c.post(
        "/spaces/s1/pages",
        json={"title": KOREAN, "body": KOREAN},
        headers={"authorization": f"Bearer {tok}"},
    )
    ctype = r.headers.get("content-type", "")
    assert "charset=utf-8" in ctype.lower(), ctype


def test_get_response_declares_utf8_charset():
    c = _client()
    tok = _token(c)
    pid = c.post(
        "/spaces/s1/pages",
        json={"title": KOREAN, "body": KOREAN},
        headers={"authorization": f"Bearer {tok}"},
    ).json()["page_id"]
    r = c.get(f"/pages/{pid}", headers={"authorization": f"Bearer {tok}"})
    assert "charset=utf-8" in r.headers.get("content-type", "").lower()


def test_error_response_declares_utf8_charset():
    """도메인 에러(JSONResponse)도 charset=utf-8을 명시해야 한다."""
    c = _client()
    # 인증 없이 호출 → 401 JSONResponse
    r = c.get("/pages/nope", headers={"authorization": "Bearer bad"})
    assert r.status_code >= 400
    assert "charset=utf-8" in r.headers.get("content-type", "").lower()


def test_request_body_decoded_as_utf8_regardless_of_charset_label():
    """UTF-8 바이트 요청은 Content-Type charset 라벨과 무관하게 UTF-8로 읽힌다.

    잘못된 charset(예: cp949)을 붙여 보내도 서버는 바디를 UTF-8로 파싱한다
    (요청 바디 디코딩 강건성). 이는 배포 경로에서 클라이언트가 charset을
    엉뚱하게 붙여도 한글이 보존됨을 보장한다.
    """
    c = _client()
    tok = _token(c)
    body = ('{"title": "%s", "body": "%s"}' % (KOREAN, KOREAN)).encode("utf-8")
    r = c.post(
        "/spaces/s1/pages",
        content=body,
        headers={
            "authorization": f"Bearer {tok}",
            "content-type": "application/json; charset=cp949",
        },
    )
    assert r.status_code == 201, r.text
    pid = r.json()["page_id"]
    got = c.get(f"/pages/{pid}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["title"] == KOREAN
    assert got["body"] == KOREAN


def test_korean_roundtrips_through_page():
    """한글 title/body가 저장→조회에서 바이트 단위로 보존된다."""
    c = _client()
    tok = _token(c)
    pid = c.post(
        "/spaces/s1/pages",
        json={"title": KOREAN, "body": KOREAN},
        headers={"authorization": f"Bearer {tok}"},
    ).json()["page_id"]
    got = c.get(f"/pages/{pid}", headers={"authorization": f"Bearer {tok}"}).json()
    assert got["title"] == KOREAN
    assert got["body"] == KOREAN
