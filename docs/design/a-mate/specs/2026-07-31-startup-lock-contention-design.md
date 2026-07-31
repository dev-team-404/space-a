# X1 — 앱 첫 로딩 지연(store 락 경합) 설계

- **날짜**: 2026-07-31
- **컴포넌트**: a-mate (`src-tauri/`, `crates/core/`)
- **관계**: [로드맵](../plans/2026-07-26-life-social-diary-followups-roadmap.md) "성능·아키텍처" 절의 X1. 로드맵의 **마지막 남은 항목**.
- **줄 번호 규약**: 이 문서의 모든 코드 좌표는 **main `1b6cc7e` 기준**이다(계측 코드를 넣은 작업 브랜치 기준이 아니다).
- **목적**: 앱 시작 시 초기 스캔이 store 락을 통짜로 쥐어 UI가 정지하는 문제를 없앤다.
  로드맵이 "측정 선행"으로 남겨 둔 (a) 락 경합 / (b) hub HTTP 판정을 실측으로 끝내고, 그 숫자로 수정 범위를 정했다.

---

## 1. 측정 (착수 전, 2026-07-31 실측)

### 1.1 방법

로드맵이 오래 달고 있던 "사외망 개발 PC에서는 hub 도달 불가"는 2026-07-31에 정정됐다 — hub는 공개 도메인
(`https://spacea.msalt.net`)이고 a-mate는 이 PC에서 정상 연결된다. 막히는 것은 `curl`뿐이다(TLS 폐기 검사,
HTTP 000 / exit 35). 따라서 **앱을 띄워서** 계측했다.

임시 `log::info!` 계측 5곳:

| # | 지점 | 얻는 값 |
|---|---|---|
| 1 | `pipeline.rs run_pipeline_once` | 락 대기 / ingest·inventory·rules **단계별** / 총 보유 |
| 2 | `ops.rs run_ingest_with_progress` | discover / 파일 읽기 / `rebuild_rollup` 분리 |
| 3 | `commands.rs lock()` | 대기 ≥20ms일 때 `#[track_caller]`로 **호출 커맨드 위치까지** |
| 4 | `commands.rs run_life_http` | 기존 헬퍼에 얹어 **모든 hub 왕복** (2초 경고는 유지) |
| 5 | `lib.rs setup()` | 진입 → 파이프라인 spawn → 락 3곳 → 반환 시간축 |

실행 조건:

- `npm run tauri dev` (포트 1420 — 앱이 떠 있으면 기동 실패하므로 사전 종료 확인)
- 실제 DB(30.7MB)와 자산을 스크래치패드로 **복사**해 `AGENT_MENTOR_DATA_DIR`로 격리 —
  스캔 비용·hub 설정이 동일해 숫자는 같고, 쓰기는 복사본에만 간다.
- 트랜스크립트 파일 220개(Windows + WSL 호스트 합계)

두 시나리오를 쟀다:

| 시나리오 | 뜻 | 재현 방법 |
|---|---|---|
| **웜** | 기존 DB로 증분 수집 — 평소 앱 재시작 | 실제 DB 복사본 그대로 |
| **콜드** | `ingest_state`가 비어 전량 재수집 — 최초 설치 **및 마이그레이션 후 첫 실행**(§1.4) | 빈 데이터 디렉터리 |

### 1.2 결과

| 계측 항목 | 웜 | 콜드 |
|---|---|---|
| **스캔 락 총 보유** | **1,512ms** | **16,707ms** |
| ├ discover | 96ms (220 files) | 97ms (220 files) |
| ├ **ingest 파일 읽기** | 454ms (384 events) | **15,641ms** (54,995 events) |
| ├ rebuild_rollup | 130ms | 131ms |
| ├ inventory | 295ms | 323ms |
| ├ rules | 527ms | 503ms |
| └ 그 외 — `before`·`after` 스냅샷·diff·`last_scan_ts`·emit | ~9ms *(차감 파생값)* | ~10ms *(차감 파생값)* |
| **커맨드 락 대기** | 952–1,099ms · 22건 / **11종** | **16,479–16,630ms · 14건 / 9종** |
| hub 왕복 — **방명록** | **51ms** | (미연결) |
| hub 왕복 — life_view | 78–163ms | (미연결) |
| hub 왕복 — people / list | 42ms / 65–99ms | (미연결) |
| `setup` 반환 | +27ms | +380ms |

