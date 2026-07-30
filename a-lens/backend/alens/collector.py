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
from . import life_client

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

# ── 안 되는 호출 기억 ────────────────────────────────────────
#
# 폴링마다 같은 실패를 되풀이하던 두 부류를 기억해두고 한동안 건너뛴다.
#   403/401 — 허브의 issues·members는 멤버십이 필요한데 /spaces는 멤버십 정보를 안 주고
#             ("내 멤버십" 엔드포인트도 없다) 비멤버 공간을 미리 걸러낼 방법이 없다.
#   404     — 배포된 허브에 아직 없는 엔드포인트(계약엔 있음). 예: /reuse-events
# 둘 다 상태가 아니라 사실이므로 재시도해도 답이 같다 — 실측 폴링 1회당 헛요청 11건이었다.
_SOFT_FAIL_TTL = 1800  # 30분 뒤 재시도 — 멤버 추가·허브 배포가 있어도 결국 반영된다
_SOFT_FAIL_STATUS = (401, 403, 404)
_soft_failed: dict[str, float] = {}


def _auth_fp(cfg: dict) -> str:
    """자격증명 지문 — 토큰이 바뀌면 권한 캐시가 자연히 무효화된다(새 토큰은 권한이 다르다)."""
    raw = f"{cfg.get('work_token', '')}|{cfg.get('work_api_key', '')}"
    return hashlib.sha256(raw.encode()).hexdigest()[:8]


def _soft_get(client: httpx.Client, path: str, key: str, what: str, ctx: str = "") -> dict | None:
    """실패를 기억하는 GET. 권한 없음·엔드포인트 없음은 TTL 동안 다시 묻지 않는다.

    None을 주면 호출부가 빈 값으로 강등한다 — 비멤버 공간도 층 목록엔 남고 공개 지식은 보인다."""
    until = _soft_failed.get(key)
    if until is not None:
        if time.monotonic() < until:
            return None
        del _soft_failed[key]  # TTL 만료 — 이번엔 다시 물어본다
    try:
        return _hub_get(client, path)
    except httpx.HTTPStatusError as e:
        if e.response.status_code in _SOFT_FAIL_STATUS:
            # 개별 로그는 안 찍는다 — 스냅숏 끝에서 _note_skips가 한 줄로 요약한다.
            _soft_failed[key] = time.monotonic() + _SOFT_FAIL_TTL
            return None
        log.warning("%s 수집 실패%s: %s", what, ctx, e)
        return None
    except _DEGRADE as e:
        log.warning("%s 수집 실패%s: %s", what, ctx, e)
        return None


_last_skip_note = ""


def _note_skips(auth_fp: str) -> None:
    """건너뛴 호출을 한 줄로 요약. 내용이 바뀔 때만 찍는다 — 폴링마다 같은 줄을 반복하지 않되,
    "이 방은 왜 이슈가 0건인가"를 로그에서 알 수 있게 남긴다."""
    global _last_skip_note
    prefix = auth_fp + ":"
    items = sorted(k[len(prefix) :] for k in _soft_failed if k.startswith(prefix))
    note = ", ".join(items)
    if note == _last_skip_note:
        return
    if note:
        log.warning(
            "권한 없음·미배포로 건너뛰는 호출 %d건: %s (%d분마다 재시도)",
            len(items),
            note,
            _SOFT_FAIL_TTL // 60,
        )
    else:
        log.warning("건너뛰던 호출이 모두 복구됐습니다")
    _last_skip_note = note

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


def _presence_status(person: dict | None, seen: dict | None, online_cutoff: float) -> str:
    """온라인 판정 — Life의 `last_seen`을 1순위로, 없으면 종전처럼 허브 write 시각을 쓴다.

    **왜 Life가 1순위인가**: 허브 write(page·issue)는 사람당 하루 1~3건뿐이고 a-mate가 자정
    직후에 몰아서 발행한다. 판정 창이 1시간(`A_LENS_PRESENCE_WINDOW`)이므로, write만 보면
    하루 23시간을 offline으로, 그리고 정작 켜지는 1시간은 새벽으로 표시했다(2026-07-30 확인).
    Life 쪽은 a-mate가 스캔마다(≈분당 1회) 인증 호출을 하므로 분 단위로 신선하다.

    `connected`만으로는 판정하지 않는다 — 그 값은 register/enter에서 True가 되고 수동
    disconnect로만 False가 되므로, 한 번 등록한 사람은 영구히 online으로 보인다.

    `last_seen` 키가 아예 없으면 구버전 Life 서버다 — 조용히 종전 방식으로 강등한다.
    """
    if person is not None and "last_seen" in person:
        if not person.get("connected", True):
            return "idle"  # 사용자가 명시적으로 연결을 끊었다
        last_seen = _parse_ts(person.get("last_seen"))
        return "working" if last_seen is not None and last_seen.timestamp() >= online_cutoff else "idle"
    return "working" if seen is not None and seen["at"].timestamp() >= online_cutoff else "idle"


