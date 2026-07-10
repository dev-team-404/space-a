# 마스코트 상주봇 잡담 업그레이드 (마스코트 보이스 #3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 상주 마스코트의 기존 잡담 말풍선 채널에 LLM 사용기록 연계 잡담 풀(스캔 편승 생성·날짜 캐시)과 정적 응원 결을 더하고, 트레이 "잡담" 서브메뉴로 빈도(chatter_level)를 노출한다.

**Architecture:** `maybe_generate_daily_line`(#1) 선례를 그대로 따르는 병렬 구조 — scan:done 후 fingerprint stale 시에만 LLM 1회 호출로 잡담 5개를 배치 생성해 `chatter_pool` 테이블에 날짜 캐시. 프론트의 기존 잡담 타이머(`Mascot.svelte`)는 발화 시점에 `get_chatter_pool` 커맨드로 pull하여 LLM 풀 + 정적 큐레이션에서 최근 3개를 회피하고 pick. 이벤트 추가 없음. 엔진 미설정이면 풀이 비어 정적만 나오는 자연 폴백.

**Tech Stack:** Rust(rusqlite·serde_json·chrono·anyhow), Tauri v2(tray menu·commands), Svelte 5(runes), vitest.

**Spec:** `docs/specs/2026-07-10-mascot-chatter-design.md` (§2 결정, §3 흐름, §4 파일, §5 프롬프트, §6 테스트)

## Global Constraints

- Windows 전용, Tauri v2 — v1 API(SystemTray 등) 금지. 트레이는 기존 `tray.rs`의 v2 `tauri::menu`/`tray` API만 사용.
- **매 cargo 명령 전 (Git Bash) 필수:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- 정밀도의 선: LLM 잡담은 `ChatContext`의 사실·수치에만 근거(프롬프트에 "지어내지 마세요" 명시). 정적 문구는 사실 주장 금지(수치는 `session_count` 템플릿만 허용).
- 잡담 풀 크기 N=5 (`CHATTER_POOL_SIZE`), 각 줄 40자 이내 지시, 표시 중복은 최근 3개 회피.
- 무변경 유지: 케이던스 값(normal 20–40분/low 60–120분/off), 새벽 1–7시 침묵, 말풍선 표시 중 침묵, 말풍선 UI(X 닫기·본문 클릭), realtime_advice 채널.
- "오늘" = `chrono::Local` 날짜(`%Y-%m-%d`) — daily_line 정책 계승. 3지점(파이프라인·커맨드·캐시 키) 일관.
- 네트워크(LLM)는 반드시 store 락 밖에서 호출(①read→drop ②network ③upsert 규율).
- 브랜치 `feat/mascot-chatter`(이미 생성됨, 스펙 커밋 f6b551d). 태스크마다 커밋.

---

### Task 1: store — `chatter_pool` 테이블 + get/upsert

**Files:**
- Modify: `crates/core/src/store.rs` — SCHEMA 상수의 `daily_line` 테이블 정의 바로 뒤, 메서드는 `upsert_daily_line` 바로 뒤(632행 부근), 테스트는 `daily_line_roundtrip_and_upsert_overwrites` 뒤

**Interfaces:**
- Consumes: 없음 (독립)
- Produces: `SqliteStore::get_chatter_pool(&self, date: &str) -> Result<Option<(Vec<String>, String)>>` (lines, fingerprint), `SqliteStore::upsert_chatter_pool(&self, date: &str, lines: &[String], fingerprint: &str) -> Result<()>` — Task 4·5가 호출

- [ ] **Step 1: 실패하는 테스트 작성**

`store.rs` 테스트 모듈의 `daily_line_roundtrip_and_upsert_overwrites` 테스트 바로 뒤에 추가:

```rust
    #[test]
    fn chatter_pool_roundtrip_and_upsert_overwrites() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 없으면 None
        assert_eq!(store.get_chatter_pool("2026-07-10").unwrap(), None);
        // upsert 후 (lines, fp) 라운드트립 — JSON 직렬화 왕복
        let lines = vec!["오늘 좀 바빴네".to_string(), "커밋은 자주".to_string()];
        store.upsert_chatter_pool("2026-07-10", &lines, "3|100|200|1").unwrap();
        assert_eq!(
            store.get_chatter_pool("2026-07-10").unwrap(),
            Some((lines, "3|100|200|1".to_string()))
        );
        // 같은 날짜 재upsert → 덮어씀. 빈 풀(idle 캐시)도 왕복 가능
        store.upsert_chatter_pool("2026-07-10", &[], "0|0|0|2").unwrap();
        assert_eq!(
            store.get_chatter_pool("2026-07-10").unwrap(),
            Some((Vec::new(), "0|0|0|2".to_string()))
        );
        // 다른 날짜는 독립적으로 None
        assert_eq!(store.get_chatter_pool("2099-01-01").unwrap(), None);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run (빌드 레시피 export 후):
```bash
cargo test -p agent-mentor chatter_pool_roundtrip
```
Expected: 컴파일 에러 — `no method named get_chatter_pool`

- [ ] **Step 3: 최소 구현**

SCHEMA 상수에서 `daily_line` 테이블 정의(65–67행) 바로 뒤에 추가:

```sql
CREATE TABLE IF NOT EXISTS chatter_pool (
  date TEXT PRIMARY KEY, lines TEXT NOT NULL, fingerprint TEXT NOT NULL
);
```

`upsert_daily_line` 메서드 바로 뒤에 추가 (lines는 JSON 배열 문자열로 저장):

```rust
    pub fn get_chatter_pool(&self, date: &str) -> Result<Option<(Vec<String>, String)>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT lines, fingerprint FROM chatter_pool WHERE date=?1",
                params![date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((lines_json, fp)) => Ok(Some((serde_json::from_str(&lines_json)?, fp))),
            None => Ok(None),
        }
    }

    pub fn upsert_chatter_pool(&self, date: &str, lines: &[String], fingerprint: &str) -> Result<()> {
        let lines_json = serde_json::to_string(lines)?;
        self.conn.execute(
            "INSERT INTO chatter_pool (date, lines, fingerprint) VALUES (?1,?2,?3)
             ON CONFLICT(date) DO UPDATE SET lines=?2, fingerprint=?3",
            params![date, lines_json, fingerprint],
        )?;
        Ok(())
    }
