# A — R6 채굴 강화 (워크플로 레버리지) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** R6(반복 지시 → 스킬화)의 **채굴(후보 찾기)만** 강화해 두 사각을 메운다 — ① never-clear(한 세션 반복도 발화), ② 완전일치만(살짝 다른 표현도 느슨히 묶어 후보화). 판정·스킬 초안·노출은 기존 R6 그대로 재사용한다.

**Architecture:** R6의 `evaluate`를 "SQL 그룹화 그대로"에서 "SQL 집계 → Rust 느슨한 군집화 → 묶음별 finding"으로 재구성한다. 발화 문턱에 `occurrences_total`(이미 집계됨) OR 조건을 추가하고, 세션 카운트 단독 의존을 제거한다. 유사 묶기는 **문자 bigram Jaccard 유사도**(재현율 위주, 결정론)로 하고, **정밀도는 기존 R6 LLM 판정이 게이트**한다(설계 결정 (나)). dedup_key는 묶음의 **사전순 최소 norm60을 앵커**로 고정해 판정 캐시를 보존한다.

**Tech Stack:** Rust (`crates/core` = lib `agent_mentor`), rusqlite (SQLite), 기존 `CoachingJudge`/`R6Judge` 판정 패스, 기존 `skill_draft` 초안 생성. 신규 의존성 없음.

## Global Constraints

§2 불변 계약(스펙 `docs/design/a-mate/specs/2026-07-22-coaching-value-redesign-design.md`) — 모든 Task가 상속:

