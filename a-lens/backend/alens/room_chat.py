"""방 안 오브젝트 잡담 — 창문·정수기를 클릭하면 캐릭터가 주고받는 짧은 대사 2~3줄.

스펙: docs/design/a-lens/specs/2026-07-28-room-object-smalltalk-design.md
원칙: **화면에 실제로 그려진 것**만 근거로 쓴다(창밖 풍경은 방 프리셋, 정수기는 정수기) ·
모르는 정보(실제 기상·날짜)는 단정하지 않는다 · LLM 미설정·실패 시 결정론 폴백 ·
(구역 × 풍경 × 시간대)당 최대 5세트만 생성해 재사용.
"""

from __future__ import annotations

import logging
import random
from datetime import datetime, timedelta, timezone

import httpx

from . import settings
from .translator import _extract_json

log = logging.getLogger(__name__)

# 프리셋별 창밖 풍경 — manifest의 life.rN과 짝. 창문 프롬프트의 유일한 사실 근거다.
VIEWS: dict[str, str] = {
    "r1": "단풍이 물든 숲",
    "r2": "벚꽃이 핀 도시 풍경",
    "r3": "불빛이 켜진 밤 도시 야경",
    "r4": "구름 몇 점 뜬 맑은 하늘과 초원",
    "r5": "늦은 밤 도시 야경",
}
DEFAULT_VIEW = "r1"

SPOTS = ("window", "water")
DEFAULT_SPOT = "window"

KST = timezone(timedelta(hours=9))

# 폴백 문구 — LLM이 없어도 오브젝트는 반응한다.
FALLBACK_WINDOW: dict[str, list[list[str]]] = {
    "r1": [
        ["밖에 단풍 다 들었네요", "이런 날 하루 재끼고 걷고 싶다"],
        ["낙엽 밟는 소리 좋을 텐데", "커피라도 들고 나갈까요"],
    ],
    "r2": [
        ["벚꽃 지기 전에 나가야 하는데", "올해도 모니터로만 보네요"],
        ["바람에 꽃잎 날리는 거 보이죠", "점심은 밖에서 먹읍시다"],
    ],
    "r3": [
        ["밖은 벌써 캄캄하네요", "오늘은 정시에 나가보죠"],
        ["야경은 예쁜데 왜 아직 여기 있죠", "이거만 끝내고 갑니다"],
    ],
    "r4": [
        ["하늘 완전 맑네요", "이런 날은 밖에서 놀아야 하는데"],
        ["바람도 좋아 보이는데요", "잠깐 나가서 바람 좀 쐬죠"],
    ],
    "r5": [
        ["도시가 다 잠든 것 같네요", "우리도 이제 접읍시다"],
        ["이 시간에 창밖 보면 괜히 짠해요", "마지막 하나만 보고 갈게요"],
    ],
}
FALLBACK_WATER: list[list[str]] = [
    ["물 한 잔 하고 하세요", "힘들죠? 좀 쉬었다 합시다"],
    ["한 잔 받아가세요", "쉬는 것도 일입니다"],
    ["목 마르지 않아요?", "잠깐 앉아서 숨 좀 돌리죠"],
]

_SYSTEM = (
    "너는 협업 공간을 배경으로 한 픽셀 게임의 대사 작가다. "
    "직장 동료 두 사람이 주고받는 짧은 잡담을 쓴다. "
    "구어체 한국어, 담백하고 자연스럽게. 이모지·따옴표·화자 이름은 쓰지 않는다."
)

POOL_MAX = 5  # 키당 최대 세트 수 — 이 수를 채우면 LLM을 더 부르지 않는다
LLM_TIMEOUT = 12  # 초. 대화는 즉시성이 품질 — 번역 경로(90초)보다 짧게 잡는다

# (구역, 풍경, 시간대) → 생성된 세트 목록. 프로세스 메모리만 씀(취향 문구라 영속화 불필요).
_pool: dict[tuple[str, str, str], list[list[str]]] = {}


def time_bucket(now: datetime | None = None) -> str:
    """KST 기준 아침/낮/저녁/밤 — 버킷이 바뀌면 풀 키가 바뀌어 문구도 자연히 갱신된다."""
    h = (now or datetime.now(KST)).astimezone(KST).hour
    if 6 <= h < 12:
        return "아침"
    if 12 <= h < 18:
        return "낮"
    if 18 <= h < 22:
        return "저녁"
    return "밤"


def _normalize_view(view: str) -> str:
    v = (view or "").strip()
    return v if v in VIEWS else DEFAULT_VIEW


def _normalize_spot(spot: str) -> str:
    s = (spot or "").strip()
    return s if s in SPOTS else DEFAULT_SPOT


def _clean_lines(raw: object) -> list[str]:
    """LLM 응답 정리 — 문자열 2~3줄, 각 40자 이내. 형식이 어긋나면 빈 목록(폴백 유도)."""
    if not isinstance(raw, list):
        return []
    out: list[str] = []
    for item in raw:
        if not isinstance(item, str):
            continue
        s = item.strip().strip('"').strip()
        if s:
            out.append(s[:40])
    return out[:3] if len(out) >= 2 else []


