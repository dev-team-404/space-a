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
import hashlib
import json
import logging
import re
import threading
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

import httpx

from . import settings, store, translator

log = logging.getLogger("alens.collector")

# repo 루트 기준 contracts/fixtures (backend/alens/ → 세 단계 위)
_FIXTURES = Path(__file__).resolve().parents[3] / "contracts" / "fixtures"
# 데모용 더미 데이터 (backend/dummy_data — _generate.py로 재생성)
_DUMMY_DIR = Path(__file__).resolve().parents[1] / "dummy_data"

# 설정(원천·a-hub URL·토큰·LLM 등)은 import 시 상수가 아니라 호출 시점에 settings에서 읽는다
# — 설정 창(POST /api/settings)에서 바꾸면 재시작 없이 반영된다.
#
# 수집·번역은 요청 스레드가 아니라 단일 백그라운드 워커가 한다:
#   - snapshot()은 최신 스냅숏(_latest)을 즉시 반환 → 요청이 절대 안 막힌다.
#   - _build_lock으로 빌드는 한 번에 하나만 → 요청이 몰려도 겹침 패스가 안 생긴다.
#   - 페이지 본문·번역은 store에 캐시 → updated_at이 그대로면 허브 재조회·LLM 둘 다 스킵.

_latest: dict | None = None            # 백그라운드 워커가 갱신하는 최신 스냅숏
_build_lock = threading.Lock()         # 스냅숏 빌드는 한 번에 하나만
_refresher_started = False
_refresher_lock = threading.Lock()

# 마지막 스냅숏을 디스크에 남겨, 재시작 직후에도 빈 화면 대신 직전 데이터를 즉시 제공한다.
_SNAPSHOT_FILE = Path(__file__).resolve().parents[1] / ".a-lens" / "snapshot.json"


def _persist(snap: dict) -> None:
    try:
        _SNAPSHOT_FILE.parent.mkdir(parents=True, exist_ok=True)
        _SNAPSHOT_FILE.write_text(json.dumps(snap, ensure_ascii=False), encoding="utf-8")
    except Exception:  # noqa: BLE001 — 스냅숏 캐시 파일 쓰기 실패는 무시
        pass


