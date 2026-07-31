# X1 첫 로딩 지연(store 락 청킹) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 초기 스캔이 store 락을 통짜로 쥐어 시작 시 UI가 최대 16.7초 정지하는 문제를, 락을 단계·파일 단위로 쪼개 없앤다.

**Architecture:** `run_pipeline_once`의 단일 락 블록을 여러 블록으로 분해한다. `discover`는 DB를 쓰지 않으므로 락 밖으로 내고, 파일 수집은 파일마다 락을 잡고 놓으며, `rollup`·`inventory`·`rules`는 각자 블록을 갖는다. `before`/`run_rules`/`after`/`diff`는 한 블록으로 묶어 diff 불변식을 국소화한다. 최장 연속 보유는 `run_rules` 블록(≈510ms)이 된다.

**Tech Stack:** Rust · Tauri v2 · rusqlite · `std::sync::Mutex` · Windows 전용

**설계 스펙:** [2026-07-31-startup-lock-contention-design.md](../specs/2026-07-31-startup-lock-contention-design.md)

## Global Constraints

- 플랫폼 **Windows 전용**. 빌드·실행은 네이티브 Windows PowerShell에서 — **WSL 금지**.
- 커밋은 **Conventional Commits · 영어 · scope `agent`**. 본문 끝에 `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- 회귀 기준값(main `1b6cc7e` 실측): 프론트 `npm test` = **224 passed**(svelte-check 0 errors), `cargo test` = **596 + 47**. 이 값이 줄면 회귀다.
- `cargo test`는 **경고 0**을 유지한다(`80e5aae`에서 정리했다).
- `crates/core`는 **Tauri 비의존**. `tauri::` 심볼을 절대 넣지 않는다.
- `npm run tauri dev`는 **포트 1420**을 쓴다. 앱이 떠 있으면 기동 실패하므로 실행 전 종료를 확인한다.
- 성능 작업이므로 **측정값을 커밋 메시지와 문서에 남긴다**. "빨라졌다"는 서술만으론 검증 불가.
- 계측 실행은 실제 앱 데이터 디렉터리를 **복사**해 `AGENT_MENTOR_DATA_DIR`로 격리한다. 실제 DB(30.7MB)에 쓰지 않는다.

## 스펙 대비 정정 2건 *(계획 작성 중 코드 확인으로 드러남)*

1. **스펙 §B가 `ingest_file`을 "기존 private"이라 했으나 이미 `pub`이다** — `crates/core/src/store.rs:2201`,
   `crates/core/src/lib.rs:24`가 `pub mod store`. 따라서 `ops::ingest_one` 래퍼를 **만들지 않는다**
   (YAGNI). `src-tauri`는 `agent_mentor::store::ingest_file`을 직접 부른다. 신규 공개 API는
   `discover_work`·`HostWork` 둘뿐이다.
2. **스펙 §G의 "ops.rs ingest 내부 분리 로그 영구 유지"를 철회한다** — Task 2 이후 앱은
   `run_ingest_with_progress`를 더 이상 호출하지 않는다(CLI `main.rs` 전용이 된다). 그 안의 로그는 앱에서
   영원히 안 찍히므로 제거하고, **단계 telemetry는 `pipeline.rs`가 단독 소유**한다.

## File Structure

| 파일 | 책임 | 변경 |
|---|---|---|
| `crates/core/src/ops.rs` | 상위 오퍼레이션. discover를 락 없이 실행 가능한 단위로 노출하고, CLI용 `run_ingest*`를 그 위에 재구현 | 수정 |
| `src-tauri/src/pipeline.rs` | 스캔 오케스트레이션 — **락 경계와 단계 telemetry의 유일한 소유자** | 수정 |
| `src-tauri/src/commands.rs` | 커맨드. 임시 계측 정리 | 수정 |
| `src-tauri/src/lib.rs` | `setup`. 임시 계측 제거 | 수정 |
| `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` | X1 완료 표시 + 전/후 수치 | 수정 |

---

## Task 1: ops.rs — discover를 락 밖으로 낼 수 있게 노출

`discover`는 파일시스템만 읽고 DB를 쓰지 않는다(실측 97ms). 지금은 `run_ingest_with_progress` 안에 갇혀
있어 호출자가 락 밖에서 실행할 수 없다. 이걸 꺼내는 것이 청킹의 전제다.

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` (`1`–`78` 영역, 기존 테스트 `ingest_all_reports_monotonic_progress`)

