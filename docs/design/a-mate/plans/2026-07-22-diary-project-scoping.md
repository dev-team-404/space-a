# 다이어리 프로젝트 스코핑 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 일기 브리프의 `work_log`가 프로젝트 라벨을 버려 서로 다른 프로젝트 이야기가 섞이던 문제를, `work_log`를 프로젝트별 묶음으로 재구성해 해소한다.

**Architecture:** `crates/core` 순수 도메인 로직만 수정. `hosts.rs`에 (host, cwd)→정규화 프로젝트 키 매핑을 추가하고, `diary/mod.rs`의 `WorkLog`를 프로젝트별 구조(`Vec<ProjectWork>` + `concurrent` 플래그)로 바꾼 뒤, 시스템 프롬프트가 프로젝트 경계를 인식하도록 갱신한다. 저장 구조·UI·Tauri 셸·다른 rules는 건드리지 않는다.

**Tech Stack:** Rust, rusqlite(SQLite), chrono, serde. 테스트는 `cargo test` + in-memory SQLite + 임시 git repo.

**스펙:** `docs/design/a-mate/specs/2026-07-22-diary-project-scoping-design.md`

**작업 위치:** 워크트리 `D:\Project\space-a\.claude\worktrees\diary-project-scoping` (브랜치 `worktree-diary-project-scoping`). 아래 경로는 이 워크트리 루트 기준 상대경로.

## Global Constraints

- 플랫폼 Windows 전용. 빌드·테스트는 네이티브 Windows PowerShell에서 — **WSL 안에서 빌드 금지**.
- 변경은 **`crates/core`에 한정**. `src-tauri`·프론트엔드(`src/`)·DB 스키마 변경 없음.
- 하루 한 편(`{date}.md`)·`diary_index` PK `(date, scope)` 유지. `tool_usage`·`findings`·`totals`·`occasions`·`recent_diaries`는 하루 전체 합산 유지.
- 커밋 메시지는 **영어 Conventional Commits**. 각 커밋 끝에 `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- 테스트 명령(모든 Task 공통):
  `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib`
  (패키지 이름은 하이픈 `agent-mentor`, lib 이름은 언더스코어 `agent_mentor`.)
- Baseline: 이 계획 시작 시점 core 테스트 **361 passed, 0 failed**.

---

## Task 1: `hosts.rs` — 프로젝트 식별 순수 함수

같은 프로젝트의 WSL 직접 세션과 Windows(WSL UNC 경로) 세션을 하나의 키로 통합하는 순수 함수. `crates/core`의 경로 처리 담당(`hosts.rs`)에 위치.

**Files:**
- Modify: `a-mate/crates/core/src/hosts.rs` (구현 함수는 `wsl_path_to_unc` 뒤 ~111행 근처, 테스트는 `mod tests`에 추가)

**Interfaces:**
- Produces:
  - `pub fn path_basename(p: &str) -> String`
  - `pub fn unc_to_wsl_path(p: &str) -> Option<(String, String)>` — `(distro, linux_path)`
  - `pub fn project_identity(host: &str, cwd: &str) -> (String, String)` — `(canonical_key, display_name)`

- [ ] **Step 1: 실패하는 테스트 작성**

`hosts.rs`의 `#[cfg(test)] mod tests` 안(기존 `host_source_derives_sibling_paths` 뒤)에 추가:

```rust
    #[test]
    fn unc_to_wsl_path_reverses_wsl_path_to_unc() {
        assert_eq!(
            unc_to_wsl_path(r"\\wsl.localhost\Ubuntu-22.04\home\jay\proj"),
            Some(("Ubuntu-22.04".to_string(), "/home/jay/proj".to_string()))
        );
        assert_eq!(
            unc_to_wsl_path(r"\\wsl$\Debian\home\x"),
            Some(("Debian".to_string(), "/home/x".to_string()))
        );
        assert_eq!(unc_to_wsl_path(r"D:\work\proj"), None);
    }

    #[test]
    fn project_identity_unifies_wsl_direct_and_windows_unc() {
        let (k1, n1) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agent-meter");
        let (k2, n2) = project_identity(
            "Windows",
            r"\\wsl.localhost\Ubuntu-22.04\home\jayb\work\agent-meter",
        );
        assert_eq!(k1, k2, "같은 프로젝트는 같은 키로 통합");
        assert_eq!(n1, "agent-meter");
        assert_eq!(n2, "agent-meter");
    }

    #[test]
    fn project_identity_plain_windows_is_case_insensitive() {
        let (k1, n1) = project_identity("Windows", r"D:\Project\space-a");
        let (k2, _) = project_identity("Windows", r"d:\project\space-a");
        assert_eq!(k1, k2, "Windows 경로 키는 대소문자 무시");
        assert_eq!(n1, "space-a");
    }

    #[test]
    fn project_identity_distinct_projects_differ() {
        let (a, _) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agent-meter");
        let (b, _) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agenttoolbox");
        assert_ne!(a, b);
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib hosts::`
Expected: 컴파일 실패 — `cannot find function 'unc_to_wsl_path'` / `'project_identity'`.

