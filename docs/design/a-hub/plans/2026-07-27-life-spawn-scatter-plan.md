# 방문 스폰 위치 분산 (V1) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 방문(`life_goto`) 시 서버가 배정하는 스폰 셀이 기존 에이전트와 Chebyshev 거리 3 이상 떨어지도록 분산한다.

**Architecture:** a-hub life 서버의 `_free_cell_locked` 하나를 개편 — 앵커 (8,16) 거리순 정렬은 유지하되 `(앵커 거리, sha256 해시)` 정렬 + 버퍼 사다리(3 → 2 → 없음)로 빈 칸을 고른다. API·스키마·계약·a-mate 무변경.

**Tech Stack:** Python 3 (dataclasses, threading.Lock), pytest. 새 의존성 없음 (`hashlib`는 이미 import됨).

**스펙:** [2026-07-27-life-spawn-scatter-design.md](../specs/2026-07-27-life-spawn-scatter-design.md)

## Global Constraints

- 실행 환경: **네이티브 Windows PowerShell** (WSL 금지). venv는 `a-hub\life\.venv`에 **이미 생성·설치됨** (베이스라인 47 passed).
- 모든 명령은 워크트리 루트(`D:\Project\space-a\.claude\worktrees\feat+life-spawn-scatter`)에서 실행.
- 테스트 명령: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q`
- 수정 허용 파일: `a-hub/life/life_server/life.py`, `a-hub/life/tests/test_life.py` **둘뿐**. api.py·store.py·contracts/·a-mate 절대 무변경.
- 버퍼 = Chebyshev 거리 **3**(사이 빈 타일 2칸), 완화 사다리 **3 → 2 → 없음**, 전부 실패 시 `CellTaken("방이 가득 참")` 유지.
- 해시는 **`hashlib.sha256`** — 내장 `hash()` 금지(프로세스 솔트 때문에 재시작 간 비결정).
- 명시 cell `enter`·`move`의 버퍼 미적용·정확-셀 409 의미는 건드리지 않는다.
- 커밋: Conventional Commits **영어**, scope `backend`, 푸터 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: 버퍼 + 결정적 해시 분산 스폰 코어

**Files:**
- Modify: `a-hub/life/life_server/life.py` (SPAWN 상수 주석 25-27행, `enter` 463행, `_free_cell_locked` 579-588행)
- Test: `a-hub/life/tests/test_life.py` (기존 `test_auto_spawn_at_spawn_point_then_nearby` 48-56행 교체 + 신규 4개)

**Interfaces:**
- Consumes: 기존 `LifeService` 공개 API (`register`, `enter`, `me`, `life_state`, `set_design`), 상수 `SPAWN_X=8`, `SPAWN_Y=16`, `GRID_W=GRID_H=20`, `FLOOR_Y=0`.
- Produces: `_free_cell_locked(self, life_id: str, for_agent: str = "") -> Cell` — for_agent는 이 태스크에서는 **해시 시드로만** 사용(자기 제외는 Task 3). 모듈 함수 `_spawn_hash(agent_id: str, cell: Cell) -> int`. Task 2·3이 이 시그니처를 그대로 확장한다.

- [ ] **Step 1: 실패하는 테스트 작성**

`a-hub/life/tests/test_life.py`의 `test_auto_spawn_at_spawn_point_then_nearby`(48-56행)를 아래로 **교체**:

```python
def test_auto_spawn_at_spawn_point_then_buffered(life):
    _, token, created_life = life.register("A")
    # 빈 방의 첫 스폰 = 스폰 지점 (구석 아님) — 회귀 가드
    assert life.me(token)["cell"] == [SPAWN_X, SPAWN_Y]
    # 스폰 지점에 주인이 있으면 방문자는 버퍼(체비셰프 거리 3)만큼 떨어져 배정
    _, token_b, _ = life.register("B")
    life.enter(token_b, created_life.id, cell=None)
    bx, by = life.me(token_b)["cell"]
    assert max(abs(bx - SPAWN_X), abs(by - SPAWN_Y)) == 3
```

파일 끝에 신규 테스트 4개 추가:

```python
def test_visit_spawn_keeps_buffer_between_agents(life):
    _, _, owner_life = life.register("owner")
    _, token_b, _ = life.register("B")
    _, token_c, _ = life.register("C")
    life.enter(token_b, owner_life.id, cell=None)
    life.enter(token_c, owner_life.id, cell=None)
    cells = [tuple(o["cell"]) for o in life.life_state(owner_life.id)["occupants"]]
    assert len(cells) == 3  # 주인 + 방문자 2
    for i in range(len(cells)):
        for j in range(i + 1, len(cells)):
            (x1, y1), (x2, y2) = cells[i], cells[j]
            assert max(abs(x1 - x2), abs(y1 - y2)) >= 3


