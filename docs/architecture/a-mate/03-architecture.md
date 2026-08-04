# a-mate 아키텍처

> **범위** — Pillar 1 데스크톱 앱 `a-mate`(제품명 Agent Mentor)의 현재 구조.
> 코드베이스 실측 기준: `main` @ `b886034` (2026-08-04).
> 설계 근거·이력은 [`docs/design/a-mate/`](../../design/a-mate/) 아래 `specs/`·`plans/`를 참고한다.
> 이 문서는 **지금 코드가 어떻게 생겼는가**만 다루고, 왜 그렇게 정했는지는 각 spec과 [ADR](../../adr/)에 있다.

---

## 1. 한눈에

```
┌─ src/ ─────────────── Svelte 5 + Vite (표시·상호작용만) ────────────┐
│  App.svelte (chat 창)          Mascot.svelte (mascot 창)            │
│  └ lib/api.ts ── 백엔드와 통하는 유일한 접점 (invoke + event)       │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ #[tauri::command] · app.emit
┌─ src-tauri/ ──────────────── Tauri v2 셸 (배선·스레드·창) ──────────┐
│  lib.rs(부트·플러그인·핸들러)  pipeline.rs(스캔 스레드)             │
│  commands.rs(77개 커맨드)      tray.rs  visit.rs  geometry.rs        │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ 순수 함수 호출
┌─ crates/core/ ────────────── 도메인 로직 (Tauri 무의존) ────────────┐
│  adapter · model · store · rules · ops · coach · diary · chat        │
│  hub · life_client · inbound · visit · mascot · sprite · profile …   │
│  + CLI 바이너리(agent-mentor) — 셸 없이 백엔드만 돌리는 디버그 경로  │
└─────────────────────────────────────────────────────────────────────┘
```

**의존 방향은 단방향**이다: `src/` → `src-tauri/` → `crates/core/`.
core는 상위를 전혀 모른다. 셸은 core의 순수 함수를 커맨드/이벤트로 배선하는 얇은 어댑터일 뿐이고,
무거운 처리(JSONL 파싱·집계·파일 감시·HTTP)는 전부 Rust에서 끝난다.

---

## 2. 저장소 레이아웃

```
a-mate/
├─ Cargo.toml            # [workspace] members = ["crates/core", "src-tauri"]
├─ crates/core/          # 패키지 agent-mentor — lib(agent_mentor) + bin(agent-mentor CLI)
├─ src-tauri/            # 패키지 agent-mentor-app — Tauri v2 셸
│  └─ tauri.conf.json    #   chat 948×820(숨김 시작) · mascot 280×280(투명)
└─ src/                  # Svelte 5 멀티페이지 (chat.html / mascot.html)
```

| 크레이트 | 주요 의존성 | 성격 |
|---|---|---|
| `agent-mentor` (core) | `rusqlite`(bundled) · `ureq`(native-certs) · `chrono` · `serde` · `sha2` · `png` · `scraper` | **Tauri 무의존.** 단위 테스트로 대부분 덮인다 |
| `agent-mentor-app` | `tauri`(tray-icon) · `notify` · autostart/log/opener/updater/process 플러그인 · `dotenvy` | 스레드·창·트레이·커맨드 배선 |

`ureq`에 `native-certs`를 켠 이유는 사내망 TLS 프록시(자체 CA) 때문이다. 내장 루트만 쓰면
프록시가 재서명한 인증서가 `UnknownIssuer`로 거부된다.

### core 모듈 지도

| 묶음 | 모듈 |
|---|---|
| 수집·정규화 | `adapter`(SourceAdapter) · `model`(NormalizedEvent) · `hosts`(Windows/WSL 열거) · `transcript` |
| 저장 | `store`(SQLite 전부, 5.7k줄 — 가장 큰 모듈) |
| 분석 | `rules/`(Rule 구현) · `ops`(오케스트레이션) · `finding` · `coach` · `judge` · `inventory` |
| 생성(LLM) | `diary/`(Engine trait 포함) · `chat` · `memory` · `curation` · `content` · `skill_draft` · `plugin_reco` |
| 팀 연동 | `hub`(a-hub work) · `life_client`(a-hub life) · `inbound`(수신 diff) · `visit` · `announcements` |
| 캐릭터 | `mascot` · `sprite` · `profile` |
| 순수 헬퍼 | `pipeline`(디바운스·diff — Tauri 무관) |

