---
status: done
archived: 2026-07-22
---

# F — 컨텍스트 위생 (에피소드 세그먼터 + R24) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** clear/compact 없이 대용량 컨텍스트를 이고 가는 습관을 결정론으로 감지해 (host,project) 코칭 카드로 노출한다. 이를 위해 C·A·F 공유 척추인 **에피소드 세그먼터**를 처음 구현하고, 그 위에 **R24 컨텍스트 위생 룰**을 얹는다.

**Architecture:** 세그먼터(`episode.rs`)는 `prompt_events`(이미 sidechain·meta·주입 제외된 8자↑ 실질 발화)를 에피소드 경계로 삼고, 각 에피소드 안의 첫 main-chain `AssistantTurn`이 물려받은 컨텍스트(`tok_input + tok_cache_read`)를 `events`에서 붙인다. R24는 순수 결정론 룰(LLM 판정 패스 미사용, R8형)로 세션별 carry-ratio를 계산해 위생 나쁜 세션을 추리고, (host,project)로 롤업해 가장 심한 세션을 근거로 인용하는 카드 1장을 낸다.

**Tech Stack:** Rust (crates/core, package `agent-mentor` / lib `agent_mentor`), rusqlite (in-memory 테스트), Tauri v2 셸(무변경), Svelte 5 + Vitest(제목 1줄).

## Global Constraints

- **플랫폼 Windows 전용.** 빌드·테스트는 네이티브 Windows PowerShell/cmd에서 — **WSL 금지**.
- **백엔드 테스트**: `a-mate/`에서 `cargo test -p agent-mentor`. **프론트 테스트**: `a-mate/`에서 `npm test` (Vitest).
- **결정론 룰**: R24는 엔진/LLM 미사용. 순회 순서는 `BTreeMap`으로 고정(결정론).
- **불변 계약 (스펙 §2 상속)**: 근거 = 사용자 직접 프롬프트 + main-chain 사실만(`is_sidechain=0`). 에이전트/서브에이전트 산출물 채점 금지. `est_tokens_saved=0`(근거 없는 수치 금지). 저빈도·dedup(같은 `dedup_key` 재노출 억제).
- **Rule ID = `R24`** (확정). dedup_key = `R24|{host}|{project}`. v3 예약 대역(R13~R22)·폐기 R23과 무충돌.
- **임계값은 잠정** — Windows 실데이터로 캘리브레이션(스펙 §8). 코드에 `⚠ CALIBRATE` 주석 필수.
- **커밋**: Conventional Commits, 영어. scope는 `agent`.
- **YAGNI**: 세그먼터의 `glue_followups`(접착 프롬프트 원문)는 F가 안 쓰고 ingest가 8자 미만 원문을 버리므로 v1에서 구현하지 않는다 — C PR에서 ingest 확장과 함께 추가.

## 참조 사실 (조사 완료)

- `prompt_events` 테이블: `(session_id, host, project_id, ts, source_offset, norm60, preview)`. ingest가 **main-chain(`is_sidechain=0`) + `normalize()` 성공(8자↑ 실질)** 프롬프트만 삽입(store.rs:317-336). → 에피소드 경계의 정답 소스. 접착(<8자) 프롬프트는 여기 없어 경계가 되지 않음.
- `events` 테이블: `UserPrompt`·`SessionMeta`는 **미삽입**(sessions/prompt_events로 라우팅, store.rs:298·338). `AssistantTurn`(kind=`'assistant_turn'`)·`Compaction`(kind=`'compaction'`)은 삽입됨. 컬럼: `kind, ts, source_offset, is_sidechain, tok_input, tok_cache_read, ...`. `tok_input = usage.input`, `tok_cache_read = usage.cache_read`(flatten, store.rs:1699).
- `Rule` 트레이트: `fn id(&self) -> &'static str; fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>>;` (rules/mod.rs:18).
- `Finding` 필드: `rule_id, severity(Severity::{Info,Suggest,Warn}), scope_host: Option<String>, scope_project: Option<String>, scope_kind, scope_ref, evidence: serde_json::Value, est_tokens_saved: u64, prescription: Option<Prescription{kind,payload}>, dedup_key` (finding.rs:23).
- `store.upsert_finding` init_status(store.rs:464): `("R6",_)`·`("R7","session")` → `pending`, 그 외 → `new`. **R24 project 카드는 자동 `new`(즉시 노출)** — 코드 변경 불필요.
- `coach_findings_inner`(commands.rs:81): rule_id allowlist 없음 — `list_findings_current` + `finding_advice` + `fix_command`로 모든 finding 노출. → R24는 advice arm만 있으면 표면화됨.
- 프론트 `coach-helpers.ts::COACH_TITLE`: 활성 룰별 제목 맵(`?? '아낄 수 있는 게 보여요'` 폴백). R24 제목 추가 필요(선택적이나 UX 일관성). `bubble.ts::RULE_LINE`은 일부 룰만 등록(R8/R10도 없음) → R24 추가 불필요.

---

## Task 1: 에피소드 세그먼터 (`episode.rs`)

**Files:**
- Create: `a-mate/crates/core/src/episode.rs`
- Modify: `a-mate/crates/core/src/lib.rs` (모듈 선언 추가)

