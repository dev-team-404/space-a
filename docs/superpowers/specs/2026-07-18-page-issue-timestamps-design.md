# Page·Issue 생성/수정 시각 (created_at·updated_at) 설계

**날짜:** 2026-07-18
**상태:** 승인됨 (구현 예정)
**범위:** `a-hub/work` — Page·Issue에 타임스탬프 저장·조회.

## 목표

Page·Issue에 **생성 시각**과 **마지막 수정 시각**을 기록하고 조회 응답에 반환한다.

## 결정

| 항목 | 결정 |
|------|------|
| 대상 | Page·Issue 둘 다, `created_at` + `updated_at` |
| 형식 | ISO 8601 UTC 문자열 (`datetime.now(timezone.utc).isoformat()`) |
| 생성 위치 | 서비스 레이어 (`SpaceAService`) |
| 시간 소스 | 주입 가능한 clock (`__init__(..., now=None)`, 기본 UTC now) |
| 하위호환 | `str \| None = None`, 마이그레이션 불필요, SQLite idempotent ALTER |

## 변경 상세

### 1. 모델 — core/models.py

Page·Issue에 각각:
```python
created_at: str | None = None   # ISO 8601 UTC, 생성 시각
updated_at: str | None = None   # ISO 8601 UTC, 마지막 수정 시각
```

### 2. 서비스 — core/services.py

```python
def __init__(self, store, *, now=None):
    self.store = store
    self._now = now or (lambda: datetime.now(timezone.utc).isoformat())

def _touch(self, obj):        # 저장 직전 updated 갱신 (빠뜨림 방지)
    obj.updated_at = self._now()
```

- **생성** (created_at = updated_at = now): `open_issue`(Issue), `create_page`·`resolve_issue` 발행 Page,
  공간 생성 시 seed되는 guide Page.
- **수정** (updated_at만 갱신, created 보존): `edit_page`·`move_page`·`set_visibility`·`archive_page`·
  `supersede_page`·`quarantine_page`·`flag_page`, 그리고 Issue의 `resolve_issue`·`cite_knowledge`.
- 규칙: `save_page`/`save_issue`(상태 변경 저장)를 부르기 직전 `_touch`. 단순 조회는 절대 안 찍음.
- `__init__`에 `now` 기본값이 있어 기존 `SpaceAService(store)` 호출부(23곳)는 수정 불필요.

### 3. 저장소 3곳

| 저장소 | 대응 |
|--------|------|
| store_memory | dataclass 그대로 → 변경 없음 |
| store_dynamodb | asdict/cls(**d) → 변경 없음 (기존 아이템 기본값 None) |
| store_sqlite | 변경 필요 (아래) |

SQLite:
- `pages`·`issues` 테이블에 `created_at TEXT`, `updated_at TEXT` 컬럼.
- `add_page`/`add_issue` INSERT + `_page`/`_issue` 복원에 두 필드.
- `_migrate()`에 idempotent ALTER (pages·issues 각각 두 컬럼) — created_by 패턴 확장.

### 4. API 응답

- **REST**: Page 응답(create/get/search/tree) + Issue 응답(list/get)에 `created_at`·`updated_at`.
- **MCP**: `search_knowledge` 결과에 `created_at`·`updated_at`.

## 테스트 (TDD, memory·sqlite 양쪽) — 고정 clock 주입

`SpaceAService(store, now=lambda: "2026-01-01T00:00:00+00:00")`:

1. `create_page` → created_at == updated_at == 주입값
2. `open_issue` → created/updated 세팅
3. 수정 시 updated만 갱신: clock t1→t2, `edit_page` 후 created_at=t1 유지, updated_at=t2
4. `resolve_issue`·`cite_knowledge`가 issue updated 갱신
5. REST 응답에 두 필드 포함
6. (하위호환) 타임스탬프 없는 기존 레코드 로드 시 None
7. SQLite 레거시 파일 마이그레이션 (구스키마 → ALTER 후 None, 신규 write 보존)
8. MCP search 결과에 두 필드 포함

## 범위 밖

- 기존 데이터 백필
- 타임스탬프 기반 정렬·필터 API
- 타임존 변환 (UTC 고정)
