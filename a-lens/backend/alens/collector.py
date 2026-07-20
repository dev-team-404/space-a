"""원천 수집 — a-hub-work 실서버 + backend/dummy_data 데모 데이터.

허브 주소·토큰은 배포에 따라 바뀔 수 있으므로 전부 환경변수로 받는다:

  A_LENS_SOURCE      auto(기본) | hub | dummy | fixtures
                     auto    허브 성공 시 허브+더미 병합, 실패 시 더미만
                     hub     허브만 (실데이터 검증용, 실패 시 에러)
                     dummy   backend/dummy_data만
                     fixtures contracts/fixtures 골든 데이터만 (계약 검증용)
  A_LENS_WORK_URL    a-hub-work base URL (기본 https://spacea.msalt.net)
  A_LENS_WORK_TOKEN  허브 Bearer 토큰 (없으면 인증 필요한 상세는 비어서 내려감)
  A_LENS_WORK_API_KEY  허브 x-api-key 헤더 값 (2026-07-19 허브 인증 전환 — 비면 생략)
  A_LENS_PRESENCE_WINDOW  online 판정 창(초, 기본 3600) — 이 시간 안에 write 한 사람만 online
  A_LENS_CACHE_TTL   허브 폴링 캐시 초 (기본 30)

C2가 허브에 구현되기 전까지는 허브의 현행 REST(GET /spaces · tree · issues ·
members)를 읽어 C2 비슷한 모양으로 맞춘다 — pipeline은 원천을 몰라도 된다.
더미 스페이스는 floors·detail에 demo=True 마킹 — 프론트가 FAKE 배지로 구분한다.
"""

import copy
import json
import logging
import os
import re
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

import httpx

log = logging.getLogger("alens.collector")

# repo 루트 기준 contracts/fixtures (backend/alens/ → 세 단계 위)
_FIXTURES = Path(__file__).resolve().parents[3] / "contracts" / "fixtures"
# 데모용 더미 데이터 (backend/dummy_data — _generate.py로 재생성)
_DUMMY_DIR = Path(__file__).resolve().parents[1] / "dummy_data"

SOURCE = os.environ.get("A_LENS_SOURCE", "auto")
WORK_URL = os.environ.get("A_LENS_WORK_URL", "https://spacea.msalt.net").rstrip("/")
WORK_TOKEN = os.environ.get("A_LENS_WORK_TOKEN", "")
WORK_API_KEY = os.environ.get("A_LENS_WORK_API_KEY", "")
# 프레즌스는 work 최근 쓰기 활동으로 판정한다 (life life-server 프레즌스 대체, 2026-07-19).
PRESENCE_WINDOW = float(os.environ.get("A_LENS_PRESENCE_WINDOW", "3600"))
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


def _parse_ts(ts: str | None) -> datetime | None:
    """ISO 8601 UTC 문자열 → tz-aware datetime. 파싱 불가·빈 값은 None."""
    if not ts:
        return None
    try:
        dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except ValueError:
        return None
    return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)


def _bump_activity(
    activity: dict[str, dict], agent_id: str | None, ts: str | None, kind: str, title: str
) -> None:
    """agent_id의 최근 활동을 갱신 — 더 최신 write면 항목(시각·종류·제목)을 통째로 덮어쓴다.
    kind: 'knowledge'(page 작성) | 'issue'(이슈 열기). 사람이 읽을 문장 합성의 재료."""
    if not agent_id:
        return
    dt = _parse_ts(ts)
    if dt is None:
        return
    prev = activity.get(agent_id)
    if prev is None or dt > prev["at"]:
        activity[agent_id] = {"at": dt, "kind": kind, "title": title}


def _flatten_tree(nodes: list[dict]) -> list[dict]:
    """tree 응답은 children 중첩 구조 — 프레즌스 집계용으로 모든 노드를 평탄화한다.
    응답이 예상과 다른 모양(리스트 아님·노드가 dict 아님)이어도 스냅숏을 살리도록 방어한다."""
    if not isinstance(nodes, list):
        return []
    flat: list[dict] = []
    for n in nodes:
        if isinstance(n, dict):
            flat.append(n)
            flat.extend(_flatten_tree(n.get("children") or []))
    return flat


# 허브 이슈는 상태 전이 이력(timeline)을 아직 내려주지 않는다 (#40 대기). 프론트 뷰모델
# (SpaceIssue.timeline)을 채우기 위해 현재 상태 1스텝을 합성한다 — 실제 단계별 이력은
# 허브가 이슈 이벤트를 제공하면 대체한다.
_ISSUE_STEP_LABEL = {"open": "이슈 발생", "knowledge_linked": "지식 연결", "resolved": "해결 완료"}