락을 기다린 커맨드 — `#[track_caller]` 실측, 두 시나리오 합집합 **11종**:

| 분류 | 커맨드 |
|---|---|
| 순수 로컬 읽기 (7종) | `get_summary` · `list_findings` · `get_week_summary` · `get_model_mix` · `get_today_occasions` · `get_settings` · `theme_get` |
| hub 설정을 읽는 것 (4종) | `hub_settings_get` · `life_view` · `life_list` · `life_people` |

**11종 전부 읽기 전용이다** — 모두 `lock(&state)` 후 `&*guard`(불변 차용)로 read-only inner를 호출한다.
쓰기 커맨드는 시작 시점에 호출되지 않아 측정에 안 잡혔을 뿐, 같은 락을 쓴다(§1.5의 1안 기각 근거).

콜드에서 `life_list`·`life_people`이 안 잡힌 이유: hub 미연결이라 `life_view`가 먼저 실패해
프론트가 그 둘을 호출하지 않았다. hub 연결 상태(웜)에서는 둘 다 잡힌다.

### 1.3 판정 — (a) 락 경합. (b) hub HTTP 무죄

방명록 조회 hub 왕복은 **51ms**다. 10초 타임아웃 근처도 아니고, 콜드 락 대기와 **약 325배** 차이다.
로드맵이 열어 둔 (a)/(b) 판정은 (a)로 끝났다.

### 1.4 콜드는 예외가 아니다 — 마이그레이션이 되풀이 만든다 *(측정이 드러낸 핵심)*

DB 경로는 번들 identifier(`dev.agentmentor.app`)로 정해지므로 어느 워크트리에서 빌드해도 **같은 파일을
재사용**한다. 설정·토큰·`findings`의 dismissed/resolved·`diary_index`는 보존된다.

그런데 `store.rs migrate()`가 **로그 파생 데이터를 의도적으로 버리는 트리거를 9개** 갖고 있다:

| 트리거 | 조건 | 비우는 것 |
|---|---|---|
| `events.model_raw` 컬럼 부재 | `store.rs:168` | events, ingest_state, daily_rollup |
| `events.source_file` 컬럼 부재 | `:178` | + sessions |
| `events.result_len` 컬럼 부재 | `:194` | events, ingest_state, daily_rollup |
| `sessions.subagent_files` 컬럼 부재 | `:205` | + sessions |
| `user_version < 1` | `:222` | 전량 + R6 카드 정리 |
| `user_version < 2` | `:231` | 전량 + prompt_events |
| `user_version < 4` | `:247` | 전량 + prompt_events |
| `user_version < 6` | `:267` | 전량 + prompt_events |
| `user_version < 8` | `:289` | 전량 + prompt_events |

`ingest_state`가 비면 다음 스캔이 **모든 파일을 offset 0부터** 다시 읽는다 = 콜드 16.7초.

이것이 사용자 증언 **"PR 머지 후 실행했을 때 설정탭도 15초 이상"** 과 정확히 맞는다 —
콜드에서 `get_settings`가 **16,630ms** 대기했다. 즉 16초 정지는 최초 설치 한정
예외가 아니라 **마이그레이션을 실은 릴리스마다 되돌아오는 상태**이고, 이 레포는 지금까지 그런 트리거를
9번 실었다. X1의 정당성은 웜(1.1초)이 아니라 이 경로에 있다.

*(웜 측정이 1.5초였던 이유: 실측 당시 실제 DB는 이미 `user_version` 8이고 컬럼이 다 있어 재수집이
걸리지 않았다 — 신규 384건만 읽었다.)*

### 1.5 조사 중 정정된 것

