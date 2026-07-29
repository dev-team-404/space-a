# Life · Hub · Lens 공통 신원 — 설계 스펙

> **목적**: 같은 사람이 세 컴포넌트에서 같은 사람으로 보이게 한다. a-lens가 Life 서버의
> **이름·마스코트 이미지**로 사람을 그리고, 그 사람의 **Hub(work) 활동**을 같은 카드에 붙인다.

## 0. 30초 요약

| 항목 | 결정 |
|---|---|
| 신원 등록처 | **Life 서버** — a-mate가 이미 보내는 값(UUID·조직·주인 계정)을 저장·노출한다 |
| Hub 연결 키 | Life에 `hub_user_id`를 두고, 값이 있으면 그것을 정답으로 쓴다 |
| 자동 매칭 | `hub_user_id`가 없으면 이름 정규화 → 주인 OS 계정 순으로 시도 |
| 중복 계정 | 한 사람이 허브 계정 여러 개 → 한 줄로 합치고 대표는 **a-mate로 등록한 계정** (§3.4) |
| 표시 | a-lens 방 캐릭터·팀 활동에 Life 이름 + 마스코트 이미지, 활동 정보는 Hub에서 |
| 수정 범위 | **Life(a-hub/life) · Lens(a-lens)만.** a-mate는 손대지 않는다 |

## 1. 지금 무엇이 끊겨 있나 (2026-07-29 실측)

a-mate가 각 시스템에 보내는 값이 겹치지 않는다.

| 경로 | 보내는 것 | 서버가 저장하는 것 |
|---|---|---|
| a-mate → **Life** | `name`(닉네임) · `mascot_seed`=로컬 UUID · `org` · `agent_uuid`=같은 UUID · `owner_os_user` · `owner_full_name` | name · mascot_seed · org · agent_uuid ✅ / **`owner_*`는 버림** ❌ |
| a-mate → **Hub(work)** | `user_id`(=`SPACE_A_USER` 또는 `user_name`) · `name`=`a-mate/<user_id>` | agent_id = user_id ✅ |
| **a-lens** | — | Hub(work)만 읽음. Life 연결 없음 ❌ |

그래서 이름 말고는 이을 값이 없고, 이름은 닉네임이라 실제로 안 맞는다:

| Life 이름 | Hub 계정 | 이름 매칭 |
|---|---|---|
| `palendy` | `a-mate/palendy` | ✅ 접두어 제거 후 일치 |
| `kimmy` | `kimmy-claude` | ⚠️ 부분 일치 |
| `준냥헐` · `돌쇠` · `소금맛` | `junnyung` · ? · `a-mate/salt.jeong` | ❌ 불가 |

**결론: 자동 매칭만으로는 못 푼다.** 사람이 한 번 지정한 값을 어딘가 저장해야 하고, 그 자리는
Life다 — a-mate·a-lens·(나중에) 다른 소비자가 모두 읽는 유일한 공통 지점이기 때문이다.

## 2. Life = 신원 등록처

### 2.1 이미 오는 값을 버리지 않는다

`owner_os_user`·`owner_full_name`은 a-mate가 **지금도 보내고 있는데** 서버가 스키마에 없어서
버린다. 컬럼을 추가해 저장하고 조회에 노출한다. 사람 이름(풀네임)은 팀원이 서로를 알아보는
가장 자연스러운 라벨이고, OS 계정은 Hub `user_id`와 겹칠 가능성이 높은 후보 키다.

### 2.2 `hub_user_id` — 사람이 한 번 지정하는 연결

```
PATCH /life/agents/{agent_id}/hub-user   { "hub_user_id": "salt.jeong" }
```

- 값이 있으면 **그것이 정답**이다. 자동 추론은 값이 없을 때만 돈다.
- 누가 고치나: 본인 토큰이면 자기 것을, 관리 키(`LIFE_SERVER_API_KEY`)를 가진 호출자는 남의 것도.
  (a-lens 설정 창에서 팀 전체를 한 번에 채우는 시나리오 — 관리자 1명이 정리하면 끝난다)
- 빈 문자열로 보내면 연결 해제(자동 추론으로 복귀).

