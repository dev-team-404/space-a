---
status: done
archived: 2026-07-22
---

# 주인 메모리(Owner Memory) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 마스코트가 주인이 명시적으로 말한 자유 텍스트 사실을 저장하고, 채팅·코칭·일기에서 자연스럽게 참조하게 한다.

**Architecture:** 신규 `memories` SQLite 테이블 + Store CRUD → 순수 `memory_block` 주입 헬퍼 → 채팅/코칭/일기 시스템 프롬프트에 결정론 주입. 캡처는 채팅 엔진의 `save_memory` tool-calling 루프(`run_memory_chat`)로, 툴 미지원 엔진은 자동 폴백(graceful degrade). 관리는 Tauri 커맨드 + `LifeSettingsTab` 카드.

**Tech Stack:** Rust (Cargo workspace `crates/core` = `agent_mentor`, `src-tauri` = Tauri v2), `rusqlite`, `ureq`, `serde_json`, Svelte 5 + TypeScript + Vitest.

## Global Constraints

- **빌드/실행은 네이티브 Windows PowerShell에서만.** WSL 안에서 `cargo`/`npm`/`tauri` 빌드·실행 금지 (CLAUDE.md). WSL 세션에서는 코드만 작성하고, 각 태스크의 테스트는 Windows에서 실행해 통과를 확인한다.
- **커밋 메시지는 영어 Conventional Commits** (`feat(...)`, `test(...)`, `refactor(...)`). Claude/Anthropic attribution trailer 금지.
- **전송 경계**: 메모리 텍스트는 설정된 엔진으로 전송된다(의도된 완화). 트랜스크립트 원문은 여전히 전송 금지.
- **에이전트별 하드코딩 금지**: 엔진 로직은 `Engine` trait 뒤에 둔다.
- **프론트엔드는 렌더링만**, 무거운 로직은 `crates/core`.
- **결정론 조립 유지**: 프롬프트 주입에 추가 LLM 호출/랭킹을 넣지 않는다.

**Spec:** `docs/archive/design/a-mate/specs/2026-07-22-owner-memory-design.md`

---

## File Structure

| 파일 | 책임 | 태스크 |
|------|------|--------|
| `crates/core/src/memory.rs` (신규) | `Memory` 모델, `memory_block` 주입 헬퍼, 캡 상수 | 1, 2 |
| `crates/core/src/lib.rs` | `pub mod memory;` 등록 | 1 |
| `crates/core/src/store.rs` | `memories` 테이블 + CRUD | 1 |
| `crates/core/src/diary/engine.rs` | `ToolDef`/`ToolCall`/`ChatTurn`, `ChatMessage` 확장, `chat_with_tools` | 3 |
| `crates/core/src/chat.rs` | `run_memory_chat` 루프, `save_memory_tool`, 프롬프트 메모리 주입 | 4, 5 |
| `crates/core/src/diary/mod.rs` | 일기 프롬프트 메모리 주입 | 6 |
| `src-tauri/src/pipeline.rs` | 일기 렌더에 메모리 로드·전달 | 6 |
| `src-tauri/src/commands.rs` | `chat_send` tool-loop, `memory_*` 커맨드 | 7, 8 |
| `src-tauri/src/lib.rs` | 커맨드 등록 | 8 |
| `src/lib/api.ts` | `Memory` 타입 + `memory*` API 래퍼 | 9 |
| `src/lib/ui/LifeSettingsTab.svelte` | "주인 메모리" 카드 | 10 |

---

## Task 1: Memory 모델 + `memories` 테이블 + Store CRUD

**Files:**
- Create: `crates/core/src/memory.rs`
- Modify: `crates/core/src/lib.rs:21` (모듈 등록), `crates/core/src/store.rs` (스키마 배치 + CRUD)
- Test: `crates/core/src/store.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `agent_mentor::memory::Memory { id: i64, text: String, created_at: String, updated_at: Option<String>, source: String }`
- Produces: `SqliteStore::add_memory(&self, text: &str, source: &str) -> Result<i64>`, `list_memories(&self) -> Result<Vec<Memory>>`, `update_memory(&self, id: i64, text: &str) -> Result<()>`, `delete_memory(&self, id: i64) -> Result<()>`, `count_memories(&self) -> Result<u64>`

- [ ] **Step 1: `memory.rs` 생성 — Memory 모델**

Create `crates/core/src/memory.rs`:

```rust
//! 주인 메모리(Owner Memory) — 주인이 명시적으로 기억을 요청한 자유 텍스트 사실.
//! 채팅 `save_memory` 툴콜 또는 수동 UI로 저장되고, 채팅·코칭·일기 프롬프트에 주입된다.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub text: String,
    pub created_at: String,        // 로컬 날짜 또는 RFC3339
    pub updated_at: Option<String>,
    pub source: String,            // "chat" | "manual"
}
```

- [ ] **Step 2: `lib.rs`에 모듈 등록**

Modify `crates/core/src/lib.rs` — `pub mod mascot;` 근처(알파벳 순 유지 위해 `pub mod judge;` 다음 줄)에 추가:

```rust
pub mod memory;
```

- [ ] **Step 3: `store.rs` 스키마 배치에 테이블 추가**

Modify `crates/core/src/store.rs` — 기존 `CREATE TABLE IF NOT EXISTS hub_share_state (...)` 다음, 스키마 배치 문자열(`"#;`로 끝나는 raw 문자열) 안에 추가:

```sql
CREATE TABLE IF NOT EXISTS memories (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  text       TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT,
  source     TEXT NOT NULL DEFAULT 'chat'
);
```

- [ ] **Step 4: 실패하는 테스트 작성**

Modify `crates/core/src/store.rs` `#[cfg(test)] mod tests` 끝에 추가:

```rust
#[test]
fn memory_crud_roundtrip() {
    use crate::memory::Memory;
    let store = SqliteStore::open_in_memory().unwrap();
    assert_eq!(store.count_memories().unwrap(), 0);

    let id1 = store.add_memory("주인은 비간을 먹지 않음", "chat").unwrap();
    let id2 = store.add_memory("목요일 오후는 회의로 바쁨", "manual").unwrap();
    assert!(id2 > id1);
    assert_eq!(store.count_memories().unwrap(), 2);

    // created_at ASC, id ASC 정렬
    let all = store.list_memories().unwrap();
    assert_eq!(all.iter().map(|m: &Memory| m.text.as_str()).collect::<Vec<_>>(),
               vec!["주인은 비간을 먹지 않음", "목요일 오후는 회의로 바쁨"]);
    assert_eq!(all[0].source, "chat");
    assert!(all[0].updated_at.is_none());

    store.update_memory(id1, "주인은 완전 채식(비건)임").unwrap();
    let updated = store.list_memories().unwrap();
    assert_eq!(updated[0].text, "주인은 완전 채식(비건)임");
    assert!(updated[0].updated_at.is_some());

    store.delete_memory(id2).unwrap();
    assert_eq!(store.count_memories().unwrap(), 1);
}

#[test]
fn add_memory_rejects_blank() {
    let store = SqliteStore::open_in_memory().unwrap();
    assert!(store.add_memory("   ", "chat").is_err());
    assert_eq!(store.count_memories().unwrap(), 0);
}
```