**`setup`은 막히지 않는다 — 범위 밖.** 착수 전 가설은 "`lib.rs:254`가 `pipeline::start`(즉시 락 점유)를
띄운 직후 같은 `setup`이 락을 3번 잡으므로(`:260` hub 설정 · `:297` `mascot_visible` ·
`:356` `content_protected`) 메인 스레드가 막혀 웹뷰 자체가 안 뜬다"였다. **실측으로 틀렸다** —
`setup`은 웜 `+27ms`, 콜드 `+380ms`에 반환한다. 파이프라인 스레드가 `spawn_watchers`를 먼저 돌기 때문에
`setup`의 락 3개가 경합에서 이긴다. 이는 보장이 아니라 **레이스**지만 실측상 문제가 아니므로 이번 범위에서
제외한다(후속 후보로만 기록).

**로드맵 3안(hub 설정 캐시) 기각.** 막힌 11종 중 hub 설정 캐시로 풀리는 것은 **4종**이고, 나머지
**7종은 그대로 16초 막힌다** — 그 7종이 홈 화면과 설정 탭이 렌더에 쓰는 재료 전부다. 증상 서술의
"및 다른 store 의존 화면"을 못 덮는다.

**로드맵 1안(읽기 전용 연결 분리) 단독 기각.** 1안은 **읽기**만 락에서 빼낸다. 측정된 11종이 전부 읽기라
화면은 즉시 뜨지만, `set_setting`·`set_finding_status`·`theme_set`·`profile_set`·`memory_*`·
`hub_connect` 같은 **쓰기 커맨드는 같은 락을 그대로 기다린다**. 그리고 그 잔여 문제가 하필 실제 사용자
시나리오("업데이트 후 첫 실행 → 설정 탭")와 겹친다 — 설정 탭은 즉시 뜨는데 토글을 누르면 최대 16초
멈춘다. 증상이 "화면이 안 뜬다"에서 "화면은 뜨는데 저장이 안 먹는다"로 옮겨갈 뿐이다.
부수적으로 1안은 **WAL 전환이 선행 조건**이다 — 현재 `SqliteStore::open`은 `journal_mode`·`busy_timeout`
PRAGMA를 전혀 설정하지 않아(기본 rollback journal) 두 번째 연결은 쓰기 트랜잭션 동안 `SQLITE_BUSY`로 막힌다.

### 1.6 채택 — 로드맵 2안(초기 스캔 락 청킹)

| 안 | 읽기 대기 | 쓰기 대기 | DB 포맷 | 판정 |
|---|---|---|---|---|
| 1안 reader 분리 | 0 | 16.6초 **그대로** | WAL로 영속 변경 | 단독 부족 |
| **2안 청킹** | **~500ms** | **~500ms** | **무변경** | **채택** |
| 3안 hub 캐시 | 11종 중 4종만 | 그대로 | 무변경 | 기각 |

2안이 읽기·쓰기를 함께 풀고 DB 포맷을 건드리지 않는다.

---

## 2. "스캔"의 정의

이 문서에서 **스캔** = `pipeline.rs run_pipeline_once` 한 번의 실행. 현재 아래 6단계 전체가
`state.store.lock()` **하나의 블록**(`pipeline.rs:71`~`:97`) 안에 있고, 그 mutex가 모든 커맨드가 DB를
만지는 유일한 통로(`commands.rs:266 fn lock()`)다.

| 단계 | 하는 일 |
|---|---|
| 1. discover | 호스트(Windows·WSL) 열거 → `~/.claude/projects`의 트랜스크립트 파일 목록 |
| 2. ingest | 파일마다 `ingest_state.last_offset`부터 끝까지 읽어 파싱 → `events`·`sessions`·`prompt_events` + 새 offset 기록 |
| 3. rebuild_rollup | `events`에서 `daily_rollup`(호스트·프로젝트·날짜별 토큰·세션 수) 재계산 |
| 4. inventory | `~/.claude.json`·`settings.json`·플러그인 캐시 → MCP·플러그인·스킬·호스트 설정 스냅숏 |
| 5. rules | 룰 엔진(R6·R7·R13…)을 DB 전체에 돌려 `findings` upsert·정리 |
| 6. diff·emit | 5번 전후 카드 비교 → 새로 뜬 것만 `coach:finding` emit |