def _life_only_agents(life_people: list[dict], used: set[str], online_cutoff: float) -> list[dict]:
    """Hub 계정과 못 이은 Life 사람 — 이름·마스코트만 있고 활동 정보는 빈 카드로 붙인다.

    사람을 화면에서 지우지 않는다: '연결 안 됨'은 데이터 상태이지 그 사람이 없는 게 아니다(§3.1 ④).
    허브 활동은 없어도 **접속 여부는 Life가 알고 있다** — 그건 채워준다."""
    return [
        {
            "agent_id": f"life:{person['agent_id']}",
            "name": person.get("name", person["agent_id"]),
            "status": _presence_status(person, None, online_cutoff),
            "last_active_at": None,
            "recent_activity": None,
            "life_agent_id": person["agent_id"],
            "mascot_url": f"/api/life-mascot/{person['agent_id']}",
            "via": "life",  # Hub 활동이 없는 이유 = 아직 연결 안 됨
            "owner": (person.get("identity") or {}).get("owner_full_name", ""),
        }
        for person in life_people
        if person["agent_id"] not in used
    ]


def _registered_by_amate(row: dict) -> bool:
    """허브 표시 이름의 `a-mate/` 접두어 = a-mate가 등록한 계정. 다른 경로(스킬·REST)는 접두어가 없다."""
    return (row.get("hub_name") or row.get("name") or "").startswith("a-mate/")


def _one_person(group: list[dict]) -> dict:
    """같은 사람의 허브 계정 여러 줄 → 한 줄. 대표는 a-mate 계정, 활동은 전부 합친다."""
    if len(group) == 1:
        return group[0]
    # 안정 정렬 2회 = "a-mate로 등록한 계정 우선, 그 안에서 최근 활동 우선"
    ranked = sorted(group, key=lambda r: r.get("last_active_at") or "", reverse=True)
    ranked.sort(key=lambda r: not _registered_by_amate(r))
    rep, *others = ranked
    latest = max(group, key=lambda r: r.get("last_active_at") or "")
    return {
        **rep,
        # 대표 계정이 조용하고 활동은 옛 계정에 있을 수 있다 — 활동은 계정이 아니라 사람의 것이다.
        "status": "working" if any(r.get("status") == "working" for r in group) else "idle",
        "last_active_at": latest.get("last_active_at"),
        "recent_activity": latest.get("recent_activity"),
        # 흡수한 허브 계정 id. 원장의 작성자 id는 그대로이므로 추적이 끊기지 않는다.
        "merged_ids": [r["agent_id"] for r in others],
    }