```

참고: `Result`는 store.rs의 기존 별칭(anyhow) 그대로. `serde_json`은 core 의존성에 이미 있음(`evidence` JSON 등에서 사용). 기존 DB는 `CREATE TABLE IF NOT EXISTS`로 오픈 시 자동 추가(daily_line 선례 — 별도 마이그레이션 불필요).

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor chatter_pool_roundtrip
```
Expected: PASS. 이어서 전체 회귀:
```bash
cargo test -p agent-mentor
```
Expected: 전부 PASS, 경고 0

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/store.rs
git commit -m "feat(store): chatter_pool 테이블 + get/upsert (잡담 풀 날짜 캐시)"
```

---

### Task 2: mascot.rs — 잡담 프롬프트·파싱 순수 조각 + facts_block 공용화

**Files:**
- Modify: `crates/core/src/mascot.rs` — `build_daily_line_prompt`(68–101행) 리팩터 + 신규 함수들, 테스트는 `daily_line_tests` 모듈 뒤에 신규 `chatter_tests` 모듈

**Interfaces:**
- Consumes: `crate::chat::ChatContext`(필드: `user_name: String, date: String, session_count: u64, tok_input: u64, tok_output: u64, est_tokens_saved_total: u64, findings: Vec<(String, String)>`), `crate::diary::voice_guidance()`, 같은 모듈의 private `strip_wrapping_quotes(&str) -> &str`
- Produces: `pub const CHATTER_POOL_SIZE: usize = 5`, `pub fn build_chatter_prompt(ctx: &ChatContext, n: usize) -> String`, `pub fn parse_chatter_lines(raw: &str, max_n: usize) -> Vec<String>` — Task 3이 호출. private `fn facts_block(ctx) -> String`(daily·chatter 프롬프트 공용)

**배경:** #1 최종 리뷰(opus)가 오늘 요약/findings 블록의 중복을 후속 cleanup 후보로 지적했다. 잡담 프롬프트가 세 번째 사본이 될 참이므로, **mascot.rs 안에서만** private `facts_block` 헬퍼로 공용화한다(chat.rs와의 파일 간 결합은 계속 회피 — 프롬프트 독립 진화 여지 유지). `build_daily_line_prompt`의 출력은 바이트 단위로 동일해야 한다(기존 테스트가 마커로 검증).

- [ ] **Step 1: 실패하는 테스트 작성**

`mascot.rs` 파일 끝(`daily_line_tests` 모듈 뒤)에 추가:

```rust
#[cfg(test)]
mod chatter_tests {
    use super::*;
    use crate::chat::ChatContext;

