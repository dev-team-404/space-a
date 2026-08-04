# 지식 재사용 루프 닫기 — 설계

- **날짜**: 2026-07-25
- **상태**: 승인 (구현 대기)
- **컴포넌트**: a-mate (`crates/core/src/hub.rs`), a-hub `work/` (`core`, `adapters`, `api`)
- **결정 요지**: README의 핵심 서사(“A팀이 기록 → B팀이 검색·인용해 재사용”)가 **코드상 완결되지
  않은 상태**다. 세 지점(공유 화이트리스트 사멸 · 검색/인용 미호출 · ReuseEvent 조회 불가)을
  고쳐 루프를 닫고, **같은 사멸이 재발하지 않도록 불변식을 테스트로 고정**한다.
- **관련**: [README](../../../../README.md) Pillar 1·2, [hub 지식 공유 스펙](../../a-mate/specs/2026-07-18-hub-knowledge-sharing-design.md),
  [C1 계약](../../../../../contracts/c1-mcp-tools.json)

## 1. 배경 — 관측된 문제

2026-07-25 기준 `main`(`bb0c4a4`)을 코드로 확인한 결과, 루프가 세 곳에서 끊겨 있다.

### 1.1 a-mate는 어떤 Finding도 발행할 수 없다 (구조적)

| 사실 | 위치 |
|---|---|
| 활성 룰은 **R6·R7·R8** 3개뿐. R1·R2·R5·R9·R10·R11·R12·R24는 은퇴 + 매 스캔 purge | `crates/core/src/ops.rs:163-185` |
| 공유 화이트리스트는 `["R1","R2","R10","R11","R12"]` | `crates/core/src/hub.rs:15` |
| 교집합 = **∅** | — |

`select_shareable`이 화이트리스트로 필터하므로(`hub.rs:85`), **사용량과 무관하게 영구적으로 0건**이
발행된다. 코칭 룰 정리(v3·가치 재설계)가 화이트리스트를 함께 갱신하지 않아 생긴 사멸이며,
어느 테스트도 이 불일치를 잡지 못했다.

> 문서는 정반대로 안내한다 — `level-2-a-mate-data-flows.md:53` “발견이 0이면 침묵이 정상이다…
> 세션·인벤토리가 쌓이면 생긴다.” 지금은 **구조상 거짓**이다.

### 1.2 검색·인용을 아무도 호출하지 않는다

a-mate 전체에서 `POST /pages/search`, `POST /issues/{id}/cite` 호출이 **0건**이다.
pull 경로인 `HubKnowledgeSource`(`content.rs:546`)는 `/spaces/{id}/tree`로 **최신 5건을 그냥
가져올 뿐**, 사용자의 현재 문제와 대조하지 않는다. 따라서 “B팀이 검색해서 인용”이 일어나지 않는다.

### 1.3 ReuseEvent는 쓰기 전용 싱크

`a-hub/work/ahub/core/models.py`가 “이 시스템의 북극성 지표”라 부르는 `ReuseEvent`는
`ports.py`에 `add_reuse_event`(쓰기)만 있고 **조회 메서드가 없다**. 어댑터 3종 모두 쓰기만 구현하며
REST에도 조회 엔드포인트가 없다. 그래서 a-lens는 `reuse: 0`을 하드코딩한다
(`a-lens/backend/alens/collector.py:456,505`).

## 2. 목표 / 비목표

**목표**

1. a-mate가 **실제로 발행**한다 (안전하게 렌더 가능한 살아있는 룰로).
2. a-mate가 발행 전 **허브를 검색**하고, 이미 있으면 **인용(ReuseEvent 생성)** 한다.
3. 생성된 ReuseEvent를 **조회**할 수 있다 (a-lens가 재사용 지표를 표시할 수 있게).
4. 화이트리스트 사멸이 **재발하지 않도록** 불변식을 테스트로 고정한다.

**비목표**

- `contracts/`와의 필드명 정합(별건, 범위가 큼).
- 검색 랭킹 개선(허브의 substring 매칭을 그대로 쓴다).
- R1·R2·R10·R11·R12 룰 부활 — 은퇴 결정은 존중한다. 렌더러는 부활 대비로 보존만 한다.