def _merge_people(rows: list[dict]) -> list[dict]:
    """한 사람이 허브 계정 여러 개로 여러 줄 뜨는 것을 막는다 — Life 신원이 같으면 한 사람이다.

    계정이 갈라지는 건 등록 경로가 여럿이라서다(a-mate · Claude Code 스킬 · 초기 자동배정 id).
    허브 원장을 고치는 건 팀 공용 데이터를 건드리는 일이라, 보는 층에서만 합친다.

    `life_agent_id`가 없는 줄은 합치지 않는다 — 같은 사람인지 알 방법이 없고, 추측으로 남의
    활동을 한 사람에게 몰아주는 것이 두 줄로 보이는 것보다 나쁘다.
    """
    groups: dict[str, list[dict]] = {}
    for row in rows:
        if row.get("life_agent_id"):
            groups.setdefault(row["life_agent_id"], []).append(row)
    out: list[dict] = []
    emitted: set[str] = set()
    for row in rows:
        life_id = row.get("life_agent_id")
        if not life_id:
            out.append(row)
        elif life_id not in emitted:  # 그룹의 첫 줄 자리에 합친 한 줄을 놓는다(순서 유지)
            emitted.add(life_id)
            out.append(_one_person(groups[life_id]))
    return out


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
    # author_agent_id(작성자 원본 id)는 나중에 추가된 컬럼이라 기존 캐시 행엔 없다. 사람별 작업
    # 기록이 이름 문자열에 의존하지 않게, 이 값이 빈 행은 페이지를 한 번만 다시 받아 채운다.
    # (번역은 캐시를 그대로 재사용하므로 LLM 비용은 없다 — 아래 need_translate 참고)
    needs_author_id = False  # 아래에서 creator를 확인한 뒤 결정한다

    # 작성자 표시 이름은 캐시에 기대지 않고 매번 이름표에서 다시 만든다 — 방에 뜬 사람 이름과
    # 같아야 하기 때문이다(§3.2·§3.4). 캐시에 굳은 허브 계정 이름을 쓰면 문서함·지식 재사용의
    # 사람 선택이 방에 뜬 닉네임과 어긋난다. 트리 노드가 created_by를 주므로 추가 조회는 없다.
    creator = node.get("created_by") or ""
    author = member_name.get(creator, "") or creator
    # 트리 노드에도, 캐시에도 작성자 원본 id가 없을 때만 페이지를 한 번 다시 받는다
    # (사람별 작업 기록을 이름 문자열에 의존시키지 않기 위해. 번역 캐시는 재사용 — LLM 비용 없음)
    needs_author_id = bool(row) and not creator and not (row or {}).get("author_agent_id")

    # 완전 캐시(본문까지) — 허브·LLM 모두 스킵
    if fresh and not needs_author_id and row.get("body") is not None:
        body = row.get("body") or ""
        doc["title"] = row.get("title") or doc["title"]
        doc["body"] = body
        doc["visibility"] = row.get("visibility") or "org"
        doc["author_agent"] = author or row.get("author_agent") or ""
        doc["author_agent_id"] = creator or row.get("author_agent_id") or ""
        doc["category"] = row.get("category")
        doc["summary"] = row.get("summary") or body[:120]
        doc["narrative"] = row.get("narrative")
        return doc

    # 본문이 필요 → 허브 조회 (읽기 실패는 잠금으로 강등)
    try:
        page = _hub_get(client, f"/pages/{page_id}")
    except _DEGRADE:
        doc.update(body="", visibility="space", author_agent=author, summary="", category=None, narrative=None)
        return doc

    body = page.get("body", "")
    visibility = page.get("visibility", "org")
    # 트리 노드에 created_by가 없는 옛 데이터만 페이지 응답으로 보충한다. 이름표(사람 이름)가
    # 허브의 created_by_name(계정 이름)을 이긴다 — 위 주석과 같은 이유.
    creator = creator or page.get("created_by") or ""
    author = author or member_name.get(creator, "") or page.get("created_by_name") or creator
    # 사람별 작업 기록은 표시 이름이 아니라 **원본 agent_id**로 묶는다 — 이름은 Life 이름으로
    # 덮이거나 'a-mate/' 접두어가 붙어 흔들린다.
    doc["author_agent_id"] = creator

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
                "author_agent_id": creator or "",
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
    auth_fp = _auth_fp(cfg)  # 권한 캐시 키에 섞는다 — 토큰이 바뀌면 캐시가 무효화된다
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
        reuse_res = _soft_get(client, "/reuse-events?limit=200", f"{auth_fp}:reuse-events", "reuse-events")
        hub_reuse = (reuse_res or {}).get("reuse_events", [])
        space_reuse_counts: dict[str, int] = {}
        for ev in hub_reuse:
            sid_ = ev.get("space_id")
            if sid_:
                space_reuse_counts[sid_] = space_reuse_counts.get(sid_, 0) + 1

        # 공통 신원 — Life 사람 목록은 공간과 무관하므로 한 번만 읽어 인덱스를 만든다.
        # Life 미설정·실패면 빈 값이고, 아래 조인은 전부 no-op이 된다(Hub-only 폴백).
        life_people = life_client.people()
        life_index = life_client.index_by_hub_user(life_people)
        life_used: set[str] = set()  # Hub와 매칭된 Life agent_id — 남은 사람은 Life-only로 붙인다

        for i, s in enumerate(spaces_raw):
            sid = s.get("id")
            if not sid:
                continue
            sname = _space_name(sid, s.get("name"))
            # 비멤버 공간·권한 부족·깨진 응답은 빈 목록으로 강등 (전체 스냅숏은 살린다)
            where = f" ({sid})"
            tree = _soft_get(client, f"/spaces/{sid}/tree", f"{auth_fp}:tree:{sid}", "tree", where)
            pages = _flatten_tree((tree or {}).get("tree", []))
            issues_res = _soft_get(client, f"/issues?space_id={sid}", f"{auth_fp}:issues:{sid}", "issues", where)
            issues = (issues_res or {}).get("issues", [])
            members_res = _soft_get(client, f"/spaces/{sid}/members", f"{auth_fp}:members:{sid}", "members", where)
            members = (members_res or {}).get("members", [])
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
            def _agent(m: dict) -> dict:
                seen = last_write.get(m["agent_id"])
                # 공통 신원 — Life에 같은 사람이 있으면 그 사람의 이름·마스코트로 보여준다.
                # 매칭 못 해도 Hub 정보는 그대로 남는다(§3.1 실패는 오류가 아니다).
                person = life_index.get(life_client.normalize(m["agent_id"])) or life_index.get(
                    life_client.normalize(m.get("name", ""))
                )
                out = {
                    "agent_id": m["agent_id"],
                    "name": m.get("name", m["agent_id"]),
                    "status": _presence_status(person, seen, online_cutoff),
                    # 접속 여부와 별개인 "마지막으로 무엇을 했나" — 허브 write가 정답이다.
                    "last_active_at": seen["at"].isoformat() if seen else None,
                    # 사람이 읽을 최근 활동 문장 (말풍선=brief, 상세=detail). 번역은 _humanize_activity.
                    "recent_activity": _humanize_activity(seen) if seen else None,
                }
                if person:
                    life_used.add(person["agent_id"])
                    out["life_agent_id"] = person["agent_id"]
                    if person.get("name"):
                        out["hub_name"] = out["name"]  # 덮기 전 허브 표시 이름(대표 계정 판정·툴팁용)
                        out["name"] = person["name"]
                    ident = person.get("identity") or {}
                    if ident.get("owner_full_name"):
                        out["owner"] = ident["owner_full_name"]
                    # 이미지 유무는 프록시가 판정한다(구버전 Life는 identity를 안 주므로 URL을 항상 준다).
                    out["mascot_url"] = f"/api/life-mascot/{person['agent_id']}"
                return out

            agents = _merge_people([_agent(m) for m in members])
            # 합쳐진 사람은 흡수된 계정 id로 남긴 기록도 그 사람 이름으로 읽혀야 한다 — 방에는
            # "돌쇠"인데 이슈 담당자는 "a-mate/coolfebreeze"로 뜨면 같은 혼동이 되돌아온다.
            for row in agents:
                if row.get("life_agent_id"):
                    for hub_id in (row["agent_id"], *row.get("merged_ids", [])):
                        member_name[hub_id] = row["name"]

            # 지식 본문+번역: 페이지별 store 캐시. updated_at이 그대로면 허브 재조회·LLM 둘 다 스킵.
            knowledge_docs = [_page_doc(client, p, sid, member_name) for p in pages]

            details[sid] = {
                "space_id": sid,
                "agents": agents + _life_only_agents(life_people, life_used, online_cutoff),
                "issues": [_issue_doc(it, member_name) for it in issues],
                "knowledge": knowledge_docs,
            }
            for p in pages:
                all_pages.append({**p, "space_id": sid, "space_name": sname})

    _note_skips(auth_fp)

    # 활동 피드 합성: knowledge_created만 (타임스탬프·재사용 피드는 #40·후속 대기).
    # 서사 문장은 구조 필드로 소비자가 조합 — 계약 consumerAutonomy가 허용하는 방식.
    all_pages.sort(key=lambda p: _id_seq(p["page_id"]), reverse=True)
    page_title = {p["page_id"]: p.get("title", p["page_id"]) for p in all_pages}
    # 문서 → 저자 표시명·원천 방. 칠판 하이라이트가 사람을 주인공으로 세울 재료
    # (specs/2026-07-30-room-board-highlight.md §3). _page_doc이 이미 표시명을 풀어놨다.
    doc_author = {d["doc_id"]: d.get("author_agent", "") for det in details.values() for d in det["knowledge"]}
    doc_space = {d["doc_id"]: det["space_id"] for det in details.values() for d in det["knowledge"]}
    events = [
        {
            "type": "knowledge_created",
            "doc_id": p["page_id"],
            "space_id": p["space_id"],
            "actor": doc_author.get(p["page_id"], ""),
            "title": _display_title(p.get("title", p["page_id"])),
            "summary": f"『{p.get('title', p['page_id'])}』 지식이 {p['space_name']}에 등록되었습니다",
        }
        for p in all_pages
    ]
    # 재사용 이벤트 — 로비 하이라이트는 reused를 knowledge_created보다 위에 랭크한다(pipeline).
    # 실데이터에서도 하이라이트가 뜨려면 여기서 실제 이벤트를 넣어줘야 한다.
    # source/consumer 방을 나눠 실는다: 허브는 재사용이 '일어난' 방(space_id)만 주므로
    # 원천 방은 문서 소유 방에서 유도한다 — 이게 있어야 (1) 방 사이드바 '지식 재사용' 필터가
    # 걸리고 (2) pipeline의 크로스팀 가점이 실데이터에서도 작동한다.
    reuse_events = [
        {
            "reuse_id": ev.get("reuse_id"),
            "doc_id": ev.get("page_id"),
            "space_id": ev.get("space_id"),
            "source_space": doc_space.get(ev.get("page_id")),
            "consumer_space": ev.get("space_id"),
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
                "source_space": ev["source_space"],
                "consumer_space": ev["consumer_space"],
                "actor": doc_author.get(ev["doc_id"], ""),  # 재사용된 지식을 '쓴' 사람
                "by": ev["by"],  # 재사용'한' 사람
                "title": title,
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
                    "actor": d.get("author", ""),
                    "title": d["title"],
                    "at": ago(d.get("created_min_ago", 0)).isoformat(),
                    "summary": f"『{d['title']}』 지식이 {sp['name']}에 등록되었습니다",
                    "demo": True,
                }
            )

        # 재사용된 지식의 저자 — 칠판 문장의 주인공. 재사용 이벤트는 이 팀 문서에서만
        # 생성되므로(_generate.gen_reuse_events) 같은 파일의 knowledge에서 찾으면 된다.
        author_of = {d["doc_id"]: d.get("author", "") for d in raw.get("knowledge", [])}
        title_of = {d["doc_id"]: d["title"] for d in raw.get("knowledge", [])}
        for r in raw.get("reuse_events", []):
            at = ago(r.get("min_ago", 0)).isoformat()
            reuse_events.append({**{k: v for k, v in r.items() if k != "min_ago"}, "at": at, "demo": True})
            events.append(
                {
                    "type": "reused",
                    "doc_id": r.get("doc_id"),
                    "space_id": r.get("consumer_space"),
                    "source_space": r.get("source_space") or sid,
                    "consumer_space": r.get("consumer_space"),
                    "actor": author_of.get(r.get("doc_id"), ""),
                    "title": title_of.get(r.get("doc_id"), ""),
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
                "highlight": None,  # 칠판·hover 카드 문장은 pipeline이 이벤트 풀에서 선정한다
                "demo": True,
            }
        )
        details[sid] = {
            "space_id": sid,
            "agents": agents,
            "issues": issues,
            "knowledge": docs,
            "visits": raw.get("visits"),
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
    reuse = _fixture("reuse-events.json")["events"]
    # 활동 피드의 reused 이벤트엔 원천/소비 방이 없다(space_id = 재사용이 일어난 방뿐).
    # 재사용 피드에서 doc_id로 이어 붙여야 방 칠판의 관점·크로스팀 가점이 픽스처에서도 작동한다.
    reuse_by_doc = {r["doc_id"]: r for r in reuse if r.get("doc_id")}
    events = []
    for e in _fixture("activity.json")["events"]:
        r = reuse_by_doc.get(e.get("doc_id")) if e.get("type") == "reused" else None
        events.append({**e, "source_space": r["source_space"], "consumer_space": r["consumer_space"]} if r else e)
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
        "events": events,
        "reuse_events": reuse,
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