    fn ctx(session_count: u64, tin: u64, tout: u64, n_findings: usize) -> ChatContext {
        ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-10".into(),
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
    fn chatter_prompt_carries_persona_facts_voice_and_directives() {
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), 5);
        assert!(p.contains("주인"));                         // 페르소나 호칭
        assert!(p.contains("jibin"));                        // 유저명
        assert!(p.contains("3건"));                          // 오늘 세션 수(사실)
        assert!(p.contains(crate::diary::voice_guidance())); // voice_guidance verbatim
        assert!(p.contains("잡담"));                         // 잡담 지시
        assert!(p.contains("5개"));                          // 개수 N
        assert!(p.contains("40자"));                         // 길이 상한
        assert!(p.contains("지어내지 마세요"));              // 정밀도의 선
        assert!(p.contains("조언"));                         // "코칭 조언처럼 굴지 말 것"
        assert!(p.contains("detail 0"));                     // findings 블록 포함
    }

    #[test]
    fn parse_keeps_clean_lines_up_to_max() {
        let raw = "오늘 세션 셋. 손이 빨랐다\n커밋은 자주, 후회는 짧게\n토큰 아낀 날";
        assert_eq!(
            parse_chatter_lines(raw, 5),
            vec![
                "오늘 세션 셋. 손이 빨랐다".to_string(),
                "커밋은 자주, 후회는 짧게".to_string(),
                "토큰 아낀 날".to_string(),
            ]
        );
        // max_n 초과는 절단
        assert_eq!(parse_chatter_lines("a\nb\nc", 2), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_strips_bullets_numbers_quotes_and_blanks() {
        let raw = "- 불릿 잡담\n2. 번호 잡담\n\n  \n\"따옴표 잡담\"\n* 별표 잡담\n3) 괄호번호 잡담";
        assert_eq!(
            parse_chatter_lines(raw, 10),
            vec![
                "불릿 잡담".to_string(),
                "번호 잡담".to_string(),
                "따옴표 잡담".to_string(),
                "별표 잡담".to_string(),
                "괄호번호 잡담".to_string(),
            ]
        );
    }

    #[test]
    fn parse_returns_empty_for_garbage() {
        assert_eq!(parse_chatter_lines("", 5), Vec::<String>::new());
        assert_eq!(parse_chatter_lines("  \n\n\t\n\"\"", 5), Vec::<String>::new());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor chatter
```
Expected: 컴파일 에러 — `cannot find function build_chatter_prompt`

- [ ] **Step 3: 구현**

③-a. `build_daily_line_prompt` 바로 위에 private 공용 헬퍼 추가:

```rust
/// 오늘 요약 + 활성 findings 사실 블록 — 오늘의 한마디·잡담 프롬프트가 공용하는 사실 재료.
/// (#1 최종 리뷰의 중복 지적 해소 — mascot.rs 안에서만 공용, chat.rs와는 독립 진화 유지)
fn facts_block(ctx: &crate::chat::ChatContext) -> String {
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
        "[오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}",
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}
```

③-b. `build_daily_line_prompt`를 헬퍼 사용으로 교체 (출력 바이트 동일 — findings_block 조립부 삭제):

```rust
pub fn build_daily_line_prompt(ctx: &crate::chat::ChatContext) -> String {
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}\n\n\
         오늘 하루의 기분이나 재치를 담아 짧은 한 문장(40자 이내)으로 표현하세요. \
         대화가 아니라 오늘을 한마디로 요약하는 혼잣말입니다. 딱 한 문장만 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
    )
}
```

③-c. 그 뒤에 잡담 프롬프트·파싱 추가:

```rust
/// 잡담 풀 크기 — 스캔당 LLM 1회 호출로 배치 생성하는 잡담 개수 (스펙 §2).
pub const CHATTER_POOL_SIZE: usize = 5;

/// 잡담 풀 생성 시스템 프롬프트 — 오늘의 한마디와 동일 인물·동일 사실 재료,
/// 지시만 "가벼운 잡담 N개"로 다름 (스펙 §5). 코칭 조언 채널(realtime_advice)과의
/// 역할 분리를 프롬프트에 명시한다.
pub fn build_chatter_prompt(ctx: &crate::chat::ChatContext, n: usize) -> String {
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}\n\n\
         위 요약을 재료로, 상주 마스코트가 가끔 툭 던질 가벼운 잡담·혼잣말을 {n}개 만드세요. \
         코칭 조언이나 보고처럼 굴지 마세요(조언은 다른 채널이 합니다). \
         한 줄에 하나씩, 각 40자 이내로, 번호·불릿·따옴표 없이 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
    )
}

/// LLM 출력에서 잡담 줄을 방어적으로 추출 — trim, 선두 불릿/번호 제거,
/// 감싼 따옴표 한 겹 제거, 빈 줄 제거, 최대 max_n개 (스펙 §5 파싱 내성).
pub fn parse_chatter_lines(raw: &str, max_n: usize) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let mut l = line.trim();
            for p in ["- ", "• ", "* "] {
                if let Some(rest) = l.strip_prefix(p) {
                    l = rest;
                    break;
                }
            }
            l = strip_leading_number(l).trim_start();
            let l = strip_wrapping_quotes(l).trim();
            if l.is_empty() { None } else { Some(l.to_string()) }
        })
        .take(max_n)
        .collect()
}

