# Agent Mentor — 데이터 파운데이션(Data Foundation) 설계

> **상태**: 브레인스토밍 합의 완료 (2026-07-01)
> **범위**: 코칭 지능 설계(`docs/specs/2026-07-01-coaching-intelligence-design.md`)가 전제하는 **"정규화된 세션 이벤트"를 실제로 만드는 층** — 감시·파싱·어댑터·저장·인벤토리·엔진 어댑터.
> **관계**: 이 스펙이 코칭 스펙의 선행 의존이다. 3-윈도우 UI 상세, 마스코트, 트레이/업데이터는 별도 설계.

---

## 0. 이 층이 하는 일 (한 줄)

Windows + WSL의 여러 에이전트 사용 기록을 **관대하게 파싱**해 하나의 **NormalizedEvent 스트림 + 인벤토리**로 만들어 SQLite에 넣고, 그 위에서 규칙 엔진이 도는 **믿을 수 있는 지반**을 제공한다.

---

## 1. 확정 결정 로그 (2026-07-01)

1. **원본 직접 파싱 + 버전 인지형 어댑터** — ccusage 위임(kickoff §2.4)을 재검토해 뒤집음. 코칭 신호(턴별 cache_read/creation, tool_use 시퀀스, 메시지별 model, file_path 반복)는 **턴 단위 이벤트**가 필요하고 ccusage는 집계기라 못 준다. 어차피 원본을 파싱해야 하므로 직접 파싱하고, 스키마 취약성은 **우리가 통제하는 어댑터**로 방어. ccusage는 의존 대상에서 제외, 원하면 토큰 총합 **선택적 교차검증**으로만.
2. **2-레이어 어댑터 심(seam) + YAGNI** — `SourceAdapter`(에이전트별 위치·원시 읽기) → `NormalizedEvent`(내부 공용어). 규칙·저장·다이어리는 오직 NormalizedEvent만 본다. **심은 지금 정의하되 구현은 Claude Code 어댑터 하나만.** OpenCode/Codex는 실제 필요 시.
3. **관대한 파싱** — Claude 어댑터 내부는 `#[serde(default)]` + `Option` + 미지 필드 catch-all(`extra`)로 **절대 하드 실패하지 않음**. 스키마 이상은 로깅만.
4. **로컬 풀 컨텍스트 + egress는 엔진 선택으로** — 본인 PC의 본인 데이터라 로컬 처리엔 프라이버시 제약 없음. NormalizedEvent는 **메타 + 원본 소스 포인터**라 필요 시 원본을 로컬에서 당겨 씀. 데이터 반출 경계는 LLM 엔진 선택이 좌우(코칭 스펙 §6.3). 별도 리댁트 모드 없음(YAGNI).
5. **엔진 어댑터 = 사내 on-prem 기본, Claude 옵션** — 코칭 스펙 §1.6과 동일. on-prem은 OpenAI 호환 엔드포인트 가정.
6. **R1 always-on 토큰 측정** — 총 always-on은 첫 턴 `cache_creation`으로 정확 측정. 서버별 귀속은 **차등+휴리스틱 기본(무부작용)**, 정확 프로브는 **명시적 옵트인 버튼**(MCP 서버 1회 실행). 추정치는 "약(~)" 라벨.

---

## 2. 실측 검증 사실 (이 설계의 근거)

이 세션에서 실제 `~/.claude`를 열어 확인한 사실. 가정이 아니라 관측이다.

### 2.1 트랜스크립트 JSONL (`.claude/projects/<enc-path>/*.jsonl`)
- 라인 타입: `assistant`, `user`, `attachment`, `system`, `file-history-snapshot`, `last-prompt`, `mode`, `permission-mode`, `ai-title`
- `assistant` 라인 → `message.model`(예: `claude-opus-4-8`), `message.usage`:
  - `input_tokens`, `output_tokens`
  - `cache_read_input_tokens`, `cache_creation_input_tokens`
  - `cache_creation.ephemeral_{5m,1h}_input_tokens`
  - `server_tool_use.{web_search,web_fetch}_requests`
  - `iterations[]` (턴 내부 반복)
- `tool_use` 블록 → `name`, `input`(예: `file_path`)
- 라인 공통 → `sessionId`, `isSidechain`(서브에이전트 구분), `parentUuid`
- 프로젝트 귀속: 디렉터리명이 cwd를 인코딩(`C--Users-jibin`, `D--Project-agent-mentor` 등)

### 2.2 MCP/설정 위치 (R1/R2의 원천)
- **글로벌 플러그인**: `settings.json` `enabledPlugins`(플러그인이 MCP 제공)
- **프로젝트별 MCP**: `~/.claude.json` `projects.<path>.{mcpServers, enabledMcpjsonServers, disabledMcpjsonServers}`
- **프로젝트 로컬**: 프로젝트 `.mcp.json`
- → R1은 이 세 곳을 종합해 "세션에서 활성인 MCP 셋"을 산출해야 한다.

