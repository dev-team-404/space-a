# Backend Vertical Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 실측 Claude Code JSONL을 배치 파싱해 SQLite에 적재하고, Tier 0 규칙(R5 반복 Read, R1 안 쓰는 always-on MCP)으로 Finding을 만든 뒤, 하루치 Finding을 LLM 엔진으로 에이전트-시점 다이어리 `.md`로 렌더링(토큰 자기계량 포함)하는 백엔드 세로 슬라이스를 완성한다.

**Architecture:** 순수 Rust 라이브러리 크레이트(+검증용 CLI 바이너리). 데이터 흐름은 `SourceAdapter(ClaudeCode) → NormalizedEvent → SqliteStore → RuleEngine(R5·R1) → Finding → Diary(brief → Engine → vault .md)`. 코칭을 **한 번 계산(Finding)** 하고 다이어리로 **렌더링**하는 "한 뇌, 여러 갈래" 명제를 끝-끝으로 증명한다. LLM 호출은 `Engine` trait 뒤로 격리해 결정적 부분은 단위테스트, 파이프라인은 `MockEngine`으로 테스트, 실제 엔진은 수동 검증한다.

**Tech Stack:** Rust (edition 2021), `serde`/`serde_json`(관대한 Value 기반 파싱), `rusqlite`(bundled SQLite), `chrono`(ISO8601·날짜), `ureq`(OpenAI 호환 엔진 HTTP), `anyhow`(에러), `tempfile`(dev, 스토어 테스트).

## Global Constraints

이 섹션은 모든 태스크의 요구사항에 암묵적으로 포함된다. 값은 kickoff §7 및 두 스펙에서 그대로 옮김.