**Interfaces:**
- Consumes: `crate::store::ingest_file(store: &SqliteStore, adapter: &dyn SourceAdapter, file: &Path) -> Result<usize>` (기존, 이미 `pub`)
- Produces:
  - `pub struct HostWork { pub adapter: crate::adapter::ClaudeCodeAdapter, pub files: Vec<std::path::PathBuf> }`
  - `pub fn discover_work() -> (Vec<HostWork>, Vec<String>)` — `(호스트별 작업, 열거 경고)`
  - `fn ingest_all(store: &SqliteStore, work: &[HostWork], report: &mut IngestReport, on_progress: &mut dyn FnMut(usize, usize))` — 시그니처의 `work` 타입이 `&[(ClaudeCodeAdapter, Vec<PathBuf>)]`에서 `&[HostWork]`로 바뀐다
  - `pub fn run_ingest_with_progress` / `run_ingest` — **시그니처·동작 불변**

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`pipeline.rs`가 의존할 성질은 "파일마다 따로 `ingest_file`을 불러도 `ingest_all`과 같은 결과가 나온다"다.
`ops.rs`의 `mod tests` 안(기존 `ingest_all_reports_monotonic_progress` 바로 뒤)에 추가한다:

```rust
    #[test]
    fn per_file_ingest_matches_ingest_all() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let mut files = Vec::new();
        for (name, sid) in [("a.jsonl", "s1"), ("b.jsonl", "s2"), ("c.jsonl", "s3")] {
            let file = proj.join(name);
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"{sid}","uuid":"{sid}-u","timestamp":"2026-07-01T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":2}}}}}}"#).unwrap();
            files.push(file);
        }

        // A: ingest_all 한 번 (기존 경로)
        let bulk = SqliteStore::open_in_memory().unwrap();
        let work = vec![HostWork {
            adapter: ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() },
            files: files.clone(),
        }];
        let mut report = IngestReport { files: files.len(), new_events: 0, warnings: Vec::new() };
        ingest_all(&bulk, &work, &mut report, &mut |_, _| {});

        // B: 파일마다 따로 (pipeline.rs가 쓸 경로 — 락을 파일 단위로 놓는다)
        let chunked = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let mut chunked_events = 0usize;
        for f in &files {
            chunked_events += crate::store::ingest_file(&chunked, &adapter, f).unwrap();
        }

        // 이벤트 수가 같다
        assert_eq!(report.new_events, chunked_events, "파일 단위 수집이 일괄 수집과 같은 이벤트 수를 낸다");
        assert_eq!(chunked_events, 3);

        // ingest_state(파일별 offset)가 같다 — 재개 지점이 어긋나면 다음 스캔이 중복/누락한다
        let offsets = |s: &SqliteStore| -> Vec<(String, i64)> {
            let mut stmt = s.conn
                .prepare("SELECT source_file, last_offset FROM ingest_state ORDER BY source_file")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(offsets(&bulk), offsets(&chunked), "ingest_state 재개 지점이 같다");
        assert_eq!(offsets(&chunked).len(), 3);
    }
```

기존 테스트 `ingest_all_reports_monotonic_progress`도 새 타입에 맞춰 고친다 —
`a-mate/crates/core/src/ops.rs`의 `let work = vec![(adapter, files)];` 한 줄을 바꾼다:

```rust
        let work = vec![HostWork { adapter, files }];
```

- [ ] **Step 2: 테스트가 실패하는 것을 확인한다**

Run: `cd a-mate; cargo test -p agent-mentor per_file_ingest_matches_ingest_all`
Expected: 컴파일 실패 — `cannot find struct, variant or union type 'HostWork' in this scope`

- [ ] **Step 3: `HostWork`·`discover_work`를 만들고 `run_ingest_with_progress`를 그 위에 재구현한다**

`a-mate/crates/core/src/ops.rs`의 `IngestReport` 정의 직후부터 `ingest_all` 끝까지(현재 `13`–`78`)를 아래로 교체한다.
**임시 X1 계측 로그는 함께 제거한다**(위 "스펙 대비 정정" 2번 — telemetry는 `pipeline.rs`로 옮겨간다):

