# 잡담 코믹 (묶음 B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 마스코트 잡담 풀에 근무 맥락(주말·연속세션 ≥5·장시간 몰입)을 능청스러운 코믹 소재로 반영한다.

**Architecture:** 묶음 A(PR 머지됨)에서 구현한 `diary::WorkContext`(is_weekend·active_hours·long_work)를 잡담 경로에 재사용한다. `build_chatter_prompt`/`compute_chatter_pool` 시그니처에 `work: &WorkContext`를 추가하고, 신호가 있을 때만 코믹 지시 문단을 프롬프트에 붙인다. fp/캐시/트레이/프론트(PR #18 구조)는 불변 — 콘텐츠만 확장.

**Tech Stack:** Rust (crates/core `agent-mentor` + src-tauri `pipeline.rs`), 테스트는 cargo test. 프론트 변경 없음(vitest 회귀만).

**스펙:** `docs/specs/2026-07-12-mascot-context-enrichment-design.md` §4 묶음 B, §5 검증 기준.

## Global Constraints

- 잡담 임계 상수: `CHATTER_REST_SESSIONS = 5` (오늘 세션 이 이상이면 "쉬어라" 지시, 조정 가능 상수).
- 사실주장 금지(정밀도의 선) 프롬프트 문구 유지, 정적 폴백(프론트) 유지.
- fp(`facts_fingerprint`) 형식 불변 — daily-line과 공용이라 바꾸면 스코프 밖(daily-line 재생성)에 영향. 근무 맥락 변화는 이벤트 유입(=세션 수·토큰 변화)과 동행하므로 기존 fp가 재생성 트리거를 실질 커버.
- 빌드 매 cargo 전(Git Bash): `export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`
- 브랜치 `feat/chatter-comic`(생성됨), main 직접 커밋 금지, 커밋 태스크 단위.

---

### Task 1: 잡담 프롬프트 코믹 지시 + 배선

core의 프롬프트·compute 시그니처 변경과 pipeline 배선은 원자적(시그니처가 바뀌면 호출부도 같이 바뀌어야 워크스페이스가 컴파일됨)이라 한 태스크·한 커밋으로 묶는다.

**Files:**
- Modify: `crates/core/src/mascot.rs` (build_chatter_prompt·compute_chatter_pool·상수·헬퍼·테스트)
- Modify: `crates/core/src/diary/mod.rs:386` (`collect_work_context` pub화)
- Modify: `src-tauri/src/pipeline.rs:245-270` (`maybe_generate_chatter_pool` 배선)

**Interfaces:**
- Consumes: `crate::diary::WorkContext { is_weekend: bool, active_hours: f64, long_work: bool }` (pub, Default 구현 있음), `crate::chat::ChatContext.session_count: u64`
- Produces:
  - `pub const CHATTER_REST_SESSIONS: u64 = 5;` (mascot.rs)
  - `pub fn build_chatter_prompt(ctx: &crate::chat::ChatContext, work: &crate::diary::WorkContext, n: usize) -> String`
  - `pub fn compute_chatter_pool(engine: &dyn crate::diary::engine::Engine, ctx: &crate::chat::ChatContext, work: &crate::diary::WorkContext, cached_fp: Option<&str>) -> anyhow::Result<Option<(Vec<String>, String)>>`
  - `pub fn collect_work_context(store: &SqliteStore, date: &str, today: NaiveDate) -> WorkContext` (diary/mod.rs, `fn`→`pub fn`만)

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/src/mascot.rs`의 `mod chatter_tests`에 WorkContext 헬퍼와 신규 테스트 4개를 추가하고, 기존 테스트의 `build_chatter_prompt`/`compute_chatter_pool` 호출에 work 인자를 추가한다.

```rust
// mod chatter_tests 안, 기존 fn ctx(...) 아래에 추가:
fn work(is_weekend: bool, active_hours: f64, long_work: bool) -> crate::diary::WorkContext {
    crate::diary::WorkContext { is_weekend, active_hours, long_work }
}

#[test]
fn chatter_prompt_weekend_adds_comic_directive() {
    let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(true, 2.0, false), 5);
    assert!(p.contains("[오늘 근무 맥락"));   // 코믹 소재 블록 헤더
    assert!(p.contains("주말"));              // 주말 신호
    assert!(p.contains("일중독"));            // 능청 예시 문구
    assert!(!p.contains("쉬었다 와라"));      // 세션 3건 → 쉬어라 지시 없음
}