# 최근 활동 → 사람이 읽는 문장. 지금은 규칙(제목+종류) 기반. 추후 이 함수 안에서 LLM으로
# body를 요약/번역해 더 자연스러운 문장을 만들 수 있다 (교체 지점 — 시그니처 유지).
def _humanize_activity(item: dict | None) -> dict | None:
    if not item:
        return None
    title = (item.get("title") or "").strip() or "이름 없는 문서"
    # 말풍선(brief)은 가장 최근 업무의 한 줄 요약 — 제목 기반. 길면 렌더러가 말줄임 처리.
    # 상세(detail)는 문장으로 풀어쓴다.
    if item.get("kind") == "issue":
        brief = f"‘{title}’ 이슈 해결 중"
        detail = f"최근에 ‘{title}’ 문제를 이슈로 등록했어요. 팀이 함께 살펴보는 중이에요."
    else:  # knowledge
        brief = f"‘{title}’ 지식 공유 중"
        detail = f"최근에 ‘{title}’ 내용을 정리해 팀에 공유했어요. 다른 사람이 참고해 재사용할 수 있어요."
    return {"brief": brief, "detail": detail}


def _issue_vm(issue: dict, member_name: dict[str, str]) -> dict:
    status = issue.get("status", "open")
    opener = issue.get("opened_by")
    return {
        "issue_id": issue.get("issue_id", ""),
        "title": issue.get("title", ""),
        "status": status,
        "opened_by": opener,
        "timeline": [
            {
                "step": status,
                "label": _ISSUE_STEP_LABEL.get(status, status),
                "actor": member_name.get(opener, opener or ""),
                "at": issue.get("updated_at") or issue.get("created_at"),
                "note": "",
            }
        ],
    }


# ── 허브 스냅숏 ──────────────────────────────────────────────


def _hub_snapshot() -> dict:
    headers = {"Authorization": f"Bearer {WORK_TOKEN}"} if WORK_TOKEN else {}
    if WORK_API_KEY:
        headers["x-api-key"] = WORK_API_KEY
    floors: list[dict] = []
    details: dict[str, dict] = {}
    all_pages: list[dict] = []
    totals = {"issues": 0, "knowledge": 0, "skills": 0, "reuses": 0}
    collected_at = datetime.now(timezone.utc)
    online_cutoff = collected_at.timestamp() - PRESENCE_WINDOW

    with httpx.Client(base_url=WORK_URL, headers=headers, timeout=8) as client:
        spaces_raw = _hub_get(client, "/spaces").get("spaces", [])

        for i, s in enumerate(spaces_raw):
            sid = s.get("id")
            if not sid:
                continue
            # 비멤버 공간·권한 부족·깨진 응답은 빈 목록으로 강등 (전체 스냅숏은 살린다)
            try:
                pages = _flatten_tree(_hub_get(client, f"/spaces/{sid}/tree").get("tree", []))
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
            # agent_id → 사람이 읽을 name. Page 작성자(author_agent)·이슈 opened_by 표시에 공용으로 쓴다.
            member_name = {m["agent_id"]: m.get("name", m["agent_id"]) for m in members}

            # 프레즌스: 이 방 사람들의 최근 write(page·issue) 시각을 집계 → online 판정 재료.
            # (life life-server 프레즌스 대체 — a-lens는 사람이 보는 view라 '사람의 활동'으로 읽는다.)
            last_write: dict[str, dict] = {}
            for p in pages:
                _bump_activity(
                    last_write, p.get("created_by"), p.get("updated_at") or p.get("created_at"),
                    "knowledge", p.get("title", ""),
                )
            for it in issues:
                _bump_activity(
                    last_write, it.get("opened_by"), it.get("updated_at") or it.get("created_at"),
                    "issue", it.get("title", ""),
                )

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
                doc = {
                    "doc_id": p["page_id"],
                    "title": p.get("title", ""),
                    "reuse_count": 0,  # 허브에 ReuseEvent 조회 endpoint가 아직 없다 (totals와 동일 사유)
                    "cited_by": [],
                }
                try:
                    page = _hub_get(client, f"/pages/{p['page_id']}")
                    doc["body"] = page.get("body", "")
                    doc["visibility"] = page.get("visibility", "org")
                    doc["summary"] = (page.get("body") or "")[:120]
                    # 작성자: created_by_name(허브가 내려주면) 우선, 없으면 멤버 목록에서 이름 조회,
                    # 그마저 없으면(비멤버 공간 등) agent_id 그대로.
                    creator = page.get("created_by")
                    doc["author_agent"] = page.get("created_by_name") or member_name.get(creator, creator) or ""
                except _DEGRADE:
                    doc["body"] = ""
                    doc["visibility"] = "space"  # 못 읽었으면 잠금으로 취급
                    doc["summary"] = ""
                    doc["author_agent"] = ""
                knowledge_docs.append(doc)

            def _agent(m: dict) -> dict:
                seen = last_write.get(m["agent_id"])
                online = seen is not None and seen["at"].timestamp() >= online_cutoff
                return {
                    "agent_id": m["agent_id"],
                    "name": m.get("name", m["agent_id"]),
                    "status": "working" if online else "idle",
                    "last_active_at": seen["at"].isoformat() if seen else None,
                    # 사람이 읽을 최근 활동 문장 (말풍선=brief, 상세=detail). 번역은 _humanize_activity.
                    "recent_activity": _humanize_activity(seen) if seen else None,
                }

            details[sid] = {
                "space_id": sid,
                "agents": [_agent(m) for m in members],
                "issues": [_issue_vm(it, member_name) for it in issues],
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
        "collected_at": collected_at.isoformat(),  # 타임스탬프 부재(#40) 동안 시각 필드 대체재 + 프레즌스 기준 시각
        "floors": floors,
        "details": details,
        "totals": totals,
        "tokens_saved_est": None,  # 실측 재료 없음 (#40 후순위 항목)
        "events": events,
        "reuse_events": [],
    }


