"""원천 수집 — a-hub-work 실서버 또는 contracts/fixtures 스냅숏.

허브 주소·토큰은 배포에 따라 바뀔 수 있으므로 전부 환경변수로 받는다:

  A_LENS_SOURCE      hub | fixtures | auto(기본) — auto는 허브 실패 시 픽스처 폴백
  A_LENS_WORK_URL    a-hub-work base URL (기본 https://spacea.msalt.net)
  A_LENS_WORK_TOKEN  허브 Bearer 토큰 (없으면 인증 필요한 상세는 비어서 내려감)
  A_LENS_WORK_API_KEY  허브 x-api-key 헤더 값 (2026-07-19 허브 인증 전환 — 비면 생략)
  A_LENS_LIFE_URL    room-server base URL (프레즌스, #39 대기 — 비면 생략)
  A_LENS_CACHE_TTL   허브 폴링 캐시 초 (기본 30)

C2가 허브에 구현되기 전까지는 허브의 현행 REST(GET /spaces · tree · issues ·
members)를 읽어 C2 비슷한 모양으로 맞춘다 — pipeline은 원천을 몰라도 된다.
원천은 스냅숏 단위로 하나만 쓴다: 실데이터와 픽스처를 섞으면 화면이 거짓말을 한다.
"""

import copy
import json
import logging
import os
import re
import time
from datetime import datetime, timezone
from pathlib import Path

import httpx

log = logging.getLogger("alens.collector")

# repo 루트 기준 contracts/fixtures (backend/alens/ → 세 단계 위)
_FIXTURES = Path(__file__).resolve().parents[3] / "contracts" / "fixtures"

SOURCE = os.environ.get("A_LENS_SOURCE", "auto")
WORK_URL = os.environ.get("A_LENS_WORK_URL", "https://spacea.msalt.net").rstrip("/")
WORK_TOKEN = os.environ.get("A_LENS_WORK_TOKEN", "")
WORK_API_KEY = os.environ.get("A_LENS_WORK_API_KEY", "")
LIFE_URL = os.environ.get("A_LENS_LIFE_URL", "")
CACHE_TTL = float(os.environ.get("A_LENS_CACHE_TTL", "30"))

_cache: dict[str, tuple[float, object]] = {}


def _memo(key: str, fn):
    now = time.monotonic()
    hit = _cache.get(key)
    if hit is not None and now - hit[0] < CACHE_TTL:
        return hit[1]
    val = fn()
    _cache[key] = (now, val)
    return val


def _fixture(name: str) -> dict:
    return json.loads((_FIXTURES / name).read_text(encoding="utf-8"))


def _hub_get(client: httpx.Client, path: str) -> dict:
    """커넥션 풀 공유 — 스냅숏 한 번에 3+N번 호출하므로 핸드셰이크를 재사용한다."""
    r = client.get(path)
    r.raise_for_status()
    return r.json()


# 개별 호출에서 "빈 값으로 강등" 처리할 예외 — HTTP 오류 + 비정상 JSON
_DEGRADE = (httpx.HTTPError, ValueError)


def _id_seq(entity_id: str) -> int:
    """'page_12' → 12. 타임스탬프 부재(#40 대기) 동안 id 순서를 의사 시간으로 쓴다."""
    m = re.search(r"(\d+)$", entity_id or "")
    return int(m.group(1)) if m else 0


# ── 허브 스냅숏 ──────────────────────────────────────────────