#[test]
fn chatter_prompt_rest_directive_at_session_threshold() {
    // 경계: CHATTER_REST_SESSIONS(5) 이상이면 on, 미만이면 off
    let on = build_chatter_prompt(&ctx(5, 100, 200, 1), &work(false, 2.0, false), 5);
    assert!(on.contains("5건"));              // 오늘 세션 수(사실) 인용
    assert!(on.contains("쉬었다 와라"));      // 잔소리 지시
    let off = build_chatter_prompt(&ctx(4, 100, 200, 1), &work(false, 2.0, false), 5);
    assert!(!off.contains("쉬었다 와라"));
}

#[test]
fn chatter_prompt_long_work_adds_care_directive() {
    let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(false, 7.5, true), 5);
    assert!(p.contains("7.5시간"));           // 몰입 시간(사실) 인용
    assert!(p.contains("배터리"));            // 다마고치 능청 예시(묶음 A A5 톤과 일관)
}

#[test]
fn chatter_prompt_without_signals_has_no_comic_block() {
    // 무신호(평일·세션 적음·짧은 몰입) → 코믹 블록 자체가 없어 기존 프롬프트와 동일 골격
    let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(false, 2.0, false), 5);
    assert!(!p.contains("근무 맥락"));
}
```

기존 테스트 보정(같은 모듈, 시그니처에 work 추가 — 전부 무신호 기본값):

```rust
// chatter_prompt_carries_persona_facts_voice_and_directives 첫 줄을:
let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &Default::default(), 5);

// compute_pool_* 4개 테스트의 compute_chatter_pool 호출을 (총 4곳):
let out = compute_chatter_pool(&eng, &ctx(3, 100, 200, 1), &Default::default(), None).unwrap();
// compute_pool_skips_when_fingerprint_matches_cache 는:
assert_eq!(compute_chatter_pool(&eng, &c, &Default::default(), Some(&fp)).unwrap(), None);
// compute_pool_returns_empty_without_engine_when_idle 는:
let out = compute_chatter_pool(&eng, &ctx(0, 0, 0, 2), &Default::default(), None).unwrap();
```

- [ ] **Step 2: 테스트가 실패(컴파일 에러)하는지 확인**

Run (Git Bash, 환경 export 선행):
```bash
cd /d/Project/agent-mentor && cargo test -p agent-mentor --lib mascot
```
Expected: FAIL — `build_chatter_prompt`/`compute_chatter_pool`이 인자 2/3개만 받는다는 컴파일 에러 (E0061).

- [ ] **Step 3: 최소 구현**

`crates/core/src/mascot.rs` — `CHATTER_POOL_SIZE` 상수 아래에 추가:

```rust
/// 오늘 세션이 이 이상이면 "그만 좀 하고 쉬어라" 코믹 지시를 넣는다 (스펙 묶음 B, 조정 가능).
pub const CHATTER_REST_SESSIONS: u64 = 5;