---

## 3. 스캔 사이클 — 앱의 심장

앱은 전용 스레드 하나에서 **스캔 사이클**을 돌린다. 나머지는 전부 이 사이클의 결과를 보여주는 것이다.

```
파일 감시 (notify)                     Windows: RecommendedWatcher
  <home>/.claude/projects/**/*.jsonl   WSL UNC: PollWatcher 폴백
        │
        ▼  60초 디바운스 (FileChanged 병합 / 트레이 "지금 스캔"=RunNow는 즉시)
        │
╔═══════╪══ store 락 구간 — 단계·파일 단위로 잡고 놓는다 ════════════════╗
║ 1) discover_work()          호스트·파일 열거 (DB 미사용 → 락 밖)        ║
║ 2) ingest_file() × N        파일마다 락 획득/해제 + hand_off            ║
║                             5건마다 scan:progress emit                  ║
║ 3) rebuild_rollup()         일별 사전 집계 재구축                       ║
║ 4) run_inventory()          MCP·플러그인·스킬 스냅샷 교체               ║
║ 5) before 스냅샷 → run_rules() → after → diff  ← 반드시 한 블록         ║
║    신규/악화분만 coach:finding emit · last_scan_ts 저장                 ║
╚═══════╪═════════════════════════════════════════════════════════════════╝
        ▼  scan:done emit
        │
        ▼  ── 여기부터 전부 락 밖 (네트워크 I/O) ──
   19개의 후처리 훅이 순서대로 실행된다 (아래 표)
```

### 3.1 락 밖 후처리 훅

각 훅은 **자기 조건이 아니면 조용히 넘어간다.** LLM 엔진 미설정, 허브 미연결, 아직 이르다 등.
그래서 아무것도 설정하지 않아도 앱은 정상 동작한다 — 수집·규칙·UI는 완전 로컬이다.

| # | 훅 | 하는 일 | 발신 이벤트 |
|---|---|---|---|
| 1 | `maybe_curate_content` | 외부 피드 수집 → 큐레이션 | `content:ready` |
| 2 | `maybe_translate_content` | 소식 번역·기한 추출 | `content:ready` |
| 3 | `maybe_notify_announcements` | 새 공지 알림 (**번역 뒤**여야 함) | `announcement:new` |
| 4 | `run_coaching_judgments` | LLM 기반 코칭 판정 | `coach:finding` |
| 5 | `maybe_judge_work_kinds` | 세션 work-kind 분류 캐시 | — |
| 6 | `maybe_generate_diaries` | 최근 누락일 일기 backfill | `diary:ready` |
| 7 | `maybe_generate_daily_line` | 오늘의 한마디 | `daily-line:ready` |
| 8 | `maybe_generate_chatter_pool` | 잡담 풀 (프론트가 pull) | — |
| 9 | `maybe_reply_guestbook` | 방명록 자동 답글 | — |
| 10 | `visit::maybe_auto_visit` | 남의 방 자동 방문 | — |
| 11 | `maybe_poll_inbound` | 방문·방명록 수신 diff | `life:visit` `guestbook:new` |
| 12 | `maybe_poll_reuse` | **인정 루프** — 내 지식 재사용 감지 | `reuse:celebrated` |
| 13 | `maybe_share_findings` | 발견을 팀 허브에 검색→인용/발행 | — |
| 14 | `maybe_push_telemetry` | 사용 지표 전송 | — |
| 15 | `maybe_post_retros` | 회고 게시 | — |
| 16 | `maybe_generate_sprite` | 마스코트 스프라이트 생성 | `sprite:ready` |
| 17 | `maybe_sync_mascot_image` | 마스코트 이미지 허브 동기화 | `occupant-sprite:ready` |
| 18 | `maybe_generate_daily_cut` | 오늘의 컷 이미지 | `daily_cut:ready` |
| 19 | `maybe_probe_docs` | 문서 도달성 점검 | — |