- [ ] **Step 3: 구현 작성**

`hosts.rs`의 `wsl_path_to_unc` 함수 정의(~111행) 바로 뒤에 추가:

```rust
/// 경로의 마지막 세그먼트(basename). '/' 와 '\\' 둘 다 구분자로 취급.
pub fn path_basename(p: &str) -> String {
    p.rsplit(|c| c == '/' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or(p)
        .to_string()
}

/// WSL UNC 경로를 (distro, 리눅스 절대경로)로 되돌린다. `wsl_path_to_unc` 의 역.
/// `\\wsl.localhost\Ubuntu-22.04\home\jay\proj` → ("Ubuntu-22.04", "/home/jay/proj").
/// `\\wsl$\Debian\home\x` 도 지원. WSL UNC 가 아니면 None.
pub fn unc_to_wsl_path(p: &str) -> Option<(String, String)> {
    let rest = p
        .strip_prefix(r"\\wsl.localhost\")
        .or_else(|| p.strip_prefix(r"\\wsl$\"))?;
    let mut it = rest.splitn(2, '\\');
    let distro = it.next().filter(|s| !s.is_empty())?.to_string();
    let tail = it.next().unwrap_or("");
    Some((distro, format!("/{}", tail.replace('\\', "/"))))
}

/// (host, cwd) → (정규화 프로젝트 키, 표시 이름).
/// 같은 프로젝트의 WSL 직접 세션과 Windows(WSL UNC 경로) 세션을 같은 키로 통합한다.
/// - WSL 직접: host=`wsl:<distro>`, cwd=리눅스경로 → key=`wsl:<distro>:<linux>`
/// - Windows(WSL UNC): cwd=`\\wsl.localhost\<distro>\...` → 위와 동일 key 로 통합
/// - 일반 Windows: key=`win:<소문자 경로>` (리눅스 경로 키는 대소문자 유지)
pub fn project_identity(host: &str, cwd: &str) -> (String, String) {
    if let Some(distro) = host.strip_prefix("wsl:") {
        return (format!("wsl:{distro}:{cwd}"), path_basename(cwd));
    }
    if let Some((distro, linux)) = unc_to_wsl_path(cwd) {
        let name = path_basename(&linux);
        return (format!("wsl:{distro}:{linux}"), name);
    }
    (format!("win:{}", cwd.to_lowercase()), path_basename(cwd))
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib hosts::`
Expected: PASS (기존 hosts 테스트 + 신규 4개 모두 ok).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/hosts.rs
git commit -m "feat(agent): add project_identity for cross-host project unification

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `diary/mod.rs` — `work_log` 프로젝트별 재구성

