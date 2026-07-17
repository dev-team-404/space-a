"""가공 — collector 스냅숏을 화면용 뷰모델로 번역한다.

프로토타입의 c2-adapter.js가 하던 번역이 여기로 올라온다 (ADR 0003 §2).
필드 단위 스펙은 docs/design/a-lens/04-data-mapping.md — 이 문서가 정답이다.
번역 3형태 중 집계·공간은 여기(결정론), 서사는 서버 제공 문장을 그대로 싣는다 (MVP).
"""

from . import collector

# 하이라이트 결정론 랭킹 (04-data-mapping "하이라이트 선정 기준", 2026-07-14 결정)
_HIGHLIGHT_RANK = {
    "reused": 0,
    "skill_proposed": 1,
    "knowledge_created": 2,
    "condensed": 3,
    "issue_opened": 4,
}
# 로비·게스트에 노출 가능한 org-safe 이벤트 (G7 정책)
_ORG_SAFE = {"reused", "knowledge_created", "skill_proposed", "condensed"}


def _pick_highlight(events: list[dict], cross_team_docs: set[str] | None = None) -> dict | None:
    """org-safe 이벤트 중 1건 선정. 랭킹: 크로스팀 reused > reused > ... , 동점은 최신순."""
    pool = [e for e in events if e.get("type") in _ORG_SAFE]
    if not pool:
        return None
    cross = cross_team_docs or set()

    def rank(e: dict) -> int:
        r = _HIGHLIGHT_RANK.get(e["type"], 9)
        if e["type"] == "reused" and e.get("doc_id") in cross:
            r -= 1  # 크로스팀 재사용 가점 (04-data-mapping: "크로스팀이면 +1")
        return r

    pool.sort(key=lambda e: e.get("at", ""), reverse=True)  # 동점은 최신순 (stable sort)
    pool.sort(key=rank)
    return pool[0]


def lobby_view() -> dict:
    """로비 뷰모델 — 층 목록 + 회사 집계 + 오늘의 하이라이트."""
    snap = collector.snapshot()
    cross_team_docs = {
        e["doc_id"]
        for e in snap["reuse_events"]
        if e.get("doc_id") and e.get("source_space") and e.get("source_space") != e.get("consumer_space")
    }
    return {
        "source": snap["source"],  # 화면에서 실데이터/픽스처 구분 표시용
        "floors": snap["floors"],
        "totals": snap["totals"],
        "tokens_saved_est": snap["tokens_saved_est"],  # 표시 시 '~' 필수
        "highlight": _pick_highlight(snap["events"], cross_team_docs),
    }


def space_view(space_id: str, tier: str = "member") -> dict:
    """방 뷰모델 — work 상세 + (가능하면) life 프레즌스 조인 (G8)."""
    detail = collector.space_detail(space_id, tier)
    presence = collector.fetch_presence(space_id)
    if presence:
        by_id = {o.get("agent_id"): o for o in presence.get("occupants", [])}
        for agent in detail.get("agents", []):
            live = by_id.get(agent.get("agent_id"))
            if live:
                agent["status"] = live.get("status", agent.get("status"))
                agent["last_active_at"] = live.get("last_active_at")
    return detail


# ── C2 wire 브리지 (프로토타입 UI 실데이터 연결용) ──────────────────
#
# 프로토타입(c2-adapter.js)은 C2 계약 wire 모양을 기대한다. 허브 스냅숏을 그 모양으로
# 변환해 내려주면, 프로토타입 화면 그대로 실데이터가 뜬다. hub 원천일 때만 동작 —
# 픽스처 모드에서는 호출측(c2-live.js)이 프로토타입 내장 가짜 데이터를 그대로 쓴다.
#
# 허브에 없는 필드의 기본값은 화면이 깨지지 않는 중립값으로 채우되, 시각 필드는
# 수집 시각(collected_at)으로 대체한다 — 실제 발생 시각은 #40(타임스탬프) 이후 가능.

_ISSUE_STEP_LABEL = {"open": "이슈 발생", "knowledge_linked": "지식 연결", "resolved": "해결"}


def _c2_guard() -> dict | None:
    snap = collector.snapshot()
    return snap if snap["source"] == "hub" else None


def c2_spaces() -> dict | None:
    snap = _c2_guard()
    if snap is None:
        return None
    return {
        "spaces": [
            {
                "space_id": f["space_id"],
                "name": f["name"],
                "floor": f["floor"],
                "seed": False,
                "motto": "",
                "token_budget": 100,
                "token_used": 0,  # 토큰 계측은 텔레메트리(계획) 이후
                "status": "정상",
                "activity": f["activity"],
                "members_online": 0,  # 프레즌스(#39) 이후
                "stats": {**f["stats"], "reuse": f["stats"].get("reuse", 0)},
                "highlight": f["highlight"],
                "viewer_tier": "member",  # 인증(G1)은 C2 밖 — 데모는 멤버 뷰
            }
            for f in snap["floors"]
        ]
    }


def c2_space_details() -> dict | None:
    """프로토타입 어댑터가 기대하는 {space_id: detail} 키드 객체."""
    snap = _c2_guard()
    if snap is None:
        return None
    at = snap["collected_at"]
    out: dict[str, dict] = {}
    for sid, d in snap["details"].items():
        out[sid] = {
            "space_id": sid,
            "viewer_tier": "member",
            "agents": [
                {
                    "agent_id": a["agent_id"],
                    "name": a["name"],
                    "role": "지식",
                    "owner": "",
                    "status": a.get("status", "idle"),
                    "status_line": "",  # 서사 부재 — 프레즌스·서사(Q1) 이후
                }
                for a in d["agents"]
            ],
            "issues": [
                {
                    "issue_id": i["issue_id"],
                    "title": i["title"],
                    "status": i["status"],
                    "timeline": [
                        {
                            "step": i["status"],
                            "label": _ISSUE_STEP_LABEL.get(i["status"], i["status"]),
                            "actor": i.get("opened_by", ""),
                            "at": at,  # 수집 시각 — 실제 전이 시각은 #40 이후
                        }
                    ],
                }
                for i in d["issues"]
            ],
            "knowledge": [
                {
                    "doc_id": k["doc_id"],
                    "title": k["title"],
                    "author_agent": "",
                    "visibility": k.get("visibility", "org"),
                    "summary": k.get("summary", ""),
                    "body": k.get("body", ""),
                    "created_at": at,  # G4(#40) 이후 실제 값
                    "cited_by": [],
                    "reuse_count": 0,
                }
                for k in d["knowledge"]
            ],
            "visits": {"today": 0, "total": 0},  # 방문은 life(#39) 영역
        }
    return out


def c2_activity() -> dict | None:
    snap = _c2_guard()
    if snap is None:
        return None
    at = snap["collected_at"]
    return {"events": [{**e, "at": at} for e in snap["events"]]}


def c2_reuse_events() -> dict | None:
    snap = _c2_guard()
    if snap is None:
        return None
    return {"events": snap["reuse_events"]}  # 허브에 조회 endpoint 부재 — 현재 빈 목록


def c2_stats() -> dict | None:
    snap = _c2_guard()
    if snap is None:
        return None
    return {
        "period": {},
        "totals": snap["totals"],
        "tokens_saved_est": 0,  # 실측 재료 없음 — 0으로 정직하게
        "top_reused_skills": [],
        "top_knowledge": [],
        "by_space": [
            {"space_id": f["space_id"], "contributed": f["stats"].get("knowledge", 0), "reused": 0}
            for f in snap["floors"]
        ],
    }