### 3.2 두 가지 불변식

**① 5단계(rules + diff)는 쪼개면 안 된다.**
스냅샷이 활성(`status='new'`) 항목만 보기 때문에, before와 after 사이에 사용자의
`set_finding_status`(해결함/무시) 커맨드가 끼면 그 변화가 diff에 섞여 "새로 떴다"고 잘못 알린다.
한 블록으로 잡아야 이 불변식이 코드에서 보인다.

**② 네트워크는 절대 락 안에서 하지 않는다.**
모든 LLM·허브 호출은 `락 → 필요한 것만 스냅샷 → 해제 → 네트워크 → 락 → persist` 순서를 지킨다.

---

## 4. 확장 심 (trait 3개)

에이전트별·엔진별 로직을 하드코딩하지 않기 위한 경계다. `a-mate/CLAUDE.md`의 제약이 코드로 표현된 곳.

### 4.1 `SourceAdapter` — 다른 AI 에이전트 지원

[`crates/core/src/adapter.rs`](../../../a-mate/crates/core/src/adapter.rs)

```rust
pub trait SourceAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>>;                  // 소스 파일 열거
    fn read_incremental(&self, file, from_offset) -> Result<…>;  // 완결 라인만 증분 읽기
    fn map(&self, line, source_file, source_offset) -> Vec<NormalizedEvent>;
}
```

- 유일 구현은 `ClaudeCodeAdapter`. 파싱은 관대하게 — 미지 스키마는 로깅만 하고 하드 실패하지 않는다.
- 규칙·저장·다이어리는 **`NormalizedEvent`만 본다.** 다른 에이전트 지원은 어댑터 구현체 하나를 추가하는 문제로 환원된다.
- 호스트 열거는 [`hosts.rs`](../../../a-mate/crates/core/src/hosts.rs) — Windows `%USERPROFILE%\.claude` +
  `wsl.exe -l -q`(UTF-16 디코드)로 모든 distro의 `\\wsl.localhost\<distro>\home\*\.claude`를 자동 발견한다.

### 4.2 `Engine` — LLM 교체

[`crates/core/src/diary/engine.rs`](../../../a-mate/crates/core/src/diary/engine.rs)

```rust
pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system, user) -> Result<EngineOutput>;
    fn chat(&self, system, messages) -> Result<EngineOutput>;
    fn chat_with_tools(&self, system, messages, tools) -> Result<ChatTurn>;  // 기본 구현 = 툴 무시
}
```

- 구현체: `OpenAiCompatEngine`(OpenAI 호환 `/chat/completions`) + `MockEngine`(결정적 테스트용).
- `chat_with_tools`의 **기본 구현이 툴을 무시하고 `chat`으로 폴백**한다 — 툴 미지원 엔진에서도 깨지지 않는다.
- 설정은 앱 설정 또는 환경변수(`AGENT_MENTOR_ENGINE_URL` / `_KEY` / `_MODEL`). 미설정이면 LLM 기능만 no-op.
- `EngineOutput.tokens_used`로 자기 토큰 소비를 스스로 계량한다.

### 4.3 `Rule` — 코칭 규칙

[`crates/core/src/rules/`](../../../a-mate/crates/core/src/rules/)

```rust
pub trait Rule { fn id(&self) -> &str; fn evaluate(&self, store: &SqliteStore) -> Vec<Finding>; }
```

**현재 등록된 규칙은 3개다** (`ops::registered_rules`):

| ID | 내용 |
|---|---|
| R6 | 반복 지시 → 스킬/커맨드화 제안 |
| R7 | 사소한 작업에 상위 모델(Opus) 사용 |
| R8 | MCP가 대형 결과를 반복 반환 — **팀 공유 대상 규칙** |

