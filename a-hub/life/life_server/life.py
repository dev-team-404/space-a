"""방 방문(Life Visit) — 개인 방·에이전트 위치·방 디자인.

설계: docs/design/life-visit.md
- 유저당 방 1개, 20×20 정사각 셀 격자. 에이전트 점유 = 논리 1셀.
- 위치의 단일 원천은 서버. 겹침 금지는 전역 락 안에서 "빈 셀일 때만 점유"로 원자 처리.
- 방 디자인(벽지·바닥·가구)은 방문자에게 보여주는 공개 표면이므로 서버가 가진다.
  사적 내용(다이어리 등)은 클라이언트에만 있다 — 여기 없음이 설계다.
- hub(Space/Page 도메인)와 별개의 서버 프로세스. 상태는 인메모리가 원천이고,
  `LIFE_SERVER_DB` 설정 시 SQLite로 write-through 영속화(store.py) — 재시작을 견딘다.
"""

import secrets
import hashlib
import threading
import uuid
from datetime import datetime, timezone
from dataclasses import dataclass, field

from . import errors
from .errors import CellTaken

GRID_W = 20
GRID_H = 20
FLOOR_Y = 0
FLOOR_DEPTH = 20
# 반깊이 바닥의 앞쪽 중앙에 가까운 자율 입장 스폰 지점.
SPAWN_X = 9
SPAWN_Y = 9
WINDOW_ROTATION_BY_WALL = {"west": 90, "north": 180}
# P4 인바운드 방문 추적 — 같은 방문자 연속 재입장 세션화 창·방당 보존 상한 (스펙 §3)
VISIT_SESSION_WINDOW_SECS = 30 * 60
VISITS_MAX_PER_LIFE = 100
# 프레즌스 `last_seen`을 DB에 실제로 쓰는 최소 간격(초). 인메모리 값은 매 호출 갱신된다.
# 소비자의 판정 창(a-lens 기본 3600초)보다 훨씬 촘촘하므로 정확도 손실은 없다.
_TOUCH_PERSIST_SECONDS = 30

Cell = tuple[int, int]


def _parse_ts(value: str | None) -> datetime | None:
    """rfc3339 문자열 → tz-aware datetime. 빈 값·파싱 실패는 None(관측 이력 없음)."""
    if not value:
        return None
    try:
        dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)


@dataclass
class LifeObject:
    asset_id: str
    category: str
    cell: Cell
    size: tuple[int, int] = (1, 1)
    rotation: int = 0
    wall: str | None = None
    footprint: tuple[Cell, ...] | None = None

    def occupied_cells(self) -> set[Cell]:
        w, h = self.size
        source = self.footprint or tuple((x, y) for y in range(h) for x in range(w))
        if self.rotation == 90:
            rotated = ((h - 1 - y, x) for x, y in source)
        elif self.rotation == 180:
            rotated = ((w - 1 - x, h - 1 - y) for x, y in source)
        elif self.rotation == 270:
            rotated = ((y, w - 1 - x) for x, y in source)
        else:
            rotated = iter(source)
        x0, y0 = self.cell
        return {(x0 + dx, y0 + dy) for dx, dy in rotated}


@dataclass
class LifeDesign:
    wallpaper: str = "lavender"
    floor: str = "cream"
    objects: list[LifeObject] = field(default_factory=list)


@dataclass
class LifeAgent:
    agent_id: str
    name: str
    life_id: str  # 자기 방 (소유)
    at_life: str  # 현재 있는 방
    cell: Cell
    mascot_seed: str = ""  # 클라이언트 마스코트 시드 — 어느 방에서든 같은 로봇으로 보이게
    org: str = ""  # 조직 (클라이언트 프로필)
    agent_uuid: str = ""  # 클라이언트가 자동부여한 고유 ID (서버 agent_id와 별개)
    # ── 공통 신원 (2026-07-29) — Life가 세 컴포넌트를 잇는 등록처 역할을 한다.
    # 스펙: docs/design/common/specs/2026-07-29-shared-identity-life-hub-lens.md
    owner_os_user: str = ""  # 주인 OS 계정 (a-mate가 보내던 값 — 이전엔 버려졌다)
    owner_full_name: str = ""  # 주인 풀네임 (사람이 서로를 알아보는 라벨)
    hub_user_id: str = ""  # work 허브 계정 id — a-lens가 Hub 활동을 붙일 때 쓰는 정답 키
    bubble: str = ""
    # O1 대문 아웃바운드 — 대문에 걸린 오늘의 한마디. 저장은 agent 키, 노출은 방 레벨
    # (owner_mascot_image_sha256 선례 — 주인이 자리를 비워도 대문은 걸려 있어야 한다).
    daily_line: str = ""
    connected: bool = True
    # 마지막 인증 호출 시각(rfc3339 UTC). `connected`는 "연결을 설정해뒀나"만 말해 준다 —
    # register/enter에서 True가 되고 수동 disconnect로만 False가 되므로 신선도가 없다.
    # 실제 프레즌스는 이 값 + 소비자 쪽 창(window)으로 판정한다. "" = 관측 이력 없음.
    last_seen: str = ""