- **제품 정체성:** 식별자 `agent-mentor`. 에이전트별 로직을 하드코딩하지 말고 `SourceAdapter` / `Engine` 어댑터 뒤로 추상화할 것 (타 에이전트 확장 대비).
- **최종 스택은 Tauri v2 + Rust:** v1 API(`SystemTray`, `tauri::updater`, `WindowBuilder`) 사용 금지. **단, 이 슬라이스는 Tauri 셸 이전 단계다.** UI/트레이/윈도우 없이 순수 백엔드 크레이트로 만들고, 공개 함수 시그니처를 나중에 Tauri v2 `#[tauri::command]`로 배선할 수 있게 유지한다. (Tauri 의존성은 이 슬라이스에 추가하지 않는다 — YAGNI.)
- **플랫폼:** Windows 전용. macOS/Linux 분기 불필요. **이 슬라이스는 Windows 호스트만 다룬다** — WSL 열거는 다음 슬라이스(데이터 파운데이션 §10.2).
- **프라이버시:** 로컬 풀 컨텍스트 허용(본인 PC 본인 데이터). 외부 전송(egress)은 오직 `Engine` 선택으로 결정 — 기본 사내 on-prem(OpenAI 호환), Claude는 옵션(코칭 §1.6). 별도 리댁트 모드 없음(YAGNI).
- **정밀도의 선(코칭 §1.4):** 처방(결과 있는 행동)은 **결정론적 규칙**만 생성. 서사·인정은 LLM. LLM은 브리프의 사실 위에서만 수치를 말하고, 자유 소감을 행동 카드로 승격하지 않는다.
- **관대한 파싱(데이터 파운데이션 결정 #3):** Claude 어댑터는 **절대 하드 실패하지 않는다.** 스키마 이상은 로깅/스킵만.
- **토큰 정직성:** 추정치는 "약(~)" 라벨. LLM 자기 소비 토큰을 계량해 다이어리 푸터에 표기.
- **수집 안전성:** 완결(개행 종료) 라인만 파싱. `(session_id, seq)` 대응 멱등 upsert로 재실행 안전.
- **실측 스키마(데이터 파운데이션 §2.1, 이 세션 검증됨):** `assistant` 라인 최상위 키 = `cwd, gitBranch, isSidechain, message, parentUuid, sessionId, timestamp, type, uuid, version`. `message.usage` = `input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, server_tool_use.{web_search_requests, web_fetch_requests}, cache_creation.{ephemeral_1h_input_tokens, ephemeral_5m_input_tokens}`. tool_use는 `message.content[]`의 `{type:"tool_use", name, input}` 블록.

---

## File Structure

단일 크레이트(lib + bin). 모듈은 스펙의 개념 층에 대응하며 파일당 단일 책임.

```
Cargo.toml                     # 워크스페이스 아님, 단일 크레이트
CLAUDE.md                      # 프로젝트 제약(kickoff §7)
.gitignore                     # Rust target/ 추가
src/
  lib.rs                       # 모듈 선언 + 재수출
  model.rs                     # NormalizedEvent, EventKind, ToolKind, NormModel, TokenUsage
  finding.rs                   # Finding, Severity, Prescription, Scope
  adapter.rs                   # SourceAdapter trait + ClaudeCodeAdapter (discover/read_incremental/map)
  store.rs                     # SqliteStore: schema, upsert_events, rebuild_rollup, findings, ingest_state, inventory
  inventory.rs                 # MCP 인벤토리 파싱(~/.claude.json, .mcp.json) → active server set
  rules/
    mod.rs                     # Rule trait + RuleEngine + normalize_project_key
    r5_repeated_read.rs        # R5
    r1_unused_mcp.rs           # R1
  diary/
    mod.rs                     # Brief, DiaryConfig, DiaryOutput, generate_diary
    engine.rs                  # Engine trait + MockEngine + OpenAiCompatEngine + EngineOutput
  main.rs                      # 검증용 CLI (ingest / rules / diary / all)
```

**핵심 join 결정 (R1이 events와 인벤토리를 잇는 방법):** events의 `project_id`는 트랜스크립트 디렉터리명(예 `C--Users-jibin`)에서, 인벤토리의 프로젝트 키는 `~/.claude.json`의 실제 cwd 경로(예 `C:\Users\jibin`)에서 온다. 스펙 §9가 유예한 정확한 역매핑 대신, **양쪽에 동일한 `normalize_project_key()`**(소문자화 + `\`/`:`→`-`)를 적용해 v0 join을 성립시킨다. `C--Users-jibin` → `c--users-jibin`, `C:\Users\jibin` → `c--users-jibin` → 일치.

---

## Task 0: 프로젝트 스캐폴드

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `CLAUDE.md`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: (없음 — 첫 태스크)
- Produces: 컴파일되는 빈 크레이트. `agent_mentor` 라이브러리 이름. `cargo test`/`cargo run` 동작.

- [ ] **Step 1: `Cargo.toml` 작성**

```toml
[package]
name = "agent-mentor"
version = "0.1.0"
edition = "2021"

[lib]
name = "agent_mentor"
path = "src/lib.rs"

[[bin]]
name = "agent-mentor"
path = "src/main.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
chrono = "0.4"
ureq = { version = "2", features = ["json"] }
anyhow = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: `src/lib.rs` 작성 (모듈 골격)**

```rust
pub mod adapter;
pub mod diary;
pub mod finding;
pub mod inventory;
pub mod model;
pub mod rules;
pub mod store;

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 3: 빈 모듈 파일 생성 (컴파일용 스텁)**

각 파일을 아래 내용으로 생성한다(이후 태스크에서 채움):

`src/model.rs`, `src/finding.rs`, `src/adapter.rs`, `src/store.rs`, `src/inventory.rs`, `src/diary/mod.rs`, `src/diary/engine.rs` → 각각 빈 파일(`// placeholder\n`).
`src/rules/mod.rs` → `pub mod r1_unused_mcp;\npub mod r5_repeated_read;\n`
`src/rules/r5_repeated_read.rs`, `src/rules/r1_unused_mcp.rs` → 각각 `// placeholder\n`.
`src/diary/mod.rs` 상단에 `pub mod engine;` 추가.

`src/main.rs`:

```rust
fn main() {
    println!("agent-mentor cli (slice)");
}
```

- [ ] **Step 4: `CLAUDE.md` 작성 (kickoff §7 제약 + 슬라이스 주석)**

```markdown
# Agent Mentor — 프로젝트 제약

- 제품명: Agent Mentor. 식별자 `agent-mentor`. Claude 외 타 에이전트 확장을 염두에 둔 이름이므로,
  에이전트별 로직은 하드코딩하지 말고 SourceAdapter / Engine 인터페이스 뒤로 추상화할 것.
- 최종 스택: Tauri v2 + Rust 백엔드. v1 API(SystemTray, tauri::updater, WindowBuilder 등) 금지.
  단, 현재는 Tauri 셸 이전의 순수 백엔드 크레이트 단계다. 공개 함수는 나중에 #[tauri::command]로
  배선 가능하게 유지한다.
- 상주/업데이트/자동시작은 (도입 시) 반드시 v2 공식 플러그인(tray-icon, updater, autostart)으로.
- 플랫폼: Windows 전용. macOS/Linux 분기 불필요.
- 프라이버시: 트랜스크립트는 기본 로컬 처리. 외부 전송은 Engine 선택(사내 on-prem 기본)으로만.
- 무거운 데이터 처리(JSONL 파싱/집계/감시)는 Rust 백엔드에서.
- 설계 스펙: docs/specs/, 구현 계획: docs/plans/.
```

- [ ] **Step 5: `.gitignore`에 Rust 산출물 추가**

기존 `.gitignore` 끝에 다음 줄을 추가:

```
/target
```

- [ ] **Step 6: 빌드+테스트로 스캐폴드 검증**

Run: `cargo test`
Expected: PASS (`crate_compiles ... ok`), 경고는 무방(빈 모듈).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml CLAUDE.md .gitignore src/
git commit -m "chore: scaffold agent-mentor backend crate"
```

---

## Task 1: NormalizedEvent 모델

**Files:**
- Create/replace: `src/model.rs`

**Interfaces:**
- Consumes: (없음)
- Produces:
  - `pub struct NormalizedEvent { source_agent:String, schema_version:String, host:String, project_id:String, session_id:String, uuid:Option<String>, parent_uuid:Option<String>, is_sidechain:bool, ts:Option<String>, source_file:String, source_offset:u64, kind:EventKind }`
  - `pub enum EventKind { AssistantTurn{model:NormModel, usage:TokenUsage, web_search:u32, web_fetch:u32}, ToolCall{kind:ToolKind, raw_name:String, target:Option<String>}, SessionMeta{cwd:String, git_branch:Option<String>} }`
  - `pub enum ToolKind { FileRead, FileEdit, FileWrite, Search, Execute, McpCall{server:String, tool:String}, WebSearch, WebFetch, SubAgent, Other(String) }` + `ToolKind::from_raw_name(&str)->ToolKind`
  - `pub struct NormModel { family:ModelFamily, tier:ModelTier, raw_id:String }` + `NormModel::from_raw_id(&str)->NormModel`; `pub enum ModelFamily{Opus,Sonnet,Haiku,Other}`; `pub enum ModelTier{High,Mid,Low}`
  - `pub struct TokenUsage { input:u64, output:u64, cache_read:u64, cache_creation:u64, eph_1h:u64, eph_5m:u64 }` (`Default`)

- [ ] **Step 1: 실패하는 테스트 작성 — 모델 티어링 + MCP명 파싱**

`src/model.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tiering_maps_family_and_tier() {
        let m = NormModel::from_raw_id("claude-opus-4-8");
        assert_eq!(m.family, ModelFamily::Opus);
        assert_eq!(m.tier, ModelTier::High);
        assert_eq!(m.raw_id, "claude-opus-4-8");

        assert_eq!(NormModel::from_raw_id("claude-haiku-4-5").tier, ModelTier::Low);
        assert_eq!(NormModel::from_raw_id("weird-model").family, ModelFamily::Other);
    }

    #[test]
    fn mcp_name_parses_server_and_tool() {
        match ToolKind::from_raw_name("mcp__context7__query-docs") {
            ToolKind::McpCall { server, tool } => {
                assert_eq!(server, "context7");
                assert_eq!(tool, "query-docs");
            }
            other => panic!("expected McpCall, got {other:?}"),
        }
        assert_eq!(ToolKind::from_raw_name("Read"), ToolKind::FileRead);
        assert_eq!(ToolKind::from_raw_name("Bash"), ToolKind::Execute);
        assert_eq!(
            ToolKind::from_raw_name("SomethingNew"),
            ToolKind::Other("SomethingNew".to_string())
        );
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib model`
Expected: FAIL — `NormModel`, `ToolKind` 등 미정의로 컴파일 에러.

- [ ] **Step 3: 최소 구현 작성**

`src/model.rs` 상단(테스트 위)에:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelFamily { Opus, Sonnet, Haiku, Other }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelTier { High, Mid, Low }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormModel {
    pub family: ModelFamily,
    pub tier: ModelTier,
    pub raw_id: String,
}

impl NormModel {
    pub fn from_raw_id(raw: &str) -> NormModel {
        let l = raw.to_ascii_lowercase();
        let (family, tier) = if l.contains("opus") {
            (ModelFamily::Opus, ModelTier::High)
        } else if l.contains("sonnet") {
            (ModelFamily::Sonnet, ModelTier::Mid)
        } else if l.contains("haiku") {
            (ModelFamily::Haiku, ModelTier::Low)
        } else {
            (ModelFamily::Other, ModelTier::Mid)
        };
        NormModel { family, tier, raw_id: raw.to_string() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub eph_1h: u64,
    pub eph_5m: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ToolKind {
    FileRead,
    FileEdit,
    FileWrite,
    Search,
    Execute,
    McpCall { server: String, tool: String },
    WebSearch,
    WebFetch,
    SubAgent,
    Other(String),
}

impl ToolKind {
    pub fn from_raw_name(raw: &str) -> ToolKind {
        if let Some(rest) = raw.strip_prefix("mcp__") {
            let mut parts = rest.splitn(2, "__");
            let server = parts.next().unwrap_or("").to_string();
            let tool = parts.next().unwrap_or("").to_string();
            return ToolKind::McpCall { server, tool };
        }
        match raw {
            "Read" => ToolKind::FileRead,
            "Edit" | "MultiEdit" => ToolKind::FileEdit,
            "Write" => ToolKind::FileWrite,
            "Grep" | "Glob" => ToolKind::Search,
            "Bash" => ToolKind::Execute,
            "WebSearch" => ToolKind::WebSearch,
            "WebFetch" => ToolKind::WebFetch,
            "Task" => ToolKind::SubAgent,
            other => ToolKind::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EventKind {
    AssistantTurn { model: NormModel, usage: TokenUsage, web_search: u32, web_fetch: u32 },
    ToolCall { kind: ToolKind, raw_name: String, target: Option<String> },
    SessionMeta { cwd: String, git_branch: Option<String> },
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedEvent {
    pub source_agent: String,
    pub schema_version: String,
    pub host: String,
    pub project_id: String,
    pub session_id: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub is_sidechain: bool,
    pub ts: Option<String>,
    pub source_file: String,
    pub source_offset: u64,
    pub kind: EventKind,
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib model`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/model.rs
git commit -m "feat: add NormalizedEvent model with model tiering and tool-kind parsing"
```

---

## Task 2: ClaudeCodeAdapter (관대한 파싱)

**Files:**
- Create/replace: `src/adapter.rs`

**Interfaces:**
- Consumes: `model::*` (Task 1)
- Produces:
  - `pub trait SourceAdapter { fn discover(&self) -> anyhow::Result<Vec<std::path::PathBuf>>; fn read_incremental(&self, file:&Path, from_offset:u64) -> anyhow::Result<(Vec<String>, u64)>; fn map(&self, line:&str, source_file:&str, source_offset:u64) -> Vec<NormalizedEvent>; }`
  - `pub struct ClaudeCodeAdapter { pub root: PathBuf, pub host: String }` (구현체) + `ClaudeCodeAdapter::windows() -> ClaudeCodeAdapter`
  - `map`은 한 줄에서 AssistantTurn 1개 + tool_use 블록마다 ToolCall를 방출. 파싱 불가/미지 라인은 빈 벡터.
  - `read_incremental`은 **개행으로 끝난 완결 라인만** 반환, 새 오프셋 반환.
  - `pub fn decode_project_id(dir_name:&str) -> String` (디렉터리명 → 정규화 project_id, `rules` 모듈과 공유하기 위해 여기 두되 재수출)

- [ ] **Step 1: 실패하는 테스트 작성 — map + read_incremental**

`src/adapter.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, ToolKind};

    fn adapter() -> ClaudeCodeAdapter {
        ClaudeCodeAdapter { root: std::path::PathBuf::from("."), host: "Windows".into() }
    }

    #[test]
    fn map_assistant_line_yields_turn_and_tool_calls() {
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","parentUuid":null,
            "isSidechain":false,"timestamp":"2026-07-01T10:00:00Z","cwd":"C:\\Users\\jibin",
            "gitBranch":"main","message":{"model":"claude-opus-4-8",
            "usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5,
            "cache_creation_input_tokens":55000,
            "cache_creation":{"ephemeral_1h_input_tokens":100,"ephemeral_5m_input_tokens":200},
            "server_tool_use":{"web_search_requests":1,"web_fetch_requests":0}},
            "content":[{"type":"text","text":"hi"},
            {"type":"tool_use","name":"Read","input":{"file_path":"C:\\a\\report.xlsx"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert_eq!(evs.len(), 2, "one AssistantTurn + one ToolCall");

        match &evs[0].kind {
            EventKind::AssistantTurn { usage, web_search, .. } => {
                assert_eq!(usage.cache_creation, 55000);
                assert_eq!(usage.eph_1h, 100);
                assert_eq!(*web_search, 1);
            }
            k => panic!("expected AssistantTurn, got {k:?}"),
        }
        match &evs[1].kind {
            EventKind::ToolCall { kind, target, .. } => {
                assert_eq!(*kind, ToolKind::FileRead);
                assert_eq!(target.as_deref(), Some("C:\\a\\report.xlsx"));
            }
            k => panic!("expected ToolCall, got {k:?}"),
        }
        assert_eq!(evs[0].session_id, "s1");
        assert_eq!(evs[0].host, "Windows");
    }

    #[test]
    fn map_never_panics_on_garbage() {
        assert!(adapter().map("not json at all", "x.jsonl", 0).is_empty());
        assert!(adapter().map("{}", "x.jsonl", 0).is_empty());
        assert!(adapter().map(r#"{"type":"summary"}"#, "x.jsonl", 0).is_empty());
    }

    #[test]
    fn read_incremental_returns_only_complete_lines() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        // 완결 라인 2개 + 미완결 tail
        write!(f, "{{\"a\":1}}\n{{\"b\":2}}\n{{\"partial\"").unwrap();
        f.flush().unwrap();

        let (lines, new_off) = adapter().read_incremental(&path, 0).unwrap();
        assert_eq!(lines, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
        // 새 오프셋은 두 완결 라인의 바이트 길이(개행 포함)
        assert_eq!(new_off, ("{\"a\":1}\n{\"b\":2}\n").len() as u64);

        // 같은 오프셋에서 다시 읽으면 새 완결 라인 없음
        let (lines2, _) = adapter().read_incremental(&path, new_off).unwrap();
        assert!(lines2.is_empty());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib adapter`
Expected: FAIL — `SourceAdapter`, `ClaudeCodeAdapter` 미정의.

- [ ] **Step 3: 최소 구현 작성**

`src/adapter.rs` 상단에:

```rust
use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage, ToolKind};
use anyhow::Result;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub trait SourceAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>>;
    fn read_incremental(&self, file: &Path, from_offset: u64) -> Result<(Vec<String>, u64)>;
    fn map(&self, line: &str, source_file: &str, source_offset: u64) -> Vec<NormalizedEvent>;
}

pub struct ClaudeCodeAdapter {
    pub root: PathBuf,
    pub host: String,
}

impl ClaudeCodeAdapter {
    /// Windows %USERPROFILE%\.claude 를 기본 루트로.
    pub fn windows() -> ClaudeCodeAdapter {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        ClaudeCodeAdapter {
            root: PathBuf::from(home).join(".claude"),
            host: "Windows".to_string(),
        }
    }
}

/// 디렉터리명/경로를 프로젝트 join 키로 정규화. (Global Constraints의 join 결정)
pub fn decode_project_id(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' => '-',
            other => other,
        })
        .collect()
}

fn as_u64(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

impl SourceAdapter for ClaudeCodeAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>> {
        let projects = self.root.join("projects");
        let mut out = Vec::new();
        if !projects.is_dir() {
            return Ok(out);
        }
        for proj in std::fs::read_dir(&projects)? {
            let proj = proj?.path();
            if !proj.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&proj)? {
                let f = f?.path();
                if f.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    out.push(f);
                }
            }
        }
        Ok(out)
    }

    fn read_incremental(&self, file: &Path, from_offset: u64) -> Result<(Vec<String>, u64)> {
        let mut f = std::fs::File::open(file)?;
        f.seek(SeekFrom::Start(from_offset))?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;

        let mut lines = Vec::new();
        let mut consumed = 0u64; // 완결 라인들이 차지한 바이트 수
        for segment in buf.split_inclusive('\n') {
            if segment.ends_with('\n') {
                consumed += segment.len() as u64;
                let trimmed = segment.trim_end_matches(['\n', '\r']);
                if !trimmed.is_empty() {
                    lines.push(trimmed.to_string());
                }
            }
            // 개행으로 끝나지 않는 마지막 tail은 버림(미완결) → consumed에 미포함
        }
        Ok((lines, from_offset + consumed))
    }

    fn map(&self, line: &str, source_file: &str, source_offset: u64) -> Vec<NormalizedEvent> {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Vec::new(), // 관대한 파싱: 하드 실패 금지
        };
        let ltype = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let session_id = match v.get("sessionId").and_then(|x| x.as_str()) {
            Some(s) => s.to_string(),
            None => return Vec::new(),
        };

        // project_id: 소스 파일의 부모 디렉터리명에서.
        let project_id = Path::new(source_file)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(decode_project_id)
            .unwrap_or_default();

        let mk = |kind: EventKind, off_bump: u64| NormalizedEvent {
            source_agent: "claude-code".to_string(),
            schema_version: v
                .get("version")
                .and_then(|x| x.as_str())
                .unwrap_or("unknown")
                .to_string(),
            host: self.host.clone(),
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            uuid: v.get("uuid").and_then(|x| x.as_str()).map(String::from),
            parent_uuid: v.get("parentUuid").and_then(|x| x.as_str()).map(String::from),
            is_sidechain: v.get("isSidechain").and_then(|x| x.as_bool()).unwrap_or(false),
            ts: v.get("timestamp").and_then(|x| x.as_str()).map(String::from),
            source_file: source_file.to_string(),
            source_offset: source_offset + off_bump,
            kind,
        };

        let mut out = Vec::new();
        if ltype == "assistant" {
            let msg = v.get("message").cloned().unwrap_or(Value::Null);
            let usage = msg.get("usage").cloned().unwrap_or(Value::Null);
            let cc = usage.get("cache_creation").cloned().unwrap_or(Value::Null);
            let stu = usage.get("server_tool_use").cloned().unwrap_or(Value::Null);
            let model = NormModel::from_raw_id(
                msg.get("model").and_then(|x| x.as_str()).unwrap_or("unknown"),
            );
            let tu = TokenUsage {
                input: as_u64(&usage, "input_tokens"),
                output: as_u64(&usage, "output_tokens"),
                cache_read: as_u64(&usage, "cache_read_input_tokens"),
                cache_creation: as_u64(&usage, "cache_creation_input_tokens"),
                eph_1h: as_u64(&cc, "ephemeral_1h_input_tokens"),
                eph_5m: as_u64(&cc, "ephemeral_5m_input_tokens"),
            };
            out.push(mk(
                EventKind::AssistantTurn {
                    model,
                    usage: tu,
                    web_search: as_u64(&stu, "web_search_requests") as u32,
                    web_fetch: as_u64(&stu, "web_fetch_requests") as u32,
                },
                0,
            ));

            // tool_use 블록 → ToolCall
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for (i, block) in content.iter().enumerate() {
                    if block.get("type").and_then(|x| x.as_str()) == Some("tool_use") {
                        let raw_name =
                            block.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").cloned().unwrap_or(Value::Null);
                        let target = input
                            .get("file_path")
                            .or_else(|| input.get("command"))
                            .and_then(|x| x.as_str())
                            .map(String::from);
                        out.push(mk(
                            EventKind::ToolCall {
                                kind: ToolKind::from_raw_name(&raw_name),
                                raw_name,
                                target,
                            },
                            (i + 1) as u64,
                        ));
                    }
                }
            }
        }
        out
    }
}
```

> 주의: 이 파일은 `tempfile`을 테스트에서 쓴다. `tempfile`은 이미 `[dev-dependencies]`에 있으므로 추가 작업 불필요.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib adapter`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/adapter.rs
git commit -m "feat: add ClaudeCodeAdapter with lenient parsing and incremental read"
```

---

## Task 3: SqliteStore (스키마 + 이벤트 upsert + 인벤토리 + ingest_state)

**Files:**
- Create/replace: `src/store.rs`

**Interfaces:**
- Consumes: `model::{NormalizedEvent, EventKind, ToolKind, ModelFamily, ModelTier}` (Task 1)
- Produces:
  - `pub struct SqliteStore { conn: rusqlite::Connection }`
  - `SqliteStore::open(path:&Path) -> Result<SqliteStore>` (스키마 생성 포함), `SqliteStore::open_in_memory() -> Result<SqliteStore>`
  - `fn upsert_events(&self, evs:&[NormalizedEvent]) -> Result<usize>` (멱등, dedup_key 기준 INSERT OR IGNORE, 삽입 개수 반환)
  - `fn get_offset(&self, source_file:&str) -> Result<u64>`, `fn set_offset(&self, source_file:&str, offset:u64) -> Result<()>`
  - `fn count_events(&self) -> Result<u64>` (테스트/CLI용)
  - dedup_key = event uuid, 없으면 `format!("{source_file}:{source_offset}")`.

- [ ] **Step 1: 실패하는 테스트 작성 — 멱등 upsert + offset 왕복**

`src/store.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn turn(session: &str, uuid: &str, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "test".into(),
            host: "Windows".into(),
            project_id: "c--users-jibin".into(),
            session_id: session.into(),
            uuid: Some(uuid.into()),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { cache_creation: cache_create, ..Default::default() },
                web_search: 0,
                web_fetch: 0,
            },
        }
    }

    #[test]
    fn upsert_is_idempotent_by_dedup_key() {
        let store = SqliteStore::open_in_memory().unwrap();
        let evs = vec![turn("s1", "u1", 55000), turn("s1", "u2", 0)];
        let n1 = store.upsert_events(&evs).unwrap();
        assert_eq!(n1, 2);
        // 같은 이벤트 재삽입 → 0개 신규
        let n2 = store.upsert_events(&evs).unwrap();
        assert_eq!(n2, 0);
        assert_eq!(store.count_events().unwrap(), 2);
    }

    #[test]
    fn offset_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 0);
        store.set_offset("f.jsonl", 4096).unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 4096);
        store.set_offset("f.jsonl", 8192).unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 8192);
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib store`
Expected: FAIL — `SqliteStore` 미정의.

- [ ] **Step 3: 최소 구현 작성 (스키마 + upsert + offset)**

`src/store.rs` 상단에:

```rust
use crate::model::{EventKind, NormalizedEvent, ToolKind};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY, host TEXT, project_id TEXT, agent TEXT,
  first_ts TEXT, last_ts TEXT, git_branch TEXT
);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  session_id TEXT NOT NULL, host TEXT NOT NULL, project_id TEXT NOT NULL,
  ts TEXT, source_offset INTEGER NOT NULL, kind TEXT NOT NULL,
  model_family TEXT, model_tier TEXT,
  tok_input INTEGER DEFAULT 0, tok_output INTEGER DEFAULT 0,
  tok_cache_read INTEGER DEFAULT 0, tok_cache_create INTEGER DEFAULT 0,
  tok_eph_1h INTEGER DEFAULT 0, web_search INTEGER DEFAULT 0, web_fetch INTEGER DEFAULT 0,
  tool_kind TEXT, tool_server TEXT, tool_tool TEXT, tool_target TEXT, raw_name TEXT,
  is_sidechain INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS daily_rollup (
  host TEXT NOT NULL, project_id TEXT NOT NULL, date TEXT NOT NULL,
  tok_input INTEGER DEFAULT 0, tok_output INTEGER DEFAULT 0,
  tok_cache_read INTEGER DEFAULT 0, tok_cache_create INTEGER DEFAULT 0,
  session_count INTEGER DEFAULT 0,
  PRIMARY KEY (host, project_id, date)
);
CREATE TABLE IF NOT EXISTS findings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  rule_id TEXT NOT NULL, severity TEXT NOT NULL,
  scope_host TEXT, scope_project TEXT, scope_kind TEXT, scope_ref TEXT,
  evidence_json TEXT NOT NULL, est_tokens_saved INTEGER DEFAULT 0,
  prescription_json TEXT, status TEXT NOT NULL DEFAULT 'new',
  first_seen TEXT, last_seen TEXT, occurrences INTEGER DEFAULT 1
);
CREATE TABLE IF NOT EXISTS mcp_inventory (
  host TEXT NOT NULL, project_id TEXT NOT NULL, server TEXT NOT NULL,
  source TEXT NOT NULL, tool_count INTEGER, est_def_tokens INTEGER,
  probed_at TEXT, last_used_ts TEXT,
  PRIMARY KEY (host, project_id, server)
);
CREATE TABLE IF NOT EXISTS diary_index (
  date TEXT NOT NULL, scope TEXT NOT NULL, path TEXT NOT NULL,
  tokens_used INTEGER DEFAULT 0, engine TEXT,
  PRIMARY KEY (date, scope)
);
CREATE TABLE IF NOT EXISTS ingest_state (
  source_file TEXT PRIMARY KEY, last_offset INTEGER NOT NULL DEFAULT 0, last_mtime INTEGER
);
"#;