`rules/` 디렉터리에는 R1·R2·R5·R9·R10·R11·R12 파일도 남아 있지만 **등록되어 있지 않다**(은퇴).
코드를 지우지 않은 이유는 `r10_automation_burst`의 `detect_bursts`처럼 다른 규칙이 계속 쓰는 헬퍼가 있어서다.
`rules/session_stats.rs`가 세션 통계의 단일 공급원이다.

---

## 5. 데이터 모델

### 5.1 `NormalizedEvent` — 내부 공용어

모든 이벤트가 같은 봉투를 갖는다: `source_agent` · `host`(`Windows` \| `wsl:<distro>`) · `project_id` ·
`session_id`/`uuid`/`parent_uuid`/`is_sidechain` · `ts` · **`source_file` + `source_offset`(원본 포인터)**.

```
EventKind =
  AssistantTurn { model(family·tier·raw), usage(입력/출력/캐시읽기/캐시생성/…), web_search, web_fetch }
| ToolCall      { kind: ToolKind, raw_name, target }   // mcp__server__tool → McpCall{server,tool}
| ToolResult    { status: Ok | Denied | Error }
| UserPrompt    { 첫 줄 미리보기 }
| SessionMeta   { cwd, git_branch }
| Compaction                                            // 컨텍스트 압축 경계
```

세 가지 설계가 여기 걸려 있다.

- **ToolKind 정규화** — 규칙은 원시 도구명이 아니라 정규 kind로만 말한다 → 에이전트 중립.
- **모델 티어링** — `claude-opus-4-8` → `{family: opus, tier: high}`. 규칙이 모델 id를 하드코딩하지 않는다.
- **본문은 DB 밖** — 대화 원문을 저장하지 않고 포인터로 지연 로드한다(`ops::deref_jsonl_line`).
  원본이 지워졌으면 미리보기로 폴백. Bash 명령의 시크릿은 `<redacted: secret>`로 치환해 저장한다.

### 5.2 SQLite 스키마

DB는 `app_data_dir/agent-mentor.db` 하나. `AGENT_MENTOR_DATA_DIR`로 덮어쓸 수 있다(한 PC에서 두 인스턴스 실행용).

| 묶음 | 테이블 |
|---|---|
| 수집 | `sessions` · `events` · `prompt_events` · `prompt_events_absent` · `ingest_state` |
| 집계 | `daily_rollup` |
| 코칭 | `findings` · `session_work_kinds` |
| 인벤토리 | `mcp_inventory` · `plugin_inventory` · `personal_skill_inventory` |
| 생성물 | `diary_index` · `daily_line` · `chatter_pool` · `content_items` · `content_first_seen` |
| 팀 연동 | `hub_share_state` · `life_visits` |
| 설정·기타 | `settings` · `host_settings` · `memories` |

- **마이그레이션 전략** — `pragma_table_info` 검사 후 `ALTER TABLE ADD COLUMN`.
  수집 파생 테이블은 비우고 **로컬 JSONL에서 전량 재수집**한다. 원본이 항상 로컬에 있다는 전제를 활용한 것.
  `findings`의 사용자 처분 상태와 `diary_index`는 보존된다.
- **멱등성** — `events`는 `dedup_key` `INSERT OR IGNORE`. `findings`는 `dedup_key` 충돌 시
  `occurrences` 증가 + evidence 갱신, **status는 불변**(사용자가 해결함/무시로 처분한 것을 되살리지 않는다).

---

## 6. Tauri 표면

### 6.1 커맨드 (77개, [`commands.rs`](../../../a-mate/src-tauri/src/commands.rs))