- [ ] **Step 5: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor memory_crud_roundtrip add_memory_rejects_blank`
Expected: FAIL — `no method named add_memory` 등 미구현 에러.

- [ ] **Step 6: CRUD 구현**

Modify `crates/core/src/store.rs` — `set_setting` 메서드(약 `store.rs:1158`) 다음에 추가. 파일 상단에 이미 `use rusqlite::{params, ... OptionalExtension}` 가 있으니 그대로 사용:

```rust
// ── 주인 메모리 (memory.rs — 2026-07-22-owner-memory 스펙) ──

pub fn add_memory(&self, text: &str, source: &str) -> Result<i64> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("메모리 텍스트가 비어 있습니다");
    }
    let now = chrono::Local::now().format("%Y-%m-%d").to_string();
    self.conn.execute(
        "INSERT INTO memories (text, created_at, source) VALUES (?1, ?2, ?3)",
        params![text, now, source],
    )?;
    Ok(self.conn.last_insert_rowid())
}

pub fn list_memories(&self) -> Result<Vec<crate::memory::Memory>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, text, created_at, updated_at, source FROM memories
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::memory::Memory {
            id: r.get(0)?,
            text: r.get(1)?,
            created_at: r.get(2)?,
            updated_at: r.get(3)?,
            source: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn update_memory(&self, id: i64, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("메모리 텍스트가 비어 있습니다");
    }
    let now = chrono::Local::now().format("%Y-%m-%d").to_string();
    self.conn.execute(
        "UPDATE memories SET text=?1, updated_at=?2 WHERE id=?3",
        params![text, now, id],
    )?;
    Ok(())
}

pub fn delete_memory(&self, id: i64) -> Result<()> {
    self.conn.execute("DELETE FROM memories WHERE id=?1", params![id])?;
    Ok(())
}

pub fn count_memories(&self) -> Result<u64> {
    let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    Ok(n as u64)
}
```

- [ ] **Step 7: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor memory`
Expected: PASS (`memory_crud_roundtrip`, `add_memory_rejects_blank`).

- [ ] **Step 8: Commit**

```bash
git add crates/core/src/memory.rs crates/core/src/lib.rs crates/core/src/store.rs
git commit -m "feat(core): add memories table and Memory CRUD"
```

---

## Task 2: `memory_block` 주입 헬퍼 (순수)

**Files:**
- Modify: `crates/core/src/memory.rs`
- Test: `crates/core/src/memory.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: (없음 — 순수 함수, `&[String]` 입력)
- Produces: `agent_mentor::memory::memory_block(lines: &[String]) -> String` (빈 목록이면 `""`), 상수 `MAX_MEMORIES: usize = 40`, `MAX_MEMORY_CHARS: usize = 2000`

- [ ] **Step 1: 실패하는 테스트 작성**

Modify `crates/core/src/memory.rs` — 파일 끝에 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_empty_when_no_memories() {
        assert_eq!(memory_block(&[]), "");
    }

    #[test]
    fn block_formats_bullets() {
        let lines = vec!["주인은 비건임".to_string(), "목요일 오후 회의".to_string()];
        let b = memory_block(&lines);
        assert!(b.contains("- 주인은 비건임"));
        assert!(b.contains("- 목요일 오후 회의"));
    }

    #[test]
    fn block_caps_by_count_keeps_most_recent() {
        // list_memories는 created_at ASC(오래된→최신). 캡 초과 시 최신(뒤쪽) 유지.
        let lines: Vec<String> = (0..(MAX_MEMORIES + 5)).map(|i| format!("mem{i}")).collect();
        let b = memory_block(&lines);
        let count = b.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(count, MAX_MEMORIES);
        assert!(b.contains(&format!("mem{}", MAX_MEMORIES + 4)));  // 최신 포함
        assert!(!b.contains("- mem0\n") && !b.contains("- mem0"));  // 가장 오래된 것 제외
    }

    #[test]
    fn block_caps_by_chars() {
        let big = "가".repeat(600);
        let lines = vec![big.clone(), big.clone(), big.clone(), big.clone(), big];
        let b = memory_block(&lines);
        // 각 ~600자, 캡 2000자 → 최신 3개 정도만
        assert!(b.chars().count() <= MAX_MEMORY_CHARS + 200);  // 헤더/불릿 여유
    }
}
```

- [ ] **Step 2: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor memory::tests`
Expected: FAIL — `cannot find function memory_block`.

- [ ] **Step 3: `memory_block` 구현**

Modify `crates/core/src/memory.rs` — `Memory` 구조체 다음에 추가:

```rust
/// 프롬프트에 주입할 최대 메모리 개수. 사용자 큐레이션이라 도달 가능성 낮음.
pub const MAX_MEMORIES: usize = 40;
/// 프롬프트에 주입할 메모리 총 문자수 상한(대략).
pub const MAX_MEMORY_CHARS: usize = 2000;