```rust
pub struct IngestReport {
    pub files: usize,
    pub new_events: usize,
    pub warnings: Vec<String>,
}

/// 한 호스트의 수집 대상. `discover_work`가 만들고 호출자가 소비한다.
pub struct HostWork {
    pub adapter: crate::adapter::ClaudeCodeAdapter,
    pub files: Vec<std::path::PathBuf>,
}

/// 전 호스트 discover. **DB를 쓰지 않으므로 store 락 밖에서 호출할 수 있다** —
/// Tauri 파이프라인이 초기 스캔의 락 보유를 줄이려고 이 성질에 의존한다(X1).
/// 반환: (호스트별 작업 목록, 열거 실패 경고). 한 호스트가 실패해도 나머지는 진행한다.
pub fn discover_work() -> (Vec<HostWork>, Vec<String>) {
    let mut work = Vec::new();
    let mut warnings = Vec::new();
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                warnings.push(format!("host {} 파일 열거 실패: {e}", hs.host));
                Vec::new()
            }
        };
        work.push(HostWork { adapter, files });
    }
    (work, warnings)
}

pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    run_ingest_with_progress(store, &mut |_, _| {})
}

/// 파일 단위 진행 콜백 `(done, total)`. CLI(`crates/core/src/main.rs`) 경로 — 호출자가 잡은 락을
/// 끝까지 유지한다. 락을 쪼개야 하는 Tauri 파이프라인은 `discover_work` + `store::ingest_file`을
/// 직접 조립한다(스펙 §A).
pub fn run_ingest_with_progress(
    store: &SqliteStore,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<IngestReport> {
    let (work, warnings) = discover_work();
    let mut report = IngestReport {
        files: work.iter().map(|w| w.files.len()).sum(),
        new_events: 0,
        warnings,
    };
    ingest_all(store, &work, &mut report, on_progress);
    store.rebuild_rollup()?;
    Ok(report)
}

fn ingest_all(
    store: &SqliteStore,
    work: &[HostWork],
    report: &mut IngestReport,
    on_progress: &mut dyn FnMut(usize, usize),
) {
    let total: usize = work.iter().map(|w| w.files.len()).sum();
    let mut done = 0;
    for w in work {
        for f in &w.files {
            match ingest_file(store, &w.adapter, f) {
                Ok(n) => report.new_events += n,
                Err(e) => report.warnings.push(format!("{} 수집 실패(건너뜀): {e}", f.display())),
            }
            done += 1;
            on_progress(done, total);
        }
    }
}
```

- [ ] **Step 4: 테스트가 통과하는 것을 확인한다**

Run: `cd a-mate; cargo test -p agent-mentor per_file_ingest_matches_ingest_all ingest_all_reports_monotonic_progress`
Expected: `test result: ok. 2 passed`

- [ ] **Step 5: 전체 테스트와 경고를 확인한다**

Run: `cd a-mate; cargo test 2>&1 | Select-String -Pattern "^test result:|^warning"`
Expected: `597 passed` / `47 passed`(신규 테스트 1건으로 596→597), **경고 줄 없음**

> `discover_work` 자체에는 헤르메틱 테스트가 없다 — `enumerate_hosts()`가 실제 머신의
> `~/.claude`를 읽기 때문이다. 이 부분은 변경 전에도 테스트가 없었고(기존 테스트는 `work`를 손으로
> 만들어 discover를 우회했다), 이 Task는 그 코드를 **이동만** 한다.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/crates/core/src/ops.rs
git commit -F- <<'EOF'
refactor(agent): let callers drive ingest per file, outside the store lock

discover reads only the filesystem (97ms measured) but was trapped inside
run_ingest_with_progress, so a caller could not run it without holding the
store lock. Expose it as discover_work returning HostWork, and rebuild
run_ingest_with_progress on top so the CLI path in crates/core/src/main.rs
keeps its signature and behaviour.

No ops::ingest_one wrapper: store::ingest_file is already pub, so the
Tauri side calls it directly. The temporary X1 breakdown log leaves with
this change — after the pipeline stops calling run_ingest_with_progress it
would only ever fire on the CLI path, and pipeline.rs takes over the
telemetry.

New test asserts the property the chunked pipeline relies on: ingesting
file by file yields the same event count and the same ingest_state offsets
as one bulk ingest_all. Mismatched offsets would make the next scan
double-count or skip.

cargo test 597 + 47, no warnings.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 2: pipeline.rs — 락을 단계·파일 단위로 쪼갠다

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs:66`–`115` (`run_pipeline_once`의 락 블록)

**Interfaces:**
- Consumes: `agent_mentor::ops::discover_work()`, `agent_mentor::store::ingest_file(...)` (Task 1), 기존 `agent_mentor::ops::run_inventory(&mut SqliteStore)`·`run_rules(&SqliteStore)`, 기존 `diff_findings(&HashMap<String,String>, &[(String,String)]) -> Vec<String>`
- Produces: 없음 (`run_pipeline_once(app: &AppHandle)` 시그니처 불변)

**단위 테스트 불가**: `pipeline.rs`의 `runtime` 모듈은 `#[cfg(not(test))]`이다(`pipeline.rs:8`). 검증은 Task 3의 실측이다.