pub struct SqliteStore {
    pub conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<SqliteStore> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn })
    }

    pub fn open_in_memory() -> Result<SqliteStore> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn })
    }

    pub fn upsert_events(&self, evs: &[NormalizedEvent]) -> Result<usize> {
        let mut inserted = 0usize;
        for e in evs {
            let dedup_key = e
                .uuid
                .clone()
                .unwrap_or_else(|| format!("{}:{}", e.source_file, e.source_offset));

            // 봉투 공통 + kind별 컬럼 추출
            let (kind_str, mfam, mtier, ti, to, tcr, tcc, e1h, ws, wf,
                 tkind, tsrv, ttool, ttarget, raw) = flatten(e);

            let n = self.conn.execute(
                "INSERT OR IGNORE INTO events
                 (dedup_key, session_id, host, project_id, ts, source_offset, kind,
                  model_family, model_tier, tok_input, tok_output, tok_cache_read,
                  tok_cache_create, tok_eph_1h, web_search, web_fetch,
                  tool_kind, tool_server, tool_tool, tool_target, raw_name, is_sidechain)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
                params![
                    dedup_key, e.session_id, e.host, e.project_id, e.ts, e.source_offset, kind_str,
                    mfam, mtier, ti, to, tcr, tcc, e1h, ws, wf,
                    tkind, tsrv, ttool, ttarget, raw, e.is_sidechain as i64
                ],
            )?;
            inserted += n;

            // sessions 갱신 (첫/마지막 ts)
            self.conn.execute(
                "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch)
                 VALUES (?1,?2,?3,'claude-code',?4,?4,NULL)
                 ON CONFLICT(session_id) DO UPDATE SET
                   last_ts = MAX(COALESCE(last_ts, ?4), ?4),
                   first_ts = MIN(COALESCE(first_ts, ?4), ?4)",
                params![e.session_id, e.host, e.project_id, e.ts],
            )?;
        }
        Ok(inserted)
    }

    pub fn get_offset(&self, source_file: &str) -> Result<u64> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_offset FROM ingest_state WHERE source_file = ?1",
                params![source_file],
                |r| r.get(0),
            )
            .ok();
        Ok(v.unwrap_or(0) as u64)
    }

    pub fn set_offset(&self, source_file: &str, offset: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ingest_state (source_file, last_offset) VALUES (?1, ?2)
             ON CONFLICT(source_file) DO UPDATE SET last_offset = ?2",
            params![source_file, offset as i64],
        )?;
        Ok(())
    }

    pub fn count_events(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        Ok(n as u64)
    }
}

