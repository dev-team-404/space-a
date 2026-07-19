---
status: done
archived: 2026-07-19
---

# 마스코트 '오늘의 한마디' Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 프로필 초상(`RobotPortrait`) 밑에, 마스코트가 오늘 하루를 한 문장(≤40자)으로 표현하는 상시 1줄을 둔다.

**Architecture:** `scan:done` 후 파이프라인이 오늘 사실(chat 컨텍스트)의 fingerprint가 바뀌었을 때만 한마디를 재생성해 날짜별 캐시(`daily_line` 테이블)에 저장한다. 사실 기반은 기존 `chat::ChatContext`, 자연스러움은 `diary::voice_guidance()`, 생성은 `Engine::generate`를 재사용한다. 생성 결정·프롬프트·정적 폴백은 테스트 가능한 core(`mascot.rs`)에 두고(diary `render_diary` 선례처럼 store 접근 없이 engine만), 락/네트워크 오케스트레이션은 app 파이프라인 런타임에 얇게 배선한다.

**Tech Stack:** Rust(core 크레이트 `agent-mentor`, app 크레이트 `agent-mentor-app`/Tauri v2), rusqlite, Svelte 5, TypeScript.

## Global Constraints

- **빌드(Windows Git Bash) — 매 cargo 호출 전 반드시 실행:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- 코어 테스트: `cargo test -p agent-mentor`. 프론트: `npm run build` + `npx vitest run`.
- 페르소나: **채팅과 동일 인물** — 1인칭, 주인 호칭, 능청. 짧은 한 문장(**≤40자**), 사실·수치에만 근거(정밀도의 선). 다이어리(과거 회고)와 별개인 오늘 라이브 1줄.
- **정적 폴백 문구(고정, verbatim):** `오늘은 널널하네. 근데 좀 심심;;;`
- **fingerprint 포맷(verbatim):** `"{session_count}|{tok_input}|{tok_output}|{findings.len()}"`.
- 폴백 정책: 엔진 미설정 → 캐시에 아무것도 안 씀(command가 `None` → 줄 숨김). 오늘 활동 있음 → LLM 생성. **활동 0건(`session_count==0`) → LLM 없이 정적 1줄 캐시.**
- **네트워크(LLM)는 store 락 밖**에서 호출(diary `render_diary` 선례).
- "오늘" = 로컬 자정 기준 날짜(`chrono::Local::now().format("%Y-%m-%d")`, 기존 `summary_inner`·`get_model_mix`와 동일).
- Tauri v2 API만. app crate 이름은 `agent_mentor::...`(core) / `crate::...`(app).
- 커밋은 이미 피처 브랜치(`feat/mascot-daily-line`) 위에서 진행. main 직접 커밋 금지.

## 착수 확인 결과 (구현 전 코드 검증 완료)

- `commands::chat_context_inner(store)`는 **오늘 기준**이다: `summary_inner`가 `chrono::Local::now()`로 오늘 날짜를 만들어 `summary_for_date`로 오늘 세션/입출력을 집계한다. `session_count==0`이 '오늘 활동 0건'에 정확히 대응 → 정적 폴백 트리거로 적합. (단 `ctx.findings`는 전체 활성 findings 상위 10개라 '오늘 한정'은 아니지만, 스펙 §2/§5가 chat 컨텍스트 재사용을 명시했으므로 그대로 사용. fingerprint의 `findings.len()`은 최대 10으로 캡되지만 세션/토큰이 함께 fp를 움직이므로 캐시 무효화에 충분.)
- `maybe_generate_diaries`는 [pipeline.rs](../../src-tauri/src/pipeline.rs)의 `#[cfg(not(test))] mod runtime` 안(`fn maybe_generate_diaries(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>)`)이고, `run_pipeline_once`가 `scan:done` emit 직후 호출한다. daily-line 훅은 그 **바로 다음 줄**에 배치. 이 runtime 모듈은 테스트 불가(`cfg(not(test))`)이므로 생성 결정 로직은 core에 두고 여기선 얇게 배선만 한다.
- 기존 `crates/core/src/mascot.rs`는 로봇 파츠 절차 생성(`RobotSpec`)이 들어있다. '오늘의 한마디' 함수들은 **이 파일에 추가**한다(스펙이 말한 "신규 파일"은 실질적으로 이 모듈에 마스코트 보이스를 묶는다는 뜻).