`WorkLog`를 프로젝트별 묶음으로 바꾸고, `balance_commits` 반환형을 그룹별로 바꾸고, `collect_work_log`를 재작성한다. 구조체 필드가 바뀌면 `collect_work_log`·기존 테스트가 동시에 맞아야 컴파일되므로 한 커밋으로 묶는다.

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs`
  - `WorkLog` 구조체 (43-47행)
  - `use chrono::...` (8행)
  - `balance_commits` (535-618행)
  - `collect_work_log` (621-685행)
  - 기존 테스트 3개: `collect_work_log_falls_back_to_branch_and_prompt`(1138), `collect_work_log_recovers_cwd_from_known_project_mapping`(1160), `balance_commits_behaviors`(1240)

**Interfaces:**
- Consumes: `crate::hosts::project_identity` (Task 1)
- Produces:
  - `pub struct WorkLog { pub projects: Vec<ProjectWork>, pub commit_count: usize, pub concurrent: bool }`
  - `pub struct ProjectWork { pub name: String, pub commits: Vec<String>, pub topics: Vec<String> }`
  - `fn balance_commits(groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<(String, Vec<String>)>` — `(label, 선택된 제목들)`, 빈 그룹 제외
  - `fn collect_work_log(store: &SqliteStore, date: &str) -> WorkLog` (시그니처 동일, 반환 내용만 변경)

- [ ] **Step 1: 구조체 교체 + import 추가**

`diary/mod.rs` 8행:
```rust
use chrono::{Datelike, NaiveDate};
```
을
```rust
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate};
```
로 바꾼다.

43-47행의 기존 `WorkLog` 정의:
```rust
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkLog {
    pub commits: Vec<String>,   // churn 우선+repo 비례로 고른 커밋 제목(최대 TITLE_CAP)
    pub commit_count: usize,    // 그날 총 커밋 수(cap 전) — 일기 목표 길이 산정용
    pub topics: Vec<String>,    // 폴백/보조: 브랜치명·정제된 첫 프롬프트
}
```
를 다음으로 교체:
```rust
/// 그날 실제로 한 작업 — 프로젝트별로 묶어 LLM이 경계를 인식하게 한다.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkLog {
    pub projects: Vec<ProjectWork>, // 그날 활동한 프로젝트들, 첫 활동 시각순
    pub commit_count: usize,        // 그날 총 커밋 수(cap 전) — 일기 목표 길이 산정용
    pub concurrent: bool,           // 서로 다른 프로젝트 세션의 시간이 실제로 겹쳤나
}