def _hub_snapshot() -> dict:
    headers = {"Authorization": f"Bearer {WORK_TOKEN}"} if WORK_TOKEN else {}
    if WORK_API_KEY:
        headers["x-api-key"] = WORK_API_KEY
    floors: list[dict] = []
    details: dict[str, dict] = {}
    all_pages: list[dict] = []
    totals = {"issues": 0, "knowledge": 0, "skills": 0, "reuses": 0}

    with httpx.Client(base_url=WORK_URL, headers=headers, timeout=8) as client:
        spaces_raw = _hub_get(client, "/spaces").get("spaces", [])

        for i, s in enumerate(spaces_raw):
            sid = s.get("id")
            if not sid:
                continue
            # 비멤버 공간·권한 부족·깨진 응답은 빈 목록으로 강등 (전체 스냅숏은 살린다)
            try:
                pages = _hub_get(client, f"/spaces/{sid}/tree").get("tree", [])
            except _DEGRADE as e:
                log.warning("tree 수집 실패 (%s): %s", sid, e)
                pages = []
            try:
                issues = _hub_get(client, f"/issues?space_id={sid}").get("issues", [])
            except _DEGRADE as e:
                log.warning("issues 수집 실패 (%s): %s", sid, e)
                issues = []
            try:
                members = _hub_get(client, f"/spaces/{sid}/members").get("members", [])
            except _DEGRADE as e:
                log.warning("members 수집 실패 (%s): %s", sid, e)
                members = []

            resolved = sum(1 for it in issues if it.get("status") == "resolved")
            knowledge = len(pages)
            totals["issues"] += len(issues)
            totals["knowledge"] += knowledge

            # 활동 열기(0~3): 실측 근거가 생기기 전까지는 축적량 기반 근사
            volume = knowledge + len(issues)
            activity = 0 if volume == 0 else 1 if volume < 3 else 2 if volume < 8 else 3

            floors.append(
                {
                    "space_id": sid,
                    "name": s.get("name", sid),
                    "floor": i + 1,
                    "activity": activity,
                    # reuse: 허브에 ReuseEvent 조회 endpoint가 아직 없다 (#40 후속 요청 후보)
                    "stats": {"knowledge": knowledge, "resolved": resolved, "reuse": 0},
                    "highlight": None,  # 서버 서사 부재 — 아래 활동 피드에서 결정론 선정
                }
            )
            # 지식 본문: 원문 모달(L4 역추적)용. 페이지 수가 적고 30s 캐시라 개별 조회 감당 가능
            knowledge_docs = []
            for p in pages:
                doc = {"doc_id": p["page_id"], "title": p.get("title", "")}
                try:
                    page = _hub_get(client, f"/pages/{p['page_id']}")
                    doc["body"] = page.get("body", "")
                    doc["visibility"] = page.get("visibility", "org")
                    doc["summary"] = (page.get("body") or "")[:120]
                except _DEGRADE:
                    doc["body"] = ""
                    doc["visibility"] = "space"  # 못 읽었으면 잠금으로 취급
                    doc["summary"] = ""
                knowledge_docs.append(doc)

            details[sid] = {
                "space_id": sid,
                "agents": [
                    {"agent_id": m["agent_id"], "name": m.get("name", m["agent_id"]), "status": "idle"}
                    for m in members
                ],
                "issues": issues,
                "knowledge": knowledge_docs,
            }
            for p in pages:
                all_pages.append({**p, "space_id": sid, "space_name": s.get("name", sid)})

    # 활동 피드 합성: knowledge_created만 (타임스탬프·재사용 피드는 #40·후속 대기).
    # 서사 문장은 구조 필드로 소비자가 조합 — 계약 consumerAutonomy가 허용하는 방식.
    all_pages.sort(key=lambda p: _id_seq(p["page_id"]), reverse=True)
    events = [
        {
            "type": "knowledge_created",
            "doc_id": p["page_id"],
            "space_id": p["space_id"],
            "summary": f"『{p.get('title', p['page_id'])}』 지식이 {p['space_name']}에 등록되었습니다",
        }
        for p in all_pages
    ]

    return {
        "source": "hub",
        "collected_at": datetime.now(timezone.utc).isoformat(),  # 타임스탬프 부재(#40) 동안 시각 필드 대체재
        "floors": floors,
        "details": details,
        "totals": totals,
        "tokens_saved_est": None,  # 실측 재료 없음 (#40 후순위 항목)
        "events": events,
        "reuse_events": [],
    }


# ── 픽스처 스냅숏 (C2 골든 데이터) ───────────────────────────


def _fixture_snapshot() -> dict:
    spaces = _fixture("spaces.json")["spaces"]
    stats = _fixture("stats.json")
    return {
        "source": "fixtures",
        "floors": [
            {
                "space_id": s["space_id"],
                "name": s.get("name", s["space_id"]),
                "floor": s.get("floor"),
                "activity": s.get("activity", 0),
                "stats": s.get("stats", {}),
                "highlight": s.get("highlight"),
            }
            for s in spaces
        ],
        "details": None,  # 픽스처 상세는 space_detail()에서 tier별 파일로
        "totals": stats.get("totals", {}),
        "tokens_saved_est": stats.get("tokens_saved_est"),
        "events": _fixture("activity.json")["events"],
        "reuse_events": _fixture("reuse-events.json")["events"],
    }


# ── 공개 API ────────────────────────────────────────────────


def snapshot() -> dict:
    """원천 하나로 통일된 스냅숏. auto: 허브 시도 → 실패 시 픽스처."""
    if SOURCE in ("hub", "auto"):
        try:
            return _memo("hub", _hub_snapshot)
        except Exception as e:  # noqa: BLE001 — 원천 전환 지점
            if SOURCE == "hub":
                raise
            log.warning("허브 수집 실패, 픽스처로 폴백: %s", e)
            # 폴백 결과를 'hub' 키에도 캐시 — 허브가 죽어 있는 동안 매 요청이
            # 타임아웃(최대 8s)을 기다리는 것을 TTL 동안 방지
            val = _memo("fixtures", _fixture_snapshot)
            _cache["hub"] = (time.monotonic(), val)
            return val
    return _memo("fixtures", _fixture_snapshot)


def space_detail(space_id: str, tier: str = "member") -> dict:
    snap = snapshot()
    if snap["source"] == "hub":
        # deepcopy — 얕은 복사면 프레즌스 조인(pipeline)이 캐시된 스냅숏을 오염시킨다
        raw = snap["details"].get(space_id)
        detail = copy.deepcopy(raw) if raw else {"space_id": space_id, "agents": [], "issues": [], "knowledge": []}
        if tier == "guest":
            detail["issues"] = []  # 이슈는 멤버 전용 (C2 계약 — 게스트 비노출 결정)
        detail["viewer_tier"] = tier
        return detail
    name = "space-detail-guest.json" if tier == "guest" else "space-detail-member.json"
    detail = _fixture(name)
    detail["space_id"] = space_id
    return detail


def fetch_presence(room_id: str) -> dict | None:
    """a-hub-life 프레즌스 (G8: a-lens가 직접 조회해 조인). #39 대기."""
    if not LIFE_URL:
        return None
    try:
        r = httpx.get(f"{LIFE_URL.rstrip('/')}/rooms/{room_id}", timeout=5)
        r.raise_for_status()
        return r.json()
    except httpx.HTTPError:
        return None