**언제 도는가** — 세 경로:

- **앱 시작 직후 즉시 1회** — `pipeline::start`가 스레드를 띄우고 디바운스 없이 실행(`pipeline.rs:29`). ← X1의 대상
- **파일 감시 + 60초 디바운스** — Claude Code 사용 중 로그가 계속 바뀌어 반복 실행(`debounce_loop(rx, 60s, …)`).
  웜 1.5초 정지가 작업 중에도 되풀이된다.
- **수동** `run_scan_now`

스캔이 락을 놓은 **뒤**에 붙는 긴 작업(일기 생성·코칭 판정·방명록 봇 답글·자율 방문·스프라이트·대문사진 등,
`pipeline.rs:108`~`:139`)은 이미 락 밖으로 옮겨져 있어(`:96` 주석) **X1 범위가 아니다** — 각자 필요할 때만
짧게 락을 잡는다.

---

## 3. 설계

### §A. 목표 구조 — 락을 단계·파일 단위로 쪼갠다

```
[락 밖]   discover_work()                       DB 미사용 (97ms)
파일마다  [락] ingest_one(file) → 해제 → yield_now()      × 220회 (§C)
[락]      rebuild_rollup()                      131ms
[락]      run_inventory()                       323ms
[락]      before = finding_severities()         ┐
          run_rules()                           │ 한 블록 ≈ 510ms
          after  = finding_severities()         │
          diff → last_scan_ts → coach:finding   ┘
```

**최장 연속 락 보유 = `run_rules` 블록 ≈ 510ms.**

#### `before` 스냅샷 이동 — 버그 수정이 아니라 비용 0의 방어 *(2026-07-31 검증으로 근거 정정)*

`before`/`after`/`diff`는 스캔에서 **두 읽기가 하나의 쓰기를 감싸야 의미가 생기는 유일한 지점**이다.
청킹하면 그 사이에 락이 220번 열리므로 "그 틈에 누가 `findings`를 바꾸면 diff가 오판한다"고 우려했으나,
**실제로는 지금 안전하다**:

| 확인한 사실 | 근거 |
|---|---|
| `finding_severities()`는 `SELECT dedup_key, severity FROM findings` — **`status`를 보지 않는다** | `store.rs:1428` |
| 커맨드 중 `findings`를 쓰는 것은 `set_finding_status` **하나뿐**이고 `status`만 바꾼다 | `commands.rs:322` → `store.rs:911` |
| `run_scan_now`는 `PipelineMsg::RunNow`를 mpsc로 보낼 뿐 — 소비자는 파이프라인 스레드 하나라 **스캔은 겹치지 않는다** | `commands.rs:322` 인접, `pipeline.rs:31` |

카드를 처리해도 `(dedup_key, severity)`가 불변이므로 `diff_findings`(`crates/core/src/pipeline.rs`)의
판정은 달라지지 않는다. **즉 청킹은 diff 의미를 바꾸지 않는다.**

그래도 옮기는 이유는 그 안전이 **다른 파일의 우연 두 개에 의존**한다는 것이다 — "스냅샷이 마침 `status`를
안 본다"와 "쓰기 커맨드가 마침 하나뿐이고 마침 `status`만 바꾼다". 둘 중 하나라도 바뀌면(향후 커맨드가
finding을 삽입하거나, 스냅샷이 `status`로 필터하게 되면) 락 틈으로 조용히 오작동이 들어온다 — 잘못 뜬
`coach:finding` 토스트. 한 블록으로 묶으면 그 의존이 사라지고 불변식이 **국소적으로 보인다**.
비용은 문장 하나의 이동이다.

**이 항목은 선택적이다** — 빼도 정합성은 유지된다.

### §B. ops.rs — CLI 경로 불변

`crates/core/src/main.rs`(CLI)가 `ops::run_ingest`·`run_inventory`·`run_rules`를 쓴다. 따라서:

- 신규 `pub fn discover_work() -> (Vec<HostWork>, Vec<String>)` — 호스트별 adapter + 파일 목록, 열거 경고
- 신규 `pub fn ingest_one(store, adapter, path) -> Result<usize>` — 기존 private `ingest_file`의 공개 래퍼
- **기존 `run_ingest_with_progress`를 이 둘 위에 재구현** — 중복 구현 없이 `run_ingest` 시그니처·동작 불변

### §C. 배치 = 파일 1개

콜드 실측 파일당 평균 71ms(15,641/220). 락 획득 220회는 ns 단위라 오버헤드는 무시된다.
`upsert_events`가 이미 파일마다 트랜잭션을 열므로(`store.rs:325`) 파일 단위 경계가 자연스럽다.

**파일 내부는 쪼개지 않는다** — `ingest_file`의 offset 재개 구조를 건드리는 비용이 이득보다 크다.
대신 **파일당 최대 소요를 로그로 남겨** 거대 파일 하나가 최장 보유를 지배하는지 확인한다.

### §D. barging — 유일한 실질 리스크

`std::sync::Mutex`는 공정성을 보장하지 않는다. 해제 직후 스캔 스레드가 재획득하면 대기 커맨드가
끼어들지 못해 **청킹이 무효**가 된다.

- **1차**: 배치마다 `std::thread::yield_now()`
- **`sleep(1ms)`는 쓰지 않는다** — Windows 기본 타이머 해상도가 ~15ms라 220 × 15ms ≈ **3.3초**가 스캔에 붙는다
- **검증 게이트**: 재측정에서 커맨드 대기가 500ms대로 내려오지 않으면 handoff 실패로 판정하고 폴백 —
  `AppState`에 대기자 카운터(`AtomicUsize`)를 두고, 커맨드가 락 요청 전 증가·후 감소시키며
  스캔은 카운터가 0이 될 때까지 재획득을 미룬다

### §E. 받아들이는 변화

- 스캔 중 커맨드가 **부분 수집 상태**를 읽을 수 있다. 스캔 시작 전에 읽는 것과 같은 상태이고
  `scan:done`이 프론트 갱신을 촉발하므로 수용한다.
- `ingest_state`는 파일마다 커밋되므로 중간에 죽어도 다음 스캔이 이어받는다 — 기존보다 개선.
- **diff 의미는 청킹 자체로 이미 보존된다**(§A의 검증 표). `before` 이동은 그 보존을 미래 변경에도
  붙잡아 두는 방어일 뿐이다.
- **스캔은 서로 겹치지 않는다** — `run_scan_now`·파일 감시·시작 스캔이 모두 같은 mpsc 채널과
  파이프라인 스레드 하나를 지나므로, 청킹이 스캔 대 스캔 경합을 새로 만들지 않는다.

### §F. 명시적 비목표

**스캔 총 소요는 줄지 않는다.** 청킹은 총량을 재분배할 뿐이다 — 마이그레이션 후 첫 실행에서 §2의 1~6은
**여전히 약 16.7초** 걸린다. 바뀌는 것은 "그 동안 설정 탭·홈·방명록이 기다리지 않는다"이다.

수집 자체를 빠르게 하는 것(2단계 파일당 트랜잭션 fsync 비용 등)은 성격이 다른 과제로 분리한다.
`setup` 락 레이스(§1.5)도 범위 밖이다.

### §G. 계측 코드 처리

| 계측 | 처리 |
|---|---|
| 스캔 단계 요약 1줄 | **영구 유지** (스캔당 1회, `info`) — 성능 회귀를 볼 유일한 창. `X1` 접두 제거·문구 정리 |
| **스캔 벽시계 + 최장 연속 락 보유** | **신설·영구 유지** — §6의 비교 지표. 청킹 후에는 "총 보유"가 의미를 잃으므로 이 둘로 대체한다 |
| `ops.rs` ingest 내부 분리 | **영구 유지** (스캔당 1회) — 위와 같은 이유 |
| `run_life_http` 무조건 로그 | **되돌린다** — 기존 2초 경고만 |
| `lock()`의 `#[track_caller]` 커맨드별 로그 | **검증 후 제거** — 스캔당 수십 줄로 시끄럽다. 재측정에는 필요 |
| `setup` 시간축 로그 | **제거** — 범위 밖으로 판정됐다(§1.5) |

