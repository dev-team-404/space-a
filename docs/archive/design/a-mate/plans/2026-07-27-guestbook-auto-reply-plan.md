---
status: done
archived: 2026-07-27
---

# G3 방명록 봇 자동 답글 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 내 방 방명록에 남의 글이 달리면 파이프라인 스캔 시 봇이 성향(MBTI+호칭+페르소나) 답글을 자동 생성·게시한다 (글당 1회, 스캔당 최대 3개, 실패 무해).

**Architecture:** 스펙 [2026-07-27-guestbook-auto-reply-design.md](../specs/2026-07-27-guestbook-auto-reply-design.md) 승인분. 순수 로직(후보 선정·프롬프트·생성·표기 조립)은 core `mascot.rs`에, HTTP·설정 읽기는 `pipeline.rs`의 얇은 스캔 훅 `maybe_reply_guestbook`에 — `compute_chatter_pool`/`maybe_generate_chatter_pool`과 동일한 분리. "글당 1회" dedup은 서버 데이터(내 답글 행 존재 여부)로 판정, 로컬 상태 없음. a-hub·프론트엔드 무변경.

**Tech Stack:** Rust (Cargo workspace: core=`agent-mentor`/lib `agent_mentor`, 셸=`agent-mentor-app`), serde_json, 기존 `Engine` trait + `MockEngine`, `LifeClient`(ureq).

## Global Constraints

- **권위 테스트는 Windows PowerShell** — `a-mate/`에서 `cargo test` (WSL 금지, a-mate/CLAUDE.md).
- macOS는 부분 신호만: `cargo test -p agent-mentor`(core)는 유효하나 기존 실패 2건(core `hosts` 1건 Windows 경로 기대, `src-tauri` 테스트 바이너리 링크 실패)은 본 작업과 무관. `src-tauri`는 `cargo check -p agent-mentor-app`으로 컴파일만 검증.
- 커밋: 영어 Conventional Commits, scope `agent` (docs 커밋은 `docs(design)`/`docs(archive)`).
- 서버 규칙(G2, 검증은 서버가 함 — 클라이언트는 회피만): body 1~500자, `author_name` ≤80자, 답글은 방 주인만·1-depth.
- a-hub·프론트엔드(`src/`)·`contracts/` 파일 수정 금지.
- 작업 위치: worktree `.claude/worktrees/guestbook-auto-reply`, 브랜치 `feat/guestbook-auto-reply` (생성 완료, 스펙 커밋 있음).

---

### Task 1: 봇 작성자 표기 조립 `bot_author_name` (core)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (함수는 `compute_chatter_pool` 뒤·`#[cfg(test)] mod tests` 앞, 테스트는 파일 끝에 새 mod)

**Interfaces:**
- Consumes: 없음 (순수 함수)
- Produces: `pub fn bot_author_name(owner_title: &str, user_name: &str) -> Option<String>` — Task 5의 훅이 호출. `Some("{title}님의 {name}")`, 어느 쪽이든 공백/빈값이거나 조립 결과가 80자(chars) 초과면 `None`(= 서버 fallback 위임, ADR 0020 규범·스펙 §D).

- [x] **Step 1: Write the failing tests**

`a-mate/crates/core/src/mascot.rs` 파일 끝(기존 `chatter_tests` mod 뒤)에 추가:

```rust
#[cfg(test)]
mod guestbook_reply_tests {
    use super::*;

    #[test]
    fn bot_author_name_joins_title_and_name() {
        // ADR 0020 규범: "{owner_title}님의 {user_name}"
        assert_eq!(bot_author_name("대장", "둘쇠").as_deref(), Some("대장님의 둘쇠"));
    }

    #[test]
    fn bot_author_name_trims_and_requires_both_parts() {
        assert_eq!(bot_author_name(" 대장 ", " 둘쇠 ").as_deref(), Some("대장님의 둘쇠"));
        assert_eq!(bot_author_name("대장", ""), None);    // user_name 미설정 → 서버 fallback
        assert_eq!(bot_author_name("대장", "   "), None); // 공백만
        assert_eq!(bot_author_name("", "둘쇠"), None);
    }

    #[test]
    fn bot_author_name_none_when_over_80_chars() {
        // "님의 " = 3자 → 75+3+2 = 정확히 80자(허용), 76+3+2 = 81자(서버 400 회피 → None)
        assert!(bot_author_name(&"가".repeat(75), "둘쇠").is_some());
        assert_eq!(bot_author_name(&"가".repeat(76), "둘쇠"), None);
    }
}
```

