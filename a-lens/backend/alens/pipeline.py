"""가공 — collector 스냅숏을 화면용 뷰모델로 번역한다 (ADR 0003 §2).

현재 데이터 흐름은 docs/architecture/a-lens/04-data-and-integration.md를 따른다.
번역 3형태 중 집계·공간과 하이라이트 문장 조립은 여기서 결정론적으로 처리한다.
Page·Issue의 분류·요약·서사는 collector가 translator를 통해 만든 결과를 싣는다.

하이라이트(로비 ★ · 방 칠판)는 여기서만 선정한다 — 소스(허브/더미/픽스처)에 무관하게
같은 랭킹을 쓰므로 로비와 방이 어긋나지 않는다.
스펙: docs/archive/design/a-lens/specs/2026-07-30-room-board-highlight.md
"""

import logging
from collections.abc import Callable
from datetime import datetime, timedelta, timezone

from . import collab, collector

log = logging.getLogger("alens.pipeline")
KST = timezone(timedelta(hours=9))

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

# 이벤트 종류 → 칠판 클릭 시 열 사이드바 탭 (스펙 §4). 참조 id가 없으면 연동을 붙이지 않는다.
_HIGHLIGHT_KIND = {
    "reused": "reuse",
    "knowledge_created": "knowledge",
    "skill_proposed": "knowledge",
    "condensed": "knowledge",
    "issue_opened": "issue",
}


# ── 선정 ────────────────────────────────────────────────────


def _is_cross_team(e: dict) -> bool:
    """방 밖으로 나간 재사용인가. 허브는 cross_team 플래그를, 더미·픽스처는 원천≠소비 방으로 준다."""
    if e.get("cross_team"):
        return True
    src, con = e.get("source_space"), e.get("consumer_space")
    return bool(src and con and src != con)


def _cross_team_docs(snap: dict) -> set[str]:
    """이벤트에 원천/소비 방이 안 실린 소스(픽스처)를 위한 보조 신호 — 재사용 피드에서 유도."""
    return {
        e["doc_id"]
        for e in snap["reuse_events"]
        if e.get("doc_id") and e.get("source_space") and e.get("source_space") != e.get("consumer_space")
    }


def _rank(e: dict, cross_docs: set[str]) -> int:
    r = _HIGHLIGHT_RANK.get(e.get("type"), 9)
    if e.get("type") == "reused" and (_is_cross_team(e) or e.get("doc_id") in cross_docs):
        r -= 1  # 크로스팀 재사용 가점 (04-data-mapping: "크로스팀이면 +1")
    return r


def _room_rank(e: dict, space_id: str, cross_docs: set[str]) -> int:
    """방 칠판용 랭킹 — 등급이 같으면 '우리 지식이 나갔다'(기여)를 '우리가 가져왔다'(소비)보다 앞에.

    북극성은 기여↔소비 중 기여 쪽이므로, 자기 칠판에는 자랑할 쪽을 먼저 적는다.
    (×2 하고 +1 하므로 상위 등급을 넘어서지 않는다)"""
    r = _rank(e, cross_docs) * 2
    if e.get("type") == "reused" and e.get("consumer_space") == space_id and e.get("source_space") != space_id:
        r += 1
    return r


def _best(pool: list[dict], rank: Callable[[dict], int]) -> dict | None:
    """랭킹 1위 1건. 동점은 최신순 (시각 미상은 뒤로 — #40 동안 정렬 재료가 없는 이벤트가 있다)."""
    if not pool:
        return None
    ranked = sorted(pool, key=lambda e: e.get("at") or "", reverse=True)  # stable sort 전제
    ranked.sort(key=rank)
    return ranked[0]


def _visible(e: dict, tier: str) -> bool:
    """노출 풀. 게스트는 org-safe만 — issue_opened는 summary에 이슈 제목이 담긴다 (G7)."""
    t = e.get("type")
    return t in _ORG_SAFE if tier == "guest" else (t in _ORG_SAFE or t == "issue_opened")


def _room_events(space_id: str, events: list[dict]) -> list[dict]:
    """이 방의 이벤트 — 방에서 발생했거나, 방의 지식이 다른 방에서 재사용됐다 (스펙 §2).

    두 번째 조건이 없으면 "우리 지식이 다른 팀에 쓰였다"는 가장 값진 사건이 원천 방
    칠판에는 안 뜨고 가져간 방에만 뜬다."""
    return [e for e in events if e.get("space_id") == space_id or e.get("source_space") == space_id]


# ── 문장 조립 (주인공은 사람 — 스펙 §3) ─────────────────────


