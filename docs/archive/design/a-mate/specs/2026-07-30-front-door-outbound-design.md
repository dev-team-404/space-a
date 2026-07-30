---
status: done
archived: 2026-07-30
---

# 대문 아웃바운드 게시 설계 (대문사진·오늘의 한마디)

> 로드맵: [7차 배치 O1](../../../../design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md)
> 선행 선례: [Life 소셜 기능 설계](../../../../design/a-mate/specs/2026-07-22-life-social-features-design.md) · 계약 정본: [life-visit.md §4](../../../../design/life-visit.md)

## 목표

방문객이 남의 방에 들어갔을 때 **그 방 주인의 대문사진과 오늘의 한마디**를 보게 한다.

- **증상**: 대문사진(`daily_cut.png`)·오늘의 한마디는 a-mate 로컬 생성물(파일·SQLite)이라 life
  서버에 없다. `App.svelte`가 `!visiting` 조건으로 방문 중엔 카드 자체를 숨긴다.
- **해결 방향**: 아웃바운드 게시 — 주인 앱이 자기 화면에 걸린 것을 life 서버에 올리고,
  방문객은 그것을 같은 자리에 렌더한다.

### 불변식

> **방문객이 보는 대문 = 그 방 주인이 자기 화면에서 보는 대문.**

`life-visit.md §1`의 "서버가 가진 것 = 방문자가 볼 수 있는 것"을 대문에 적용한 형태다.
게시 단위는 **사진 1장 + 문장 1개**, 그게 전부다.

## 결정 요약

| 열린 결정 | 결정 | 근거 |
|---|---|---|
| 공개 범위 체계 | **게이트 없음** — `content_visibility`를 재사용하지도, 새 범위를 두지도 않는다. 상주 말풍선과 같은 수위 | ADR 0026 (§5, 구현 PR에서 신설) |
| 방문 레이아웃 | 로컬 미러링 — 초상 박스가 주인 컷으로 채워지고, 그 아래 한마디 카드 | 내 방과 구조가 같아야 "동일 화면" 불변식이 성립 |
| 서버 계약 모양 | 새 표면 3개 (텍스트 PATCH · 이미지 PUT/GET) — 말풍선·마스코트 이미지 선례와 동형 | raw PNG 바디 규율 유지, 문장만 바뀔 때 이미지 전송량 0 |
| 데이터 레벨 | agent 키로 저장하고 **방(life) 레벨로 노출** | 주인이 남의 방에 놀러 가 있어도 대문은 걸려 있어야 한다 |
| ADR | 남긴다 (0026) | 0019·0024·0025는 전부 *엔진으로* 나가는 경계였다. 이건 처음으로 *다른 사람이 읽는 서버*로 나간다 |

## 1. 서버 (a-hub/life)

### 1.1 데이터 모델

| 항목 | 저장 | 근거 |
|---|---|---|
| 대문 한마디 | `agents.daily_line TEXT NOT NULL DEFAULT ''` | `PRAGMA table_info` 가산 마이그레이션 (`bubble`·`hub_user_id` 선례, `store.py:107-120`) |
| 대문사진 | 신규 `daily_cuts(agent_id PK, png BLOB, sha256, updated_at)` | `mascot_images`와 동형 — BLOB은 별 테이블이 기존 규율 |

`LifeService`는 `_mascot_image_hashes`와 나란히 `_daily_cut_hashes: dict[str, str]`를 들고,
시작 시 `store.load_daily_cut_hashes()`로 복원한다. PNG 본문은 요청 시에만 DB에서 읽는다.

한마디를 **agent 컬럼**에 두는 이유는 마스코트 이미지와 대칭을 맞추기 위해서다 —
저장은 `agent_id` 키, 노출은 방 레벨. 방(`life`) 테이블에 두어도 동등하지만
`save_agent` 한 곳만 쓰면 되는 쪽이 변이 지점이 적다.

### 1.2 노출 (`GET /life/{id}` = `life_state`)

