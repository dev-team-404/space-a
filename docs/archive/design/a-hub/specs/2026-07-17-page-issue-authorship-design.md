---
status: done
archived: 2026-07-19
---

# Page·Issue 생성자(작성자) 저장·조회 설계

**날짜:** 2026-07-17
**상태:** 승인됨 (구현 예정)
**범위:** `a-hub/work` — 생성자 정보 저장·조회까지. 세밀한 접근 제어는 현재 제품 범위 밖.

## 목표

토큰으로 인증한 에이전트(`agent_id`) 기준으로 Page·Issue의 **생성자를 기록**하고,
조회 응답에도 그 정보를 **함께 반환**한다.

- **Issue**: `opened_by`(agent_id)는 이미 저장·조회됨. 변경 없음.
- **Page**: 생성자 필드가 없음 → `created_by`(agent_id)를 추가한다.

## 핵심 결정

| 결정 | 선택 | 근거 |
|------|------|------|
| 반환 형태 | **agent_id만** (name 생략) | 가장 단순. name은 호출측이 `GET /agents/{id}`로 조회. 요구의 핵심(작성자 추적) 충족 |
| 하위호환 | `created_by: str \| None = None` | 기존 `opened_by`와 동일 패턴. 기존 레코드는 `None`으로 읽힘, 마이그레이션 불필요 |
| 설계 방식 | 기존 `opened_by` 패턴 미러링 | 필드 하나뿐 → 공통 authorship 추상화는 YAGNI |

## 변경 상세

### 1. 데이터 모델 — `core/models.py`

`Page`에 필드 하나 추가 (dataclass 기본값 필드):

```python
created_by: str | None = None   # 작성한 agent_id (issue-derived면 resolve한 agent)
```

`Issue.opened_by`는 이미 존재 → 변경 없음.

### 2. 서비스 로직 — `core/services.py`

인증된 `agent.id`로 `created_by`를 채운다 (두 메서드 모두 이미 `agent`를 갖고 있음):

- `create_page`: `Page(..., created_by=agent.id)`
- `resolve_issue`: 발행 Page에 `created_by=agent.id` (resolve한 에이전트)

`open_issue`의 `opened_by=agent.id`는 이미 있음 → 변경 없음.

### 3. 저장소 3곳

| 저장소 | 대응 |
|--------|------|
| `store_memory.py` | dataclass 그대로 보관 → **변경 없음** |
| `store_dynamodb.py` | `asdict`/`cls(**d)` 직렬화 → **변경 없음** (기존 아이템은 dataclass 기본값 `None`으로 복원) |
| `store_sqlite.py` | 명시적 컬럼 스키마 → **변경 필요** |

SQLite 변경:
- `CREATE TABLE pages`에 `created_by TEXT` 컬럼
- `add_page` INSERT 컬럼·값에 `created_by`
- `_page` 행→객체 복원에 `created_by=r["created_by"]`
- **기존 파일 하위호환**: 초기화 시 `ALTER TABLE pages ADD COLUMN created_by TEXT`를
  idempotent하게 시도(이미 있으면 무시). 프로덕션은 DynamoDB라 실제 영향은 로컬/Docker 한정.

### 4. API 응답

**REST (`rest_server.py`)** — Page 응답에 `created_by` 추가:
- `POST /spaces/{id}/pages` (생성 응답)
- `GET /pages/{id}`
- `POST /pages/search` (각 결과 항목)
- `GET /spaces/{id}/tree` (각 node)

Issue 응답(`GET /issues`, `GET /issues/{id}`)은 `opened_by`가 이미 노출됨 → 변경 없음.

**MCP (`mcp_server.py`)**:
- `search_knowledge` 결과 항목에 `created_by` 추가.
- `get_guide`는 가이드 전용 → 저자 노출 제외(기존 스키마 유지).

## 테스트 (TDD)

새 파일 `tests/test_authorship.py` — memory·sqlite 양쪽(conftest `service` 픽스처) + REST:

1. `create_page` → `created_by` = 인증 agent.id
2. `resolve_issue` 발행 Page의 `created_by` = resolve한 agent.id
3. `GET /pages/{id}` 응답에 `created_by` 포함
4. `POST /pages/search` 결과에 `created_by` 포함
5. 서로 다른 두 에이전트가 쓴 Page의 `created_by`가 각각 다름
6. (하위호환) `created_by` 없는 기존 Page 로드 시 `None`
7. MCP `search_knowledge` 결과에 `created_by` 포함

각 테스트는 RED 확인 후 GREEN.

## 범위 밖

- 작성자/방 기반 세밀한 접근 제어 (현재 제품 범위 밖 — 별도 결정 문서 참조).
- 작성자 `name`의 조회-시 lookup 또는 denormalize (지금은 agent_id만).
- 기존 손상/무저자 데이터 백필.