### 2.3 조회 응답에 신원 블록 추가

`GET /life/people`, `GET /life/{life_id}`, `GET /life/me`에 아래를 실어 준다(하위호환 — 추가만):

```json
{
  "agent_id": "ragt_b63d2af8",
  "name": "palendy",
  "identity": {
    "agent_uuid": "05f5b736-…",
    "org": "sw-innov",
    "owner_os_user": "palendy",
    "owner_full_name": "박팔렌",
    "hub_user_id": "palendy",
    "mascot_image_sha256": "6c13b93…"
  }
}
```

## 3. Lens = 조인과 표시

### 3.1 연결 순서 (앞이 이기면 뒤는 보지 않는다)

1. Life `identity.hub_user_id` — 사람이 지정한 값
2. 이름 정규화 일치 — 소문자화 · `a-mate/` 접두어 제거 · 공백/`.`/`-`/`_` 제거 후 비교
3. `owner_os_user` 정규화 일치
4. 실패 → **Life 정보만으로 표시**한다(이름·아이콘은 보이고 활동 정보는 빔). 사람을 화면에서
   지우지 않는다 — "연결 안 됨"은 데이터 상태이지 그 사람이 없는 게 아니다.

### 3.2 표시

- 방 캐릭터: 절차 생성 로봇 대신 **Life 마스코트 이미지**. 이름표는 Life 이름.
- 팀 활동 목록: 줄 앞에 작은 마스코트 아이콘.
- 상세 패널: 마스코트 + Life 이름 + (연결됐으면) Hub 최근 활동·문서 수.
- 마스코트는 a-lens 백엔드가 **프록시**한다(`GET /api/life-mascot/{agent_id}`) — 프론트가 Life
  토큰을 들고 있지 않게. sha256을 ETag로 써서 브라우저 캐시를 살린다.

### 3.3 설정

a-lens 설정 창에 Life 연결 3개를 추가한다: `life_url` · `life_token` · `life_api_key`.
`life_url`이 비면 Life 연동 전체가 **조용히 off**(기존 Hub-only 화면 그대로) — 기존 설정
관례(`work_*`, `llm_*`)와 같다.

### 3.4 한 사람이 허브 계정을 여러 개 가질 때 (2026-07-29 추가)

§3.1은 "한 허브 계정 → 한 사람"만 풀었다. 실제로는 **한 사람이 허브 계정을 여러 개** 갖는다 —
등록 경로마다 id가 달라서다:

| 사람 | 허브 계정 | 등록 경로 |
|---|---|---|
| 돌쇠 | `coolfebreeze` · `palen` | a-mate (`SPACE_A_USER` 미설정 시 Windows `USERNAME`이 들어간다) |
| kimmy | `kimmy-mate` · `kimmy-claude` · `agt_9` | a-mate · Claude Code 스킬 · 초기 자동배정 id |

조인만으로는 부족하다. 계정 둘이 같은 Life 사람에 붙어도 **줄은 여전히 둘**이라 같은 사람이
두 번 보인다(kimmy가 실제로 그렇게 보였다).

**결정 — 허브 원장은 고치지 않고 보는 층에서만 합친다.**

1. **별칭을 1:N으로**: `life_alias`에 `닉네임=id1|id2`로 쓴다(`돌쇠=palen|coolfebreeze`).
   같은 닉네임을 두 번 써도 덮지 않고 더한다.
2. **사람 단위 병합**: `life_agent_id`가 같은 줄을 한 줄로 접는다(`collector._merge_people`).
   - **대표는 a-mate로 등록한 계정** — 허브 표시 이름의 `a-mate/` 접두어로 판정한다. 사람이
     실제로 쓰는 본계정이고, 신원(이름·마스코트)의 출처인 Life에 값을 보내는 주체이기도 하다.
     후보가 여럿이면 최근 활동이 있는 쪽.
   - 활동(`status`·`last_active_at`·`recent_activity`)은 **계정이 아니라 사람의 것**이므로 전부
     합친다. 대표가 조용하고 활동은 옛 계정에 있는 경우가 실제로 있다(`kimmy-mate`는 기록 0건,
     실적은 `kimmy-claude`에 11건).
   - 흡수한 id는 `merged_ids`로 남긴다. 허브 원장의 작성자 id(`created_by`·`opened_by`)는 그대로
     이므로 추적이 끊기지 않는다.
   - 이슈·문서의 담당자 표시 이름도 사람 이름을 따른다 — 방에는 "돌쇠"인데 담당자는
     "a-mate/coolfebreeze"로 뜨면 같은 혼동이 되돌아온다.
