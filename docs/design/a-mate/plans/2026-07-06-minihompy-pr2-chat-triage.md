# 미니홈피 PR② — 채팅 탭 + triage 마감 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 채팅 탭(Engine 재사용 + 코칭 컨텍스트 주입)과 triage 잔여 5건(스캔 진행률 바, 파일 로그, 로컬 "오늘" 정책, content_protected 적용, allowScripts 정리)을 구현한다.

**Architecture:** 스펙 `docs/specs/2026-07-05-minihompy-restyle-design.md` §5·§7. 채팅은 다이어리와 같은 `Engine` 트레이트를 확장(`chat` 메서드)해 재사용하고, 시스템 프롬프트(페르소나+오늘 요약+활성 findings advice)는 core의 결정론 함수로 조립한다. 트랜스크립트 원문은 절대 전송하지 않는다. 날짜 버킷은 SQLite `'localtime'` 수정자 + `chrono::Local`로 로컬 자정 기준으로 통일하되, 절대 시각(finding first/last_seen, last_scan_ts)은 UTC RFC3339 유지.

**Tech Stack:** Rust(Tauri v2, rusqlite, ureq, chrono), tauri-plugin-log v2, Svelte 5(runes), vitest.

## Global Constraints

- Tauri **v2 전용**, Windows 전용. v1 API 금지 (프로젝트 CLAUDE.md).
- store를 만지는 커맨드는 반드시 `#[tauri::command(async)]` — 동기면 메인 스레드 프리즈.
- store 락을 잡은 채 네트워크 I/O 금지 — 컨텍스트 수집 후 락 해제, 그 다음 LLM 호출.
- UI 색/radius/그림자는 `src/lib/theme.css` 파스텔 토큰만 (`--pastel-lav`, `--frame-bg`, `--ink`, `--ink-soft`, `--accent`, `--radius-s/m/l`, `--shadow-soft`). 픽셀 폰트·도트 보더 금지.
- 전송 경계: 채팅에는 findings의 `detail`/`suggested_action`·집계 수치만 — 트랜스크립트 원문 미전송 (스펙 §5).
- 에러 철학: 말풍선으로 에러를 알리지 않음. `chat_send` 실패는 채팅 UI 인라인 (스펙 §9).
- **빌드 환경**: Rust가 PATH에 없고 MSVC 링커도 없음 — cargo를 실행하는 **모든 셸/서브에이전트에서** Git Bash 기준 아래를 먼저 실행:

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
```

- `git push`가 revocation 에러로 실패하면 `git -c http.schannelCheckRevoke=false push`.
- 시작 전 베이스라인: `cargo test --workspace` 전원 통과 + `npm run test` 통과 확인 후 진행 (main은 그린 상태).

---

### Task 1: 로컬 "오늘" 정책 — UTC → 로컬 자정 기준 통일

**Files:**
- Modify: `crates/core/src/store.rs` (SQL 3곳 + 테스트 1개 수정 + 신규 테스트 1개)
- Modify: `crates/core/src/diary/mod.rs` (`local_date_of` 헬퍼 신설 + anchor 사용처)
- Modify: `crates/core/src/main.rs:44`
- Modify: `src-tauri/src/commands.rs` (`Utc::now` 4곳 + anchor + 테스트 2곳)
- Modify: `src-tauri/src/pipeline.rs:117`

**Interfaces:**
- Consumes: 기존 `SqliteStore` 쿼리들.
- Produces: `agent_mentor::diary::local_date_of(ts: &str) -> Option<chrono::NaiveDate>` — 이후 태스크는 이 정책을 전제로 함(추가 시그니처 변경 없음).

**원칙:** 날짜 **버킷**(rollup date, "오늘" 판정, 달력 날짜)만 로컬로 바꾼다. 절대 **시각**(`ops.rs:104`의 finding seen ts, `pipeline.rs:81`의 `last_scan_ts`)은 UTC RFC3339 그대로 둔다.

- [ ] **Step 1: 실패하는 테스트 작성 — 로컬 날짜 버킷**

`crates/core/src/store.rs` 테스트 모듈에 추가 (기존 `sess_turn` 헬퍼 재사용):

```rust
    #[test]
    fn rollup_and_model_mix_bucket_by_local_date() {
        // UTC 자정 직전 이벤트 — 로컬 타임존(KST 등 동쪽)에선 다음날로 버킷돼야 한다.
        // 기대값을 chrono::Local로 계산하므로 머신 타임존과 무관하게 결정론적.
        let store = SqliteStore::open_in_memory().unwrap();
        let ts = "2026-07-01T23:30:00Z";
        store.upsert_events(&[sess_turn("Windows", "p1", "s1", "u1", ts)]).unwrap();
        store.rebuild_rollup().unwrap();

        let expected = chrono::DateTime::parse_from_rfc3339(ts).unwrap()
            .with_timezone(&chrono::Local).format("%Y-%m-%d").to_string();
        assert_eq!(store.summary_for_date(&expected).unwrap().session_count, 1);
        assert!(!store.model_mix_for_date(&expected).unwrap().is_empty());
    }
```

참고: `sess_turn`(store.rs 834행)은 `NormModel::from_raw_id("claude-opus-4-8")` 모델을 포함하므로 model_mix 단언이 그대로 성립한다.

`crates/core/src/diary/mod.rs` 테스트 모듈에 추가:

```rust
    #[test]
    fn local_date_of_converts_utc_and_falls_back() {
        let expected = chrono::DateTime::parse_from_rfc3339("2026-07-01T23:30:00Z").unwrap()
            .with_timezone(&chrono::Local).date_naive();
        assert_eq!(super::local_date_of("2026-07-01T23:30:00Z"), Some(expected));
        // RFC3339 파싱 불가 → 앞 10자(YYYY-MM-DD) 폴백
        assert_eq!(
            super::local_date_of("2026-07-01(비표준)"),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 1)
        );
        assert_eq!(super::local_date_of("junk"), None);
    }