# ── 더미 스냅숏 (backend/dummy_data — 데모용 fake 데이터) ─────
#
# 시각은 파일에 "몇 분 전(min_ago)" 오프셋으로 저장돼 있어 로드 시점 기준으로
# 환산한다 — 언제 띄워도 프레즌스(온라인/오프라인)가 살아 있는 데모가 된다.


def _dummy_snapshot() -> dict:
    now = datetime.now(timezone.utc)
    cutoff = now.timestamp() - PRESENCE_WINDOW
    floors: list[dict] = []
    details: dict[str, dict] = {}
    totals = {"issues": 0, "knowledge": 0, "skills": 0, "reuses": 0}
    events: list[dict] = []
    reuse_events: list[dict] = []

    def ago(minutes: float) -> datetime:
        return now - timedelta(minutes=minutes)

    for i, f in enumerate(sorted(_DUMMY_DIR.glob("*.json"))):
        raw = json.loads(f.read_text(encoding="utf-8"))
        sp = raw["space"]
        sid = sp["space_id"]
        # 하이라이트: 방 상세엔 구조체(kind+참조 id — 칠판 클릭 연동), 층 목록엔 문자열만
        hl = raw.get("highlight")
        hl_text = hl.get("text") if isinstance(hl, dict) else hl
        member_name = {m["agent_id"]: m["name"] for m in raw.get("members", [])}

        agents = []
        for m in raw.get("members", []):
            at = ago(m["last_active_min_ago"]) if m.get("last_active_min_ago") is not None else None
            online = at is not None and at.timestamp() >= cutoff
            agents.append(
                {
                    "agent_id": m["agent_id"],
                    "name": m["name"],
                    "role": m.get("role", ""),
                    "owner": m.get("owner", ""),
                    "status": "working" if online else "idle",
                    "status_line": "",
                    "last_active_at": at.isoformat() if at else None,
                    "recent_activity": _humanize_activity(m.get("activity")),
                }
            )

        issues = []
        for it in raw.get("issues", []):
            status = it.get("status", "open")
            issues.append(
                {
                    "issue_id": it["issue_id"],
                    "title": it["title"],
                    "status": status,
                    "opened_by": it.get("opened_by", ""),
                    "timeline": [
                        {
                            "step": status,
                            "label": _ISSUE_STEP_LABEL.get(status, status),
                            "actor": member_name.get(it.get("opened_by"), it.get("opened_by", "")),
                            "at": ago(it.get("updated_min_ago", 0)).isoformat(),
                            "note": "",
                        }
                    ],
                }
            )

        docs = []
        for d in raw.get("knowledge", []):
            docs.append(
                {
                    "doc_id": d["doc_id"],
                    "title": d["title"],
                    "author_agent": d.get("author", ""),
                    "visibility": d.get("visibility", "org"),
                    "summary": d.get("summary", (d.get("body") or "")[:120]),
                    "body": d.get("body", ""),
                    "cited_by": d.get("cited_by", []),
                    "reuse_count": d.get("reuse_count", 0),
                }
            )
            events.append(
                {
                    "type": "knowledge_created",
                    "doc_id": d["doc_id"],
                    "space_id": sid,
                    "at": ago(d.get("created_min_ago", 0)).isoformat(),
                    "summary": f"『{d['title']}』 지식이 {sp['name']}에 등록되었습니다",
                    "demo": True,
                }
            )

        for r in raw.get("reuse_events", []):
            at = ago(r.get("min_ago", 0)).isoformat()
            reuse_events.append({**{k: v for k, v in r.items() if k != "min_ago"}, "at": at, "demo": True})
            events.append(
                {
                    "type": "reused",
                    "doc_id": r.get("doc_id"),
                    "space_id": r.get("consumer_space"),
                    "at": at,
                    "summary": r.get("summary", ""),
                    "demo": True,
                }
            )

        resolved = sum(1 for it in issues if it["status"] == "resolved")
        reuses = sum(d["reuse_count"] for d in docs)
        totals["issues"] += len(issues)
        totals["knowledge"] += len(docs)
        totals["reuses"] += reuses

        volume = len(docs) + len(issues)
        activity = 0 if volume == 0 else 1 if volume < 3 else 2 if volume < 8 else 3
        floors.append(
            {
                "space_id": sid,
                "name": sp.get("name", sid),
                "floor": i + 1,
                "activity": activity,
                "stats": {"knowledge": len(docs), "resolved": resolved, "reuse": reuses},
                "highlight": hl_text,
                "demo": True,
            }
        )
        details[sid] = {
            "space_id": sid,
            "agents": agents,
            "issues": issues,
            "knowledge": docs,
            "visits": raw.get("visits"),
            # 오늘의 하이라이트 — 방 칠판 표시 + 클릭 시 사이드바 관련 항목 연동
            "highlight": hl,
            "demo": True,
        }

    return {
        "source": "dummy",
        "collected_at": now.isoformat(),
        "floors": floors,
        "details": details,
        "totals": totals,
        "tokens_saved_est": None,
        "events": events,
        "reuse_events": reuse_events,
    }