def test_visit_spawn_is_deterministic_for_same_agent(life):
    _, _, owner_life = life.register("owner")
    _, token_b, b_life = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    first = life.me(token_b)["cell"]
    life.enter(token_b, b_life.id, cell=None)      # 집으로 돌아감
    life.enter(token_b, owner_life.id, cell=None)  # 같은 방 상태에서 재방문
    assert life.me(token_b)["cell"] == first


def test_visit_spawn_scatters_across_agents(life):
    _, _, owner_life = life.register("owner")
    cells = set()
    for i in range(5):
        _, token, guest_life = life.register(f"G{i}")
        life.enter(token, owner_life.id, cell=None)
        cells.add(tuple(life.me(token)["cell"]))
        life.enter(token, guest_life.id, cell=None)  # 다음 프로브를 위해 집으로
    # 매번 '주인만 있는 방'이라는 같은 조건 — 에이전트별 해시로 자리가 전부 같지는 않다
    assert len(cells) >= 2


def test_visit_spawn_avoids_furniture_footprint(life):
    _, owner_token, owner_life = life.register("owner")
    # 앵커 위쪽 거리 3 링 일부를 소파(7x2)로 덮어도 가구 위에는 스폰되지 않는다
    life.set_design(owner_token, owner_life.id, {"objects": [
        {"asset_id": "sofa.big", "category": "sofa", "cell": [SPAWN_X - 3, SPAWN_Y - 3],
         "size": [7, 2], "rotation": 0},
    ]})
    _, token_b, _ = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    bx, by = life.me(token_b)["cell"]
    blocked = {(SPAWN_X - 3 + dx, SPAWN_Y - 3 + dy) for dx in range(7) for dy in range(2)}
    assert (bx, by) not in blocked
```

- [ ] **Step 2: 실패 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q`
Expected: FAIL 3개 — `test_auto_spawn_at_spawn_point_then_buffered`(현행은 거리 1 배정), `test_visit_spawn_keeps_buffer_between_agents`(거리 1 < 3), `test_visit_spawn_scatters_across_agents`(현행은 모두 같은 최근접 셀 → `len(cells) == 1`). `deterministic`(현행도 안정 정렬이라 결정적)·`avoids_furniture`(현행도 가구 회피)는 통과함 — 행동 고정용이니 그대로 진행.

- [ ] **Step 3: 구현**

`a-hub/life/life_server/life.py` 25-27행 상수 주석 정정:

```python
# 자율 입장(셀 미지정) 스폰 지점 — 구석이 아니라 방의 가로 3/7, 세로 4/5 지점 근처
SPAWN_X = GRID_W * 3 // 7  # 8
SPAWN_Y = GRID_H * 4 // 5  # 16
```

`_validate_cell` 함수(87행) 바로 위에 모듈 함수 추가:

```python
def _spawn_hash(agent_id: str, cell: Cell) -> int:
    """등거리 스폰 후보 타이브레이크 — 에이전트마다 다르고 프로세스 재시작에도 같은 순서.

    내장 hash()는 프로세스별 솔트 때문에 재시작 간 비결정이라 sha256을 쓴다.
    """
    digest = hashlib.sha256(f"{agent_id}:{cell[0]}:{cell[1]}".encode()).digest()
    return int.from_bytes(digest[:8], "big")
```

`_free_cell_locked`(579-588행)를 아래로 교체:

```python
    def _free_cell_locked(self, life_id: str, for_agent: str = "") -> Cell:
        """자율 입장용 빈 셀 배정 — 앵커에서 가까운 순, 다른 에이전트와 거리 버퍼 우선.

        후보는 (앵커 체비셰프 거리, 해시(for_agent, cell)) 순 — 같은 링 위에서는
        에이전트마다 다른 칸을 골라 흩어진다(결정적). 버퍼를 만족하는 칸이 없으면
        버퍼 없이 현행대로 가장 가까운 빈 칸.
        """
        life = self._life.get(life_id)
        agent_cells = {a.cell for a in self._agents.values() if a.at_life == life_id}
        object_cells: set[Cell] = set()
        if life:
            for o in life.design.objects:
                if o.category != "window":
                    object_cells |= o.occupied_cells()
        free = [
            c for c in sorted(
                ((x, y) for y in range(FLOOR_Y, GRID_H) for x in range(GRID_W)),
                key=lambda c: (max(abs(c[0] - SPAWN_X), abs(c[1] - SPAWN_Y)), _spawn_hash(for_agent, c)),
            )
            if c not in agent_cells and c not in object_cells
        ]
        for buffer in (3,):
            for cell in free:
                if all(max(abs(cell[0] - ax), abs(cell[1] - ay)) >= buffer for ax, ay in agent_cells):
                    return cell
        if free:
            return free[0]
        raise CellTaken("방이 가득 참")
```