| 그룹 | 대표 커맨드 |
|---|---|
| 요약·통계 | `get_summary` `get_week_summary` `get_model_mix` `day_activity` |
| 코칭 | `list_findings` `set_finding_status` `sessions_ctx` `get_session_transcript` |
| 다이어리 | `list_diary_dates` `get_diary` |
| 마스코트·이미지 | `get_mascot_seed` `get_sprite` `get_daily_cut` `generate_daily_cut_now` `mascot_preview` `mascot_commit` `robot_spec_for_seed` |
| 채팅·메모리 | `chat_status` `chat_send` `memory_list/add/update/delete` |
| 콘텐츠 | `list_content` `set_content_status` |
| 설정 | `get_settings` `set_setting` `engine_settings_*` `image_settings_*` `theme_*` `profile_*` |
| 팀 지식 허브 | `knowledge_hub_settings_get/set` `knowledge_hub_share_set` |
| 미니홈피(life) | `hub_settings_get` `hub_connect/disconnect` `life_view` `life_goto` `life_move_cell` `life_people` `life_guestbook` `life_diaries` … (`life_*` 22개) |
| 제어 | `run_scan_now` `open_chat_tab` `notices_ready` `mascot_set_expanded` |

각 커맨드는 `*_inner` 순수 함수로 분리되어 테스트된다. 설정 키·탭 이름·채팅 role은 화이트리스트로 검증한다.

### 6.2 이벤트 (백엔드 → 프론트)

```
scan:progress   scan:done        coach:finding      content:ready
diary:ready     daily-line:ready daily_cut:ready    sprite:ready
announcement:new  life:visit     guestbook:new      reuse:celebrated
occupant-sprite:ready  settings:changed  theme:changed
chat:goto-tab   chat:shown       update:check
```

### 6.3 창과 트레이

| 표면 | 구성 |
|---|---|
| tray | 좌클릭 → chat 토글. 메뉴에 마스코트 표시 / 마스코트 위치 초기화 / 실시간 조언 / 화면 캡처 보호 / 잡담 빈도 / 지금 스캔 / 설정 / 업데이트 확인 / 시작 시 실행 / 종료. **설정의 source of truth는 store** — CheckMenuItem 자동 토글을 덮어쓴다 |
| chat | 948×820, 숨김 시작. X는 destroy가 아니라 hide (상주 앱) |
| mascot | 280×280 투명·무장식·alwaysOnTop·skipTaskbar. 말풍선 시 일시 확장. 위치는 `geometry::sanitize_pos`로 모니터 구성 변경을 방어한 뒤 복원 |

`content_protected` 설정을 켜면 두 창 모두 화면 캡처에서 제외된다(사내 코드 노출 대비, 기본 off).

---

## 7. 외부 연동

a-mate가 밖으로 나가는 경로는 4개뿐이고, 전부 **꺼져 있어도 앱이 동작한다.**

| 대상 | 모듈 | 내용 |
|---|---|---|
| **a-hub / work** (팀 지식) | [`hub.rs`](../../../a-mate/crates/core/src/hub.rs) | 발견 공유·검색·인용·재사용 조회 |
| **a-hub / life** (미니홈피) | [`life_client.rs`](../../../a-mate/crates/core/src/life_client.rs) | 방·방문·방명록·프로필·이미지 동기화 |
| **LLM 엔진** | `diary/engine.rs` | 일기·한마디·잡담·채팅·판정 |
| **이미지 모델** | `sprite.rs` | 마스코트 스프라이트·오늘의 컷 생성 |

### 7.1 지식 공유 조건 (`hub.rs` 상수)

```rust
SHARE_RULES              = ["R8"]                      // 공유 대상 규칙
DEFAULT_MIN_TOKENS       = 1000                        // 절감 추정 하한
MIN_OCCURRENCES_WHEN_NO_EST = 3                        // 추정 없을 때 반복 하한
MAX_PER_SCAN             = 3                           // 스캔당 발행 상한
DEFAULT_HUB_URL          = "https://spacea.msalt.net"
DEFAULT_SPACE_ID         = "sw-innov"
```

선별 조건은 `화이트리스트 ∩ status=new ∩ 문턱 통과 ∩ 미공유`, 상한 `MAX_PER_SCAN`.

### 7.2 발행 경로 — 무조건 올리지 않는다

`maybe_share_findings`는 **검색을 먼저 한다.**

