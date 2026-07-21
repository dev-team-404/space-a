# 코칭 데이터 위생 (PR1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** resume 포크 복제·다중 라인 usage 반복·인터럽트 마커·tokenizer 구멍으로 오염된 수집 데이터를 쓰기 시점 논리 dedup 키로 정화한다 (스펙 §3: [2026-07-21-repeat-coaching-judgment-redesign.md](../specs/2026-07-21-repeat-coaching-judgment-redesign.md)).

**Architecture:** `NormalizedEvent`에 assistant `message.id`를 실어 나르고, `store::upsert_events`가 kind별 논리 식별자(tool_use_id / message.id / ts+내용해시)로 dedup_key를 만든다. 중복이 DB에 아예 들어오지 않으므로 롤업·룰·세션 수가 자동 교정된다. `user_version=6` 마이그레이션이 전체 재수집을 유도한다.

**Tech Stack:** Rust (crates/core, lib `agent_mentor`), rusqlite, sha2. 테스트는 인라인 `#[cfg(test)]`.

## Global Constraints

- **빌드·테스트는 네이티브 Windows PowerShell에서** — WSL 안에서 빌드/실행 금지 (a-mate/CLAUDE.md).
- 작업 디렉터리: `D:\Project\space-a\a-mate` (cargo workspace 루트). 모든 `cargo` 명령은 여기서 실행.
- 커밋 메시지: Conventional Commits, **영어**, 소문자 시작, 마침표 없음. 푸터에 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- 브랜치: `fix/coaching-data-hygiene` (스펙 커밋 `88bef3a`가 이미 실려 있음). main 직접 커밋 금지.
- 프라이버시 원칙: 프롬프트/명령 본문은 미리보기 120자 외 미저장, 시크릿은 evidence에 노출 금지.
- 주석은 한국어, 기존 파일의 주석 밀도·관례를 따른다.

---

### Task 1: `NormalizedEvent`에 assistant `message.id` 운반

**Files:**
- Modify: `a-mate/crates/core/src/model.rs:107-121` (`NormalizedEvent`에 필드 추가)
- Modify: `a-mate/crates/core/src/adapter.rs:199-216` (`mk` 클로저에서 추출)
- Test: `a-mate/crates/core/src/adapter.rs` (tests 모듈)
- Modify(기계적): `NormalizedEvent` 구조체 리터럴을 가진 모든 테스트/시드 헬퍼 — 컴파일러가 지목. 최소한 다음 파일: `store.rs`(tests), `skill_draft.rs`(tests), `rules/r5_repeated_read.rs`, `rules/r6_repeated_prompts.rs`, `rules/r23_tool_sequences.rs`, `rules/r10_automation_burst.rs`, `rules/session_stats.rs`

**Interfaces:**
- Produces: `NormalizedEvent.msg_id: Option<String>` — assistant 라인의 `message.id`(`msg_…`). Task 2의 dedup 키가 소비.

- [ ] **Step 1: 실패하는 테스트 작성**

`adapter.rs` tests 모듈에 추가 (기존 `map_assistant_line_yields_turn_and_tool_calls` 근처):

```rust
#[test]
fn map_assistant_line_carries_message_id() {
    // resume 포크 복제본에서도 보존되는 message.id — 논리 dedup 키 재료 (스펙 §3.1)
    let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1",
        "message":{"id":"msg_011Ccp9b","model":"claude-opus-4-8",
        "usage":{"input_tokens":1,"output_tokens":2},
        "content":[{"type":"text","text":"hi"}]}}"#;
    let evs = adapter().map(line, "s1.jsonl", 0);
    let turn = evs.iter().find(|e| matches!(e.kind, EventKind::AssistantTurn { .. })).unwrap();
    assert_eq!(turn.msg_id.as_deref(), Some("msg_011Ccp9b"));
}

#[test]
fn map_user_line_has_no_message_id() {
    let line = r#"{"type":"user","sessionId":"s1","uuid":"u2",
        "message":{"role":"user","content":"이 함수 리팩토링 진행해줘"}}"#;
    let evs = adapter().map(line, "s1.jsonl", 0);
    assert!(evs.iter().all(|e| e.msg_id.is_none()));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test map_assistant_line_carries_message_id`
Expected: **컴파일 에러** — `no field msg_id on type NormalizedEvent`

- [ ] **Step 3: 최소 구현**

`model.rs`의 `NormalizedEvent`에 필드 추가 (`source_offset` 다음, `kind` 앞):