/// 한 프로젝트의 그날 작업 소재.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectWork {
    pub name: String,         // 표시 이름 = cwd basename ("space-a", "agent-meter")
    pub commits: Vec<String>, // 이 프로젝트 커밋 제목(balance_commits 배분 몫)
    pub topics: Vec<String>,  // 이 프로젝트 브랜치·정제된 첫 프롬프트(폴백/보조)
}
```
(`WORK_LOG_TITLE_CAP`/`WORK_LOG_TOPIC_CAP`/`WORK_LOG_CHURN_CLAMP` 상수 49-51행은 그대로 둔다.)

- [ ] **Step 2: `balance_commits` 반환형 변경**

`balance_commits` 시그니처(535행)를
```rust
fn balance_commits(mut groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<(String, Vec<String>)> {
```
로 바꾼다. `n == 0` early-return, `eff`/`counts`/`weight`/`order`/내부 정렬/`quota` 배분 로직(536-600행)은 **그대로 둔다**. 마지막 채택 블록(602-617행)만 다음으로 교체:
```rust
    // 채택: order 순으로 repo별 quota만큼 churn 순, 전역 중복 제목은 skip(슬롯 소비 안 함).
    // 반환은 입력 그룹 순서대로 (label, 선택 제목들); 빈 그룹은 제외.
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, Vec<String>)> =
        groups.iter().map(|(label, _)| (label.clone(), Vec::new())).collect();
    for &i in &order {
        let mut take = quota[i];
        for (subj, _) in &groups[i].1 {
            if take == 0 {
                break;
            }
            if seen.insert(subj.clone()) {
                out[i].1.push(subj.clone());
                take -= 1;
            }
        }
    }
    out.into_iter().filter(|(_, v)| !v.is_empty()).collect()
}
```

- [ ] **Step 3: `collect_work_log` 재작성**

621-685행의 `collect_work_log` 함수 전체를 다음으로 교체:
```rust
/// 그날 실제 한 작업 — 프로젝트별로 묶은 커밋·토픽 + 동시 진행 여부.
/// 프로젝트 = 정규화 키(hosts::project_identity)로 통합 — 같은 프로젝트의 WSL 직접·
/// Windows(WSL UNC) 세션은 하나로 묶인다. 커밋·topic 둘 다 없는 프로젝트는 노이즈로 제외.
fn collect_work_log(store: &SqliteStore, date: &str) -> WorkLog {
    use crate::hosts::project_identity;
    use std::collections::HashMap;

    // 세션 단위 조회(시간 포함 — concurrent 판정·정렬용).
    let rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = store
        .conn
        .prepare(
            "SELECT host, project_id, cwd, git_branch, first_prompt_preview, first_ts, last_ts
             FROM sessions WHERE date(first_ts,'localtime')=?1",
        )
        .and_then(|mut s| {
            let r = s.query_map(params![date], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
            })?;
            r.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();

    // cwd 없는 옛 세션 보완: 같은 (host, project_id)를 cwd와 함께 기록한 다른 세션의 cwd 재사용.
    let known: HashMap<(String, String), String> = store
        .conn
        .prepare("SELECT host, project_id, cwd FROM sessions WHERE cwd IS NOT NULL")
        .and_then(|mut s| {
            let r = s.query_map([], |r| {
                Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?))
            })?;
            r.collect::<rusqlite::Result<HashMap<_, _>>>()
        })
        .unwrap_or_default();

    struct Accum {
        name: String,
        rep: Option<(String, String)>, // 커밋 수집 대표 (host, cwd) — WSL 직접 우선
        starts: Vec<DateTime<FixedOffset>>,
        spans: Vec<(DateTime<FixedOffset>, DateTime<FixedOffset>)>,
        topics: Vec<String>,
    }
    let mut projects: HashMap<String, Accum> = HashMap::new();
    let mut order: Vec<String> = Vec::new(); // key 최초 등장 순(안정 정렬 tiebreak)

    for (host, pid, cwd, branch, prompt, first_ts, last_ts) in &rows {
        let eff_cwd = cwd.clone().or_else(|| known.get(&(host.clone(), pid.clone())).cloned());
        let (key, name) = match &eff_cwd {
            Some(c) => project_identity(host, c),
            None => (format!("pid:{pid}"), pid.clone()),
        };
        let acc = projects.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Accum {
                name,
                rep: None,
                starts: Vec::new(),
                spans: Vec::new(),
                topics: Vec::new(),
            }
        });
        // 커밋 대표: WSL 직접(host=wsl:) 우선(리눅스 git 정확), 없으면 최초 값.
        if let Some(c) = &eff_cwd {
            let is_wsl = host.starts_with("wsl:");
            let replace = match &acc.rep {
                None => true,
                Some((h, _)) => is_wsl && !h.starts_with("wsl:"),
            };
            if replace {
                acc.rep = Some((host.clone(), c.clone()));
            }
        }
        // 시간(concurrent·정렬).
        if let Some(st) = first_ts.as_deref().and_then(|t| DateTime::parse_from_rfc3339(t).ok()) {
            let en = last_ts
                .as_deref()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .filter(|e| *e >= st)
                .unwrap_or(st);
            acc.starts.push(st);
            acc.spans.push((st, en));
        }
        // topics: 비-main 브랜치 + 정제된 첫 프롬프트.
        if let Some(b) = branch {
            if !matches!(b.as_str(), "main" | "master" | "HEAD" | "") {
                acc.topics.push(b.clone());
            }
        }
        if let Some(p) = prompt.as_deref().and_then(clean_prompt) {
            acc.topics.push(p);
        }
    }

    // 커밋 수집: 프로젝트별 대표 (host, cwd)로 — host별 git 실행 방식 분기(Windows/WSL)는 git_commits_for가 담당.
    let mut commit_groups: Vec<(String, Vec<(String, u64)>)> = Vec::new();
    for key in &order {
        if let Some((h, c)) = &projects[key].rep {
            let cs = git_commits_for(h, c, date);
            if !cs.is_empty() {
                commit_groups.push((key.clone(), cs));
            }
        }
    }
    let commit_count: usize = commit_groups.iter().map(|(_, v)| v.len()).sum();
    let mut commits_by_key: HashMap<String, Vec<String>> =
        balance_commits(commit_groups).into_iter().collect();

    // concurrent: 서로 다른 프로젝트 세션 구간이 겹치는가.
    let mut spans: Vec<(&String, DateTime<FixedOffset>, DateTime<FixedOffset>)> = Vec::new();
    for (key, acc) in &projects {
        for (st, en) in &acc.spans {
            spans.push((key, *st, *en));
        }
    }
    let mut concurrent = false;
    'outer: for i in 0..spans.len() {
        for j in (i + 1)..spans.len() {
            if spans[i].0 != spans[j].0 && spans[i].1 < spans[j].2 && spans[j].1 < spans[i].2 {
                concurrent = true;
                break 'outer;
            }
        }
    }

    // ProjectWork 조립 + 노이즈 필터 + 첫 활동 시각순 정렬.
    let mut works: Vec<(Option<DateTime<FixedOffset>>, ProjectWork)> = Vec::new();
    for key in &order {
        let acc = &projects[key];
        let commits = commits_by_key.remove(key).unwrap_or_default();
        let mut topics = acc.topics.clone();
        topics.sort();
        topics.dedup();
        topics.truncate(WORK_LOG_TOPIC_CAP);
        if commits.is_empty() && topics.is_empty() {
            continue; // 노이즈(temp/드라이브 루트/홈 등) 제외
        }
        let start = acc.starts.iter().min().copied();
        works.push((start, ProjectWork { name: acc.name.clone(), commits, topics }));
    }
    works.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    let projects_out = works.into_iter().map(|(_, w)| w).collect();

    WorkLog { projects: projects_out, commit_count, concurrent }
}
```

- [ ] **Step 4: 기존 테스트 3개 마이그레이션**

**(4a)** `collect_work_log_falls_back_to_branch_and_prompt`(1152-1157행)의 assert 블록을 교체:
```rust
        let wl = super::collect_work_log(&store, "2026-07-08");
        // cwd 없는 두 세션은 같은 project_id → 하나의 프로젝트로 묶임(폴백 키).
        assert_eq!(wl.projects.len(), 1, "cwd 없는 동일 project는 프로젝트 하나: {wl:?}");
        let p = &wl.projects[0];
        assert!(p.commits.is_empty(), "cwd 없어 git 커밋 없음");
        assert!(p.topics.contains(&"feat/mascot-daily-line".to_string()), "서술적 브랜치 포함");
        assert!(p.topics.contains(&"마스코트 한마디 구현".to_string()), "정제된 프롬프트 포함");
        assert!(!p.topics.contains(&"main".to_string()), "main 브랜치 제외");
        assert!(!p.topics.iter().any(|t| t.contains("task-notification")), "노이즈 프롬프트 제외");