type FlatRow = (
    String, Option<String>, Option<String>, i64, i64, i64, i64, i64, i64, i64,
    Option<String>, Option<String>, Option<String>, Option<String>, Option<String>,
);

fn flatten(e: &NormalizedEvent) -> FlatRow {
    match &e.kind {
        EventKind::AssistantTurn { model, usage, web_search, web_fetch } => (
            "assistant_turn".into(),
            Some(format!("{:?}", model.family).to_lowercase()),
            Some(format!("{:?}", model.tier).to_lowercase()),
            usage.input as i64, usage.output as i64, usage.cache_read as i64,
            usage.cache_creation as i64, usage.eph_1h as i64,
            *web_search as i64, *web_fetch as i64,
            None, None, None, None, None,
        ),
        EventKind::ToolCall { kind, raw_name, target } => {
            let (tkind, tsrv, ttool) = match kind {
                ToolKind::McpCall { server, tool } => (
                    "mcp_call".to_string(),
                    Some(server.clone()),
                    Some(tool.clone()),
                ),
                other => (tool_kind_str(other).to_string(), None, None),
            };
            (
                "tool_call".into(), None, None, 0, 0, 0, 0, 0, 0, 0,
                Some(tkind), tsrv, ttool, target.clone(), Some(raw_name.clone()),
            )
        }
        EventKind::SessionMeta { .. } => (
            "session_meta".into(), None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, None, None,
        ),
    }
}