**반드시 보존할 동작 5가지** — 리뷰 체크리스트:
1. `scan:progress`를 5건마다 + 마지막에 emit (`{done, total}`)
2. discover 경고와 파일별 수집 실패를 `log::warn!`로 남긴다
3. `last_scan_ts` 설정
4. 새로 뜬 finding이 있으면 `coach:finding` emit
5. 성공 시 `now`(RFC3339)를 반환 → 호출부가 `scan:done`에 쓴다

- [ ] **Step 1: 락 블록을 분해한다**

`a-mate/src-tauri/src/pipeline.rs`에서 `pub fn run_pipeline_once(app: &AppHandle) {`부터
`            Ok(now)\n        })();` 까지(현재 `66`–`115`)를 아래로 교체한다:

```rust
    /// 스캔 1회. 락을 **단계·파일 단위로** 잡는다 — 통짜로 쥐면 마이그레이션 후 전량 재수집
    /// (실측 16.7초) 동안 모든 커맨드가 막힌다(X1). 최장 연속 보유는 rules 블록이다.
    pub fn run_pipeline_once(app: &AppHandle) {
        let state = app.state::<AppState>();
        let scan_result = (|| -> anyhow::Result<String> {
            let scan_started = std::time::Instant::now();
            let mut longest_hold = Duration::ZERO;

            // ── 1) discover — DB를 쓰지 않으므로 락 밖 ──────────────────────────
            let (work, warnings) = agent_mentor::ops::discover_work();
            for w in &warnings {
                log::warn!("{w}");
            }
            let total: usize = work.iter().map(|w| w.files.len()).sum();

            // ── 2) 파일 수집 — 파일마다 락을 잡고 놓는다 ─────────────────────────
            let mut ingest_held = Duration::ZERO;
            let mut slowest_file = Duration::ZERO;
            let mut new_events = 0usize;
            let mut done = 0usize;
            for w in &work {
                for f in &w.files {
                    let (held, result) = {
                        let store = state
                            .store
                            .lock()
                            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                        let t = std::time::Instant::now();
                        let result = agent_mentor::store::ingest_file(&store, &w.adapter, f);
                        (t.elapsed(), result)
                    }; // guard drops here — 다음 파일 전에 대기 커맨드가 끼어들 수 있다
                    // std::sync::Mutex는 공정하지 않다. 해제 직후 이 스레드가 재획득하면
                    // 청킹이 무효가 되므로 대기자에게 넘길 틈을 준다. sleep은 쓰지 않는다 —
                    // Windows 타이머 해상도가 ~15ms라 파일당 sleep은 스캔에 수 초를 붙인다.
                    std::thread::yield_now();
                    match result {
                        Ok(n) => new_events += n,
                        Err(e) => log::warn!("{} 수집 실패(건너뜀): {e}", f.display()),
                    }
                    ingest_held += held;
                    if held > slowest_file {
                        slowest_file = held;
                    }
                    if held > longest_hold {
                        longest_hold = held;
                    }
                    done += 1;
                    // 파일 수천 개일 수 있어 5건 단위로만 emit (마지막은 항상)
                    if done == total || done % 5 == 0 {
                        let _ = app
                            .emit("scan:progress", serde_json::json!({"done": done, "total": total}));
                    }
                }
            }

            // ── 3) rollup ────────────────────────────────────────────────────
            let rollup_held = {
                let store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                store.rebuild_rollup()?;
                t.elapsed()
            };
            std::thread::yield_now();
            if rollup_held > longest_hold {
                longest_hold = rollup_held;
            }

            // ── 4) inventory ─────────────────────────────────────────────────
            let inventory_held = {
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                for w in agent_mentor::ops::run_inventory(&mut store)? {
                    log::warn!("{w}");
                }
                t.elapsed()
            };
            std::thread::yield_now();
            if inventory_held > longest_hold {
                longest_hold = inventory_held;
            }

            // ── 5) rules + diff + emit — 한 블록 ──────────────────────────────
            // before/run_rules/after/diff를 쪼개지 않는다. 지금은 쪼개도 안전하지만
            // (findings를 쓰는 커맨드는 set_finding_status 하나뿐이고 status만 바꾸며,
            // finding_severities는 status를 안 본다) 그 안전이 다른 파일의 사실 두 개에
            // 의존한다. 한 블록이면 불변식이 여기서 보인다 (설계 §A).
            let (now, rules_held) = {
                let store = state
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let t = std::time::Instant::now();
                let before: HashMap<String, String> =
                    store.finding_severities()?.into_iter().collect();
                agent_mentor::ops::run_rules(&store)?;
                let after = store.finding_severities()?;
                let fresh = diff_findings(&before, &after);
                let now = chrono::Utc::now().to_rfc3339();
                store.set_setting("last_scan_ts", &now)?;
                if !fresh.is_empty() {
                    let rows: Vec<_> = store
                        .list_findings_current(false)?
                        .into_iter()
                        .filter(|f| fresh.contains(&f.dedup_key))
                        .collect();
                    app.emit("coach:finding", &rows)?;
                }
                (now, t.elapsed())
            };
            if rules_held > longest_hold {
                longest_hold = rules_held;
            }

            // 성능 telemetry — 스캔당 1줄. X1의 회귀를 보는 유일한 창이므로 영구 유지한다.
            // 핵심 지표는 total이 아니라 **longest**다: 커맨드가 기다리는 시간이 그것이다.
            log::info!(
                "scan done — wall {}ms | ingest {}ms ({total} files, {new_events} events, slowest file {}ms) \
                 | rollup {}ms | inventory {}ms | rules {}ms | longest lock hold {}ms",
                scan_started.elapsed().as_millis(),
                ingest_held.as_millis(),
                slowest_file.as_millis(),
                rollup_held.as_millis(),
                inventory_held.as_millis(),
                rules_held.as_millis(),
                longest_hold.as_millis(),
            );
            Ok(now)
        })();
```