- **근거 = 사용자가 직접 친 프롬프트 전문 + main-chain 사실만.** `prompt_events`는 이미 sidechain·meta·도구결과·주입을 제외한 "사람 발화"만 담는다. 에이전트/서브에이전트 산출물·모델선택 채점 금지.
- **`est_tokens_saved = 0`** (근거 없는 수치 금지) — 기존 R6 관행 유지.
- **결정론 채굴 → LLM 판정 게이트.** 채굴은 순수 결정론(SQL+Rust). 유사 묶기 임계는 낮게(느슨), 정밀도는 LLM.
- **Fail-safe 침묵.** 엔진 미설정이면 판정 no-op(pending 잔류). 채굴 자체는 엔진 무관.
- **키 안정성.** 묶음 dedup_key는 앵커(사전순 최소 norm60) 기반 → 새 변형이 들어와도 흔들리지 않아 "이미 판정/닫음" 상태 보존.
- **임계값은 잠정(⚠ CALIBRATE)** — Windows 실데이터로 핀(스펙 §8). B(#92)·F(#93) 관행.

**설계 노트(스펙 대비 확정 사항):**
- A는 **에피소드 세그먼터를 소비하지 않는다.** `prompt_events` 한 행 = 실질 프롬프트 = 에피소드 lead 이므로 `occurrences_total`(=`COUNT(*)`)가 곧 에피소드 단위 반복 수다 — 세그먼터가 더할 정밀도가 없다. 따라서 PR #93의 "run_rules에서 segment_all 공유 캐싱" 후속은 **불필요**(F가 유일 소비자로 유지).
- 이 계획은 R6 v2 스펙(`2026-07-20-r6-v2-session-repeat-mining.md`) §4.3의 "세션당 1회만 센다(세션 내 다회는 evidence 병기만)"를 **의도적으로 대체**한다 — 그 설계가 never-clear 사각의 원인이었다.

---

## 파일 구조

| 파일 | 책임 | 변경 |
|------|------|------|
| `crates/core/src/rules/r6_cluster.rs` | norm60 목록의 느슨한 군집화(문자 bigram Jaccard, union-find). 순수 로직. | **신규** |
| `crates/core/src/rules/mod.rs` | `pub mod r6_cluster;` 등록 | 수정 |
| `crates/core/src/store.rs` | `prompt_occurrence_rows`(채굴 재료), `prompt_sessions_for_norms`(여러 norm 재료). `prompt_sessions_for_norm`(단수) 제거. | 수정 |
| `crates/core/src/rules/r6_repeated_prompts.rs` | `evaluate` 재구성(집계→군집→묶음 finding), `min_occurrences`·`cluster_threshold` 필드, occurrence-OR 발화, member_norms evidence, 앵커 키 | 수정 |
| `crates/core/src/skill_draft.rs` | `gather_context_multi`(여러 norm) 추가, `gather_context`는 위임 | 수정 |
| `crates/core/src/judge.rs` | `R6Judge::build_prompt`이 evidence의 `member_norms`로 묶음 전체 재료 수집 | 수정 |

**무변경(검증만):** `src-tauri/src/pipeline.rs`(`run_coaching_judgments` 드라이버 — 아이템 무관), `R6Judge::classify`/`rollup`(항등), 프론트 `CoachTab.svelte`(evidence `repeated_prompt` 그대로, `member_norms`는 추가 키라 무시됨). CLI `main.rs::cmd_skill_draft`·Tauri `commands.rs::generate_skill_draft`는 `gather_context(대표)`로 앵커 기준 초안을 만든다 — 묶음 전체 재료는 판정 경로만 사용(v1 범위 경계).

---

### Task 1: 느슨한 군집화 순수 함수 (`r6_cluster.rs`)

**Files:**
- Create: `a-mate/crates/core/src/rules/r6_cluster.rs`
- Modify: `a-mate/crates/core/src/rules/mod.rs` (모듈 등록)
- Test: 같은 파일 `#[cfg(test)]`

**Interfaces:**
- Produces: `pub(crate) fn cluster_norms(norms: &[String], threshold: f64) -> Vec<Vec<usize>>`
  — 입력 norm60 목록을 문자 bigram Jaccard 유사도 ≥ threshold 인 쌍끼리 묶는다(union-find). 반환: 각 묶음 = `norms`의 인덱스 오름차순 `Vec`, 묶음들은 최소 인덱스 오름차순. 입력 순서에만 의존(결정론).

- [ ] **Step 1: 실패하는 테스트 작성**

`a-mate/crates/core/src/rules/r6_cluster.rs` 에 파일을 만들고 테스트부터 넣는다(구현은 다음 스텝):

```rust
//! R6 채굴 강화(A) — 정규화 프롬프트(norm60)의 느슨한 군집화(재현율 위주).
//! 문자 bigram Jaccard 유사도로 "살짝 다르게 쓴 같은 지시"를 한 묶음으로 모은다.
//! 정밀도는 상위 R6 LLM 판정이 담당하므로 threshold는 낮게(느슨하게) 잡는다.
//! LLM·네트워크 무관 순수 로직 — 한국어 조사 변형(코멘트/코멘트를)에도 문자 단위라 강건.

use std::collections::HashSet;

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clusters_paraphrases_sharing_most_chars() {
        // "pr 리뷰 코멘트 종합 검토..." 변형 3개 — 대부분 문자를 공유 → 한 묶음.
        let norms = v(&[
            "pr 리뷰 코멘트 종합 검토해서 조치해줘",
            "pr 리뷰 코멘트 종합 검토하고 반영해줘",
            "pr 리뷰 코멘트 종합 검토 후 조치",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters.len(), 1, "패러프레이즈 3개는 한 묶음");
        assert_eq!(clusters[0], vec![0, 1, 2]);
    }

    #[test]
    fn separates_unrelated_norms() {
        let norms = v(&[
            "매일 아침 판매 리포트 뽑아줘",
            "도커 컨테이너 로그 확인해줘",
            "리액트 상태관리 코드 리팩터",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters.len(), 3, "관련 없는 지시는 각자 싱글턴");
    }

    #[test]
    fn identical_norms_cluster_together() {
        let norms = v(&["같은 지시를 두 번 넣었다", "같은 지시를 두 번 넣었다"]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters, vec![vec![0, 1]]);
    }

    #[test]
    fn deterministic_min_index_anchor_ordering() {
        // 0과 2가 유사, 1은 별개 → 묶음 [0,2] 는 인덱스 오름차순, 그리고 [1] 보다 먼저(최소 인덱스 0).
        let norms = v(&[
            "리뷰 코멘트 종합 검토해줘",
            "완전히 다른 무관한 작업 지시",
            "리뷰 코멘트 종합 검토 부탁",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters, vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(cluster_norms(&[], 0.5).is_empty());
    }
}
```

`a-mate/crates/core/src/rules/mod.rs` 에 모듈 등록(`pub mod r5_repeated_read;` 아래 근처, 알파벳/기존 순서에 맞춰):

```rust
pub mod r6_cluster;
```

- [ ] **Step 2: 실패 확인**

Run (from `a-mate/`): `cargo test r6_cluster`
Expected: 컴파일 실패 — `cannot find function cluster_norms`.

- [ ] **Step 3: 최소 구현**

테스트 모듈 위, `use std::collections::HashSet;` 아래에 구현을 추가:

```rust
/// 문자 bigram 집합. 공백 붕괴된 norm60 기준. 길이 1이면 그 문자를 유니그램으로(폴백).
fn bigrams(s: &str) -> HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = HashSet::new();
    if chars.len() < 2 {
        if let Some(&c) = chars.first() {
            set.insert((c, c));
        }
        return set;
    }
    for w in chars.windows(2) {
        set.insert((w[0], w[1]));
    }
    set
}

/// 두 bigram 집합의 Jaccard 유사도 = |∩| / |∪|. 한쪽이라도 비면 0.
fn jaccard(a: &HashSet<(char, char)>, b: &HashSet<(char, char)>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// union-find 루트(경로 압축).
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// norm60 목록을 느슨하게 군집화. sim ≥ threshold 인 쌍을 같은 묶음으로(union-find).
/// 반환: 각 묶음 = 인덱스 오름차순 Vec, 묶음들은 최소 인덱스 오름차순. 결정론(입력 순서 의존).
pub(crate) fn cluster_norms(norms: &[String], threshold: f64) -> Vec<Vec<usize>> {
    let n = norms.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let grams: Vec<HashSet<(char, char)>> = norms.iter().map(|s| bigrams(s)).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if jaccard(&grams[i], &grams[j]) >= threshold {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    // 작은 루트로 병합 → 앵커(최소 인덱스) 안정성.
                    let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                    parent[hi] = lo;
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().collect()
}
```

- [ ] **Step 4: 통과 확인**

Run (from `a-mate/`): `cargo test r6_cluster`
Expected: 5개 테스트 PASS.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/rules/r6_cluster.rs a-mate/crates/core/src/rules/mod.rs
git commit -m "feat(agent): add loose norm60 clustering for R6 mining (A)"
```

---

### Task 2: 채굴 재료 스토어 쿼리 (`store.rs`)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs`
- Test: 같은 파일 `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `pub struct PromptOccRow { pub host: String, pub norm60: String, pub session_id: String, pub occurrences: u64, pub preview: String }`
  - `pub fn prompt_occurrence_rows(&self, cutoff: &str) -> Result<Vec<PromptOccRow>>`
    — 관찰창(`ts >= cutoff`) 내 `(host, norm60, session_id)`별 등장수(`COUNT(*)`)와 대표 preview(`MIN(preview)`). ts NULL 행은 제외(기존 R6 SQL과 동일).
  - `pub fn prompt_sessions_for_norms(&self, host: &str, norms: &[String]) -> Result<Vec<(String, String)>>`
    — 주어진 여러 norm60에 매칭되는 `(session_id, preview)` 전량(변수 한도 대비 990개씩 청크).
- Consumes: 기존 `prompt_events` 테이블.

- [ ] **Step 1: 실패하는 테스트 작성**

`store.rs`의 `#[cfg(test)] mod tests` 안(파일 하단)에 추가. 시딩은 기존 store 테스트 패턴을 따른다(직접 INSERT 대신 `upsert_events`로 `UserPrompt` 이벤트를 넣어 `prompt_events` 적재 경로를 태운다):

```rust
    #[test]
    fn prompt_occurrence_rows_aggregates_per_norm_session() {
        use crate::model::{EventKind, NormalizedEvent};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sess: &str, off: u64, text: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-{off}")), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: off,
            msg_id: None, kind: EventKind::UserPrompt { preview: text.into() },
        };
        // s1: 같은 지시 2회(다른 ts) + s2: 같은 지시 1회. (모두 8자↑ = norm60 대상)
        store.upsert_events(&[
            mk("s1", 0, "이 함수 리팩토링 진행해줘", "2026-07-01T10:00:00Z"),
            mk("s1", 1, "이 함수 리팩토링 진행해줘", "2026-07-01T10:05:00Z"),
            mk("s2", 0, "이 함수 리팩토링 진행해줘", "2026-07-01T11:00:00Z"),
        ]).unwrap();

        let rows = store.prompt_occurrence_rows("2026-06-01T00:00:00Z").unwrap();
        // (host,norm,session) 그룹: (s1)=2, (s2)=1
        assert_eq!(rows.len(), 2);
        let s1 = rows.iter().find(|r| r.session_id == "s1").unwrap();
        assert_eq!(s1.occurrences, 2);
        assert!(s1.norm60.contains("리팩토링"));
        let s2 = rows.iter().find(|r| r.session_id == "s2").unwrap();
        assert_eq!(s2.occurrences, 1);
    }

    #[test]
    fn prompt_occurrence_rows_excludes_outside_window() {
        use crate::model::{EventKind, NormalizedEvent};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "old".into(),
            uuid: Some("old-0".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-01-01T00:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0, msg_id: None,
            kind: EventKind::UserPrompt { preview: "관찰창 밖 오래된 지시".into() },
        }]).unwrap();
        let rows = store.prompt_occurrence_rows("2026-06-01T00:00:00Z").unwrap();
        assert!(rows.is_empty(), "cutoff 이전 프롬프트는 제외");
    }

    #[test]
    fn prompt_sessions_for_norms_unions_multiple_norms() {
        use crate::model::{EventKind, NormalizedEvent};
        use crate::rules::r6_repeated_prompts::normalize;
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sess: &str, text: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-0")), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None, kind: EventKind::UserPrompt { preview: text.into() },
        };
        store.upsert_events(&[
            mk("s1", "리뷰 코멘트 종합 검토해줘", "2026-07-01T10:00:00Z"),
            mk("s2", "리뷰 코멘트 종합 반영 부탁", "2026-07-01T11:00:00Z"),
        ]).unwrap();
        let n1 = normalize("리뷰 코멘트 종합 검토해줘").unwrap();
        let n2 = normalize("리뷰 코멘트 종합 반영 부탁").unwrap();

        let rows = store.prompt_sessions_for_norms("Windows", &[n1, n2]).unwrap();
        let sessions: std::collections::HashSet<&str> =
            rows.iter().map(|(s, _)| s.as_str()).collect();
        assert!(sessions.contains("s1") && sessions.contains("s2"), "두 norm의 세션 합집합");
    }

    #[test]
    fn prompt_sessions_for_norms_empty_returns_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.prompt_sessions_for_norms("Windows", &[]).unwrap().is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run (from `a-mate/`): `cargo test prompt_occurrence_rows prompt_sessions_for_norms`
Expected: 컴파일 실패 — `no method named prompt_occurrence_rows` / `PromptOccRow` 미정의.

- [ ] **Step 3: 최소 구현**

`store.rs`에서 기존 `prompt_sessions_for_norm`(단수, 약 1413행) 근처에 추가한다. 먼저 구조체를 `impl SqliteStore` 블록 **밖**(파일 상단의 다른 pub 구조체들과 같은 층위)에 정의:

```rust
/// R6 채굴(A) 재료 — 관찰창 내 (host, norm60, session)별 등장수·대표 preview.
pub struct PromptOccRow {
    pub host: String,
    pub norm60: String,
    pub session_id: String,
    pub occurrences: u64,
    pub preview: String,
}
```

그리고 `impl SqliteStore` 안, `prompt_sessions_for_norm` 근처에 두 메서드 추가:

```rust
    /// R6 채굴(A) — 관찰창 내 (host, norm60, session)별 등장수 + 대표 preview.
    /// 미더가 Rust에서 norm 단위 집계 + 느슨한 군집화에 쓴다. ts NULL 행은 제외(기존 R6 SQL 동치).
    pub fn prompt_occurrence_rows(&self, cutoff: &str) -> Result<Vec<PromptOccRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT host, norm60, session_id, COUNT(*), MIN(preview)
             FROM prompt_events WHERE ts >= ?1
             GROUP BY host, norm60, session_id
             ORDER BY host, norm60, session_id",
        )?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok(PromptOccRow {
                host: r.get(0)?,
                norm60: r.get(1)?,
                session_id: r.get(2)?,
                occurrences: r.get::<_, i64>(3)? as u64,
                preview: r.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// R6 스킬 초안용(A) — 여러 정규화 지시(norm60)에 매칭되는 (session_id, preview) 전량.
    /// SQLite 변수 한도 대비 990개씩 청크. 세션·원문 중복 제거는 호출부(skill_draft) 책임.
    pub fn prompt_sessions_for_norms(
        &self,
        host: &str,
        norms: &[String],
    ) -> Result<Vec<(String, String)>> {
        if norms.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for chunk in norms.chunks(990) {
            let placeholders = std::iter::repeat("?").take(chunk.len()).collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT session_id, preview FROM prompt_events
                 WHERE host = ? AND norm60 IN ({placeholders}) ORDER BY id",
            );
            let mut binds: Vec<String> = Vec::with_capacity(chunk.len() + 1);
            binds.push(host.to_string());
            binds.extend(chunk.iter().cloned());
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }
```

- [ ] **Step 4: 통과 확인**

Run (from `a-mate/`): `cargo test prompt_occurrence_rows prompt_sessions_for_norms`
Expected: 4개 테스트 PASS.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add R6 mining store queries (occurrence rows, multi-norm sessions)"
```

---

### Task 3: `evaluate` 재구성 + occurrence 발화 (never-clear 사각)

**Files:**
- Modify: `a-mate/crates/core/src/rules/r6_repeated_prompts.rs`
- Test: 같은 파일 `#[cfg(test)]`

**Interfaces:**
- Consumes: `store.prompt_occurrence_rows(cutoff)` (Task 2), `hash8`(기존), `normalize`(기존).
- Produces: R6 finding evidence에 신규 키 `member_norms: [String]`(이 Task에선 항상 단일 원소=앵커 norm). dedup_key = `R6|{host}|{hash8(anchor_norm60)}`(싱글턴에선 기존과 동일). 발화 조건: `session_count >= min_sessions` **OR** `occurrences_total >= min_occurrences`.

이 Task는 **군집화 없이**(각 norm = 자기 자신 묶음) 재구성만 하고, occurrence 발화까지 넣는다. 실제 유사 묶기는 Task 4.

- [ ] **Step 1: 실패하는(그리고 뒤집히는) 테스트 작성/수정**

기존 테스트 `r6_counts_session_once_despite_in_session_repeats`(약 160–170행)를 **뒤집는다**(이제 발화해야 함). 내용을 다음으로 교체:

```rust
    #[test]
    fn r6_fires_on_within_session_repetition_never_clear() {
        // never-clear 사각: 한 세션에서 같은 지시를 여러 번 반복하면 세션=1~2라도
        // occurrences_total 문턱으로 발화한다 (A — R6 v2 §4.3 "세션당 1회" 대체).
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "s1", "이 함수 리팩토링 진행해줘", &ts_at(i as i64), *off);
        }
        seed_prompt_at(&store, "s2", "이 함수 리팩토링 진행해줘", &ts_at(10), 0);
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "occurrences_total(6) ≥ 문턱 → 발화");
        assert_eq!(findings[0].evidence["occurrences_total"], 6);
        assert_eq!(findings[0].evidence["session_count"], 2);
    }

    #[test]
    fn r6_fires_on_single_session_heavy_repetition() {
        // 순수 never-clear: 한 세션 안에서만 5회 반복(세션=1) — occurrence 경로로 발화.
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, off) in [0u64, 10, 20, 30, 40].iter().enumerate() {
            seed_prompt_at(&store, "solo", "매주 배포 전 체크리스트 점검해줘", &ts_at(i as i64), *off);
        }
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence["session_count"], 1);
        assert_eq!(findings[0].evidence["occurrences_total"], 5);
        assert!(findings[0].evidence.get("member_norms").is_some(), "member_norms 키 존재");
    }
```

- [ ] **Step 2: 실패 확인**

Run (from `a-mate/`): `cargo test r6_fires_on_within_session_repetition_never_clear r6_fires_on_single_session_heavy_repetition`
Expected: FAIL — 현행 `evaluate`는 세션 문턱만 봐서 발화하지 않음(빈 결과).

- [ ] **Step 3: 구현 — `evaluate` 재구성 + `min_occurrences`**

구조체와 Default를 수정(약 12–23행):

```rust
pub struct R6RepeatedPrompts {
    /// 같은 지시로 열린 세션 수 문턱 (다세션 반복 경로).
    pub min_sessions: usize,
    /// ⚠ CALIBRATE — 한 세션 내 반복도 잡는 발화 문턱(총 등장수). never-clear 사각용.
    /// Windows 실데이터로 핀(스펙 §8). R7·R8·F 관행.
    pub min_occurrences: u64,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R6RepeatedPrompts {
    fn default() -> Self {
        R6RepeatedPrompts { min_sessions: 3, min_occurrences: 5, days: 14 }
    }
}
```

`evaluate` 전체를 교체(약 46–90행):

```rust
    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        use std::collections::{BTreeMap, BTreeSet};
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let rows = store.prompt_occurrence_rows(&cutoff)?;

        // 1) (host, norm60) 집계: 세션 집합·총 등장수·대표 preview(MIN 동치).
        struct NormAgg {
            sessions: BTreeSet<String>,
            occurrences: u64,
            preview: String,
        }
        let mut per_host: BTreeMap<String, BTreeMap<String, NormAgg>> = BTreeMap::new();
        for r in rows {
            let host_map = per_host.entry(r.host).or_default();
            let agg = host_map.entry(r.norm60).or_insert_with(|| NormAgg {
                sessions: BTreeSet::new(),
                occurrences: 0,
                preview: r.preview.clone(),
            });
            agg.sessions.insert(r.session_id);
            agg.occurrences += r.occurrences;
            if r.preview < agg.preview {
                agg.preview = r.preview; // MIN(preview)
            }
        }

        // 2) host별 묶기 → 묶음별 finding. (Task 3: 싱글턴, Task 4: 실제 군집)
        let mut out = Vec::new();
        for (host, norm_map) in per_host {
            let norms: Vec<String> = norm_map.keys().cloned().collect();
            let clusters: Vec<Vec<usize>> = (0..norms.len()).map(|i| vec![i]).collect();

            for cluster in clusters {
                // norms 는 사전순 정렬(BTreeMap keys) → cluster[0] = 사전순 최소 = 앵커.
                let member_norms: Vec<String> =
                    cluster.iter().map(|&i| norms[i].clone()).collect();
                let anchor = &member_norms[0];
                let mut sessions: BTreeSet<&String> = BTreeSet::new();
                let mut occurrences: u64 = 0;
                for nrm in &member_norms {
                    let a = &norm_map[nrm];
                    for s in &a.sessions {
                        sessions.insert(s);
                    }
                    occurrences += a.occurrences;
                }
                let session_count = sessions.len();
                // 발화: 다세션 반복 OR 세션 내 다회 반복(never-clear).
                if session_count < self.min_sessions && occurrences < self.min_occurrences {
                    continue;
                }
                let h = hash8(anchor);
                let rep_short: String = norm_map[anchor].preview.chars().take(60).collect();
                out.push(Finding {
                    rule_id: "R6".into(),
                    severity: Severity::Suggest,
                    scope_host: Some(host.clone()),
                    scope_project: None,
                    scope_kind: "pattern".into(),
                    scope_ref: format!("pattern:{h}"),
                    evidence: serde_json::json!({
                        "repeated_prompt": rep_short,
                        "session_count": session_count,
                        "occurrences_total": occurrences,
                        "member_norms": member_norms,
                        "window_days": self.days,
                    }),
                    est_tokens_saved: 0, // 근거 없는 수치 금지 — 가치 제안형
                    prescription: Some(Prescription {
                        kind: "skillify".into(),
                        payload: serde_json::json!({ "preview": rep_short }),
                    }),
                    dedup_key: format!("R6|{host}|{h}"),
                });
            }
        }
        Ok(out)
    }
```

- [ ] **Step 4: 통과 확인 (신규 + 기존 회귀)**

Run (from `a-mate/`): `cargo test r6_`
Expected: 신규 2개 PASS. 기존 `r6_fires_on_three_sessions_with_same_prompt`·`r6_fires_on_mid_session_repeats_across_sessions`·`r6_ignores_short_or_rare_prompts`·`r6_counts_forked_copies_once` 모두 여전히 PASS (싱글턴 묶음이라 다세션 경로 동작 불변).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/rules/r6_repeated_prompts.rs
git commit -m "feat(agent): fire R6 on within-session repetition (never-clear blind spot, A)"
```

---

### Task 4: 느슨한 유사 묶기 활성화 (완전일치만 사각)

**Files:**
- Modify: `a-mate/crates/core/src/rules/r6_repeated_prompts.rs`
- Test: 같은 파일 `#[cfg(test)]`

**Interfaces:**
- Consumes: `crate::rules::r6_cluster::cluster_norms` (Task 1).
- Produces: `R6RepeatedPrompts.cluster_threshold: f64` 필드. 묶음의 `member_norms`가 이제 다중 원소일 수 있고, 세션/등장수는 묶음 전체 합집합·합계. dedup_key 앵커 = 묶음 내 사전순 최소 norm60.

- [ ] **Step 1: 실패하는 테스트 작성 + 기존 노이즈 시드 수정**

새 테스트 추가:

```rust
    #[test]
    fn r6_clusters_paraphrased_repeats_across_sessions() {
        // 완전일치만 사각: 표현이 조금씩 달라도(다른 norm60) 문자 유사도로 묶여 한 후보가 된다.
        // 세션 3개(각 1회) → 완전일치로는 각 1개라 침묵했겠지만, 묶여서 session_count=3 발화.
        let store = SqliteStore::open_in_memory().unwrap();
        seed_prompt_at(&store, "s1", "pr 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(0), 0);
        seed_prompt_at(&store, "s2", "pr 리뷰 코멘트 종합 검토하고 반영해줘", &ts_at(1), 0);
        seed_prompt_at(&store, "s3", "pr 리뷰 코멘트 종합 검토 후 조치", &ts_at(2), 0);

        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "패러프레이즈 3개가 한 묶음으로 발화");
        let f = &findings[0];
        assert_eq!(f.evidence["session_count"], 3);
        let members = f.evidence["member_norms"].as_array().unwrap();
        assert!(members.len() >= 2, "묶음에 서로 다른 표현(norm)이 여러 개");
    }
```

기존 `r6_fires_on_mid_session_repeats_across_sessions`(약 144–158행)의 노이즈 프롬프트 `"서로 다른 작업 요청 {i}번"`은 이제 서로 문자 유사도가 높아 한 묶음으로 뭉쳐 **의도치 않은 추가 finding**을 만든다. 노이즈를 상호 비유사한 3개로 교체:

```rust
    #[test]
    fn r6_fires_on_mid_session_repeats_across_sessions() {
        // 첫 프롬프트가 아니라 세션 중간에 반복되는 지시도 잡는다 (v2 — 스펙 §4.3)
        let store = SqliteStore::open_in_memory().unwrap();
        let noise = ["도커 이미지 빌드 캐시 정리", "리액트 훅 의존성 배열 점검", "sql 인덱스 실행계획 확인"];
        for (i, sess) in ["s1", "s2", "s3"].iter().enumerate() {
            seed_prompt_at(&store, sess, noise[i], &ts_at(i as i64 * 2), 0);
            seed_prompt_at(&store, sess, "PR 리뷰 코멘트 종합 검토해서 조치해줘", &ts_at(i as i64 * 2 + 1), 10);
        }
        let findings = R6RepeatedPrompts::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.evidence["session_count"], 3);
        assert_eq!(f.evidence["occurrences_total"], 3);
        assert!(f.evidence["repeated_prompt"].as_str().unwrap().contains("리뷰 코멘트"));
    }
```

- [ ] **Step 2: 실패 확인**

Run (from `a-mate/`): `cargo test r6_clusters_paraphrased_repeats_across_sessions`
Expected: FAIL — 현재 싱글턴 묶기라 3개가 각각 세션 1개 → 발화 안 함(빈 결과).

- [ ] **Step 3: 구현 — 군집화 배선 + `cluster_threshold`**

구조체에 필드 추가:

```rust
pub struct R6RepeatedPrompts {
    /// 같은 지시로 열린 세션 수 문턱 (다세션 반복 경로).
    pub min_sessions: usize,
    /// ⚠ CALIBRATE — 한 세션 내 반복도 잡는 발화 문턱(총 등장수). never-clear 사각용.
    pub min_occurrences: u64,
    /// ⚠ CALIBRATE — 느슨한 묶기 문자 bigram Jaccard 임계(재현율 위주, 정밀도는 LLM 판정).
    /// Windows 실데이터로 핀(스펙 §8).
    pub cluster_threshold: f64,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R6RepeatedPrompts {
    fn default() -> Self {
        R6RepeatedPrompts { min_sessions: 3, min_occurrences: 5, cluster_threshold: 0.5, days: 14 }
    }
}
```

`evaluate`에서 싱글턴 배정 줄을 실제 군집화로 교체:

```rust
            // 이전(Task 3): let clusters: Vec<Vec<usize>> = (0..norms.len()).map(|i| vec![i]).collect();
            let clusters = crate::rules::r6_cluster::cluster_norms(&norms, self.cluster_threshold);
```

- [ ] **Step 4: 통과 확인 (신규 + 기존 회귀)**

Run (from `a-mate/`): `cargo test r6_`
Expected: 신규 `r6_clusters_paraphrased_repeats_across_sessions` PASS. 수정한 `r6_fires_on_mid_session_repeats_across_sessions` PASS. 나머지 R6 테스트 전부 PASS.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/rules/r6_repeated_prompts.rs
git commit -m "feat(agent): cluster near-duplicate prompts in R6 mining (exact-match blind spot, A)"
```

---

### Task 5: 판정·초안이 묶음 전체 변형을 보게 (`skill_draft` + `judge`)

**Files:**
- Modify: `a-mate/crates/core/src/skill_draft.rs`
- Modify: `a-mate/crates/core/src/judge.rs`
- Modify: `a-mate/crates/core/src/store.rs` (고아가 된 단수 쿼리 제거)
- Test: `judge.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `pub fn gather_context_multi(store: &SqliteStore, host: &str, representative: &str, norms: &[String]) -> Result<DraftContext>`.
- Consumes: `store.prompt_sessions_for_norms` (Task 2), evidence의 `member_norms` (Task 3/4).
- `gather_context(store, host, representative)`는 시그니처 불변(3개 호출부 무영향) — 내부적으로 `gather_context_multi`에 위임.

- [ ] **Step 1: 실패하는 테스트 작성**

`judge.rs`의 `#[cfg(test)] mod tests` 안에 추가:

```rust
    #[test]
    fn r6_judge_gathers_across_member_norms() {
        use crate::model::{EventKind, NormalizedEvent};
        use crate::rules::r6_repeated_prompts::normalize;
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sess: &str, text: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-0")), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0, msg_id: None,
            kind: EventKind::UserPrompt { preview: text.into() },
        };
        store.upsert_events(&[
            mk("s1", "PR 리뷰 코멘트 종합 검토해서 조치해줘"),
            mk("s2", "PR 리뷰 코멘트 종합 검토하고 반영해줘"),
        ]).unwrap();
        let member_norms = vec![
            normalize("PR 리뷰 코멘트 종합 검토해서 조치해줘").unwrap(),
            normalize("PR 리뷰 코멘트 종합 검토하고 반영해줘").unwrap(),
        ];
        let c = PendingCandidate {
            dedup_key: "R6|Windows|deadbeef".into(),
            scope_host: "Windows".into(),
            scope_project: None,
            evidence: serde_json::json!({
                "repeated_prompt": "PR 리뷰 코멘트 종합 검토해서 조치해줘",
                "member_norms": member_norms,
            }),
            prev_attempts: 0,
        };
        let (_system, user) = R6Judge.build_prompt(&store, &c).unwrap();
        // 묶음의 두 변형 모두 판정 재료(표본)에 들어가야 한다.
        assert!(user.contains("조치해줘"), "앵커 변형 포함");
        assert!(user.contains("반영해줘"), "다른 변형도 포함 — member_norms 전체 수집");
    }
```

- [ ] **Step 2: 실패 확인**

Run (from `a-mate/`): `cargo test r6_judge_gathers_across_member_norms`
Expected: FAIL — 현재 `build_prompt`는 `repeated_prompt` 하나(앵커 norm)만 gather하므로 user에 "반영해줘"가 없음.

- [ ] **Step 3: 구현 — `gather_context_multi` + 위임 + 단수 쿼리 제거 + judge 배선**

`skill_draft.rs`의 `gather_context`(약 29–56행)를 다음으로 교체:

```rust
/// R6 finding의 evidence(대표 프롬프트)로 세션을 되짚어 재료를 모은다(단일 norm).
/// 3개 호출부(judge·CLI·make-draft 커맨드) 호환용 — 내부적으로 gather_context_multi에 위임.
pub fn gather_context(
    store: &SqliteStore,
    host: &str,
    representative: &str,
) -> Result<DraftContext> {
    let Some(target) = normalize(representative) else {
        return Err(anyhow!("대표 프롬프트가 너무 짧아 초안 대상이 아닙니다"));
    };
    gather_context_multi(store, host, representative, &[target])
}

/// A — 느슨한 묶음의 **여러 변형(norm60)** 세션을 되짚어 재료를 모은다.
/// 세션·원문 중복 제거 후 표본 5개, 도구 상위 집계. representative는 표시용 대표(앵커).
pub fn gather_context_multi(
    store: &SqliteStore,
    host: &str,
    representative: &str,
    norms: &[String],
) -> Result<DraftContext> {
    let mut matched_ids: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for (sid, preview) in store.prompt_sessions_for_norms(host, norms)? {
        if !matched_ids.iter().any(|s| s == &sid) {
            matched_ids.push(sid);
        }
        let trimmed = preview.trim().to_string();
        if !trimmed.is_empty() && !samples.iter().any(|s| s == &trimmed) {
            samples.push(trimmed);
        }
    }
    samples.truncate(5);
    let top_tools = store.tool_usage_for_sessions(&matched_ids)?;
    Ok(DraftContext {
        representative: representative.trim().to_string(),
        session_count: matched_ids.len() as u64,
        sample_prompts: samples,
        top_tools,
    })
}
```

`store.rs`에서 이제 아무도 안 쓰는 단수 `prompt_sessions_for_norm`(약 1411–1422행)을 **제거**한다(고아 정리 — 본 변경이 유일 소비자를 없앰).

`judge.rs`의 `R6Judge::build_prompt`(약 153–158행)를 교체:

```rust
    fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)> {
        let rep = c.evidence.get("repeated_prompt").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("R6 evidence에 repeated_prompt 없음"))?;
        // A — 묶음의 모든 변형(member_norms)에서 재료 수집. 구버전 finding(member_norms 없음)은
        // 대표 하나로 폴백(하위호환).
        let norms: Vec<String> = c.evidence.get("member_norms")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let ctx = if norms.is_empty() {
            crate::skill_draft::gather_context(store, &c.scope_host, rep)?
        } else {
            crate::skill_draft::gather_context_multi(store, &c.scope_host, rep, &norms)?
        };
        Ok(judgment_prompt(&ctx))
    }
```

- [ ] **Step 4: 통과 확인 + 고아 정리 검증**

Run (from `a-mate/`): `cargo test r6_judge_gathers_across_member_norms`
Expected: PASS.

단수 쿼리 참조가 남지 않았는지 확인:
Run (from `a-mate/`): `grep -rn "prompt_sessions_for_norm\b" crates/ src-tauri/`
Expected: `prompt_sessions_for_norm`(단수) 참조 0건. (`prompt_sessions_for_norms` 복수만 존재.)

- [ ] **Step 5: 전체 회귀 + 커밋**

Run (from `a-mate/`): `cargo test`
Expected: 워크스페이스 전체 PASS(기존 `gather_collects_sessions_and_tools` 등 skill_draft 테스트 포함 — 위임이 동작 보존).

```bash
git add a-mate/crates/core/src/skill_draft.rs a-mate/crates/core/src/judge.rs a-mate/crates/core/src/store.rs
git commit -m "feat(agent): judge/draft R6 across clustered prompt variants (A)"
```

---

## DoD (Definition of Done)

- [ ] `cargo test` 워크스페이스 전체 그린 (네이티브 Windows — a-mate/CLAUDE.md).
- [ ] 임계값(`min_occurrences`, `cluster_threshold`)은 잠정(⚠ CALIBRATE)으로 남기고, Windows 실데이터 핀은 후속.
- [ ] 이 계획이 완료되면 같은 PR에서 `docs-archive` 스킬을 실행해 본 plan을 `docs/archive/` 미러로 이관(ADR 0013).
- [ ] PR 설명에 스펙 §7 후속 상태 업데이트: PR #93의 "segment_all 캐싱" 후속은 A가 세그먼터를 소비하지 않으므로 **불필요**로 종결. 남은 순서: E(마켓플레이스).

---

## Self-Review (작성자 체크)

- **스펙 커버리지**: §4 A의 두 확장(never-clear = Task 3, 완전일치만 = Task 1/4) 모두 태스크 있음. §2 불변 계약은 Global Constraints + 각 evidence(`est_tokens_saved=0`, 프롬프트만) 준수. 판정·rollup 재사용(§4 A "확장은 미더뿐") — Task 5는 gather 재료 확장만, verdict 스키마·rollup 불변.
- **플레이스홀더**: 없음 — 모든 스텝에 실제 코드/명령/기대출력.
- **타입 일관성**: `cluster_norms(&[String], f64) -> Vec<Vec<usize>>`(Task 1) = Task 4 호출부 일치. `PromptOccRow`/`prompt_occurrence_rows`/`prompt_sessions_for_norms`(Task 2) = Task 3/5 소비부 일치. `gather_context_multi(store, host, representative, norms)`(Task 5) = judge 호출부 일치. evidence 키 `member_norms`는 Task 3에서 생성, Task 5에서 소비.
- **회귀 영향**: 기존 R6 테스트 중 `r6_counts_session_once_despite_in_session_repeats`(Task 3에서 의도적 대체), `r6_fires_on_mid_session_repeats_across_sessions`(Task 4에서 노이즈 시드 수정) 2개만 변경. 나머지 불변.