**Interfaces:**
- Consumes: `SqliteStore.conn`(prepare 직접 쿼리, session_stats.rs 선례), `prompt_events`·`events` 테이블.
- Produces:
  - `pub struct Episode { pub session_id: String, pub host: String, pub project_id: String, pub lead_preview: String, pub lead_offset: u64, pub first_ts: Option<String>, pub inherited_ctx: u64, pub had_compaction: bool }`
  - `pub fn segment_all(store: &SqliteStore) -> anyhow::Result<Vec<Episode>>` — 모든 세션의 에피소드를 세션 그룹 내 시간순으로 반환.

- [ ] **Step 1: 모듈 선언 + 스켈레톤 작성**

`a-mate/crates/core/src/lib.rs` 20행 근처(`pub mod diary;` 뒤 알파벳/논리 순서 무관, `pub mod ops;` 위)에 추가:

```rust
pub mod episode;
```

`a-mate/crates/core/src/episode.rs` 생성 (본문 먼저, 테스트는 다음 스텝):

```rust
//! 에피소드 세그먼터 (결정론) — 코칭 v3 재설계 §3①.
//! 한 세션의 이벤트를 [실질 UserPrompt → 다음 실질 프롬프트 전까지의 작업] = 에피소드로 자른다.
//! clear/compact 습관에 불변인 작업 단위. C·A·F 공유 척추이며 F가 첫 소비자.
//!
//! 경계(실질 프롬프트)는 `prompt_events`에서 온다 — ingest가 이미 sidechain·meta·도구결과·
//! 주입을 제외하고 8자↑ 실질 발화만 담는다(store.rs). 따라서 접착(<8자) 프롬프트는 애초에
//! 경계가 되지 않아 별도 병합 로직이 필요 없다. 에피소드 내 작업 사실(물려받은 컨텍스트·
//! compaction)은 `events`의 main-chain AssistantTurn·Compaction에서 온다.
//!
//! 범위 경계(YAGNI): v1(F)은 접착 프롬프트 원문 리스트(`glue_followups`)를 싣지 않는다 —
//! F는 안 쓰고 ingest가 <8자 원문을 버리기 때문. C PR에서 ingest 확장과 함께 추가.

use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

/// 한 실질 프롬프트가 여는 작업 단위.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    /// 에피소드를 연 실질 프롬프트 미리보기 (근거 인용용).
    pub lead_preview: String,
    /// 실질 프롬프트의 source_offset (deref 포인터).
    pub lead_offset: u64,
    /// 에피소드 시작 ts (= lead 프롬프트 ts). 없으면 None.
    pub first_ts: Option<String>,
    /// 이 에피소드의 첫 main-chain AssistantTurn이 물려받은 컨텍스트
    /// = tok_input + tok_cache_read. 그런 턴이 없으면 0.
    pub inherited_ctx: u64,
    /// 에피소드 범위 안에 (main-chain) Compaction 이벤트가 있었는지 (보조 신호).
    pub had_compaction: bool,
}

// 내부 정렬 키: (ts 문자열, source_offset). ts 우선, offset 타이브레이크.
// ts 없는 행은 빈 문자열로 맨 앞 정렬 (스펙 §3① "ts 없는 프롬프트는 offset 정렬").
type OrderKey = (String, u64);

struct Lead {
    host: String,
    project_id: String,
    ts: Option<String>,
    offset: u64,
    preview: String,
}

enum Ev {
    Turn { inherited: u64 },
    Compaction,
}

struct EvRow {
    key: OrderKey,
    ev: Ev,
}

pub fn segment_all(store: &SqliteStore) -> Result<Vec<Episode>> {
    // 1) 경계: prompt_events (실질·main-chain 프롬프트) — 세션별로 모은다.
    let mut leads: BTreeMap<String, Vec<Lead>> = BTreeMap::new();
    {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id, ts, source_offset, preview FROM prompt_events",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Lead {
                    host: r.get(1)?,
                    project_id: r.get(2)?,
                    ts: r.get(3)?,
                    offset: r.get::<_, i64>(4)? as u64,
                    preview: r.get(5)?,
                },
            ))
        })?;
        for row in rows {
            let (sid, lead) = row?;
            leads.entry(sid).or_default().push(lead);
        }
    }

    // 2) 작업 사실: events의 main-chain assistant_turn·compaction — 세션별로 모은다.
    let mut evs: BTreeMap<String, Vec<EvRow>> = BTreeMap::new();
    {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, kind, ts, source_offset, is_sidechain, tok_input, tok_cache_read
             FROM events WHERE kind IN ('assistant_turn','compaction')",
        )?;
        let rows = stmt.query_map([], |r| {
            let sid: String = r.get(0)?;
            let kind: String = r.get(1)?;
            let ts: Option<String> = r.get(2)?;
            let offset = r.get::<_, i64>(3)? as u64;
            let is_side = r.get::<_, i64>(4)? != 0;
            let ti = r.get::<_, i64>(5)? as u64;
            let tcr = r.get::<_, i64>(6)? as u64;
            // main-chain만: sidechain(서브에이전트) 활동은 사용자 컨텍스트가 아니다.
            let ev = match (kind.as_str(), is_side) {
                ("compaction", false) => Some(Ev::Compaction),
                ("assistant_turn", false) => Some(Ev::Turn { inherited: ti + tcr }),
                _ => None,
            };
            Ok((sid, ev.map(|ev| EvRow { key: (ts.unwrap_or_default(), offset), ev })))
        })?;
        for row in rows {
            let (sid, maybe) = row?;
            if let Some(evrow) = maybe {
                evs.entry(sid).or_default().push(evrow);
            }
        }
    }

    // 3) 세션별 병합.
    let mut out = Vec::new();
    for (session_id, mut sess_leads) in leads {
        if sess_leads.is_empty() {
            continue;
        }
        sess_leads.sort_by(|a, b| {
            (a.ts.clone().unwrap_or_default(), a.offset)
                .cmp(&(b.ts.clone().unwrap_or_default(), b.offset))
        });
        let lead_keys: Vec<OrderKey> = sess_leads
            .iter()
            .map(|l| (l.ts.clone().unwrap_or_default(), l.offset))
            .collect();
        let mut episodes: Vec<Episode> = sess_leads
            .iter()
            .map(|l| Episode {
                session_id: session_id.clone(),
                host: l.host.clone(),
                project_id: l.project_id.clone(),
                lead_preview: l.preview.clone(),
                lead_offset: l.offset,
                first_ts: l.ts.clone(),
                inherited_ctx: 0,
                had_compaction: false,
            })
            .collect();
        let mut turn_set = vec![false; episodes.len()];

        if let Some(mut sess_evs) = evs.remove(&session_id) {
            sess_evs.sort_by(|a, b| a.key.cmp(&b.key));
            for evrow in sess_evs {
                // ev가 속한 에피소드 = key <= ev.key 인 마지막 lead.
                let idx = match lead_keys.iter().rposition(|k| *k <= evrow.key) {
                    Some(i) => i,
                    None => continue, // 첫 실질 프롬프트 이전 이벤트 → 무시.
                };
                match evrow.ev {
                    Ev::Turn { inherited } => {
                        if !turn_set[idx] {
                            episodes[idx].inherited_ctx = inherited;
                            turn_set[idx] = true;
                        }
                    }
                    Ev::Compaction => episodes[idx].had_compaction = true,
                }
            }
        }
        out.extend(episodes);
    }
    Ok(out)
}
```