fn tool_kind_str(k: &ToolKind) -> &'static str {
    match k {
        ToolKind::FileRead => "file_read",
        ToolKind::FileEdit => "file_edit",
        ToolKind::FileWrite => "file_write",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::McpCall { .. } => "mcp_call",
        ToolKind::WebSearch => "web_search",
        ToolKind::WebFetch => "web_fetch",
        ToolKind::SubAgent => "sub_agent",
        ToolKind::Other(_) => "other",
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib store`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/store.rs
git commit -m "feat: add SqliteStore with schema, idempotent event upsert, and ingest_state"
```

---

## Task 4: 배치 수집 + daily_rollup 재구축

**Files:**
- Modify: `src/store.rs` (rollup 메서드 + rollup 조회 추가)
- Modify: `src/adapter.rs` 아님 — 수집 오케스트레이션은 `store.rs`에 함수로 두거나 별도. 여기서는 `src/store.rs`에 `ingest_file` 헬퍼를 추가한다.

**Interfaces:**
- Consumes: `SqliteStore`(Task 3), `SourceAdapter`(Task 2)
- Produces:
  - `impl SqliteStore { pub fn rebuild_rollup(&self) -> Result<()>; pub fn rollup_for(&self, host:&str, project_id:&str, date:&str) -> Result<Option<RollupRow>>; }`
  - `pub struct RollupRow { pub tok_input:u64, pub tok_output:u64, pub tok_cache_read:u64, pub tok_cache_create:u64, pub session_count:u64 }`
  - `pub fn ingest_file(store:&SqliteStore, adapter:&dyn SourceAdapter, file:&Path) -> Result<usize>` — offset부터 읽어 map→upsert→set_offset. 반환값=신규 이벤트 수. (rollup은 호출자가 배치 끝에 `rebuild_rollup` 1회.)

- [ ] **Step 1: 실패하는 테스트 작성 — 파일 수집 후 rollup 집계**

`src/store.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn ingest_file_then_rollup_aggregates_tokens() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        // 트랜스크립트 디렉터리명이 project_id의 원천이므로 하위 디렉터리에 배치
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("s1.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        let line1 = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","timestamp":"2026-07-01T10:00:00Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":55000}}}"#;
        let line2 = r#"{"type":"assistant","sessionId":"s1","uuid":"u2","timestamp":"2026-07-01T10:05:00Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":5,"output_tokens":7}}}"#;
        writeln!(f, "{line1}").unwrap();
        writeln!(f, "{line2}").unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let n = ingest_file(&store, &adapter, &file).unwrap();
        assert_eq!(n, 2);

        // 재수집(offset 저장됨) → 신규 0
        assert_eq!(ingest_file(&store, &adapter, &file).unwrap(), 0);

        store.rebuild_rollup().unwrap();
        let r = store.rollup_for("Windows", "c--users-jibin", "2026-07-01").unwrap().unwrap();
        assert_eq!(r.tok_input, 15);
        assert_eq!(r.tok_output, 27);
        assert_eq!(r.tok_cache_create, 55000);
        assert_eq!(r.session_count, 1);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib store::tests::ingest_file_then_rollup`
Expected: FAIL — `ingest_file`, `rebuild_rollup`, `rollup_for` 미정의.

- [ ] **Step 3: 최소 구현 작성**

`src/store.rs`의 `impl SqliteStore` 안에 추가:

```rust
    pub fn rebuild_rollup(&self) -> Result<()> {
        self.conn.execute("DELETE FROM daily_rollup", [])?;
        self.conn.execute(
            "INSERT INTO daily_rollup
                (host, project_id, date, tok_input, tok_output, tok_cache_read,
                 tok_cache_create, session_count)
             SELECT host, project_id, date(ts) AS d,
                    SUM(tok_input), SUM(tok_output), SUM(tok_cache_read),
                    SUM(tok_cache_create), COUNT(DISTINCT session_id)
             FROM events
             WHERE ts IS NOT NULL
             GROUP BY host, project_id, d",
            [],
        )?;
        Ok(())
    }

    pub fn rollup_for(&self, host: &str, project_id: &str, date: &str) -> Result<Option<RollupRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT tok_input, tok_output, tok_cache_read, tok_cache_create, session_count
                 FROM daily_rollup WHERE host=?1 AND project_id=?2 AND date=?3",
                params![host, project_id, date],
                |r| {
                    Ok(RollupRow {
                        tok_input: r.get::<_, i64>(0)? as u64,
                        tok_output: r.get::<_, i64>(1)? as u64,
                        tok_cache_read: r.get::<_, i64>(2)? as u64,
                        tok_cache_create: r.get::<_, i64>(3)? as u64,
                        session_count: r.get::<_, i64>(4)? as u64,
                    })
                },
            )
            .ok();
        Ok(row)
    }
```

`src/store.rs` 파일 하단(`mod tests` 밖, 최상위)에 추가:

```rust
#[derive(Debug, Clone)]
pub struct RollupRow {
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
    pub session_count: u64,
}

/// 한 파일을 offset부터 증분 수집. 반환값 = 신규 삽입 이벤트 수.
pub fn ingest_file(
    store: &SqliteStore,
    adapter: &dyn crate::adapter::SourceAdapter,
    file: &Path,
) -> Result<usize> {
    let file_key = file.to_string_lossy().to_string();
    let from = store.get_offset(&file_key)?;
    let (lines, new_offset) = adapter.read_incremental(file, from)?;

    let mut all = Vec::new();
    let mut off = from;
    for line in &lines {
        let evs = adapter.map(line, &file_key, off);
        off += line.len() as u64 + 1; // 개행 1바이트 근사(정렬용, dedup은 uuid 기준)
        all.extend(evs);
    }
    let inserted = store.upsert_events(&all)?;
    store.set_offset(&file_key, new_offset)?;
    Ok(inserted)
}
```

> `use std::path::Path;`가 이미 파일 상단에 있으므로 추가 import 불필요.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib store`
Expected: PASS (3 tests: 기존 2 + 신규 1).

- [ ] **Step 5: Commit**

```bash
git add src/store.rs
git commit -m "feat: add batch ingest_file and daily_rollup rebuild"
```

---

## Task 5: Finding 모델 + Rule trait + RuleEngine

**Files:**
- Create/replace: `src/finding.rs`
- Create/replace: `src/rules/mod.rs`

**Interfaces:**
- Consumes: `store::SqliteStore`
- Produces:
  - `pub enum Severity { Info, Suggest, Warn }` (+ `as_str`)
  - `pub struct Prescription { pub kind:String, pub payload:serde_json::Value }`
  - `pub struct Finding { pub rule_id:String, pub severity:Severity, pub scope_host:Option<String>, pub scope_project:Option<String>, pub scope_kind:String, pub scope_ref:String, pub evidence:serde_json::Value, pub est_tokens_saved:u64, pub prescription:Option<Prescription>, pub dedup_key:String }`
  - `pub trait Rule { fn id(&self) -> &'static str; fn evaluate(&self, store:&SqliteStore) -> anyhow::Result<Vec<Finding>>; }`
  - `pub struct RuleEngine { rules: Vec<Box<dyn Rule>> }` + `RuleEngine::new(rules)`, `fn run(&self, store:&SqliteStore) -> Result<Vec<Finding>>`
  - `impl SqliteStore { pub fn upsert_finding(&self, f:&Finding, now_ts:&str) -> Result<()>; pub fn count_findings(&self) -> Result<u64>; pub fn findings_for_date(&self, date:&str) -> Result<Vec<Finding>>; }` (store.rs에 추가)
  - `pub fn normalize_project_key(s:&str) -> String` (rules/mod.rs; = adapter::decode_project_id 재사용)

- [ ] **Step 1: 실패하는 테스트 작성 — Finding upsert 멱등 + occurrences 증가**

`src/finding.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;

    fn sample() -> Finding {
        Finding {
            rule_id: "R5".into(),
            severity: Severity::Suggest,
            scope_host: Some("Windows".into()),
            scope_project: Some("c--users-jibin".into()),
            scope_kind: "session".into(),
            scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"report.xlsx","count":7}),
            est_tokens_saved: 7200,
            prescription: None,
            dedup_key: "R5|s1|report.xlsx".into(),
        }
    }

    #[test]
    fn finding_upsert_is_idempotent_and_counts_occurrences() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&sample(), "2026-07-01T10:00:00Z").unwrap();
        store.upsert_finding(&sample(), "2026-07-01T11:00:00Z").unwrap();
        assert_eq!(store.count_findings().unwrap(), 1);

        let got = store.findings_for_date("2026-07-01").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].est_tokens_saved, 7200);
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib finding`
Expected: FAIL — `Finding`, `upsert_finding` 미정의.

- [ ] **Step 3: `src/finding.rs` 구현**

`src/finding.rs` 상단에:

```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity { Info, Suggest, Warn }

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Suggest => "suggest",
            Severity::Warn => "warn",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Prescription {
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub scope_host: Option<String>,
    pub scope_project: Option<String>,
    pub scope_kind: String,
    pub scope_ref: String,
    pub evidence: Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<Prescription>,
    pub dedup_key: String,
}
```

- [ ] **Step 4: `SqliteStore`에 Finding 메서드 추가**

`src/store.rs` 상단 import에 추가: `use crate::finding::{Finding, Prescription, Severity};` — 단, `Severity`는 `as_str`만 쓰므로 `use crate::finding::Finding;`만으로 충분하면 그걸로. 아래 코드는 `Finding`만 사용.

`src/store.rs`의 `impl SqliteStore` 안에 추가:

```rust
    pub fn upsert_finding(&self, f: &Finding, now_ts: &str) -> Result<()> {
        let evidence = serde_json::to_string(&f.evidence)?;
        let presc = match &f.prescription {
            Some(p) => Some(serde_json::to_string(p)?),
            None => None,
        };
        self.conn.execute(
            "INSERT INTO findings
                (dedup_key, rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est_tokens_saved, prescription_json, status,
                 first_seen, last_seen, occurrences)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'new',?11,?11,1)
             ON CONFLICT(dedup_key) DO UPDATE SET
                last_seen = ?11,
                occurrences = occurrences + 1,
                est_tokens_saved = ?9,
                evidence_json = ?8",
            params![
                f.dedup_key, f.rule_id, f.severity.as_str(), f.scope_host, f.scope_project,
                f.scope_kind, f.scope_ref, evidence, f.est_tokens_saved as i64, presc, now_ts
            ],
        )?;
        Ok(())
    }

    pub fn count_findings(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM findings", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// last_seen 날짜가 주어진 날짜인 Finding들. (다이어리 브리프 재료)
    pub fn findings_for_date(&self, date: &str) -> Result<Vec<Finding>> {
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key
             FROM findings WHERE date(last_seen) = ?1
             ORDER BY est_tokens_saved DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            let sev = match r.get::<_, String>(1)?.as_str() {
                "warn" => Severity::Warn,
                "suggest" => Severity::Suggest,
                _ => Severity::Info,
            };
            let evidence: serde_json::Value =
                serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or(serde_json::Value::Null);
            let presc: Option<Prescription> = r
                .get::<_, Option<String>>(8)?
                .and_then(|s| serde_json::from_str(&s).ok());
            Ok(Finding {
                rule_id: r.get(0)?,
                severity: sev,
                scope_host: r.get(2)?,
                scope_project: r.get(3)?,
                scope_kind: r.get(4)?,
                scope_ref: r.get(5)?,
                evidence,
                est_tokens_saved: r.get::<_, i64>(7)? as u64,
                prescription: presc,
                dedup_key: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
```

> `src/store.rs` 상단에 `use crate::finding::{Finding, Prescription, Severity};` 추가.

- [ ] **Step 5: `src/rules/mod.rs` 구현 (Rule trait + RuleEngine)**

`src/rules/mod.rs`를 다음으로 교체(기존 `pub mod` 선언 유지):

```rust
pub mod r1_unused_mcp;
pub mod r5_repeated_read;

use crate::finding::Finding;
use crate::store::SqliteStore;
use anyhow::Result;

pub trait Rule {
    fn id(&self) -> &'static str;
    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>>;
}

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleEngine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> RuleEngine {
        RuleEngine { rules }
    }

    pub fn run(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut out = Vec::new();
        for rule in &self.rules {
            out.extend(rule.evaluate(store)?);
        }
        Ok(out)
    }
}

/// events(project_id)와 인벤토리(claude.json 경로)를 잇는 v0 정규화 키.
pub fn normalize_project_key(s: &str) -> String {
    crate::adapter::decode_project_id(s)
}
```

- [ ] **Step 6: 테스트 통과 확인**

Run: `cargo test --lib finding`
Expected: PASS (1 test).
Run: `cargo build` — Expected: OK (RuleEngine 컴파일, r1/r5는 아직 placeholder지만 `pub mod` 선언만 있으면 빈 파일이라 OK).

- [ ] **Step 7: Commit**

```bash
git add src/finding.rs src/store.rs src/rules/mod.rs
git commit -m "feat: add Finding model, Rule trait, RuleEngine, and finding persistence"
```

---

## Task 6: R5 — 같은 파일 반복 Read 규칙

**Files:**
- Create/replace: `src/rules/r5_repeated_read.rs`

**Interfaces:**
- Consumes: `Rule`, `Finding`, `Severity`, `SqliteStore`
- Produces: `pub struct R5RepeatedRead { pub threshold:u64, pub heuristic_tokens_per_read:u64 }` + `Default`(threshold=4, heuristic=1200) + `impl Rule`
  - 세션 스코프. 같은 `tool_target`을 `tool_kind='file_read'`로 `threshold`회 이상 읽은 (session, target) → Finding.
  - `est_tokens_saved = (count-1) * heuristic_tokens_per_read`. severity=Suggest. prescription=None.
  - dedup_key = `format!("R5|{session}|{target}")`.

- [ ] **Step 1: 실패하는 테스트 작성**

`src/rules/r5_repeated_read.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::rules::Rule;
    use crate::store::SqliteStore;

    fn read_event(session: &str, uuid: &str, path: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "test".into(),
            host: "Windows".into(),
            project_id: "c--users-jibin".into(),
            session_id: session.into(),
            uuid: Some(uuid.into()),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::FileRead,
                raw_name: "Read".into(),
                target: Some(path.into()),
            },
        }
    }

    #[test]
    fn r5_flags_file_read_4_or_more_times() {
        let store = SqliteStore::open_in_memory().unwrap();
        let evs: Vec<_> = (0..5)
            .map(|i| read_event("s1", &format!("u{i}"), "C:\\a\\report.xlsx"))
            .chain(std::iter::once(read_event("s1", "u9", "C:\\a\\once.txt")))
            .collect();
        store.upsert_events(&evs).unwrap();

        let findings = R5RepeatedRead::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "only report.xlsx (x5) crosses threshold");
        let f = &findings[0];
        assert_eq!(f.rule_id, "R5");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.est_tokens_saved, 4 * 1200); // (5-1)*1200
        assert_eq!(f.evidence["path"], "C:\\a\\report.xlsx");
        assert_eq!(f.evidence["count"], 5);
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib r5`
Expected: FAIL — `R5RepeatedRead` 미정의.

- [ ] **Step 3: 구현**

`src/rules/r5_repeated_read.rs` 상단에:

```rust
use crate::finding::{Finding, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

pub struct R5RepeatedRead {
    pub threshold: u64,
    pub heuristic_tokens_per_read: u64,
}

impl Default for R5RepeatedRead {
    fn default() -> Self {
        R5RepeatedRead { threshold: 4, heuristic_tokens_per_read: 1200 }
    }
}

impl Rule for R5RepeatedRead {
    fn id(&self) -> &'static str {
        "R5"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id, tool_target, COUNT(*) AS c
             FROM events
             WHERE kind='tool_call' AND tool_kind='file_read' AND tool_target IS NOT NULL
             GROUP BY session_id, tool_target
             HAVING c >= ?1
             ORDER BY c DESC",
        )?;
        let rows = stmt.query_map(params![self.threshold as i64], |r| {
            Ok((
                r.get::<_, String>(0)?, // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(), // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(), // project
                r.get::<_, String>(3)?, // target
                r.get::<_, i64>(4)? as u64, // count
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, target, count) = row?;
            out.push(Finding {
                rule_id: "R5".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({ "path": target, "count": count }),
                est_tokens_saved: (count - 1) * self.heuristic_tokens_per_read,
                prescription: None,
                dedup_key: format!("R5|{session}|{target}"),
            });
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib r5`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add src/rules/r5_repeated_read.rs
git commit -m "feat: add R5 repeated-read rule"
```

---

## Task 7: MCP 인벤토리 파싱

**Files:**
- Create/replace: `src/inventory.rs`
- Modify: `src/store.rs` (인벤토리 upsert + 조회)
- Modify: `src/lib.rs` 아님 (이미 `pub mod inventory;` 있음)

**Interfaces:**
- Consumes: `SqliteStore`, `rules::normalize_project_key`
- Produces:
  - `pub struct McpServer { pub name:String, pub source:String }`
  - `pub fn parse_claude_json(json:&serde_json::Value) -> Vec<(String, Vec<McpServer>)>` — `projects.<path>` 별로 (정규화된 project_key, servers). servers = `mcpServers` 키 ∪ `enabledMcpjsonServers` 배열.
  - `impl SqliteStore { pub fn upsert_inventory(&self, host:&str, project_id:&str, servers:&[McpServer]) -> Result<()>; pub fn active_servers(&self, host:&str, project_id:&str) -> Result<Vec<String>>; }`

- [ ] **Step 1: 실패하는 테스트 작성 — claude.json 파싱**

`src/inventory.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_json_extracts_servers_per_project() {
        let json = serde_json::json!({
            "projects": {
                "C:\\Users\\jibin": {
                    "mcpServers": { "context7": {}, "playwright": {} },
                    "enabledMcpjsonServers": ["vercel"]
                },
                "D:\\Project\\agent-mentor": {
                    "mcpServers": {}
                }
            }
        });
        let mut parsed = parse_claude_json(&json);
        parsed.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(parsed[0].0, "c--users-jibin");
        let mut names: Vec<_> = parsed[0].1.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "playwright", "vercel"]);

        assert_eq!(parsed[1].0, "d--project-agent-mentor");
        assert!(parsed[1].1.is_empty());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib inventory`
Expected: FAIL — `parse_claude_json` 미정의.

- [ ] **Step 3: `src/inventory.rs` 구현**

```rust
use crate::rules::normalize_project_key;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub source: String, // "project" | "global"
}