/// 근무 맥락 코믹 지시 — 신호(주말·연속세션·장시간)가 있을 때만 소재 블록 생성, 없으면 빈 문자열.
/// 위로가 아니라 능청·놀림 톤(다이어리 A5의 anti-monotony 결과 일관).
fn comic_directives(ctx: &crate::chat::ChatContext, work: &crate::diary::WorkContext) -> String {
    let mut items: Vec<String> = Vec::new();
    if work.is_weekend {
        items.push(
            "- 오늘은 주말인데 주인이 또 나와서 일하고 있다 — \"주말에 또 나왔어? 일중독이야ㅋㅋ\" 같은 능청."
                .to_string(),
        );
    }
    if ctx.session_count >= CHATTER_REST_SESSIONS {
        items.push(format!(
            "- 오늘 세션이 벌써 {}건 — \"그만 좀 하고 쉬었다 와라\" 같은 잔소리.",
            ctx.session_count
        ));
    }
    if work.long_work {
        items.push(format!(
            "- 오늘 몰입 시간이 {}시간 — \"오래 붙어 있었네, 배터리 방전되겠다\" 같은 챙김.",
            work.active_hours
        ));
    }
    if items.is_empty() {
        return String::new();
    }
    format!(
        "\n\n[오늘 근무 맥락 — 코믹 소재]\n{}\n\
         위 근무 맥락은 사실이니 잡담 일부에 능청스럽게 녹이세요. \
         걱정 어투 말고 웃기게 — 다마고치가 주인을 놀리는 톤.",
        items.join("\n")
    )
}
```

`build_chatter_prompt` 시그니처·본문 수정 (facts 뒤에 comic 삽입 — 빈 문자열이면 기존 프롬프트와 동일):

```rust
pub fn build_chatter_prompt(
    ctx: &crate::chat::ChatContext,
    work: &crate::diary::WorkContext,
    n: usize,
) -> String {
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}{comic}\n\n\
         위 요약을 재료로, 상주 마스코트가 가끔 툭 던질 가벼운 잡담·혼잣말을 {n}개 만드세요. \
         코칭 조언이나 보고처럼 굴지 마세요(조언은 다른 채널이 합니다). \
         한 줄에 하나씩, 각 40자 이내로, 번호·불릿·따옴표 없이 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
        comic = comic_directives(ctx, work),
    )
}
```

`compute_chatter_pool` 시그니처 수정 (work를 프롬프트 빌드에 전달, 나머지 로직 불변 — fp·idle 빈 풀·파싱 그대로):

```rust
pub fn compute_chatter_pool(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    work: &crate::diary::WorkContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(Vec<String>, String)>> {
    let fp = facts_fingerprint(ctx);
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    if ctx.session_count == 0 {
        return Ok(Some((Vec::new(), fp)));
    }
    let system = build_chatter_prompt(ctx, work, CHATTER_POOL_SIZE);
    let raw = engine.generate(&system, "")?.text;
    Ok(Some((parse_chatter_lines(&raw, CHATTER_POOL_SIZE), fp)))
}
```

`crates/core/src/diary/mod.rs:386` — pipeline에서 재사용하도록 pub화 (본문 불변):

```rust
/// 근무 맥락: 요일(주말)과 그날 몰입 시간. 몰입 시간은 연속 이벤트 간격 중 IDLE_GAP_SECS(30분)
/// 이하인 것만 합산 — 첫~마지막 span은 중간 공백(점심·회의 등)까지 포함해 과장되므로 쓰지 않는다.
pub fn collect_work_context(store: &SqliteStore, date: &str, today: NaiveDate) -> WorkContext {
```

- [ ] **Step 4: core 테스트 통과 확인**

Run:
```bash
cargo test -p agent-mentor --lib mascot
```
Expected: PASS — chatter_tests 신규 4개 + 기존 보정분 전부 그린.

- [ ] **Step 5: pipeline 배선**

`src-tauri/src/pipeline.rs`의 `maybe_generate_chatter_pool` — ① 락 구간에서 work_context를 함께 수집, ② compute에 전달. daily-line 쪽(`maybe_generate_daily_line`)은 불변.

```rust
    /// 잡담 풀 — scan:done마다 fp가 stale할 때만 재생성 (스펙 §3). 엔진 없으면 no-op.
    /// 네트워크(LLM)는 daily-line과 동일하게 store 락 밖에서 호출. 이벤트는 emit하지
    /// 않는다 — 프론트 잡담 타이머가 발화 시점에 get_chatter_pool로 pull한다.
    fn maybe_generate_chatter_pool(store_mutex: &std::sync::Mutex<SqliteStore>) {
        let Some(engine) = OpenAiCompatEngine::from_env() else { return; };
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();

        // ① 락: 오늘 컨텍스트 + 근무 맥락 + 캐시된 fingerprint 읽기 → 즉시 해제
        let (ctx, work, cached_fp) = match store_mutex.lock() {
            Ok(store) => {
                let ctx = match crate::commands::chat_context_inner(&store) {
                    Ok(c) => c,
                    Err(e) => { log::warn!("chatter chat_context 실패: {e}"); return; }
                };
                let work = agent_mentor::diary::collect_work_context(&store, &today, now.date_naive());
                let cached_fp = match store.get_chatter_pool(&today) {
                    Ok(v) => v.map(|(_, fp)| fp),
                    Err(e) => { log::warn!("get_chatter_pool 실패: {e}"); return; }
                };
                (ctx, work, cached_fp)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // ② 락 없이 compute (0건 빈 풀 or 네트워크 생성). None이면 skip.
        let outcome = match agent_mentor::mascot::compute_chatter_pool(&engine, &ctx, &work, cached_fp.as_deref()) {
            Ok(o) => o,
            Err(e) => { log::warn!("compute_chatter_pool 실패: {e}"); return; }
        };
        let Some((lines, fp)) = outcome else { return; };

        // ③ 락: 캐시 upsert → 즉시 해제
        match store_mutex.lock() {
            Ok(store) => {
                if let Err(e) = store.upsert_chatter_pool(&today, &lines, &fp) {
                    log::warn!("upsert_chatter_pool 실패: {e}");
                }
            }
            Err(e) => log::warn!("store lock poisoned: {e}"),
        }
    }
```

- [ ] **Step 6: 워크스페이스 전체 그린 확인**

Run:
```bash
cargo test --workspace
```
Expected: PASS — core 전체(회귀 포함) + src-tauri 컴파일·테스트 그린.

- [ ] **Step 7: 커밋**

```bash
git add crates/core/src/mascot.rs crates/core/src/diary/mod.rs src-tauri/src/pipeline.rs
git commit -m "feat(mascot): 잡담 코믹 — 주말·연속세션(≥5)·장시간 몰입을 능청 소재로 (스펙 묶음 B)"
```

---

### Task 2: 게이트 + PR

**Files:** 변경 없음 (검증·PR만)

- [ ] **Step 1: clippy·fmt 게이트**

Run:
```bash
cargo clippy --workspace -- -D warnings && cargo fmt --check
```
Expected: 경고 0. (fmt이 기존 파일 스타일과 충돌하면 — 이 저장소는 수동 정렬 주석 스타일 — `cargo fmt --check` 실패가 이번 변경 파일 밖이면 스킵하고 clippy만 게이트로 삼는다.)

- [ ] **Step 2: 프론트 회귀 (vitest)**

프론트 변경이 없으므로 회귀만:
```bash
npm test
```
Expected: PASS — 기존 스위트 전부 그린.

- [ ] **Step 3: push + PR 생성 (base=main)**

```bash
git push -u origin feat/chatter-comic
# revocation 에러 시: git -c http.schannelCheckRevoke=false push -u origin feat/chatter-comic
```

PR 본문 (heredoc로 전달, 테스트 결과 수치는 실행 시점 값으로 치환):

```bash
gh pr create --base main \
  --title "feat(mascot): 잡담 코믹 — 근무 맥락(주말·연속세션·장시간) 능청 반영 (묶음 B)" \
  --body "$(cat <<'EOF'
## 요약
마스코트 잡담 풀에 근무 맥락을 능청 코믹 소재로 반영 (스펙: docs/specs/2026-07-12-mascot-context-enrichment-design.md §4 묶음 B).

- `build_chatter_prompt`/`compute_chatter_pool`에 `work: &diary::WorkContext` 추가 — 묶음 A 신호(주말·몰입시간·장시간) 재사용
- 코믹 지시(신호 있을 때만): 주말 "주말에 또 나왔어? 일중독ㅋㅋ" / 세션 ≥ `CHATTER_REST_SESSIONS`(5) "그만 좀 하고 쉬었다 와라" / `long_work` "배터리 방전되겠다"
- `collect_work_context` pub화 + pipeline `maybe_generate_chatter_pool` 배선

## 불변 (PR #18 구조 재사용)
- fp(`facts_fingerprint`)·store 캐시·트레이 레벨·프론트(정적 폴백, pickChatter) 변경 없음
- 무신호면 프롬프트가 기존과 동일 골격, idle(세션 0) 빈 풀 유지

## 테스트
- core: chatter 프롬프트 코믹 마커 4건 신규(주말/세션 경계 5·4/장시간/무신호) + 기존 회귀 그린
- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `npm test`(프론트 변경 없음, 회귀) 그린

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
Expected: PR 생성, base=main.