- [x] **Step 2: Run tests to verify they fail**

Run (from `a-mate/`): `cargo test -p agent-mentor guestbook_reply`
Expected: COMPILE ERROR — `cannot find function 'bot_author_name'`

- [x] **Step 3: Write minimal implementation**

`mascot.rs`의 `compute_chatter_pool` 함수 끝(`}` 뒤, `#[cfg(test)] mod tests` 앞)에 추가:

```rust
/// G3 — 봇 답글 작성자 표기 "{owner_title}님의 {user_name}" (ADR 0020 규범 구현).
/// 어느 쪽이든 비어 있거나 조립 결과가 서버 상한(80자)을 넘으면 None —
/// 호출자는 author_name 미전달로 서버 fallback(등록된 agent name)에 위임한다.
pub fn bot_author_name(owner_title: &str, user_name: &str) -> Option<String> {
    let title = owner_title.trim();
    let name = user_name.trim();
    if title.is_empty() || name.is_empty() {
        return None;
    }
    let s = format!("{title}님의 {name}");
    (s.chars().count() <= 80).then_some(s)
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: PASS — `3 passed`

- [x] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): add bot author name assembly for guestbook replies"
```

---

### Task 2: 답글 후보 선정 `select_reply_targets` (core)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (Task 1 함수 아래에 타입·상수·함수, 테스트는 `guestbook_reply_tests` mod에 추가)

**Interfaces:**
- Consumes: 없음 (순수 함수 — 입력은 GET `/life/{id}/guestbook` 응답의 `entries` 배열)
- Produces (Task 5의 훅이 호출):
  - `pub struct ReplyTarget { pub entry_id: String, pub author_name: String, pub body: String }`
  - `pub const GUESTBOOK_REPLY_MAX_PER_SCAN: usize = 3;`
  - `pub fn select_reply_targets(entries: &[serde_json::Value], my_agent_id: &str, cap: usize) -> Vec<ReplyTarget>` — 오래된 순, 최대 `cap`개.

- [x] **Step 1: Write the failing tests**

`guestbook_reply_tests` mod 안에 추가:

```rust
    fn gb(entry_id: &str, author: &str, name: &str, body: &str, parent: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "entry_id": entry_id, "life_id": "l1", "author_agent_id": author,
            "author_name": name, "body": body, "parent_id": parent,
            "created_at": "2026-07-27T00:00:00Z"
        })
    }

    fn ids(t: &[ReplyTarget]) -> Vec<&str> {
        t.iter().map(|x| x.entry_id.as_str()).collect()
    }

    #[test]
    fn select_targets_excludes_own_posts_and_orders_oldest_first() {
        // 서버 순서 = 최신순: e3(최신) → e1(가장 오래됨). 내 글(e2)은 제외.
        let entries = vec![
            gb("e3", "visitor2", "이웃", "안녕", None),
            gb("e2", "me", "나", "내가 쓴 글", None),
            gb("e1", "visitor1", "손님", "놀러왔어요", None),
        ];
        let t = select_reply_targets(&entries, "me", 3);
        assert_eq!(ids(&t), ["e1", "e3"]);
        assert_eq!(t[0].author_name, "손님");
        assert_eq!(t[0].body, "놀러왔어요");
    }

    #[test]
    fn select_targets_excludes_replied_and_reply_rows() {
        // "글당 1회" = 서버 데이터 판정: 내 답글 행(r1)이 있는 원글(e1) 제외.
        // 답글 행 자체(top-level 아님)도 대상 아님.
        let entries = vec![
            gb("r1", "me", "대장님의 둘쇠", "고마워!", Some("e1")),
            gb("e2", "visitor", "손님", "두 번째 글", None),
            gb("e1", "visitor", "손님", "첫 글", None),
        ];
        assert_eq!(ids(&select_reply_targets(&entries, "me", 3)), ["e2"]);
    }

    #[test]
    fn select_targets_only_my_replies_count_for_dedup() {
        // 서버 규칙상 답글은 방 주인만 가능하지만, dedup은 방어적으로 "내" 답글만 센다.
        let entries = vec![
            gb("r1", "someone-else", "딴사람", "답글?", Some("e1")),
            gb("e1", "visitor", "손님", "첫 글", None),
        ];
        assert_eq!(ids(&select_reply_targets(&entries, "me", 3)), ["e1"]);
    }

    #[test]
    fn select_targets_caps_from_oldest() {
        let entries = vec![
            gb("e3", "v", "손님", "셋", None),
            gb("e2", "v", "손님", "둘", None),
            gb("e1", "v", "손님", "하나", None),
        ];
        assert_eq!(ids(&select_reply_targets(&entries, "me", 2)), ["e1", "e2"]);
    }

    #[test]
    fn select_targets_skips_malformed_rows_and_handles_empty() {
        let entries = vec![
            serde_json::json!({"entry_id": "bad1"}),          // author·body 누락
            serde_json::json!({"author_agent_id": "v", "body": "x"}), // entry_id 누락
            gb("", "v", "손님", "빈 id", None),
            gb("e0", "v", "", "빈 작성자명", None),
            gb("e1", "v", "손님", "", None),                   // 빈 body
            gb("ok", "v", "손님", "정상", None),
        ];
        assert_eq!(ids(&select_reply_targets(&entries, "me", 3)), ["ok"]);
        assert!(select_reply_targets(&[], "me", 3).is_empty());
    }
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: COMPILE ERROR — `cannot find struct 'ReplyTarget'` / `cannot find function 'select_reply_targets'`

- [x] **Step 3: Write minimal implementation**

`bot_author_name` 아래에 추가:

```rust
/// G3 — 자동 답글 대상 원글 (select_reply_targets 결과 행).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTarget {
    pub entry_id: String,
    pub author_name: String,
    pub body: String,
}

/// G3 — 스캔당 자동 답글 상한. 백로그 도배·LLM 비용을 바운드한다 (스펙 §B).
pub const GUESTBOOK_REPLY_MAX_PER_SCAN: usize = 3;