```

- [ ] **Step 2: 실패 확인**

```bash
cargo test -p agent-mentor rollup_and_model_mix_bucket_by_local_date local_date_of -- --nocapture
```
기대: FAIL (`local_date_of` 미정의 컴파일 에러 → 함수 추가 전이므로 우선 store 테스트만 실패 확인해도 됨)

- [ ] **Step 3: 구현**

`crates/core/src/store.rs`:
- `rebuild_rollup` (173행): `date(ts) AS d` → `date(ts, 'localtime') AS d`
- `model_mix_for_date` (477행): `WHERE substr(ts,1,10)=?1` → `WHERE date(ts, 'localtime')=?1`
- `findings_for_date` (347행): `date(s.first_ts) = ?2` → `date(s.first_ts, 'localtime') = ?2`

`crates/core/src/diary/mod.rs`에 공개 헬퍼 추가 (`resolve_locale` 근처):

```rust
/// RFC3339 ts(UTC 포함)를 로컬 타임존 날짜로 변환. 파싱 실패 시 앞 10자(YYYY-MM-DD) 폴백.
/// "오늘" 정책: 날짜 버킷은 로컬 자정 기준 (스펙 §7).
pub fn local_date_of(ts: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
        .ok()
        .or_else(|| ts.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()))
}
```

같은 파일 `assemble_brief`의 anchor 계산(191–193행)을 헬퍼로 교체:

```rust
    let anchor = store
        .earliest_session_ts()?
        .and_then(|ts| local_date_of(&ts));
```

`crates/core/src/main.rs:44`: `chrono::Utc::now()` → `chrono::Local::now()`.

`src-tauri/src/commands.rs`:
- 23행(`summary_inner`), 125행(`today_occasions_inner`), 202행(`get_model_mix`): `chrono::Utc::now()` → `chrono::Local::now()`
- 94행(`week_summary_inner`): `chrono::Utc::now().date_naive()` → `chrono::Local::now().date_naive()`
- `today_occasions_inner`의 anchor(130–132행)를 헬퍼로 교체:

```rust
    let anchor = store.earliest_session_ts()?.and_then(|ts| {
        agent_mentor::diary::local_date_of(&ts)
    });
```

- 테스트 338행·400행: `chrono::Utc::now()` → `chrono::Local::now()`

`src-tauri/src/pipeline.rs:117`: `chrono::Utc::now()` → `chrono::Local::now()`.

- [ ] **Step 4: 전체 테스트 통과 확인**

```bash
cargo test --workspace
```
기대: 전원 PASS. 날짜가 하루 밀려 깨지는 픽스처가 있으면(자정 부근 UTC 시각) 해당 픽스처 시각을 `T01:00:00Z`–`T10:00:00Z` 범위로 옮기거나, 기대 날짜를 위 신규 테스트처럼 `chrono::Local` 변환으로 계산하도록 수정. 특히 `ingest_file_then_rollup_aggregates_tokens`(store.rs 827행)의 `"2026-07-01"` 리터럴은 다음처럼 계산값으로 교체:

```rust
        let expected = chrono::DateTime::parse_from_rfc3339("2026-07-01T10:00:00Z").unwrap()
            .with_timezone(&chrono::Local).format("%Y-%m-%d").to_string();
        let r = store.rollup_for("Windows", "c--users-jibin", &expected).unwrap().unwrap();
```

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/store.rs crates/core/src/diary/mod.rs crates/core/src/main.rs src-tauri/src/commands.rs src-tauri/src/pipeline.rs
git commit -m "feat(core): '오늘' 정책 UTC→로컬 자정 기준 통일 — rollup·model_mix·findings 날짜 버킷 localtime"
```

---

### Task 2: Engine `chat` 확장 + 채팅 시스템 프롬프트 (core)

**Files:**
- Modify: `crates/core/src/diary/engine.rs`
- Create: `crates/core/src/chat.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod chat;` 추가)

**Interfaces:**
- Consumes: 기존 `Engine` 트레이트, `EngineOutput`.
- Produces:
  - `agent_mentor::diary::engine::ChatMessage { pub role: String, pub content: String }` (Serialize+Deserialize+Clone+Debug)
  - `Engine::chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>` (트레이트 필수 메서드)
  - `agent_mentor::chat::ChatContext { user_name, date, session_count, tok_input, tok_output, est_tokens_saved_total, findings: Vec<(String, String)> }`
  - `agent_mentor::chat::build_chat_system_prompt(ctx: &ChatContext) -> String`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/src/diary/engine.rs` 테스트 모듈에 추가:

```rust
    #[test]
    fn mock_engine_chat_returns_canned_and_meters_tokens() {
        let eng = MockEngine { canned: "안녕 주인".into() };
        let msgs = vec![ChatMessage { role: "user".into(), content: "안녕?".into() }];
        let out = eng.chat("system prompt", &msgs).unwrap();
        assert_eq!(out.text, "안녕 주인");
        assert!(out.tokens_used > 0);
    }