/// ~/.claude.json 을 받아 프로젝트별 활성 MCP 서버 목록을 반환.
/// 관대한 파싱: 없는 키/타입 불일치는 무시.
pub fn parse_claude_json(json: &Value) -> Vec<(String, Vec<McpServer>)> {
    let mut out = Vec::new();
    let Some(projects) = json.get("projects").and_then(|p| p.as_object()) else {
        return out;
    };
    for (path, entry) in projects {
        let key = normalize_project_key(path);
        let mut servers = Vec::new();

        if let Some(map) = entry.get("mcpServers").and_then(|m| m.as_object()) {
            for name in map.keys() {
                servers.push(McpServer { name: name.clone(), source: "project".into() });
            }
        }
        if let Some(arr) = entry.get("enabledMcpjsonServers").and_then(|a| a.as_array()) {
            for name in arr.iter().filter_map(|v| v.as_str()) {
                if !servers.iter().any(|s| s.name == name) {
                    servers.push(McpServer { name: name.to_string(), source: "project".into() });
                }
            }
        }
        out.push((key, servers));
    }
    out
}
```

- [ ] **Step 4: `SqliteStore`에 인벤토리 메서드 추가**

`src/store.rs`의 `impl SqliteStore` 안에 추가:

```rust
    pub fn upsert_inventory(
        &self,
        host: &str,
        project_id: &str,
        servers: &[crate::inventory::McpServer],
    ) -> Result<()> {
        for s in servers {
            self.conn.execute(
                "INSERT INTO mcp_inventory (host, project_id, server, source)
                 VALUES (?1,?2,?3,?4)
                 ON CONFLICT(host, project_id, server) DO UPDATE SET source = ?4",
                params![host, project_id, s.name, s.source],
            )?;
        }
        Ok(())
    }

    pub fn active_servers(&self, host: &str, project_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT server FROM mcp_inventory WHERE host=?1 AND project_id=?2 ORDER BY server",
        )?;
        let rows = stmt.query_map(params![host, project_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test --lib inventory`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
git add src/inventory.rs src/store.rs
git commit -m "feat: add MCP inventory parsing and persistence"
```

---

## Task 8: R1 — 안 쓰는 always-on MCP 규칙

**Files:**
- Create/replace: `src/rules/r1_unused_mcp.rs`

**Interfaces:**
- Consumes: `Rule`, `Finding`, `Severity`, `Prescription`, `SqliteStore`
- Produces: `pub struct R1UnusedMcp { pub min_resident_tokens:u64, pub heuristic_tokens_per_server:u64 }` + `Default`(min_resident=2000, heuristic=2500) + `impl Rule`
  - 각 (host, project)의 인벤토리 서버 중, 해당 project 이벤트에서 `tool_kind='mcp_call' AND tool_server=server` 카운트 0인 서버.
  - 단, 그 project의 대표 always-on(첫 턴 `tok_cache_create` 최댓값)이 `min_resident_tokens` 초과일 때만(의미 있는 상주 비용 존재).
  - severity=Warn, prescription={kind:"remove_mcp", payload:{server}}, est_tokens_saved = heuristic_tokens_per_server (라벨 "약(~)").
  - dedup_key = `format!("R1|{project}|{server}")`.
  - **유예(스펙 §8):** 서버별 정확 귀속(옵트인 프로브)은 다음 슬라이스. v0는 flat heuristic.

- [ ] **Step 1: 실패하는 테스트 작성**

`src/rules/r1_unused_mcp.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::McpServer;
    use crate::model::*;
    use crate::rules::Rule;
    use crate::store::SqliteStore;

    fn first_turn(project: &str, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: project.into(), session_id: "s1".into(),
            uuid: Some("u_turn".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { cache_creation: cache_create, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn mcp_call(project: &str, uuid: &str, server: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: project.into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:01:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "x".into() },
                raw_name: format!("mcp__{server}__x"), target: None,
            },
        }
    }

    #[test]
    fn r1_flags_configured_but_unused_server_with_resident_cost() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "c--users-jibin";
        // 상주 비용 55k, context7만 호출됨 → playwright는 미사용
        store.upsert_events(&[
            first_turn(proj, 55000),
            mcp_call(proj, "m1", "context7"),
        ]).unwrap();
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "context7".into(), source: "project".into() },
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R1");
        assert_eq!(f.evidence["server"], "playwright");
        assert_eq!(f.prescription.as_ref().unwrap().kind, "remove_mcp");
        assert_eq!(f.est_tokens_saved, 2500);
    }

    #[test]
    fn r1_silent_when_no_resident_cost() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "c--users-jibin";
        store.upsert_events(&[first_turn(proj, 100)]).unwrap(); // 상주 비용 미미
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();
        assert!(R1UnusedMcp::default().evaluate(&store).unwrap().is_empty());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib r1`
Expected: FAIL — `R1UnusedMcp` 미정의.

- [ ] **Step 3: 구현**

`src/rules/r1_unused_mcp.rs` 상단에:

```rust
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

pub struct R1UnusedMcp {
    pub min_resident_tokens: u64,
    pub heuristic_tokens_per_server: u64,
}

impl Default for R1UnusedMcp {
    fn default() -> Self {
        R1UnusedMcp { min_resident_tokens: 2000, heuristic_tokens_per_server: 2500 }
    }
}

impl Rule for R1UnusedMcp {
    fn id(&self) -> &'static str {
        "R1"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        // 1) 인벤토리에 등록된 (host, project, server) 전체
        let mut inv_stmt = store
            .conn
            .prepare("SELECT host, project_id, server FROM mcp_inventory")?;
        let inv: Vec<(String, String, String)> = inv_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;

        let mut out = Vec::new();
        for (host, project, server) in inv {
            // 2) 이 서버가 해당 project에서 호출된 횟수
            let calls: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM events
                 WHERE project_id=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                params![project, server],
                |r| r.get(0),
            )?;
            if calls > 0 {
                continue;
            }

            // 3) 해당 project의 대표 상주 비용(첫 턴 cache_create 최댓값)
            let resident: i64 = store.conn.query_row(
                "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                 WHERE project_id=?1 AND kind='assistant_turn'",
                params![project],
                |r| r.get(0),
            )?;
            if (resident as u64) <= self.min_resident_tokens {
                continue;
            }

            out.push(Finding {
                rule_id: "R1".into(),
                severity: Severity::Warn,
                scope_host: Some(host.clone()),
                scope_project: Some(project.clone()),
                scope_kind: "project".into(),
                scope_ref: project.clone(),
                evidence: serde_json::json!({
                    "server": server,
                    "resident_tokens_total": resident,
                    "calls": 0,
                    "note": "약(~) 추정 — 서버별 정확 귀속은 옵트인 프로브(유예)"
                }),
                est_tokens_saved: self.heuristic_tokens_per_server,
                prescription: Some(Prescription {
                    kind: "remove_mcp".into(),
                    payload: serde_json::json!({ "server": server }),
                }),
                dedup_key: format!("R1|{project}|{server}"),
            });
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib r1`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/rules/r1_unused_mcp.rs
git commit -m "feat: add R1 unused always-on MCP rule"
```

---

## Task 9: 다이어리 브리프 조립 (결정적)

**Files:**
- Create/replace: `src/diary/mod.rs` (Brief + 조립 함수; engine은 Task 10)

**Interfaces:**
- Consumes: `SqliteStore`, `finding::Finding`
- Produces:
  - `pub struct Brief { pub date:String, pub host:String, pub totals:BriefTotals, pub findings:Vec<BriefFinding> }` (Serialize)
  - `pub struct BriefTotals { pub tok_input:u64, pub tok_output:u64, pub tok_cache_create:u64, pub session_count:u64 }`
  - `pub struct BriefFinding { pub rule_id:String, pub severity:String, pub evidence:serde_json::Value, pub est_tokens_saved:u64, pub prescription:Option<serde_json::Value> }`
  - `pub fn assemble_brief(store:&SqliteStore, host:&str, date:&str) -> anyhow::Result<Brief>` — 해당 날짜의 findings + 그 host의 date rollup 합산.

- [ ] **Step 1: 실패하는 테스트 작성**

`src/diary/mod.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Finding, Severity};
    use crate::model::*;
    use crate::store::SqliteStore;

    #[test]
    fn assemble_brief_collects_findings_and_totals() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 하루치 이벤트 → rollup
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: 10, output: 20, cache_creation: 55000, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.rebuild_rollup().unwrap();

        store.upsert_finding(&Finding {
            rule_id: "R5".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("c--users-jibin".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"report.xlsx","count":7}),
            est_tokens_saved: 7200, prescription: None,
            dedup_key: "R5|s1|report.xlsx".into(),
        }, "2026-07-01T10:00:00Z").unwrap();

        let brief = assemble_brief(&store, "Windows", "2026-07-01").unwrap();
        assert_eq!(brief.date, "2026-07-01");
        assert_eq!(brief.totals.tok_cache_create, 55000);
        assert_eq!(brief.totals.session_count, 1);
        assert_eq!(brief.findings.len(), 1);
        assert_eq!(brief.findings[0].rule_id, "R5");
        assert_eq!(brief.findings[0].est_tokens_saved, 7200);
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib diary`
Expected: FAIL — `assemble_brief` 미정의.

- [ ] **Step 3: 구현**

`src/diary/mod.rs` 상단(파일 맨 위, `pub mod engine;` 선언 유지)에:

```rust
pub mod engine;