- [ ] **Step 2: 컴파일과 전체 테스트를 확인한다**

Run: `cd a-mate; cargo test 2>&1 | Select-String -Pattern "^test result:|^warning|^error"`
Expected: `597 passed` / `47 passed`, 경고·에러 없음

`Duration`·`HashMap`은 `pipeline.rs:18`–`19`에 이미 import되어 있으므로 새 `use`는 필요 없다.

- [ ] **Step 3: 보존 동작 5가지를 코드에서 눈으로 확인한다**

위 "반드시 보존할 동작 5가지" 목록을 새 코드와 하나씩 대조한다. 특히 `scan:progress`의
`done % 5 == 0` 조건과 `total` 계산이 그대로인지, `coach:finding`이 `fresh`가 비지 않을 때만
나가는지 확인한다.

- [ ] **Step 4: 커밋**

```bash
git add a-mate/src-tauri/src/pipeline.rs
git commit -F- <<'EOF'
perf(agent): chunk the scan lock so startup no longer freezes the UI

run_pipeline_once held the store mutex across the whole scan, and that
mutex is the only path every command takes to the database. On a full
re-ingest the hold measured 16,707ms and twenty-two command calls waited
up to 16,630ms — the settings tab, the home screen and the guestbook all
sat blank until the scan finished.

Split the single block:
- discover runs outside the lock (filesystem only, 97ms)
- each file takes and releases the lock, then yields
- rollup and inventory get their own blocks
- before/run_rules/after/diff stay in one block, so the diff invariant is
  visible locally instead of resting on facts in other files

yield_now after each release, not sleep: std::sync::Mutex is not fair and
the scan thread would otherwise re-acquire immediately, but a 1ms sleep
costs ~15ms per file on Windows timer granularity — seconds across 220
files.

Longest continuous hold is now the rules block. Replaces the temporary X1
probes in this file with one permanent telemetry line per scan; longest
hold is the number that matters, since that is what a command waits.

Behaviour preserved: scan:progress cadence, discover and per-file warnings,
last_scan_ts, coach:finding, and the returned timestamp for scan:done.

Not unit-testable — the runtime module is #[cfg(not(test))]. Verified by
measurement in the following commit.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 3: 실측 검증 (게이트)

이 Task가 §D의 barging 위험을 판정한다. **코드 변경 없이 끝날 수도 있고**, 게이트에 걸리면 폴백을 적용한다.

**Files:**
- Modify (게이트 실패 시에만): `a-mate/src-tauri/src/lib.rs`, `a-mate/src-tauri/src/commands.rs`

- [ ] **Step 1: 앱이 꺼져 있는지 확인한다**

Run:
```powershell
Get-Process -Name 'agent-mentor-app' -ErrorAction SilentlyContinue
if (Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue) { "1420 BUSY" } else { "1420 free" }
```
Expected: 프로세스 없음 + `1420 free`. 떠 있으면 **사용자에게 종료를 요청한다**(임의로 죽이지 않는다).

- [ ] **Step 2: 콜드 측정 — 빈 데이터 디렉터리**

```powershell
$cold = "$env:TEMP\x1-verify-cold"
if (Test-Path $cold) { Remove-Item $cold -Recurse -Force }
New-Item -ItemType Directory -Force $cold | Out-Null
$env:AGENT_MENTOR_DATA_DIR = $cold
cd a-mate; npm run tauri dev
```

로그에서 다음 두 줄을 찾는다(stdout과 `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log` 양쪽에 나온다):
- `scan done — wall … | longest lock hold …ms`
- `X1 cmd lock: waited …ms (caller …)` — Task 4에서 지울 임시 계측. 여기서 쓴다.

스캔이 끝나면 앱을 종료한다.

- [ ] **Step 3: 웜 측정 — 실제 데이터 디렉터리 복사본**

```powershell
$warm = "$env:TEMP\x1-verify-warm"
if (Test-Path $warm) { Remove-Item $warm -Recurse -Force }
New-Item -ItemType Directory -Force $warm | Out-Null
Copy-Item "$env:APPDATA\dev.agentmentor.app\*" $warm -Recurse -Force
$env:AGENT_MENTOR_DATA_DIR = $warm
cd a-mate; npm run tauri dev
```

- [ ] **Step 4: 게이트 판정**

| 항목 | 전 (실측) | 통과 기준 |
|---|---|---|
| 콜드 최장 연속 락 보유 | 16,707ms | **≤ 1,000ms** |
| 콜드 커맨드 최대 대기 | 16,630ms | **≤ 1,000ms** |
| 웜 커맨드 최대 대기 | 1,099ms | **≤ 1,000ms** |
| 스캔 벽시계 | 콜드 16,707 / 웜 1,512ms | 전값 **±10%** 이내 |
| 파일당 최대(`slowest file`) | 미측정 | 기록. `longest hold`를 지배하면 스펙 §C 재검토 |

**통과하면 Step 6으로 간다.** 어느 하나라도 초과하면 Step 5의 폴백을 적용하고 Step 2–4를 다시 돈다.

- [ ] **Step 5: *(게이트 실패 시에만)* 대기자 카운터 폴백**

`yield_now()`가 handoff를 못 만든 경우다. 대기자 수를 명시적으로 세어 스캔이 재획득을 미룬다.

`a-mate/src-tauri/src/lib.rs`의 `AppState`에 필드를 추가한다:

```rust
    /// 스토어 락을 기다리는 커맨드 수. 스캔이 배치 사이에 이 값을 보고 재획득을 미룬다 —
    /// std::sync::Mutex는 공정하지 않아 yield_now만으로는 해제 직후 재획득을 못 막는다(X1 §D).
    pub store_waiters: std::sync::atomic::AtomicUsize,
