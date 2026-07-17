"""가공 — raw 원천을 화면용 뷰모델로 번역한다.

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


def _pick_highlight(events: list[dict]) -> dict | None:
    pool = [e for e in events if e.get("type") in _ORG_SAFE]
    if not pool:
        return None
    pool.sort(key=lambda e: (_HIGHLIGHT_RANK.get(e["type"], 9), e.get("at", "")), reverse=False)
    # 크로스팀 재사용 가점: reused 중 cross-team이 있으면 최우선
    return pool[0]


def lobby_view() -> dict:
    """로비 뷰모델 — 층 목록 + 회사 집계 + 오늘의 하이라이트."""
    spaces = collector.fetch_spaces()["spaces"]
    stats = collector.fetch_stats()
    activity = collector.fetch_activity()["events"]

    return {
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
        "totals": stats.get("totals", {}),
        "tokens_saved_est": stats.get("tokens_saved_est"),  # 표시 시 '~' 필수
        "highlight": _pick_highlight(activity),
    }


def space_view(space_id: str, tier: str = "member") -> dict:
    """방 뷰모델 — work 상세 + (가능하면) life 프레즌스 조인 (G8)."""
    detail = collector.fetch_space_detail(space_id, tier)
    presence = collector.fetch_presence(space_id)
    if presence:
        by_id = {o.get("agent_id"): o for o in presence.get("occupants", [])}
        for agent in detail.get("agents", []):
            live = by_id.get(agent.get("agent_id"))
            if live:
                agent["status"] = live.get("status", agent.get("status"))
                agent["last_active_at"] = live.get("last_active_at")
    return detail