use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct BriefTotals {
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_create: u64,
    pub session_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefFinding {
    pub rule_id: String,
    pub severity: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
}

pub fn assemble_brief(store: &SqliteStore, host: &str, date: &str) -> Result<Brief> {
    // 해당 host+date의 rollup 합산(여러 프로젝트 합)
    let totals = store.conn.query_row(
        "SELECT COALESCE(SUM(tok_input),0), COALESCE(SUM(tok_output),0),
                COALESCE(SUM(tok_cache_create),0), COALESCE(SUM(session_count),0)
         FROM daily_rollup WHERE host=?1 AND date=?2",
        params![host, date],
        |r| {
            Ok(BriefTotals {
                tok_input: r.get::<_, i64>(0)? as u64,
                tok_output: r.get::<_, i64>(1)? as u64,
                tok_cache_create: r.get::<_, i64>(2)? as u64,
                session_count: r.get::<_, i64>(3)? as u64,
            })
        },
    )?;

    let findings = store
        .findings_for_date(date)?
        .into_iter()
        .map(|f| BriefFinding {
            rule_id: f.rule_id,
            severity: f.severity.as_str().to_string(),
            evidence: f.evidence,
            est_tokens_saved: f.est_tokens_saved,
            prescription: f.prescription.map(|p| serde_json::json!({
                "kind": p.kind, "payload": p.payload
            })),
        })
        .collect();

    Ok(Brief { date: date.to_string(), host: host.to_string(), totals, findings })
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib diary`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add src/diary/mod.rs
git commit -m "feat: add deterministic diary brief assembly"
```

---

## Task 10: 엔진 어댑터 (trait + Mock + OpenAI 호환) + 토큰 계량

**Files:**
- Create/replace: `src/diary/engine.rs`

**Interfaces:**
- Consumes: (없음, HTTP는 ureq)
- Produces:
  - `pub struct EngineOutput { pub text:String, pub tokens_used:u64 }`
  - `pub trait Engine { fn name(&self) -> String; fn generate(&self, system:&str, user:&str) -> anyhow::Result<EngineOutput>; }`
  - `pub struct MockEngine { pub canned:String }` (`generate` → canned, tokens_used = system+user 길이/4 근사)
  - `pub struct OpenAiCompatEngine { pub base_url:String, pub api_key:String, pub model:String }` + `OpenAiCompatEngine::from_env() -> Option<OpenAiCompatEngine>` (env `AGENT_MENTOR_ENGINE_URL/KEY/MODEL`)
  - OpenAiCompat: `POST {base_url}/chat/completions`, 응답 `choices[0].message.content` + `usage.total_tokens`.

- [ ] **Step 1: 실패하는 테스트 작성 — MockEngine 계약**

`src/diary/engine.rs` 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_engine_returns_canned_text_and_meters_tokens() {
        let eng = MockEngine { canned: "오늘 주인은 나를 꽤 굴렸다.".into() };
        let out = eng.generate("system prompt", "user brief json").unwrap();
        assert_eq!(out.text, "오늘 주인은 나를 꽤 굴렸다.");
        assert!(out.tokens_used > 0, "mock meters an approximate token count");
        assert_eq!(eng.name(), "mock");
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib engine`
Expected: FAIL — `MockEngine`, `Engine` 미정의.

- [ ] **Step 3: 구현**

`src/diary/engine.rs` 상단에:

```rust
use anyhow::{anyhow, Result};

pub struct EngineOutput {
    pub text: String,
    pub tokens_used: u64,
}

pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;
}

/// 결정적 테스트용. 실제 호출 없이 정해진 텍스트 반환.
pub struct MockEngine {
    pub canned: String,
}

impl Engine for MockEngine {
    fn name(&self) -> String {
        "mock".to_string()
    }
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput> {
        let approx = ((system.len() + user.len() + self.canned.len()) / 4).max(1) as u64;
        Ok(EngineOutput { text: self.canned.clone(), tokens_used: approx })
    }
}

/// 사내 on-prem(OpenAI 호환) 또는 OpenAI API. base_url 예: "https://.../v1"
pub struct OpenAiCompatEngine {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiCompatEngine {
    pub fn from_env() -> Option<OpenAiCompatEngine> {
        let base_url = std::env::var("AGENT_MENTOR_ENGINE_URL").ok()?;
        let api_key = std::env::var("AGENT_MENTOR_ENGINE_KEY").unwrap_or_default();
        let model = std::env::var("AGENT_MENTOR_ENGINE_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());
        Some(OpenAiCompatEngine { base_url, api_key, model })
    }
}

impl Engine for OpenAiCompatEngine {
    fn name(&self) -> String {
        format!("openai-compat:{}", self.model)
    }

    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": 0.7
        });
        let resp = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| anyhow!("engine request failed: {e}"))?;

        let v: serde_json::Value = resp.into_json()?;
        let text = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("engine response missing choices[0].message.content"))?
            .to_string();
        let tokens_used = v
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        Ok(EngineOutput { text, tokens_used })
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib engine`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add src/diary/engine.rs
git commit -m "feat: add Engine trait with MockEngine and OpenAI-compatible engine"
```

---

## Task 11: 다이어리 생성 파이프라인

**Files:**
- Modify: `src/diary/mod.rs` (DiaryConfig, DiaryOutput, 시스템 프롬프트, generate_diary)

**Interfaces:**
- Consumes: `Brief`(Task 9), `Engine`/`EngineOutput`(Task 10), `SqliteStore`
- Produces:
  - `pub struct DiaryConfig { pub vault_dir:PathBuf, pub tone:String, pub honorific:String }` + `Default`(tone="B", honorific="주인", vault_dir="./diary")
  - `pub struct DiaryOutput { pub path:PathBuf, pub tokens_used:u64 }`
  - `pub fn build_system_prompt(cfg:&DiaryConfig) -> String`
  - `pub fn generate_diary(store:&SqliteStore, engine:&dyn Engine, brief:&Brief, cfg:&DiaryConfig) -> anyhow::Result<DiaryOutput>` — 프롬프트 조립 → engine.generate → 푸터에 토큰 계량 → `{vault}/{date}.md` 기록 → `diary_index` upsert.
  - `impl SqliteStore { pub fn upsert_diary_index(&self, date:&str, scope:&str, path:&str, tokens:u64, engine:&str) -> Result<()>; }` (store.rs)

- [ ] **Step 1: 실패하는 테스트 작성 — MockEngine로 파이프라인**

`src/diary/mod.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn generate_diary_writes_md_with_token_footer_and_index() {
        use crate::diary::engine::MockEngine;
        let store = SqliteStore::open_in_memory().unwrap();
        let brief = Brief {
            date: "2026-07-01".into(),
            host: "Windows".into(),
            totals: BriefTotals { tok_cache_create: 55000, session_count: 3, ..Default::default() },
            findings: vec![],
        };
        let tmp = tempfile::tempdir().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            tone: "B".into(),
            honorific: "주인".into(),
        };
        let engine = MockEngine { canned: "오늘 주인은 세 세션을 돌렸다.".into() };

        let out = generate_diary(&store, &engine, &brief, &cfg).unwrap();
        assert_eq!(out.path, tmp.path().join("2026-07-01.md"));
        let content = std::fs::read_to_string(&out.path).unwrap();
        assert!(content.contains("오늘 주인은 세 세션을 돌렸다."));
        assert!(content.contains("토큰"), "footer meters tokens");
        assert!(out.tokens_used > 0);

        // diary_index 기록됨
        let n: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM diary_index WHERE date='2026-07-01'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn system_prompt_injects_tone_and_honorific() {
        let cfg = DiaryConfig::default();
        let p = build_system_prompt(&cfg);
        assert!(p.contains("주인"));
        assert!(p.contains("B"));
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib diary`
Expected: FAIL — `generate_diary`, `DiaryConfig`, `build_system_prompt` 미정의.

- [ ] **Step 3: `SqliteStore`에 diary_index 메서드 추가**

`src/store.rs`의 `impl SqliteStore` 안에 추가:

```rust
    pub fn upsert_diary_index(
        &self,
        date: &str,
        scope: &str,
        path: &str,
        tokens: u64,
        engine: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO diary_index (date, scope, path, tokens_used, engine)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(date, scope) DO UPDATE SET
                path=?3, tokens_used=?4, engine=?5",
            params![date, scope, path, tokens as i64, engine],
        )?;
        Ok(())
    }
```

- [ ] **Step 4: `src/diary/mod.rs`에 파이프라인 구현**

`src/diary/mod.rs` 상단 import에 추가: `use crate::diary::engine::Engine;` 및 `use std::path::PathBuf;`. 그리고 `assemble_brief` 아래에 추가:

```rust
#[derive(Debug, Clone)]
pub struct DiaryConfig {
    pub vault_dir: PathBuf,
    pub tone: String,      // "A" | "B" | "C"
    pub honorific: String, // 기본 "주인"
}

impl Default for DiaryConfig {
    fn default() -> Self {
        DiaryConfig {
            vault_dir: PathBuf::from("./diary"),
            tone: "B".to_string(),
            honorific: "주인".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiaryOutput {
    pub path: PathBuf,
    pub tokens_used: u64,
}

pub fn build_system_prompt(cfg: &DiaryConfig) -> String {
    format!(
        "당신은 사용자의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         1인칭으로 하루를 회고하는 일기를 씁니다. 사용자를 '{honorific}'이라고 부릅니다. \
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)입니다. \
         규칙(정밀도의 선): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. 자유로운 소감은 서사에만 담고 \
         행동 지시로 승격하지 마세요. '잘한 것'과 '아쉬운 것'을 均衡있게 담되 짧게 쓰세요.",
        honorific = cfg.honorific,
        tone = cfg.tone,
    )
}

pub fn generate_diary(
    store: &SqliteStore,
    engine: &dyn Engine,
    brief: &Brief,
    cfg: &DiaryConfig,
) -> Result<DiaryOutput> {
    let system = build_system_prompt(cfg);
    let user = serde_json::to_string_pretty(brief)?;

    let out = engine.generate(&system, &user)?;
    let body = format!(
        "{narrative}\n\n*— 이 일기 ~{tokens} 토큰 (엔진: {engine})*\n",
        narrative = out.text,
        tokens = out.tokens_used,
        engine = engine.name(),
    );

    std::fs::create_dir_all(&cfg.vault_dir)?;
    let path = cfg.vault_dir.join(format!("{}.md", brief.date));
    std::fs::write(&path, body)?;

    store.upsert_diary_index(
        &brief.date,
        &brief.host,
        &path.to_string_lossy(),
        out.tokens_used,
        &engine.name(),
    )?;

    Ok(DiaryOutput { path, tokens_used: out.tokens_used })
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test --lib diary`
Expected: PASS (3 tests: brief 1 + 신규 2).

- [ ] **Step 6: Commit**

```bash
git add src/diary/mod.rs src/store.rs
git commit -m "feat: add diary generation pipeline with token metering and index"
```

---

## Task 12: 검증용 CLI + 실측 데이터 종단 검증

**Files:**
- Create/replace: `src/main.rs`

**Interfaces:**
- Consumes: 모든 이전 태스크의 공개 API.
- Produces: `agent-mentor` 바이너리. 서브커맨드:
  - `ingest` — Windows `~/.claude` discover → 모든 jsonl `ingest_file` → `rebuild_rollup`. 적재 이벤트 수 출력.
  - `inventory` — `~/.claude.json` 파싱 → `upsert_inventory`. 프로젝트/서버 수 출력.
  - `rules` — RuleEngine(R5+R1) 실행 → `upsert_finding` → Finding 요약(rule_id, scope, est_tokens_saved) 출력.
  - `diary [YYYY-MM-DD]` — 오늘(또는 지정일) `assemble_brief` → `generate_diary`(엔진 = `OpenAiCompatEngine::from_env()` 있으면 그것, 없으면 `MockEngine`) → 경로/토큰 출력.
  - `all` — 위 4개 순차 실행.
  - DB 경로: `./agent-mentor.db`.

- [ ] **Step 1: `src/main.rs` 구현**

```rust
use agent_mentor::adapter::{ClaudeCodeAdapter, SourceAdapter};
use agent_mentor::diary::engine::{Engine, MockEngine, OpenAiCompatEngine};
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::inventory::parse_claude_json;
use agent_mentor::rules::r1_unused_mcp::R1UnusedMcp;
use agent_mentor::rules::r5_repeated_read::R5RepeatedRead;
use agent_mentor::rules::RuleEngine;
use agent_mentor::store::{ingest_file, SqliteStore};
use anyhow::Result;
use std::path::PathBuf;

fn db() -> Result<SqliteStore> {
    SqliteStore::open(std::path::Path::new("./agent-mentor.db"))
}

fn cmd_ingest(store: &SqliteStore) -> Result<()> {
    let adapter = ClaudeCodeAdapter::windows();
    let files = adapter.discover()?;
    let mut total = 0usize;
    for f in &files {
        total += ingest_file(store, &adapter, f)?;
    }
    store.rebuild_rollup()?;
    println!("ingested {total} new events from {} files", files.len());
    Ok(())
}

fn cmd_inventory(store: &SqliteStore) -> Result<()> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let path = PathBuf::from(home).join(".claude.json");
    let raw = std::fs::read_to_string(&path)?;
    let json: serde_json::Value = serde_json::from_str(&raw)?;
    let parsed = parse_claude_json(&json);
    let mut servers = 0;
    for (project, srvs) in &parsed {
        servers += srvs.len();
        store.upsert_inventory("Windows", project, srvs)?;
    }
    println!("inventory: {} projects, {servers} servers", parsed.len());
    Ok(())
}

fn cmd_rules(store: &SqliteStore) -> Result<()> {
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
    ]);
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    for f in &findings {
        store.upsert_finding(f, &now)?;
        println!(
            "[{}] {} scope={} ~{}토큰 절약  {}",
            f.severity.as_str(), f.rule_id, f.scope_ref, f.est_tokens_saved, f.evidence
        );
    }
    println!("total {} findings", findings.len());
    Ok(())
}

fn cmd_diary(store: &SqliteStore, date: Option<String>) -> Result<()> {
    let date = date.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let brief = assemble_brief(store, "Windows", &date)?;
    let cfg = DiaryConfig::default();

    let engine: Box<dyn Engine> = match OpenAiCompatEngine::from_env() {
        Some(e) => {
            println!("engine: {}", e.name());
            Box::new(e)
        }
        None => {
            println!("engine: mock (set AGENT_MENTOR_ENGINE_URL for real engine)");
            Box::new(MockEngine {
                canned: format!(
                    "오늘 {honorific}은 나를 {n}개 세션 굴렸다. 브리프 기반 요약이다.",
                    honorific = cfg.honorific,
                    n = brief.totals.session_count
                ),
            })
        }
    };
    let out = generate_diary(store, engine.as_ref(), &brief, &cfg)?;
    println!("diary → {} (~{} 토큰)", out.path.display(), out.tokens_used);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("all");
    let store = db()?;
    match cmd {
        "ingest" => cmd_ingest(&store)?,
        "inventory" => cmd_inventory(&store)?,
        "rules" => cmd_rules(&store)?,
        "diary" => cmd_diary(&store, args.get(2).cloned())?,
        "all" => {
            cmd_ingest(&store)?;
            cmd_inventory(&store)?;
            cmd_rules(&store)?;
            cmd_diary(&store, None)?;
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: agent-mentor [ingest|inventory|rules|diary [date]|all]");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: 빌드 확인**

Run: `cargo build`
Expected: OK. (경고 무방.)

- [ ] **Step 3: 전체 테스트 스위트 통과 확인**

Run: `cargo test`
Expected: PASS — 모든 단위 테스트 통과.

- [ ] **Step 4: 실측 데이터 종단 검증 (수동)**

Run: `cargo run -- ingest`
Expected: `ingested N new events from M files` (N, M > 0 — 실제 `~/.claude/projects`에 데이터 있음).

Run: `cargo run -- inventory`
Expected: `inventory: P projects, S servers` (P > 0).

Run: `cargo run -- rules`
Expected: R5/R1 Finding 목록 출력. **검증 포인트(데이터 파운데이션 §10.3, 코칭 §10.2):** 안 쓰는 MCP + 대략 비용, 또는 반복 Read Finding이 실제로 뜨는가. 최소 1건 이상 그럴듯한 Finding이 나오면 통과.

- [ ] **Step 5: 다이어리 실측 검증 (수동, 실제 엔진)**

준비된 OpenAI API로 환경변수 설정 후 실행:

```bash
export AGENT_MENTOR_ENGINE_URL="https://api.openai.com/v1"
export AGENT_MENTOR_ENGINE_KEY="sk-..."
export AGENT_MENTOR_ENGINE_MODEL="gpt-4o-mini"
cargo run -- diary
```

Expected(코칭 §10.3 검증):
- `./diary/<오늘>.md` 파일 생성.
- 파일 내용이 **"주인" 호칭 + B톤** 에이전트-시점 일기이고, 브리프의 findings/수치를 반영.
- 마지막 줄에 `*— 이 일기 ~<N> 토큰 (엔진: openai-compat:...)*` 자기계량 푸터.
- `diary_index`에 행 1건.

- [ ] **Step 6: ccusage 교차검증 (수동, 선택 — 데이터 파운데이션 §10.1)**

`ccusage`가 설치돼 있으면 토큰 총합을 근사 비교:

```bash
npx ccusage@latest --json > ccusage.json
sqlite3 ./agent-mentor.db "SELECT SUM(tok_input), SUM(tok_output), SUM(tok_cache_create) FROM events;"
```

Expected: 우리 events 합계가 ccusage 총합과 **자릿수·근사치 수준에서 일치**(완전 일치 아님 — 우리는 세로 슬라이스라 일부 라인 타입만 파싱). 큰 괴리(수십 배)면 파서 버그 신호.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: add verification CLI wiring ingest/inventory/rules/diary end-to-end"
```

---

## Spec Coverage Map (self-review)

| 스펙 요구 | 태스크 |
|---|---|
| 데이터파운데이션 §3 SourceAdapter 심(discover/read_incremental/map) | Task 2 |
| §3 관대한 파싱(하드 실패 금지) | Task 2 (Value 기반, `map_never_panics`) |
| §4 NormalizedEvent 봉투 + EventKind + 서브타입(ToolKind/NormModel/TokenUsage) | Task 1 |
| §4.5 ToolKind 정규화 + MCP명 파싱 | Task 1 |
| §4.5 모델 티어링 | Task 1 |
| §5 SQLite 스키마(sessions/events/daily_rollup/findings/mcp_inventory/diary_index/ingest_state) | Task 3 |
| §6 증분 수집·완결 라인·멱등 upsert·워터마크 | Task 2(read_incremental) + Task 3(upsert/offset) + Task 4(ingest_file) |
| §6 daily_rollup 갱신 | Task 4 |
| §8 R1 총 always-on(첫 턴 cache_creation) + 미사용 탐지 + 추정 라벨 | Task 8 |
| §2.2 MCP 설정 위치 종합(claude.json) | Task 7 |
| 코칭 §3.2 Finding 모델(전 필드) | Task 5 |
| 코칭 §5 R5(반복 Read), R1(안 쓰는 MCP) | Task 6, Task 8 |
| 코칭 §3.2 est_tokens_saved 공용통화 | Task 6/8(규칙별 산정) |
| 코칭 §7.2 다이어리 파이프라인(브리프→LLM→vault .md→색인) | Task 9, 11 |
| 코칭 §1.6 엔진 정책(on-prem 기본 OpenAI 호환, 옵션) | Task 10 |
| 코칭 §6.4 자기 토큰 계량 | Task 10(EngineOutput.tokens_used) + Task 11(푸터) |
| 코칭 §1.4 정밀도의 선(처방=규칙, 서사=LLM, 수치 근거) | Task 8(prescription) + Task 11(system prompt) |
| §10.1 ccusage 교차검증 | Task 12 Step 6 |

**명시적 유예(이번 슬라이스 밖, 스펙 근거 있음):**
- WSL 호스트 열거 + `notify` 실시간 감시(데이터파운데이션 §7, §6) — 배치 수집으로 대체
- 옵트인 MCP 프로브 / 서버별 정확 귀속(§8) — R1은 flat heuristic
- OpenCode/Codex 어댑터(§3, YAGNI)
- `following` 상태머신·나깅 라이프사이클(코칭 §8), 실시간 넛지·채팅(코칭 §7.4), Tier 2(코칭 §6.2)
- ToolResult/UserPrompt EventKind, tool_assets(신규 도구 diff), 주간·월간 종단 서사
- Tauri 셸/3-윈도우/마스코트

---

## Notes for the implementer

- **경고 무시 가능:** 슬라이스 중간 태스크에서 미사용 `import`/`dead_code` 경고가 날 수 있다. Task 12에서 CLI가 전부 배선하면 대부분 사라진다. 규칙 위반이 아닌 한 경고 제거를 위해 코드를 바꾸지 말 것(Global: surgical).
- **rusqlite 버전:** `0.32` 기준 API. 만약 `cargo build`가 rusqlite에서 실패하면 `cargo add rusqlite --features bundled`로 최신 호환 버전을 잡고 API 차이(주로 `params!`/`query_map` 시그니처)만 맞춘다.
- **ureq 버전:** `2.x` 기준(`send_json`, `into_json`). `3.x`가 잡히면 API가 다르니 `Cargo.toml`에서 `ureq = "2"`로 고정.
- **project_id join:** R1이 Finding을 못 내면 십중팔구 events의 `project_id`(디렉터리명 정규화)와 인벤토리의 `project_id`(claude.json 경로 정규화)가 안 맞는 것이다. `normalize_project_key`를 양쪽에 동일 적용했는지 먼저 확인.
```
