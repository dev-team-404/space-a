"""런타임 설정 — a-hub 연결·LLM API 등을 설정 창(UI)에서 바꾸고 파일로 영속화한다.

우선순위: **저장된 파일 > 환경변수 > 하드코딩 기본값.**
collector·translator·store는 import 시 상수가 아니라 이 모듈을 **호출 시점에** 읽는다
→ 설정 창에서 값을 바꾸면 재시작 없이 반영된다(POST /api/settings가 캐시를 비운다).

비밀값(work_token·work_api_key·llm_key)은 GET 응답에서 값 대신 `*_set` 불리언으로만 노출한다.
"""

import json
import os
import threading
from pathlib import Path

_BACKEND = Path(__file__).resolve().parents[1]
_DEFAULT_DIR = _BACKEND / ".a-lens"
_SETTINGS_PATH = Path(os.environ.get("A_LENS_SETTINGS", str(_DEFAULT_DIR / "settings.json")))

# GET에서 값을 숨기고 존재 여부만 내보낼 키
SECRET_KEYS = {"work_token", "work_api_key", "llm_key"}
_NUMERIC = {"presence_window", "cache_ttl"}


def _defaults() -> dict:
    e = os.environ.get
    return {
        # 원천: auto | hub | dummy | fixtures
        "source": e("A_LENS_SOURCE", "auto"),
        # a-hub(work) 연결
        "work_url": e("A_LENS_WORK_URL", "https://spacea.msalt.net"),
        "work_token": e("A_LENS_WORK_TOKEN", ""),
        "work_api_key": e("A_LENS_WORK_API_KEY", ""),
        "presence_window": float(e("A_LENS_PRESENCE_WINDOW", "3600")),
        "cache_ttl": float(e("A_LENS_CACHE_TTL", "30")),
        # LLM API (OpenAI 호환). 기본값 = 로컬 LM Studio(WSL 호스트).
        "llm_url": e("A_LENS_LLM_URL", "http://172.26.80.1:1234/v1"),
        "llm_model": e("A_LENS_LLM_MODEL", "qwen/qwen3/qwen3-30b-a3b-instruct-2507-q4_k_m.gguf"),
        "llm_key": e("A_LENS_LLM_KEY", ""),
        # 요약 길이: brief(한 문장·담백) | normal(2~3문장) | detailed(3~5문장). 바꾸면 재번역됨.
        "summary_style": e("A_LENS_SUMMARY_STYLE", "brief"),
        # 번역 캐시 DB(SQLite). 비우면 캐시 없이 매번 재번역.
        "db_path": e("A_LENS_DB", str(_DEFAULT_DIR / "translation.db")),
    }


_lock = threading.Lock()
_current: dict | None = None
_listeners: list = []


def _load() -> dict:
    cfg = _defaults()
    try:
        if _SETTINGS_PATH.exists():
            saved = json.loads(_SETTINGS_PATH.read_text(encoding="utf-8"))
            cfg.update({k: v for k, v in saved.items() if k in cfg})
    except Exception:  # noqa: BLE001 — 깨진 설정 파일은 무시하고 기본값으로
        pass
    return cfg


def get() -> dict:
    global _current
    if _current is None:
        with _lock:
            if _current is None:
                _current = _load()
    return dict(_current)


def update(partial: dict) -> dict:
    """부분 갱신 후 파일에 저장하고 변경 리스너(캐시 비우기 등)를 호출한다."""
    global _current
    with _lock:
        cur = dict(_current if _current is not None else _load())
        for k, v in (partial or {}).items():
            if k not in cur or v is None:
                continue
            if k in _NUMERIC:
                try:
                    v = float(v)
                except (TypeError, ValueError):
                    continue
            cur[k] = v
        _SETTINGS_PATH.parent.mkdir(parents=True, exist_ok=True)
        _SETTINGS_PATH.write_text(
            json.dumps(cur, ensure_ascii=False, indent=1), encoding="utf-8"
        )
        _current = cur
    for cb in list(_listeners):
        try:
            cb()
        except Exception:  # noqa: BLE001
            pass
    return dict(_current)


def on_change(cb) -> None:
    _listeners.append(cb)


def public(cfg: dict | None = None) -> dict:
    """GET 응답용 뷰 — 비밀값은 `*_set` 불리언으로만, 나머지는 그대로."""
    c = dict(cfg if cfg is not None else get())
    out: dict = {}
    for k, v in c.items():
        if k in SECRET_KEYS:
            out[f"{k}_set"] = bool(v)
        else:
            out[k] = v
    return out