- [ ] **Step 2: 컴파일 확인 (테스트 전 골격)**

Run: `cargo build -p agent-mentor`
Expected: 성공 (경고 없이 컴파일). 실패 시 오타·경로 수정.

- [ ] **Step 3: 실패하는 테스트 작성 (경계·상속·접착·빈세션)**

`episode.rs` 하단에 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    // 관찰 무관 — 세그먼터는 창 필터를 하지 않는다. 고정 ts로 순서만 검증.
    fn prompt(session: &str, offset: u64, preview: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-p{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::UserPrompt { preview: preview.into() },
        }
    }

    fn turn(session: &str, offset: u64, input: u64, cache_read: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-a{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input, cache_read, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    #[test]
    fn segments_by_substantive_prompt_and_attaches_first_turn_context() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 에피소드 A: 실질 프롬프트 → 첫 턴(input 40k + cache 20k = 60k) → 둘째 턴(무시)
        // 에피소드 B: 실질 프롬프트 → 첫 턴(input 5k)
        store.upsert_events(&[
            prompt("s1", 0, "첫 작업을 시작해줘 자세히", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 40_000, 20_000, "2026-07-01T10:00:01Z"),
            turn("s1", 2, 99_000, 0, "2026-07-01T10:00:02Z"),
            prompt("s1", 3, "다른 작업으로 넘어가자 상세히", "2026-07-01T11:00:00Z"),
            turn("s1", 4, 5_000, 0, "2026-07-01T11:00:01Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 2, "실질 프롬프트 2개 → 에피소드 2개");
        assert_eq!(eps[0].lead_offset, 0);
        assert_eq!(eps[0].inherited_ctx, 60_000, "첫 main-chain 턴의 input+cache_read");
        assert_eq!(eps[0].had_compaction, false);
        assert_eq!(eps[1].lead_offset, 3);
        assert_eq!(eps[1].inherited_ctx, 5_000);
        assert_eq!(eps[0].lead_preview, "첫 작업을 시작해줘 자세히");
    }

    #[test]
    fn glue_prompt_does_not_open_new_episode() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            prompt("s1", 0, "긴 실질 작업 지시입니다", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 50_000, 0, "2026-07-01T10:00:01Z"),
            // 접착 프롬프트(<8자) — normalize None → prompt_events 미삽입 → 경계 아님
            prompt("s1", 2, "계속", "2026-07-01T10:05:00Z"),
            turn("s1", 3, 55_000, 0, "2026-07-01T10:05:01Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 1, "접착 프롬프트는 새 에피소드를 열지 않는다");
        assert_eq!(eps[0].inherited_ctx, 50_000, "첫 턴만 상속으로 잡힌다");
    }

    #[test]
    fn sidechain_turn_is_not_counted_as_inherited_context() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut side = turn("s1", 1, 90_000, 0, "2026-07-01T10:00:01Z");
        side.is_sidechain = true;
        store.upsert_events(&[
            prompt("s1", 0, "실질 작업 지시 문장입니다", "2026-07-01T10:00:00Z"),
            side, // sidechain 턴 먼저 와도 무시
            turn("s1", 2, 3_000, 0, "2026-07-01T10:00:02Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].inherited_ctx, 3_000, "sidechain 턴 제외, 첫 main-chain 턴만");
    }

    #[test]
    fn session_without_substantive_prompt_is_skipped() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", 0, 70_000, 0, "2026-07-01T10:00:00Z"), // 프롬프트 없이 턴만
        ]).unwrap();
        assert!(segment_all(&store).unwrap().is_empty());
    }
}
```

- [ ] **Step 4: 테스트 실패 확인**

Run: `cargo test -p agent-mentor episode::tests`
Expected: 4개 테스트 실행. (Step 1 구현이 이미 통과하도록 작성됐다면) 통과. 만약 실패하면 로직 수정 — 흔한 원인: `lead_keys.rposition` 비교, ts 정렬. 실패 메시지로 디버깅.

> 참고: 구현(Step 1)이 테스트보다 먼저 존재하므로 이 태스크는 "구현→테스트 확정" 순서다. 테스트가 처음부터 통과하면 그대로 진행하되, 각 assert가 **의미 있게** 통과하는지(예: `inherited_ctx=60_000`가 우연이 아닌지) 값을 바꿔 한 번 실패시켜 확인할 것.

- [ ] **Step 5: had_compaction 실패 테스트 작성**

`episode.rs` tests에 헬퍼 + 테스트 추가:

```rust
    fn compaction(session: &str, offset: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-c{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::Compaction,
        }
    }

    #[test]
    fn compaction_within_episode_sets_flag() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            prompt("s1", 0, "첫 에피소드 실질 지시야", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 10_000, 0, "2026-07-01T10:00:01Z"),
            prompt("s1", 2, "둘째 에피소드 실질 지시야", "2026-07-01T11:00:00Z"),
            compaction("s1", 3, "2026-07-01T11:00:30Z"),
            turn("s1", 4, 20_000, 0, "2026-07-01T11:00:40Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].had_compaction, false);
        assert_eq!(eps[1].had_compaction, true, "compaction이 든 에피소드만 true");
    }
```

- [ ] **Step 6: had_compaction 테스트 확인**

Run: `cargo test -p agent-mentor episode::tests::compaction_within_episode_sets_flag`
Expected: PASS. (`Compaction`이 events에 kind=`'compaction'`으로 삽입됨 — store.rs:1722 확인 완료.)

- [ ] **Step 7: 전체 세그먼터 테스트 + 커밋**

Run: `cargo test -p agent-mentor episode`
Expected: 5개 PASS.

```bash
git add a-mate/crates/core/src/episode.rs a-mate/crates/core/src/lib.rs
git commit -m "feat(agent): add deterministic episode segmenter (shared C/A/F spine)"
```

---

## Task 2: R24 컨텍스트 위생 룰 (`r24_context_hygiene.rs`)

**Files:**
- Create: `a-mate/crates/core/src/rules/r24_context_hygiene.rs`
- Modify: `a-mate/crates/core/src/rules/mod.rs` (모듈 선언)

**Interfaces:**
- Consumes: `crate::episode::{segment_all, Episode}`(Task 1), `Rule` 트레이트, `Finding`/`Prescription`/`Severity`.
- Produces: `pub struct R24ContextHygiene { pub min_episodes: usize, pub inherited_ctx_threshold: u64, pub carry_ratio: f64, pub min_bad_sessions: usize, pub days: i64 }` + `impl Default` + `impl Rule`(id=`"R24"`). evaluate가 (host,project) project-scope Finding들을 반환.

- [ ] **Step 1: 모듈 선언**

`a-mate/crates/core/src/rules/mod.rs` 11행(`pub mod r12_unused_skills;`) 뒤에 추가:

```rust
pub mod r24_context_hygiene;
```

- [ ] **Step 2: 룰 구현 작성**

`a-mate/crates/core/src/rules/r24_context_hygiene.rs` 생성:

```rust
//! R24 — 컨텍스트 위생 (신규 결정론 룰, 코칭 v3 재설계 §4 F).
//! clear/compact 없이 대용량 컨텍스트를 상주시켜, 작업을 바꿔도 이전 컨텍스트를 이고 가는 습관.
//! 절대 토큰 임계 대신 "작업 경계에서 컨텍스트가 리셋되는가"를 본다 — 각 에피소드의 첫
//! main-chain 턴이 물려받은 컨텍스트(tok_input+tok_cache_read)로 판별. LLM 판정 미사용(R8형).

use crate::episode::{segment_all, Episode};
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct R24ContextHygiene {
    /// 습관 성립에 필요한 세션당 최소 에피소드(작업) 수.
    pub min_episodes: usize,
    /// "큰 컨텍스트를 물려받음"으로 볼 토큰 임계.
    pub inherited_ctx_threshold: u64,
    /// 큰 컨텍스트를 물려받은 에피소드 비율 문턱 (0.0~1.0).
    pub carry_ratio: f64,
    /// (host,project) 카드 발화에 필요한 최소 "위생 나쁜 세션" 수.
    pub min_bad_sessions: usize,
    /// 관찰 기간(일).
    pub days: i64,
}

impl Default for R24ContextHygiene {
    // ⚠ CALIBRATE — 잠정값. Windows 실데이터로 캘리브레이션 (스펙 §8). R7·R8 관행.
    fn default() -> Self {
        R24ContextHygiene {
            min_episodes: 5,
            inherited_ctx_threshold: 50_000,
            carry_ratio: 0.60,
            min_bad_sessions: 1,
            days: 14,
        }
    }
}

struct BadSession {
    session_id: String,
    host: String,
    project_id: String,
    episodes: usize,
    carry_count: usize,
    carry_ratio_pct: u64,
    max_inherited: u64,
    sample_prompts: Vec<String>,
    first_ts: Option<String>,
}

impl Rule for R24ContextHygiene {
    fn id(&self) -> &'static str {
        "R24"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let episodes = segment_all(store)?;

        // 세션별 그룹화 (결정론 위해 BTreeMap).
        let mut by_session: BTreeMap<String, Vec<Episode>> = BTreeMap::new();
        for ep in episodes {
            by_session.entry(ep.session_id.clone()).or_default().push(ep);
        }

        // 세션별 carry-ratio → 위생 나쁜 세션 추림.
        let mut bad: Vec<BadSession> = Vec::new();
        for (session_id, eps) in by_session {
            // 관찰창: 세션 최초 ts ≥ cutoff. ts 없는 세션 제외 (R7 관행).
            let first_ts = eps.iter().filter_map(|e| e.first_ts.clone()).min();
            match first_ts.as_deref() {
                Some(ts) if ts >= cutoff.as_str() => {}
                _ => continue,
            }
            let n = eps.len();
            if n < self.min_episodes {
                continue;
            }
            let carry_count =
                eps.iter().filter(|e| e.inherited_ctx >= self.inherited_ctx_threshold).count();
            let ratio = carry_count as f64 / n as f64;
            if ratio < self.carry_ratio {
                continue;
            }
            let max_inherited = eps.iter().map(|e| e.inherited_ctx).max().unwrap_or(0);
            let host = eps[0].host.clone();
            let project_id = eps[0].project_id.clone();
            // 근거 인용: inherited_ctx 큰 순 상위 3개 에피소드의 lead preview.
            let mut sorted = eps.clone();
            sorted.sort_by(|a, b| b.inherited_ctx.cmp(&a.inherited_ctx));
            let sample_prompts =
                sorted.iter().take(3).map(|e| e.lead_preview.clone()).collect::<Vec<_>>();
            bad.push(BadSession {
                session_id,
                host,
                project_id,
                episodes: n,
                carry_count,
                carry_ratio_pct: (ratio * 100.0).round() as u64,
                max_inherited,
                sample_prompts,
                first_ts,
            });
        }

        // (host, project) 롤업.
        let mut by_proj: BTreeMap<(String, String), Vec<BadSession>> = BTreeMap::new();
        for b in bad {
            by_proj.entry((b.host.clone(), b.project_id.clone())).or_default().push(b);
        }

        let mut out = Vec::new();
        for ((host, project_id), mut sessions) in by_proj {
            if sessions.len() < self.min_bad_sessions {
                continue;
            }
            // 가장 심한 세션 = carry_ratio_pct desc, max_inherited desc.
            sessions.sort_by(|a, b| {
                b.carry_ratio_pct
                    .cmp(&a.carry_ratio_pct)
                    .then(b.max_inherited.cmp(&a.max_inherited))
            });
            let worst = &sessions[0];
            out.push(Finding {
                rule_id: "R24".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: Some(project_id.clone()),
                scope_kind: "project".into(),
                scope_ref: project_id.clone(),
                evidence: serde_json::json!({
                    "host": host,
                    "project": project_id,
                    "bad_session_count": sessions.len(),
                    "worst_session_id": worst.session_id,
                    "worst_carry_ratio_pct": worst.carry_ratio_pct,
                    "worst_episodes": worst.episodes,
                    "worst_carry_count": worst.carry_count,
                    "worst_max_inherited_tokens": worst.max_inherited,
                    "inherited_ctx_threshold": self.inherited_ctx_threshold,
                    "carry_ratio_threshold_pct": (self.carry_ratio * 100.0).round() as u64,
                    "min_episodes": self.min_episodes,
                    "window_days": self.days,
                    "sample_prompts": worst.sample_prompts,
                    "first_ts": worst.first_ts,
                }),
                est_tokens_saved: 0,
                prescription: Some(Prescription {
                    kind: "context_hygiene".into(),
                    payload: serde_json::json!({}),
                }),
                dedup_key: format!("R24|{host}|{project_id}"),
            });
        }
        Ok(out)
    }
}
```

- [ ] **Step 3: 실패하는 테스트 작성**

`r24_context_hygiene.rs` 하단에 추가. 헬퍼는 관찰창 안쪽 상대 ts를 쓴다(하드코딩 날짜가 창 밖으로 밀려 테스트가 썩는 것 방지 — r7 선례).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn recent(h: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339()
    }

    fn prompt(session: &str, offset: u64, preview: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-p{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::UserPrompt { preview: preview.into() },
        }
    }

    fn turn(session: &str, offset: u64, input: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-a{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    /// 한 세션에 에피소드 `n`개를 만든다. 각 에피소드 첫 턴 input = `inherited`.
    /// offset은 세션 내 단조 증가. ts는 recent(base_h - i) 로 관찰창 안쪽.
    fn session_with_episodes(sid: &str, n: usize, inherited: u64, base_h: i64) -> Vec<NormalizedEvent> {
        let mut evs = Vec::new();
        for i in 0..n {
            let off = (i as u64) * 2;
            let ts = recent(base_h - i as i64); // 시간 진행
            evs.push(prompt(sid, off, &format!("에피소드 {i} 실질 작업 지시 문장"), &ts));
            evs.push(turn(sid, off + 1, inherited, &ts));
        }
        evs
    }

    #[test]
    fn fires_when_carry_ratio_high() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 6개 에피소드 전부 60k 상속 → carry_ratio 100% ≥ 60%, n=6 ≥ 5.
        store.upsert_events(&session_with_episodes("s1", 6, 60_000, 20)).unwrap();

        let f = R24ContextHygiene::default().evaluate(&store).unwrap();
        assert_eq!(f.len(), 1, "위생 나쁜 프로젝트 카드 1장");
        let card = &f[0];
        assert_eq!(card.rule_id, "R24");
        assert_eq!(card.scope_kind, "project");
        assert_eq!(card.dedup_key, "R24|Windows|d--proj");
        assert_eq!(card.est_tokens_saved, 0);
        assert_eq!(card.prescription.as_ref().unwrap().kind, "context_hygiene");
        assert_eq!(card.evidence["worst_carry_ratio_pct"], 100);
        assert_eq!(card.evidence["worst_episodes"], 6);
        assert_eq!(card.evidence["worst_session_id"], "s1");
        assert!(card.evidence["sample_prompts"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn silent_for_healthy_clear_habit() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 6개 에피소드지만 전부 2k만 상속 (/clear로 끊음) → carry_ratio 0%.
        store.upsert_events(&session_with_episodes("s1", 6, 2_000, 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn silent_below_min_episodes() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 4개 에피소드(큰 상속이어도) < min_episodes(5) → 침묵.
        store.upsert_events(&session_with_episodes("s1", 4, 80_000, 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn silent_outside_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        // base_h = 24*20 시간 전(=20일 전) → 관찰창(14일) 밖.
        store.upsert_events(&session_with_episodes("s1", 6, 60_000, 24 * 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn rollup_cites_worst_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 같은 프로젝트 두 세션 모두 위생 나쁨 — 상속량 다르게.
        store.upsert_events(&session_with_episodes("s1", 6, 55_000, 30)).unwrap();
        store.upsert_events(&session_with_episodes("s2", 6, 90_000, 20)).unwrap();

        let f = R24ContextHygiene::default().evaluate(&store).unwrap();
        assert_eq!(f.len(), 1, "프로젝트당 카드 1장");
        assert_eq!(f[0].evidence["bad_session_count"], 2);
        assert_eq!(f[0].evidence["worst_session_id"], "s2", "max_inherited 큰 세션 인용");
        assert_eq!(f[0].evidence["worst_max_inherited_tokens"], 90_000);
    }
}
```

- [ ] **Step 4: 테스트 실패 확인 → 통과**

Run: `cargo test -p agent-mentor r24_context_hygiene`
Expected: 5개 PASS. 실패 시 흔한 원인:
- `carry_ratio_pct` 반올림/타입 — `serde_json` 비교는 정수(`100`) 대 `Number`. `assert_eq!(v["k"], 100)`는 u64 리터럴과 매칭됨.
- 관찰창 경계: `silent_outside_window`가 통과하려면 base_h가 확실히 14일↑ 전이어야 함(24*20=480h=20일 → OK).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/rules/r24_context_hygiene.rs a-mate/crates/core/src/rules/mod.rs
git commit -m "feat(agent): add R24 context-hygiene rule on episode segmenter"
```

---

## Task 3: 파이프라인 등록 (`ops.rs::run_rules`)

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` (run_rules 엔진 벡터 + 테스트)

**Interfaces:**
- Consumes: `crate::rules::r24_context_hygiene::R24ContextHygiene`(Task 2), 기존 `RuleEngine`.
- Produces: run_rules 결과에 R24 project 카드 포함, upsert 시 status=`new`.

- [ ] **Step 1: 엔진에 R24 등록**

`a-mate/crates/core/src/ops.rs` 171행(`Box::new(crate::rules::r8_mcp_large_result::R8McpLargeResult::default()),`) 다음 줄에 추가:

```rust
        // F(컨텍스트 위생) — 에피소드 세그먼터 기반 결정론 룰 (코칭 v3 재설계 §4 F)
        Box::new(crate::rules::r24_context_hygiene::R24ContextHygiene::default()),
```

- [ ] **Step 2: 실패하는 등록·노출 테스트 작성**

`ops.rs`의 `#[cfg(test)] mod tests`에 추가. (기존 테스트 헬퍼 `mk`가 있으면 재사용하지 말고, 이 테스트는 실제 이벤트를 시드해 run_rules 전 경로를 탄다.)

```rust
    #[test]
    fn run_rules_registers_r24_and_surfaces_project_card_as_new() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let recent = |h: i64| (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339();
        // 6개 에피소드, 전부 60k 상속 → R24 발화.
        let mut evs = Vec::new();
        for i in 0..6u64 {
            let ts = recent(20 - i as i64);
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(),
                session_id: "s1".into(), uuid: Some(format!("p{i}")), parent_uuid: None,
                is_sidechain: false, ts: Some(ts.clone()), source_file: "s.jsonl".into(),
                source_offset: i * 2, msg_id: None,
                kind: EventKind::UserPrompt { preview: format!("에피소드 {i} 실질 작업 지시 문장") },
            });
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(),
                session_id: "s1".into(), uuid: Some(format!("a{i}")), parent_uuid: None,
                is_sidechain: false, ts: Some(ts), source_file: "s.jsonl".into(),
                source_offset: i * 2 + 1, msg_id: None,
                kind: EventKind::AssistantTurn {
                    model: NormModel::from_raw_id("claude-opus-4-8"),
                    usage: TokenUsage { input: 60_000, ..Default::default() },
                    web_search: 0, web_fetch: 0,
                },
            });
        }
        store.upsert_events(&evs).unwrap();

        let findings = run_rules(&store).unwrap();
        assert!(
            findings.iter().any(|f| f.rule_id == "R24" && f.scope_kind == "project"),
            "run_rules가 R24 프로젝트 카드를 낸다"
        );

        // 노출 상태: R24 project → status 'new' (즉시 노출).
        let status: String = store
            .conn
            .query_row(
                "SELECT status FROM findings WHERE dedup_key = 'R24|Windows|d--proj'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "new");
    }
```

- [ ] **Step 3: 테스트 확인**

Run: `cargo test -p agent-mentor run_rules_registers_r24_and_surfaces_project_card_as_new`
Expected: PASS. (init_status 맵이 R24/project를 `new`로 처리 — store.rs:464~467, 코드 변경 없이 통과.)

- [ ] **Step 4: 회귀 확인 + 커밋**

Run: `cargo test -p agent-mentor ops`
Expected: 기존 run_rules 테스트 전부 PASS (등록 추가가 기존 룰에 영향 없음).

```bash
git add a-mate/crates/core/src/ops.rs
git commit -m "feat(agent): register R24 context-hygiene rule in run_rules"
```

---

## Task 4: 코칭 카드 렌더링 (advice + fix_command + 제목)

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (finding_advice R24 arm + 테스트)
- Modify: `a-mate/crates/core/src/coach.rs` (R24 fix_command None 테스트)
- Modify: `a-mate/src/lib/ui/coach-helpers.ts` (COACH_TITLE R24)
- Create: `a-mate/src/lib/ui/coach-helpers.test.ts` (제목 vitest)

**Interfaces:**
- Consumes: R24 evidence 스키마(Task 2) — `worst_carry_ratio_pct, worst_episodes, worst_carry_count, inherited_ctx_threshold, sample_prompts`.
- Produces: `finding_advice("R24", ..)` → (detail, action) 한국어 문구. `fix_command("R24", ..)` → None. `coachTitle("R24", ..)` → 제목.

- [ ] **Step 1: finding_advice R24 arm 실패 테스트 작성**

`a-mate/crates/core/src/diary/mod.rs`의 finding_advice 테스트 블록(`finding_advice_r8_cites_server_and_measured_tokens` 근처)에 추가:

```rust
    #[test]
    fn finding_advice_r24_context_hygiene_cites_worst_session() {
        let ev = serde_json::json!({
            "worst_carry_ratio_pct": 83,
            "worst_episodes": 6,
            "worst_carry_count": 5,
            "inherited_ctx_threshold": 50_000,
            "sample_prompts": ["로그인 폼 만들어줘", "결제 붙여줘"],
        });
        let (detail, action) = finding_advice("R24", &ev, 0);
        assert!(detail.contains("83%"), "carry ratio 인용");
        assert!(detail.contains("6") && detail.contains("5"), "에피소드/큰컨텍스트 수 인용");
        assert!(action.contains("/clear"), "작업 경계에서 끊는 처방");
        assert!(action.contains("로그인 폼"), "가장 심한 세션 프롬프트 예시 인용");
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent-mentor finding_advice_r24_context_hygiene_cites_worst_session`
Expected: FAIL — 현재 `_ =>` default arm이 raw evidence JSON을 반환하므로 assert 불일치.

- [ ] **Step 3: finding_advice R24 arm 구현**

`diary/mod.rs`의 finding_advice `match rule_id` 안, `"R8" => {...}` arm 다음에 추가:

```rust
        "R24" => {
            let pct = evidence.get("worst_carry_ratio_pct").and_then(|v| v.as_u64()).unwrap_or(0);
            let eps = evidence.get("worst_episodes").and_then(|v| v.as_u64()).unwrap_or(0);
            let carry = evidence.get("worst_carry_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let thr_k = evidence
                .get("inherited_ctx_threshold")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                / 1000;
            let detail = format!(
                "이 프로젝트에서 작업을 바꿔도 컨텍스트를 끊지 않아, 가장 심한 세션은 {eps}개 작업 중 {carry}개({pct}%)가 큰 컨텍스트(~{thr_k}k 토큰↑)를 물려받은 채 시작했어요"
            );
            let mut action = "작업을 전환할 때 `/clear`로 컨텍스트를 한 번씩 끊으면 토큰뿐 아니라 응답 품질(주의 희석 감소)에도 도움이 돼요 — auto-compact에 맡기기보다 작업 경계에서 끊어보세요".to_string();
            if let Some(p) = evidence
                .get("sample_prompts")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            {
                action.push_str(&format!("\n💬 예: '{p}'"));
            }
            (detail, action)
        }
```

- [ ] **Step 4: R24 advice 테스트 통과 확인**

Run: `cargo test -p agent-mentor finding_advice_r24_context_hygiene_cites_worst_session`
Expected: PASS.

- [ ] **Step 5: coach fix_command R24=None 테스트 추가**

`a-mate/crates/core/src/coach.rs`의 `v2_aggregate_rules_have_no_fix_command` 테스트에 assert 한 줄 추가(R24는 처방 CLI 없음 — 습관 넛지):

```rust
        // F(컨텍스트 위생)은 습관 넛지 — 복사 명령 없음
        assert_eq!(fix_command("R24", &json!({})), None);
```

Run: `cargo test -p agent-mentor v2_aggregate_rules_have_no_fix_command`
Expected: PASS (R24는 `match`의 `_ => None` 분기 — 코드 변경 불필요, 계약을 테스트로 고정).

- [ ] **Step 6: 백엔드 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs a-mate/crates/core/src/coach.rs
git commit -m "feat(agent): render R24 context-hygiene coaching card copy"
```

- [ ] **Step 7: 프론트 제목 실패 테스트 작성**

`a-mate/src/lib/ui/coach-helpers.test.ts` 생성:

```ts
import { describe, expect, it } from 'vitest';
import { coachTitle } from './coach-helpers';

describe('coachTitle', () => {
  it('R24 컨텍스트 위생 카드 제목', () => {
    expect(coachTitle('R24', {})).toBe('작업을 바꿀 때 컨텍스트를 끊으면 더 좋아요');
  });
  it('알 수 없는 룰은 제네릭 폴백', () => {
    expect(coachTitle('RZ', {})).toBe('아낄 수 있는 게 보여요');
  });
});
```

Run: `npm test -- coach-helpers`
Expected: 첫 테스트 FAIL (R24 미등록 → 폴백 '아낄 수 있는 게 보여요' 반환).

- [ ] **Step 8: COACH_TITLE에 R24 추가**

`a-mate/src/lib/ui/coach-helpers.ts`의 `COACH_TITLE` 맵(38행 `R12:` 뒤)에 추가:

```ts
  R24: '작업을 바꿀 때 컨텍스트를 끊으면 더 좋아요',
```

Run: `npm test -- coach-helpers`
Expected: 2개 PASS.

- [ ] **Step 9: 프론트 커밋**

```bash
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(agent): add R24 coaching card title in coach tab"
```

---

## Task 5: 검증 & DoD

**Files:** 없음 (검증·아카이브·PR).

- [ ] **Step 1: 백엔드 전체 테스트**

Run (a-mate/): `cargo test -p agent-mentor`
Expected: 베이스라인(404) + 신규(세그먼터 5 + R24 5 + run_rules 1 + advice 1) 전부 PASS, 0 failures.

- [ ] **Step 2: 프론트 전체 테스트**

Run (a-mate/): `npm test`
Expected: 기존 + coach-helpers 2 PASS.

- [ ] **Step 3: Tauri 앱 빌드 클린 확인**

Run (a-mate/, 네이티브 Windows): `cargo build`
Expected: 워크스페이스(core + src-tauri) 클린 빌드. (프론트 dev 서버 없이 Rust 컴파일만 확인.)

- [ ] **Step 4: 스펙 대조 자기점검**

스펙 §4 F 요구사항 대조:
- [x] 결정론(LLM 미사용) — R8형
- [x] 에피소드 세그먼터 소비 — `segment_all`
- [x] 첫 main-chain 턴 `tok_input+tok_cache_read` 상속 신호
- [x] carry_ratio·min_episodes·inherited_ctx_threshold 파라미터
- [x] (host,project) 롤업 카드 1장 + 가장 심한 세션 근거 인용
- [x] kind `context_hygiene`, 품질 우선 프레이밍, auto-compact 비난 X, `est_tokens_saved=0`, severity Suggest
- [x] 임계값 잠정(`⚠ CALIBRATE`)
- [x] 룰 ID R24, dedup_key `R24|{host}|{project}`

- [ ] **Step 5: DoD — 계획 문서 아카이브**

`docs-archive` 스킬을 실행해 이 계획(`docs/design/a-mate/plans/2026-07-22-coaching-f-context-hygiene.md`)을 같은 PR에서 `docs/archive/` 미러로 옮긴다 (ADR 0013).

- [ ] **Step 6: PR 생성**

`superpowers:finishing-a-development-branch` 또는 직접 PR — base `main`. 제목: `feat(agent): coaching F — context hygiene (episode segmenter + R24)`. 본문에 스펙 링크·구현 순서 내 위치(B→**F**→C→A→E) 명시.

---

## 미확정 / 후속 (스펙 §8 계승)

- **임계값 핀**: `inherited_ctx_threshold(50k)`·`carry_ratio(60%)`·`min_episodes(5)`·`min_bad_sessions(1)` → Windows 실데이터 캘리브레이션.
- **세그먼터 `glue_followups`**: C PR에서 ingest(<8자 원문 보존) 확장과 함께 추가 — C(재설명 루프)·A가 소비.
- **had_compaction 활용**: v1은 evidence에 싣지 않고 세그먼터 필드로만 확보. 보조 신호로 발화 게이트에 넣는 것은 캘리브레이션 후 검토.