```diff
  "owner_mascot_seed": ...,
  "owner_mascot_image_sha256": ...,
+ "owner_daily_line": owner.daily_line if owner else "",
+ "owner_daily_cut_sha256": self._daily_cut_hashes.get(life.owner_agent_id),
```

방 레벨인 이유는 `owner_mascot_image_sha256`과 같다(`life.py:275` 주석) — 주인이 남의 방에
가 있어도 방문자가 대문을 볼 수 있어야 하므로 `occupants[].bubble`처럼 agent 레벨에 둘 수 없다.

> ⚠️ `life_state`는 **무인증**이다(`api.py:227` — `authorization` 파라미터 없음). 따라서
> 한마디 텍스트는 토큰 없이 읽힌다. 게이트 없이 공개 결정의 직접적 결과이며 ADR 0026이 수용한다.
> 지금도 `bubble`·`owner_mascot_image_sha256`이 같은 조건으로 노출되고 있다.

### 1.3 엔드포인트

| Method | Path | 계약 | 동형 선례 |
|---|---|---|---|
| PATCH | `/life/me/daily-line` | `{body}` — `strip()` 후 **≤120자**, 초과 시 400. 빈 문자열 = 지움. 응답 `{"daily_line": body}` | `set_bubble` (`life.py:538`) |
| PUT | `/life/me/daily-cut` | raw PNG 바디. PNG 시그니처 검증 + **≤5 MiB**, 위반 시 400. store 없으면 400. 기존 sha256과 같으면 쓰기 생략. 응답 `{"sha256", "size"}` | `set_mascot_image` (`life.py:415`) |
| GET | `/life/agents/{id}/daily-cut` | Bearer. `image/png` + `ETag: "<sha>"` + `Cache-Control: private, max-age=300`. 없는 agent·이미지 없음 → 404 | `mascot_image` (`life.py:431`) |

- 자수 상한 120자는 말풍선과 같은 값을 쓴다. 실제 생성물은 40자 이내(캡션·한마디 둘 다)라
  여유가 충분하고, 상한을 새로 발명하지 않는다.
- 요청 바디는 기존 `TextBody`를 재사용한다(신규 pydantic 모델 없음).
- **이미지 GET만 Bearer**인 이유: "게이트 없이 공개"는 공개*범위*(private/friends/public)를
  두지 않는다는 뜻이고, Life 인증 경계 자체는 마스코트 이미지와 같은 수위로 유지한다.
- **삭제 API는 두지 않는다.** 로컬에서 `daily_cut_enabled`를 꺼도 로컬 컷은 계속 걸려 있고
  (자동 생성만 멈춘다) 내 화면도 계속 보여주므로, 서버가 유지하는 쪽이 미러링과 일치한다.

### 1.4 계약 문서

`life-visit.md §4` 표에 위 3행을 추가한다.

> **조사 사실**: §4 표에는 `bubble`·`mascot-image`·`guestbook`·`content-visibility`가 한 줄도
> 없다 — 정본인데 소셜 엔드포인트를 반영한 적이 없다. 이 작업은 **추가하는 3행만** 적는다.
> 기존 누락 메우기는 §6 백로그로 남긴다(같은 PR에 끼우면 diff가 흐려진다).

## 2. 클라이언트 (a-mate)

### 2.1 게시 — "내 화면에 걸린 것을 그대로 올린다"

표시 문장의 단일 원천은 이미 `App.svelte`가 들고 있다: `cutCaption || dailyLine`.
대문사진이 걸려 있으면 그 사진의 캡션, 없으면 오늘의 한마디 — 어느 쪽이든 화면의 문장은 하나다.
그 **하나**를 올린다.

| 대상 | 트리거 | 경로 |
|---|---|---|
| 문장 | `$derived` 값 변화 → `$effect` | `lifeSetDailyLine(text)` |
| 사진 | 기존 `onDailyCutReady` 구독 + 앱 실행당 1회 부팅 캐치업 | `lifeSyncDailyCut()` → Rust가 `app_data_dir/daily_cut.png`를 읽어 업로드 |