```
발견 1건
  └─ POST /pages/search  (마커로 팀에 같은 지식이 이미 있는지)
       ├─ 있음 → POST /issues → POST /issues/{id}/cite   ← 중복 발행 대신 인용
       └─ 없음 → POST /issues → POST /issues/{id}/resolve ← 새 지식 발행
```

**마커는 이슈 제목이 아니라 `summary`에 심는다.** 허브의 `resolve_issue`가 발행 페이지 제목을
`title=summary`로 만들기 때문에, 이슈 제목에 넣으면 나중에 되찾을 수 없다.

### 7.3 인정 루프 (`inbound.rs` + `maybe_poll_reuse`)

```
GET /reuse-events?limit=200
  → page_id가 내 발행분에 속하는 행만
  → cited_by ≠ 나 (자기 인용 제외)
  → created_at > cursor
  → reuse:celebrated emit
```

커서 규약이 dedup을 대신한다.

- 첫 실행(cursor=None)이면 **emit 없이 커서만 초기화** — 설치 직후 과거분 도배 방지.
- 커서 후보는 **내 대상 행만**으로 계산한다. 남의 인용까지 커서를 밀면 내 것이 유실된다.
- 커서는 emit이 성공했을 때만 전진 → **한 이벤트는 평생 1회만** 축하된다.
- 허브가 `/reuse-events`를 404로 응답하면 프로세스 수명 동안 `UNSUPPORTED` 플래그로 스킵한다
  (허브 배포 후에는 앱 재시작이 필요하다).

`select_new_visits` / `select_new_guestbook` / `select_new_reuses` 세 함수가 같은 커서 규약을 공유한다.

---

## 8. 프론트엔드

```
src/
├─ App.svelte      # chat 창 루트 — 미니홈피 셸(사이드바 + 세로 탭), 이벤트 구독
├─ Mascot.svelte   # mascot 창 루트 — 렌더 루프·말풍선·잡담 타이머·클릭/드래그 판별
└─ lib/
   ├─ api.ts       # 모든 invoke 래퍼 + 타입 + 이벤트 리스너 (백엔드와의 유일한 접점)
   ├─ theme.css    # 색·radius·그림자의 단일 출처 — 모드(light/dark) × 스킨(sky/mint/peach/lavender)
   ├─ ui/          # HomeTab · CoachTab · DiaryTab · ChatTab · GuestbookTab · SettingsTab
   │  ├─ coach-stream.ts   # 스트림 조립·정렬·공지 수명·배지 키 집합 (순수 함수)
   │  ├─ coach-helpers.ts  # 카드 뷰모델 — 문법 A/B·근거 칩·고정 슬롯·처분 줄
   │  ├─ home/     #   WeekTrend · ModelMix · SaveTop3(「지금 볼 코칭」) · NoticeLog
   │  ├─ coach/    #   LearnCard(문법 B) · LogCard(문법 A)
   │  ├─ settings/ #   MeGroup · ConnectionGroup · LookGroup · PrivacyGroup · AppInfo
   │  └─ LifeView · MiniLife · RobotPortrait · SessionModal …
   ├─ robot/       # 절차 생성 로봇 (parts / render / anim / bubble)
   └─ interior/    # 방 인테리어
```

탭은 6개: `home` `coach` `diary` `chat` `guestbook` `settings`.

- **색은 반드시 `theme.css`에서만.** 컴포넌트는 시맨틱 토큰만 참조하고,
  이를 `no-hardcoded-colors.test.ts`가 강제한다.
- 다이어리 본문은 `marked` + `DOMPurify.sanitize`로 렌더한다 — LLM 산출물 살균.
- 채팅 이력은 창 수명 메모리(`$state`)에만 있고 영속화하지 않는다.
- **코칭 탭의 판단은 전부 `coach-stream.ts`의 순수 함수에 있다** — 정렬(`first_seen` 내림차순),
  공지 수명(일반 7일·긴급 2일), 배지가 셀 활성 키 집합. 컴포넌트는 렌더만 한다.
  탭 배지와 화면이 **같은 목록**을 보게 하는 것이 이 분리의 목적이다.