```

`crates/core/src/chat.rs` 신규 파일 — 테스트 먼저 포함해 작성하되, Step 2에서 본문 미구현 상태로 실패를 확인하기 어렵다면 파일 전체(Step 3 코드)를 작성한 뒤 테스트만 먼저 돌려 RED→GREEN을 압축해도 된다(신규 모듈은 컴파일 단위라 순수 RED가 어려움 — 단언이 실제 동작을 검증하는지 눈으로 확인할 것):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_prompt_includes_persona_summary_and_findings() {
        let ctx = ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-06".into(),
            session_count: 3,
            tok_input: 100,
            tok_output: 200,
            est_tokens_saved_total: 4200,
            findings: vec![("playwright가 상주하는데 호출 0회".into(), "제거하면 아껴요".into())],
        };
        let p = build_chat_system_prompt(&ctx);
        assert!(p.contains("주인"));           // 페르소나 호칭
        assert!(p.contains("jibin"));          // 유저명
        assert!(p.contains("2026-07-06"));     // 오늘 날짜
        assert!(p.contains("3건"));            // 세션 수
        assert!(p.contains("4200"));           // 절약 가능 총량
        assert!(p.contains("playwright가 상주하는데 호출 0회")); // finding detail
        assert!(p.contains("→ 제거하면 아껴요"));                // suggested_action
        assert!(p.contains("지어내지 마세요")); // 정밀도의 선
    }

    #[test]
    fn chat_prompt_empty_findings_says_none() {
        let ctx = ChatContext {
            user_name: "u".into(), date: "2026-07-06".into(),
            session_count: 0, tok_input: 0, tok_output: 0,
            est_tokens_saved_total: 0, findings: vec![],
        };
        assert!(build_chat_system_prompt(&ctx).contains("활성 코칭 지적이 없어요"));
    }
}
```

- [ ] **Step 2: 실패 확인**

```bash
cargo test -p agent-mentor mock_engine_chat chat_prompt
```
기대: FAIL (`ChatMessage`/`chat`/모듈 미정의 컴파일 에러)

- [ ] **Step 3: 구현**

`crates/core/src/diary/engine.rs` — 상단 import에 serde 추가, 타입·트레이트 확장:

```rust
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>;
}
```

`MockEngine` impl에 추가:

```rust
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput> {
        let user_len: usize = messages.iter().map(|m| m.content.len()).sum();
        let approx = ((system.len() + user_len + self.canned.len()) / 4).max(1) as u64;
        Ok(EngineOutput { text: self.canned.clone(), tokens_used: approx })
    }
```

`OpenAiCompatEngine` — 요청 본문 조립을 공용 메서드로 추출하고 `generate`/`chat` 둘 다 위임:

```rust
impl OpenAiCompatEngine {
    // from_env는 기존 그대로

    fn request(&self, messages: serde_json::Value) -> Result<EngineOutput> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
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

impl Engine for OpenAiCompatEngine {
    fn name(&self) -> String {
        format!("openai-compat:{}", self.model)
    }

    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput> {
        self.request(serde_json::json!([
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]))
    }

    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput> {
        let mut arr = vec![serde_json::json!({"role": "system", "content": system})];
        arr.extend(messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})));
        self.request(serde_json::Value::Array(arr))
    }
}
```

`crates/core/src/chat.rs` 신규 (테스트는 Step 1 코드 그대로 포함):

```rust
//! 채팅 탭 시스템 프롬프트 — 다이어리 페르소나와 동일 인물, 결정론 조립 (스펙 §5).
//! 전송 경계: 트랜스크립트 원문은 절대 포함하지 않는다 — 집계 수치·advice 요약만.

pub struct ChatContext {
    pub user_name: String,
    pub date: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub est_tokens_saved_total: u64,
    /// (detail, suggested_action) — 활성 findings 상위 N개
    pub findings: Vec<(String, String)>,
}

pub fn build_chat_system_prompt(ctx: &ChatContext) -> String {
    let findings_block = if ctx.findings.is_empty() {
        "- (지금은 활성 코칭 지적이 없어요)".to_string()
    } else {
        ctx.findings
            .iter()
            .map(|(detail, action)| format!("- {detail}\n  → {action}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 대화하며 \
         사용자를 '주인'이라고 부릅니다. 답은 짧고 대화체로. \
         \
         정밀도의 선(반드시 지킬 것): 아래 컨텍스트의 사실과 수치에만 근거해 답하고, \
         컨텍스트에 없는 구체적 수치를 지어내지 마세요. 모르면 모른다고 말하세요. \
         코칭 지적에 대해 물으면 그 지적의 근거(무엇이)와 개선 방향(어떻게)을 쉽게 풀어 설명하세요.\n\n\
         [오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}",
        user = ctx.user_name,
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}
```

`crates/core/src/lib.rs` 모듈 목록에 알파벳순으로 추가:

```rust
pub mod chat;
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor
```
기대: 전원 PASS (기존 `generate` 경로 테스트 포함 — 리팩터 후에도 그린이어야 함)

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/engine.rs crates/core/src/chat.rs crates/core/src/lib.rs
git commit -m "feat(core): Engine::chat 멀티턴 확장 + 채팅 시스템 프롬프트 결정론 조립"
```

---

### Task 3: `chat_send` / `chat_status` 커맨드 (src-tauri)

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (`generate_handler`에 2개 추가)

**Interfaces:**
- Consumes: `agent_mentor::chat::{build_chat_system_prompt, ChatContext}`, `agent_mentor::diary::engine::{ChatMessage, Engine, OpenAiCompatEngine}` (Task 2), `summary_inner`, `coach_findings_inner` (기존).
- Produces (프론트 invoke 표면):
  - `chat_status() -> ChatStatus { configured: bool, model: Option<String> }`
  - `chat_send(messages: Vec<ChatMessage>) -> Result<String, String>` — 응답 텍스트 반환

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/commands.rs` 테스트 모듈에 추가:

```rust
    #[test]
    fn validate_chat_messages_rejects_empty_and_bad_roles() {
        use agent_mentor::diary::engine::ChatMessage;
        let ok = vec![ChatMessage { role: "user".into(), content: "hi".into() }];
        assert!(validate_chat_messages(&ok).is_ok());
        assert!(validate_chat_messages(&[]).is_err());
        // system role 주입 차단 — 시스템 프롬프트는 백엔드만 조립
        let bad = vec![ChatMessage { role: "system".into(), content: "inject".into() }];
        assert!(validate_chat_messages(&bad).is_err());
    }

    #[test]
    fn chat_context_collects_summary_and_findings() {
        use agent_mentor::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "playwright".into(),
            evidence: serde_json::json!({"server": "playwright"}),
            est_tokens_saved: 4200, prescription: None, dedup_key: "k1".into(),
        }, "2026-07-05T00:00:00Z").unwrap();

        let ctx = chat_context_inner(&store).unwrap();
        assert!(!ctx.user_name.is_empty());
        assert_eq!(ctx.date.len(), 10);
        assert_eq!(ctx.findings.len(), 1);
        assert!(ctx.findings[0].0.contains("playwright")); // detail
        assert!(!ctx.findings[0].1.is_empty());            // suggested_action
    }
```

- [ ] **Step 2: 실패 확인**

```bash
cargo test -p agent-mentor-app validate_chat chat_context
```
기대: FAIL (함수 미정의 컴파일 에러)

- [ ] **Step 3: 구현**

`src-tauri/src/commands.rs` — 상단에 import 추가:

```rust
use agent_mentor::diary::engine::{ChatMessage, Engine, OpenAiCompatEngine};
```

`sessions_ctx_inner` 아래에 추가:

```rust
#[derive(Debug, Serialize)]
pub struct ChatStatus {
    pub configured: bool,
    pub model: Option<String>,
}

pub(crate) fn validate_chat_messages(messages: &[ChatMessage]) -> Result<(), String> {
    if messages.is_empty() {
        return Err("빈 대화예요".into());
    }
    if !messages.iter().all(|m| m.role == "user" || m.role == "assistant") {
        // system 프롬프트는 백엔드만 조립 — 프론트發 role 주입 차단
        return Err("허용되지 않은 role이 있어요".into());
    }
    Ok(())
}

/// 채팅 시스템 프롬프트용 컨텍스트 — 요약 수치 + 활성 findings advice만 (전송 경계, 스펙 §5).
pub fn chat_context_inner(store: &SqliteStore) -> anyhow::Result<agent_mentor::chat::ChatContext> {
    let s = summary_inner(store)?;
    let findings = coach_findings_inner(store, false)?;
    Ok(agent_mentor::chat::ChatContext {
        user_name: s.user_name,
        date: s.date,
        session_count: s.session_count,
        tok_input: s.tok_input,
        tok_output: s.tok_output,
        est_tokens_saved_total: s.est_tokens_saved_total,
        findings: findings
            .into_iter()
            .take(10) // 프롬프트 크기 상한 — 절약 큰 순 정렬은 list_findings_current가 보장
            .map(|f| (f.detail, f.suggested_action))
            .collect(),
    })
}
```

커맨드 2개 (`run_scan_now` 위쪽 커맨드 구역에 추가):

```rust
#[tauri::command]
pub fn chat_status() -> ChatStatus {
    match OpenAiCompatEngine::from_env() {
        Some(e) => ChatStatus { configured: true, model: Some(e.model) },
        None => ChatStatus { configured: false, model: None },
    }
}

#[tauri::command(async)]
pub fn chat_send(state: State<AppState>, messages: Vec<ChatMessage>) -> Result<String, String> {
    validate_chat_messages(&messages)?;
    let Some(engine) = OpenAiCompatEngine::from_env() else {
        // UI는 chat_status로 사전 안내 — 여기는 방어선 (스펙 §5: 미설정은 에러가 아닌 안내)
        return Err("엔진이 설정되지 않았어요".into());
    };
    // 락 범위: 컨텍스트 수집만. 네트워크(LLM) 호출 전에 반드시 해제.
    let ctx = {
        let guard = lock(&state)?;
        chat_context_inner(&*guard).map_err(|e| e.to_string())?
    };
    let system = agent_mentor::chat::build_chat_system_prompt(&ctx);
    let recent = &messages[messages.len().saturating_sub(20)..]; // 이력 상한 20턴
    engine.chat(&system, recent).map(|o| o.text).map_err(|e| e.to_string())
}
```

`src-tauri/src/lib.rs`의 `generate_handler!` 목록 끝에 추가:

```rust
                commands::chat_status,
                commands::chat_send,
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test --workspace
```
기대: 전원 PASS

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(app): chat_send/chat_status 커맨드 — Engine 재사용, 락 밖 LLM 호출, role 주입 차단"
```

---

### Task 4: 채팅 탭 UI (ChatTab.svelte)

**Files:**
- Modify: `src/lib/api.ts`
- Create: `src/lib/ui/chat-store.svelte.ts`
- Create: `src/lib/ui/ChatTab.svelte`
- Modify: `src/App.svelte` (placeholder 교체)

**Interfaces:**
- Consumes: `chat_status`/`chat_send` invoke 표면 (Task 3).
- Produces: `ChatTab` 컴포넌트(프롭 없음), `chatState.messages` (창 수명 동안 유지되는 이력 — 탭 전환에도 보존).

- [ ] **Step 1: api.ts에 타입·함수 추가**

`src/lib/api.ts` — `SessionCtxItem` 인터페이스 아래에 추가:

```ts
export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface ChatStatus {
  configured: boolean;
  model: string | null;
}
```

invoke 함수 구역(`sessionsCtx` 아래)에 추가:

```ts
export const chatStatus = () => invoke<ChatStatus>('chat_status');
export const chatSend = (messages: ChatMessage[]) => invoke<string>('chat_send', { messages });
```

- [ ] **Step 2: 이력 스토어 작성**

`src/lib/ui/chat-store.svelte.ts` 신규:

```ts
// 채팅 이력 — 창 수명 동안 프론트 메모리에만 유지(탭 전환에도 보존, 영속화 없음 — 스펙 §5)
import type { ChatMessage } from '../api';