/// 메모리 텍스트 목록을 프롬프트 블록으로 직렬화한다(캡 적용).
/// 입력은 `list_memories()` 순서(created_at ASC = 오래된→최신)를 가정한다.
/// 캡을 넘으면 **최신(뒤쪽)**을 유지하고 초과분은 버리며, 버린 수를 로그로 남긴다(무음 절단 금지).
/// 빈 목록이면 빈 문자열을 반환한다(호출부가 섹션을 생략).
pub fn memory_block(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    // 최신 우선으로 뒤에서부터 캡 적용, 출력은 다시 오래된→최신 순으로.
    let mut selected: Vec<&String> = Vec::new();
    let mut chars = 0usize;
    for line in lines.iter().rev() {
        if selected.len() >= MAX_MEMORIES {
            break;
        }
        let next = chars + line.chars().count() + 2; // "- " 여유
        if !selected.is_empty() && next > MAX_MEMORY_CHARS {
            break;
        }
        chars = next;
        selected.push(line);
    }
    let dropped = lines.len() - selected.len();
    if dropped > 0 {
        log::info!("memory_block: {dropped}개 메모리가 캡({MAX_MEMORIES}개/{MAX_MEMORY_CHARS}자)으로 제외됨");
    }
    selected.reverse();
    selected
        .iter()
        .map(|l| format!("- {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
```

- [ ] **Step 4: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor memory::tests`
Expected: PASS (4개 테스트).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/memory.rs
git commit -m "feat(core): add memory_block prompt-injection helper with cap"
```

---

## Task 3: Engine tool-calling 프리미티브

**Files:**
- Modify: `crates/core/src/diary/engine.rs`
- Test: `crates/core/src/diary/engine.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `ToolDef { name: String, description: String, parameters: serde_json::Value }`, `ToolCall { id: String, name: String, arguments: String }`, `ChatTurn { text: Option<String>, tool_calls: Vec<ToolCall>, tokens_used: u64 }`
- Produces: `ChatMessage` 확장 — 추가 필드 `tool_calls: Vec<ToolCall>`, `tool_call_id: Option<String>` (둘 다 serde default, 비면 직렬화 생략)
- Produces: `Engine::chat_with_tools(&self, system: &str, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<ChatTurn>` (trait 기본 구현 = 툴 무시하고 `chat` 래핑 → graceful degrade; `OpenAiCompatEngine`가 override)
- Produces: `parse_tool_turn(v: &serde_json::Value) -> ChatTurn` (순수 파서, 테스트용 공개)

- [ ] **Step 1: 새 타입 + ChatMessage 확장**

Modify `crates/core/src/diary/engine.rs` — `ChatMessage` 정의를 교체하고 새 타입 추가:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// 어시스턴트가 낸 툴콜 에코(툴 루프 재요청용). 비면 직렬화 생략.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// role=="tool" 결과 메시지의 대상 툴콜 id. 비면 직렬화 생략.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 엔진에 넘기는 툴 정의(OpenAI function tool로 직렬화).
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 모델이 낸 툴콜 1건.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // JSON 문자열
}

/// tool-aware chat 한 턴 결과: 최종 텍스트이거나 툴콜 목록.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tokens_used: u64,
}
```

- [ ] **Step 2: 실패하는 테스트 작성**

Modify `crates/core/src/diary/engine.rs` `#[cfg(test)] mod tests` 끝에 추가:

```rust
#[test]
fn parse_tool_turn_reads_tool_calls() {
    let v: serde_json::Value = serde_json::from_str(r#"{
      "choices":[{"message":{"content":null,"tool_calls":[
        {"id":"call_1","type":"function","function":{"name":"save_memory","arguments":"{\"text\":\"주인은 비건임\"}"}}
      ]}}],
      "usage":{"total_tokens":42}
    }"#).unwrap();
    let turn = parse_tool_turn(&v);
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "save_memory");
    assert_eq!(turn.tool_calls[0].id, "call_1");
    assert!(turn.tool_calls[0].arguments.contains("비건"));
    assert_eq!(turn.tokens_used, 42);
}

#[test]
fn parse_tool_turn_reads_plain_text() {
    let v: serde_json::Value = serde_json::from_str(r#"{
      "choices":[{"message":{"content":"기억했어요!"}}],"usage":{"total_tokens":5}
    }"#).unwrap();
    let turn = parse_tool_turn(&v);
    assert!(turn.tool_calls.is_empty());
    assert_eq!(turn.text.as_deref(), Some("기억했어요!"));
}

#[test]
fn default_chat_with_tools_degrades_to_text() {
    // 기본 구현(MockEngine)은 툴을 무시하고 텍스트만 반환한다.
    let eng = MockEngine { canned: "안녕 주인".into() };
    let msgs = vec![ChatMessage { role: "user".into(), content: "안녕?".into(), ..Default::default() }];
    let turn = eng.chat_with_tools("sys", &msgs, &[]).unwrap();
    assert!(turn.tool_calls.is_empty());
    assert_eq!(turn.text.as_deref(), Some("안녕 주인"));
}
```

- [ ] **Step 3: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor parse_tool_turn default_chat_with_tools`
Expected: FAIL — `cannot find function parse_tool_turn`, `no method chat_with_tools`.

- [ ] **Step 4: trait 기본 구현 + 파서 + OpenAI override**

Modify `crates/core/src/diary/engine.rs`:

(a) `Engine` trait에 기본 구현 메서드 추가:

```rust
pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>;

    /// tool-aware chat. 기본 구현은 툴을 무시하고 `chat`을 래핑한다(툴 미지원 엔진 graceful degrade).
    fn chat_with_tools(
        &self,
        system: &str,
        messages: &[ChatMessage],
        _tools: &[ToolDef],
    ) -> Result<ChatTurn> {
        let out = self.chat(system, messages)?;
        Ok(ChatTurn { text: Some(out.text), tool_calls: vec![], tokens_used: out.tokens_used })
    }
}
```

(b) 순수 파서(모듈 레벨 함수) 추가:

```rust
/// OpenAI chat/completions 응답에서 텍스트 또는 툴콜을 추출한다.
pub fn parse_tool_turn(v: &serde_json::Value) -> ChatTurn {
    let msg = v.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("message"));
    let tool_calls = msg
        .and_then(|m| m.get("tool_calls"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc.get("id")?.as_str()?.to_string(),
                        name: tc.get("function")?.get("name")?.as_str()?.to_string(),
                        arguments: tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let text = msg
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let tokens_used = v
        .get("usage")
        .and_then(|u| u.get("total_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    ChatTurn { text, tool_calls, tokens_used }
}
```

(c) `impl Engine for OpenAiCompatEngine`에 override 추가 (기존 `chat` 메서드 다음):

```rust
fn chat_with_tools(
    &self,
    system: &str,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> Result<ChatTurn> {
    let mut arr = vec![serde_json::json!({"role": "system", "content": system})];
    for m in messages {
        arr.push(chat_message_to_json(m));
    }
    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": self.model,
        "messages": arr,
        "temperature": 0.7,
        "tools": tools_json,
        "tool_choice": "auto",
    });
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .set("Authorization", &format!("Bearer {}", self.api_key))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("engine tool request failed: {e}"))?;
    let v: serde_json::Value = resp.into_json()?;
    Ok(parse_tool_turn(&v))
}
```

(d) `ChatMessage` → OpenAI JSON 변환 헬퍼(모듈 레벨, 파일 하단 `#[cfg(test)]` 위):

```rust
/// ChatMessage를 OpenAI messages 요소로 변환한다(툴콜 에코·tool 결과 포함).
fn chat_message_to_json(m: &ChatMessage) -> serde_json::Value {
    if let Some(tcid) = &m.tool_call_id {
        return serde_json::json!({"role": m.role, "tool_call_id": tcid, "content": m.content});
    }
    if !m.tool_calls.is_empty() {
        let tcs: Vec<serde_json::Value> = m
            .tool_calls
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id, "type": "function",
                    "function": {"name": t.name, "arguments": t.arguments}
                })
            })
            .collect();
        return serde_json::json!({"role": m.role, "content": m.content, "tool_calls": tcs});
    }
    serde_json::json!({"role": m.role, "content": m.content})
}
```

- [ ] **Step 5: 기존 ChatMessage 리터럴 컴파일 확인**

기존 코드/테스트의 `ChatMessage { role, content }` 리터럴은 새 필드 때문에 컴파일 에러가 난다. `..Default::default()`를 붙여 수정한다. 대상(검색으로 확인): `crates/core/src/diary/engine.rs` 테스트, `crates/core/src/chat.rs` 없음(별도), `src-tauri/src/commands.rs` 테스트.

Run: `cargo build -p agent_mentor` 로 에러 위치를 찾고 각 리터럴에 `..Default::default()` 추가. 예:
```rust
ChatMessage { role: "user".into(), content: "안녕?".into(), ..Default::default() }
```

- [ ] **Step 6: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor parse_tool_turn default_chat_with_tools`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/diary/engine.rs
git commit -m "feat(core): add tool-calling primitives to Engine (ChatTurn, chat_with_tools)"
```

---

## Task 4: `run_memory_chat` 루프 + `save_memory` 툴

**Files:**
- Modify: `crates/core/src/chat.rs`
- Test: `crates/core/src/chat.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `Engine`, `ChatMessage`, `ChatTurn`, `ToolDef`, `ToolCall` (Task 3)
- Produces: `agent_mentor::chat::save_memory_tool() -> ToolDef`
- Produces: `agent_mentor::chat::run_memory_chat<E: Engine + ?Sized>(engine: &E, system: &str, convo: &mut Vec<ChatMessage>, max_rounds: usize, on_save: impl FnMut(&str)) -> anyhow::Result<String>` — 툴 루프 실행, `save_memory` 툴콜 시 `on_save(text)` 호출, 최종 텍스트 반환. 엔진 `chat_with_tools` 에러 시 tools 없는 `chat`으로 폴백.

- [ ] **Step 1: 실패하는 테스트 작성**

Modify `crates/core/src/chat.rs` `#[cfg(test)] mod tests` 끝에 추가:

```rust
use crate::diary::engine::{ChatMessage, ChatTurn, Engine, EngineOutput, ToolCall, ToolDef};
use std::cell::RefCell;

/// 첫 턴에 save_memory 툴콜을 내고, tool 결과를 받으면 텍스트를 반환하는 테스트 엔진.
struct ToolMock { fail_tools: bool }
impl Engine for ToolMock {
    fn name(&self) -> String { "toolmock".into() }
    fn generate(&self, _s: &str, _u: &str) -> anyhow::Result<EngineOutput> {
        Ok(EngineOutput { text: "gen".into(), tokens_used: 1 })
    }
    fn chat(&self, _s: &str, _m: &[ChatMessage]) -> anyhow::Result<EngineOutput> {
        Ok(EngineOutput { text: "폴백 답변".into(), tokens_used: 1 })
    }
    fn chat_with_tools(&self, _s: &str, m: &[ChatMessage], _t: &[ToolDef]) -> anyhow::Result<ChatTurn> {
        if self.fail_tools {
            anyhow::bail!("tools unsupported");
        }
        if m.last().map(|x| x.role == "tool").unwrap_or(false) {
            return Ok(ChatTurn { text: Some("기억했어요!".into()), tool_calls: vec![], tokens_used: 1 });
        }
        Ok(ChatTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "save_memory".into(),
                arguments: r#"{"text":"주인은 비건임"}"#.into(),
            }],
            tokens_used: 1,
        })
    }
}

#[test]
fn run_memory_chat_saves_and_returns_final_text() {
    let eng = ToolMock { fail_tools: false };
    let mut convo = vec![ChatMessage { role: "user".into(), content: "나 비건이야 기억해".into(), ..Default::default() }];
    let saved = RefCell::new(Vec::<String>::new());
    let out = run_memory_chat(&eng, "sys", &mut convo, 3, |t| saved.borrow_mut().push(t.to_string())).unwrap();
    assert_eq!(out, "기억했어요!");
    assert_eq!(saved.borrow().as_slice(), &["주인은 비건임".to_string()]);
    // convo에 어시스턴트 툴콜 에코 + tool 결과가 추가됨
    assert!(convo.iter().any(|m| m.role == "assistant" && !m.tool_calls.is_empty()));
    assert!(convo.iter().any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_1")));
}

#[test]
fn run_memory_chat_degrades_when_tools_error() {
    let eng = ToolMock { fail_tools: true };
    let mut convo = vec![ChatMessage { role: "user".into(), content: "안녕".into(), ..Default::default() }];
    let saved = RefCell::new(Vec::<String>::new());
    let out = run_memory_chat(&eng, "sys", &mut convo, 3, |t| saved.borrow_mut().push(t.to_string())).unwrap();
    assert_eq!(out, "폴백 답변");
    assert!(saved.borrow().is_empty());
}

#[test]
fn save_memory_tool_has_text_param() {
    let t = save_memory_tool();
    assert_eq!(t.name, "save_memory");
    assert!(t.parameters["properties"]["text"].is_object());
}
```

- [ ] **Step 2: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor run_memory_chat save_memory_tool`
Expected: FAIL — `cannot find function run_memory_chat`.

- [ ] **Step 3: 구현**

Modify `crates/core/src/chat.rs` — 파일 상단 `//!` 주석 다음, 첫 `pub` 정의 위에 추가:

```rust
use crate::diary::engine::{ChatMessage, Engine, ToolCall, ToolDef};

/// 채팅 캡처용 save_memory 툴 정의.
pub fn save_memory_tool() -> ToolDef {
    ToolDef {
        name: "save_memory".to_string(),
        description: "주인이 자신에 대해 기억해 달라고 명시적으로 요청한 사실을 한 문장으로 저장한다. \
                      주인이 기억을 요청할 때만 호출하고, 일상 대화에는 호출하지 마라."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "기억할 사실 (간결한 평서문, 예: '주인은 비건임')" }
            },
            "required": ["text"]
        }),
    }
}

/// save_memory 툴콜 arguments(JSON 문자열)에서 text를 추출한다.
fn parse_memory_arg(arguments: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get("text")?
        .as_str()
        .map(|s| s.to_string())
}

/// tool-calling 채팅 루프. save_memory 툴콜이 오면 `on_save(text)`를 호출하고
/// tool 결과를 대화에 붙여 재요청한다. 텍스트 응답이 오면 반환한다.
/// 엔진이 tools를 지원하지 않아 `chat_with_tools`가 실패하면 tools 없는 `chat`으로 폴백한다.
pub fn run_memory_chat<E: Engine + ?Sized>(
    engine: &E,
    system: &str,
    convo: &mut Vec<ChatMessage>,
    max_rounds: usize,
    mut on_save: impl FnMut(&str),
) -> anyhow::Result<String> {
    let tools = [save_memory_tool()];
    for _ in 0..max_rounds {
        let turn = match engine.chat_with_tools(system, convo, &tools) {
            Ok(t) => t,
            Err(_) => return engine.chat(system, convo).map(|o| o.text),
        };
        if turn.tool_calls.is_empty() {
            return Ok(turn.text.unwrap_or_default());
        }
        convo.push(ChatMessage {
            role: "assistant".into(),
            content: turn.text.clone().unwrap_or_default(),
            tool_calls: turn.tool_calls.clone(),
            tool_call_id: None,
        });
        for call in &turn.tool_calls {
            if call.name == "save_memory" {
                if let Some(text) = parse_memory_arg(&call.arguments) {
                    let t = text.trim();
                    if !t.is_empty() {
                        on_save(t);
                    }
                }
            }
            convo.push(ChatMessage {
                role: "tool".into(),
                content: "saved".into(),
                tool_calls: vec![],
                tool_call_id: Some(call.id.clone()),
            });
        }
    }
    // 루프 상한 초과 — tools 없이 마무리 답변
    engine.chat(system, convo).map(|o| o.text)
}
```

참고: 미사용 import 경고를 피하려면 `ToolCall`은 위 코드에서 직접 안 쓰므로 import 목록에서 제외한다(`use crate::diary::engine::{ChatMessage, Engine, ToolDef};`). 테스트 모듈은 자체 `use`를 가진다.

- [ ] **Step 4: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor run_memory_chat save_memory_tool`
Expected: PASS (3개).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/chat.rs
git commit -m "feat(core): add run_memory_chat tool loop with graceful degrade"
```

---

## Task 5: 채팅·코칭 프롬프트에 메모리 주입

**Files:**
- Modify: `crates/core/src/chat.rs` (`ChatContext`, `CoachingBrief`, 두 빌더, `assemble_coaching_brief`)
- Modify: `src-tauri/src/commands.rs` (`chat_context_inner`)
- Test: `crates/core/src/chat.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `memory::memory_block` (Task 2)
- Produces: `ChatContext.memories: Vec<String>`, `CoachingBrief.memories: Vec<String>` — 프롬프트에 `[주인에 대해 기억한 것]` 블록 주입

- [ ] **Step 1: 실패하는 테스트 작성**

Modify `crates/core/src/chat.rs` `#[cfg(test)] mod tests` — 기존 `sample_brief()`에 `memories` 필드를 추가하고(아래 Step 3에서 필드 추가 후 컴파일되도록), 새 테스트 추가:

```rust
#[test]
fn chat_prompt_injects_memories() {
    let ctx = ChatContext {
        user_name: "jibin".into(), date: "2026-07-22".into(),
        session_count: 1, tok_input: 1, tok_output: 1, est_tokens_saved_total: 0,
        findings: vec![],
        memories: vec!["주인은 비건임".into(), "목요일 오후 회의".into()],
    };
    let p = build_chat_system_prompt(&ctx);
    assert!(p.contains("[주인에 대해 기억한 것]"));
    assert!(p.contains("- 주인은 비건임"));
    assert!(p.contains("지어내지 마세요")); // 기존 정밀도의 선 유지
}

#[test]
fn chat_prompt_omits_memory_section_when_empty() {
    let ctx = ChatContext {
        user_name: "u".into(), date: "2026-07-22".into(),
        session_count: 0, tok_input: 0, tok_output: 0, est_tokens_saved_total: 0,
        findings: vec![], memories: vec![],
    };
    assert!(!build_chat_system_prompt(&ctx).contains("[주인에 대해 기억한 것]"));
}

#[test]
fn coaching_prompt_injects_memories() {
    let mut brief = sample_brief();
    brief.memories = vec!["주인은 아침형 인간".into()];
    let p = build_coaching_system_prompt(&brief);
    assert!(p.contains("[주인에 대해 기억한 것]"));
    assert!(p.contains("- 주인은 아침형 인간"));
}
```

기존 테스트의 `ChatContext { ... }` 리터럴(예: `chat_prompt_includes_persona_summary_and_findings`, `chat_prompt_empty_findings_says_none`)에도 `memories: vec![]`를 추가한다. `sample_brief()`의 `CoachingBrief { ... }`와 `coaching_prompt_handles_empty_and_mastered`의 리터럴에도 `memories: vec![]` 추가.

- [ ] **Step 2: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor chat_prompt_injects_memories coaching_prompt_injects_memories`
Expected: FAIL — `ChatContext`에 `memories` 필드 없음.

- [ ] **Step 3: 구조체 필드 + 빌더 주입**

Modify `crates/core/src/chat.rs`:

(a) `ChatContext`에 필드 추가:
```rust
pub struct ChatContext {
    pub user_name: String,
    pub date: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub est_tokens_saved_total: u64,
    pub findings: Vec<(String, String)>,
    /// 주인 메모리 텍스트(list_memories 순서). 프롬프트에 주입.
    pub memories: Vec<String>,
}
```

(b) `CoachingBrief`에 필드 추가(구조체 끝 `model_mix` 다음):
```rust
    pub model_mix: Vec<(String, u64)>,
    /// 주인 메모리 텍스트.
    pub memories: Vec<String>,
```

(c) 공용 섹션 헬퍼(파일 내 모듈 레벨, `build_chat_system_prompt` 위):
```rust
/// 메모리 프롬프트 섹션(비면 빈 문자열).
fn memory_section(memories: &[String]) -> String {
    let block = crate::memory::memory_block(memories);
    if block.is_empty() {
        String::new()
    } else {
        format!("\n\n[주인에 대해 기억한 것 — 관련될 때만 자연스럽게 언급, 없는 사실 지어내지 말 것]\n{block}")
    }
}
```

(d) `build_chat_system_prompt`의 `format!(...)` 끝(`{findings_block}` 뒤)에 `{mem}`를 붙이고 인자에 `mem = memory_section(&ctx.memories)` 추가:
```rust
    format!(
        "... [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}{mem}",
        // ...기존 인자...
        mem = memory_section(&ctx.memories),
    )
```

(e) `build_coaching_system_prompt`의 `format!(...)` 끝(`{mix_block}` 뒤)에 동일하게 `{mem}` 추가:
```rust
        "...[모델 사용 믹스]\n{mix_block}{mem}",
        // ...기존 인자...
        mem = memory_section(&brief.memories),
```

(f) `assemble_coaching_brief`에서 메모리 로드 — `Ok(CoachingBrief { ... })` 직전에:
```rust
    let memories = store.list_memories()?.into_iter().map(|m| m.text).collect();
```
그리고 `CoachingBrief { ... }` 리터럴에 `memories,` 추가.

- [ ] **Step 4: `chat_context_inner`에 메모리 로드**

Modify `src-tauri/src/commands.rs` — `chat_context_inner`(약 `commands.rs:222`)의 `Ok(agent_mentor::chat::ChatContext { ... })`에 메모리 로드 추가:
```rust
pub fn chat_context_inner(store: &SqliteStore) -> anyhow::Result<agent_mentor::chat::ChatContext> {
    // ...기존 수집...
    let memories = store.list_memories()?.into_iter().map(|m| m.text).collect();
    Ok(agent_mentor::chat::ChatContext {
        // ...기존 필드...
        memories,
    })
}
```

- [ ] **Step 5: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor chat`
Expected: PASS (신규 3개 + 기존 chat 테스트 회귀 없음).

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/chat.rs src-tauri/src/commands.rs
git commit -m "feat(core): inject owner memories into chat and coaching prompts"
```

---

## Task 6: 일기 프롬프트에 메모리 주입

**Files:**
- Modify: `crates/core/src/diary/mod.rs` (`build_system_prompt`, `build_idle_prompt`, `render_diary`, `render_idle_diary`, `generate_diary`)
- Modify: `src-tauri/src/pipeline.rs` (렌더 호출부에 메모리 로드·전달)
- Modify: `crates/core/src/main.rs` (CLI generate_diary 경로 — 변경 없음, generate_diary 내부에서 로드)
- Test: `crates/core/src/diary/mod.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `build_system_prompt(cfg: &DiaryConfig, commit_count: usize, memories: &[String]) -> String`
- Produces: `build_idle_prompt(cfg: &DiaryConfig, memories: &[String]) -> String`
- Produces: `render_diary(engine, brief, cfg, memories: &[String])`, `render_idle_diary(engine, idle, cfg, memories: &[String])`
- `generate_diary`는 시그니처 유지(내부에서 `store.list_memories()` 로드)

- [ ] **Step 1: 실패하는 테스트 작성**

Modify `crates/core/src/diary/mod.rs` `#[cfg(test)] mod tests` 끝에 추가:

```rust
#[test]
fn diary_system_prompt_injects_memories() {
    let cfg = DiaryConfig::default();
    let mems = vec!["주인은 비건임".to_string()];
    let p = build_system_prompt(&cfg, 0, &mems);
    assert!(p.contains("[주인에 대해 기억한 것"));
    assert!(p.contains("주인은 비건임"));
}

#[test]
fn diary_system_prompt_no_memory_section_when_empty() {
    let p = build_system_prompt(&DiaryConfig::default(), 0, &[]);
    assert!(!p.contains("[주인에 대해 기억한 것"));
}

#[test]
fn idle_prompt_injects_memories() {
    let p = build_idle_prompt(&DiaryConfig::default(), &["주인은 고양이를 키움".to_string()]);
    assert!(p.contains("주인은 고양이를 키움"));
}
```

기존 테스트에서 `build_system_prompt(&cfg, N)` / `build_idle_prompt(&cfg)` 호출부에 `&[]` 인자를 추가한다(mod.rs 내 여러 곳: `988`, `1523`, `1553`, `1559`, `1569`, `1590~1592`, `1597`, `1609`, `1321` 등). `render_diary(&engine, &brief, &cfg)` / `render_idle_diary(&engine, &idle, &cfg)` 호출부(`1831`, `1337`)에도 `&[]` 추가.

- [ ] **Step 2: 테스트 실패 확인 (Windows)**

Run: `cargo test -p agent_mentor diary_system_prompt_injects_memories idle_prompt_injects_memories`
Expected: FAIL — 인자 개수 불일치 / 미구현.

- [ ] **Step 3: 프롬프트 빌더에 메모리 섹션 추가**

Modify `crates/core/src/diary/mod.rs`:

(a) 모듈 레벨 헬퍼(파일 내 `build_system_prompt` 위)에 추가:
```rust
/// 일기용 메모리 섹션(비면 빈 문자열).
fn memory_section(memories: &[String]) -> String {
    let block = crate::memory::memory_block(memories);
    if block.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n[주인에 대해 기억한 것 — 관련되면 자연스럽게 녹이되, 억지로 넣거나 없는 사실을 지어내지 말 것]\n{block}"
        )
    }
}
```

(b) `build_system_prompt` 시그니처를 `(cfg, commit_count, memories: &[String])`로 바꾸고, `format!(...)` 문자열 끝에 `{mem}` 추가 + 인자 `mem = memory_section(memories)`:
```rust
pub fn build_system_prompt(cfg: &DiaryConfig, commit_count: usize, memories: &[String]) -> String {
    let (target, paras) = diary_length(commit_count);
    format!(
        "... 같은 이모지를 반복하지 마세요.{mem}",
        honorific = cfg.honorific,
        tone = cfg.tone,
        voice = voice_guidance(),
        target = target,
        paras = paras,
        mem = memory_section(memories),
    )
}
```

(c) `build_idle_prompt` 시그니처를 `(cfg, memories: &[String])`로 바꾸고 동일하게 `{mem}` 추가:
```rust
pub fn build_idle_prompt(cfg: &DiaryConfig, memories: &[String]) -> String {
    format!(
        "... 그날 컨텍스트로 매번 다르게.{mem}",
        honorific = cfg.honorific,
        voice = voice_guidance(),
        mem = memory_section(memories),
    )
}
```

- [ ] **Step 4: render 함수에 메모리 전달**

Modify `crates/core/src/diary/mod.rs`:

```rust
pub fn render_diary(engine: &dyn Engine, brief: &Brief, cfg: &DiaryConfig, memories: &[String]) -> Result<RenderedDiary> {
    let system = build_system_prompt(cfg, brief.work_log.commit_count, memories);
    // ...나머지 동일...
}

pub fn render_idle_diary(engine: &dyn Engine, idle: &IdleContext, cfg: &DiaryConfig, memories: &[String]) -> Result<RenderedDiary> {
    let system = build_idle_prompt(cfg, memories);
    // ...나머지 동일...
}

pub fn generate_diary(store: &SqliteStore, engine: &dyn Engine, brief: &Brief, cfg: &DiaryConfig) -> Result<DiaryOutput> {
    let memories: Vec<String> = store.list_memories()?.into_iter().map(|m| m.text).collect();
    let rendered = render_diary(engine, brief, cfg, &memories)?;
    persist_diary(store, &brief.date, &brief.host, &rendered, cfg)
}
```

- [ ] **Step 5: 파이프라인 호출부 수정**

Modify `src-tauri/src/pipeline.rs` — phase ①(store 락 구간, 약 `285~297`)에서 메모리를 함께 로드하고 튜플로 반환:

```rust
let (brief, cfg, days_idle, memories) = match store_mutex.lock() {
    Ok(store) => {
        let cfg = DiaryConfig { vault_dir: vault.clone(), ..DiaryConfig::default() };
        match assemble_brief(&store, "Windows", &date, &cfg) {
            Ok(brief) => {
                let days_idle = store.days_since_last_active(&date).ok().flatten();
                let memories: Vec<String> =
                    store.list_memories().map(|v| v.into_iter().map(|m| m.text).collect()).unwrap_or_default();
                (brief, cfg, days_idle, memories)
            }
            Err(e) => { log::warn!("assemble_brief({date}) 실패: {e}"); continue; }
        }
    }
    Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
};
```

그리고 render 호출부(약 `307`, `312`)에 `&memories` 추가:
```rust
match render_idle_diary(&engine, &idle, &cfg, &memories) { /* ... */ }
// ...
match render_diary(&engine, &brief, &cfg, &memories) { /* ... */ }
```

- [ ] **Step 6: 테스트 통과 확인 (Windows)**

Run: `cargo test -p agent_mentor diary`
Expected: PASS (신규 3개 + 기존 diary 테스트 회귀 없음).
Run: `cargo build -p agent-mentor-app` (Tauri 셸 컴파일 — pipeline 변경 확인).
Expected: 컴파일 성공.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/diary/mod.rs src-tauri/src/pipeline.rs
git commit -m "feat(core): inject owner memories into diary prompts"
```

---

## Task 7: `chat_send` tool-loop 연결

**Files:**
- Modify: `src-tauri/src/commands.rs` (`chat_send`)

**Interfaces:**
- Consumes: `chat::run_memory_chat`, `chat::save_memory_tool`, `store.add_memory` (Tasks 1, 4)

- [ ] **Step 1: `chat_send` 교체**

Modify `src-tauri/src/commands.rs` — `chat_send`(약 `commands.rs:460`)를 교체:

```rust
#[tauri::command(async)]
pub fn chat_send(state: State<AppState>, messages: Vec<ChatMessage>) -> Result<String, String> {
    use agent_mentor::chat::{classify_intent, run_memory_chat, ChatIntent};
    validate_chat_messages(&messages)?;
    let last_user = messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.as_str()).unwrap_or("");
    let intent = classify_intent(last_user);

    // 락 범위: 엔진 해석 + 시스템 프롬프트(메모리 포함) 조립. 네트워크 전에 해제.
    let (engine, system) = {
        let guard = lock(&state)?;
        let engine = crate::resolve_engine(&guard);
        let system = match intent {
            ChatIntent::Coaching => {
                let brief = coaching_brief_inner(&*guard).map_err(|e| e.to_string())?;
                agent_mentor::chat::build_coaching_system_prompt(&brief)
            }
            _ => {
                let ctx = chat_context_inner(&*guard).map_err(|e| e.to_string())?;
                agent_mentor::chat::build_chat_system_prompt(&ctx)
            }
        };
        (engine, system)
    };
    let Some(engine) = engine else {
        return Err("엔진이 설정되지 않았어요".into());
    };

    // 이력 상한 20턴 + 루프 중 tool 메시지 누적
    let mut convo: Vec<ChatMessage> = messages[messages.len().saturating_sub(20)..].to_vec();

    // save_memory 툴콜은 락을 새로 잡아 저장한다(네트워크 호출은 run_memory_chat 안, 락 밖).
    let state_ref = &state;
    run_memory_chat(&engine, &system, &mut convo, 3, |text| {
        if let Ok(guard) = lock(state_ref) {
            if let Err(e) = guard.add_memory(text, "chat") {
                log::warn!("add_memory(chat) 실패: {e}");
            }
        }
    })
    .map_err(|e| e.to_string())
}
```

참고: `run_memory_chat`의 `on_save` 클로저는 `chat_with_tools`(네트워크) 호출과 호출 사이에서만 실행되므로, 클로저 안에서 락을 잡아도 네트워크 중 락 보유가 아니다(규율 유지).

- [ ] **Step 2: 컴파일 + 기존 커맨드 테스트 확인 (Windows)**

Run: `cargo build -p agent-mentor-app`
Expected: 컴파일 성공.
Run: `cargo test -p agent-mentor-app`
Expected: 기존 커맨드 테스트 PASS (회귀 없음).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(app): wire chat_send through run_memory_chat tool loop"
```

