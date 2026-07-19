# Agent Mentor — 아키텍처

> 코드베이스 실측(2026-07-10, main) 기준의 전체 구조. 설계 근거는 [`specs/2026-07-01-data-foundation-design.md`](specs/2026-07-01-data-foundation-design.md),
> [`specs/2026-07-03-frontend-vision-design.md`](specs/2026-07-03-frontend-vision-design.md) 참고.

## 1. 저장소 레이아웃

```
agent-mentor/
├─ Cargo.toml               # [workspace] members = ["crates/core", "src-tauri"]
├─ crates/core/             # 순수 Rust 백엔드 크레이트 (Tauri 의존 없음)
│  └─ src/                  #   lib(agent_mentor) + bin(agent-mentor CLI)
├─ src-tauri/               # Tauri v2 앱 크레이트 (core를 path 의존)
│  ├─ tauri.conf.json       #   chat(1000×760, 숨김 시작)·mascot(160×160, 투명) 창 정의
│  └─ src/                  #   lib.rs / commands.rs / pipeline.rs / tray.rs / geometry.rs
├─ src/                     # Svelte 5 + Vite 멀티페이지 프론트
│  ├─ chat.html → App.svelte       # 미니홈피 창
│  ├─ mascot.html → Mascot.svelte  # 마스코트 창
│  └─ lib/                  #   api.ts(브리지) / ui/(탭) / robot/(절차 렌더)
└─ docs/                    # brainstorming(킥오프) / specs(설계) / plans(구현 플랜) / overview(이 문서)
```

**의존 방향은 단방향**: `src/`(프론트) → `src-tauri`(셸) → `crates/core`(로직).
core는 상위를 전혀 모르며, Tauri 셸은 core의 순수 함수를 `#[tauri::command]`로 배선하는 얇은 어댑터다.
무거운 처리(JSONL 파싱·집계·감시)는 전부 Rust, 프론트는 표시·상호작용만.

### crates/core (패키지 `agent-mentor`)

- 의존성: `rusqlite`(bundled) / `serde` / `chrono` / `ureq`(LLM HTTP) / `sys-locale` / `sha2`. **Tauri 무의존**.
- 공개 모듈: `adapter, chat, coach, curation, diary, finding, hosts, inventory, mascot, model, ops, pipeline, rules, store, transcript`
- **CLI 바이너리 동봉** (`main.rs`): `agent-mentor ingest | inventory | rules | diary [date] | all` — 셸 없이 백엔드만 디버깅하는 용도. `./agent-mentor.db` 사용.

### src-tauri (패키지 `agent-mentor-app`)

- 의존성: `tauri`(tray-icon 기능) / `tauri-plugin-autostart` / `tauri-plugin-log` / `notify` / `dotenvy` / `agent-mentor`(path).
- `crate-type = ["rlib"]` — GNU ld의 export ordinal 한계 회피를 위해 cdylib 제외.
- `AppState { store: Mutex<SqliteStore>, scan_tx }` 전역 상태. DB는 `app_data_dir/agent-mentor.db`.

## 2. 데이터 흐름 (백그라운드 파이프라인)

```
notify 파일 감시                          Windows: RecommendedWatcher
  .claude/projects/**/*.jsonl 등          WSL UNC: 30초 PollWatcher 폴백
        │
        ▼
  60초 디바운스 배치 (FileChanged 병합, 트레이 "지금 스캔"=RunNow는 즉시)
        │
        ▼  ┌── store 뮤텍스 락 안 ──────────────────────────────┐
        │  before findings 스냅샷
        │  → run_ingest (증분, scan:progress 5파일마다 emit)
        │  → run_inventory (snapshot-replace)
        │  → run_rules (R1·R2·R5·R7·R9·R10·R11·R12)
        │  → finding diff (신규/악화만) → "coach:finding" emit
        │  → last_scan_ts 저장
        │  └────────────────────────────────────────────────────┘
        ▼
   "scan:done" emit
        │
        ▼  ── 락 밖(네트워크) — "락→브리프 조회→해제→LLM→락→persist" 규율 ──
   maybe_generate_diaries      (최근 7일 누락분 backfill → "diary:ready")
   maybe_generate_daily_line   (fingerprint stale 시만 → "daily-line:ready")
   maybe_generate_chatter_pool (fingerprint stale 시만, 이벤트 없이 프론트가 pull)
```