```

**(4b)** `collect_work_log_recovers_cwd_from_known_project_mapping`(1190-1194행)의 assert 블록을 교체:
```rust
        let wl = super::collect_work_log(&store, "2026-07-05");
        let all_commits: Vec<&String> = wl.projects.iter().flat_map(|p| &p.commits).collect();
        assert!(
            all_commits.iter().any(|s| s.contains("옛 세션 repo 커밋")),
            "project 매핑으로 cwd 복원: {all_commits:?}"
        );
```

**(4c)** `balance_commits_behaviors`(1240행) — 반환형이 `Vec<(String, Vec<String>)>`로 바뀌었으므로, 테스트 첫 줄(`use super::balance_commits;`) 아래에 flatten 헬퍼를 추가하고, 함수 내 모든 `balance_commits(...)` 호출을 `flat(balance_commits(...))`로 감싼다. 헬퍼:
```rust
        use super::balance_commits;
        // 반환은 그룹별 (label, titles); 기존 검증은 평탄 리스트 기준 — 순서 무관 검증만 남는다.
        fn flat(g: Vec<(String, Vec<String>)>) -> Vec<String> {
            g.into_iter().flat_map(|(_, v)| v).collect()
        }
```
그리고 예:
- `assert!(balance_commits(vec![]).is_empty());` → `assert!(flat(balance_commits(vec![])).is_empty());`
- `let out = balance_commits(vec![("A".into(), a), ("B".into(), b)]);` → `let out = flat(balance_commits(vec![("A".into(), a), ("B".into(), b)]));`
- 나머지 5개 `balance_commits(...)` 호출도 동일하게 `flat(...)`으로 감싼다.

`assert_eq!(out, vec!["high".to_string(), "mid".to_string(), "low".to_string()]);`(1276행, 단일 그룹)은 flatten 후에도 그룹 내부 순서가 보존되므로 그대로 통과한다.

- [ ] **Step 5: 컴파일 + 기존 테스트 통과 확인**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib`
Expected: PASS (구조 변경·마이그레이션 후 컴파일 성공, 기존 테스트 전부 ok). 실패 시 컴파일 에러 메시지대로 잔여 `.commits`/`.topics` 직접 참조를 찾아 수정.

- [ ] **Step 6: 새 동작 테스트 작성**

`diary/mod.rs`의 `#[cfg(test)] mod tests` 안(`collect_work_log_recovers_cwd_from_known_project_mapping` 뒤)에 추가. 프로젝트 분리·통합·정렬·노이즈·concurrent는 `project_identity`가 cwd 문자열만 쓰므로 실제 디렉토리 없이 검증 가능(커밋 수집은 기존 테스트가 커버):

