"""방(life 꾸미기) 공유 저장 — 모든 뷰어가 같은 방을 보도록 서버에 저장한다.

프론트가 만든 LifeConfig(불투명 dict, space_id로 식별)를 그대로 보관한다. 백엔드는 스키마를
알 필요 없이 space_id만 안다. 저장 위치는 번역 캐시와 같은 .a-lens 디렉터리(rooms.json)라
Docker 볼륨에 함께 영속화된다. (기존 localStorage 저장의 'backend 저장으로 승격' — store.ts 주석)
"""

import json
import threading
from pathlib import Path

from . import settings

_lock = threading.Lock()


def _path() -> Path:
    db = (settings.get().get("db_path") or "").strip()
    base = Path(db).parent if db else Path(__file__).resolve().parents[1] / ".a-lens"
    return base / "rooms.json"


def _read() -> list:
    try:
        p = _path()
        if p.exists():
            data = json.loads(p.read_text(encoding="utf-8"))
            return data if isinstance(data, list) else []
    except Exception:  # noqa: BLE001 — 깨진 파일은 빈 목록으로
        pass
    return []


def _write(rooms: list) -> None:
    p = _path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(rooms, ensure_ascii=False), encoding="utf-8")


def list_rooms() -> list:
    return _read()


def save_room(config: dict) -> None:
    """같은 space_id의 방은 하나 — 있으면 덮어쓴다."""
    space_id = config.get("space_id")
    if not space_id:
        return
    with _lock:
        rooms = [r for r in _read() if r.get("space_id") != space_id]
        rooms.append(config)
        _write(rooms)


def delete_room(space_id: str) -> None:
    with _lock:
        _write([r for r in _read() if r.get("space_id") != space_id])