3. **`life_agent_id`가 없는 줄은 합치지 않는다.** 같은 사람인지 알 방법이 없고, 추측으로 남의
   활동을 한 사람에게 몰아주는 것이 두 줄로 보이는 것보다 나쁘다.

> 별칭은 어디까지나 **Life가 `hub_user_id`를 주기 전의 임시 보정**이다(§5 1단계). Life가 배포되면
> `hub_user_id`가 1순위가 되고, 그때도 병합은 그대로 필요하다 — 계정이 여러 개인 사실 자체는
> Life 배포로 사라지지 않는다.

## 4. 실패 격리

- Life 다운·401·타임아웃(6초) → 경고 로그 + **Hub-only로 폴백**. 방은 계속 뜬다.
- 마스코트 이미지는 1MB를 넘는 것이 실측 확인됨(992×1056). 프록시가 캐시하고, 프론트는
  캐릭터 크기로 축소해 쓴다. 원본 재인코딩은 하지 않는다(품질·복잡도 대비 이득 없음).
- 매칭 실패는 오류가 아니다 — 로그 레벨 `info`, 화면은 Life 정보만.

## 5. 배포 제약

Life 서버는 팀원 PC(`http://10.116.67.127:8001`)에서 돌고 있어 **이 저장소에서 재배포할 수
없다.** 따라서:

- **1단계 (지금 가능)**: a-lens가 현재 Life API만으로 읽어서 이름·마스코트를 표시하고, 조인은
  ①이 없으니 ②③으로 시도 + a-lens 설정의 별칭 매핑으로 보정.
- **2단계 (Life 배포 후)**: `identity` 블록·`hub_user_id`가 오면 a-lens가 자동으로 그 값을
  우선 사용한다. 프론트·백엔드 코드는 1단계에서 이미 그 자리를 비워 둔다(있으면 쓰고 없으면 무시).

## 6. 코드 배치

| 위치 | 내용 |
|---|---|
| `a-hub/life/life_server/store.py` | `agents`에 `owner_os_user`·`owner_full_name`·`hub_user_id` 컬럼 추가(ALTER 마이그레이션 관례 그대로) |
| `a-hub/life/life_server/life.py` | register가 주인 필드 저장 · `set_hub_user` · 조회 응답 `identity` 블록 |
| `a-hub/life/life_server/api.py` | `PATCH /life/agents/{agent_id}/hub-user` |
| `a-lens/backend/alens/life_client.py` | Life 읽기(people·mascot) + 6초 타임아웃 + 실패 폴백 |
| `a-lens/backend/alens/collector.py` | Hub 에이전트에 Life 신원 조인(§3.1) |
| `a-lens/backend/alens/main.py` | `GET /api/life-mascot/{agent_id}` 프록시 |
| `a-lens/backend/alens/settings.py` | `life_url`·`life_token`·`life_api_key`·`life_alias`(별칭 매핑) |
| `a-lens/frontend/src/life/renderer.ts` | 캐릭터 스프라이트를 마스코트 이미지로 |

## 7. 검증

- Life 연결 없이(설정 비움) 기존 화면이 그대로 뜬다.
- 실서버(`10.116.67.127:8001`)에서 5명의 이름·마스코트를 읽어 방에 표시.
- `palendy`는 자동으로 Hub 활동이 붙고, 닉네임 3명은 별칭 매핑을 넣으면 붙는다.
- 마스코트 없는 사람(현재 `kimmy`·`소금맛`)은 기존 절차 생성 로봇으로 폴백.
- Life를 끄고도 방이 뜬다(폴백).