```rust
    pub source_offset: u64,
    /// assistant 라인의 API message id (`msg_…`) — resume 포크 복제본·다중 라인에서
    /// 보존되는 논리 식별자. AssistantTurn dedup 키 재료 (데이터 위생 스펙 §3.1).
    pub msg_id: Option<String>,
    pub kind: EventKind,
```

`adapter.rs`의 `mk` 클로저(199행 부근)에 추출 추가 (`source_offset` 줄 다음):

```rust
            source_offset: source_offset + off_bump,
            msg_id: v
                .get("message")
                .and_then(|m| m.get("id"))
                .and_then(|x| x.as_str())
                .map(String::from),
            kind,
```

- [ ] **Step 4: 컴파일 에러 기계적 수정**

Run: `cargo build --workspace`
컴파일러가 지목하는 모든 `NormalizedEvent` 구조체 리터럴(테스트 시드 헬퍼)에 `msg_id: None,`을 추가한다. `adapter.rs`의 `mk`만 실제 추출 — 나머지는 전부 `None`.

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test --workspace`
Expected: 전부 PASS (기존 테스트 포함 — 이 단계에서 동작 변화 없음)

- [ ] **Step 6: Commit**

```powershell
git add -A; git commit -m @'
feat(agent): carry assistant message id on normalized events

message.id survives resume-fork copies and multi-line splits, unlike
uuid. Groundwork for logical dedup keys (data hygiene spec 3.1).

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 2: 쓰기 시점 논리 dedup 키

**Files:**
- Modify: `a-mate/crates/core/src/store.rs:220-280` (`upsert_events` dedup 키 + `hash8` 헬퍼)
- Modify: `a-mate/crates/core/src/rules/r6_repeated_prompts.rs` (tests — ts 분산 시딩으로 교정)
- Test: `a-mate/crates/core/src/store.rs`, `a-mate/crates/core/src/rules/r6_repeated_prompts.rs`

**Interfaces:**
- Consumes: `NormalizedEvent.msg_id` (Task 1), `EventKind::ToolCall.tool_use_id`, `EventKind::ToolResult.tool_use_id`
- Produces: events dedup_key 규약 `tc:{host}:{tool_use_id}` / `tr:{host}:{tool_use_id}` / `at:{host}:{msg_id}`, prompt_events dedup_key 규약 `up:{host}:{project}:{ts}:{hash8(preview)}`. 식별자 부재 시 기존 `uuid:offset` 폴백.

- [ ] **Step 1: 실패하는 테스트 작성**

`store.rs` tests 모듈에 추가:

