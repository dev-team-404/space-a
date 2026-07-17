"""원천 수집 — a-hub-work(C2)·a-hub-life(프레즌스) 폴링.

C2가 서버에 구현되기 전까지는 contracts/fixtures/를 원천으로 쓴다 (ADR 0003 §4).
원천이 픽스처든 실서버든 반환 모양은 같다 — pipeline은 여기를 몰라도 된다.
"""

import json
import os
from pathlib import Path

import httpx

# repo 루트 기준 contracts/fixtures (backend/alens/ → 세 단계 위)
_FIXTURES = Path(__file__).resolve().parents[3] / "contracts" / "fixtures"

WORK_URL = os.environ.get("A_LENS_WORK_URL", "https://spacea.msalt.net")
LIFE_URL = os.environ.get("A_LENS_LIFE_URL", "")  # room-server. 비면 프레즌스 생략


def _fixture(name: str) -> dict:
    return json.loads((_FIXTURES / name).read_text(encoding="utf-8"))


def fetch_spaces() -> dict:
    """C2 GET /spaces 모양. C2 미구현 동안은 픽스처."""
    return _fixture("spaces.json")


def fetch_space_detail(space_id: str, tier: str = "member") -> dict:
    """C2 GET /spaces/{id} 모양. tier별 픽스처로 대체."""
    name = "space-detail-guest.json" if tier == "guest" else "space-detail-member.json"
    detail = _fixture(name)
    detail["space_id"] = space_id
    return detail


def fetch_stats() -> dict:
    return _fixture("stats.json")


def fetch_activity() -> dict:
    return _fixture("activity.json")


def fetch_reuse_events() -> dict:
    return _fixture("reuse-events.json")


def fetch_presence(room_id: str) -> dict | None:
    """a-hub-life 프레즌스 (G8: a-lens가 직접 조회해 조인).

    하트비트·상태 필드는 #39 대기 — API가 열리면 여기만 채우면 된다.
    """
    if not LIFE_URL:
        return None
    try:
        r = httpx.get(f"{LIFE_URL.rstrip('/')}/rooms/{room_id}", timeout=5)
        r.raise_for_status()
        return r.json()
    except httpx.HTTPError:
        return None