## 3. 결정 ① — 공유 대상은 **R8만**, 그리고 불변식을 테스트로 고정

살아있는 3개 룰을 “팀 일반화 가능성 × 개인정보 안전성”으로 판정한다.

| 룰 | 증거에 개인 텍스트? | 팀 일반화? | 판정 |
|---|---|---|---|
| **R8** MCP 대형 결과 | 없음 (`server`, 건수, 문자수) | 같은 MCP를 쓰는 팀에 그대로 적용 | ✅ **공유** |
| R6 반복 지시 | **있음** — `repeated_prompt`, `member_norms`가 프롬프트 원문 | 원문을 빼면 “누가 N번 반복했다”만 남아 무의미 | ❌ 제외 유지 |
| R7 상위모델 잔심부름 | 없음 | 개인의 **세션 단위** 모델 선택 — 팀 지식 아님 | ❌ 제외 |

→ `SHARE_RULES = ["R8"]`.

R6 제외는 기존 코드의 판단(`r6_repeated_prompts.rs:5`)을 유지하는 것이다. 프롬프트 원문은
로컬을 떠나지 않는다는 원칙(`01-product.md` §4.5)이 우선한다.

**재발 방지가 이 결정의 핵심이다.** 다음 불변식을 테스트로 고정한다:

> `SHARE_RULES`의 모든 원소는 **`run_rules`가 실제로 등록한 룰**이어야 하고,
> 각 원소는 `render_share`가 `Some`을 반환해야 한다.

룰을 은퇴시키면서 화이트리스트를 안 고치면 **테스트가 깨진다**. 이번 사멸의 근본 원인을 없앤다.

## 4. 결정 ② — 발행 전에 검색하고, 있으면 인용한다

`run_share`의 신규 발행 경로를 다음으로 바꾼다.

```
finding 선별
  └→ search_knowledge(space, query)         ← 신규
       ├─ 동일 지식 있음 → open_issue → cite_knowledge(page_id)   ← 신규 (ReuseEvent 생성)
       └─ 없음          → open_issue → resolve(publish_knowledge) ← 기존
```

- **마커(marker)**: 같은 지식인지 기계가 판별하는 안정 키 — `[a-mate:R8:{server}]`.
  **`summary` 안에 심는다.** 허브가 `resolve_issue`에서 발행 Page의 제목을 **이슈 제목이 아니라
  `summary`로** 만들기 때문이다(`services.py` resolve_issue: `title=summary`).
  이슈 제목으로 매칭하면 영원히 못 찾는다 — **E2E에서 실제로 중복 발행이 나와 발견한 사실**이다.
- **질의(query)**: 마커 그대로. LLM 무개입(정밀도의 선).
- **검색 범위**: **org 전체(`space_id` 미지정)**. 허브의 `search_knowledge`는 `space_id`를 주면
  그 공간만 보므로, 자기 공간으로 좁히면 README의 핵심인 **교차 팀 재사용이 구조적으로 불가능**해진다.
- **동일 판정**: 검색 결과 중 **제목에 마커가 포함**된 페이지. 느슨한 유사도 매칭은 쓰지 않는다(오인용 방지).
- **자기 글 제외**: 응답의 `created_by`가 자신이면 인용하지 않는다 — 자기 글 인용은 재사용이 아니다.
- **마커 없는 룰**: `marker`가 비면 검색·인용을 건너뛰고 바로 발행한다(오인용 방지 우선).
- **실패 무해**: 검색이 실패하면 **기존 발행 경로로 폴백**한다. 루프가 허브 장애로 멈추지 않는다.

이로써 “개인의 시행착오가 조직의 자산이 된다”가 실제 이벤트로 남는다. 인용 시 허브가
`cross_team`을 계산하므로(`services.py`), 팀 간 재사용도 자동 판별된다.

**중복 발행 방지**: 인용한 경우에도 `hub_mark_published`로 기록해 다음 스캔에서 다시 시도하지
않는다(평생 1회 원칙 유지). page_id는 **인용한 남의 페이지 id**를 쓴다.

## 5. 결정 ③ — ReuseEvent 조회 경로 추가

최소 표면만 연다.