```rust
    /// 논리 dedup 키 테스트용 이벤트 — 포크 복제본은 uuid·session·file이 다르고
    /// ts·message.id·tool_use_id가 보존된다 (2026-07-21 실데이터 검증).
    fn hygiene_ev(
        sess: &str, file: &str, uuid: &str, off: u64,
        msg_id: Option<&str>, kind: crate::model::EventKind,
    ) -> crate::model::NormalizedEvent {
        crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-08T08:53:54.410Z".into()),
            source_file: file.into(), source_offset: off,
            msg_id: msg_id.map(Into::into), kind,
        }
    }

    #[test]
    fn resume_fork_copies_collapse_by_logical_identity() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage { input: 10, output: 1556, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        for (i, (sess, file)) in
            [("orig", "a.jsonl"), ("fork1", "b.jsonl"), ("fork2", "c.jsonl")].iter().enumerate()
        {
            store.upsert_events(&[
                hygiene_ev(sess, file, &format!("u{i}a"), 0, Some("msg_A"), turn()),
                hygiene_ev(sess, file, &format!("u{i}b"), 1, None, EventKind::ToolCall {
                    kind: ToolKind::from_raw_name("Bash"), raw_name: "Bash".into(),
                    target: Some("gh pr view".into()), tool_use_id: Some("toolu_X".into()),
                }),
                hygiene_ev(sess, file, &format!("u{i}c"), 2, None, EventKind::ToolResult {
                    tool_use_id: "toolu_X".into(), status: ResultStatus::Ok, result_len: 10,
                }),
            ]).unwrap();
        }
        let count = |k: &str| -> i64 {
            store.conn.query_row("SELECT COUNT(*) FROM events WHERE kind=?1",
                rusqlite::params![k], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("assistant_turn"), 1, "포크 복제 AssistantTurn은 1행");
        assert_eq!(count("tool_call"), 1, "포크 복제 ToolCall은 1행");
        assert_eq!(count("tool_result"), 1, "포크 복제 ToolResult는 1행");
        let out: i64 = store.conn.query_row(
            "SELECT COALESCE(SUM(tok_output),0) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(out, 1556, "토큰 이중 계산 금지");
    }

    #[test]
    fn multiline_assistant_message_counts_usage_once() {
        use crate::model::*;
        // 한 API 응답이 여러 assistant 라인으로 쪼개질 때 usage가 라인마다 반복된다
        // (실측: assistant 601줄 = 메시지 233개). AssistantTurn은 msg.id당 1행,
        // ToolCall은 tool_use_id가 블록마다 달라 전부 보존 (스펙 §3.1).
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage { input: 10, output: 500, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        let call = |tid: &str| EventKind::ToolCall {
            kind: ToolKind::from_raw_name("Read"), raw_name: "Read".into(),
            target: Some("a.rs".into()), tool_use_id: Some(tid.into()),
        };
        store.upsert_events(&[
            hygiene_ev("s1", "s1.jsonl", "u1", 0, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u2", 10, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u2t", 11, None, call("toolu_1")),
            hygiene_ev("s1", "s1.jsonl", "u3", 20, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u3t", 21, None, call("toolu_2")),
        ]).unwrap();
        let turns: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='assistant_turn'", [], |r| r.get(0)).unwrap();
        let calls: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='tool_call'", [], |r| r.get(0)).unwrap();
        let out: i64 = store.conn.query_row(
            "SELECT COALESCE(SUM(tok_output),0) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(turns, 1, "같은 msg.id의 AssistantTurn은 1행");
        assert_eq!(calls, 2, "블록별 ToolCall은 전부 보존");
        assert_eq!(out, 500, "usage 반복은 1회만 합산");
    }

    #[test]
    fn assistant_without_message_id_falls_back_to_uuid_offset() {
        use crate::model::*;
        // <synthetic> 등 message.id 없는 라인은 기존 uuid:offset 규칙 유지
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("<synthetic>"),
            usage: TokenUsage { input: 0, output: 0, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        store.upsert_events(&[
            hygiene_ev("s1", "s1.jsonl", "u1", 0, None, turn()),
            hygiene_ev("s1", "s1.jsonl", "u2", 10, None, turn()),
        ]).unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='assistant_turn'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "식별자 없으면 병합하지 않는다 (폴백)");
    }

    #[test]
    fn forked_prompt_copies_collapse_to_one_row() {
        use crate::model::*;
        // resume 포크: 같은 ts·내용의 프롬프트가 3개 세션 파일에 복제 → prompt_events 1행
        // (R6 "3개 세션" 부풀림의 근본 원인 — 스펙 §1.1-1)
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, (sess, file)) in
            [("orig", "a.jsonl"), ("fork1", "b.jsonl"), ("fork2", "c.jsonl")].iter().enumerate()
        {
            store.upsert_events(&[hygiene_ev(sess, file, &format!("u{i}"), 0, None,
                EventKind::UserPrompt { preview: "그 배포 버전 사내망에 올린 것 맞는지 확인해줘".into() },
            )]).unwrap();
        }
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM prompt_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "포크 복제 프롬프트는 1행");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test resume_fork_copies_collapse; cargo test multiline_assistant_message; cargo test forked_prompt_copies`
Expected: FAIL — 현재는 uuid:offset 키라 복제본이 각각 저장됨 (count 3 != 1)

- [ ] **Step 3: 구현**

`store.rs`에 `hash8` 헬퍼 추가 (`SqliteStore` impl 위, `migrate` 아래):

```rust
/// 논리 dedup 키용 내용 해시 — r6/r23의 hash8과 같은 규약 (sha256 앞 4바이트 hex).
fn hash8(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}
```

`upsert_events`의 기존 dedup_key 계산(226-229행)을 교체:

```rust
            // 논리 dedup 키 (데이터 위생 스펙 §3.1) — resume 포크 복제본과 다중 라인
            // usage 반복이 DB에 들어오지 않게 한다. uuid는 복제 시 재발급되지만
            // tool_use_id·message.id·ts는 보존된다 (2026-07-21 실데이터 검증).
            // 식별자가 없으면 기존 uuid:offset 규칙으로 폴백.
            let fallback = || match &e.uuid {
                Some(u) => format!("{}:{}", u, e.source_offset),
                None => format!("{}:{}", e.source_file, e.source_offset),
            };
            let dedup_key = match &e.kind {
                EventKind::ToolCall { tool_use_id: Some(tid), .. } => {
                    format!("tc:{}:{}", e.host, tid)
                }
                EventKind::ToolResult { tool_use_id, .. } if !tool_use_id.is_empty() => {
                    format!("tr:{}:{}", e.host, tool_use_id)
                }
                EventKind::AssistantTurn { .. } => match &e.msg_id {
                    Some(m) => format!("at:{}:{}", e.host, m),
                    None => fallback(),
                },
                _ => fallback(),
            };
```