- 표시 층 상태(본 카드 키·고정 슬롯 확인·알림 히스토리)는 백엔드 스키마를 건드리지 않으려고
  localStorage에 둔다. 알림 히스토리는 최근 20건.
- 방은 허브 연결 시 격자 방(`LifeView`), 미연결 시 장식 방(`MiniLife`)으로 갈린다.

---

## 9. 동시성 규율

store는 `Mutex<SqliteStore>` 하나를 스캔 스레드와 커맨드 워커가 공유한다. 여기서 나온 두 장치가 있다.

**`hand_off()`** — 락을 놓은 뒤 대기 중인 커맨드가 실제로 잡을 틈을 만든다.
`yield_now()`만으로는 부족했다. Windows `SwitchToThread`는 같은 프로세서의 ready 스레드에만
양보하므로 멀티코어에서는 스캔 스레드가 그대로 재획득한다. 실측에서 홈 탭 커맨드가
inventory+rules를 통째로 기다려 898ms가 나왔다. 상한(2,000회)을 둬서 커맨드가 끊이지 않을 때
스캔이 굶지 않게 한다. `sleep`은 쓰지 않는다 — Windows 타이머 해상도가 ~15ms라 파일당 sleep은
스캔에 수 초를 붙인다.

**스캔당 1줄 telemetry** — `wall / ingest(파일수, 이벤트수, 최장 파일) / rollup / inventory / rules / longest lock hold`.
핵심 지표는 합계가 아니라 **longest**다. 커맨드가 기다리는 시간이 그것이기 때문이다.

---

## 10. 테스트

| 층 | 방식 | 현재 |
|---|---|---|
| core | 거의 전 모듈에 동봉된 단위 테스트. store는 `open_in_memory()`, 엔진은 `MockEngine`으로 결정적 | **668 passed** |
| src-tauri | 디바운스·diff·geometry를 Tauri 무관 순수 함수로 분리, 커맨드는 `*_inner`로 테스트 | **48 passed** |
| 프론트 | vitest — 30파일 | **339 passed** |
| E2E | 수동 체크리스트 (Tauri WebDriver 불안정) | — |

핵심 패턴은 하나다: **부수효과(네트워크·파일·창)를 가장자리로 밀고, 판단 로직은 순수 함수로.**
`diff_findings` · `select_new_reuses` · `sanitize_pos` · `frameAt`이 전부 이 패턴이다.

---

## 11. 실행

```powershell
# 데스크톱 앱 (개발, 핫리로드)
npm run tauri dev

# 릴리스 빌드
npm run tauri build

# 백엔드만 — 셸 없이 CLI로 (./agent-mentor.db 사용)
cargo run -p agent-mentor -- all
#   ingest | inventory | rules | curate | diary [date] | skill-draft
#   hub-share | reuse | telemetry | retro | coach-chat | occ-sprite | all

# 테스트
cargo test --workspace ; npm test
```

**플랫폼은 Windows 전용**이다. 빌드·실행은 네이티브 PowerShell/cmd에서 하고,
WSL 안에서 빌드하지 않는다(WSL은 분석 대상일 뿐).
셋업·트러블슈팅은 [`build-and-run.md`](build-and-run.md) 참고.

---

## 관련 문서

| 문서 | 내용 |
|---|---|
| [`README.md`](README.md) | a-mate 문서 묶음 안내 |
| [`01-product.md`](01-product.md) | 제품 정의·포지셔닝·설계 원칙 |
| [`02-features.md`](02-features.md) | 기능 카탈로그 |
| [`04-history-and-roadmap.md`](04-history-and-roadmap.md) | 개발 연혁·교훈·로드맵 |
| [`build-and-run.md`](build-and-run.md) | 빌드·실행·트러블슈팅 |
| [`docs/design/a-mate/`](../../design/a-mate/) | 설계 스펙·구현 플랜 (시점 기록) |
| [`a-mate/CLAUDE.md`](../../../a-mate/CLAUDE.md) | 개발 제약 |
| [`contracts/`](../../../contracts/) | a-hub와의 경계 계약 (C1/C2/C4) |