def _load_persisted() -> dict | None:
    try:
        if _SNAPSHOT_FILE.exists():
            return json.loads(_SNAPSHOT_FILE.read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        pass
    return None


def clear_cache() -> None:
    """설정 변경 시 — 최신 스냅숏을 버리고 즉시 백그라운드 재빌드를 건다(새 URL/원천 반영)."""
    global _latest
    _latest = None
    threading.Thread(target=_rebuild_once, daemon=True, name="alens-rebuild").start()


def _rebuild_once() -> None:
    global _latest
    try:
        _latest = _build_guarded()
        _persist(_latest)
    except Exception as e:  # noqa: BLE001
        log.warning("스냅숏 재빌드 실패: %s", e)


def _refresh_loop() -> None:
    global _latest
    while True:
        try:
            _latest = _build_guarded()
            _persist(_latest)
        except Exception as e:  # noqa: BLE001
            log.warning("스냅숏 갱신 실패: %s", e)
        time.sleep(max(5.0, settings.get()["cache_ttl"]))


def _ensure_refresher() -> None:
    global _refresher_started, _latest
    if _refresher_started:
        return
    with _refresher_lock:
        if not _refresher_started:
            if _latest is None:  # 재시작 직후: 디스크의 마지막 스냅숏을 즉시 띄운다
                _latest = _load_persisted()
            threading.Thread(target=_refresh_loop, daemon=True, name="alens-refresh").start()
            _refresher_started = True


def _build_current() -> dict:
    """현재 원천 설정에 맞는 스냅숏을 만든다 (백그라운드 워커에서만 호출)."""
    source = settings.get()["source"]
    if source == "hub":
        return _hub_snapshot()
    if source == "dummy":
        return _dummy_snapshot()
    if source == "fixtures":
        return _fixture_snapshot()
    try:
        return _merged_snapshot()
    except Exception as e:  # noqa: BLE001 — 허브 실패 시 더미만
        log.warning("허브 수집 실패, dummy_data만 표시: %s", e)
        return _dummy_snapshot()


def _build_guarded() -> dict:
    with _build_lock:  # 한 번에 하나의 빌드만 — 요청 폭주에도 겹침 패스가 안 생긴다
        return _build_current()


def _fixture(name: str) -> dict:
    return json.loads((_FIXTURES / name).read_text(encoding="utf-8"))


# a-mate가 같은 지식을 되찾으려고 본문 앞에 심는 기계 키(예: `[a-mate:R8:github]`).
# 사람이 보는 화면에서는 지운다 — 판별에는 필요하지만 읽는 사람에겐 노이즈다.
_MACHINE_MARKER = re.compile(r"^\s*\[a-mate:[^\]]+\]\s*")


def _display_title(title: str) -> str:
    return _MACHINE_MARKER.sub("", title or "")


def _hub_get(client: httpx.Client, path: str) -> dict:
    """커넥션 풀 공유 — 스냅숏 한 번에 3+N번 호출하므로 핸드셰이크를 재사용한다."""
    r = client.get(path)
    r.raise_for_status()
    return r.json()


# 개별 호출에서 "빈 값으로 강등" 처리할 예외 — HTTP 오류 + 비정상 JSON
_DEGRADE = (httpx.HTTPError, ValueError)

# 공간 이름 오버라이드 — 허브 원문이 mojibake(복구 불가)인 경우 표시용으로 교정한다.
_NAME_OVERRIDES = {"sw-innov": "S/W 혁신팀"}


def _space_name(sid: str, raw: str | None) -> str:
    return _NAME_OVERRIDES.get(sid) or (raw or sid)


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
    activity: dict[str, dict],
    agent_id: str | None,
    ts: str | None,
    kind: str,
    title: str,
    ref_id: str | None = None,
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
        activity[agent_id] = {"at": dt, "kind": kind, "title": title, "ref_id": ref_id}


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
    title = (item.get("title") or "").strip()
    # 말풍선도 카드처럼 짧게 — store에 생성된 명사형 제목이 있으면 그걸 쓴다(없으면 원문).
    ref_id = item.get("ref_id")
    if ref_id:
        st = store.get_store()
        if st is not None:
            if item.get("kind") == "issue":
                row = st.get_issue(ref_id)
                if row and row.get("gen_title"):
                    title = row["gen_title"]
            else:
                row = st.get(ref_id)
                if row and row.get("title"):
                    title = row["title"]
    title = title or "이름 없는 문서"
    # 말풍선(brief)은 가장 최근 업무의 한 줄 요약 — 제목 기반. 길면 렌더러가 말줄임 처리.
    # 상세(detail)는 문장으로 풀어쓴다.
    if item.get("kind") == "issue":
        brief = f"‘{title}’ 이슈 등록"
        detail = f"최근에 ‘{title}’ 이슈를 등록했어요."
    else:  # knowledge
        brief = f"‘{title}’ 지식 작성"
        detail = f"최근에 ‘{title}’ 지식을 작성했어요."
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


def _issue_doc(it: dict, member_name: dict[str, str]) -> dict:
    """이슈 VM + 번역(분류·요약·서사). 제목이 그대로면 캐시 사용 — 상태 변화로는 재번역 안 함."""
    vm = _issue_vm(it, member_name)
    title = it.get("title", "")
    st = store.get_store()
    row = st.get_issue(vm["issue_id"]) if st is not None else None
    if row and row.get("title") == title and row.get("summary"):
        vm["category"], vm["summary"], vm["narrative"] = (
            row.get("category"),
            row.get("summary"),
            row.get("narrative"),
        )
        vm["title"] = row.get("gen_title") or title
        return vm
    # 이슈는 본문이 없어 제목을 내용으로 번역한다(제목을 body 자리에도 넣어 맥락 확보).
    res, model = translator.translate_text(title, title, "issue")
    vm["category"], vm["summary"], vm["narrative"] = (
        res["category"],
        res["summary"],
        res.get("narrative"),
    )
    vm["title"] = res.get("title") or title
    if st is not None:
        st.upsert_issue(
            {
                "issue_id": vm["issue_id"],
                "title": title,
                "gen_title": res.get("title"),
                "category": res["category"],
                "summary": res["summary"],
                "narrative": res.get("narrative"),
                "model": model,
                "translated_at": datetime.now(timezone.utc).isoformat(),
            }
        )
    return vm


# ── 지식 문서 뷰 (본문+번역, store 증분 캐시) ────────────────


def _page_doc(client: httpx.Client, node: dict, space_id: str, member_name: dict[str, str]) -> dict:
    """트리 노드 하나 → 지식 문서 뷰(body·category·summary·narrative).

    store 캐시로 증분 처리: 같은 updated_at이면 허브 /pages 재조회·LLM 번역을 둘 다 건너뛴다.
    본문만 캐시에 없고(구 스키마 등) 번역은 있으면, 본문만 한 번 받아 채우고 LLM은 스킵한다."""
    page_id = node["page_id"]
    updated_at = node.get("updated_at") or node.get("created_at") or ""
    doc = {"doc_id": page_id, "title": node.get("title", ""), "reuse_count": 0, "cited_by": []}

    st = store.get_store()
    row = st.get(page_id) if st is not None else None
    fresh = bool(row and row.get("updated_at") == updated_at and row.get("summary"))

    # 완전 캐시(본문까지) — 허브·LLM 모두 스킵
    if fresh and row.get("body") is not None:
        body = row.get("body") or ""
        doc["title"] = row.get("title") or doc["title"]
        doc["body"] = body
        doc["visibility"] = row.get("visibility") or "org"
        doc["author_agent"] = row.get("author_agent") or ""
        doc["category"] = row.get("category")
        doc["summary"] = row.get("summary") or body[:120]
        doc["narrative"] = row.get("narrative")
        return doc

    # 본문이 필요 → 허브 조회 (읽기 실패는 잠금으로 강등)
    try:
        page = _hub_get(client, f"/pages/{page_id}")
    except _DEGRADE:
        doc.update(body="", visibility="space", author_agent="", summary="", category=None, narrative=None)
        return doc

    body = page.get("body", "")
    visibility = page.get("visibility", "org")
    # 작성자: created_by_name(허브가 내려주면) 우선, 없으면 멤버 목록 이름, 그마저 없으면 agent_id.
    creator = page.get("created_by")
    author = page.get("created_by_name") or member_name.get(creator, creator) or ""

    if fresh:  # 번역은 이미 있으니 LLM 스킵, 본문만 채워 캐시 보강
        title_out = row.get("title") or node.get("title", "")
        category, summary, narrative = row.get("category"), row.get("summary"), row.get("narrative")
        model = row.get("model") or "cache"
    else:  # 새/변경 문서만 LLM(또는 규칙)로 번역
        res, model = translator.translate_text(
            node.get("title", ""), body, node.get("source", "authored")
        )
        title_out = res.get("title") or node.get("title", "")
        category, summary, narrative = res["category"], res["summary"], res.get("narrative")

    doc["title"] = title_out
    doc["body"] = body
    doc["visibility"] = visibility
    doc["author_agent"] = author
    doc["category"] = category
    doc["summary"] = summary or body[:120]
    doc["narrative"] = narrative

    if st is not None:
        st.upsert(
            {
                "page_id": page_id,
                "space_id": space_id,
                "updated_at": updated_at,
                "source_hash": hashlib.sha256((body or "").encode("utf-8")).hexdigest(),
                "body": body,
                "visibility": visibility,
                "author_agent": author,
                "title": title_out,
                "category": category,
                "summary": summary,
                "narrative": narrative,
                "model": model,
                "translated_at": datetime.now(timezone.utc).isoformat(),
            }
        )
    return doc


# ── 허브 스냅숏 ──────────────────────────────────────────────


def _hub_snapshot() -> dict:
    cfg = settings.get()
    headers = {"Authorization": f"Bearer {cfg['work_token']}"} if cfg["work_token"] else {}
    if cfg["work_api_key"]:
        headers["x-api-key"] = cfg["work_api_key"]
    floors: list[dict] = []
    details: dict[str, dict] = {}
    all_pages: list[dict] = []
    totals = {"issues": 0, "knowledge": 0, "skills": 0, "reuses": 0}
    collected_at = datetime.now(timezone.utc)
    online_cutoff = collected_at.timestamp() - cfg["presence_window"]

    with httpx.Client(base_url=cfg["work_url"].rstrip("/"), headers=headers, timeout=8) as client:
        spaces_raw = _hub_get(client, "/spaces").get("spaces", [])

        # 재사용(북극성 지표) — 허브의 GET /reuse-events. 구버전 허브엔 없으므로 강등 처리.
        # 한 번만 받아 space별로 집계한다(공간마다 재호출하지 않는다).
        try:
            hub_reuse = _hub_get(client, "/reuse-events?limit=200").get("reuse_events", [])
        except _DEGRADE as e:
            log.warning("reuse-events 수집 실패(구버전 허브면 정상): %s", e)
            hub_reuse = []
        space_reuse_counts: dict[str, int] = {}
        for ev in hub_reuse:
            sid_ = ev.get("space_id")
            if sid_:
                space_reuse_counts[sid_] = space_reuse_counts.get(sid_, 0) + 1

        for i, s in enumerate(spaces_raw):
            sid = s.get("id")
            if not sid:
                continue
            sname = _space_name(sid, s.get("name"))
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
                    "knowledge", p.get("title", ""), p.get("page_id"),
                )
            for it in issues:
                _bump_activity(
                    last_write, it.get("opened_by"), it.get("updated_at") or it.get("created_at"),
                    "issue", it.get("title", ""), it.get("issue_id"),
                )

            resolved = sum(1 for it in issues if it.get("status") == "resolved")
            knowledge = len(pages)
            totals["issues"] += len(issues)
            totals["knowledge"] += knowledge
            reuse_count = space_reuse_counts.get(sid, 0)
            totals["reuses"] += reuse_count

            # 활동 열기(0~3): 실측 근거가 생기기 전까지는 축적량 기반 근사
            volume = knowledge + len(issues)
            activity = 0 if volume == 0 else 1 if volume < 3 else 2 if volume < 8 else 3

            floors.append(
                {
                    "space_id": sid,
                    "name": sname,
                    "floor": i + 1,
                    "activity": activity,
                    "stats": {"knowledge": knowledge, "resolved": resolved, "reuse": reuse_count},
                    "highlight": None,  # 서버 서사 부재 — 아래 활동 피드에서 결정론 선정
                }
            )
            # 지식 본문+번역: 페이지별 store 캐시. updated_at이 그대로면 허브 재조회·LLM 둘 다 스킵.
            knowledge_docs = [_page_doc(client, p, sid, member_name) for p in pages]

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
                "issues": [_issue_doc(it, member_name) for it in issues],
                "knowledge": knowledge_docs,
            }
            for p in pages:
                all_pages.append({**p, "space_id": sid, "space_name": sname})

    # 활동 피드 합성: knowledge_created만 (타임스탬프·재사용 피드는 #40·후속 대기).
    # 서사 문장은 구조 필드로 소비자가 조합 — 계약 consumerAutonomy가 허용하는 방식.
    all_pages.sort(key=lambda p: _id_seq(p["page_id"]), reverse=True)
    page_title = {p["page_id"]: p.get("title", p["page_id"]) for p in all_pages}
    events = [
        {
            "type": "knowledge_created",
            "doc_id": p["page_id"],
            "space_id": p["space_id"],
            "summary": f"『{p.get('title', p['page_id'])}』 지식이 {p['space_name']}에 등록되었습니다",
        }
        for p in all_pages
    ]
    # 재사용 이벤트 — 로비 하이라이트는 reused를 knowledge_created보다 위에 랭크한다(pipeline).
    # 실데이터에서도 하이라이트가 뜨려면 여기서 실제 이벤트를 넣어줘야 한다.
    reuse_events = [
        {
            "reuse_id": ev.get("reuse_id"),
            "doc_id": ev.get("page_id"),
            "space_id": ev.get("space_id"),
            "by": ev.get("cited_by_name") or ev.get("cited_by"),
            "cross_team": bool(ev.get("cross_team")),
            "at": ev.get("created_at"),
        }
        for ev in hub_reuse
    ]
    for ev in reuse_events:
        title = _display_title(page_title.get(ev["doc_id"], ev["doc_id"]))
        scope = "다른 팀의" if ev["cross_team"] else "팀의"
        events.append(
            {
                "type": "reused",
                "doc_id": ev["doc_id"],
                "space_id": ev["space_id"],
                "at": ev["at"],
                "summary": f"{ev['by']}님의 에이전트가 {scope} 지식 『{title}』을 재사용했습니다",
            }
        )

    return {
        "source": "hub",
        "collected_at": collected_at.isoformat(),  # 타임스탬프 부재(#40) 동안 시각 필드 대체재 + 프레즌스 기준 시각
        "floors": floors,
        "details": details,
        "totals": totals,
        "tokens_saved_est": None,  # 실측 재료 없음 (#40 후순위 항목)
        "events": events,
        "reuse_events": reuse_events,
    }