```ts
// 게시용 — visiting을 절대 참조하지 않는다 (§2.4 함정 1)
const publishLine = $derived(cutCaption || dailyLine || '');
$effect(() => {
  // 첫 refresh() 완료 전에는 게시하지 않는다 (§2.4 함정 2)
  if (!homeLoaded) return;
  lifeSetDailyLine(publishLine).catch(() => {});
});
```

`homeLoaded`는 `refresh()`가 `cutCaption`·`dailyLine` 대입을 마친 뒤 `true`가 되는 플래그다.

- **PNG를 프론트로 왕복시키지 않는다.** `life_sync_daily_cut`은 `upload_cached_mascot`
  (`commands.rs:794`)과 동형으로 Rust가 파일을 읽어 바로 올린다.
- **`LifeView.svelte`가 아니라 `App.svelte`에 둔다.** 마스코트 이미지 업로드는 `LifeView`가
  하지만(`LifeView.svelte:86`) LifeView는 홈 탭 안에만 마운트되므로 다른 탭에 있으면 돌지 않는다.
  `App.svelte`는 항상 살아 있고 이미 `onDailyCutReady` 구독자다.
- **부팅 캐치업**은 `refresh()`가 아니라 `gbBootstrapped` 관용구와 같은 **일회성 가드**로 둔다
  (`cutSyncBootstrapped`, 실패 시 가드를 되돌려 다음 tick 재시도). `refresh()`는 `scan:done`마다
  불리므로 거기에 얹으면 스캔마다 PNG를 올린다. 캐치업이 필요한 이유는 `syncAllSharedDiaries`와
  같다 — 앱이 꺼진 동안의 미게시분과 구서버→신서버 배포 전환 시점을 메운다.
- 결과적으로 PNG 업로드는 **앱 실행당 1회 + 컷 생성 시마다**(일일 상한 3회) = 최대 4회/일.
- **미연결·구서버**: 신규 커맨드는 전부 자동 경로이므로 hub 미연결 시 `Ok(false)`/`Ok(None)`로
  조용히 no-op한다(`life_sync_mascot_image`·`life_mascot_image` 선례). 사용자 개시 커맨드처럼
  `Err("hub_not_connected")`를 던지지 않는다. 구서버의 404는 프론트에서 catch로 무시한다 —
  `maybe_reply_guestbook`의 `INCOMPATIBLE` 래치는 불필요하다(스캔 편승 도배 성질이 아니라
  이벤트 구동이라 호출 빈도가 낮다).

### 2.2 방문객 렌더

| 지점 | 변경 |
|---|---|
| 한마디 카드 (`App.svelte:255`) | `!visiting` 조건 제거. 문장 = `resolveHomeLine(...)` (§2.3). `ownerDailyLine`은 **기존 2초 폴링**(`lifeView()`)의 `v.life.owner_daily_line`에서 받으므로 새 네트워크 호출이 없다 |
| `RobotPortrait.svelte` | props에 `cutAgentId`·`cutVersion`(= `owner_daily_cut_sha256`) 추가. `seed`가 있으면 `cut = null`로 강제하는 16행 분기를 "방문 컷 조회"로 교체 |

초상 우선순위:

| 문맥 | 우선순위 |
|---|---|
| 내 방 | 내 컷 → 내 sprite → 시드 절차 생성 *(현행 그대로)* |
| 방문 | **주인 컷** → 주인 마스코트 이미지 → 주인 시드 절차 생성 |

`cutVersion`이 캐시 무효화 키다 — 마스코트에서 `imageVersion`이 하는 역할과 같다.
인플라이트 응답 무시(`stale` 플래그)도 기존 구조를 그대로 쓴다.

### 2.3 순수 함수 분리 (하나만)

```ts
// src/lib/home-line.ts
export function resolveHomeLine(input: {
  visiting: boolean; ownerLine: string; cutCaption: string | null; dailyLine: string | null;
}): string | null
```