/// "1. " / "2) " 류 선두 번호 매김 제거 — 번호가 아니면 원문 그대로.
fn strip_leading_number(s: &str) -> &str {
    let rest = s.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() < s.len() {
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r;
        }
    }
    s
}
```

주의: `strip_wrapping_quotes`는 같은 모듈의 기존 private fn — 가시성 변경 불필요.

- [ ] **Step 4: 테스트 통과 확인 (신규 + 기존 daily_line 회귀)**

```bash
cargo test -p agent-mentor mascot
```
Expected: `chatter_tests` 4개 + 기존 `daily_line_tests` 6개(리팩터 회귀) 전부 PASS. 이어서:
```bash
cargo test -p agent-mentor
```
Expected: 전부 PASS, 경고 0

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/mascot.rs
git commit -m "feat(mascot): 잡담 프롬프트·파싱 순수 조각 + facts_block 공용화"
```

---

### Task 3: mascot.rs — compute_chatter_pool 오케스트레이션

**Files:**
- Modify: `crates/core/src/mascot.rs` — `compute_daily_line` 뒤에 추가, 테스트는 `chatter_tests` 모듈에 추가

**Interfaces:**
- Consumes: Task 2의 `build_chatter_prompt`/`parse_chatter_lines`/`CHATTER_POOL_SIZE`, 기존 `facts_fingerprint(ctx)`, `crate::diary::engine::Engine`(`generate(&self, system: &str, user: &str) -> anyhow::Result<GenOutput>`, `.text: String`), 테스트는 `crate::diary::engine::MockEngine { canned: String }`
- Produces: `pub fn compute_chatter_pool(engine: &dyn Engine, ctx: &ChatContext, cached_fp: Option<&str>) -> anyhow::Result<Option<(Vec<String>, String)>>` — Task 4가 호출. 반환 규약: `None`=skip(fp 동일) / `Some((lines, fp))`=이 값으로 캐시하라(빈 lines 포함)

- [ ] **Step 1: 실패하는 테스트 작성**

`chatter_tests` 모듈에 추가:

```rust
    use crate::diary::engine::MockEngine;

    #[test]
    fn compute_pool_generates_and_parses_when_active_and_uncached() {
        let eng = MockEngine { canned: "오늘 세션 셋, 좀 굴렀다\n- 커밋은 자주\n\"토큰 아낀 날\"".into() };
        let out = compute_chatter_pool(&eng, &ctx(3, 100, 200, 1), None).unwrap();
        assert_eq!(
            out,
            Some((
                vec![
                    "오늘 세션 셋, 좀 굴렀다".to_string(),
                    "커밋은 자주".to_string(),
                    "토큰 아낀 날".to_string(),
                ],
                "3|100|200|1".to_string()
            ))
        );
    }

    #[test]
    fn compute_pool_skips_when_fingerprint_matches_cache() {
        let eng = MockEngine { canned: "안 나와야 함".into() };
        let c = ctx(3, 100, 200, 1);
        let fp = facts_fingerprint(&c);
        assert_eq!(compute_chatter_pool(&eng, &c, Some(&fp)).unwrap(), None);
    }

    #[test]
    fn compute_pool_returns_empty_without_engine_when_idle() {
        // 활동 0건 → 엔진 미호출·빈 풀 캐시 (canned가 파싱돼 나오면 엔진이 불렸다는 뜻이라 실패)
        let eng = MockEngine { canned: "엔진이 불렸다면 이게 나온다".into() };
        let out = compute_chatter_pool(&eng, &ctx(0, 0, 0, 2), None).unwrap();
        assert_eq!(out, Some((Vec::new(), "0|0|0|2".to_string())));
    }

    #[test]
    fn compute_pool_caches_empty_when_output_is_garbage() {
        // 전부 파싱 실패 → 빈 풀 + fp 캐시 (다음 스캔까지 재시도 안 함, 프론트는 정적 폴백)
        let eng = MockEngine { canned: "  \n\n".into() };
        let out = compute_chatter_pool(&eng, &ctx(3, 100, 200, 1), None).unwrap();
        assert_eq!(out, Some((Vec::new(), "3|100|200|1".to_string())));
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor compute_pool
```
Expected: 컴파일 에러 — `cannot find function compute_chatter_pool`

- [ ] **Step 3: 구현**

`compute_daily_line` 함수 뒤에 추가:

```rust
/// 잡담 풀 계산 — store 접근 없음, 네트워크만. 호출자가 락 밖에서 부른다
/// (compute_daily_line 선례). 반환: None=재생성 불필요(fp 동일, skip) /
/// Some((lines, fp))=이 값으로 캐시하라.
/// - fp가 캐시와 동일: None(skip).
/// - 오늘 활동 0건(session_count==0): 엔진 호출 없이 빈 풀(정적 폴백은 프론트 담당).
/// - 그 외: 엔진 1회 호출로 잡담 N개 배치 생성·파싱(전부 실패면 빈 풀 캐시).
pub fn compute_chatter_pool(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(Vec<String>, String)>> {
    let fp = facts_fingerprint(ctx);
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    if ctx.session_count == 0 {
        return Ok(Some((Vec::new(), fp)));
    }
    let system = build_chatter_prompt(ctx, CHATTER_POOL_SIZE);
    let raw = engine.generate(&system, "")?.text;
    Ok(Some((parse_chatter_lines(&raw, CHATTER_POOL_SIZE), fp)))
}
```