| 층 | 추가 |
|---|---|
| `core/ports.py` | `list_reuse_events(space_id: str \| None, limit: int) -> list[ReuseEvent]` |
| `adapters/store_memory.py` | 리스트 역순 슬라이스 |
| `adapters/store_sqlite.py` | `SELECT … ORDER BY created_at DESC LIMIT ?` |
| `adapters/store_dynamodb.py` | 기존 스캔 패턴 재사용 |
| `core/services.py` | `list_reuse_events(token, space_id, limit)` — 권한은 기존 규칙 준용 |
| `api/rest_server.py` | `GET /reuse-events?space_id=&limit=` |

응답 형태는 a-lens가 바로 쓰도록 `{reuse_events: [{reuse_id, issue_id, page_id, space_id,
cited_by, cross_team, created_at}]}`로 한다.

## 6. 테스트 계획

**a-mate (`cargo test`)**
1. `share_rules_are_all_active_and_renderable` — §3 불변식. 화이트리스트 ⊆ 등록 룰, 각각 렌더 Some.
2. `render_share_r8_has_no_personal_text` — 렌더 결과에 프롬프트·경로·session_id가 없음.
3. `select_shareable_picks_r8` — est=0 룰이 occurrences 문턱으로 선별됨.
4. 검색 결과 매칭 순수 함수 `pick_citable`의 단위 테스트 — 제목 일치/불일치/자기 글 제외.

**a-hub (`pytest`)**
5. 인용 후 `list_reuse_events`가 그 이벤트를 돌려준다 (memory·sqlite 양쪽).
6. `GET /reuse-events`가 인증을 요구하고 목록을 반환한다.

> 🍎 **macOS에서 REST 테스트가 수집 단계에서 죽는 경우.** `create_app()` 기본값이
> `mount_mcp=True`라 `mcp` 패키지를 import하는데, x86_64 Python(Rosetta) 환경에서는
> `cryptography` 휠 빌드가 실패해 `mcp` 설치가 막힌다(`pip` 의존성 해석도 `ResolutionTooDeep`).
> 테스트 파일이나 프로덕션 기본값을 바꾸지 말고, 아래 플러그인으로 **기본값만** 우회한다:
>
> ```python
> # /tmp/mcpless_plugin.py
> import ahub.api.rest_server as rs
> _orig = rs.create_app
> rs.create_app = lambda service=None, *, mount_mcp=True: _orig(service, mount_mcp=False)
> ```
> ```sh
> PYTHONPATH=/tmp .venv/bin/python -m pytest tests/ -p mcpless_plugin \
>   --ignore=tests/test_mcp.py --ignore=tests/test_mcp_http.py
> ```
> 결과(2026-07-26): **262 passed, 4 skipped**. `tests/test_mcp*.py`는 실제 `mcp` 패키지가
> 필요하므로 이 환경에서는 검증 불가 — 그 부분은 Windows/Linux CI에 맡긴다.

**E2E (실제 서버)**
7. 로컬 허브 기동 → 에이전트 A가 발행 → 에이전트 B(다른 user_id)가 같은 finding으로 검색·인용
   → `GET /reuse-events`에 `cross_team` 이벤트가 보인다. **이것이 README 시나리오의 실증이다.**

### 6.1 E2E 실행 결과 (2026-07-25, 로컬 허브 `mount_mcp=False`)

| 단계 | 결과 |
|---|---|
| ① A팀(ds2) `alice` 발행 | `published: R8\|github → page_3` |
| ② B팀(sw2) `bob` 검색·인용 | `cited(재사용): R8\|github → 기존 page page_3 (reuse reuse_1)` — **중복 발행 안 함** |
| ③ `GET /reuse-events` | 1건: `reuse_1`, page `page_3`, 인용자 `bob`, **`cross_team=true`**, 시각 기록됨 |
| ④ B 재실행 | `0건 발행, 0건 인용, 1건 보류` — 평생 1회 원칙 유지, 이벤트 총 1건 |

첫 시도에서는 ②가 인용 대신 **중복 발행**됐다. 원인은 위 §4의 마커 결정으로 기록한
`title=summary` 동작이었고, 마커를 본문에 심어 해결했다. 문서만 보고 설계했으면 놓쳤을 지점이다.

## 7. 리스크