- 방문 중이면 `ownerLine`(비거나 공백만이면 `null`), 내 방이면 `cutCaption || dailyLine`
  (둘 다 없으면 `null`). 반환이 `null`이면 카드를 그리지 않는다.
- 호출자는 구서버 대비로 `ownerLine: v.life.owner_daily_line ?? ''`를 넘긴다 —
  필드가 없는 서버에서는 방문 시 문장이 안 보이는 현행 동작으로 자연스럽게 폴백한다.

**이유**: 로드맵 Q3가 미해결이라 이 레포엔 타입 체크 단계가 없다 — `.svelte`의 오류는
`npm run build`(transpile-only)·`npm test` 어디에도 걸리지 않는다(실측). 판정 로직을 `.svelte`
밖으로 빼는 것이 현재 유일한 안전망이다. `RobotPortrait`의 분기는 props 기반으로 이미 단순해
추가 추출은 하지 않는다(YAGNI).

### 2.4 함정 (구현 시 반드시 지킬 것)

1. **게시 값에 `visiting`이 새어 들어가면 안 된다.** 내가 남의 방을 보고 있는 동안에도 내 대문은
   그대로여야 한다. 렌더용(`resolveHomeLine`, `visiting` 참조)과 게시용(`publishLine`,
   `visiting` 미참조)을 **별개 `$derived`로** 둔다. 하나로 합치면 남의 방에 들어간 순간
   내 서버 문장이 빈 문자열로 지워진다.
2. **첫 `refresh()` 전에는 문장을 게시하지 않는다.** `cutCaption`·`dailyLine`의 초기값은 둘 다
   `null`이라 `publishLine`이 `''`이다. `$effect`는 마운트 시점에 한 번 돌므로 가드가 없으면
   **앱 실행마다 서버 문장이 빈 문자열로 지워진다** — `refresh()`가 실패하면 영구히 지워진 채
   남는다. `homeLoaded` 플래그로 첫 게시를 첫 `refresh()` 완료 이후로 늦춘다.
3. **부팅 캐치업(사진)을 `refresh()`에 얹지 말 것** — `scan:done`마다 PNG를 올린다(§2.1).
4. `pipeline.rs` 미접촉 — 생성 훅을 건드리지 않으므로 `#[cfg(not(test))] mod runtime` 함정을
   피한다. 그래도 규율상 `cargo check --all-targets`는 돌린다.

### 2.5 신규 표면 요약

| 레이어 | 추가 |
|---|---|
| `life_client.rs` | `set_daily_line` · `upload_daily_cut` · `daily_cut` (각각 `set_bubble`/`upload_mascot_image`/`mascot_image` 동형 한 줄 래퍼) |
| `commands.rs` + `lib.rs` 등록부 | `life_set_daily_line` · `life_sync_daily_cut` · `life_daily_cut` |
| `api.ts` | 위 3개 + `LifeState`에 `owner_daily_line: string` · `owner_daily_cut_sha256?: string \| null` |
| `src/lib/home-line.ts` | `resolveHomeLine` (+ 테스트) |

## 3. 검증

| 대상 | 명령 | 내용 |
|---|---|---|
| life 서버 | `a-hub/life/`에서 `.venv\Scripts\python.exe -m pytest -q` | 아래 목록 |
| a-mate Rust | `cargo test` + `cargo check --all-targets` | 회귀 없음 확인. **신규 3메서드는 단위 테스트 대상이 아니다** — `life_client.rs`의 기존 테스트는 순수 바디 빌더만 검증하고 `set_bubble`·`upload_mascot_image`도 테스트가 없다. HTTP 래퍼의 실질 검증은 pytest + 실환경 스모크가 담당한다 |
| a-mate 프론트 | `npm test` | `resolveHomeLine`: 방문 중 주인 문장 / 주인 문장이 빈 문자열·공백이면 null / 내 방 캡션 우선 / 캡션 없으면 한마디 / 둘 다 없으면 null / **방문 중에는 내 `cutCaption`·`dailyLine`이 있어도 무시**(잔상 방지) |

### life 서버 테스트 목록