`enter`(463행)의 호출부만 변경 — `register`(157행)는 그대로 둔다(신규 에이전트의 빈 방 첫 스폰):

```python
            target = cell if cell is not None else self._free_cell_locked(life_id, for_agent=agent.agent_id)
```

- [ ] **Step 4: 통과 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q`
Expected: 전부 PASS (기존 테스트 포함 — `test_auto_cell_assignment_no_overlap`은 셀 중복 없음만 단언하므로 계속 통과해야 한다)

- [ ] **Step 5: 커밋**

```powershell
git add a-hub/life/life_server/life.py a-hub/life/tests/test_life.py
git commit -m "feat(backend): scatter visit spawn cells with distance buffer

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 완화 사다리 (3 → 2 → 없음) + 만석 시나리오

**Files:**
- Modify: `a-hub/life/life_server/life.py` (Task 1이 만든 `_free_cell_locked`의 버퍼 튜플 1곳)
- Test: `a-hub/life/tests/test_life.py` (헬퍼 2개 + 신규 3개)

**Interfaces:**
- Consumes: Task 1의 `_free_cell_locked(life_id, for_agent="")` — `for buffer in (3,):` 사다리.
- Produces: 사다리 `for buffer in (3, 2):`로 확장된 동일 시그니처. 테스트 헬퍼 `_fill_grid_agents(life, owner_life_id)`, `_cover_floor_except(life, owner_token, life_id, holes)` (Task 3은 이 헬퍼를 쓰지 않음).

- [ ] **Step 1: 실패하는 테스트 작성**

`a-hub/life/tests/test_life.py` 끝에 헬퍼 2개와 테스트 3개 추가:

```python
def _fill_grid_agents(life, owner_life_id):
    """{0,4,8,12,16}² 격자점 25곳에 에이전트 배치 — 어떤 빈 칸도 최근접 에이전트가 거리 2 이하."""
    for gx in (0, 4, 8, 12, 16):
        for gy in (0, 4, 8, 12, 16):
            if (gx, gy) == (SPAWN_X, SPAWN_Y):
                continue  # 주인이 이미 앵커 (8,16)에 서 있다
            _, token, _ = life.register(f"grid-{gx}-{gy}")
            life.enter(token, owner_life_id, cell=(gx, gy))


def _cover_floor_except(life, owner_token, life_id, holes):
    """바닥 전체를 가구 footprint로 덮되 holes만 비운다. 주인이 선 칸은 holes에 포함해야 한다."""
    objects = []
    for bx in (0, 8, 16):
        for by in (0, 8, 16):
            w, h = min(8, GRID_W - bx), min(8, GRID_H - by)
            cells = [[x, y] for y in range(h) for x in range(w) if (bx + x, by + y) not in holes]
            if cells:
                objects.append({"asset_id": f"block-{bx}-{by}", "category": "block",
                                "cell": [bx, by], "size": [w, h], "footprint": cells, "rotation": 0})
    life.set_design(owner_token, life_id, {"objects": objects})


def test_spawn_buffer_relaxes_to_two_when_three_impossible(life):
    _, _, owner_life = life.register("owner")  # 주인 스폰 = 앵커 (8,16)
    _fill_grid_agents(life, owner_life.id)
    _, token_v, _ = life.register("visitor")
    life.enter(token_v, owner_life.id, cell=None)
    vx, vy = life.me(token_v)["cell"]
    dists = [max(abs(vx - o["cell"][0]), abs(vy - o["cell"][1]))
             for o in life.life_state(owner_life.id)["occupants"] if o["name"] != "visitor"]
    # 격자 간격 4라 어떤 칸도 거리 3 불가·2는 가능 — 사다리가 2로 완화한 자리여야 한다
    assert min(dists) == 2


def test_spawn_buffer_fully_relaxes_before_full(life):
    _, owner_token, owner_life = life.register("owner")  # 주인 = (8,16)
    _cover_floor_except(life, owner_token, owner_life.id,
                        holes={(SPAWN_X, SPAWN_Y), (SPAWN_X, SPAWN_Y + 1)})
    _, token_v, _ = life.register("visitor")
    life.enter(token_v, owner_life.id, cell=None)
    # 남은 빈 칸이 주인 옆칸뿐 — 409가 아니라 버퍼 없이 배정된다
    assert life.me(token_v)["cell"] == [SPAWN_X, SPAWN_Y + 1]


def test_spawn_full_room_still_rejects(life):
    _, owner_token, owner_life = life.register("owner")
    _cover_floor_except(life, owner_token, owner_life.id, holes={(SPAWN_X, SPAWN_Y)})
    _, token_v, _ = life.register("visitor")
    with pytest.raises(CellTaken):
        life.enter(token_v, owner_life.id, cell=None)
```