## File Structure

- **`crates/core/src/mascot.rs`** (수정): 오늘의 한마디 순수 조각 추가 — `STATIC_DAILY_LINE`/`static_daily_line()`, `facts_fingerprint(&ChatContext)`, `build_daily_line_prompt(&ChatContext)`, `compute_daily_line(&dyn Engine, &ChatContext, Option<&str>)`. store 접근 없음. (Task 2·3)
- **`crates/core/src/store.rs`** (수정): `daily_line(date PK, text, fingerprint)` 테이블 + `get_daily_line`/`upsert_daily_line`. (Task 1)
- **`src-tauri/src/pipeline.rs`** (수정): `runtime` 모듈에 `maybe_generate_daily_line(app, store_mutex)` + `run_pipeline_once` 배선. (Task 4)
- **`src-tauri/src/commands.rs`** (수정): `daily_line_inner(store, date)` + `#[tauri::command] get_daily_line(state)`. (Task 5)
- **`src-tauri/src/lib.rs`** (수정): `invoke_handler`에 `commands::get_daily_line` 등록. (Task 5)
- **`src/lib/api.ts`** (수정): `getDailyLine()` 바인딩. (Task 6)
- **`src/App.svelte`** (수정): `RobotPortrait` 밑 `<p class="daily-line">` + `$state` + `refresh()`에서 재조회. (Task 6)

---

### Task 1: `daily_line` 테이블 + get/upsert (core, store.rs)

**Files:**
- Modify: `crates/core/src/store.rs` (SCHEMA 상수 + `impl SqliteStore` + tests)

**Interfaces:**
- Consumes: 기존 `SqliteStore`, `rusqlite::{params, OptionalExtension}`(이미 import됨).
- Produces:
  - `pub fn get_daily_line(&self, date: &str) -> anyhow::Result<Option<(String, String)>>` — `(text, fingerprint)`, 없으면 `None`.
  - `pub fn upsert_daily_line(&self, date: &str, text: &str, fingerprint: &str) -> anyhow::Result<()>`.
  - 신규 테이블 `daily_line(date TEXT PRIMARY KEY, text TEXT, fingerprint TEXT)`.

- [ ] **Step 1: 실패하는 테스트 추가**

`crates/core/src/store.rs`의 `#[cfg(test)] mod tests { ... }` 안(다른 `#[test]` 옆)에 추가:

```rust
    #[test]
    fn daily_line_roundtrip_and_upsert_overwrites() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 없으면 None
        assert_eq!(store.get_daily_line("2026-07-08").unwrap(), None);
        // upsert 후 (text, fp) 라운드트립
        store.upsert_daily_line("2026-07-08", "오늘 좀 굴렀다.", "3|100|200|1").unwrap();
        assert_eq!(
            store.get_daily_line("2026-07-08").unwrap(),
            Some(("오늘 좀 굴렀다.".to_string(), "3|100|200|1".to_string()))
        );
        // 같은 날짜 재upsert → text·fp 덮어씀
        store.upsert_daily_line("2026-07-08", "생각보다 바빴네.", "5|300|400|2").unwrap();
        assert_eq!(
            store.get_daily_line("2026-07-08").unwrap(),
            Some(("생각보다 바빴네.".to_string(), "5|300|400|2".to_string()))
        );
        // 다른 날짜는 독립적으로 None
        assert_eq!(store.get_daily_line("2099-01-01").unwrap(), None);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor store::tests::daily_line_roundtrip_and_upsert_overwrites
```
Expected: 컴파일 FAIL — `no method named `get_daily_line` / `upsert_daily_line` found`.

- [ ] **Step 3: SCHEMA에 테이블 추가**

`crates/core/src/store.rs`의 `const SCHEMA: &str = r#" ... "#;` 안, `settings` 테이블 정의 **다음**(닫는 `"#` 앞)에 추가:

```sql
CREATE TABLE IF NOT EXISTS daily_line (
  date TEXT PRIMARY KEY, text TEXT NOT NULL, fingerprint TEXT NOT NULL
);
```