`UserPrompt` 분기의 prompt_events INSERT(266-275행 부근)에서 키를 교체 — `if let Some(norm60) = …` 블록 안, INSERT 직전에:

```rust
                    if let Some(norm60) = crate::rules::r6_repeated_prompts::normalize(preview) {
                        // 포크 복제본은 ts·내용이 보존되므로 (host, project, ts, 내용해시)가
                        // 논리 식별자 — 같은 물리적 입력은 세션 파일이 몇 개든 1행.
                        let pe_key = match &e.ts {
                            Some(ts) => format!(
                                "up:{}:{}:{}:{}", e.host, e.project_id, ts, hash8(preview)
                            ),
                            None => dedup_key.clone(),
                        };
                        self.conn.execute(
                            "INSERT OR IGNORE INTO prompt_events
                               (dedup_key, session_id, host, project_id, ts,
                                source_file, source_offset, norm60, preview)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                            params![pe_key, e.session_id, e.host, e.project_id, e.ts,
                                    e.source_file, e.source_offset as i64, norm60, preview],
                        )?;
                    }
```

- [ ] **Step 4: 새 테스트 통과 확인**

Run: `cargo test resume_fork_copies_collapse; cargo test multiline_assistant_message; cargo test assistant_without_message_id; cargo test forked_prompt_copies`
Expected: 4건 모두 PASS

- [ ] **Step 5: 깨진 기존 R6 테스트를 ts 분산 시딩으로 교정**

Run: `cargo test --workspace` → `r6_repeated_prompts` 테스트 2건이 깨진다 (같은 `now` ts + 같은 내용 시딩이 이제 **의도적으로** 1행으로 접히므로). 실제 반복은 서로 다른 시각에 오므로 테스트를 현실에 맞게 교정한다.

`rules/r6_repeated_prompts.rs` tests 모듈에 헬퍼 추가:

```rust
    /// 논리 dedup 키(ts+내용)가 같은 ts·내용을 한 행으로 접으므로,
    /// "다른 세션의 실제 반복"은 반드시 서로 다른 ts로 시딩한다.
    fn ts_at(i: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::minutes(i)).to_rfc3339()
    }
```

각 테스트의 시딩을 교체 (`let now = …` 줄 삭제):

```rust
    #[test]
    fn r6_fires_on_three_sessions_with_same_prompt() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "s1", "매일 아침 판매 리포트 뽑아줘", &ts_at(0));
        seed_session(&store, "s2", "매일  아침 판매 리포트 뽑아줘 ", &ts_at(1)); // 공백 차이 → 동치
        seed_session(&store, "s3", "매일 아침 판매 리포트 뽑아줘", &ts_at(2));
        seed_session(&store, "s4", "완전 다른 요청입니다", &ts_at(3));
        // 이하 기존 어서션 유지
```

```rust
    #[test]
    fn r6_fires_on_mid_session_repeats_across_sessions() {
        // 첫 프롬프트가 아니라 세션 중간에 반복되는 지시도 잡는다 (v2 — 스펙 §4.3)
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, sess) in ["s1", "s2", "s3"].iter().enumerate() {
            seed_prompt_at(&store, sess, &format!("서로 다른 작업 요청 {i}번"), &ts_at(i as i64 * 2), 0);
            seed_prompt_at(&store, sess, "PR 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(i as i64 * 2 + 1), 10);
        }
        // 이하 기존 어서션 유지
```

```rust
    #[test]
    fn r6_counts_session_once_despite_in_session_repeats() {
        // 한 세션 안에서 5번 반복 ≠ 5개 세션 — 세션당 1회만 센다 (스펙 §4.3)
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "s1", "이 함수 리팩토링 진행해줘", &ts_at(i as i64), *off);
        }
        seed_prompt_at(&store, "s2", "이 함수 리팩토링 진행해줘", &ts_at(10), 0);
        // 이하 기존 어서션 유지
```

```rust
    #[test]
    fn r6_ignores_short_or_rare_prompts() {
        let store = SqliteStore::open_in_memory().unwrap();
        for i in 0..4 {
            seed_session(&store, &format!("a{i}"), "ㅇㅋ", &ts_at(i)); // 8자 미만 → 제외
        }
        seed_session(&store, "b1", "이건 두 번뿐인 반복 요청", &ts_at(5));
        seed_session(&store, "b2", "이건 두 번뿐인 반복 요청", &ts_at(6));
        // 이하 기존 어서션 유지
```

그리고 포크 침묵 테스트를 추가:

```rust
    #[test]
    fn r6_counts_forked_copies_once() {
        // resume 포크: 같은 ts·내용 프롬프트가 3개 세션 파일에 복제돼도 "3개 세션"이
        // 되면 안 된다 (2026-07-21 데이터 위생 스펙 §1.1-1 — 실사용 junk 카드의 주범)
        let store = SqliteStore::open_in_memory().unwrap();
        let ts = chrono::Utc::now().to_rfc3339();
        for sess in ["orig", "fork1", "fork2"] {
            seed_session(&store, sess, "그 배포 버전 어제 사내망에 올린 것 맞는지 확인해줘", &ts);
        }
        assert!(R6RepeatedPrompts::default().evaluate(&store).unwrap().is_empty(),
            "포크 복제본이 세션 수로 계산되면 안 됨");
    }
```

다른 테스트가 더 깨지면 같은 원칙으로 판단한다: **같은 ts+내용의 다중 시딩이 이제 1행이 되는 것은 설계 의도** — 테스트 의도가 "실제 반복"이면 ts를 분산하고, "복제본"이면 접힘을 어서션한다.

- [ ] **Step 6: 전체 테스트 통과 확인**

Run: `cargo test --workspace`
Expected: 전부 PASS

- [ ] **Step 7: Commit**