@dataclass
class Life:
    id: str
    owner_agent_id: str
    owner_name: str
    design: LifeDesign = field(default_factory=LifeDesign)


def _spawn_hash(agent_id: str, cell: Cell) -> int:
    """등거리 스폰 후보 타이브레이크 — 에이전트마다 다르고 프로세스 재시작에도 같은 순서.

    내장 hash()는 프로세스별 솔트 때문에 재시작 간 비결정이라 sha256을 쓴다.
    """
    digest = hashlib.sha256(f"{agent_id}:{cell[0]}:{cell[1]}".encode()).digest()
    return int.from_bytes(digest[:8], "big")


def _is_floor_cell(cell: Cell) -> bool:
    x, y = cell
    return 0 <= x < GRID_W and 0 <= y < GRID_H and x + y < FLOOR_DEPTH


def _validate_cell(cell: Cell) -> None:
    if not _is_floor_cell(cell):
        x, y = cell
        raise errors.InvalidRequest(
            f"바닥 셀 범위 밖: ({x},{y}) — 0<=x,y<{GRID_W}, x+y<{FLOOR_DEPTH}"
        )


class LifeService:
    """방 서비스. 모든 변이는 self._lock 안 — 겹침 금지의 원자성 보장.

    상태는 인메모리가 원천이고, store(SqliteStore)를 주면 변이를 write-through로
    영속화하고 시작 시 복원한다. store가 없으면 순수 인메모리(재시작 시 초기화).
    """

    def __init__(self, store=None) -> None:
        self._lock = threading.Lock()
        self._store = store
        self._life: dict[str, Life] = {}
        self._agents: dict[str, LifeAgent] = {}
        self._tokens: dict[str, str] = {}  # token -> agent_id
        self._friends: set[tuple[str, str]] = set()
        self._content_visibility: dict[tuple[str, str], str] = {}
        self._diaries: dict[tuple[str, str], dict] = {}
        self._guestbook: list[dict] = []
        self._mascot_image_hashes: dict[str, str] = {}
        self._daily_cut_hashes: dict[str, str] = {}
        self._visits: list[dict] = []
        if store is not None:
            self._life, self._agents, self._tokens = store.load()
            self._friends, self._content_visibility, self._diaries, self._guestbook = store.load_social()
            self._mascot_image_hashes = store.load_mascot_image_hashes()
            self._daily_cut_hashes = store.load_daily_cut_hashes()
            self._visits = store.load_visits()
            # protocol v2에서는 창문 회전이 자유값이었다. v3부터 벽이 방향의 단일 원천이다.
            # protocol v4에서는 앞쪽 절반을 버린다. 창문 회전 보정과 함께 한 번에 영속화한다.
            for life in self._life.values():
                changed = False
                for obj in life.design.objects:
                    expected = WINDOW_ROTATION_BY_WALL.get(obj.wall) if obj.category == "window" else None
                    if expected is not None and obj.rotation != expected:
                        obj.rotation = expected
                        changed = True
                kept = [
                    obj for obj in life.design.objects
                    if obj.category == "window" or all(_is_floor_cell(cell) for cell in obj.occupied_cells())
                ]
                if len(kept) != len(life.design.objects):
                    life.design.objects = kept
                    changed = True
                if changed:
                    store.save_design(life)
            self._relocate_invalid_agents()

    def _relocate_invalid_agents(self) -> None:
        """protocol v4 시작 마이그레이션 — 비활성·충돌 위치의 봇만 결정론적으로 이동."""
        for life_id in sorted(self._life):
            life = self._life[life_id]
            furniture = set().union(*(
                obj.occupied_cells() for obj in life.design.objects if obj.category != "window"
            )) if life.design.objects else set()
            used: set[Cell] = set()
            agents = sorted(
                (agent for agent in self._agents.values() if agent.at_life == life_id),
                key=lambda agent: agent.agent_id,
            )
            for agent in agents:
                if _is_floor_cell(agent.cell) and agent.cell not in furniture and agent.cell not in used:
                    used.add(agent.cell)
                    continue
                agent.cell = self._free_cell_locked(life_id, for_agent=agent.agent_id)
                used.add(agent.cell)
                if self._store:
                    self._store.save_agent(agent)

    # --- 신원 ---

    def register(
        self, name: str, mascot_seed: str = "", org: str = "", agent_uuid: str = "",
        owner_os_user: str = "", owner_full_name: str = "", hub_user_id: str = "",
    ) -> tuple[LifeAgent, str, Life]:
        """유저 등록 + 개인 방 생성. 에이전트는 자기 방에 자동 입장.

        org·agent_uuid는 클라이언트 프로필(조직·고유 ID). owner_*·hub_user_id는 공통 신원
        (§2 — a-lens가 Hub 활동을 붙일 때 쓴다). **빈 값이면 기존 값을 유지한다** — 구버전
        클라이언트가 재등록해도 사람이 지정해 둔 연결을 지우지 않는다.
        """
        name = name.strip()
        if not name:
            raise errors.InvalidRequest("이름이 비어 있음")
        with self._lock:
            existing = next((agent for agent in self._agents.values() if agent.name == name), None)
            if existing is not None:
                existing.connected = True
                existing.mascot_seed = mascot_seed or existing.mascot_seed
                existing.org = org or existing.org
                existing.agent_uuid = agent_uuid or existing.agent_uuid
                existing.owner_os_user = owner_os_user or existing.owner_os_user
                existing.owner_full_name = owner_full_name or existing.owner_full_name
                existing.hub_user_id = hub_user_id or existing.hub_user_id
                token = secrets.token_urlsafe(24)
                self._tokens[token] = existing.agent_id
                if self._store:
                    self._store.save_agent(existing)
                    self._store.save_token(token, existing.agent_id)
                return existing, token, self._life[existing.life_id]
            agent_id = f"ragt_{uuid.uuid4().hex[:8]}"
            life_id = f"life_{uuid.uuid4().hex[:8]}"
            life = Life(id=life_id, owner_agent_id=agent_id, owner_name=name)
            self._life[life_id] = life
            agent = LifeAgent(
                agent_id=agent_id, name=name, life_id=life_id, at_life=life_id,
                cell=self._free_cell_locked(life_id), mascot_seed=mascot_seed,
                org=org, agent_uuid=agent_uuid, owner_os_user=owner_os_user,
                owner_full_name=owner_full_name, hub_user_id=hub_user_id,
            )
            self._agents[agent_id] = agent
            token = secrets.token_urlsafe(24)
            self._tokens[token] = agent_id
            if self._store:
                self._store.save_registration(agent, token, life)
            return agent, token, life

    def _authed(self, token: str | None) -> LifeAgent:
        agent_id = self._tokens.get(token or "")
        if agent_id is None:
            raise errors.Unauthorized("invalid or missing token")
        agent = self._agents[agent_id]
        self._touch(agent)
        return agent

    def _touch(self, agent: LifeAgent) -> None:
        """인증된 호출 = 클라이언트가 살아 있다는 증거. 그 시각을 `last_seen`에 남긴다.

        전용 하트비트 엔드포인트를 두지 않는 이유: a-mate가 이미 스캔에 편승해 방문·방명록
        폴링으로 분당 한 번쯤 인증 호출을 하고 있다. 그 트래픽에 시각만 붙이면 프레즌스가
        공짜로 생기고, 클라이언트는 고칠 것이 없다.

        DB 쓰기는 `_TOUCH_PERSIST_SECONDS` 간격으로 성글게 한다 — 이 서버는 지금까지 변이에서만
        쓰기를 했으므로, 읽기 요청마다 쓰기를 하면 쓰기량 성격이 달라진다. 인메모리 값은 항상
        최신이라 판정 정확도는 떨어지지 않는다.

        주의: 호출자는 `self._lock`을 들고 있지 않아야 한다(비재진입 Lock). 현재 `_authed`의
        모든 호출부가 락 밖이다.
        """
        now = datetime.now(timezone.utc)
        with self._lock:
            prev = _parse_ts(agent.last_seen)
            agent.last_seen = now.isoformat()
            stale = prev is None or (now - prev).total_seconds() >= _TOUCH_PERSIST_SECONDS
            if self._store and stale:
                self._store.save_agent(agent)

    def rename(self, token: str | None, name: str) -> dict:
        """유저 이름 변경 — 에이전트 이름과 자기 방의 주인 이름을 함께 바꾼다."""
        name = name.strip()
        if not name:
            raise errors.InvalidRequest("이름이 비어 있음")
        agent = self._authed(token)
        with self._lock:
            agent.name = name
            life = self._life[agent.life_id]
            life.owner_name = name
            if self._store:
                self._store.save_agent(agent)
                self._store.save_owner_name(life)
        return self.me(token)

    # --- 조회 ---

    def list_life(self) -> list[dict]:
        with self._lock:
            return [
                {
                    "life_id": r.id,
                    "owner_name": r.owner_name,
                    "occupants": sum(1 for a in self._agents.values() if a.connected and a.at_life == r.id),
                }
                for r in self._life.values()
            ]

    def life_state(self, life_id: str) -> dict:
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            owner = self._agents.get(life.owner_agent_id)
            return {
                "life_id": life.id,
                "owner_agent_id": life.owner_agent_id,
                "owner_name": life.owner_name,
                # 주인이 다른 방에 가 있어도 방문자가 주인의 로봇(미니홈피 프로필)을 그릴 수 있게
                "owner_mascot_seed": owner.mascot_seed if owner else "",
                "owner_mascot_image_sha256": self._mascot_image_hashes.get(life.owner_agent_id),
                # O1 대문 — 방 레벨. 주인이 남의 방에 가 있어도 대문은 이 방에 걸려 있다.
                "owner_daily_line": owner.daily_line if owner else "",
                "owner_daily_cut_sha256": self._daily_cut_hashes.get(life.owner_agent_id),
                "grid": {"w": GRID_W, "h": GRID_H},
                "design": {
                    "wallpaper": life.design.wallpaper,
                    "floor": life.design.floor,
                    "objects": [
                        {
                            "asset_id": o.asset_id,
                            "category": o.category,
                            "cell": list(o.cell),
                            "size": list(o.size),
                            "rotation": o.rotation,
                            "wall": o.wall,
                            "footprint": [list(cell) for cell in o.footprint] if o.footprint else None,
                        }
                        for o in life.design.objects
                    ],
                },
                "occupants": [
                    {
                        "agent_id": a.agent_id,
                        "name": a.name,
                        "cell": list(a.cell),
                        "is_owner": a.agent_id == life.owner_agent_id,
                        "mascot_seed": a.mascot_seed,
                        "mascot_image_sha256": self._mascot_image_hashes.get(a.agent_id),
                        "identity": self._identity(a),
                        "bubble": a.bubble,
                    }
                    for a in self._agents.values()
                    if a.connected and a.at_life == life.id
                ],
            }

    def _identity(self, agent: LifeAgent) -> dict:
        """공통 신원 블록 — a-lens가 Hub(work) 활동을 붙일 때 쓰는 재료(§2.3).

        hub_user_id가 채워져 있으면 그것이 정답이고, 없으면 소비자가 이름·OS 계정으로
        추론한다. 추가 필드만 늘리므로 기존 소비자는 영향받지 않는다."""
        return {
            "agent_uuid": agent.agent_uuid,
            "org": agent.org,
            "owner_os_user": agent.owner_os_user,
            "owner_full_name": agent.owner_full_name,
            "hub_user_id": agent.hub_user_id,
            "mascot_image_sha256": self._mascot_image_hashes.get(agent.agent_id),
        }

    def set_hub_user(self, token: str | None, agent_id: str, hub_user_id: str, *, admin: bool = False) -> dict:
        """work 허브 계정 연결을 지정/해제한다. 빈 문자열이면 해제(추론으로 복귀).

        본인 것은 자기 토큰으로, 남의 것은 관리 키(x-api-key)를 가진 호출자만 — 관리자 한 명이
        팀 전체 연결을 한 번에 정리하는 시나리오를 위해서다(§2.2)."""
        me = self._authed(token)
        with self._lock:
            target = self._agents.get(agent_id)
            if target is None:
                raise errors.NotFound(f"에이전트 '{agent_id}' 없음")
            if target.agent_id != me.agent_id and not admin:
                raise errors.Forbidden("남의 연결은 관리 키로만 바꿀 수 있어요")
            target.hub_user_id = hub_user_id.strip()
            if self._store:
                self._store.save_agent(target)
            return {"agent_id": target.agent_id, "hub_user_id": target.hub_user_id}

    def me(self, token: str | None) -> dict:
        agent = self._authed(token)
        with self._lock:
            return {
                "agent_id": agent.agent_id,
                "name": agent.name,
                "my_life_id": agent.life_id,
                "life_id": agent.at_life,
                "cell": list(agent.cell),
                "connected": agent.connected,
                "last_seen": agent.last_seen or None,
                "identity": self._identity(agent),
            }

    # --- 소셜 기능 ---

    def people(self, token: str | None) -> list[dict]:
        me = self._authed(token)
        with self._lock:
            return [
                {"agent_id": a.agent_id, "name": a.name, "life_id": a.life_id,
                 "is_friend": (me.agent_id, a.agent_id) in self._friends,
                 # 프레즌스 재료 — 판정(창 크기)은 소비자 몫이다. `connected`만으로는 부족하고
                 # `last_seen`이 있어야 "지금 켜져 있나"를 알 수 있다.
                 "connected": a.connected, "last_seen": a.last_seen or None,
                 "identity": self._identity(a)}
                for a in self._agents.values() if a.agent_id != me.agent_id
            ]

    def set_friend(self, token: str | None, friend_agent_id: str, enabled: bool) -> dict:
        me = self._authed(token)
        with self._lock:
            if friend_agent_id not in self._agents or friend_agent_id == me.agent_id:
                raise errors.InvalidRequest("잘못된 일촌 대상")
            key = (me.agent_id, friend_agent_id)
            self._friends.add(key) if enabled else self._friends.discard(key)
            if self._store:
                self._store.set_friend(me.agent_id, friend_agent_id, enabled)
        return {"friend_agent_id": friend_agent_id, "enabled": enabled}

    def _can_view_locked(self, owner_agent_id: str, viewer_agent_id: str, visibility: str) -> bool:
        """Life 콘텐츠 공통 공개 범위 판정. 호출자는 self._lock을 보유해야 한다."""
        if owner_agent_id == viewer_agent_id:
            return True
        if visibility == "public":
            return True
        if visibility == "friends":
            return (owner_agent_id, viewer_agent_id) in self._friends
        return False

    def set_content_visibility(self, token: str | None, feature: str, visibility: str) -> dict:
        me = self._authed(token)
        if feature != "diary":
            raise errors.InvalidRequest("unsupported content feature")
        if visibility not in ("private", "friends", "public"):
            raise errors.InvalidRequest("visibility must be private, friends, or public")
        with self._lock:
            self._content_visibility[(me.agent_id, feature)] = visibility
            if visibility == "private":
                self._diaries = {key: row for key, row in self._diaries.items() if key[0] != me.life_id}
            if self._store:
                self._store.set_content_visibility(me.agent_id, feature, visibility)
                if visibility == "private":
                    self._store.clear_shared_diaries(me.life_id)
        return {"feature": feature, "visibility": visibility}

    def content_access(self, token: str | None, life_id: str) -> dict:
        viewer = self._authed(token)
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            visibility = self._content_visibility.get((life.owner_agent_id, "diary"), "private")
            return {"features": {"diary": {
                "visibility": visibility,
                "can_view": self._can_view_locked(life.owner_agent_id, viewer.agent_id, visibility),
            }}}

    def set_mascot_image(self, token: str | None, png: bytes) -> dict:
        me = self._authed(token)
        if not png.startswith(b"\x89PNG\r\n\x1a\n"):
            raise errors.InvalidRequest("mascot image must be a PNG")
        if len(png) > 5 * 1024 * 1024:
            raise errors.InvalidRequest("mascot image exceeds 5 MiB")
        digest = hashlib.sha256(png).hexdigest()
        with self._lock:
            if not self._store:
                raise errors.InvalidRequest("mascot image storage is unavailable")
            current = self._store.mascot_image(me.agent_id)
            if current is None or current[1] != digest:
                self._store.save_mascot_image(me.agent_id, png, digest, datetime.now(timezone.utc).isoformat())
                self._mascot_image_hashes[me.agent_id] = digest
        return {"sha256": digest, "size": len(png)}

    def mascot_image(self, token: str | None, agent_id: str) -> tuple[bytes, str]:
        self._authed(token)
        with self._lock:
            if agent_id not in self._agents:
                raise errors.NotFound(f"agent '{agent_id}' not found")
            row = self._store.mascot_image(agent_id) if self._store else None
            if row is None:
                raise errors.NotFound("mascot image not found")
            return row

    def set_daily_cut(self, token: str | None, png: bytes) -> dict:
        """O1 — 대문사진 게시 (set_mascot_image와 동일 규율). 같은 sha면 쓰기를 생략한다.

        마스코트 이미지와 **다른 슬롯**이어야 한다 — mascot_images는 방 안 점유자 로봇 렌더에도
        쓰이므로, 컷을 그 슬롯에 넣으면 방 안 로봇이 컷 그림으로 바뀐다 (ADR 0026)."""
        me = self._authed(token)
        if not png.startswith(b"\x89PNG\r\n\x1a\n"):
            raise errors.InvalidRequest("daily cut must be a PNG")
        if len(png) > 5 * 1024 * 1024:
            raise errors.InvalidRequest("daily cut exceeds 5 MiB")
        digest = hashlib.sha256(png).hexdigest()
        with self._lock:
            if not self._store:
                raise errors.InvalidRequest("daily cut storage is unavailable")
            current = self._store.daily_cut(me.agent_id)
            if current is None or current[1] != digest:
                self._store.save_daily_cut(me.agent_id, png, digest, datetime.now(timezone.utc).isoformat())
                self._daily_cut_hashes[me.agent_id] = digest
        return {"sha256": digest, "size": len(png)}

    def daily_cut(self, token: str | None, agent_id: str) -> tuple[bytes, str]:
        """O1 — 방문객이 방 주인의 대문사진을 가져온다. 없으면 404 (클라이언트가 폴백)."""
        self._authed(token)
        with self._lock:
            if agent_id not in self._agents:
                raise errors.NotFound(f"agent '{agent_id}' not found")
            row = self._store.daily_cut(agent_id) if self._store else None
            if row is None:
                raise errors.NotFound("daily cut not found")
            return row

    def share_diary(self, token: str | None, date: str, body: str, visibility: str) -> dict:
        me = self._authed(token)
        if visibility not in ("friends", "public") or not date.strip() or not body.strip():
            raise errors.InvalidRequest("날짜, 본문, 공개 범위를 확인하세요")
        if len(body) > 100_000:
            raise errors.InvalidRequest("다이어리 본문이 너무 큼")
        row = {"date": date, "body": body, "visibility": visibility}
        with self._lock:
            self._diaries[(me.life_id, date)] = row
            if self._store:
                self._store.save_shared_diary(me.life_id, date, body, visibility)
        return row

    def unshare_diary(self, token: str | None, date: str) -> dict:
        me = self._authed(token)
        with self._lock:
            self._diaries.pop((me.life_id, date), None)
            if self._store:
                self._store.delete_shared_diary(me.life_id, date)
        return {"date": date, "visibility": "private"}

    def shared_diaries(self, token: str | None, life_id: str) -> list[dict]:
        viewer = self._authed(token)
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            owner = life.owner_agent_id
            visibility = self._content_visibility.get((owner, "diary"), "private")
            if not self._can_view_locked(owner, viewer.agent_id, visibility):
                return []
            return [dict(row) for (lid, _), row in sorted(self._diaries.items(), reverse=True)
                    if lid == life_id]

    def guestbook(self, life_id: str) -> list[dict]:
        with self._lock:
            if life_id not in self._life:
                raise errors.NotFound(f"life '{life_id}' not found")
            return [dict(row) for row in reversed(self._guestbook) if row["life_id"] == life_id]

    def add_guestbook(self, token: str | None, life_id: str, body: str,
                      author_name: str | None = None, parent_id: str | None = None,
                      author_kind: str | None = None) -> dict:
        author = self._authed(token)
        body = body.strip()
        if not body or len(body) > 500:
            raise errors.InvalidRequest("방명록은 1~500자여야 함")
        # G1: 클라이언트가 작성자 표기를 조립해 보낸다(사람=풀네임, 봇="{호칭}님의 {봇이름}").
        # 미제공이면 현행대로 등록된 agent name — 구클라 하위호환.
        name = (author_name or "").strip()
        if len(name) > 80:
            raise errors.InvalidRequest("작성자 이름은 80자 이하여야 함")
        if not name:
            name = author.name
        # P3: 봇 자동 작성 판별 플래그 — 미제공(None)=human 간주는 소비자 몫 (구클라 하위호환)
        kind = (author_kind or "").strip() or None
        if kind not in (None, "human", "bot"):
            raise errors.InvalidRequest("author_kind는 human 또는 bot이어야 함")
        parent_id = (parent_id or "").strip() or None
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            if parent_id is not None:
                # G2(ADR 0021): 답글은 방 주인만, 1단계만 — 부모는 같은 방의 top-level 원글.
                parent = next((r for r in self._guestbook if r["entry_id"] == parent_id), None)
                if parent is None or parent["life_id"] != life_id:
                    raise errors.NotFound("부모 방명록 항목을 찾을 수 없음")
                if parent.get("parent_id"):
                    raise errors.InvalidRequest("답글에는 답글을 달 수 없음")
                if author.agent_id != life.owner_agent_id:
                    raise errors.Forbidden("방 주인만 답글을 달 수 있음")
            row = {"entry_id": f"gb_{uuid.uuid4().hex[:12]}", "life_id": life_id,
                   "author_agent_id": author.agent_id, "author_name": name, "body": body,
                   "parent_id": parent_id, "author_kind": kind,
                   "created_at": datetime.now(timezone.utc).isoformat()}
            self._guestbook.append(row)
            if self._store:
                self._store.save_guestbook_entry(row)
            return dict(row)

    def delete_guestbook(self, token: str | None, entry_id: str) -> dict:
        actor = self._authed(token)
        with self._lock:
            row = next((r for r in self._guestbook if r["entry_id"] == entry_id), None)
            if row is None:
                raise errors.NotFound("방명록 항목을 찾을 수 없음")
            life = self._life[row["life_id"]]
            if actor.agent_id not in (row["author_agent_id"], life.owner_agent_id):
                raise errors.Forbidden("작성자 또는 방 주인만 삭제할 수 있음")
            # G2(ADR 0021): 원글 삭제 시 답글도 함께(cascade) — 고아 행 금지
            self._guestbook = [r for r in self._guestbook
                               if r["entry_id"] != entry_id and r.get("parent_id") != entry_id]
            if self._store:
                self._store.delete_guestbook_entry(entry_id)
        return {"entry_id": entry_id, "deleted": True}

    def set_bubble(self, token: str | None, body: str) -> dict:
        agent = self._authed(token)
        body = body.strip()
        if len(body) > 120:
            raise errors.InvalidRequest("말풍선은 120자 이하여야 함")
        with self._lock:
            agent.bubble = body
            if self._store:
                self._store.save_agent(agent)
        return {"bubble": body}

    def set_daily_line(self, token: str | None, body: str) -> dict:
        """O1 — 대문에 걸린 오늘의 한마디. 빈 문자열은 지움 (set_bubble과 동일 규율).

        값의 해석(캡션 우선·"오늘" 판정)은 전부 클라이언트가 한다 — 서버는 주인의 시간대를
        모르므로 문자열을 보관·반환만 하고, 방문객은 주인 화면과 같은 문장을 본다."""
        agent = self._authed(token)
        body = body.strip()
        if len(body) > 120:
            raise errors.InvalidRequest("대문 한마디는 120자 이하여야 함")
        with self._lock:
            agent.daily_line = body
            if self._store:
                self._store.save_agent(agent)
        return {"daily_line": body}

    def disconnect(self, token: str | None) -> dict:
        agent = self._authed(token)
        with self._lock:
            agent.connected = False
            if self._store:
                self._store.save_agent(agent)
        return {"disconnected": True}

    # --- 인바운드 방문 추적 (P4) ---

    def _record_visit_locked(self, life: Life, agent: LifeAgent) -> None:
        """방문 자동 기록. 자기 방 입장은 기록하지 않는다. 같은 (방, 방문자)의 최신 방문이
        세션 창(30분) 이내면 새 행 대신 last_at 연장 — 들락날락 도배 억제 (스펙 §1)."""
        if life.owner_agent_id == agent.agent_id:
            return
        now = datetime.now(timezone.utc)
        latest = next((v for v in reversed(self._visits)
                       if v["life_id"] == life.id and v["visitor_agent_id"] == agent.agent_id), None)
        if latest is not None:
            last = datetime.fromisoformat(latest["last_at"])
            if (now - last).total_seconds() <= VISIT_SESSION_WINDOW_SECS:
                latest["last_at"] = now.isoformat()
                latest["visitor_name"] = agent.name  # 개명 반영 (이력 행은 당시 이름 유지)
                if self._store:
                    self._store.save_visit(latest)
                return
        row = {"visit_id": f"vst_{uuid.uuid4().hex[:12]}", "life_id": life.id,
               "visitor_agent_id": agent.agent_id, "visitor_name": agent.name,
               "first_at": now.isoformat(), "last_at": now.isoformat()}
        self._visits.append(row)
        if self._store:
            self._store.save_visit(row)
        self._prune_visits_locked(life.id)

    def _prune_visits_locked(self, life_id: str) -> None:
        rows = [v for v in self._visits if v["life_id"] == life_id]
        if len(rows) <= VISITS_MAX_PER_LIFE:
            return
        rows.sort(key=lambda v: v["last_at"])
        for stale in rows[: len(rows) - VISITS_MAX_PER_LIFE]:
            self._visits.remove(stale)
            if self._store:
                self._store.delete_visit(stale["visit_id"])

    def visits(self, token: str | None, since: str | None = None, limit: int = 50) -> list[dict]:
        """내 방 인바운드 방문 조회 — Bearer 본인 방 전용(타인 방 기록은 구조적으로 불가).
        since: last_at 초과 필터(RFC3339 사전순). last_at 내림차순, limit 1~100 클램프.
        present = 그 방문자가 지금도 내 방에 있는지 (문구 현재형/과거형 분기용, 스펙 §1)."""
        me = self._authed(token)
        limit = max(1, min(int(limit), 100))
        with self._lock:
            rows = [v for v in self._visits if v["life_id"] == me.life_id]
            if since:
                rows = [v for v in rows if v["last_at"] > since]
            rows.sort(key=lambda v: v["last_at"], reverse=True)
            out = []
            for v in rows[:limit]:
                visitor = self._agents.get(v["visitor_agent_id"])
                out.append({**v, "present": bool(visitor and visitor.at_life == me.life_id)})
            return out

    # --- 위치 변이 (전부 락 안에서 원자 처리) ---

    def enter(self, token: str | None, life_id: str, cell: Cell | None) -> dict:
        agent = self._authed(token)
        if cell is not None:
            _validate_cell(cell)
        with self._lock:
            if life_id not in self._life:
                raise errors.NotFound(f"life '{life_id}' not found")
            life = self._life[life_id]
            target = cell if cell is not None else self._free_cell_locked(life_id, for_agent=agent.agent_id)
            if self._occupied_locked(life_id, target, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({target[0]},{target[1]}) 이미 점유됨")
            # 이전 방 자동 퇴장 = at_life/cell 원자 교체
            agent.at_life = life_id
            agent.cell = target
            agent.connected = True
            if self._store:
                self._store.save_agent(agent)
            # P4: 인바운드 방문 자동 기록 — 방문자≠주인일 때만 (같은 락 안)
            self._record_visit_locked(life, agent)
        return self.me(token)

    def move(self, token: str | None, cell: Cell) -> dict:
        agent = self._authed(token)
        _validate_cell(cell)
        with self._lock:
            if self._occupied_locked(agent.at_life, cell, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({cell[0]},{cell[1]}) 이미 점유됨")
            agent.cell = cell
            if self._store:
                self._store.save_agent(agent)
        return self.me(token)

    # --- 방 디자인 (주인만) ---

    def set_design(self, token: str | None, life_id: str, design: dict) -> dict:
        agent = self._authed(token)
        with self._lock:
            life = self._life.get(life_id)
            if life is None:
                raise errors.NotFound(f"life '{life_id}' not found")
            if life.owner_agent_id != agent.agent_id:
                raise errors.Forbidden("방 주인만 디자인을 바꿀 수 있음")
            objects: list[LifeObject] = []
            occupied: set = set()
            for o in design.get("objects", []):
                cell = (int(o["cell"][0]), int(o["cell"][1]))
                _validate_cell(cell)
                size_raw = o.get("size", [1, 1])
                size = (int(size_raw[0]), int(size_raw[1]))
                if not (1 <= size[0] <= 8 and 1 <= size[1] <= 8):
                    raise errors.InvalidRequest("가구 크기는 각 축 1~8셀이어야 함")
                rotation = int(o.get("rotation", 0))
                if rotation not in (0, 90, 180, 270):
                    raise errors.InvalidRequest("회전은 0/90/180/270만 허용")
                category = str(o.get("category", "legacy"))
                wall = o.get("wall")
                footprint_raw = o.get("footprint")
                footprint = None
                if footprint_raw is not None:
                    if not isinstance(footprint_raw, list) or not 1 <= len(footprint_raw) <= 64:
                        raise errors.InvalidRequest("가구 footprint는 1~64개 셀이어야 함")
                    footprint = tuple((int(cell[0]), int(cell[1])) for cell in footprint_raw)
                    if len(set(footprint)) != len(footprint) or any(x < 0 or y < 0 or x >= size[0] or y >= size[1] for x, y in footprint):
                        raise errors.InvalidRequest("가구 footprint 셀이 기본 크기를 벗어남")
                asset_id = str(o.get("asset_id", o.get("kind", "unknown"))).strip()
                if not asset_id or len(asset_id) > 80 or len(category) > 40:
                    raise errors.InvalidRequest("잘못된 가구 식별자")
                obj = LifeObject(
                    asset_id=asset_id,
                    category=category,
                    cell=cell,
                    size=size,
                    rotation=rotation,
                    wall=str(wall) if wall is not None else None,
                    footprint=footprint,
                )
                cells = obj.occupied_cells()
                if category == "window":
                    if obj.wall not in ("north", "west"):
                        raise errors.InvalidRequest("창문 벽은 north 또는 west여야 함")
                    expected_rotation = WINDOW_ROTATION_BY_WALL[obj.wall]
                    if obj.rotation != expected_rotation:
                        raise errors.InvalidRequest(
                            f"창문 방향은 설치 벽에 고정됨: {obj.wall} 벽은 rotation={expected_rotation}"
                        )
                    limit = GRID_W if obj.wall == "north" else GRID_H
                    if cell[0] < 0 or cell[0] + size[0] > limit:
                        raise errors.InvalidRequest("창문이 벽 범위를 벗어남")
                    wall_cells = {(obj.wall, x) for x in range(cell[0], cell[0] + size[0])}
                    if wall_cells & occupied:
                        raise CellTaken("창문끼리 겹침")
                    occupied.update(wall_cells)
                    objects.append(obj)
                    continue
                if any(not _is_floor_cell(cell) for cell in cells):
                    raise errors.InvalidRequest("가구가 방 범위를 벗어남")
                if cells & occupied:
                    raise CellTaken("가구끼리 겹침")
                # 에이전트가 서 있는 셀에는 가구를 못 놓는다
                for a in self._agents.values():
                    if a.at_life == life_id and a.cell in cells:
                        raise CellTaken(f"셀 ({a.cell[0]},{a.cell[1]})에 에이전트가 있음")
                occupied.update(cells)
                objects.append(obj)
            life.design = LifeDesign(
                wallpaper=str(design.get("wallpaper", life.design.wallpaper)),
                floor=str(design.get("floor", life.design.floor)),
                objects=objects,
            )
            if self._store:
                self._store.save_design(life)
        return self.life_state(life_id)

    # --- 내부 (호출자가 락 보유) ---

    def _occupied_locked(self, life_id: str, cell: Cell, except_agent: str) -> bool:
        for a in self._agents.values():
            if a.agent_id != except_agent and a.at_life == life_id and a.cell == cell:
                return True
        life = self._life.get(life_id)
        if life and any(o.category != "window" and cell in o.occupied_cells() for o in life.design.objects):
            return True
        return False

    def _free_cell_locked(self, life_id: str, for_agent: str = "") -> Cell:
        """자율 입장용 빈 셀 배정 — 앵커에서 가까운 순, 다른 에이전트와 거리 버퍼 우선.

        후보는 (앵커 체비셰프 거리, 해시(for_agent, cell)) 순 — 같은 링 위에서는
        에이전트마다 다른 칸을 골라 흩어진다(결정적). 버퍼를 만족하는 칸이 없으면
        버퍼 없이 현행대로 가장 가까운 빈 칸.
        """
        life = self._life.get(life_id)
        agent_cells = {a.cell for a in self._agents.values()
                       if a.agent_id != for_agent and a.at_life == life_id and _is_floor_cell(a.cell)}
        object_cells: set[Cell] = set()
        if life:
            for o in life.design.objects:
                if o.category != "window":
                    object_cells |= {cell for cell in o.occupied_cells() if _is_floor_cell(cell)}
        free = [
            c for c in sorted(
                ((x, y) for y in range(FLOOR_Y, GRID_H) for x in range(GRID_W) if _is_floor_cell((x, y))),
                key=lambda c: (max(abs(c[0] - SPAWN_X), abs(c[1] - SPAWN_Y)), _spawn_hash(for_agent, c)),
            )
            if c not in agent_cells and c not in object_cells
        ]
        for buffer in (3, 2):
            for cell in free:
                if all(max(abs(cell[0] - ax), abs(cell[1] - ay)) >= buffer for ax, ay in agent_cells):
                    return cell
        if free:
            return free[0]
        raise CellTaken("방이 가득 참")