---

## Task 8: 메모리 관리 Tauri 커맨드

**Files:**
- Modify: `src-tauri/src/commands.rs` (신규 커맨드), `src-tauri/src/lib.rs` (등록)
- Test: `src-tauri/src/commands.rs` `#[cfg(test)]`

**Interfaces:**
- Produces (Tauri 커맨드): `memory_list() -> Vec<Memory>`, `memory_add(text: String) -> Memory`, `memory_update(id: i64, text: String) -> Memory`, `memory_delete(id: i64) -> ()`

- [ ] **Step 1: 실패하는 테스트 작성 (inner 로직)**

Modify `src-tauri/src/commands.rs` `#[cfg(test)] mod tests` 끝에 추가:

```rust
#[test]
fn memory_inner_add_list_delete() {
    use agent_mentor::store::SqliteStore;
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.add_memory("주인은 비건임", "manual").unwrap();
    let all = store.list_memories().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].text, "주인은 비건임");
    store.delete_memory(id).unwrap();
    assert_eq!(store.count_memories().unwrap(), 0);
}
```

(커맨드 자체는 `State<AppState>` 의존이라 단위 테스트가 어렵다 — Store 계약은 Task 1에서 검증되므로, 여기서는 커맨드가 Store 메서드로 위임함만 얇게 확인한다.)