**`test_life.py`**
- `set_daily_line` 저장·조회, 120자 초과 400, 빈 문자열로 지움
- **주인이 남의 방에 입장한 상태에서도** `life_state(주인 방)`이 `owner_daily_line`·
  `owner_daily_cut_sha256`를 계속 준다 (§1.2 방 레벨 근거의 회귀 테스트)
- `set_daily_cut`: PNG 시그니처 아니면 400, 5 MiB 초과 400, 같은 sha면 재기록 생략
- `daily_cut`: 없는 agent 404, 이미지 없는 agent 404

**`test_api.py`**
- PATCH 200 + 에코, PUT 200 `{sha256, size}`, GET 200 `image/png` + `ETag` 헤더, 무토큰 GET 401

**`test_store.py`**
- `daily_cuts` 왕복 + `load_daily_cut_hashes`
- `agents.daily_line` 컬럼이 없는 기존 DB를 열어도 마이그레이션되어 동작

### 실환경 스모크

`http://10.116.67.127:8001`. ⚠️ **새 엔드포인트라 life 서버 재배포가 선행**되어야 한다
(묶음 ②·④ 스모크도 같은 이유로 대기 중). 방문객 시점 확인에는 `AGENT_MENTOR_DATA_DIR`로
분리한 두 번째 인스턴스가 필요하다.

## 4. 범위 밖

| 항목 | 이유 |
|---|---|
| **a-lens** | `/life/people` + `mascot-image`만 소비하고 방을 렌더하지 않는다(`a-lens/backend/alens/life_client.py`) → 대문사진을 표시할 지점이 없다 |
| `contracts/` | life 서버 계약 정본은 `life-visit.md §4`다 |
| 서버 컷 삭제 API | §1.3 마지막 항목 — 미러링이 이미 일치한다 |
| 상주 말풍선 통합 | 별개 표면 유지. 자동 생성물이 사용자가 직접 쓴 말풍선을 덮어쓰지 않는다 |
| 대문사진 생성 로직 | `pipeline.rs`·`sprite.rs` 미접촉. 이 작업은 **게시와 렌더만** 한다 |
| `content_visibility` 일반화 | 게이트를 두지 않기로 했으므로 `feature != "diary"` 하드 거부는 그대로 남는다 |

## 5. ADR 0026 요지

> **로컬 생성 대문(사진·한마디)을 공개범위 게이트 없이 life 서버에 게시한다**

- **배경**: 0019(주인 메모리)·0024(이미지 엔진)·0025(이웃 일기)는 전부 소재가 **LLM 엔진으로**
  나가는 경계였다. 이 결정은 처음으로 **다른 사람이 읽는 서버**로 나간다.
- **결정**: 공개범위 게이트를 두지 않는다. 게시 값은 주인 화면의 표시 문장·그림 그대로.
  무인증 `life_state` 노출을 수용한다. 이미지 GET은 기존 마스코트 이미지와 같은 Bearer 수위.
- **수용한 결과**: 대문사진은 생성 자체가 옵트인(`daily_cut_enabled` 기본 off)이지만
  **오늘의 한마디는 생성 토글이 없다** — 엔진만 설정돼 있으면 사전 동의 없이 공개된다.
  이것이 자동 생성물이 옵트인 없이 게시되는 첫 사례임을 명시한다.
- **대안**: `content_visibility`에 새 feature 키 추가(1개 또는 2개) / 기존 `diary` 키 편승.
  둘 다 기각 — 기각 사유는 ADR 본문에 기록한다.

## 6. 미결·후속 (백로그)

- `life-visit.md §4`에 누락된 기존 소셜 엔드포인트(`bubble`·`mascot-image`·`guestbook`·
  `content-visibility`·`diaries`·`friends`) 채우기 — 별도 문서 소품.
- `a-hub/life/DEPLOY.md` 10행의 낡은 주소(`158.179.194.42:8001`) 갱신 — 별도 소품.
- 로드맵 8차 배치 Q1·Q2·Q3 — 이 작업과 의존 없음.