- [ ] **Step 2: 실패 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q -k spawn`
Expected: `test_spawn_buffer_relaxes_to_two_when_three_impossible` **FAIL** (현행 Task 1 사다리 `(3,)`은 3 실패 시 곧장 버퍼 없이 배정 → 거리 1 자리 선택, `min(dists) == 1`). 나머지 2개는 Task 1 구현에서도 통과할 수 있음(행동 고정용 — 통과해도 그대로 진행).

- [ ] **Step 3: 구현**

`a-hub/life/life_server/life.py`의 `_free_cell_locked`에서 버퍼 사다리 한 줄 변경:

```python
        for buffer in (3, 2):
```

- [ ] **Step 4: 통과 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q`
Expected: 전부 PASS

- [ ] **Step 5: 커밋**

```powershell
git add a-hub/life/life_server/life.py a-hub/life/tests/test_life.py
git commit -m "feat(backend): relax spawn buffer gradually when room is crowded

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 자동 재입장 시 자기 옛 자리 제외

**Files:**
- Modify: `a-hub/life/life_server/life.py` (`_free_cell_locked`의 agent_cells 계산 1곳)
- Test: `a-hub/life/tests/test_life.py` (신규 1개)

**Interfaces:**
- Consumes: Task 1·2의 `_free_cell_locked(life_id, for_agent="")` — for_agent는 현재 해시 시드로만 쓰임. `enter`는 이미 `for_agent=agent.agent_id`를 넘긴다.
- Produces: for_agent가 **점유·버퍼 계산에서도 자기 자신을 제외**하는 완성형 `_free_cell_locked`. 이후 태스크 없음.

- [ ] **Step 1: 실패하는 테스트 작성**

`a-hub/life/tests/test_life.py` 끝에 추가:

```python
def test_reenter_same_room_keeps_own_cell(life):
    _, _, owner_life = life.register("owner")
    _, token_b, _ = life.register("B")
    life.enter(token_b, owner_life.id, cell=None)
    first = life.me(token_b)["cell"]
    # 같은 방 자동 재입장(재연결 경로) — 자기 옛 자리가 점유·버퍼로 잡히면 자리가 튄다
    life.enter(token_b, owner_life.id, cell=None)
    assert life.me(token_b)["cell"] == first
```

- [ ] **Step 2: 실패 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q -k reenter`
Expected: FAIL — 자기 옛 셀이 agent_cells에 들어가 다른 셀로 밀려남 (`first`와 다른 값)

- [ ] **Step 3: 구현**

`_free_cell_locked`의 agent_cells 한 줄 변경:

```python
        agent_cells = {a.cell for a in self._agents.values()
                       if a.agent_id != for_agent and a.at_life == life_id}
```

- [ ] **Step 4: 통과 확인**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests\test_life.py -q`
Expected: 전부 PASS

- [ ] **Step 5: 커밋**

```powershell
git add a-hub/life/life_server/life.py a-hub/life/tests/test_life.py
git commit -m "feat(backend): exclude self from spawn buffer on re-entry

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 전체 검증

**Files:**
- 수정 없음 — 검증만.

**Interfaces:**
- Consumes: Task 1~3 완료 상태.
- Produces: life 전체 스위트(단위+API+store) 녹색 확인.

- [ ] **Step 1: life 전체 테스트**

Run: `a-hub\life\.venv\Scripts\python -m pytest a-hub\life\tests -q`
Expected: **55 passed** (베이스라인 47 + 신규 8), 0 failed

- [ ] **Step 2: 변경 파일 범위 확인**

Run: `git diff --stat origin/main...HEAD -- a-hub`
Expected: `a-hub/life/life_server/life.py`와 `a-hub/life/tests/test_life.py` 두 파일만

---

## 완료 후 (플랜 범위 밖 — 세션에서 이어서)

1. **PR 생성** (`superpowers:finishing-a-development-branch`): base `main`, 제목 `feat(backend): scatter life visit spawn cells`. PR 체크리스트에 **온프레(OCI VM) 배포 후 실환경 스모크(방문 시 겹침 없음 육안 확인) = 사용자 몫** 명시.
2. **docs-archive DoD** (ADR 0013): 같은 PR에서 `docs-archive` 스킬로 본 plan·spec을 `docs/archive/` 미러로 이동.
3. **로드맵 V1 완료 기록**: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md`는 G7 세션과 공유 — **작업 마지막에 main 최신 반영 후** 구현 결과를 기록·커밋해 충돌 회피.