def _has_final(word: str) -> bool:
    """마지막 글자에 종성이 있나 — 조사(을/를·이/가) 선택용. 한글이 아니면 있다고 본다."""
    if not word:
        return False
    ch = word[-1]
    if "가" <= ch <= "힣":
        return (ord(ch) - 0xAC00) % 28 != 0
    return True  # 영문·숫자 끝은 자음 취급 (완벽하진 않지만 방 이름 대부분이 한글이다)


def _obj(word: str) -> str:
    """목적격 조사만 반환 — 제목은 『』로 감싸므로 조사를 따로 붙여야 한다."""
    return "을" if _has_final(word) else "를"


def _subj(word: str) -> str:
    return "이" if _has_final(word) else "가"


# 허브의 페이지 제목은 LLM이 만든 여러 문장짜리라 그대로 쓰면 자리를 덮는다 (실측 151자)
# — 줄 전체 예산에서 남는 폭만큼만 제목에 준다. 스펙 §3.
#
# 예산이 두 개인 이유: **같은 문장이 두 자리에 쓰인다.**
#   · 로비 층 hover 카드 — 진짜 한 줄. 넘치면 카드가 깨지므로 좁게 유지한다.
#   · 방 칠판 — 여러 줄로 줄바꿈되고 폰트가 자동으로 맞춰진다(renderer의 이진 탐색).
#     칠판 안쪽은 342×138(이미지 px)로 한 줄 34자 기준 7줄이 들어간다. 48자는 그중 두 줄만
#     쓰고 나머지를 비워둔 채 제목을 '도구…'처럼 잘라버렸다.
_LINE_MAX = 48
_BOARD_MAX = 120  # 칠판 전용 — 34자/줄이면 약 4줄. 용량(7줄)에 여유를 두고 잡았다
_TITLE_MIN = 12  # 예산이 아무리 빡빡해도 제목은 이만큼은 보여준다


def _stem(title: str) -> str:
    """조사 선택용 어간 — 말줄임표로는 종성을 못 보므로 떼고 본다."""
    return title.rstrip("…").rstrip()


def _clip(title: str, limit: int) -> str:
    t = " ".join(str(title).split())  # 줄바꿈·연속 공백 정리
    limit = max(_TITLE_MIN, limit)
    return t if len(t) <= limit else t[: limit - 1].rstrip() + "…"


def _line(head: str, title: str, tail: str, budget: int) -> str:
    """제목을 남는 폭에 맞춰 잘라 넣고 한 줄을 만든다. `tail`의 `{p}`·`{s}`가 조사 자리.

    조사는 잘린 제목을 보고 골라야 한다 — format이 아니라 replace를 쓰는 건 방 이름에
    중괄호가 들어와도 터지지 않게 하려는 것."""
    clipped = _clip(title, budget - len(head) - len(tail))
    stem = _stem(clipped)
    return head + clipped + tail.replace("{p}", _obj(stem)).replace("{s}", _subj(stem))


def _recency(at: str | None, now: datetime) -> str:
    """오늘이 아니면 붙일 시간 접두. 시각을 모르면 빈 문자열 — 없는 날짜를 만들지 않는다 (#40)."""
    if not at:
        return ""
    try:
        ts = datetime.fromisoformat(str(at).replace("Z", "+00:00"))
    except ValueError:
        return ""
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    days = (now.astimezone(KST).date() - ts.astimezone(KST).date()).days
    if days <= 0:
        return ""
    if days == 1:
        return "어제"
    if days <= 7:
        return "지난주"
    if days <= 31:
        return f"{days}일 전"
    return "예전"


def _sentence(e: dict, space_id: str, name_of: dict[str, str], budget: int = _LINE_MAX) -> str:
    """칠판 한 줄. 재료(제목)가 없으면 서버 summary를 그대로 쓴다 — 문장을 억지로 만들지 않는다.

    같은 재사용 이벤트가 원천 방과 가져간 방 양쪽 칠판에 걸리므로 **이 방의 관점**으로 쓴다.
    안 그러면 내 방 칠판에 내 방 이름이 3인칭("SoC팀이 가져갔어요")으로 나온다."""
    if not (e.get("title") or "").strip():
        return _clip(e.get("summary", ""), budget)
    title = e["title"].strip()
    actor = (e.get("actor") or "").strip()
    t = e.get("type")

    if t == "reused":
        wrote = f"{actor}님이 쓴 『" if actor else "『"
        if not _is_cross_team(e):
            return _line(wrote, title, "』, 팀에서 다시 꺼내 썼어요", budget)
        if e.get("consumer_space") == space_id:  # 우리가 가져온 쪽
            giver = name_of.get(e.get("source_space") or "", "다른 팀")
            who = f"{giver} {actor}님의 『" if actor else f"{giver}의 『"
            return _line(who, title, "』{p} 가져와 썼어요", budget)
        taker = name_of.get(e.get("consumer_space") or "", "다른 팀")  # 우리 지식이 나간 쪽
        return _line(wrote, title, f"』, {taker}{_subj(taker)} 가져갔어요", budget)

    if t == "knowledge_created":
        if actor:
            return _line(f"{actor}님이 『", title, "』{p} 문서함에 넣었어요", budget)
        return _line("『", title, "』{s} 문서함에 들어왔어요", budget)

    if t == "issue_opened":
        if actor:
            return _line("‘", title, f"’ 이슈를 {actor}님이 올렸어요", budget)
        return _line("‘", title, "’ 이슈가 올라왔어요", budget)

    return _clip(e.get("summary", ""), budget)