```powershell
git add -A; git commit -m @'
fix(agent): dedup events by logical identity to kill fork inflation

Resume forks copy history into new session files with fresh uuids but
preserved timestamps, tool_use_ids and message ids, so uuid:offset keys
stored every copy: session counts tripled (R6/R23 junk cards) and token
rollups double-counted. Multi-line assistant messages also repeat usage
per line (~2.6x overcount). Key events by tool_use_id / message.id and
prompts by ts+content hash; fall back to uuid:offset when absent.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 3: 인터럽트 합성 마커 필터

**Files:**
- Modify: `a-mate/crates/core/src/adapter.rs:102-109` (`is_synthetic_marker`)
- Test: `a-mate/crates/core/src/adapter.rs` (tests 모듈)

**Interfaces:**
- Consumes/Produces: 없음 (내부 필터 — `extract_prompt_preview`가 이미 호출)

- [ ] **Step 1: 실패하는 테스트 작성**

`adapter.rs` tests 모듈, 기존 `map_user_string_content_with_synthetic_marker_yields_no_prompt`(500행 부근) 옆에 추가:

```rust
    #[test]
    fn map_user_interrupt_markers_yield_no_prompt() {
        // Claude Code가 인터럽트 시 합성하는 user 라인 — 지시가 아니다.
        // 실사용: 43개 파일에 존재, R6 "27개 세션" junk 카드의 원인 (스펙 §1.1-3)
        for text in ["[Request interrupted by user]", "[Request interrupted by user for tool use]"] {
            let line = format!(
                r#"{{"type":"user","sessionId":"s1","uuid":"u9","message":{{"role":"user","content":"{text}"}}}}"#
            );
            let evs = adapter().map(&line, "s1.jsonl", 0);
            assert!(
                !evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })),
                "인터럽트 마커가 프롬프트로 수집되면 안 됨: {text}"
            );
        }
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test map_user_interrupt_markers`
Expected: FAIL — 현재 `is_synthetic_marker`는 `<` 접두만 거른다

- [ ] **Step 3: 최소 구현**

`is_synthetic_marker`(102행)에 접두 하나 추가:

```rust
fn is_synthetic_marker(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<command")
        || t.starts_with("<local-command")
        || t.starts_with("<ide_")
        || t.starts_with("<system-reminder")
        || t.starts_with("<task-notification")
        || t.starts_with("[Request interrupted")
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test map_user_interrupt_markers; cargo test --workspace`
Expected: PASS (전체 그린)

- [ ] **Step 5: Commit**

```powershell
git add -A; git commit -m @'
fix(agent): filter interrupt markers from prompt previews

[Request interrupted by user] lines are Claude Code synthetics, not
instructions; they were feeding R6 a 27-session junk card.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 4: tokenizer 구멍 — env 할당 접두·시크릿 강등·GENERIC_BASH 보강

**Files:**
- Modify: `a-mate/crates/core/src/rules/r23_tool_sequences.rs:31-76` (`tokenize`, `GENERIC_BASH`, 신규 `is_env_assignment`)
- Test: `a-mate/crates/core/src/rules/r23_tool_sequences.rs` (tests 모듈)

**Interfaces:**
- Produces: `tokenize` 동작 변화 — env 할당 접두 스킵, `://`/시크릿 패턴 명령은 `bash`로 강등. (`skill_draft`·R23 판정이 공유)

- [ ] **Step 1: 실패하는 테스트 작성**

tests 모듈에 추가:

```rust
    #[test]
    fn tokenize_skips_env_assignment_prefix() {
        // 실사용 junk: bash:test_database_url=postgresql+asyncpg://… (스펙 §1.1-4)
        assert_eq!(
            tokenize("execute", None,
                Some("TEST_DATABASE_URL=postgresql+asyncpg://postgres:postgres@localhost:5432/t pytest -q"),
                Some("Bash")),
            "bash:pytest"
        );
        assert_eq!(tokenize("execute", None, Some("FOO=1 BAR=2 make test"), Some("Bash")), "bash:make");
        // 할당만 있고 명령이 없으면 일반 bash
        assert_eq!(tokenize("execute", None, Some("FOO=bar"), Some("Bash")), "bash");
    }

    #[test]
    fn tokenize_demotes_secretlike_command_token() {
        // 접속 문자열/URL이 명령 자리에 오면 evidence에 노출하지 않는다 — 일반 bash로 강등
        assert_eq!(
            tokenize("execute", None, Some("postgresql://user:pass@h:5432/db"), Some("Bash")),
            "bash"
        );
    }

    #[test]
    fn r23_ignores_env_prefixed_generic_loops() {
        // 실사용 junk 재현: file-ops → bash:TEST_DATABASE_URL=… → bash:cd (스펙 §1.1-4).
        // env 접두를 벗기면 pytest·cd·export 전부 일반 명령 → 특이 토큰 없음 → 침묵
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for sess in ["e1", "e2", "e3"] {
            seed_tool(&store, sess, 0, &now, "Read", Some("conftest.py"));
            seed_tool(&store, sess, 10, &now, "Bash",
                Some("TEST_DATABASE_URL=postgresql://u:p@localhost/t pytest -q"));
            seed_tool(&store, sess, 20, &now, "Bash", Some("export PATH=/x:$PATH"));
        }
        assert!(R23ToolSequences::default().evaluate(&store).unwrap().is_empty(),
            "env 접두를 벗긴 일반 루프는 침묵해야 함");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test tokenize_skips_env; cargo test tokenize_demotes; cargo test r23_ignores_env_prefixed`
Expected: FAIL — 현재는 첫 단어(할당 포함)를 그대로 토큰화

주의: Task 2의 논리 키 도입으로 `seed_tool`의 tool_use_id가 세션별로 고유(`t-{sess}-{off}`)인지 확인 — 이미 고유하므로 r23 기존 테스트는 영향 없다.

- [ ] **Step 3: 구현**

`GENERIC_BASH`(60-65행)에 3개 추가 — 마지막 원소 `"test"` 뒤:

```rust
    "curl", "wget", "powershell", "pwsh", "cmd", "dir", "type", "sh", "bash", "test",
    "export", "set", "env",
];
```

`tokenize`의 `"execute"` 분기(40-46행)를 교체하고 헬퍼 추가:

```rust
        // 시크릿 리댁션된 명령(<redacted: …>)은 내용을 알 수 없으니 일반 bash로 취급 —
        // 특이 토큰으로 오인해 무관한 시크릿 명령들이 한 패턴으로 뭉치는 것 방지 (Codex P2).
        // env 할당 접두(NAME=value cmd)는 명령이 아니다 — 실제 명령 단어를 찾는다.
        // 접속 문자열(://)·시크릿 패턴이 명령 자리에 오면 evidence 노출 차단을 위해
        // 일반 bash로 강등한다 (데이터 위생 스펙 §3.3).
        "execute" => tool_target
            .filter(|t| !t.starts_with('<'))
            .and_then(|t| t.split_whitespace().find(|w| !is_env_assignment(w)))
            .filter(|c| !c.contains("://") && crate::curation::find_secret_patterns(c).is_empty())
            .map(|c| format!("bash:{}", c.to_lowercase()))
            .unwrap_or_else(|| "bash".into()),
```

```rust
/// `NAME=value` 형태의 env 할당 접두인가 — 명령 앞의 환경변수 지정은 명령이 아니다.
fn is_env_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((name, _)) => !name.is_empty()
            && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        None => false,
    }
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --workspace`
Expected: 전부 PASS (기존 `tokenize_maps_kinds_per_spec` 등 포함)

- [ ] **Step 5: Commit**

```powershell
git add -A; git commit -m @'
fix(agent): skip env assignment prefixes in bash sequence tokens

TEST_DATABASE_URL=postgres://user:pass@host pytest tokenized as the
assignment word, minting a junk R23 card that leaked the connection
string into evidence. Take the first non-assignment word, demote
secret-like command tokens to plain bash, and treat export/set/env
as generic.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 5: 마이그레이션 `user_version=6` — 전체 재수집 + junk finding 정화

**Files:**
- Modify: `a-mate/crates/core/src/store.rs:193-201` (`migrate`에 v6 분기 추가)
- Test: `a-mate/crates/core/src/store.rs` (tests 모듈 — 기존 `migrate_v5…` 테스트 옆)

**Interfaces:**
- Consumes: Task 2의 논리 dedup 키 (재수집이 새 키로 적재되게 함)
- Produces: `PRAGMA user_version = 6`

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
    #[test]
    fn migrate_v6_recollects_and_purges_repeat_junk() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv6.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 5;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);
                 INSERT INTO prompt_events (dedup_key, session_id, host, project_id,
                   source_file, source_offset, norm60, preview)
                 VALUES ('old-key', 's1', 'Windows', 'p', 'f.jsonl', 0, 'x', 'x');",
            ).unwrap();
            let store = SqliteStore { conn };
            let f = |rule: &str, key: &str| Finding {
                rule_id: rule.into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: format!("pattern:{key}"),
                evidence: serde_json::json!({}), est_tokens_saved: 0,
                prescription: None, dedup_key: key.into(),
            };
            store.upsert_finding(&f("R6", "R6|Windows|junk"), "2026-07-21T00:00:00Z").unwrap();
            store.upsert_finding(&f("R23", "R23|Windows|junk"), "2026-07-21T00:00:00Z").unwrap();
            store.upsert_finding(&f("R6", "R6|Windows|muted"), "2026-07-21T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|muted", "dismissed").unwrap();
            store.upsert_finding(&f("R1", "R1|Windows|keep"), "2026-07-21T00:00:00Z").unwrap();
        }
        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v6 분기 발화
        for table in ["ingest_state", "prompt_events", "events", "daily_rollup", "sessions"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table}는 재수집을 위해 비워져야 함");
        }
        let keys: Vec<String> = {
            let mut stmt = store.conn
                .prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap()
                .collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(keys, vec!["R1|Windows|keep".to_string(), "R6|Windows|muted".to_string()],
            "R6/R23 junk('new')만 삭제 — dismissed·타 룰은 보존");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(uv, 6);
        // 멱등: 다시 열어도 변화 없음
        drop(store);
        let store = SqliteStore::open(&db).unwrap();
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(uv, 6);
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test migrate_v6_recollects`
Expected: FAIL — v6 분기가 없어 `user_version=5`에 머무름

- [ ] **Step 3: 구현**

`migrate`의 v5 분기 뒤(store.rs 201행 `Ok(())` 앞)에 추가:

```rust
    // v3.6 재수집 — 논리 dedup 키 도입(데이터 위생 스펙 §3.1): resume 포크 복제본·
    // 다중 라인 usage 반복을 기존 uuid:offset 행에서 소급 제거할 수 없어 전체 재수집한다.
    // 오염된 데이터로 만들어진 R6/R23 활성('new') 카드도 정화 — dismissed/resolved는
    // 사용자 기록(나깅 방지 쿨다운)이라 보존 (v5 전례).
    if user_version < 6 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM prompt_events;
             DELETE FROM findings WHERE rule_id IN ('R6','R23') AND status='new';
             PRAGMA user_version = 6;",
        )?;
    }
```

- [ ] **Step 4: 기존 v5 마이그레이션 테스트 교정**

Run: `cargo test migrate` → `mv5.db`를 쓰는 기존 테스트(store.rs 2478행 부근)가 깨진다.
open()은 밀린 분기를 전부 실행하므로 v5 시대 DB는 이제 v6까지 통과한다:
- `uv == 5` → `6`이 됨
- `ingest_state == 1`("v5 정리는 재수집 유도 안 함") → v6이 비우므로 `0`
- R6 `new`("R6|Windows|ok") → v6이 삭제

해당 테스트의 마지막 어서션 블록을 다음으로 교체 (dismissed 보존 검증은 유지하되,
v5 단독 관찰이 불가능해진 어서션은 v6 통과 후 상태로 갱신):

```rust
        let store = SqliteStore::open(&db).unwrap();
        let keys: Vec<String> = {
            let mut stmt = store.conn.prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap()
        };
        // v5(R23 new 삭제) 후 v6(R6/R23 new 삭제·재수집)까지 연쇄 실행된 결과 —
        // dismissed는 어느 분기에서도 삭제되지 않는다는 것이 이 테스트의 핵심.
        assert_eq!(keys, vec!["R23|Windows|muted".to_string()],
            "dismissed는 v5·v6 연쇄에도 보존, new는 정화");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 6, "밀린 분기 전부 통과 (mv2 테스트 전례)");
```

`migrate_v3…`/`migrate_v4…` 테스트가 추가로 깨지면 같은 기준으로 교정: 정확 일치
어서션(`uv == N`, 특정 테이블 행 수)을 v6 통과 후 상태 또는 `>=` 형태로 갱신하되,
각 테스트의 **핵심 의도**(finding 정리 대상·보존 대상)는 어서션으로 유지한다.

- [ ] **Step 5: 통과 확인**

Run: `cargo test migrate_v6_recollects; cargo test --workspace`
Expected: PASS (기존 migrate 테스트 포함 전체 그린 — `migrate_v2…` 테스트는 `uv >= 2` 어서션이라 v6 도입에도 견딤)

스펙 문서 갱신: `docs/design/a-mate/specs/2026-07-20-r6-v2-session-repeat-mining.md` §5 마이그레이션 목록에 한 줄 추가:

```markdown
- `→ 6`: 논리 dedup 키 도입(데이터 위생 스펙 §3) — 전체 재수집 + R6/R23 'new' 정화.
```

- [ ] **Step 6: Commit**

```powershell
git add -A; git commit -m @'
chore(agent): migrate v6 to recollect with logical dedup keys

Existing rows carry uuid:offset keys, so fork copies and repeated
usage cannot be deduped in place; wipe derived tables for a full
recollect and purge R6/R23 findings minted from polluted counts
(dismissed/resolved user records survive).

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 6: 전체 검증·문서 정리·PR

**Files:**
- 없음 (검증·문서·PR만)

- [ ] **Step 1: 전체 테스트**

Run (a-mate/에서): `cargo test --workspace`
Expected: 전부 PASS, 실패 0

Run: `npm test`
Expected: 전부 PASS (프론트엔드는 이번 PR에서 미변경 — 회귀 확인용)

- [ ] **Step 2: 실기기 스모크 (선택이지만 권장)**

`npm run tauri dev`로 앱을 띄워 재스캔 1회 수행 후 확인:
- 코치 탭에서 `[Request interrupted…]`·`bash:…postgresql…` 카드가 사라졌는지
- 대시보드 토큰 수치가 감소했는지 (**정상화 — 스펙 §3.1 고지 사항**)

- [ ] **Step 3: 계획 문서 아카이브 (DoD)**

`docs-archive` 스킬을 실행해 이 계획 문서(`docs/design/a-mate/plans/2026-07-21-coaching-data-hygiene-pr1.md`)를 `docs/archive/` 미러로 이동한다 (ADR 0013). 스펙 문서는 PR2가 남았으므로 **활성 유지**.

- [ ] **Step 4: 푸시 + PR**

```powershell
git push -u origin fix/coaching-data-hygiene
gh pr create --title "fix(agent): coaching data hygiene - logical dedup keys" --body @'
## Summary
- Dedup events by logical identity (tool_use_id / message.id / ts+content hash) so resume-fork copies and multi-line usage repetition never enter the DB (session-count inflation and ~2.6x token overcount fixed)
- Filter `[Request interrupted…]` synthetic markers from prompt previews
- Skip env-assignment prefixes in bash sequence tokens; demote secret-like tokens (connection-string leak fixed)
- Migration `user_version=6`: full recollect + purge polluted R6/R23 `new` findings (dismissed preserved)

Spec: docs/design/a-mate/specs/2026-07-21-repeat-coaching-judgment-redesign.md (PR1 of 2 - PR2 adds the R6 judgment layer and retires R23)

## Test plan
- [x] cargo test --workspace
- [x] npm test
- [x] Manual rescan: junk cards gone, token stats normalized

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'@
```

---

## Self-Review 결과 (작성 시 수행)

- **스펙 §3 커버리지**: §3.1 논리 키(Task 1·2), §3.2 마커(Task 3), §3.3 tokenizer(Task 4), §3.4 마이그레이션(Task 5) — 전부 매핑됨.
- **타입 일관성**: `msg_id: Option<String>`(Task 1) ↔ `e.msg_id`(Task 2), `hash8`(Task 2 정의·사용), `is_env_assignment`(Task 4 정의·사용) 일치 확인.
- **기존 테스트 영향**: r6 4건은 ts 분산으로 교정(Task 2 Step 5에 전체 코드), `mv5.db` 마이그레이션 테스트는 v6 연쇄 실행 반영으로 교정(Task 5 Step 4에 전체 코드), r23 시드는 tool_use_id가 세션별 고유라 무영향, `prompt_events_accumulate…`는 preview가 달라 무영향, `migrate_v2…`는 `uv >= 2` 어서션이라 무영향.