```rust
    // 헬퍼: 세션 한 건 삽입(cwd 있음). 커밋은 없지만 브랜치로 프로젝트가 생존.
    #[cfg(test)]
    fn seed_session(
        store: &SqliteStore,
        sid: &str,
        host: &str,
        pid: &str,
        cwd: &str,
        branch: &str,
        first_ts: &str,
        last_ts: &str,
    ) {
        store
            .conn
            .execute(
                "INSERT INTO sessions
                 (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd)
                 VALUES (?1,?2,?3,'claude-code',?4,?5,?6,?7)",
                rusqlite::params![sid, host, pid, first_ts, last_ts, branch, cwd],
            )
            .unwrap();
    }

    #[test]
    fn collect_work_log_splits_projects_and_sorts_by_first_activity() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 오후에 시작한 space-a, 오전에 시작한 agent-meter → 정렬은 agent-meter 먼저.
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T05:00:00Z", "2026-07-20T06:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/meter", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 2, "두 프로젝트로 분리: {wl:?}");
        assert_eq!(wl.projects[0].name, "agent-meter", "먼저 시작한 프로젝트가 앞");
        assert_eq!(wl.projects[1].name, "space-a");
    }

    #[test]
    fn collect_work_log_unifies_wsl_and_windows_unc_variants() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 같은 agent-meter를 WSL 직접 + Windows(WSL UNC)로 접근 → 한 프로젝트.
        seed_session(&store, "s1", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");
        seed_session(&store, "s2", "Windows", "pC",
            r"\\wsl.localhost\Ubuntu-22.04\home\jayb\work\agent-meter",
            "feat/b", "2026-07-20T03:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 1, "경로 변종은 한 프로젝트로 통합: {wl:?}");
        assert_eq!(wl.projects[0].name, "agent-meter");
    }

    #[test]
    fn collect_work_log_flags_concurrent_on_time_overlap() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 두 프로젝트의 구간이 겹침(01:00-03:00 vs 02:00-04:00).
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T03:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/b", "2026-07-20T02:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert!(wl.concurrent, "시간 겹치는 두 프로젝트 → concurrent");
    }

    #[test]
    fn collect_work_log_not_concurrent_when_sequential() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 순차(01:00-02:00, 03:00-04:00) — 겹치지 않음.
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/a", "2026-07-20T01:00:00Z", "2026-07-20T02:00:00Z");
        seed_session(&store, "s2", "wsl:Ubuntu-22.04", "pB", "/home/jayb/work/agent-meter",
            "feat/b", "2026-07-20T03:00:00Z", "2026-07-20T04:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert!(!wl.concurrent, "순차 진행 → not concurrent");
    }

    #[test]
    fn collect_work_log_filters_noise_projects() {
        let store = SqliteStore::open_in_memory().unwrap();
        // main 브랜치 + 노이즈 프롬프트 + cwd 있지만 git repo 아님 → 커밋·topic 모두 없음 → 제외.
        store.conn.execute(
            "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd, first_prompt_preview)
             VALUES ('n1','Windows','pN','claude-code','2026-07-20T01:00:00Z','2026-07-20T01:10:00Z','main',?1,'<task-notification>')",
            rusqlite::params![r"C:\Users\jibin\AppData\Local\Temp\noise"],
        ).unwrap();
        // 살아남는 프로젝트 하나(서술 브랜치).
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T02:00:00Z", "2026-07-20T03:00:00Z");

        let wl = super::collect_work_log(&store, "2026-07-20");
        assert_eq!(wl.projects.len(), 1, "노이즈 프로젝트 제외: {wl:?}");
        assert_eq!(wl.projects[0].name, "space-a");
    }

    #[test]
    fn brief_serializes_project_names() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "s1", "Windows", "pA", r"D:\Project\space-a",
            "feat/diary", "2026-07-20T02:00:00Z", "2026-07-20T03:00:00Z");
        let wl = super::collect_work_log(&store, "2026-07-20");
        let json = serde_json::to_string(&wl).unwrap();
        assert!(json.contains("\"projects\""), "work_log JSON에 projects: {json}");
        assert!(json.contains("space-a"), "프로젝트 이름 직렬화: {json}");
        assert!(json.contains("\"concurrent\""), "concurrent 플래그 직렬화");
    }
```