- 파이프라인은 전용 스레드 + 채널. watcher 사망 시 자동 재시작. 앱 시작 시 풀 스캔 1회.
- 디바운스·finding diff·누락 다이어리 날짜 계산 등은 **core의 Tauri 무관 순수 함수**(`pipeline.rs`)로 분리되어 단위 테스트되고, `src-tauri/pipeline.rs`는 스레드·emit 배선만 담당한다.

## 3. 추상화 심 (확장 지점)

에이전트별/엔진별 로직을 하드코딩하지 않기 위한 trait 경계. "agent-mentor"라는 이름의 약속이 코드로 표현된 곳.

### 3.1 SourceAdapter — 에이전트 확장 (`crates/core/src/adapter.rs`)

```rust
pub trait SourceAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>>;                 // 소스 파일 위치 열거
    fn read_incremental(&self, file, from_offset) -> Result<…>; // 완결 라인만 증분 읽기
    fn map(&self, line, source_file, source_offset) -> Vec<NormalizedEvent>;
}
```

- 유일 구현: `ClaudeCodeAdapter` (관대한 파싱 — 하드 실패 금지, 미지 스키마는 로깅만).
- 규칙·저장·다이어리는 **오직 NormalizedEvent만 본다** — 에이전트 포맷과 완전 분리. OpenCode/Codex 지원은 어댑터 구현체 하나를 추가하는 문제로 환원된다(YAGNI — 심만 정의, 필요 시 구현).
- 호스트 열거는 `hosts.rs` — Windows `%USERPROFILE%\.claude` + `wsl.exe -l -q`(UTF-16 디코드)로 전 distro의 `\\wsl.localhost\<distro>\home\*\.claude` 자동 발견.

### 3.2 Engine — LLM 교체 (`crates/core/src/diary/engine.rs`)

```rust
pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;   // 일기·한마디·잡담
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>; // 채팅 탭
}
```

- 구현체: `OpenAiCompatEngine`(사내 on-prem 등 OpenAI 호환 `/chat/completions`, `ureq`) + `MockEngine`(결정적 테스트용).
- 설정은 환경변수: `AGENT_MENTOR_ENGINE_URL`(필수) / `AGENT_MENTOR_ENGINE_KEY` / `AGENT_MENTOR_ENGINE_MODEL`. 미설정이면 LLM 기능만 조용히 no-op — 스캔·코칭은 정상 동작(프라이버시 기본값 = 완전 로컬).
- `EngineOutput.tokens_used`로 자기 토큰 계량.

### 3.3 Rule — 코칭 규칙 (`crates/core/src/rules/`)

```rust
pub trait Rule { fn id(&self) -> &str; fn evaluate(&self, store: &SqliteStore) -> Vec<Finding>; }
```

- `RuleEngine`이 등록된 8개 규칙(R1·R2·R5·R7·R9·R10·R11·R12)을 실행. 규칙 추가 = 구현체 추가 + 등록.
- `rules/session_stats.rs`가 세션 통계·자동화 버스트 판별의 단일 공급원(R7/R10/R12 공유).

### 3.4 SkillRecommendationSource — 스킬 추천 소스 (`crates/core/src/curation.rs`)

현재 `BuiltinCurationSource`(내장 큐레이션)만. 사내 스킬허브 검색 API나 외부 레지스트리(skills.sh 등)를 붙일 확장 시드.

## 4. 데이터 모델

### 4.1 NormalizedEvent — 내부 공용어 (`model.rs`)

모든 이벤트가 공통 봉투를 갖는다: `source_agent`, `host`(Windows|wsl:distro), `project_id`,
`session_id`/`uuid`/`parent_uuid`/`is_sidechain`, `ts`, **`source_file`+`source_offset`(원본 포인터)**.