---

## 4. 터치 파일

| 파일 | 변경 |
|---|---|
| `crates/core/src/ops.rs` | `discover_work`·`ingest_one` 신설, `run_ingest_with_progress` 재구현, 등가성 테스트 |
| `src-tauri/src/pipeline.rs` | `run_pipeline_once`의 락 블록을 §A 구조로 분해 |
| `src-tauri/src/commands.rs` | 계측 정리(§G) |
| `src-tauri/src/lib.rs` | 계측 제거(§G). *폴백 발동 시에만* `AppState` 대기자 카운터 |

로드맵이 X1의 터치로 예상한 것 중 **`crates/core/src/store.rs`는 전혀 건드리지 않는다** — 연결 구조 개편은
기각된 1안의 면적이다. `src-tauri/src/lib.rs`도 **`AppState` 구조는 그대로**이고 계측 로그 제거만 한다
(§D 폴백이 발동할 때만 필드 하나가 늘어난다).

---

## 5. 테스트

`pipeline.rs`의 `runtime` 모듈은 `#[cfg(not(test))]`이라 **청킹 오케스트레이션은 단위 테스트로 덮을 수 없다.**
정직하게 측정으로 검증하고(§6), 대신 리팩터가 수집 결과를 바꾸지 않았음을 ops.rs 테스트로 보장한다:

- `discover_work` + `ingest_one` 반복 == `run_ingest` — 같은 이벤트 수, 같은 `ingest_state` 내용
- 기존 ops.rs 테스트(`run_rules_*` 5건)는 그대로 통과해야 한다

---

## 6. 검증 (전/후)

계측을 붙인 채 §1.1과 **같은 두 시나리오**를 재측정한다.

| 검증 항목 | 전 (실측) | 후 목표 |
|---|---|---|
| 콜드 **최장 연속** 락 보유 | 16,707ms | ~510ms |
| 콜드 커맨드 대기 | 16,479–16,630ms | ≤ ~510ms |
| 웜 커맨드 대기 | 952–1,099ms | ≤ ~530ms |
| **스캔 벽시계**(시작→`scan:done`) | 콜드 16,707ms / 웜 1,512ms | 동일 ±10% (§F) |
| 파일당 최대 ingest 시간 | 미측정 | 기록 — 최장 보유를 지배하면 §C 재검토 |

**"스캔 벽시계"의 전값이 락 보유와 같은 이유**: 변경 전에는 락 밖에서 도는 작업이 없어 두 값이 같다.
변경 후에는 `discover`가 락 밖으로 나가므로 **락 보유 합계 < 벽시계**가 된다 — 그래서 비교 지표를
락 보유가 아니라 벽시계로 고정한다. 계측에 스캔 벽시계 1줄을 추가한다.

**게이트**: 콜드 커맨드 대기가 500ms대로 내려오지 않으면 §D 폴백을 적용하고 재측정한다.

회귀 기준값 (main 1b6cc7e 실측):

| 항목 | 기준값 |
|---|---|
| 프론트 `npm test` | **224 passed** (27 files, svelte-check 0 errors) |
| `cargo test` | **596 + 47** |

> 로드맵·사용자 노트의 `586 + 45`는 낡은 값이다 — 묶음 ⑦이 그 숫자를 잰 뒤
> **PR #144(`0f1958c`, `#[test]` +10)** 와 **PR #143(`481d091`, +2)** 가 main에 들어왔다.
> 전부 green이므로 main은 깨져 있지 않다.

---

## 7. DoD

- 로드맵 X1에 완료 표시 + 구현 결과(전/후 수치 포함) 기록
- 커밋 메시지·구현 결과에 **측정값을 남긴다** — "빨라졌다"는 서술만으론 검증 불가
- `docs-archive`로 이 스펙·계획 아카이브. **X1이 로드맵의 마지막 항목이므로 로드맵 자체의 아카이브도 검토**