/// 평면 방명록 목록(서버 최신순)에서 자동 답글 대상 원글을 고른다 (스펙 §B):
/// top-level만 · 내 글 제외 · 내 답글이 이미 달린 원글 제외("글당 1회" — 서버 데이터가
/// dedup의 원천, 로컬 상태 없음. 사람 주인이 수동으로 단 답글도 같은 agent_id라 존중됨).
/// 필수 필드가 빈/누락된 행은 방어적으로 skip. 반환은 오래된 순, 최대 cap개.
pub fn select_reply_targets(
    entries: &[serde_json::Value],
    my_agent_id: &str,
    cap: usize,
) -> Vec<ReplyTarget> {
    let field = |e: &serde_json::Value, k: &str| -> Option<String> {
        e.get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    // 내가 이미 답글을 단 원글 id 집합
    let replied: std::collections::HashSet<String> = entries
        .iter()
        .filter(|e| field(e, "author_agent_id").as_deref() == Some(my_agent_id))
        .filter_map(|e| field(e, "parent_id"))
        .collect();
    entries
        .iter()
        .rev() // 서버 최신순 → 오래된 순
        .filter(|e| field(e, "parent_id").is_none())
        .filter_map(|e| {
            let entry_id = field(e, "entry_id")?;
            let author = field(e, "author_agent_id")?;
            let author_name = field(e, "author_name")?;
            let body = field(e, "body")?;
            (author != my_agent_id && !replied.contains(&entry_id))
                .then_some(ReplyTarget { entry_id, author_name, body })
        })
        .take(cap)
        .collect()
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: PASS — `8 passed` (Task 1의 3개 + 신규 5개)

- [x] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): add guestbook auto-reply target selection"
```

---

### Task 3: 답글 프롬프트 `build_guestbook_reply_prompt` (core)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (함수는 `select_reply_targets` 아래, 테스트는 `guestbook_reply_tests` mod에 추가)

**Interfaces:**
- Consumes: `mbti_voice_hint(Option<&str>) -> String`(내부에서 정규화, 무효 MBTI는 빈 문자열), `crate::diary::voice_guidance()` — 둘 다 기존.
- Produces: `pub fn build_guestbook_reply_prompt(honorific: &str, mbti: Option<&str>, visitor_name: &str, post_body: &str) -> String` — Task 4가 호출.

- [x] **Step 1: Write the failing tests**

`guestbook_reply_tests` mod에 추가:

```rust
    #[test]
    fn reply_prompt_carries_persona_visitor_post_and_directives() {
        let p = build_guestbook_reply_prompt("주인", None, "손님", "놀러왔어요");
        assert!(p.contains("주인"));                         // 페르소나 호칭
        assert!(p.contains("'손님'"));                       // 방문자 이름
        assert!(p.contains("놀러왔어요"));                   // 원글 본문
        assert!(p.contains(crate::diary::voice_guidance())); // voice_guidance verbatim
        assert!(p.contains("한 줄"));                        // 한 줄 지시
        assert!(p.contains("100자"));                        // 길이 상한
        assert!(p.contains("지어내지 마세요"));              // 정밀도의 선(원글 근거)
        assert!(!p.contains("[오늘("));                      // facts_block 미포함 (스펙 §C)
    }

    #[test]
    fn reply_prompt_uses_custom_honorific_and_mbti_voice() {
        let p = build_guestbook_reply_prompt("대장", Some("INTJ"), "이웃", "잘 지내?");
        assert!(p.contains("대장"));
        assert!(!p.contains("'주인'"));
        assert!(p.contains("냉정")); // T 성향 voice hint
    }
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: COMPILE ERROR — `cannot find function 'build_guestbook_reply_prompt'`

- [x] **Step 3: Write minimal implementation**

`select_reply_targets` 아래에 추가 (페르소나 전문은 `build_daily_line_prompt`/`build_chatter_prompt`와 동일 문구 — 동일 인물 규약):

```rust
/// G3 — 방명록 자동 답글 시스템 프롬프트. 한마디·잡담과 동일 페르소나(1인칭·능청·호칭·
/// MBTI voice·voice_guidance)이되 facts_block(오늘 업무 요약)은 뺀다 — 답글은 원글에
/// 반응해야 하고, 무관한 업무 수치를 끌어와 날조할 위험만 늘린다 (스펙 §C).
pub fn build_guestbook_reply_prompt(
    honorific: &str,
    mbti: Option<&str>,
    visitor_name: &str,
    post_body: &str,
) -> String {
    format!(
        "당신은 {honorific}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '{honorific}'이라고 부릅니다.{voice} \
         \
         {voice_guidance} \
         \
         방문자 '{visitor_name}'이 {honorific}의 미니홈피 방명록에 아래 글을 남겼습니다. \
         정밀도의 선(반드시 지킬 것): 원글에 없는 사실을 지어내지 마세요.\n\n\
         [방명록 원글 — {visitor_name}]\n{post_body}\n\n\
         {honorific}의 마스코트로서 이 방문자에게 남길 방명록 답글을 딱 한 줄(100자 이내)로 \
         작성하세요. 번호·불릿·따옴표 없이 답글 본문만 출력하세요.",
        honorific = honorific,
        voice = mbti_voice_hint(mbti),
        voice_guidance = crate::diary::voice_guidance(),
        visitor_name = visitor_name,
        post_body = post_body,
    )
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: PASS — `10 passed`

- [x] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): add guestbook reply prompt builder"
```

---

### Task 4: 답글 생성 `compute_guestbook_reply` (core)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (함수는 `build_guestbook_reply_prompt` 아래, 테스트는 `guestbook_reply_tests` mod에 추가)

**Interfaces:**
- Consumes: Task 2 `ReplyTarget`, Task 3 `build_guestbook_reply_prompt`, 기존 `Engine` trait(`crate::diary::engine::Engine`, `generate(&self, system, user) -> Result<EngineOutput>`), 기존 `parse_chatter_lines(raw, max_n)`.
- Produces: `pub fn compute_guestbook_reply(engine: &dyn crate::diary::engine::Engine, honorific: &str, mbti: Option<&str>, target: &ReplyTarget) -> anyhow::Result<String>` — Task 5가 호출. store 접근 없음(호출자가 락 밖에서 부른다 — `compute_daily_line` 선례).

- [x] **Step 1: Write the failing tests**

`guestbook_reply_tests` mod에 추가 (mod 상단 `use super::*;` 옆에 `use crate::diary::engine::MockEngine;` 추가):

```rust
    fn target() -> ReplyTarget {
        ReplyTarget { entry_id: "e1".into(), author_name: "손님".into(), body: "놀러왔어요".into() }
    }

    #[test]
    fn compute_reply_returns_first_cleaned_line() {
        // parse_chatter_lines 재사용: 감싼 따옴표 벗기고, 여러 줄이면 첫 줄만.
        let eng = MockEngine { canned: "  \"어서와, 반가워!\"  \n둘째 줄은 버림".into() };
        let out = compute_guestbook_reply(&eng, "주인", None, &target()).unwrap();
        assert_eq!(out, "어서와, 반가워!");
    }

    #[test]
    fn compute_reply_errs_on_empty_output() {
        let eng = MockEngine { canned: "   \n  ".into() };
        assert!(compute_guestbook_reply(&eng, "주인", None, &target()).is_err());
    }

    #[test]
    fn compute_reply_truncates_to_server_limit() {
        // 서버 상한(500자) 방어 truncate — 400 반환·영구 재시도 루프 회피.
        let eng = MockEngine { canned: "가".repeat(600) };
        let out = compute_guestbook_reply(&eng, "주인", None, &target()).unwrap();
        assert_eq!(out.chars().count(), 500);
    }
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: COMPILE ERROR — `cannot find function 'compute_guestbook_reply'`

- [x] **Step 3: Write minimal implementation**

`build_guestbook_reply_prompt` 아래에 추가:

```rust
/// G3 — 방명록 답글 한 줄 생성. store 접근 없음, 네트워크(LLM)만 — 호출자가 락 밖에서
/// 부른다 (compute_daily_line 선례). 빈 출력은 Err(호출자 warn+skip), 서버 상한(500자)
/// 초과분은 방어 truncate.
pub fn compute_guestbook_reply(
    engine: &dyn crate::diary::engine::Engine,
    honorific: &str,
    mbti: Option<&str>,
    target: &ReplyTarget,
) -> anyhow::Result<String> {
    let system = build_guestbook_reply_prompt(honorific, mbti, &target.author_name, &target.body);
    let raw = engine.generate(&system, "")?.text;
    let line = parse_chatter_lines(&raw, 1)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("방명록 답글 생성 결과가 비어 있음"))?;
    Ok(line.chars().take(500).collect())
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor guestbook_reply`
Expected: PASS — `13 passed`

또한 core 전체 무회귀: `cargo test -p agent-mentor`
Expected: 기존 실패(macOS라면 `hosts` 1건) 외 전부 PASS

- [x] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): add guestbook reply generation"
```

---

### Task 5: 스캔 훅 `maybe_reply_guestbook` (src-tauri)

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs` — ① `run_pipeline_once`의 훅 연쇄(`maybe_generate_chatter_pool(&state.store);` 직후, 현재 :119 부근), ② `mod runtime` 안 함수 본문(`maybe_generate_chatter_pool` 함수 뒤, `mod runtime` 닫는 `}` 앞, 현재 :827 부근)

**Interfaces:**
- Consumes: Task 1 `bot_author_name`, Task 2 `select_reply_targets`·`GUESTBOOK_REPLY_MAX_PER_SCAN`, Task 4 `compute_guestbook_reply`, 기존 `normalize_mbti`, 기존 `crate::resolve_engine(&SqliteStore) -> Option<OpenAiCompatEngine>`, 기존 `crate::commands::owner_title(&SqliteStore) -> String`(pub(crate)), 기존 `LifeClient { base_url, token, api_key }`·`guestbook`·`add_guestbook`, settings 키 `hub_url`/`hub_token`/`hub_api_key`/`hub_life_id`/`hub_agent_id`/`user_name`/`user_mbti`.
- Produces: `fn maybe_reply_guestbook(store_mutex: &std::sync::Mutex<SqliteStore>)` — 파이프라인 내부 전용, 외부 인터페이스 없음.

- [x] **Step 1: 훅 함수 구현**

`pipeline.rs`의 `maybe_generate_chatter_pool` 함수 끝(`}` 뒤, `mod runtime` 닫는 `}` 앞)에 추가:

```rust
    /// G3 — 내 방 방명록의 미답글 원글에 봇 성향 답글 (스펙 §A·§E). hub 미연결·엔진
    /// 미설정이면 no-op. 모든 실패는 warn 후 skip — 다음 스캔 재시도. 구서버(G2 미배포)가
    /// parent_id를 무시하면 답글이 원글로 저장돼 dedup이 깨지고 스캔마다 도배되므로,
    /// POST 응답의 parent_id 에코를 검증해 불일치 시 앱 실행 동안 비활성(maybe_probe_docs 선례).
    fn maybe_reply_guestbook(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static INCOMPATIBLE: AtomicBool = AtomicBool::new(false);
        if INCOMPATIBLE.load(Ordering::SeqCst) { return; }

        // ① 락: 엔진·hub 설정·페르소나 읽기 → 즉시 해제 (maybe_generate_chatter_pool 선례)
        let (engine, url, token, api_key, life_id, agent_id, title, user_name, mbti) =
            match store_mutex.lock() {
                Ok(store) => {
                    let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                    (
                        crate::resolve_engine(&store),
                        get("hub_url"), get("hub_token"), get("hub_api_key"),
                        get("hub_life_id"), get("hub_agent_id"),
                        crate::commands::owner_title(&store), get("user_name"), get("user_mbti"),
                    )
                }
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
        let Some(engine) = engine else { return; };
        if url.trim().is_empty() || token.is_empty() || life_id.is_empty() || agent_id.is_empty() {
            return;
        }

        // ② 락 없이 네트워크: 조회 → 선정 → 후보별 [생성 → 게시]
        let client = agent_mentor::life_client::LifeClient {
            base_url: url,
            token,
            api_key: { let k = api_key.trim(); (!k.is_empty()).then(|| k.to_string()) },
        };
        let entries = match client.guestbook(&life_id) {
            Ok(v) => v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default(),
            Err(e) => { log::warn!("방명록 자동 답글: 조회 실패(다음 스캔 재시도): {e}"); return; }
        };
        let targets = agent_mentor::mascot::select_reply_targets(
            &entries, &agent_id, agent_mentor::mascot::GUESTBOOK_REPLY_MAX_PER_SCAN);
        if targets.is_empty() { return; }
        let author = agent_mentor::mascot::bot_author_name(&title, &user_name);
        let mbti = agent_mentor::mascot::normalize_mbti(&mbti);
        for t in &targets {
            let reply = match agent_mentor::mascot::compute_guestbook_reply(
                &engine, &title, mbti.as_deref(), t)
            {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("방명록 자동 답글: 생성 실패(entry {}): {e}", t.entry_id);
                    continue;
                }
            };
            match client.add_guestbook(&life_id, &reply, author.as_deref(), Some(&t.entry_id)) {
                Ok(resp) => {
                    let echoed = resp.get("parent_id").and_then(|p| p.as_str())
                        == Some(t.entry_id.as_str());
                    if !echoed {
                        log::warn!("방명록 자동 답글: 서버가 parent_id 미지원(G2 미배포) — 이번 실행 동안 비활성");
                        INCOMPATIBLE.store(true, Ordering::SeqCst);
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("방명록 자동 답글: 게시 실패(entry {}): {e}", t.entry_id);
                    continue;
                }
            }
        }
    }
```

- [x] **Step 2: 훅 연쇄에 등록**

`run_pipeline_once`의 훅 연쇄에서 `maybe_generate_chatter_pool(&state.store);` 바로 다음 줄에 추가:

```rust
                // G3 방명록 봇 자동 답글 — hub 미연결·엔진 없으면 no-op, 실패는 조용히(다음 스캔 재시도)
                maybe_reply_guestbook(&state.store);
```

- [x] **Step 3: 컴파일·무회귀 검증**

Run (from `a-mate/`):
- `cargo check -p agent-mentor-app` — Expected: PASS (macOS에서도 check는 링크 안 함)
- `cargo test -p agent-mentor` — Expected: 기존 실패(macOS `hosts` 1건) 외 전부 PASS

훅 자체 단위 테스트는 없음 — `mod runtime`은 `#[cfg(not(test))]`이고 기존 `maybe_*` 훅 전부 동일 관행. 판정·생성 로직은 Task 1~4에서 core 테스트로 검증 완료.

- [x] **Step 4: Commit**

```bash
git add a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(agent): auto-reply to own guestbook posts on scan"
```

---

### Task 6: 최종 검증 · DoD(아카이브) · PR

**Files:**
- Move: 본 plan + `docs/design/a-mate/specs/2026-07-27-guestbook-auto-reply-design.md` → `docs/archive/design/a-mate/` 미러 (docs-archive 스킬이 수행)

**Interfaces:**
- Consumes: Task 1~5 완료 상태
- Produces: 머지 가능한 PR (ADR 0013 DoD 충족)

- [x] **Step 1: 권위 테스트 (Windows PowerShell)**

Windows PowerShell, `a-mate/`에서:

```powershell
cargo test
```

Expected: 전부 PASS (워크스페이스 전체 — core + src-tauri). macOS에서 실행 중이라면 이 단계는 사용자에게 Windows 실행을 요청하고 결과를 받는다 — **macOS 신호만으로 PASS 주장 금지** (memory: macos-partial-verification).

프론트엔드는 무변경이므로 `npm test`는 필수 아님 — CI/리뷰어 요구 시에만.

- [x] **Step 2: 플랜 체크박스 완료 확인 + docs-archive**

본 plan의 모든 체크박스가 완료됐는지 확인 후, `docs-archive` 스킬을 실행해 본 plan과 스펙(`2026-07-27-guestbook-auto-reply-design.md`)을 `docs/archive/` 미러로 이동·커밋한다 (같은 PR — ADR 0013). 로드맵(`2026-07-26-life-social-diary-followups-roadmap.md`)은 다른 아이템이 남아 있으므로 이동하지 않고, G3 항목에 완료 표시(`✅ 완료(2026-07-27, PR #NN)`)만 추가한다.

```bash
git add docs/
git commit -m "docs(archive): archive G3 guestbook auto-reply spec and plan"
```

- [x] **Step 3: PR 생성**

```bash
git push -u origin feat/guestbook-auto-reply
gh pr create --title "feat(agent): guestbook bot auto-reply (G3)" --body "$(cat <<'EOF'
## Summary
- G3 (roadmap 2차 배치): 내 방 방명록에 남의 글이 달리면 파이프라인 스캔 시 봇이
  성향(MBTI+호칭+페르소나) 답글을 자동 생성·게시
- "글당 1회" dedup은 서버 데이터(내 답글 행 존재)로 판정 — 로컬 상태 없음, 스캔당 상한 3개
- 봇 작성자 표기 "{owner_title}님의 {user_name}" 구현 (ADR 0020 규범)
- 실패 무해: hub 미연결·엔진 미설정 no-op, 실패는 warn 후 다음 스캔 재시도,
  구서버(G2 미배포) 감지 시 실행 동안 비활성
- a-hub·프론트엔드 무변경. 설계: docs/archive/design/a-mate/specs/2026-07-27-guestbook-auto-reply-design.md

## Test plan
- [x] Windows PowerShell `cargo test` (워크스페이스) 전부 PASS
- [x] core 신규 테스트 13건 (선정 필터·상한·정렬 / 프롬프트 / 생성·truncate / 표기 조립)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL 출력.