```

같은 파일 `setup`의 `app.manage(AppState { … })`에 초기값을 넣는다:

```rust
                    store_waiters: std::sync::atomic::AtomicUsize::new(0),
```

`a-mate/src-tauri/src/commands.rs`의 `lock()`이 카운터를 올리고 내리게 한다:

```rust
fn lock<'a>(state: &'a State<AppState>) -> Result<std::sync::MutexGuard<'a, SqliteStore>, String> {
    use std::sync::atomic::Ordering;
    state.store_waiters.fetch_add(1, Ordering::AcqRel);
    let guard = state.store.lock().map_err(|e| e.to_string());
    state.store_waiters.fetch_sub(1, Ordering::AcqRel);
    guard
}
```

`a-mate/src-tauri/src/pipeline.rs`에서 `std::thread::yield_now();` **3곳 전부**를 아래 호출로 바꾸고,
`runtime` 모듈 안에 헬퍼를 추가한다:

```rust
    /// 락을 놓은 뒤 대기 커맨드가 실제로 잡을 틈을 만든다. 상한을 두어 커맨드가 끊이지 않을 때
    /// (홈 탭 2초 폴링) 스캔이 굶지 않게 한다.
    fn hand_off(state: &AppState) {
        use std::sync::atomic::Ordering;
        for _ in 0..2_000 {
            if state.store_waiters.load(Ordering::Acquire) == 0 {
                break;
            }
            std::thread::yield_now();
        }
    }