### 2.3 WSL 실체
- distro 1개: **Ubuntu-22.04**, `\\wsl.localhost\Ubuntu-22.04\home\jayb\`로 접근.
- **함정**: (1) WSL username(`jayb`) ≠ Windows username(`jibin`) → 홈 경로 하드코딩 금지. (2) 현대 경로는 `\\wsl.localhost\`(kickoff이 적은 `\\wsl$\` 아님) → 둘 다 시도.

---

## 3. 어댑터 아키텍처 (2-레이어 심)

```
SourceAdapter (에이전트별)                 NormalizedEvent (공용어)
─────────────────────────                 ────────────────────────
ClaudeCodeAdapter                          ┐
  - discover(): 소스 파일 위치 열거          │  규칙·저장·다이어리는
  - read_incremental(offset): 원시 레코드    ├─►  오직 이것만 본다
  - map(raw) -> NormalizedEvent[]           │  (에이전트 포맷과 완전 분리)
  - 관대한 파싱(하드 실패 금지)              ┘
[미구현] OpenCodeAdapter / CodexAdapter … 심만 존재
```

- `SourceAdapter` trait: `discover`, `read_incremental`, `map`. 각 에이전트가 구현.
- **YAGNI**: 지금은 `ClaudeCodeAdapter`만 구현. 심의 유일한 책무는 NormalizedEvent를 에이전트 중립으로 유지하는 것.

---

## 4. NormalizedEvent 모델

### 4.1 봉투 (모든 이벤트 공통)
```
source_agent + schema_version   // claude-code, 관측된 버전 태그
host                            // Windows | wsl:Ubuntu-22.04
project_id                      // 정규화된 cwd
session_id, uuid, parent_uuid, is_sidechain
ts
source_file, source_offset      // ← 원본 소스 포인터(결정 #4)
```

### 4.2 EventKind
```
AssistantTurn { model: NormModel, usage: TokenUsage, web_search: u32, web_fetch: u32 }
ToolCall      { kind: ToolKind, raw_name: String, target: Option<String> }
ToolResult    { for_call: uuid, outcome: Ok|Err, size }
UserPrompt    { char_len, had_attachments }
SessionMeta   { cwd, git_branch?, started_at, mode }
```

### 4.3 정규화 서브타입
```
ToolKind  = FileRead | FileEdit | FileWrite | Search | Execute
          | McpCall{server, tool} | WebSearch | WebFetch | SubAgent | Other(raw)
NormModel = { family: opus|sonnet|haiku|other, tier: high|mid|low, raw_id }
TokenUsage= { input, output, cache_read, cache_creation, eph_1h, eph_5m }
```

### 4.4 인벤토리 스냅샷 (이벤트 스트림과 별개)
```
McpInventory { host, project, servers:[{name, source: project|plugin|global, tool_count, est_def_tokens?}] }
ToolAssets   { skills[], plugins[] }   // 신규 감지 diff용
```

### 4.5 여기 박힌 핵심 결정
1. **로컬 풀 컨텍스트 + 소스 포인터**(결정 #4) — 내용을 중복 저장하지 않되, `source_file/offset`로 원본을 로컬에서 자유 참조.
2. **ToolKind 정규화 + MCP명 파싱** — 원시 도구명(`mcp__server__tool` 등)을 정규 kind로. 규칙은 정규 kind로만 말함 → 에이전트 중립 + 규칙 안정.
3. **모델 티어링** — `claude-opus-4-8` → `{family:opus, tier:high}`. 규칙이 id 하드코딩 안 함.
4. **인벤토리는 별개 아티팩트** — 시점 스냅샷 + diff. "새 도구 배움"·R1·R2가 여기서.

---

## 5. SQLite 스키마 (초안)

```
sessions(session_id PK, host, project_id, agent, first_ts, last_ts, parent_session?, git_branch)

events(id, session_id FK, seq, ts, kind, model_family, model_tier,
       tok_input, tok_output, tok_cache_read, tok_cache_create, tok_eph_1h,
       tool_kind, tool_target, raw_name, web_search, web_fetch,
       source_file, source_offset)               -- 소스 포인터

daily_rollup(host, project_id, date, tokens_by_type…, model_mix,
             cache_ratio, session_count)          -- 다이어리/채팅용 사전 집계 뷰

findings(id, rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
         evidence_json, est_tokens_saved, prescription_json, status,
         first_seen, last_seen, occurrences)

mcp_inventory(host, project_id, server, source, tool_count, est_def_tokens,
              probed_at, last_used_ts)

tool_assets(host, kind, name, first_seen, last_used_ts)   -- 새 도구 감지

diary_index(date, scope, path, tokens_used, engine)