(`migrate()`는 손대지 않는다 — 신규 테이블은 `CREATE TABLE IF NOT EXISTS`라 기존 DB에도 다음 `open` 시 자동 생성된다. `diary_index` 등 기존 테이블과 동일 패턴.)

- [ ] **Step 4: get/upsert 메서드 구현**

`impl SqliteStore { ... }` 안, `get_setting`/`set_setting` 근처에 추가:

```rust
    pub fn get_daily_line(&self, date: &str) -> Result<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT text, fingerprint FROM daily_line WHERE date=?1",
                params![date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_daily_line(&self, date: &str, text: &str, fingerprint: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO daily_line (date, text, fingerprint) VALUES (?1,?2,?3)
             ON CONFLICT(date) DO UPDATE SET text=?2, fingerprint=?3",
            params![date, text, fingerprint],
        )?;
        Ok(())
    }
```

- [ ] **Step 5: 테스트 통과 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor store::tests::daily_line_roundtrip_and_upsert_overwrites
```
Expected: PASS.

- [ ] **Step 6: 커밋**

```bash
git add crates/core/src/store.rs
git commit -m "feat(mascot): daily_line 테이블 + get/upsert_daily_line (오늘의 한마디 캐시)"
```

---

### Task 2: 한마디 순수 조각 — static/fingerprint/prompt (core, mascot.rs)

**Files:**
- Modify: `crates/core/src/mascot.rs` (파일 하단에 추가 + tests 모듈 신설)

**Interfaces:**
- Consumes: `crate::chat::ChatContext`(필드: `user_name, date, session_count, tok_input, tok_output, est_tokens_saved_total, findings: Vec<(String,String)>`), `crate::diary::voice_guidance() -> &'static str`.
- Produces:
  - `pub const STATIC_DAILY_LINE: &str` / `pub fn static_daily_line() -> &'static str`.
  - `pub fn facts_fingerprint(ctx: &ChatContext) -> String`.
  - `pub fn build_daily_line_prompt(ctx: &ChatContext) -> String`.

- [ ] **Step 1: 실패하는 테스트 추가**

`crates/core/src/mascot.rs` 맨 아래(기존 `mod tests` 뒤)에 새 테스트 모듈을 추가한다. (기존 `RobotSpec` 테스트 모듈 `tests`는 그대로 두고, 이름 충돌을 피해 `daily_line_tests`로 신설.)

```rust
#[cfg(test)]
mod daily_line_tests {
    use super::*;
    use crate::chat::ChatContext;

    fn ctx(session_count: u64, tin: u64, tout: u64, n_findings: usize) -> ChatContext {
        ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-08".into(),
            session_count,
            tok_input: tin,
            tok_output: tout,
            est_tokens_saved_total: 4200,
            findings: (0..n_findings)
                .map(|i| (format!("detail {i}"), format!("action {i}")))
                .collect(),
        }
    }

    #[test]
    fn static_daily_line_is_fixed_idle_copy() {
        assert_eq!(static_daily_line(), "오늘은 널널하네. 근데 좀 심심;;;");
    }

    #[test]
    fn fingerprint_reflects_facts_and_is_stable() {
        assert_eq!(facts_fingerprint(&ctx(3, 100, 200, 1)), "3|100|200|1");
        // 같은 사실 → 같은 fp
        assert_eq!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(3, 100, 200, 1)));
        // 세션 수 / findings 수가 바뀌면 fp 달라짐
        assert_ne!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(4, 100, 200, 1)));
        assert_ne!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(3, 100, 200, 2)));
    }

    #[test]
    fn prompt_carries_persona_facts_voice_and_one_line_directive() {
        let p = build_daily_line_prompt(&ctx(3, 100, 200, 1));
        assert!(p.contains("주인"));                         // 페르소나 호칭
        assert!(p.contains("jibin"));                        // 유저명
        assert!(p.contains("3건"));                          // 오늘 세션 수(사실)
        assert!(p.contains(crate::diary::voice_guidance())); // voice_guidance 그대로 주입
        assert!(p.contains("한 문장"));                      // 한 문장 지시
        assert!(p.contains("40자"));                         // 길이 상한
        assert!(p.contains("지어내지 마세요"));              // 정밀도의 선
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor mascot::daily_line_tests
```
Expected: 컴파일 FAIL — `cannot find function `static_daily_line` / `facts_fingerprint` / `build_daily_line_prompt` in this scope`.