```

호출은 `hand_off(&state);`로 바꾼다.

- [ ] **Step 6: 측정값을 기록하고 커밋한다**

Task 2 커밋이 "verified by measurement in the following commit"이라 약속했다. 이 커밋이 그 후속이다.
코드 변경이 없으면 빈 커밋 대신 **Task 4 커밋 메시지에 수치를 싣는다**(아래 Task 4 Step 4가 그렇게 되어 있다).
폴백을 적용했다면:

```bash
git add a-mate/src-tauri/src/lib.rs a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/pipeline.rs
git commit -F- <<'EOF'
perf(agent): hand the store lock to waiting commands explicitly

yield_now was not enough: after chunking, the cold command wait stayed at
<MEASURED>ms instead of dropping near the rules block. std::sync::Mutex is
not fair, and the scan thread kept winning the re-acquire.

Commands now count themselves while waiting, and the scan spins a bounded
number of yields between batches until the count reaches zero. The bound
keeps the home tab's 2s poll from starving the scan.

Cold command wait <BEFORE>ms -> <AFTER>ms.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 4: 임시 계측 정리

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`lock()`, `hub_client()`, `run_life_http()`)
- Modify: `a-mate/src-tauri/src/lib.rs` (`setup()` 시간축 로그 7줄)

- [ ] **Step 1: `commands.rs`의 커맨드별 락 로그를 되돌린다**

`lock()`을 main의 원형으로 되돌린다 — 단, Task 3에서 폴백을 적용했다면 카운터 두 줄은 **남긴다**.

폴백 미적용 시:
```rust
fn lock<'a>(state: &'a State<AppState>) -> Result<std::sync::MutexGuard<'a, SqliteStore>, String> {
    state.store.lock().map_err(|e| e.to_string())
}
```

`hub_client`에서 계측용 `#[track_caller]` 한 줄을 제거한다:
```rust
fn hub_client(state: &State<AppState>) -> Result<Option<LifeClient>, String> {
```

`run_life_http`의 무조건 로그를 제거해 기존 2초 경고만 남긴다:
```rust
    let elapsed = started.elapsed();
    if elapsed >= std::time::Duration::from_secs(2) {
        log::warn!("slow Life request: {operation} took {elapsed:?}");
    }
    result
```

- [ ] **Step 2: `lib.rs setup()`의 시간축 로그를 제거한다**

`X1 setup:`으로 시작하는 `log::info!` **7줄과 `let x1_t0 = …;` 1줄**을 지운다. 범위 밖으로 판정된
부분이다(스펙 §1.5). `setup`의 나머지 코드는 손대지 않는다.

Run: `cd a-mate; Select-String -Path src-tauri/src/*.rs,crates/core/src/*.rs -Pattern "X1"`
Expected: **출력 없음**

- [ ] **Step 3: 전체 검증**

Run: `cd a-mate; cargo test 2>&1 | Select-String -Pattern "^test result:|^warning"`
Expected: `597 passed` / `47 passed`, 경고 없음

Run: `cd a-mate; npm test`
Expected: svelte-check 0 errors + `224 passed`

Run: `cd a-mate; npm run build`
Expected: exit 0

- [ ] **Step 4: 커밋 — 측정값을 여기 싣는다**

`<…>`를 Task 3의 실측값으로 채운다.

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs
git commit -F- <<'EOF'
chore(agent): drop the temporary X1 probes and record the verified numbers

The per-command lock log, the unconditional hub round-trip log and the
setup timeline existed to answer "lock or hub?" and to verify the fix.
Both jobs are done, so they go; pipeline.rs keeps one telemetry line per
scan. run_life_http returns to warning only past 2s. The setup timeline
leaves entirely — setup measured +27ms warm and +380ms cold, so it was
never the bottleneck and is out of X1's scope.

Verified on this PC, 220 transcript files, isolated via
AGENT_MENTOR_DATA_DIR:

                          before      after
  cold longest hold       16,707ms    <…>ms
  cold command wait       16,630ms    <…>ms
  warm command wait        1,099ms    <…>ms
  scan wall clock (cold)  16,707ms    <…>ms
  slowest single file            -    <…>ms

The scan itself is no different in length — chunking redistributes the
hold, it does not make ingest faster. What changed is that the settings
tab, home screen and guestbook no longer wait for it.