- [ ] **Step 7: 새 테스트 실행**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib collect_work_log`
그리고 `... --lib brief_serializes`
Expected: PASS (신규 6개 포함 모두 ok).

- [ ] **Step 8: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(agent): group diary work_log by project with concurrency flag

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `diary/mod.rs` — 시스템 프롬프트 프로젝트 인식

LLM이 프로젝트별 `work_log` 구조를 이해하고 프로젝트를 라벨 없이 섞지 않도록 지침을 갱신한다. 기존 철학("종류별 문단 나열 금지", "중심 줄기 하나")과 기존 프롬프트 테스트는 유지된다.

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` — `build_system_prompt` (790-797행 문구), `mod tests`에 새 테스트

**Interfaces:**
- Consumes: `WorkLog`/`ProjectWork`(Task 2). `build_system_prompt` 시그니처 불변.

- [ ] **Step 1: 실패하는 테스트 작성**

`diary/mod.rs`의 `mod tests`에 추가(프롬프트 테스트 그룹, `system_prompt_directs_context_signals_and_comfort` 뒤):
```rust
    #[test]
    fn system_prompt_directs_project_scoped_work_log() {
        let p = build_system_prompt(&DiaryConfig::default(), 5);
        assert!(p.contains("projects"), "프로젝트별 구조 언급");
        assert!(p.contains("어느 프로젝트"), "작업의 프로젝트 귀속 지시");
        assert!(p.contains("concurrent"), "동시 진행 지시");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib system_prompt_directs_project_scoped_work_log`
Expected: FAIL (assert `p.contains("projects")` 실패 — 현 프롬프트엔 없음).

- [ ] **Step 3: 프롬프트 문구 갱신**

`build_system_prompt`(790-796행)의 다음 블록:
```rust
         오늘 하루의 재료는 이렇습니다: `work_log`(그날 한 작업 — git 커밋 제목이나 작업 갈래), \
         `tool_usage`(도구 사용량), `work_context`(주말 여부·몰입 시간), `findings`(오늘 새 코칭거리), `occasions`. \
         이 재료들을 종류별로 문단을 나눠 나열하지 마세요 — '도구 문단 / 커밋 문단 / MCP 문단'처럼 쓰면 실패입니다. \
         그날을 가장 잘 말해주는 한 가지(대개 무슨 작업을 했는지)를 중심 줄기로 잡고, 나머지는 곁들이듯 흘려 \
         하나의 자연스러운 하루 이야기로 엮으세요. 모든 재료를 억지로 다 넣지 말고 골라 쓰세요. \
         특히 '몇 시간 붙어 있었다'처럼 작업 시간 수치로 일기를 시작하지 마세요. \
```
을 다음으로 교체(첫 줄 `work_log` 설명 확장 — 나머지 4줄은 그대로 유지):
```rust
         오늘 하루의 재료는 이렇습니다: `work_log`(그날 한 작업 — `projects` 배열로 프로젝트별 커밋 제목·작업 갈래, \
         `concurrent`는 여러 프로젝트를 동시에 진행했는지, `commit_count`는 총 커밋 수), \
         `tool_usage`(도구 사용량), `work_context`(주말 여부·몰입 시간), `findings`(오늘 새 코칭거리), `occasions`. \
         이 재료들을 종류별로 문단을 나눠 나열하지 마세요 — '도구 문단 / 커밋 문단 / MCP 문단'처럼 쓰면 실패입니다. \
         그날을 가장 잘 말해주는 한 가지(대개 무슨 작업을 했는지)를 중심 줄기로 잡고, 나머지는 곁들이듯 흘려 \
         하나의 자연스러운 하루 이야기로 엮으세요. 모든 재료를 억지로 다 넣지 말고 골라 쓰세요. \
         특히 '몇 시간 붙어 있었다'처럼 작업 시간 수치로 일기를 시작하지 마세요. \
```
이어서 797행의 `work_log` 지침 한 줄:
```rust
         `work_log`가 있으면 무슨 작업을 했는지 구체적으로(여러 갈래면 '여러 일을 오갔다'는 분주함도 슬쩍). \
```
을 다음으로 교체:
```rust
         `work_log.projects`가 있으면 무슨 작업을 했는지 구체적으로 쓰세요. 프로젝트가 여럿이면 \
         각 작업이 어느 프로젝트(`name`)에서 한 일인지 자연스럽게 드러내세요 — 라벨 없이 한 프로젝트 얘기에 \
         다른 프로젝트 작업을 섞으면 실패입니다. 단 프로젝트마다 문단을 딱딱 나누지는 말고 하루 흐름으로 엮으세요 \
         (예: '오전엔 space-a 다이어리를 손봤고, 오후엔 agent-meter 쪽으로 넘어갔다'). \
         `concurrent`가 true면 두 일을 동시에 오간 분주함도 슬쩍 담으세요('두 프로젝트를 왔다 갔다 하느라 정신없었네'). \
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib system_prompt`
Expected: PASS (신규 + 기존 프롬프트 테스트 `system_prompt_directs_context_signals_and_comfort`("나열하지"·"work_log")·`..._has_humor_..`·`..._short_length_..` 모두 ok).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(agent): teach diary prompt to keep project narratives distinct

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 전체 검증 및 마무리

