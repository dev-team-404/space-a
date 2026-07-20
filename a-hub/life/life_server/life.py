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
import threading
import uuid
from dataclasses import dataclass, field

from . import errors
from .errors import CellTaken

GRID_W = 20
GRID_H = 20
FLOOR_Y = 0
# 자율 입장(셀 미지정) 스폰 지점 — 구석이 아니라 방의 가로 3/7, 세로 4/5 지점 근처
SPAWN_X = GRID_W * 3 // 7  # 12
SPAWN_Y = GRID_H * 4 // 5  # 12
WINDOW_ROTATION_BY_WALL = {"west": 90, "north": 180}

Cell = tuple[int, int]


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


@dataclass
class Life:
    id: str
    owner_agent_id: str
    owner_name: str
    design: LifeDesign = field(default_factory=LifeDesign)


def _validate_cell(cell: Cell) -> None:
    x, y = cell
    if not (0 <= x < GRID_W and 0 <= y < GRID_H):
        raise errors.InvalidRequest(f"셀 범위 밖: ({x},{y}) — 0~{GRID_W - 1} × 0~{GRID_H - 1}")


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
        if store is not None:
            self._life, self._agents, self._tokens = store.load()
            # protocol v2에서는 창문 회전이 자유값이었다. v3부터 벽이 방향의 단일 원천이다.
            for life in self._life.values():
                changed = False
                for obj in life.design.objects:
                    expected = WINDOW_ROTATION_BY_WALL.get(obj.wall) if obj.category == "window" else None
                    if expected is not None and obj.rotation != expected:
                        obj.rotation = expected
                        changed = True
                if changed:
                    store.save_design(life)

    # --- 신원 ---

    def register(self, name: str, mascot_seed: str = "") -> tuple[LifeAgent, str, Life]:
        """유저 등록 + 개인 방 생성. 에이전트는 자기 방에 자동 입장."""
        name = name.strip()
        if not name:
            raise errors.InvalidRequest("이름이 비어 있음")
        with self._lock:
            agent_id = f"ragt_{uuid.uuid4().hex[:8]}"
            life_id = f"life_{uuid.uuid4().hex[:8]}"
            life = Life(id=life_id, owner_agent_id=agent_id, owner_name=name)
            self._life[life_id] = life
            agent = LifeAgent(
                agent_id=agent_id, name=name, life_id=life_id, at_life=life_id,
                cell=self._free_cell_locked(life_id), mascot_seed=mascot_seed,
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
        return self._agents[agent_id]

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
                    "occupants": sum(1 for a in self._agents.values() if a.at_life == r.id),
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
                "owner_name": life.owner_name,
                # 주인이 다른 방에 가 있어도 방문자가 주인의 로봇(미니홈피 프로필)을 그릴 수 있게
                "owner_mascot_seed": owner.mascot_seed if owner else "",
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
                    }
                    for a in self._agents.values()
                    if a.at_life == life.id
                ],
            }

    def me(self, token: str | None) -> dict:
        agent = self._authed(token)
        with self._lock:
            return {
                "agent_id": agent.agent_id,
                "name": agent.name,
                "my_life_id": agent.life_id,
                "life_id": agent.at_life,
                "cell": list(agent.cell),
            }

    # --- 위치 변이 (전부 락 안에서 원자 처리) ---

    def enter(self, token: str | None, life_id: str, cell: Cell | None) -> dict:
        agent = self._authed(token)
        if cell is not None:
            _validate_cell(cell)
        with self._lock:
            if life_id not in self._life:
                raise errors.NotFound(f"life '{life_id}' not found")
            target = cell if cell is not None else self._free_cell_locked(life_id)
            if self._occupied_locked(life_id, target, except_agent=agent.agent_id):
                raise CellTaken(f"셀 ({target[0]},{target[1]}) 이미 점유됨")
            # 이전 방 자동 퇴장 = at_life/cell 원자 교체
            agent.at_life = life_id
            agent.cell = target
            if self._store:
                self._store.save_agent(agent)
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
                if any(not (0 <= x < GRID_W and 0 <= y < GRID_H) for x, y in cells):
                    raise errors.InvalidRequest("가구가 방 범위를 벗어남")
                if any(y < 0 for _, y in cells):
                    raise errors.InvalidRequest("가구는 바닥 영역에만 배치할 수 있음")
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

    def _free_cell_locked(self, life_id: str) -> Cell:
        """자율 입장용 빈 셀 배정 — 스폰 지점(SPAWN_X, SPAWN_Y)에서 가까운 순으로 첫 빈 셀."""
        cells = sorted(
            ((x, y) for y in range(FLOOR_Y, GRID_H) for x in range(GRID_W)),
            key=lambda c: max(abs(c[0] - SPAWN_X), abs(c[1] - SPAWN_Y)),
        )
        for cell in cells:
            if not self._occupied_locked(life_id, cell, except_agent=""):
                return cell
        raise CellTaken("방이 가득 참")