| 리스크 | 완화 |
|---|---|
| R8이 잘 안 뜨는 사용자는 여전히 공유가 드물다 | 사실이다. 다만 **0건 보장**에서 **조건부 발생**으로 바뀌는 것이 핵심이고, 룰이 늘면 §3 테스트가 화이트리스트 갱신을 강제한다 |
| 허브 검색이 substring이라 오탐 | 제목 **완전 일치**만 인용하므로 오인용은 발생하지 않는다 |
| 인용 실패 시 지식 유실 | 인용 실패는 폴백 없이 다음 스캔 재시도(발행 상태 미기록) |

---

## 8. 배포 현황 — 인정 루프는 당분간 동작하지 않는다 (2026-08-03)

**코드는 양쪽 다 머지돼 있다.** a-mate의 폴링(`pipeline.rs`의 `maybe_poll_reuse`)과 a-hub의 §5 조회 API가 전부 main에 있다 — `origin/main:a-hub/work/ahub/api/rest_server.py`의 `@app.get("/reuse-events")`(PR #110). 그런데 실환경에서는 앱이 **404**를 받는다.

### 8.1 서버가 둘이고 `/reuse-events`는 work 허브 몫이다

`a-mate/crates/core/src/hub.rs`의 `HubConfig`가 설정 키를 의도적으로 분리해 뒀다:

| 설정 키 | 서버 | 담당 |
|---|---|---|
| `hub_url` | **life** | 방·방명록·방문 폴링 |
| `knowledge_hub_url` | **work 허브** | 팀 지식·발행·인용·**`/reuse-events`** |

`HubClient.base_url`은 `knowledge_hub_url`에서 온다. 따라서 **life 서버를 재배포해도 이 경로는 고쳐지지 않는다** — 실제로 2026-08-03에 life 서버를 재배포한 뒤에도 404가 그대로였고, 같은 시점에 방문 알림·방명록은 정상 동작했다.

### 8.2 work 허브는 관리자 부재로 재배포 불가

그 서버를 운영하던 담당자가 부재해 한동안 배포가 불가능하다. **`/reuse-events`와 `source_space`는 업데이트되지 않는 것으로 전제한다.** 이슈 [#136](https://github.com/dev-team-404/space-a/issues/136)이 그 요청이지만 우리 쪽에서 진행할 수 있는 것이 없다.

### 8.3 코드는 이미 fail-safe다 — 추가 방어가 필요 없다

- `hub.rs`의 `reuse_events()`가 **404만** `Ok(None)`(=기능 없음)으로 구분하고, 그 밖의 오류는 `Err`로 올려 다음 스캔에 재시도한다. 즉 구버전 허브와 일시 장애가 섞이지 않는다.
- `pipeline.rs`의 `static UNSUPPORTED: AtomicBool`이 404를 한 번 받으면 이후 호출을 건너뛴다. 매 스캔 네트워크를 타지도, 로그를 반복해 남기지도 않는다.

**⚠ 그 플래그는 프로세스 수명 동안 유지된다.** 나중에 work 허브를 재배포하면 **앱을 재시작**해야 인정 루프가 다시 붙는다. 방문 폴링의 `VISITS_UNSUPPORTED`도 같은 패턴이므로 함께 기억할 것.

### 8.4 침묵하는 범위는 좁다

인정 루프(재사용 축하 말풍선·소식)만 동작하지 않는다. **팀 지식 조회와 발행은 구버전 허브에도 있는 경로라 계속 동작한다** — 2026-08-03 스모크에서 코칭 탭에 팀 지식 카드(`hub-page_229`, 「학습」+「팀」 칩)가 정상 표시된 것으로 확인했다.

### 8.5 에이전트가 서버 상태를 직접 확인할 수 없다

개발 PC는 사외망이라 `spacea.msalt.net`에 대한 `curl`이 **전부 `HTTP 000`**(연결 자체 불가)이다. 서버가 구버전인지 판단하는 근거는 **앱 로그**다 — `인정 루프: 허브에 /reuse-events가 없어 건너뜁니다(구버전 배포)`가 찍히면 `Ok(None)` 경로이므로 연결은 됐고 그 경로만 없다는 뜻이다. 연결 실패라면 `조회 실패(다음 스캔 재시도)` 쪽이 찍힌다.