_RULES = (
    "규칙:\n"
    "- 각 줄 30자 이내, 말끝은 구어체로 (~네요, ~하다, ~죠)\n"
    "- '이거/이게/그거'처럼 지시어로 시작하지 않는다\n"
    "- 어제·오늘 같은 날짜나 다른 날과의 비교는 하지 않는다 (모르는 정보다)\n"
)


def _prompt(spot: str, view: str, bucket: str, space_name: str) -> str:
    where = f"'{space_name}' 방" if space_name else "사무실"
    if spot == "water":
        return (
            f"{where} 한쪽에 정수기가 있다. 지금은 {bucket}이다.\n"
            "정수기 앞에서, 일하다 지친 동료에게 물 한 잔 권하며 건네는 잡담을 2~3줄 써라. "
            "한 줄이 한 사람의 말이고, 순서대로 주고받는 대화다. "
            "'좀 쉬었다 하라'는 위로가 담기면 좋다.\n"
            f"{_RULES}"
            "- 건강 조언이나 훈계처럼 들리지 않게, 옆자리 동료의 말투로\n"
            "좋은 예:\n"
            '{"lines": ["물 한 잔 하고 하세요", "힘들죠? 좀 쉬었다 합시다"]}\n'
            '{"lines": ["한 잔 받아가세요", "쉬는 것도 일이죠"]}\n'
            '이제 새 대화를 아래 JSON으로만 출력: {"lines": ["첫째 사람 말", "둘째 사람 말"]}'
        )
    return (
        f"{where} 창밖에는 {VIEWS[view]}이 보인다. 지금은 {bucket}이다.\n"
        "일하다 창밖을 본 동료 두 사람의 잡담을 2~3줄 써라. "
        "한 줄이 한 사람의 말이고, 순서대로 주고받는 대화다. "
        "'밖에 나가고 싶다'는 마음이 은근히 배어 있으면 좋다.\n"
        f"{_RULES}"
        "- 실제 기온·날씨 수치를 말하지 않는다. 창밖 그림에 보이는 것에만 반응한다\n"
        "좋은 예:\n"
        '{"lines": ["밖에 단풍 다 들었네요", "이런 날 반차 쓰고 걷고 싶다"]}\n'
        '{"lines": ["창밖 보니까 일할 맛이 안 나죠", "이것만 끝내고 산책 갑시다"]}\n'
        '이제 새 대화를 아래 JSON으로만 출력: {"lines": ["첫째 사람 말", "둘째 사람 말"]}'
    )


def _llm(cfg: dict, spot: str, view: str, bucket: str, space_name: str) -> list[str]:
    url = cfg["llm_url"].rstrip("/") + "/chat/completions"
    headers = {"Content-Type": "application/json"}
    if cfg.get("llm_key"):
        headers["Authorization"] = f"Bearer {cfg['llm_key']}"
    payload = {
        "model": cfg["llm_model"],
        # 잡담은 다양성이 품질 — 번역(0.2)보다 온도를 높인다.
        "temperature": 0.9,
        "messages": [
            {"role": "system", "content": _SYSTEM},
            {"role": "user", "content": _prompt(spot, view, bucket, space_name)},
        ],
    }
    r = httpx.post(url, json=payload, headers=headers, timeout=LLM_TIMEOUT)
    r.raise_for_status()
    content = r.json()["choices"][0]["message"]["content"]
    return _clean_lines(_extract_json(content).get("lines"))


def _fallback(spot: str, view: str) -> list[str]:
    return random.choice(FALLBACK_WATER if spot == "water" else FALLBACK_WINDOW[view])


def chat(spot: str = "", view: str = "", space_name: str = "") -> dict:
    """오브젝트 대사 한 세트. 항상 성공한다 — 방이 오류를 말하는 일은 없다."""
    s = _normalize_spot(spot)
    v = _normalize_view(view)
    bucket = time_bucket()
    # 정수기 대사는 창밖 풍경과 무관하므로 풍경을 키에 넣지 않는다(방마다 다시 생성할 이유가 없다).
    key = (s, v if s == "window" else "-", bucket)
    pool = _pool.setdefault(key, [])

    if len(pool) < POOL_MAX:
        cfg = settings.get()
        if cfg.get("llm_url"):
            try:
                lines = _llm(cfg, s, v, bucket, space_name)
                if lines:
                    pool.append(lines)
                    return {"lines": lines, "source": "llm"}
            except Exception as e:  # noqa: BLE001 — 잡담 실패는 폴백으로 흡수
                log.warning("방 오브젝트 대사 LLM 실패, 폴백 (%s): %s", s, e)
    elif pool:
        return {"lines": random.choice(pool), "source": "llm"}

    return {"lines": _fallback(s, v), "source": "fallback"}