ingest_state(source_file PK, last_offset, last_mtime)      -- 증분 재개 워터마크
```

- `daily_rollup`은 다이어리·채팅 질의(코칭 스펙 §7.4)를 위한 사전 집계.
- `findings`는 코칭 스펙 §3.2 Finding 모델의 저장 형태.

---

## 6. 수집(Ingestion) 메커니즘

- `notify`(Rust)가 소스 디렉터리 감시 → 변경 시 `ingest_state.last_offset`부터 **증분 읽기**.
- **완결 라인만 파싱**, 쓰이는 중인 부분 tail은 버퍼링(동시 다중 세션 append 안전 — 코칭 스펙 §4).
- `(session_id, seq)` 기준 **멱등 upsert** → 재실행/재시작 안전.
- 이벤트 수집과 동시에 `daily_rollup` 증분 갱신.
- 시작 시 워터마크 vs 파일 mtime 비교로 놓친 변경 복구.

---

## 7. 호스트/소스 열거

- **자동 발견 기본**:
  - Windows: `%USERPROFILE%\.claude`
  - WSL: `wsl -l -q`로 distro 열거 → 각 distro `\\wsl.localhost\<distro>\home\*\.claude\projects` 존재 탐색(**username 하드코딩 금지**), `\\wsl.localhost\` 실패 시 `\\wsl$\` 폴백.
- **커스텀 소스 추가**(설정): 마운트 드라이브·커스텀 `CLAUDE_CONFIG_DIR` 등 엣지.
- 각 소스는 `host` 라벨로 구분되어 NormalizedEvent 봉투에 실림.

---

## 8. R1 always-on MCP 토큰 측정 (헤드라인 차별점의 지반)

- **총 always-on = 정확** — 세션 첫 턴 `cache_creation_input_tokens` ≈ 시스템 프롬프트 + 전체 도구 정의 + CLAUDE.md. 그 자체로 강력한 코칭("대화 전에 이미 55k 깔고 시작").
- **미사용 탐지 = 정확** — 활성 MCP 셋(§2.2) vs 그 서버 도구 호출 0회(events의 `McpCall`).
- **서버별 비용 귀속 = 추정**:
  - 기본(무부작용): 세션 간 활성 MCP 셋이 다를 때 `cache_creation` 델타를 귀속(차등) + 도구 개수 휴리스틱.
  - 옵트인 프로브: 설정 변경 시 사용자가 "정확히 재보기" 버튼 → MCP 서버 1회 연결, 도구 스키마 토큰 계수, `mcp_inventory.est_def_tokens` 캐싱.
- **정직한 라벨링** — 추정치는 "약(~)". 코칭 스펙 "토큰 정직성"과 일관.

---

## 9. Non-goals · 유예 · 열린 질문

**Non-goals**
- ccusage 재구현/의존 (선택적 교차검증만)
- OpenCode/Codex 어댑터 구현 (심만 정의)
- 3-윈도우 UI / 마스코트 / 트레이 / 업데이터 (별도 설계)

**유예/추후**
- `schema_version` 태깅 방식(관측 기반 vs Claude Code 버전 매핑) 구체화
- 프로젝트 경로 인코딩(`C--Users-jibin`) ↔ 실제 cwd 역매핑 규칙
- `daily_rollup`의 정확한 컬럼 셋(코칭 규칙 확정과 함께 굳힘)
- 휴리스틱 "도구당 평균 토큰" 계수 초기값(실측 프로브로 보정)
- 토큰 계수기(tokenizer) 선택 — Claude 토크나이저 근사 vs 바이트 휴리스틱

**kickoff에서 이 문서가 닫는 열린 질문**
- §3.1 실제 JSONL 스키마 검증 → §2
- §3.1 여러 WSL distro 열거·통합 → §7
- §3.1 스키마 버전 방어 추상화 → §3(어댑터) + 결정 #3(관대한 파싱)
- §3.1 타 에이전트 일반화 → §3(2-레이어 심)
- §2.4 ccusage 위임 재검토 → 결정 #1(직접 파싱으로 뒤집음)

---

## 10. 첫 스프린트 (코칭 스펙 §10과 합류)

1. **ClaudeCodeAdapter + NormalizedEvent + SQLite** — 실측 JSONL을 관대하게 파싱해 events/sessions/rollup 적재 → *검증: 실제 내 히스토리에서 세션·토큰 롤업이 ccusage 총합과 근사한가(교차검증)*
2. **호스트 열거 + notify 증분 수집** — Win + Ubuntu-22.04 양쪽 스캔·tail → *검증: 양 호스트 세션이 한 스토어에 host 라벨로 통합되나*
3. **인벤토리 + R1 지반** — MCP 셋 산출 + 첫 턴 cache_creation 총 always-on → *검증: "안 쓰는 MCP + 대략 비용"이 실제로 뜨나*

→ 이 위에 코칭 스펙 §10의 Finding·규칙(R1/R5)·다이어리 최소판이 얹혀 첫 스프린트가 완성된다.