cargo test 597 + 47, frontend 224, no warnings.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 5: DoD — 로드맵 갱신과 아카이브

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md`
- Move (docs-archive): 이 계획 + 설계 스펙 (+ 로드맵 자체 검토)

- [ ] **Step 1: 로드맵의 X1 항목에 완료 표시와 구현 결과를 쓴다**

`**X1 — 앱 시작 시 방명록 등 초기 로딩 지연 (store 락 경합)** (백로그 — 2026-07-27 검증 피드백)` 제목에
`— ✅ 완료(2026-07-31)`를 붙이고, 그 항목 끝에 **전/후 수치를 담은 구현 결과**를 추가한다. 다음을 반드시 담는다:

- 판정 = (a) 락 경합. hub HTTP 무죄(방명록 왕복 **51ms**)
- **콜드가 마이그레이션마다 재현된다는 발견** — `migrate()`의 재수집 트리거 9개가 `ingest_state`를 비운다.
  "PR 머지 후 설정탭 15초+" 증언과 `get_settings` 16,630ms 대기가 일치
- 3안 기각(11종 중 4종만 완화)·1안 단독 기각(쓰기 대기 잔존 + WAL 선행) 근거
- 채택 = 2안 청킹. `discover`를 락 밖으로, 파일 단위 락, rules 블록이 최장(≈510ms)
- 전/후 수치표(Task 4 커밋과 같은 값)
- **비목표**: 스캔 총량은 그대로 — 수집 자체를 빠르게 하는 건 별도 과제
- 정정: `setup`은 안 막힌다(웜 +27ms / 콜드 +380ms) → 범위 밖
- 스펙 대비 정정 2건(`ingest_file`은 이미 pub / ops 계측 로그는 pipeline이 인수)

**남은 항목** 절(로드맵 `451`행 근처)도 갱신한다 — X1이 끝나면 로드맵에 남은 항목이 없다.

- [ ] **Step 2: 로드맵 링크가 살아 있는지 확인한다**

Run: `Select-String -Path docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md -Pattern "startup-lock-contention"`
Expected: 스펙·계획을 가리키는 아카이브 경로가 보인다(Step 3 이후 경로 기준)

- [ ] **Step 3: `docs-archive` 스킬을 실행한다**

이 계획과 설계 스펙을 `docs/archive/` 미러로 옮긴다. **로드맵 자체도 아카이브 대상인지 판단한다** —
X1이 마지막 항목이라 로드맵이 완료된다(ADR 0013 규칙에 따름). 옮긴 뒤 로드맵 안의 상대 링크가
깨지지 않았는지 확인한다.

- [ ] **Step 4: 커밋**

```bash
git add docs/
git commit -F- <<'EOF'
docs(agent): close X1 and archive the roadmap it completes

X1 was the last open item on the life-social-diary follow-up roadmap.
Records the measured verdict, the migration finding that explains why the
16s freeze recurs on every release that carries one, why options 1 and 3
were rejected, and the before/after numbers.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Self-Review

**스펙 커버리지**

| 스펙 절 | Task |
|---|---|
| §A 목표 구조 (discover 락 밖 / 파일 단위 / rollup·inventory·rules 분리) | Task 2 Step 1 |
| §A `before` 스냅샷 이동 | Task 2 Step 1 (rules 블록) |
| §B ops.rs CLI 경로 불변 | Task 1 (+ 정정: `ingest_one` 불필요) |
| §C 배치 = 파일 1개 + 파일당 최대 기록 | Task 2 Step 1 (`slowest_file`), Task 3 Step 4 |
| §D barging — yield_now, sleep 금지, 폴백 게이트 | Task 2 Step 1, Task 3 Step 4–5 |
| §E 받아들이는 변화 | 코드 주석(Task 2 Step 1) + Task 5 Step 1 문서화 |
| §F 비목표 | Task 4 커밋 메시지, Task 5 Step 1 |
| §G 계측 처리 | Task 1 Step 3(ops 제거), Task 2 Step 1(영구 telemetry), Task 4 |
| §5 테스트 | Task 1 Step 1 |
| §6 검증 전/후 | Task 3 |
| §7 DoD | Task 5 |

**테스트 수 기대값**: Task 1이 테스트 1건을 추가하므로 `cargo test`는 **596 → 597**이 된다.
Task 2 이후 문서의 모든 기대값은 `597 + 47`이다. 프론트는 미접촉이라 **224** 유지.

**타입 일관성 확인**: `HostWork { adapter, files }`가 Task 1에서 정의되고 Task 2에서
`w.adapter`·`w.files`로만 쓰인다. `discover_work()`의 반환은 두 Task 모두 `(Vec<HostWork>, Vec<String>)`.
`ingest_file(store, adapter, file) -> Result<usize>`는 기존 시그니처 그대로다.
