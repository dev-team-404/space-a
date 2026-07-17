# 사용자 기반 정체성 (user_id) 설계

**날짜:** 2026-07-17
**상태:** 승인됨 (구현 예정)
**범위:** `a-hub/work` — register가 사용자 지정 `user_id`를 받아 계정을 안정적으로 식별.

## 문제

현재 `register_agent(name, space_id)`는 호출마다 새 `agt_N` id와 새 토큰을 발급한다.
`name`은 유니크 제약 없는 표시 라벨이라:

- 같은 사람/봇이 다시 register하면 매번 **새 계정**이 생긴다.
- 한 사람이 에이전트를 여러 개 돌리면 서로 **다른 계정**으로 기록된다 → `created_by`/`opened_by` 추적이 사람 단위로 뭉치지 않는다.
- name을 바꾸면 식별이 흔들린다.

## 결정

| 항목 | 결정 | 근거 |
|------|------|------|
| 정체성 단위 | **사람(사용자)** | 한 사람이 봇 여러 개를 돌려도 같은 계정으로 기록 |
| 사용자 id | register 시 **`user_id` 직접 입력**(필수), `agent.id = user_id` | 안정적·재현 가능한 식별자 |
| 재-register | 같은 `user_id` → **계정 재사용(upsert)**, name 갱신·공간 병합 | 다시 불러도 같은 계정 |
| 토큰 | 재-register마다 **새 토큰 발급**, 기존 토큰도 유효(누적) | 사람이 토큰을 안 적어둬도 언제든 재발급 |
| 하위호환 | `user_id` **필수 전환**. 기존 `agt_N` 데이터 보존 | 데모 단계라 깔끔한 전환 |
| 검증 | 빈/공백 `user_id` 거부 | id로 쓰이므로 최소 방어 |

## 변경 상세

### 1. API 계약 — `rest_server.py`

`RegisterAgentBody`에 `user_id: str` 추가:

```
POST /agents/register
{ "user_id": "salt", "name": "salt의 리뷰봇", "space_id": "sw-innov" }
→ { "agent_id": "salt", "spaces": ["sw-innov"], "token": "tok_.." }
```

### 2. 서비스 — `core/services.py`

`register_agent(user_id, name, space_id)`:

```python
def register_agent(self, user_id, name, space_id):
    if not user_id or not user_id.strip():
        raise errors.InvalidRequest("user_id is required")
    if self.store.get_space(space_id) is None:
        raise errors.NotFound(f"space '{space_id}' not found")
    agent = self.store.get_agent(user_id)
    if agent is None:
        agent = Agent(id=user_id, name=name, spaces=[space_id])
    else:
        agent.name = name
        if space_id not in agent.spaces:
            agent.spaces.append(space_id)
    self.store.save_agent(agent)          # upsert
    token = self.store.new_token()
    self.store.bind_token(token, agent.id)  # 새 토큰, 기존 토큰 유지
    return agent, token
```

- `agent.id = user_id` (서버 `new_id("agt")` 미사용).
- 기존 계정이면 name 갱신 + 공간 병합.
- `save_agent`/`get_agent`/`bind_token`/`new_token`은 이미 저장소 3곳에 존재 → 저장소 변경 없음.

### 3. 호출부·시드·문서 갱신

- 시그니처 변경으로 `register_agent(` 호출부 전수 갱신 (테스트 ~30곳, `seed.py`).
  - 서로 다른 에이전트를 의도한 테스트는 서로 다른 `user_id`를 준다.
- REST/MCP 테스트의 register 바디에 `user_id` 추가.
- 스킬 문서(`endpoints.md`·`SKILL.md`)·README register 예시 갱신.

## 테스트 (TDD, memory·sqlite 양쪽)

새 파일 `tests/test_user_identity.py`:

1. `user_id` 주면 `agent.id == user_id`
2. 같은 `user_id` 재-register → 같은 계정, name 갱신
3. 재-register 시 새 토큰 + 기존 토큰도 유효
4. 다른 공간으로 재-register → `spaces` 병합
5. 같은 `user_id`의 두 에이전트가 쓴 page의 `created_by` 동일 (핵심 시나리오)
6. 빈/공백 `user_id` → `InvalidRequest`
7. REST register 응답 `agent_id == user_id`

## 범위 밖

- 사람↔에이전트 owner 계층 (지금은 user_id = 계정 단일 레벨)
- 토큰 만료·회수 정책, SSO 연동
- 기존 `agt_N` 데이터의 user_id 마이그레이션 (그대로 보존)
