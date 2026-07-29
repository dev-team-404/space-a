"""a-hub(life) 읽기 — 사람의 이름·마스코트 이미지와 공통 신원을 가져온다.

스펙: docs/design/common/specs/2026-07-29-shared-identity-life-hub-lens.md
원칙: `life_url` 미설정이면 조용히 off(Hub-only 화면 그대로) · 실패는 경고 후 폴백(방은 계속
뜬다) · 조인 키는 Life가 준 `identity.hub_user_id`가 1순위이고 없을 때만 추론한다(§3.1).
"""

from __future__ import annotations

import logging
import threading
import time

import httpx

from . import settings

log = logging.getLogger(__name__)

TIMEOUT = 6.0  # 초 — 방 렌더를 붙잡지 않게 짧게. 실패하면 Hub-only로 폴백한다.
PEOPLE_TTL = 30.0  # 초 — 이름·마스코트는 자주 바뀌지 않는다(work 캐시와 같은 결)
IMAGE_TTL = 60.0  # 초 — 사진 교체가 늦게 보이면 "안 바뀐다"로 읽힌다. 대역폭은 ETag가 아낀다.

_lock = threading.Lock()
_people_cache: tuple[float, list[dict]] | None = None
_image_cache: dict[str, tuple[float, bytes]] = {}


def clear_cache() -> None:
    """설정이 바뀌면 호출 — 새 URL·토큰을 다음 요청부터 즉시 반영."""
    global _people_cache
    with _lock:
        _people_cache = None
        _image_cache.clear()


def _conf() -> tuple[str, dict] | None:
    """(base_url, headers) — life_url이 비어 있으면 None(연동 off)."""
    cfg = settings.get()
    url = (cfg.get("life_url") or "").strip().rstrip("/")
    if not url:
        return None
    headers: dict[str, str] = {}
    if cfg.get("life_token"):
        headers["Authorization"] = f"Bearer {cfg['life_token']}"
    if cfg.get("life_api_key"):
        headers["x-api-key"] = str(cfg["life_api_key"])
    return url, headers


def enabled() -> bool:
    return _conf() is not None


def normalize(value: str) -> str:
    """조인용 정규화 — 소문자 · `a-mate/` 접두어 제거 · 공백/`.`/`-`/`_` 제거 (§3.1)."""
    v = (value or "").strip().lower()
    if v.startswith("a-mate/"):
        v = v[len("a-mate/") :]
    for ch in (" ", ".", "-", "_"):
        v = v.replace(ch, "")
    return v


def alias_map() -> dict[str, list[str]]:
    """설정의 `life_alias`("닉네임=hub_user_id[|hub_user_id…],…") → 정규화 닉네임 → hub id 목록.

    한 사람이 허브 계정을 **여러 개** 갖는 일이 흔하다 — 등록 경로마다 id가 다르기 때문이다
    (a-mate · Claude Code 스킬 · 초기 자동배정 id). `|`로 이어 쓰면 전부 같은 사람으로 잇고,
    화면에서는 `collector._merge_people`이 한 줄로 합친다.
    """
    raw = (settings.get().get("life_alias") or "").strip()
    out: dict[str, list[str]] = {}
    for pair in raw.split(","):
        if "=" not in pair:
            continue
        nick, hubs = pair.split("=", 1)
        ids = [h.strip() for h in hubs.split("|") if h.strip()]
        if nick.strip() and ids:
            # 같은 닉네임이 두 번 나와도 덮지 않고 더한다("돌쇠=palen,돌쇠=coolfebreeze"도 통한다)
            out.setdefault(normalize(nick), []).extend(ids)
    return out


def people(force: bool = False) -> list[dict]:
    """Life 사람 목록 — 자기 자신(`/life/me`)까지 포함한다. 실패·미설정이면 빈 목록."""
    global _people_cache
    conf = _conf()
    if conf is None:
        return []
    with _lock:
        if not force and _people_cache is not None and time.time() - _people_cache[0] < PEOPLE_TTL:
            return list(_people_cache[1])
    url, headers = conf
    rows: list[dict] = []
    try:
        with httpx.Client(timeout=TIMEOUT, headers=headers) as c:
            r = c.get(f"{url}/life/people")
            r.raise_for_status()
            rows = list(r.json().get("people") or [])
            # /life/people은 호출자 자신을 제외한다 — 나도 팀원이므로 /life/me로 채운다.
            try:
                me = c.get(f"{url}/life/me").json()
                if me.get("agent_id"):
                    rows.append({
                        "agent_id": me["agent_id"],
                        "name": me.get("name", ""),
                        "life_id": me.get("my_life_id") or me.get("life_id"),
                        "identity": me.get("identity") or {},
                        "is_me": True,
                    })
            except Exception as e:  # noqa: BLE001 — 내 정보 실패는 목록 전체를 버릴 이유가 아니다
                log.info("life /life/me 조회 실패(목록만 사용): %s", e)
    except Exception as e:  # noqa: BLE001 — Life 실패는 Hub-only 폴백
        log.warning("life 사람 목록 수집 실패 (Hub-only로 폴백): %s", e)
        return []
    with _lock:
        _people_cache = (time.time(), rows)
    return list(rows)


def mascot_digest(agent_id: str) -> str:
    """이미지 지문 — Life가 identity로 주는 sha256(없으면 빈 문자열). ETag·캐시 무효화에 쓴다."""
    for person in people():
        if person.get("agent_id") == agent_id:
            return (person.get("identity") or {}).get("mascot_image_sha256") or ""
    return ""


def mascot_png(agent_id: str) -> bytes | None:
    """마스코트 PNG 원본. 없거나(404) 실패면 None — 호출자는 기존 로봇으로 폴백한다."""
    conf = _conf()
    if conf is None:
        return None
    with _lock:
        hit = _image_cache.get(agent_id)
        if hit is not None and time.time() - hit[0] < IMAGE_TTL:
            return hit[1] or None
    url, headers = conf
    png: bytes = b""
    try:
        with httpx.Client(timeout=TIMEOUT, headers=headers) as c:
            r = c.get(f"{url}/life/agents/{agent_id}/mascot-image")
            if r.status_code == 200:
                png = r.content
            elif r.status_code != 404:
                r.raise_for_status()
    except Exception as e:  # noqa: BLE001
        log.warning("마스코트 이미지 수집 실패 (%s): %s", agent_id, e)
        return None
    with _lock:
        _image_cache[agent_id] = (time.time(), png)  # 빈 값도 캐시 — 404 반복 조회 방지
    return png or None


def index_by_hub_user(rows: list[dict] | None = None) -> dict[str, dict]:
    """work 계정 id(정규화) → Life 사람. 연결 순서는 §3.1 — 앞이 이기면 뒤는 덮지 않는다.

    1) identity.hub_user_id (a-mate가 보낸 값 = 정답)
    2) 설정의 별칭 매핑
    3) 이름 정규화
    4) owner_os_user 정규화
    """
    rows = people() if rows is None else rows
    aliases = alias_map()
    out: dict[str, dict] = {}

    def put(key: str, person: dict) -> None:
        k = normalize(key)
        if k and k not in out:
            out[k] = person

    for person in rows:
        ident = person.get("identity") or {}
        if ident.get("hub_user_id"):
            put(ident["hub_user_id"], person)
    for person in rows:
        for hub in aliases.get(normalize(person.get("name", "")), []):
            put(hub, person)
    for person in rows:
        put(person.get("name", ""), person)
    for person in rows:
        ident = person.get("identity") or {}
        if ident.get("owner_os_user"):
            put(ident["owner_os_user"], person)
    return out