# ── 더미 스냅숏 (backend/dummy_data — 데모용 fake 데이터) ─────
#
# 시각은 파일에 "몇 분 전(min_ago)" 오프셋으로 저장돼 있어 로드 시점 기준으로
# 환산한다 — 언제 띄워도 프레즌스(온라인/오프라인)가 살아 있는 데모가 된다.


def _dummy_snapshot() -> dict:
    now = datetime.now(timezone.utc)
    cutoff = now.timestamp() - settings.get()["presence_window"]
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
    """최신 스냅숏을 즉시 반환 — 수집·번역은 백그라운드 워커가 하므로 요청은 안 막힌다.

    첫 빌드 전이면 빠른 폴백(더미/픽스처)을 임시로 주고, 곧 실데이터로 교체된다."""
    _ensure_refresher()
    latest = _latest
    if latest is not None:
        return latest
    source = settings.get()["source"]
    try:
        return _fixture_snapshot() if source == "fixtures" else _dummy_snapshot()
    except Exception:  # noqa: BLE001 — 폴백조차 실패하면 빈 스냅숏
        return {
            "source": "loading",
            "collected_at": None,
            "floors": [],
            "details": {},
            "totals": {},
            "tokens_saved_est": None,
            "events": [],
            "reuse_events": [],
        }


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