def _merged_snapshot() -> dict:
    """허브 실데이터 + 더미 데모 데이터. 더미 층은 허브 층 뒤 번호로 이어 붙인다."""
    hub = _hub_snapshot()
    dummy = _dummy_snapshot()
    floors = list(hub["floors"])
    base = len(floors)
    floors += [{**f, "floor": base + j + 1} for j, f in enumerate(dummy["floors"])]
    keys = set(hub["totals"]) | set(dummy["totals"])
    return {
        "source": "hub+dummy",
        "collected_at": hub["collected_at"],
        "floors": floors,
        # space_id 충돌 시 허브(실데이터) 우선
        "details": {**dummy["details"], **hub["details"]},
        "totals": {k: hub["totals"].get(k, 0) + dummy["totals"].get(k, 0) for k in keys},
        "tokens_saved_est": hub["tokens_saved_est"],
        "events": hub["events"] + dummy["events"],
        "reuse_events": hub["reuse_events"] + dummy["reuse_events"],
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
    """스냅숏. auto: 허브+더미 병합, 허브 실패 시 더미만 (더미 항목은 demo 마킹)."""
    if SOURCE == "hub":
        return _memo("hub", _hub_snapshot)
    if SOURCE == "dummy":
        return _memo("dummy", _dummy_snapshot)
    if SOURCE == "fixtures":
        return _memo("fixtures", _fixture_snapshot)
    try:
        return _memo("merged", _merged_snapshot)
    except Exception as e:  # noqa: BLE001 — 원천 전환 지점
        log.warning("허브 수집 실패, dummy_data만 표시: %s", e)
        # 폴백 결과를 'merged' 키에도 캐시 — 허브가 죽어 있는 동안 매 요청이
        # 타임아웃(최대 8s)을 기다리는 것을 TTL 동안 방지
        val = _memo("dummy", _dummy_snapshot)
        _cache["merged"] = (time.monotonic(), val)
        return val


def space_detail(space_id: str, tier: str = "member") -> dict:
    snap = snapshot()
    if snap["details"] is not None:
        # deepcopy — 얕은 복사면 tier별 변형(guest 이슈 제거 등)이 캐시된 스냅숏을 오염시킨다
        raw = snap["details"].get(space_id)
        detail = copy.deepcopy(raw) if raw else {"space_id": space_id, "agents": [], "issues": [], "knowledge": []}
        if tier == "guest":
            detail["issues"] = []  # 이슈는 멤버 전용 (C2 계약 — 게스트 비노출 결정)
        # 이 방이 원천이거나 소비자인 재사용 이벤트 — Hub 사이드바 '지식 재사용' 탭 재료.
        # reused는 org-safe(G7)라 게스트에게도 노출한다.
        detail["reuse_events"] = sorted(
            (
                r
                for r in snap["reuse_events"]
                if r.get("source_space") == space_id or r.get("consumer_space") == space_id
            ),
            key=lambda r: r.get("at", ""),
            reverse=True,
        )
        detail["viewer_tier"] = tier
        return detail
    name = "space-detail-guest.json" if tier == "guest" else "space-detail-member.json"
    detail = _fixture(name)
    detail["space_id"] = space_id
    return detail