- [ ] **Step 2: 커맨드 구현**

Modify `src-tauri/src/commands.rs` — `profile_set` 근처(약 `commands.rs:1483`) 다음에 추가. 상단에 이미 `use agent_mentor::memory::Memory;`가 없으면 추가:

```rust
#[tauri::command(async)]
pub fn memory_list(state: State<AppState>) -> Result<Vec<agent_mentor::memory::Memory>, String> {
    let guard = lock(&state)?;
    guard.list_memories().map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn memory_add(state: State<AppState>, text: String) -> Result<agent_mentor::memory::Memory, String> {
    let guard = lock(&state)?;
    let id = guard.add_memory(&text, "manual").map_err(|e| e.to_string())?;
    guard.list_memories().map_err(|e| e.to_string())?
        .into_iter().find(|m| m.id == id)
        .ok_or_else(|| "저장 직후 메모리를 찾지 못했어요".to_string())
}

#[tauri::command(async)]
pub fn memory_update(state: State<AppState>, id: i64, text: String) -> Result<agent_mentor::memory::Memory, String> {
    let guard = lock(&state)?;
    guard.update_memory(id, &text).map_err(|e| e.to_string())?;
    guard.list_memories().map_err(|e| e.to_string())?
        .into_iter().find(|m| m.id == id)
        .ok_or_else(|| "수정 대상 메모리를 찾지 못했어요".to_string())
}

#[tauri::command(async)]
pub fn memory_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    let guard = lock(&state)?;
    guard.delete_memory(id).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: 커맨드 등록**

Modify `src-tauri/src/lib.rs` — `tauri::generate_handler![` 목록(약 `lib.rs:374`, `profile_set` 다음)에 추가:

```rust
                commands::memory_list,
                commands::memory_add,
                commands::memory_update,
                commands::memory_delete,
```

- [ ] **Step 4: 테스트 통과 + 컴파일 확인 (Windows)**

Run: `cargo test -p agent-mentor-app memory_inner_add_list_delete`
Expected: PASS.
Run: `cargo build -p agent-mentor-app`
Expected: 컴파일 성공.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(app): add memory_list/add/update/delete commands"
```

---

## Task 9: 프론트엔드 API 래퍼

**Files:**
- Modify: `src/lib/api.ts`
- Test: `src/lib/api.test.ts` (없으면 생성) — 얇은 타입/호출 확인은 Task 10 컴포넌트 테스트에서 커버하므로 생략 가능. 여기서는 타입·래퍼만 추가.

**Interfaces:**
- Produces: `export interface Memory { id: number; text: string; created_at: string; updated_at: string | null; source: string }`
- Produces: `memoryList()`, `memoryAdd(text)`, `memoryUpdate(id, text)`, `memoryDelete(id)`

- [ ] **Step 1: 타입 + 래퍼 추가**

Modify `src/lib/api.ts` — `profileGet`/`profileSet`(약 `api.ts:295`) 근처에 추가:

```typescript
export interface Memory {
  id: number;
  text: string;
  created_at: string;
  updated_at: string | null;
  source: string;
}

export const memoryList = () => invoke<Memory[]>('memory_list');
export const memoryAdd = (text: string) => invoke<Memory>('memory_add', { text });
export const memoryUpdate = (id: number, text: string) => invoke<Memory>('memory_update', { id, text });
export const memoryDelete = (id: number) => invoke<void>('memory_delete', { id });
```

- [ ] **Step 2: 타입체크 (Windows)**

Run: `npm run check` (svelte-check/tsc — package.json 스크립트 확인 후) 또는 `npx tsc --noEmit`
Expected: 타입 에러 없음.

- [ ] **Step 3: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat(frontend): add Memory type and memory API wrappers"
```

---

## Task 10: "주인 메모리" 관리 UI

**Files:**
- Modify: `src/lib/ui/LifeSettingsTab.svelte`
- Test: `src/lib/ui/memory-card.test.ts` (신규, Vitest) — 로직 함수만 얇게. 렌더 테스트는 기존 컴포넌트 테스트 패턴을 따른다.

**Interfaces:**
- Consumes: `memoryList/Add/Update/Delete`, `Memory` (Task 9)

- [ ] **Step 1: 스크립트 로직 추가**

Modify `src/lib/ui/LifeSettingsTab.svelte` `<script>` — import에 memory API 추가하고 상태·핸들러 추가:

```typescript
  import { /* ...기존... */ memoryList, memoryAdd, memoryUpdate, memoryDelete, type Memory } from '../api';

  // --- 주인 메모리 ---
  let memories = $state<Memory[]>([]);
  let memInput = $state('');
  let memEditId = $state<number | null>(null);
  let memEditText = $state('');
  let memErr = $state('');
  async function loadMemories(){ try { memories = await memoryList(); } catch(e){ memErr = `${e}`; } }
  loadMemories();
  async function addMemory(){
    const t = memInput.trim(); if(!t) return;
    try { await memoryAdd(t); memInput=''; await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
  function startEdit(m: Memory){ memEditId = m.id; memEditText = m.text; }
  async function saveEdit(){
    if(memEditId==null) return;
    const t = memEditText.trim(); if(!t) return;
    try { await memoryUpdate(memEditId, t); memEditId=null; await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
  async function removeMemory(id: number){
    try { await memoryDelete(id); await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
```

- [ ] **Step 2: 마크업 추가**

Modify `src/lib/ui/LifeSettingsTab.svelte` — 마지막 `</section>` 직전(또는 "개인정보" 섹션 다음)에 카드 추가:

```svelte
<hr/><h2>주인 메모리</h2>
<p>마스코트가 기억할 나에 대한 사실이에요. 채팅에서 "기억해둬"라고 하거나 여기서 직접 추가할 수 있어요. (엔진으로 전송됩니다)</p>
<div class="memadd">
  <input type="text" bind:value={memInput} placeholder="예: 나는 비건이야" onkeydown={(e)=>{if(e.key==='Enter')addMemory();}} />
  <button class="save" onclick={addMemory}>추가</button>
</div>
<ul class="memlist">
  {#each memories as m (m.id)}
    <li>
      {#if memEditId===m.id}
        <input type="text" bind:value={memEditText} onkeydown={(e)=>{if(e.key==='Enter')saveEdit();}} />
        <button onclick={saveEdit}>저장</button>
        <button onclick={()=>memEditId=null}>취소</button>
      {:else}
        <span class="memtext">{m.text}</span>
        <span class="memdate">{m.created_at}</span>
        <button onclick={()=>startEdit(m)}>편집</button>
        <button onclick={()=>removeMemory(m.id)}>삭제</button>
      {/if}
    </li>
  {:else}
    <li class="memempty">아직 기억한 게 없어요.</li>
  {/each}
</ul>
{#if memErr}<p class="error">{memErr}</p>{/if}
```

- [ ] **Step 3: 스타일 추가**

Modify `src/lib/ui/LifeSettingsTab.svelte` `<style>` 끝에 추가:

```css
.memadd{display:flex;gap:8px;margin-top:12px}
.memadd input{flex:1;border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
.memlist{list-style:none;padding:0;margin:12px 0 0;display:flex;flex-direction:column;gap:6px}
.memlist li{display:flex;align-items:center;gap:8px;padding:8px 10px;background:var(--cream);color:var(--cream-ink);border-radius:8px}
.memlist .memtext{flex:1}
.memlist .memdate{font-size:11px;color:var(--text-soft)}
.memlist input{flex:1;border:1px solid var(--line);border-radius:6px;padding:6px 8px;background:var(--surface-inset);color:var(--text);font:inherit}
.memlist button{border:0;border-radius:99px;padding:5px 10px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font-size:12px}
.memempty{color:var(--text-soft);justify-content:center}
```

- [ ] **Step 4: 로직 테스트 작성 (선택, Vitest)**

기존 프론트 테스트 패턴이 컴포넌트 렌더 테스트를 쓰면 그 패턴을 따르고, 순수 로직이 거의 없으므로 최소한으로: 이 태스크는 수동 확인(아래 Step 5)이 주 검증이다.

- [ ] **Step 5: 수동 확인 (Windows)**

Run: `npm run tauri dev`
확인:
1. 설정 탭 → "주인 메모리" 카드에서 추가/편집/삭제가 동작.
2. 채팅에서 "나 비건이야, 기억해둬" → 마스코트가 "기억했어요!"류 응답 → 설정 카드에 새 메모리 표시(엔진이 tool-calling 지원 시).
3. 이어지는 채팅에서 관련 질문 시 메모리를 반영.

Run: `npm test`
Expected: 프론트 테스트 회귀 없음.

- [ ] **Step 6: Commit**

```bash
git add src/lib/ui/LifeSettingsTab.svelte src/lib/ui/memory-card.test.ts
git commit -m "feat(frontend): add owner-memory management card to settings"
```

---

## Task 11: 아카이브 (DoD)

- [ ] **Step 1: docs-archive 스킬 실행**

이 계획과 스펙이 구현 완료되면 같은 PR에서 `docs-archive` 스킬을 실행해 `docs/design/a-mate/specs/2026-07-22-owner-memory-design.md`와 `docs/design/a-mate/plans/2026-07-22-owner-memory.md`를 `docs/archive/` 미러로 이동한다 (ADR 0013).

- [ ] **Step 2: ADR 후보 판단**

전송 경계 완화(사용자 자유 텍스트가 엔진으로 전송)는 되돌리기 어려운 결정이므로, 채택 시 `docs/adr/NNNN-owner-memory-transmission-boundary.md` 작성을 검토한다.

---

## Self-Review

**Spec coverage:**
- 데이터 모델(§2) → Task 1 ✓
- 회상/주입(§3) → Task 2(헬퍼) + Task 5(채팅·코칭) + Task 6(일기) ✓
- 캡처 tool-loop(§4) → Task 3(프리미티브) + Task 4(루프) + Task 7(chat_send 연결) ✓
- graceful degrade(§4) → Task 3 기본 구현 + Task 4 폴백 테스트 ✓
- 관리 UI(§5) → Task 8(커맨드) + Task 9(API) + Task 10(UI) ✓
- 프라이버시/ADR(§6) → Task 11 ✓
- 테스트(§7) → 각 태스크 TDD + Task 10 수동 확인 ✓
- 범위 밖(§8) → 태그/TTL/자동추출/forget/랭킹 미포함 ✓

**Placeholder scan:** 모든 스텝에 실제 코드·명령·기대 결과 포함. "TBD/적절히" 없음.

**Type consistency:** `Memory`(id:i64/number, text, created_at, updated_at:Option/null, source) 일관. `run_memory_chat(engine, system, convo, max_rounds, on_save)` 시그니처 Task 4 정의 = Task 7 호출 일치. `ChatContext.memories`/`CoachingBrief.memories`/`build_system_prompt(...,memories)`/`build_idle_prompt(...,memories)` 일관. `chat_with_tools(system, messages, tools)` Task 3 정의 = Task 4 사용 일치.