def _room_highlight(space_id: str, snap: dict, tier: str, budget: int = _LINE_MAX) -> dict | None:
    """방 칠판 1건. 풀이 비면 None — 넓힐 대상이 없는 정직한 빈칸이므로 문구를 만들지 않는다.

    `budget`은 부르는 자리가 정한다 — 같은 문장이 칠판(여러 줄)과 로비 hover 카드(한 줄)
    양쪽에 쓰이고, 들어갈 수 있는 길이가 서로 다르다. 기본값은 좁은 쪽이다."""
    cross_docs = _cross_team_docs(snap)
    pool = [e for e in _room_events(space_id, snap["events"]) if _visible(e, tier)]
    best = _best(pool, lambda e: _room_rank(e, space_id, cross_docs))
    if not best:
        return None
    name_of = {f["space_id"]: f.get("name", f["space_id"]) for f in snap["floors"]}
    # 시간 접두도 같은 한 줄이므로 예산에서 먼저 뺀다 — 안 그러면 접두가 붙은 줄만 길어진다.
    prefix = _recency(best.get("at"), datetime.now(timezone.utc))
    lead = f"({prefix}) " if prefix else ""
    text = _sentence(best, space_id, name_of, budget - len(lead))
    if not text:
        return None
    hl: dict = {"text": lead + text}
    kind = _HIGHLIGHT_KIND.get(best.get("type"))
    if kind == "issue" and best.get("issue_id"):
        hl.update(kind="issue", issue_id=best["issue_id"])
    elif kind in ("reuse", "knowledge") and best.get("doc_id"):
        hl.update(kind=kind, doc_id=best["doc_id"])
    return hl


def _pick_highlight(events: list[dict], cross_team_docs: set[str] | None = None) -> dict | None:
    """org-safe 이벤트 중 1건 선정 (로비 ★). 랭킹: 크로스팀 reused > reused > ... , 동점은 최신순."""
    cross = cross_team_docs or set()
    return _best([e for e in events if _visible(e, "guest")], lambda e: _rank(e, cross))


# ── 뷰모델 ──────────────────────────────────────────────────


def lobby_view() -> dict:
    """로비 뷰모델 — 층 목록 + 회사 집계 + 오늘의 하이라이트."""
    snap = collector.snapshot()
    # 층 hover 카드의 1줄. 로비는 조직 공개 표면이므로 guest 풀로 뽑는다 (G7).
    # 랭킹으로 못 뽑으면 소스가 준 문장을 남긴다 (픽스처 하드코딩 보존).
    floors = [
        {**f, "highlight": (_room_highlight(f["space_id"], snap, "guest") or {}).get("text") or f.get("highlight")}
        for f in snap["floors"]
    ]
    return {
        "source": snap["source"],  # 화면에서 실데이터/픽스처 구분 표시용
        "floors": floors,
        "totals": snap["totals"],
        "tokens_saved_est": snap["tokens_saved_est"],  # 표시 시 '~' 필수
        "highlight": _pick_highlight(snap["events"], _cross_team_docs(snap)),
    }


def space_view(space_id: str, tier: str = "member") -> dict:
    """방 뷰모델 — work 상세. 프레즌스(online/offline)는 collector가 work의 최근 쓰기
    활동으로 이미 판정해 status·last_active_at에 채워둔다 (2026-07-19, life 프레즌스 대체)."""
    detail = collector.space_detail(space_id, tier)
    snap = collector.snapshot()
    # 칠판 하이라이트는 소스가 준 값을 쓰지 않고 여기서 다시 뽑는다 — 소스별 규칙 불일치 제거(스펙 §6).
    detail["highlight"] = _room_highlight(space_id, snap, tier, _BOARD_MAX)
    # 협업 지도 — 사람 노드 + 엣지 3종. 계산이 터져도 방은 열려야 하므로 빈 그래프로 강등한다.
    try:
        detail["collab"] = collab.build(detail, snap)
    except Exception:  # noqa: BLE001
        log.exception("협업 지도 계산 실패 (%s) — 빈 그래프로 강등", space_id)
        detail["collab"] = None
    return detail