export const chatState = $state({ messages: [] as ChatMessage[] });
```

- [ ] **Step 3: ChatTab.svelte 작성**

`src/lib/ui/ChatTab.svelte` 신규:

```svelte
<script lang="ts">
  import { chatSend, chatStatus, type ChatStatus } from '../api';
  import { chatState } from './chat-store.svelte';

  let status = $state<ChatStatus | null>(null);
  let draft = $state('');
  let sending = $state(false);
  let error = $state<string | null>(null);
  let listEl = $state<HTMLElement | null>(null);

  chatStatus()
    .then((s) => (status = s))
    .catch(() => (status = { configured: false, model: null }));

  async function send() {
    const text = draft.trim();
    if (!text || sending) return;
    draft = '';
    error = null;
    chatState.messages.push({ role: 'user', content: text });
    sending = true;
    scrollBottom();
    try {
      const reply = await chatSend($state.snapshot(chatState.messages));
      chatState.messages.push({ role: 'assistant', content: reply });
    } catch (e) {
      error = String(e); // 인라인 표시 — 말풍선/토스트 아님 (스펙 §9)
    } finally {
      sending = false;
      scrollBottom();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    // isComposing: 한글 IME 조합 중 Enter로 전송되는 것 방지
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }

  function scrollBottom() {
    requestAnimationFrame(() => listEl?.scrollTo({ top: listEl.scrollHeight }));
  }
</script>

<section class="chat">
  {#if status && !status.configured}
    <div class="setup">
      <p>엔진이 아직 없어요, 주인. 다이어리와 같은 엔진을 써요.</p>
      <p>
        환경변수 <code>AGENT_MENTOR_ENGINE_URL</code>(필요 시
        <code>AGENT_MENTOR_ENGINE_KEY</code> · <code>AGENT_MENTOR_ENGINE_MODEL</code>)을
        설정하고 앱을 다시 시작하면 여기서 대화할 수 있어요.
      </p>
      <p class="fine">트랜스크립트 원문은 보내지 않아요 — 요약 수치와 코칭 지적만 참고해요.</p>
    </div>
  {:else}
    <div class="list" bind:this={listEl}>
      {#if chatState.messages.length === 0}
        <p class="hint">오늘 요약이나 코칭 지적에 대해 물어보세요. (예: “왜 playwright를 빼라는 거야?”)</p>
      {/if}
      {#each chatState.messages as m, i (i)}
        <div class="msg {m.role}">{m.content}</div>
      {/each}
      {#if sending}<div class="msg assistant pending">생각 중…</div>{/if}
      {#if error}<div class="msg fail">답장을 못 받았어요: {error}</div>{/if}
    </div>
    <div class="composer">
      <textarea
        rows="2"
        placeholder="주인, 뭐가 궁금해요? (Enter 전송 · Shift+Enter 줄바꿈)"
        bind:value={draft}
        onkeydown={onKeydown}
        disabled={sending}
      ></textarea>
      <button onclick={send} disabled={sending || !draft.trim()}>보내기</button>
    </div>
  {/if}
</section>

<style>
  .chat { flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 14px 16px; gap: 10px; }
  .setup {
    margin: auto; max-width: 420px; font-size: 13px; color: var(--ink);
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 18px 20px;
  }
  .setup .fine { color: var(--ink-soft); font-size: 12px; }
  .setup code { background: var(--pastel-lav); border-radius: 4px; padding: 1px 4px; }
  .list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding-right: 4px; }
  .hint { color: var(--ink-soft); font-size: 12px; margin: auto; }
  .msg {
    max-width: 78%; padding: 8px 12px; font-size: 13px; white-space: pre-wrap;
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft); line-height: 1.5;
  }
  .msg.user { align-self: flex-end; background: var(--pastel-lav); }
  .msg.assistant { align-self: flex-start; background: var(--frame-bg); }
  .msg.pending { color: var(--ink-soft); animation: blink 1.2s ease-in-out infinite; }
  .msg.fail { align-self: flex-start; background: var(--pastel-coral); }
  @keyframes blink { 50% { opacity: 0.4; } }
  .composer { display: flex; gap: 8px; align-items: flex-end; }
  .composer textarea {
    flex: 1; resize: none; font: inherit; font-size: 13px; color: var(--ink);
    border: 1px solid var(--pastel-lav); border-radius: var(--radius-s);
    padding: 8px 10px; background: var(--frame-bg); box-sizing: border-box;
  }
  .composer button {
    border: none; cursor: pointer; font: inherit; font-size: 13px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 9px 16px;
  }
  .composer button:disabled { opacity: 0.6; cursor: default; }
</style>
```

주의: `--pastel-coral` 등 토큰명이 `src/lib/theme.css`에 실제 존재하는지 확인하고, 없으면 존재하는 토큰으로 대체(신규 토큰 추가 금지).

- [ ] **Step 4: App.svelte placeholder 교체**

`src/App.svelte` — import 추가:

```ts
  import ChatTab from './lib/ui/ChatTab.svelte';
```

본문 분기 교체:

```svelte
        {:else}
          <ChatTab />
        {/if}
```

(82행의 `<section class="placeholder">…</section>` 제거. `.placeholder` CSS 규칙도 이 변경으로 고아가 되므로 함께 제거.)

- [ ] **Step 5: 검증**

```bash
npm run test && npm run build
```
기대: vitest 전원 PASS, vite build 성공(타입 에러 없음)

- [ ] **Step 6: 커밋**

```bash
git add src/lib/api.ts src/lib/ui/chat-store.svelte.ts src/lib/ui/ChatTab.svelte src/App.svelte
git commit -m "feat(ui): 채팅 탭 — 엔진 미설정 안내, IME 안전 전송, 인라인 에러, 이력 탭 전환 보존"
```

---

### Task 5: 스캔 진행률 바 — `scan:progress`

**Files:**
- Modify: `crates/core/src/ops.rs` (`run_ingest` 진행 콜백 분리 + 테스트)
- Modify: `src-tauri/src/pipeline.rs` (emit)
- Modify: `src/lib/api.ts`, `src/lib/ui/HomeTab.svelte`

**Interfaces:**
- Consumes: `ingest_file`, `enumerate_hosts`, `ClaudeCodeAdapter` (기존).
- Produces:
  - `agent_mentor::ops::run_ingest_with_progress(store: &SqliteStore, on_progress: &mut dyn FnMut(usize, usize)) -> Result<IngestReport>` — 콜백 인자 `(done, total)`
  - 기존 `run_ingest(store)`는 no-op 콜백 위임으로 시그니처 불변(CLI 무변경)
  - Tauri 이벤트 `scan:progress` payload `{ done: number, total: number }`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/src/ops.rs` 테스트 모듈에 추가 (fixture 라인은 store.rs의 `ingest_file_then_rollup_aggregates_tokens` 테스트와 동일 형식):

```rust
    #[test]
    fn ingest_all_reports_monotonic_progress() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let mut files = Vec::new();
        for (name, sid) in [("a.jsonl", "s1"), ("b.jsonl", "s2")] {
            let file = proj.join(name);
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"{sid}","uuid":"{sid}-u","timestamp":"2026-07-01T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":2}}}}}}"#).unwrap();
            files.push(file);
        }

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let work = vec![(adapter, files)];
        let mut seen = Vec::new();
        let mut report = IngestReport { files: 2, new_events: 0, warnings: Vec::new() };
        ingest_all(&store, &work, &mut report, &mut |done, total| seen.push((done, total)));

        assert_eq!(seen, vec![(1, 2), (2, 2)]); // 파일 단위 단조 증가
        assert_eq!(report.new_events, 2);
        assert!(report.warnings.is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

```bash
cargo test -p agent-mentor ingest_all_reports
```
기대: FAIL (`ingest_all` 미정의)

- [ ] **Step 3: 구현 — run_ingest 분해**

`crates/core/src/ops.rs`의 `run_ingest`를 다음으로 교체:

```rust
pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    run_ingest_with_progress(store, &mut |_, _| {})
}

/// 파일 단위 진행 콜백 `(done, total)` — Tauri 쪽에서 scan:progress emit에 사용 (스펙 §7).
pub fn run_ingest_with_progress(
    store: &SqliteStore,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<IngestReport> {
    let mut report = IngestReport { files: 0, new_events: 0, warnings: Vec::new() };
    // 1) 전 호스트 discover 먼저 — total을 알아야 진행률이 됨
    let mut work: Vec<(crate::adapter::ClaudeCodeAdapter, Vec<std::path::PathBuf>)> = Vec::new();
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                report.warnings.push(format!("host {} 파일 열거 실패: {e}", hs.host));
                Vec::new()
            }
        };
        report.files += files.len();
        work.push((adapter, files));
    }
    // 2) 파일 단위 수집 + 진행 보고
    ingest_all(store, &work, &mut report, on_progress);
    store.rebuild_rollup()?;
    Ok(report)
}

fn ingest_all(
    store: &SqliteStore,
    work: &[(crate::adapter::ClaudeCodeAdapter, Vec<std::path::PathBuf>)],
    report: &mut IngestReport,
    on_progress: &mut dyn FnMut(usize, usize),
) {
    let total: usize = work.iter().map(|(_, files)| files.len()).sum();
    let mut done = 0;
    for (adapter, files) in work {
        for f in files {
            match ingest_file(store, adapter, f) {
                Ok(n) => report.new_events += n,
                Err(e) => report.warnings.push(format!("{} 수집 실패(건너뜀): {e}", f.display())),
            }
            done += 1;
            on_progress(done, total);
        }
    }
}
```

참고: `discover()`는 `Result<Vec<PathBuf>>`를 반환한다(adapter.rs 8행) — 위 `work` 타입 표기가 그대로 성립한다.

- [ ] **Step 4: core 테스트 통과 확인**

```bash
cargo test -p agent-mentor
```
기대: 전원 PASS

- [ ] **Step 5: 파이프라인 emit + 프론트**

`src-tauri/src/pipeline.rs` — `run_pipeline_once` 내 `run_ingest` 호출(74행)을 교체:

```rust
                let report = agent_mentor::ops::run_ingest_with_progress(&store, &mut |done, total| {
                    // 파일 수천 개일 수 있어 5건 단위로만 emit (마지막은 항상)
                    if done == total || done % 5 == 0 {
                        let _ = app.emit("scan:progress", serde_json::json!({"done": done, "total": total}));
                    }
                })?;
```

`src/lib/api.ts` — 이벤트 구독 구역에 추가:

```ts
export interface ScanProgress {
  done: number;
  total: number;
}

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>('scan:progress', (e) => cb(e.payload));
```

`src/lib/ui/HomeTab.svelte` — import에 `onScanProgress, type ScanProgress` 추가, 상태 추가:

```ts
  let progress = $state<ScanProgress | null>(null);
```

기존 `$effect`(onScanDone만 구독)를 다음으로 교체:

```ts
  $effect(() => {
    const subs = [
      onScanProgress((p) => { scanning = true; progress = p; }),
      onScanDone(() => { scanning = false; progress = null; load(); }),
    ];
    return () => { subs.forEach((s) => s.then((u) => u())); };
  });
```

footer의 스캔 중 분기를 교체:

```svelte
    {#if scanning}
      <span class="scan-live">
        <span class="scanning">스캔 중…</span>
        {#if progress && progress.total > 0}
          <span class="bar"><span class="fill" style="width: {Math.min(100, Math.round((progress.done / progress.total) * 100))}%"></span></span>
          <span class="pct">{progress.done}/{progress.total}</span>
        {/if}
      </span>
    {:else if summary?.last_scan}
```

스타일 추가:

```css
  .scan-live { display: flex; align-items: center; gap: 8px; }
  .bar {
    width: 140px; height: 6px; border-radius: 999px;
    background: var(--pastel-lav); overflow: hidden; display: inline-block;
  }
  .fill { display: block; height: 100%; background: var(--accent); border-radius: 999px; transition: width 0.2s ease; }
  .pct { font-size: 11px; color: var(--ink-soft); }
```

- [ ] **Step 6: 검증**

```bash
cargo test --workspace && npm run test && npm run build
```
기대: 전원 PASS

- [ ] **Step 7: 커밋**

```bash
git add crates/core/src/ops.rs src-tauri/src/pipeline.rs src/lib/api.ts src/lib/ui/HomeTab.svelte
git commit -m "feat: 스캔 진행률 — run_ingest 파일 단위 콜백 분리 + scan:progress emit + 홈 상태줄 바"
```

---

### Task 6: 파일 로그 — tauri-plugin-log v2

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/pipeline.rs` (eprintln 전량 교체)

**Interfaces:**
- Consumes: 없음.
- Produces: `log::warn!`/`log::error!`/`log::info!` 사용 가능(이후 태스크가 사용). 로그 파일: `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log`.

**배경:** `windows_subsystem = "windows"` 빌드에선 stderr가 사라져 `eprintln!`이 관측 불가 (스펙 §7).

- [ ] **Step 1: 의존성 추가**

`src-tauri/Cargo.toml` `[dependencies]`에 추가:

```toml
tauri-plugin-log = "2"
log = "0.4"
```

- [ ] **Step 2: 플러그인 등록**

`src-tauri/src/lib.rs` — `tauri::Builder::default()` 바로 다음, autostart 플러그인 앞에 추가:

```rust
            .plugin(
                tauri_plugin_log::Builder::new()
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: Some("agent-mentor".into()),
                        }),
                    ])
                    .level(log::LevelFilter::Info)
                    .max_file_size(512_000)
                    .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                    .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                    .build(),
            )
```

setup 클로저 끝(`Ok(())` 직전)에 시작 로그 1줄:

```rust
                log::info!("Agent Mentor 시작 — 파이프라인·트레이 초기화 완료");
```

- [ ] **Step 3: eprintln 교체**

`src-tauri/src/pipeline.rs`의 `eprintln!` 14곳 전부 교체 — `warn:` 접두 메시지는 `log::warn!`으로(접두어 `warn: ` 제거), `pipeline error:` 는 `log::error!`로:

- 54행: `log::warn!("{} 감시 실패: {e}", projects.display());`
- 59행: `log::warn!("{} watcher 생성 실패: {e}", hs.host);`
- 75행: `for w in &report.warnings { log::warn!("{w}"); }`
- 76행: `for w in agent_mentor::ops::run_inventory(&mut store)? { log::warn!("{w}"); }`
- 99행: `log::error!("scan:done emit 실패: {e}");`
- 104행: `Err(e) => log::error!("pipeline error: {e}"),`
- 115·123·125·135·138·146·152·156행: `eprintln!("warn: ...")` → `log::warn!("...")` (형식 문자열·인자 유지)

`crates/core`(CLI `main.rs` 포함)는 건드리지 않는다 — 콘솔 앱이라 eprintln이 정상 관측됨.

- [ ] **Step 4: 검증**

```bash
cargo build -p agent-mentor-app && cargo test --workspace
```
기대: 빌드 성공(경고 없음), 테스트 전원 PASS. (파일 로그 실물 확인은 사용자 E2E 체크리스트로: 앱 실행 후 `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log` 존재·시작 로그 확인)

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/Cargo.toml Cargo.lock src-tauri/src/lib.rs src-tauri/src/pipeline.rs
git commit -m "feat(app): tauri-plugin-log 파일 로그 — windows_subsystem eprintln 소실 해소"
```

---

### Task 7: content_protected 실적용 + 트레이 토글

**Files:**
- Modify: `src-tauri/src/lib.rs` (헬퍼 + 시작 시 적용)
- Modify: `src-tauri/src/tray.rs` (토글 메뉴)
- Modify: `src-tauri/src/commands.rs` (`set_setting`에서 즉시 적용)

**Interfaces:**
- Consumes: 설정 키 `content_protected`("true"/"false", 기본 false), `log::warn!` (Task 6).
- Produces: `crate::apply_content_protection(app: &tauri::AppHandle, on: bool)` — chat·mascot 두 창에 `set_content_protected` 적용.

**참고(스코프 노트):** 스펙 §7은 "설정값을 창에 실제 적용"만 명시하나, 현재 이 설정을 바꿀 UI가 없어 E2E 검증이 불가능하므로 기존 트레이 토글 패턴(마스코트 표시/실시간 조언과 동일)으로 스위치를 함께 추가한다.

- [ ] **Step 1: 적용 헬퍼 + 시작 시 적용**

`src-tauri/src/lib.rs` — `AppState` 정의 아래에 추가:

```rust
/// content_protected 설정을 두 창(chat·mascot)에 적용 — 화면 캡처/녹화에서 제외 (스펙 §7).
pub(crate) fn apply_content_protection(app: &tauri::AppHandle, on: bool) {
    use tauri::Manager;
    for label in ["chat", "mascot"] {
        if let Some(w) = app.get_webview_window(label) {
            if let Err(e) = w.set_content_protected(on) {
                log::warn!("content_protected({label}) 적용 실패: {e}");
            }
        }
    }
}
```

setup 클로저의 mascot 창 블록 다음(시작 로그 앞)에 추가:

```rust
                // content_protected 설정을 시작 시 실제 적용 (스펙 §7)
                {
                    let state = app.state::<AppState>();
                    let on = {
                        let store = state.store.lock().map_err(|_| anyhow::anyhow!("store lock"))?;
                        store.get_setting("content_protected")?.map(|v| v == "true").unwrap_or(false)
                    };
                    if on {
                        apply_content_protection(app.handle(), true);
                    }
                }
```

- [ ] **Step 2: 트레이 토글**

`src-tauri/src/tray.rs` — `realtime` 항목 아래에 추가 (동일 패턴):

```rust
    let protect_on = {
        let state = app.state::<AppState>();
        let guard = state.store.lock().ok();
        guard
            .and_then(|s| s.get_setting("content_protected").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    };
    let protect = CheckMenuItem::with_id(app, "protect", "화면 캡처 보호", true, protect_on, None::<&str>)?;
```

메뉴 배열에 삽입(realtime과 scan 사이):

```rust
    let menu = Menu::with_items(app, &[&open, &mascot, &realtime, &protect, &scan, &autostart, &quit])?;
```

클론(핸들러 캡처용, `realtime_item` 옆):

```rust
    let protect_item = protect.clone();
```

`on_menu_event` match에 추가:

```rust
            "protect" => {
                // muda CheckMenuItem은 클릭 시 checked 자동 토글 — store를 소스오브트루스로.
                let next = {
                    let Ok(store) = app.state::<AppState>().store.lock() else { return };
                    let cur = store
                        .get_setting("content_protected")
                        .ok()
                        .flatten()
                        .map(|v| v == "true")
                        .unwrap_or(false);
                    let next = !cur;
                    let _ = store.set_setting("content_protected", if next { "true" } else { "false" });
                    next
                }; // 락 해제 후 창 적용
                let _ = protect_item.set_checked(next);
                crate::apply_content_protection(app, next);
            }
```

- [ ] **Step 3: set_setting 경유 변경도 즉시 적용**

`src-tauri/src/commands.rs`의 `set_setting`을 교체 (AppHandle 파라미터 추가 — 프론트 invoke 인자는 불변):

```rust
#[tauri::command(async)]
pub fn set_setting(app: tauri::AppHandle, state: State<AppState>, key: String, value: String) -> Result<(), String> {
    const ALLOWED: &[&str] = &["mascot_visible", "chatter_level", "content_protected", "mascot_pos", "realtime_advice", "last_advice_key"];
    if !ALLOWED.contains(&key.as_str()) {
        return Err(format!("허용되지 않은 설정 키: {key}"));
    }
    {
        let guard = lock(&state)?;
        guard.set_setting(&key, &value).map_err(|e| e.to_string())?;
    } // 락 해제 후 창 적용
    if key == "content_protected" {
        crate::apply_content_protection(&app, value == "true");
    }
    Ok(())
}
```

- [ ] **Step 4: 검증**

```bash
cargo test --workspace && cargo build -p agent-mentor-app
```
기대: 전원 PASS, 빌드 성공. (실제 캡처 제외 동작은 사용자 E2E: 트레이 토글 후 Win+Shift+S 캡처에서 창이 검게 나오는지 확인)

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/lib.rs src-tauri/src/tray.rs src-tauri/src/commands.rs
git commit -m "feat(app): content_protected 실적용 — 시작 시·트레이 토글·set_setting 경유 즉시 반영"
```

---

### Task 8: allowScripts 정리 + 전체 게이트

**Files:**
- Modify: `package.json`

**Interfaces:**
- Consumes/Produces: 없음 (정리 태스크).

- [ ] **Step 1: 비표준 필드 제거**

`package.json`에서 루트 `"allowScripts"` 블록(24–27행) 제거 — npm이 인식하지 않는 비표준 필드(스펙 §7). 마지막 프로퍼티의 트레일링 콤마 정리 주의:

```json
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5.0.0",
    "@tauri-apps/cli": "^2.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0",
    "vitest": "^2.0.0"
  }
}
```

- [ ] **Step 2: 전체 게이트**

```bash
npm install && npm run test && npm run build && cargo test --workspace
```
기대: install 정상(esbuild 포함), vitest·vite build·cargo 전원 PASS

- [ ] **Step 3: 커밋**

```bash
git add package.json package-lock.json
git commit -m "chore: package.json 비표준 allowScripts 필드 제거"
```

---

## 사용자 E2E 체크리스트 (자동 게이트 이후, PR 전 수동)

1. **채팅**: 엔진 env 없이 실행 → 채팅 탭에 설정 안내(에러 아님). `.env.ref` 참조로 env 설정 후 → 질문 전송 → 페르소나 답변, 코칭 지적 문답 가능, 한글 IME Enter 오발송 없음, 탭 전환 후 이력 유지.
2. **진행률 바**: "지금 스캔" 클릭 → 상태줄에 바+`n/m` 표시 → 완료 시 "마지막 스캔"으로 복귀.
3. **로그**: `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log` 생성·시작 로그 존재.
4. **로컬 "오늘"**: 홈 오늘 스트립·주간 추이 마지막 날·다이어리 날짜가 로컬 날짜와 일치(자정 직후가 이상적 검증 시점이나, 최소한 오늘 세션 수가 KST 기준으로 나오는지).
5. **캡처 보호**: 트레이 "화면 캡처 보호" 켬 → Win+Shift+S 캡처에서 chat·mascot 창이 검게/제외되는지, 끄면 정상 캡처.

## 실행 관례 (핸드오프 이관)

- 브랜치: `feat/minihompy-pr2-chat` (main에서 분기).
- SDD 원장: `.superpowers/sdd/progress.md`에 새 섹션. 태스크별 fresh 서브에이전트(sonnet) + 리뷰, **최종 whole-branch 리뷰는 상위 모델 1회**.
- PR 생성 → 봇 리뷰(gemini/Codex) 코드 대조 검증 → 픽스 → 인라인 답글 → 사용자 E2E가 최종 게이트.
