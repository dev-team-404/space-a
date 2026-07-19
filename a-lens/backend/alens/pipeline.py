"""가공 — collector 스냅숏을 화면용 뷰모델로 번역한다 (ADR 0003 §2).

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
    """방 뷰모델 — work 상세. 프레즌스(online/offline)는 collector가 work의 최근 쓰기
    활동으로 이미 판정해 status·last_active_at에 채워둔다 (2026-07-19, life 프레즌스 대체)."""
    return collector.space_detail(space_id, tier)