- [ ] **Step 3: 순수 조각 구현**

`crates/core/src/mascot.rs`의 기존 `robot_spec_for` 함수 **다음**, `#[cfg(test)] mod tests` **앞**에 추가:

```rust
// ──────────────────────────────────────────────────────────────────────────
// 오늘의 한마디 (마스코트 보이스 #1) — 채팅과 동일 인물이 오늘 하루를 한 문장으로.
// 사실 기반은 chat::ChatContext(오늘 요약), 자연스러움은 diary::voice_guidance() 재사용.
// #3 상주봇 주기 말풍선이 나중에 이 프롬프트/정적 문구를 재사용한다.

/// 오늘 활동 0건일 때 LLM 없이 캐시하는 고정 폴백 문구 (스펙 §2).
pub const STATIC_DAILY_LINE: &str = "오늘은 널널하네. 근데 좀 심심;;;";

pub fn static_daily_line() -> &'static str {
    STATIC_DAILY_LINE
}

/// 사실 지문 — 이 값이 바뀌었거나 캐시가 없을 때만 한마디를 재생성한다 (스펙 §2·§3).
pub fn facts_fingerprint(ctx: &crate::chat::ChatContext) -> String {
    format!(
        "{}|{}|{}|{}",
        ctx.session_count,
        ctx.tok_input,
        ctx.tok_output,
        ctx.findings.len()
    )
}

/// 오늘의 한마디 생성 시스템 프롬프트 — 채팅과 동일 인물(1인칭·주인·능청) +
/// 오늘 요약(chat과 같은 사실 블록) + voice_guidance + "짧은 한 문장" 지시 (스펙 §5).
pub fn build_daily_line_prompt(ctx: &crate::chat::ChatContext) -> String {
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
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         [오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}\n\n\
         오늘 하루의 기분이나 재치를 담아 짧은 한 문장(40자 이내)으로 표현하세요. \
         대화가 아니라 오늘을 한마디로 요약하는 혼잣말입니다. 딱 한 문장만 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor mascot::daily_line_tests
```
Expected: PASS (3개: `static_daily_line_is_fixed_idle_copy`, `fingerprint_reflects_facts_and_is_stable`, `prompt_carries_persona_facts_voice_and_one_line_directive`).

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/mascot.rs
git commit -m "feat(mascot): 한마디 정적 문구·fingerprint·프롬프트 조립 (chat 컨텍스트+voice_guidance 재사용)"
```

---

### Task 3: `compute_daily_line` 오케스트레이션 결정 (core, mascot.rs)

**Files:**
- Modify: `crates/core/src/mascot.rs` (`build_daily_line_prompt` 다음에 함수 추가 + `daily_line_tests`에 테스트 추가)

**Interfaces:**
- Consumes: `crate::diary::engine::Engine`(`generate(&self, system, user) -> Result<EngineOutput{ text, tokens_used }>`), `crate::diary::engine::MockEngine{ canned: String }`(테스트), Task 2의 `facts_fingerprint`/`static_daily_line`/`build_daily_line_prompt`.
- Produces:
  - `pub fn compute_daily_line(engine: &dyn Engine, ctx: &ChatContext, cached_fp: Option<&str>) -> anyhow::Result<Option<(String, String)>>` — `None` = 재생성 불필요(skip), `Some((text, fp))` = 이 값으로 캐시하라. store 접근 없음, 네트워크만(diary `render_diary` 선례) → 호출자가 락 밖에서 부른다.

- [ ] **Step 1: 실패하는 테스트 추가**

`crates/core/src/mascot.rs`의 `mod daily_line_tests` 안(Task 2 테스트 옆)에 추가:

```rust
    use crate::diary::engine::MockEngine;

    #[test]
    fn compute_generates_when_active_and_uncached() {
        let eng = MockEngine { canned: "오늘 주인이 나를 꽤 굴렸다".into() };
        let out = compute_daily_line(&eng, &ctx(3, 100, 200, 1), None).unwrap();
        assert_eq!(
            out,
            Some(("오늘 주인이 나를 꽤 굴렸다".to_string(), "3|100|200|1".to_string()))
        );
    }

    #[test]
    fn compute_skips_when_fingerprint_matches_cache() {
        let eng = MockEngine { canned: "안 나와야 함".into() };
        let c = ctx(3, 100, 200, 1);
        let fp = facts_fingerprint(&c);
        assert_eq!(compute_daily_line(&eng, &c, Some(&fp)).unwrap(), None);
    }

    #[test]
    fn compute_uses_static_line_without_engine_when_idle() {
        // 활동 0건 → 엔진을 부르지 않고 정적 문구. canned(≠정적)가 나오면 엔진이 호출됐다는 뜻이라 실패.
        let eng = MockEngine { canned: "엔진이 불렸다면 이게 나온다".into() };
        let out = compute_daily_line(&eng, &ctx(0, 0, 0, 2), None).unwrap();
        assert_eq!(
            out,
            Some(("오늘은 널널하네. 근데 좀 심심;;;".to_string(), "0|0|0|2".to_string()))
        );
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor mascot::daily_line_tests::compute
```
Expected: 컴파일 FAIL — `cannot find function `compute_daily_line` in this scope`.

- [ ] **Step 3: `compute_daily_line` 구현**

`crates/core/src/mascot.rs`의 `build_daily_line_prompt` **다음**, `#[cfg(test)]` **앞**에 추가:

```rust
/// 오늘의 한마디 계산 — store 접근 없음, 네트워크만. 호출자가 락 밖에서 부른다
/// (diary::render_diary 선례). 반환: None=재생성 불필요(fp 동일, skip) /
/// Some((text, fp))=이 값으로 캐시하라.
/// - 오늘 활동 0건(session_count==0): 엔진 호출 없이 정적 문구.
/// - fp가 캐시와 동일: None(skip).
/// - 그 외: 엔진으로 오늘 한 문장 생성.
pub fn compute_daily_line(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(String, String)>> {
    let fp = facts_fingerprint(ctx);
    if ctx.session_count == 0 {
        return Ok(Some((static_daily_line().to_string(), fp)));
    }
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    let system = build_daily_line_prompt(ctx);
    let text = engine.generate(&system, "")?.text;
    Ok(Some((text, fp)))
}
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor mascot::daily_line_tests
```
Expected: PASS (6개 — Task 2의 3개 + `compute_generates_when_active_and_uncached`, `compute_skips_when_fingerprint_matches_cache`, `compute_uses_static_line_without_engine_when_idle`).

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/mascot.rs
git commit -m "feat(mascot): compute_daily_line — 0건 정적/fp skip/생성 결정 (MockEngine 테스트)"
```

---

### Task 4: 파이프라인 배선 `maybe_generate_daily_line` (app, pipeline.rs)

**Files:**
- Modify: `src-tauri/src/pipeline.rs` (`#[cfg(not(test))] mod runtime` 안 — 함수 추가 + `run_pipeline_once` 배선)

**Interfaces:**
- Consumes: `agent_mentor::mascot::compute_daily_line`(Task 3), `SqliteStore::{get_daily_line, upsert_daily_line}`(Task 1), `crate::commands::chat_context_inner`(기존, `pub fn`), `agent_mentor::diary::engine::OpenAiCompatEngine`(이미 import됨), `AppHandle`.
- Produces: `fn maybe_generate_daily_line(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>)` — 엔진 게이트(없으면 no-op) + 락 짧게(ctx·cached_fp 읽기 / upsert) + 네트워크는 락 밖.

**참고 — 이 태스크는 유닛테스트 대상이 아니다.** `mod runtime`은 `#[cfg(not(test))]`라 테스트에서 컴파일되지 않는다(기존 `maybe_generate_diaries`와 동일). 생성 결정 로직은 Task 3의 `compute_daily_line`이 이미 커버하므로 여기선 락/네트워크 배선만 하고 **`cargo build`(컴파일) + 수동 E2E**로 검증한다.

- [ ] **Step 1: `maybe_generate_daily_line` 구현**

`src-tauri/src/pipeline.rs`의 `mod runtime` 안, 기존 `fn maybe_generate_diaries(...) { ... }` **다음**에 추가:

```rust
    /// 오늘의 한마디 — scan:done마다 fp가 stale할 때만 재생성(스펙 §3). 엔진 없으면 no-op.
    /// 네트워크(LLM)는 diary와 동일하게 store 락 밖에서 호출.
    fn maybe_generate_daily_line(
        app: &AppHandle,
        store_mutex: &std::sync::Mutex<SqliteStore>,
    ) {
        let Some(engine) = OpenAiCompatEngine::from_env() else { return; };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // ① 락: 오늘 컨텍스트 + 캐시된 fingerprint 읽기 → 즉시 해제
        let (ctx, cached_fp) = match store_mutex.lock() {
            Ok(store) => {
                let ctx = match crate::commands::chat_context_inner(&store) {
                    Ok(c) => c,
                    Err(e) => { log::warn!("daily-line chat_context 실패: {e}"); return; }
                };
                let cached_fp = match store.get_daily_line(&today) {
                    Ok(v) => v.map(|(_, fp)| fp),
                    Err(e) => { log::warn!("get_daily_line 실패: {e}"); return; }
                };
                (ctx, cached_fp)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // ② 락 없이 compute (0건 정적 or 네트워크 생성). None이면 skip.
        let outcome = match agent_mentor::mascot::compute_daily_line(&engine, &ctx, cached_fp.as_deref()) {
            Ok(o) => o,
            Err(e) => { log::warn!("compute_daily_line 실패: {e}"); return; }
        };
        let Some((text, fp)) = outcome else { return; };

        // ③ 락: 캐시 upsert → 즉시 해제
        match store_mutex.lock() {
            Ok(store) => {
                if let Err(e) = store.upsert_daily_line(&today, &text, &fp) {
                    log::warn!("upsert_daily_line 실패: {e}");
                }
            }
            Err(e) => log::warn!("store lock poisoned: {e}"),
        }
    }
```

`use agent_mentor::store::SqliteStore;`는 이미 `mod runtime` 상단에 있고 `OpenAiCompatEngine`도 이미 import돼 있다(추가 import 불필요).

- [ ] **Step 2: `run_pipeline_once`에 배선**

`src-tauri/src/pipeline.rs`의 `run_pipeline_once` 안, `scan:done` emit 성공 경로에서 `maybe_generate_diaries(app, &state.store);` **다음 줄**에 추가:

```rust
                // 다이어리 실패는 조용히 — 다음 사이클에서 재시도
                maybe_generate_diaries(app, &state.store);
                // 오늘의 한마디 — 엔진 없으면 no-op, 실패는 조용히(다음 스캔 재시도)
                maybe_generate_daily_line(app, &state.store);
```

- [ ] **Step 3: 컴파일 검증**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo build -p agent-mentor-app
```
Expected: 컴파일 성공, 경고 없음. (런타임 동작은 Task 6 이후 수동 E2E에서 확인.)

- [ ] **Step 4: 커밋**

```bash
git add src-tauri/src/pipeline.rs
git commit -m "feat(mascot): scan:done 파이프라인에 daily-line 생성 훅 배선 (네트워크 락 밖)"
```

---

### Task 5: `get_daily_line` command + 등록 (app, commands.rs·lib.rs)

**Files:**
- Modify: `src-tauri/src/commands.rs` (inner + command + tests)
- Modify: `src-tauri/src/lib.rs` (`invoke_handler` 등록)

**Interfaces:**
- Consumes: `SqliteStore::get_daily_line`(Task 1), 기존 `lock`/`AppState`/`State`.
- Produces:
  - `pub fn daily_line_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Option<String>>` — 캐시된 text만(fp 버림).
  - `#[tauri::command(async)] pub fn get_daily_line(state: State<AppState>) -> Result<Option<String>, String>` — 오늘 날짜로 조회.

- [ ] **Step 1: 실패하는 테스트 추가**

`src-tauri/src/commands.rs`의 `#[cfg(test)] mod tests { ... }` 안(다른 `_inner` 테스트 옆)에 추가:

```rust
    #[test]
    fn daily_line_inner_returns_cached_text_or_none() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(daily_line_inner(&store, "2026-07-08").unwrap(), None);
        store.upsert_daily_line("2026-07-08", "오늘 좀 굴렀다.", "3|1|2|0").unwrap();
        assert_eq!(
            daily_line_inner(&store, "2026-07-08").unwrap(),
            Some("오늘 좀 굴렀다.".to_string())
        );
        assert_eq!(daily_line_inner(&store, "2099-01-01").unwrap(), None);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor-app daily_line_inner_returns_cached_text_or_none
```
Expected: 컴파일 FAIL — `cannot find function `daily_line_inner` in this scope`.

- [ ] **Step 3: inner + command 구현**

`src-tauri/src/commands.rs`에서, 기존 `diary_inner` **다음**(자연스러운 위치)에 inner를 추가:

```rust
pub fn daily_line_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Option<String>> {
    Ok(store.get_daily_line(date)?.map(|(text, _fp)| text))
}
```

그리고 command를 기존 `get_diary` command **다음**에 추가:

```rust
#[tauri::command(async)]
pub fn get_daily_line(state: State<AppState>) -> Result<Option<String>, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let guard = lock(&state)?;
    daily_line_inner(&*guard, &today).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test -p agent-mentor-app daily_line_inner_returns_cached_text_or_none
```
Expected: PASS.

- [ ] **Step 5: `invoke_handler`에 등록**

`src-tauri/src/lib.rs`의 `tauri::generate_handler![ ... ]` 목록 안, `commands::get_diary,` **다음**에 추가:

```rust
                commands::get_diary,
                commands::get_daily_line,
```

- [ ] **Step 6: 앱 전체 컴파일 확인**

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo build -p agent-mentor-app
```
Expected: 컴파일 성공(핸들러 등록 반영).

- [ ] **Step 7: 커밋**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(mascot): get_daily_line 커맨드 + 등록 (오늘 캐시 조회, 없으면 None)"
```

---

### Task 6: 프론트 — api 바인딩 + 초상 밑 표시 (frontend)

**Files:**
- Modify: `src/lib/api.ts` (바인딩 추가)
- Modify: `src/App.svelte` (`$state` + `refresh()` 재조회 + 마크업 + 스타일)

**Interfaces:**
- Consumes: `get_daily_line` command(Task 5), 기존 `invoke`, 기존 `onScanDone(() => refresh())`([App.svelte:33](../../src/App.svelte#L33)).
- Produces: `getDailyLine(): Promise<string | null>`; App 사이드바 `.profile`의 `RobotPortrait` 밑 `<p class="daily-line">`(값 없으면 미렌더).

**참고 — 신규 vitest 없음.** 이 레포엔 Svelte 컴포넌트 렌더 테스트 인프라(@testing-library/svelte)가 없고 모든 `.test.ts`는 순수 로직이다. daily-line 표시는 `{#if dailyLine}` 마크업 한 줄이라 순수 로직이 없다 → `npm run build`(타입/컴파일) + 기존 `npx vitest run` 그린 유지 + 수동 E2E로 검증(스펙 §6 "vitest 가능 범위").

- [ ] **Step 1: api.ts 바인딩 추가**

`src/lib/api.ts`에서 `getDiary` 바인딩 **다음**([api.ts:96](../../src/lib/api.ts#L96))에 추가:

```ts
export const getDailyLine = () => invoke<string | null>('get_daily_line');
```

- [ ] **Step 2: App.svelte — import·state·refresh**

`src/App.svelte` `<script>`의 import 목록에서 `getSummary`가 포함된 `from './lib/api'` 블록에 `getDailyLine`을 추가:

```ts
  import {
    getSummary, getDailyLine, listFindings, onScanDone, onGotoTab,
    onNewFindings, onDiaryReady, onOccasionToday, type Summary,
  } from './lib/api';
```

`summary` state 선언 **다음**([App.svelte:23](../../src/App.svelte#L23))에 추가:

```ts
  let dailyLine = $state<string | null>(null);
```

기존 `refresh()` 함수([App.svelte:28-31](../../src/App.svelte#L28-L31)) 안에 한 줄 추가(기존 `onScanDone(() => refresh())`가 scan:done마다 이걸 다시 부른다 — 별도 리스너 불필요):

```ts
  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
    dailyLine = await getDailyLine().catch(() => null);
  }
```

- [ ] **Step 3: App.svelte — 초상 밑 마크업**

`<aside class="profile">`([App.svelte:71-74](../../src/App.svelte#L71-L74))에서 `<RobotPortrait />` 밑, `<p class="mood">` 위에 추가:

```svelte
      <aside class="profile">
        <RobotPortrait />
        {#if dailyLine}<p class="daily-line">“{dailyLine}”</p>{/if}
        <p class="mood">“{mood}”</p>
      </aside>
```

- [ ] **Step 4: App.svelte — 스타일**

`<style>`의 `.mood { ... }` 규칙([App.svelte:127](../../src/App.svelte#L127)) **다음**에 추가(테마 토큰만 사용):

```css
  .daily-line { margin: 0; font-size: 13px; color: var(--ink); text-align: center; line-height: 1.45; }
```

- [ ] **Step 5: 프론트 빌드·테스트 확인**

```bash
npm run build
npx vitest run
```
Expected: `npm run build` 타입/컴파일 성공(에러 없음), `npx vitest run` 기존 테스트 전부 그린(신규 실패 없음).

- [ ] **Step 6: 커밋**

```bash
git add src/lib/api.ts src/App.svelte
git commit -m "feat(mascot): 프로필 초상 밑 '오늘의 한마디' 상시 1줄 (없으면 미표시)"
```

---

## 수동 E2E (전체 태스크 후, 사용자 확인)

스펙 §6 수동 항목 — 코드로 검증 불가한 부분:
1. `.env`(localhost:4444) 실엔진으로 앱 dev 실행 → 스캔 후 초상 밑 오늘 한마디 표시. 자연스러움·주인 호칭·≤40자·오늘 사실 정합 육안.
2. 오늘 활동 0건인 상태(빈/신규 DB)에서 정적 문구 `오늘은 널널하네. 근데 좀 심심;;;` 표시 확인.
3. 엔진 미설정(`.env` 없이) → 한마디 줄이 아예 안 뜨는지(초상만) 확인.
4. 같은 사실로 재스캔 → 한마디 안 바뀜(캐시 hit), 활동이 늘면 다음 스캔에 갱신.

## Self-Review

- **스펙 커버리지:**
  - §2 생성 시점(scan:done·stale만 재생성)·날짜 캐시·fingerprint → Task 1(캐시)·Task 3(결정)·Task 4(scan:done 훅).
  - §2 폴백(엔진 미설정 숨김 / 활동 LLM / 0건 정적) → Task 4(엔진 게이트)·Task 3(0건 정적)·Task 5(캐시 없음→None→숨김).
  - §2 정적 문구 verbatim → Task 2 `STATIC_DAILY_LINE`.
  - §4 `mascot.rs` 3함수 + store 테이블/get/upsert + 파이프라인 훅 + command + api + App → Task 2·3 / Task 1 / Task 4 / Task 5 / Task 6.
  - §5 프롬프트(페르소나·오늘 요약 블록·voice_guidance·한 문장 지시·수치 지어내기 금지) → Task 2 `build_daily_line_prompt` + 테스트.
  - §6 테스트 기준: mascot 단위(Task 2) / store 라운드트립(Task 1) / 파이프라인 MockEngine 3케이스=활동·skip·0건(Task 3 `compute_daily_line`; 엔진 None no-op는 Task 4 app 게이트+수동) / command Some·None(Task 5) / 프론트 build(Task 6) / 수동 E2E(위 절).
  - §7 스코프: #1만, #3은 후속(compute/프롬프트/정적을 재사용하도록 `pub`). 정적 다변화 YAGNI.
- **플레이스홀더 스캔:** 모든 code 스텝에 실제 코드·실제 명령·기대 출력 포함. TODO/"적절히 처리" 없음.
- **타입 일관성:** `get_daily_line -> Option<(String,String)>`(store) → `compute_daily_line`은 `cached_fp: Option<&str>` 소비·`Option<(String,String)>` 반환 → 파이프라인이 `v.map(|(_,fp)| fp)`로 fp만 추출해 `.as_deref()` 전달, `daily_line_inner`는 `.map(|(text,_)| text)`로 text만. `facts_fingerprint` 포맷과 store에 저장되는 fp 문자열 동일. command 이름 `get_daily_line`이 lib.rs 등록·api.ts invoke 문자열과 일치.