```
EventKind =
  AssistantTurn { model(family·tier·raw), usage(input/output/cache_read/cache_creation/eph), web_search, web_fetch }
| ToolCall      { kind: ToolKind, raw_name, target }      // mcp__server__tool → McpCall{server,tool} 정규화
| ToolResult    { status: Ok|Denied|Error, … }            // 거부 마커 판정 (R11용)
| UserPrompt    { 첫 줄 미리보기 }
| SessionMeta   { cwd, git_branch }
| Compaction                                              // 컨텍스트 압축 경계 (R5 subtype용)
```

- **ToolKind 정규화**: 규칙은 원시 도구명이 아니라 정규 kind로만 말한다 → 에이전트 중립.
- **모델 티어링**: `claude-opus-4-8` → `{family: opus, tier: high}` — 규칙이 모델 id를 하드코딩하지 않음.
- **프로즈는 DB 밖**: 대화 원문은 저장하지 않고 포인터로 지연 로드(`ops::deref_jsonl_line`). 원본 삭제 시 미리보기 폴백.

### 4.2 SQLite 스키마 (`store.rs`)

| 테이블 | 내용 |
|---|---|
| `sessions` | 세션 메타 (host, project, first/last_ts, git_branch, cwd, 첫 프롬프트 미리보기 + 포인터) |
| `events` | 정규화 이벤트 (dedup_key UNIQUE, 토큰 6종, tool 분류, result_status, 포인터) |
| `daily_rollup` | (host, project, date) 일별 사전 집계 — 홈 탭·다이어리 질의용 |
| `findings` | 코칭 결과 (dedup_key UNIQUE, evidence/prescription JSON, est_tokens_saved, status, occurrences) |
| `mcp_inventory` / `plugin_inventory` | 활성 설정 스냅샷 (R1/R2의 원천, snapshot-replace) |
| `diary_index` | 날짜별 일기 md 경로·소비 토큰·엔진 |
| `daily_line` / `chatter_pool` | 오늘의 한마디·잡담 풀 (date PK + fingerprint 캐시) |
| `ingest_state` | source_file별 last_offset — 증분 수집 워터마크 |
| `settings` | key/value (mascot_visible, chatter_level, content_protected, mascot_pos, realtime_advice 등 화이트리스트) |

- 마이그레이션은 `pragma_table_info` 검사 후 `ALTER TABLE ADD COLUMN`. 수집 파생 테이블(events 등)은 비우고 **로컬 JSONL에서 전체 재수집**하는 전략 — 원본이 항상 로컬에 있다는 전제를 활용. findings 상태·diary_index는 보존.
- 멱등성: events는 dedup_key `INSERT OR IGNORE`, findings는 dedup_key 충돌 시 occurrences 증가 + evidence 갱신(status는 불변 — 해결함/무시 유지).

## 5. Tauri 표면 (커맨드 · 이벤트 · 창)

### 5.1 커맨드 (19개, `commands.rs`)

| 그룹 | 커맨드 |
|---|---|
| 요약/통계 | `get_summary` `get_week_summary` `get_model_mix` |
| 코칭 | `list_findings(include_hidden)` `set_finding_status` `sessions_ctx(ids)` `get_session_transcript` |
| 다이어리 | `list_diary_dates` `get_diary(date)` |
| 마스코트 | `get_mascot_seed` `get_daily_line` `get_chatter_pool` `get_today_occasions` |
| 채팅 | `chat_status` `chat_send(messages)` |
| 제어/설정 | `run_scan_now` `open_chat_tab(tab)` `get_settings` `set_setting(key,value)` |

- 각 커맨드는 `*_inner` 순수 함수로 분리되어 테스트된다. 설정 키·탭 이름·채팅 role은 화이트리스트 검증.
- `chat_send`: system 프롬프트는 백엔드가 조립(코칭 컨텍스트 주입), 최근 20턴만 전달, 네트워크는 락 밖.

### 5.2 이벤트 (백엔드 → 프론트)

`scan:progress` · `scan:done` · `coach:finding` · `diary:ready` · `daily-line:ready` · `occasion:today` · `chat:goto-tab` · `settings:changed`

### 5.3 창과 트레이