**Files:** 없음(검증만). 필요 시 발견된 문제를 해당 Task 방식으로 수정.

- [ ] **Step 1: core 전체 테스트**

Run: `cargo test --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml" -p agent-mentor --lib`
Expected: PASS. 기존 361 + 신규(hosts 4 + collect 6 + prompt 1 = 11) ≈ 372 passed, 0 failed.

- [ ] **Step 2: 워크스페이스 컴파일 확인(Tauri 셸 포함)**

`WorkLog` 필드 변경이 `src-tauri`(assemble_brief/render_diary 호출부)를 깨뜨리지 않는지 확인:
Run: `cargo build --manifest-path "D:\Project\space-a\.claude\worktrees\diary-project-scoping\a-mate\Cargo.toml"`
Expected: 성공(에러 0). 만약 `src-tauri`가 `WorkLog.commits`/`.topics`를 직접 참조해 깨지면(현재 조사상 없음), 그 호출부를 `projects` 기반으로 최소 수정 후 재확인.

- [ ] **Step 3: 실 데이터 스모크 확인(선택, 비파괴)**

앱 DB를 읽기 전용으로 확인해 오늘 세션이 프로젝트별로 정규화되는지 육안 검증(쓰기 없음):
Run:
```bash
python "C:\Users\jibin\AppData\Local\Temp\claude\D--Project-space-a\14def659-665f-46cc-a9fc-2fc302399fa3\scratchpad\diary_concurrency_probe.py"
```
Expected: 멀티프로젝트/동시작업 날 분포 재확인(구현 검증용 참고 지표 — 통과/실패 게이트 아님).

- [ ] **Step 4: DoD 아카이브**

구현이 머지되는 PR에서 `docs-archive` 스킬로 이 spec/plan을 `docs/archive/` 미러로 이동(ADR 0013). 별도 커밋.

---

## Self-Review (작성자 체크)

**1. 스펙 커버리지**
- 데이터 구조(WorkLog/ProjectWork) → Task 2 Step 1 ✅
- 프로젝트 식별/WSL 통합/이름/대소문자 → Task 1 (`project_identity`) ✅
- 수집 로직(그룹핑·대표 repo·노이즈 필터·정렬·concurrent) → Task 2 Step 3 ✅
- 프롬프트 갱신 → Task 3 ✅
- 범위 밖(tool_usage/findings/totals/저장/UI/idle) → 어느 Task도 건드리지 않음 ✅
- 테스트 계획(분리·통합·concurrent·노이즈·직렬화·프롬프트·회귀) → Task 1/2/3 테스트로 매핑 ✅

**2. Placeholder 스캔:** TBD/TODO 없음. 모든 코드 스텝에 완전한 코드 포함.

**3. 타입 일관성:** `WorkLog{projects,commit_count,concurrent}`·`ProjectWork{name,commits,topics}`·`balance_commits(...)->Vec<(String,Vec<String>)>`·`project_identity(&str,&str)->(String,String)`·`unc_to_wsl_path(&str)->Option<(String,String)>` — Task 간 시그니처 일치 확인.

**주의(구현자 참고):**
- Task 2 Step 3에서 `balance_commits` 반환의 순서는 입력 그룹 순서다. 최종 `projects` 순서는 첫 활동 시각으로 별도 정렬하므로 balance 순서에 의존하지 않는다.
- `collect_work_log`는 이제 세션 단위(비-DISTINCT) 조회다. 커밋 수집 대표(host,cwd)는 프로젝트당 하나만 골라 `git_commits_for`를 호출하므로 중복 실행이 없다.