결정 순서 주의: **cached_fp skip 검사가 session_count==0보다 먼저** — idle-fp로 저장된 풀은 항상 빈 풀이므로 skip해도 캐시 상태 동일(#1 gemini 리뷰 #1과 동일한 논리, idle 매 스캔 재-upsert 방지).

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor
```
Expected: 전부 PASS(신규 4개 포함), 경고 0

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/mascot.rs
git commit -m "feat(mascot): compute_chatter_pool 오케스트레이션 (fp 게이트·idle 빈 풀)"
```

---

### Task 4: pipeline — maybe_generate_chatter_pool 배선

**Files:**
- Modify: `src-tauri/src/pipeline.rs` — `run_pipeline_once`의 `maybe_generate_daily_line(app, &state.store);` 호출(109행) 뒤에 호출 추가, 함수는 `maybe_generate_daily_line` 함수 정의 뒤(222행 부근)에 추가

**Interfaces:**
- Consumes: Task 1 `store.get_chatter_pool`/`upsert_chatter_pool`, Task 3 `agent_mentor::mascot::compute_chatter_pool`, 기존 `crate::commands::chat_context_inner(&store)`, `OpenAiCompatEngine::from_env()`
- Produces: 없음 (파이프라인 내부 함수). 이벤트 emit 없음 — 프론트는 타이머 발화 시 pull(스펙 §2)

주의: 이 모듈은 `#[cfg(not(test))]` runtime — 유닛 테스트 없음(Task 3이 로직 커버, 여기는 컴파일 게이트). `app` 핸들은 받지 않는다(emit 없음 — #1 Task 4에서 미사용 파라미터 경고를 겪은 선례).

- [ ] **Step 1: 구현**

`run_pipeline_once`의 성공 분기에서:

```rust
                // 오늘의 한마디 — 엔진 없으면 no-op, 실패는 조용히(다음 스캔 재시도)
                maybe_generate_daily_line(app, &state.store);
                // 잡담 풀 — 동일 규율, 이벤트 없음(프론트 타이머가 pull)
                maybe_generate_chatter_pool(&state.store);
```

`maybe_generate_daily_line` 함수 정의 뒤에 추가:

```rust
    /// 잡담 풀 — scan:done마다 fp가 stale할 때만 재생성 (스펙 §3). 엔진 없으면 no-op.
    /// 네트워크(LLM)는 daily-line과 동일하게 store 락 밖에서 호출. 이벤트는 emit하지
    /// 않는다 — 프론트 잡담 타이머가 발화 시점에 get_chatter_pool로 pull한다.
    fn maybe_generate_chatter_pool(store_mutex: &std::sync::Mutex<SqliteStore>) {
        let Some(engine) = OpenAiCompatEngine::from_env() else { return; };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // ① 락: 오늘 컨텍스트 + 캐시된 fingerprint 읽기 → 즉시 해제
        let (ctx, cached_fp) = match store_mutex.lock() {
            Ok(store) => {
                let ctx = match crate::commands::chat_context_inner(&store) {
                    Ok(c) => c,
                    Err(e) => { log::warn!("chatter chat_context 실패: {e}"); return; }
                };
                let cached_fp = match store.get_chatter_pool(&today) {
                    Ok(v) => v.map(|(_, fp)| fp),
                    Err(e) => { log::warn!("get_chatter_pool 실패: {e}"); return; }
                };
                (ctx, cached_fp)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here — 네트워크 전에 락 해제

        // ② 락 없이 compute (0건 빈 풀 or 네트워크 생성). None이면 skip.
        let outcome = match agent_mentor::mascot::compute_chatter_pool(&engine, &ctx, cached_fp.as_deref()) {
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

- [ ] **Step 2: 컴파일 게이트**

```bash
cargo build -p agent-mentor-app
```
Expected: 성공, 경고 0 (특히 unused variable 없음)

- [ ] **Step 3: 커밋**

```bash
git add src-tauri/src/pipeline.rs
git commit -m "feat(pipeline): 스캔 편승 잡담 풀 생성 배선 (fp stale 시만, 락 밖 네트워크)"
```

---

### Task 5: get_chatter_pool 커맨드 + 등록

**Files:**
- Modify: `src-tauri/src/commands.rs` — `daily_line_inner`(47–49행) 뒤에 inner, `get_daily_line`(283–288행) 뒤에 command, 테스트는 기존 `daily_line_inner_returns_cached_text_or_none`(396행 부근) 뒤
- Modify: `src-tauri/src/lib.rs` — invoke_handler의 `commands::get_daily_line,`(145행) 뒤에 등록

**Interfaces:**
- Consumes: Task 1 `store.get_chatter_pool`, 기존 `lock(&state)` 헬퍼
- Produces: `#[tauri::command(async)] get_chatter_pool(state) -> Result<Vec<String>, String>` — 프론트가 `invoke('get_chatter_pool')`로 호출(Task 7). 오늘 풀 없으면 빈 벡터(에러 아님)

- [ ] **Step 1: 실패하는 테스트 작성**

commands.rs 테스트 모듈의 `daily_line_inner_returns_cached_text_or_none` 뒤에 추가:

```rust
    #[test]
    fn chatter_pool_inner_returns_lines_or_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 캐시 없음 → 빈 벡터 (에러 아님)
        assert_eq!(chatter_pool_inner(&store, "2026-07-10").unwrap(), Vec::<String>::new());
        store
            .upsert_chatter_pool("2026-07-10", &["잡담 하나".to_string(), "잡담 둘".to_string()], "3|1|2|0")
            .unwrap();
        assert_eq!(
            chatter_pool_inner(&store, "2026-07-10").unwrap(),
            vec!["잡담 하나".to_string(), "잡담 둘".to_string()]
        );
        // 다른 날짜 → 빈 벡터
        assert_eq!(chatter_pool_inner(&store, "2099-01-01").unwrap(), Vec::<String>::new());
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor-app chatter_pool_inner
```
Expected: 컴파일 에러 — `cannot find function chatter_pool_inner`

- [ ] **Step 3: 구현**

`daily_line_inner` 뒤에:

```rust
pub fn chatter_pool_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Vec<String>> {
    Ok(store.get_chatter_pool(date)?.map(|(lines, _fp)| lines).unwrap_or_default())
}
```

`get_daily_line` 커맨드 뒤에:

```rust
#[tauri::command(async)]
pub fn get_chatter_pool(state: State<AppState>) -> Result<Vec<String>, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let guard = lock(&state)?;
    chatter_pool_inner(&*guard, &today).map_err(|e| e.to_string())
}
```

`lib.rs` invoke_handler에 등록:

```rust
                commands::get_daily_line,
                commands::get_chatter_pool,
```

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor-app && cargo build -p agent-mentor-app
```
Expected: 전부 PASS(신규 1개 포함), 빌드 경고 0

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): get_chatter_pool 커맨드 (오늘 풀, 없으면 빈 벡터)"
```

---

### Task 6: tray — "잡담" 빈도 서브메뉴

**Files:**
- Modify: `src-tauri/src/tray.rs` — 메뉴 구성(protect 뒤·scan 앞)과 `on_menu_event` 핸들러

**Interfaces:**
- Consumes: 기존 `store.get_setting("chatter_level")`/`set_setting`, `settings:changed` emit 관례(realtime 선례). `Mascot.svelte`는 `onSettingsChanged`→`loadSettings()`로 이미 반영하며, 잡담 타이머 `$effect`가 `chatterLevel`을 동기적으로 읽어 재스케줄됨(프론트 수정 불필요)
- Produces: 트레이 서브메뉴 "잡담" — 자주(normal)/가끔(low)/안 함(off) 3택 1 수동 라디오

주의: runtime 코드 — 유닛 테스트 없음(컴파일 게이트 + 수동 E2E). muda `CheckMenuItem`은 클릭 시 checked 자동 토글 — **store를 소스오브트루스로**, 클릭 후 세 항목의 체크를 전부 명시적으로 set한다(realtime/protect 선례).

- [ ] **Step 1: 구현**

①. import에 `SubmenuBuilder` 추가:

```rust
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
```

②. `protect` CheckMenuItem 생성 뒤·`scan` 앞에 서브메뉴 구성 추가:

```rust
    let chatter_level = {
        let state = app.state::<AppState>();
        let guard = state.store.lock().ok();
        let raw = guard
            .and_then(|s| s.get_setting("chatter_level").ok().flatten())
            .unwrap_or_else(|| "low".into());
        // 알 수 없는 값은 low로 정규화 (Mascot.svelte 기본값과 일치)
        if raw == "normal" || raw == "off" { raw } else { "low".to_string() }
    };
    let chatter_normal =
        CheckMenuItem::with_id(app, "chatter_normal", "자주", true, chatter_level == "normal", None::<&str>)?;
    let chatter_low =
        CheckMenuItem::with_id(app, "chatter_low", "가끔", true, chatter_level == "low", None::<&str>)?;
    let chatter_off =
        CheckMenuItem::with_id(app, "chatter_off", "안 함", true, chatter_level == "off", None::<&str>)?;
    let chatter_menu = SubmenuBuilder::with_id(app, "chatter", "잡담")
        .items(&[&chatter_normal, &chatter_low, &chatter_off])
        .build()?;
```

③. `Menu::with_items`에 protect 뒤로 삽입:

```rust
    let menu = Menu::with_items(app, &[&open, &mascot, &realtime, &protect, &chatter_menu, &scan, &autostart, &quit])?;
```

④. 핸들러용 clone 3개 추가(기존 clone들 옆):

```rust
    let chatter_normal_item = chatter_normal.clone();
    let chatter_low_item = chatter_low.clone();
    let chatter_off_item = chatter_off.clone();
```

⑤. `on_menu_event` match에 분기 추가("protect" 뒤):

```rust
            "chatter_normal" | "chatter_low" | "chatter_off" => {
                use tauri::Emitter;
                let level = match e.id().as_ref() {
                    "chatter_normal" => "normal",
                    "chatter_off" => "off",
                    _ => "low",
                };
                if let Ok(store) = app.state::<AppState>().store.lock() {
                    let _ = store.set_setting("chatter_level", level);
                }
                // 수동 라디오: muda 자동 토글을 덮어써 선택 항목만 체크 (store가 소스오브트루스)
                let _ = chatter_normal_item.set_checked(level == "normal");
                let _ = chatter_low_item.set_checked(level == "low");
                let _ = chatter_off_item.set_checked(level == "off");
                let _ = app.emit("settings:changed", ());
            }
```

- [ ] **Step 2: 컴파일 게이트**

```bash
cargo build -p agent-mentor-app
```
Expected: 성공, 경고 0

- [ ] **Step 3: 커밋**

```bash
git add src-tauri/src/tray.rs
git commit -m "feat(tray): 잡담 빈도 서브메뉴 자주/가끔/안 함 (chatter_level 노출)"
```

---

### Task 7: 프론트 — pickChatter 혼합 pick + 타이머 연동

**Files:**
- Modify: `src/lib/api.ts` — `getDailyLine`(97행) 뒤에 바인딩 추가
- Modify: `src/lib/robot/bubble.ts` — CHATTER 응원 결 5개 추가, `chatterBubble` 제거 → `chatterCandidates`/`pickChatter` 신설
- Modify: `src/lib/robot/bubble.test.ts` — chatterBubble 단언 교체 + pickChatter 테스트
- Modify: `src/Mascot.svelte` — 잡담 타이머 콜백에서 풀 pull + pick

**Interfaces:**
- Consumes: Task 5 커맨드(`invoke<string[]>('get_chatter_pool')`), 기존 `Bubble` 타입·`getSummary`·타이머 구조
- Produces: `chatterCandidates(pool: string[], summary: { session_count: number } | null): string[]`, `pickChatter(pool, summary, recent: string[], rand: () => number): Bubble` — Mascot.svelte가 사용. **`chatterBubble`은 제거**(이 변경으로 미사용이 됨 — 유일 호출처가 pickChatter로 교체)

- [ ] **Step 1: 실패하는 테스트 작성**

`bubble.test.ts`에서 ①기존 결합 테스트의 chatterBubble 단언 2줄을 제거하고 ②pickChatter 테스트를 추가:

```ts
import { describe, expect, it } from 'vitest';
import { adviceBubble, chatterCandidates, diaryBubble, findingBubble, occasionBubble, pickChatter } from './bubble';
```

기존 `'diary는 diary 탭, occasion은 첫 라벨, chatter는 수치 삽입'` 테스트를 다음으로 교체:

```ts
  it('diary는 diary 탭, occasion은 첫 라벨', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
  });
```

파일 끝에 추가:

```ts
describe('pickChatter', () => {
  it('rand 주입으로 결정적 — LLM 풀 항목이 후보에 포함된다', () => {
    const b = pickChatter(['풀A', '풀B'], null, [], () => 0);
    expect(b).toEqual({ kind: 'chatter', tab: 'home', text: '풀A' });
  });
  it('최근 표시분은 제외한다', () => {
    const b = pickChatter(['풀A', '풀B'], null, ['풀A'], () => 0);
    expect(b.text).toBe('풀B');
  });
  it('빈 풀이면 정적 후보만으로 pick — 수치 삽입 유지', () => {
    const b = pickChatter([], { session_count: 7 }, [], () => 0);
    expect(b.text).toContain('7'); // CHATTER[0]이 세션 수 삽입
  });
  it('전 후보가 recent면 recent를 무시한다(기아 방지)', () => {
    const all = chatterCandidates(['풀A'], null);
    const b = pickChatter(['풀A'], null, all, () => 0);
    expect(all).toContain(b.text);
  });
});
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
npx vitest run src/lib/robot/bubble.test.ts
```
Expected: FAIL — `pickChatter`/`chatterCandidates` export 없음

- [ ] **Step 3: 구현**

③-a. `bubble.ts`의 CHATTER 배열 끝에 응원 결 5개 추가(사실 주장 없음 — 수치는 session_count 템플릿만):

```ts
  () => '주인, 오늘도 제가 응원해요. 조용히, 근데 진심으로',
  () => '막히면 잠깐 산책 — 코드는 도망 안 가요',
  () => '어제보다 한 커밋만 더. 그게 성장이에요',
  (n) => (n === null ? '오늘의 주인도 응원합니다!' : `${n}세션째 달리는 주인, 존경해요`),
  () => '실패한 시도도 데이터예요. 제가 다 보고 있었어요',
```

③-b. `chatterBubble` 함수를 제거하고 그 자리에:

```ts
/** 잡담 후보 전체 — LLM 풀(사용기록 연계) + 정적 큐레이션(잡담/응원, summary 렌더). */
export function chatterCandidates(
  pool: string[],
  summary: { session_count: number } | null,
): string[] {
  return [...pool, ...CHATTER.map((f) => f(summary?.session_count ?? null))];
}

/** 잡담 pick — 후보에서 최근 표시분(recent)을 제외하고 균등 랜덤.
 *  제외 후 후보가 비면 recent를 무시하고 전체에서 pick(기아 방지). rand는 [0,1) 주입. */
export function pickChatter(
  pool: string[],
  summary: { session_count: number } | null,
  recent: string[],
  rand: () => number,
): Bubble {
  const all = chatterCandidates(pool, summary);
  const fresh = all.filter((t) => !recent.includes(t));
  const candidates = fresh.length ? fresh : all;
  return { kind: 'chatter', tab: 'home', text: candidates[Math.floor(rand() * candidates.length)] };
}
```

③-c. `api.ts`의 `getDailyLine` 뒤에:

```ts
export const getChatterPool = () => invoke<string[]>('get_chatter_pool');
```

③-d. `Mascot.svelte` — import 교체:

```ts
  import {
    emitOccasionToday, getChatterPool, getMascotSeed, getSettings, getSummary, getTodayOccasions,
    listFindings, openChatTab, setSetting,
    onDiaryReady, onNewFindings, onScanDone, onSettingsChanged,
  } from './lib/api';
  import { adviceBubble, diaryBubble, findingBubble, occasionBubble, pickChatter, type Bubble } from './lib/robot/bubble';
```

상태 추가(`lastAdviceKey` 옆, 일반 변수 — 렌더 비의존):

```ts
  let recentChatter: string[] = []; // 최근 표시 잡담 3개 (세션-로컬, 영속화 안 함)
```

잡담 타이머 콜백 교체 (기존 게이트·구조 유지, pick만 변경):

```ts
      timer = setTimeout(async () => {
        const hour = new Date().getHours();
        if (chatterLevel !== 'off' && !(hour >= 1 && hour < 7) && bubble === null) {
          const [summary, pool] = await Promise.all([
            getSummary().catch(() => null),
            getChatterPool().catch(() => [] as string[]),
          ]);
          const b = pickChatter(pool, summary, recentChatter, Math.random);
          recentChatter = [...recentChatter.slice(-2), b.text];
          showBubble(b);
        }
        schedule();
      }, delayMin * 60_000);
```

- [ ] **Step 4: 테스트·빌드 통과 확인**

```bash
npx vitest run && npm run build
```
Expected: vitest 전부 PASS(신규 4개 포함), build 성공

- [ ] **Step 5: 커밋**

```bash
git add src/lib/api.ts src/lib/robot/bubble.ts src/lib/robot/bubble.test.ts src/Mascot.svelte
git commit -m "feat(mascot-front): 잡담 말풍선 LLM 풀+정적 혼합 pick (최근 3개 회피)"
```

---

## 최종 게이트 (전 태스크 완료 후)

- [ ] 전체 그린 확인:

```bash
cargo test -p agent-mentor && cargo test -p agent-mentor-app && cargo build -p agent-mentor-app
npx vitest run && npm run build
```
Expected: 전부 PASS·경고 0

- [ ] 수동 E2E (실엔진 `.env` localhost:4444, dev 자동 로드):
  1. 스캔 후 `chatter_pool` 생성 확인(로그 또는 DB `SELECT * FROM chatter_pool`)
  2. 잡담 말풍선에 LLM 잡담·정적 문구가 섞여 표시되는지(빈도 테스트는 `chatter_level=normal`로)
  3. 트레이 "잡담" 서브메뉴 3택 1 라디오 동작, "안 함" 시 침묵
  4. 엔진 미설정(.env 제거) 시 정적만 표시
  5. X 닫기·본문 클릭(홈 탭 열기)·다른 말풍선 우선 기존 동작 유지