| 표면 | 구성 |
|---|---|
| tray | 좌클릭 → chat 토글. 메뉴: 열기 / 마스코트 표시 ✓ / 실시간 조언 ✓ / 화면 캡처 보호 ✓ / 잡담(자주·가끔·안 함) / 지금 스캔 / 시작 시 실행 ✓ / 종료. 설정의 소스오브트루스는 store — CheckMenuItem 자동 토글을 덮어씀 |
| chat | 1000×760, 숨김 시작, X → destroy 아닌 hide (상주) |
| mascot | 160×160 투명·무장식·alwaysOnTop·skipTaskbar. 말풍선 시 320×230 일시 확장(우하단 고정 보정). 위치는 `geometry::sanitize_pos`로 모니터 구성 변경 방어 후 복원 |

`content_protected` 설정 시 두 창 모두 화면 캡처에서 제외(사내 코드 노출 대비, 기본 off).

## 6. 프론트엔드 구조

```
src/
├─ App.svelte              # chat 창 루트 — 미니홈피 셸(사이드바+세로 탭), 이벤트 구독
├─ Mascot.svelte           # mascot 창 루트 — 렌더 루프, 말풍선, 잡담 타이머, 클릭/드래그 판별
└─ lib/
   ├─ api.ts               # 모든 invoke 래퍼 + 타입 + 이벤트 리스너 (유일한 백엔드 접점)
   ├─ ui/                  # HomeTab / CoachTab / DiaryTab / ChatTab / SessionModal / MiniRoom
   │  ├─ home/             #   WeekTrend·ModelMix·SaveTop3·NoticeLog 위젯
   │  ├─ calendar.ts, coach-helpers.ts, notices.ts, chat-store.svelte.ts
   └─ robot/               # 절차 생성 로봇
      ├─ parts.ts          #   슬롯별 변형 정의 + 팔레트 (Rust 상수와 개수 일치를 vitest가 단언)
      ├─ render.ts         #   buildRobotShapes(spec, frame) → Shape[] → drawRobot(canvas)
      ├─ anim.ts           #   상태 머신(idle/talk/happy/alert/sleep) + frameAt 순수 함수
      └─ bubble.ts         #   말풍선 텍스트 템플릿 + pickChatter(풀+정적 혼합, 최근 3개 회피)
```

- 다이어리 본문은 `marked` + `DOMPurify.sanitize`로 렌더 — LLM 산출물 살균.
- 채팅 이력은 창 수명 메모리(`$state`)만, 영속화 없음.
- 알림 히스토리는 localStorage 최근 20건 (백엔드 스키마 불변).

## 7. 테스트 전략

| 층 | 방식 |
|---|---|
| core | 거의 전 모듈에 동봉된 단위 테스트. store는 `open_in_memory()`, 엔진은 `MockEngine`으로 결정적 |
| src-tauri | 디바운스·diff·geometry 등을 Tauri 무관 순수 함수로 분리해 단위 테스트, 커맨드는 `*_inner`로 테스트 |
| 프론트 | vitest — calendar/coach-helpers/notices/robot(anim·bubble·drag·parts). 로봇 렌더는 canvas 무관 `buildRobotShapes`의 결정성·경계 단언 |
| E2E | 수동 체크리스트 (Tauri WebDriver 불안정) — 실사용 E2E가 설계 피드백 루프의 핵심 |

핵심 패턴: **부수효과(네트워크·파일·창)를 가장자리로 밀고 판단 로직은 순수 함수로** — LLM 호출 여부(`compute_daily_line`), 프레임 계산(`frameAt`), 좌표 검증(`sanitize_pos`) 등이 전부 이 패턴이다.

## 8. 실행 방법

```sh
# 백엔드만 (CLI, ./agent-mentor.db 사용)
cargo run -p agent-mentor -- all        # ingest → inventory → rules → diary

# 데스크톱 앱 (개발)
npm run tauri dev

# 테스트
cargo test --workspace && npm test
```

LLM 기능을 켜려면 `AGENT_MENTOR_ENGINE_URL`(+`_KEY`, `_MODEL`)을 설정한다(`.env` 지원, `dotenvy`).
미설정이어도 수집·코칭·UI는 전부 동작한다.
