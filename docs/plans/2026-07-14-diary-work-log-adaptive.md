# 다이어리 work_log 작업량 적응 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `collect_work_log`의 Windows-우선정렬 축소 버그를 없애고, 커밋 제목 선정을 churn 비례로, 일기 길이를 커밋 수에 적응시킨다.

**Architecture:** 커밋 선택을 순수 함수 `balance_commits`로 분리(git/WSL 없이 단위 테스트)하고, `git_commits_for`가 numstat로 커밋별 churn을 반환하도록 확장한다. 일기 길이는 `diary_length` 밴드로 계산해 시스템 프롬프트에 주입한다. 모든 변경은 `crates/core/src/diary/mod.rs` 한 파일 안.

**Tech Stack:** Rust (edition 2021), rusqlite(bundled), 표준 `std::process::Command`로 git 호출. 테스트는 `tempfile` + 실제 git.

## Global Constraints

- 플랫폼: **Windows 전용**. macOS/Linux 분기 없음.
- 현재는 순수 백엔드 크레이트 단계 — 공개 함수는 나중에 `#[tauri::command]` 배선 가능하게 유지(시그니처 변경 OK, 단 pub 유지).
- 무거운 처리는 Rust 백엔드에서(본 변경 전부 해당).
- 상수(정확값): `WORK_LOG_TITLE_CAP = 12`, `WORK_LOG_TOPIC_CAP = 8`, `WORK_LOG_CHURN_CLAMP: u64 = 400`. 기존 `WORK_LOG_CAP = 8`은 제거.
- 일기 길이 밴드(커밋 수 C): `0..=3 → (400, "2~3")`, `4..=10 → (550, "3")`, `_ → (750, "4")`.
- 대상 파일: `crates/core/src/diary/mod.rs` (단일 파일).
- 설계 스펙: `docs/specs/2026-07-14-diary-work-log-adaptive-design.md`.

**빌드/테스트 환경 (이 머신은 비표준 — cargo 실행 전 매번 Git Bash에서):**
```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
```
이후 테스트는 `cargo test -p agent-mentor <필터>` 로 실행. `git push`가 revocation 에러면
`git -c http.schannelCheckRevoke=false push`.

---

## File Structure

- **Modify only:** `crates/core/src/diary/mod.rs`
  - 신규 순수 함수: `balance_commits`(커밋 선택), `parse_numstat_log`(numstat 파싱), `diary_length`(길이 밴드)
  - 시그니처 변경: `git_commits_for` (반환 `Vec<String>` → `Vec<(String, u64)>`), `build_system_prompt` (인자 `commit_count: usize` 추가)
  - 구조체 변경: `WorkLog`에 `commit_count: usize` 추가
  - 배선 변경: `collect_work_log` 커밋 수집부, `render_diary` 프롬프트 호출

세 태스크는 의존 순서다: Task 1(`balance_commits`) → Task 2(수집 경로, `balance_commits` 사용) → Task 3(길이, Task 2의 `commit_count` 필드 사용).

---

## Task 1: `balance_commits` 순수 선택 함수 + 상수

**Files:**
- Modify: `crates/core/src/diary/mod.rs` (상수 블록 근처 `crates/core/src/diary/mod.rs:48`, 함수는 `collect_work_log` 위 `crates/core/src/diary/mod.rs:470` 부근)
- Test: 동일 파일 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `fn balance_commits(groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<String>` — repo별 `(label, [(제목, raw churn)])`를 받아 floor + clamp된 churn 비례로 골라 제목 리스트(≤ 12) 반환. `const WORK_LOG_TITLE_CAP: usize = 12;`, `const WORK_LOG_CHURN_CLAMP: u64 = 400;`.

- [ ] **Step 1: 실패하는 테스트 작성**

`mod tests` 안에 추가:

```rust
#[test]
fn balance_commits_behaviors() {
    use super::balance_commits;

    // 빈 입력
    assert!(balance_commits(vec![]).is_empty());

    // floor 보장: churn 큰 A(12개) + churn 작은 B(1개), C=13 → B 실종 금지
    let a: Vec<(String, u64)> = (0..12).map(|i| (format!("a{i}"), 500)).collect();
    let b = vec![(String::from("bonly"), 5)];
    let out = balance_commits(vec![("A".into(), a), ("B".into(), b)]);
    assert!(out.contains(&"bonly".to_string()), "floor: B 커밋 실종 금지: {out:?}");
    assert!(out.len() <= 12);

    // churn 비례 (개수 역전): 둘 다 10개인데 A churn 800, B churn 20 → A 과반
    let a: Vec<(String, u64)> = (0..10).map(|i| (format!("a{i}"), 800)).collect();
    let b: Vec<(String, u64)> = (0..10).map(|i| (format!("b{i}"), 20)).collect();
    let out = balance_commits(vec![("A".into(), a), ("B".into(), b)]);
    let na = out.iter().filter(|s| s.starts_with('a')).count();
    let nb = out.iter().filter(|s| s.starts_with('b')).count();
    assert!(na >= 8, "churn 큰 A 과반: na={na} nb={nb}");
    assert!(nb >= 1, "B floor 보장");
    assert_eq!(out.len(), 12);

    // churn 우선 채택: 단일 repo 13개(cap 초과) → churn 최저 탈락
    let mut c: Vec<(String, u64)> = (0..12).map(|i| (format!("big{i}"), 100)).collect();
    c.push(("tiny".into(), 1));
    let out = balance_commits(vec![("A".into(), c)]);
    assert_eq!(out.len(), 12);
    assert!(!out.contains(&"tiny".to_string()), "churn 최저 탈락: {out:?}");

    // churn desc 순서 (cap 이내)
    let out = balance_commits(vec![(
        "A".into(),
        vec![("low".into(), 10), ("high".into(), 900), ("mid".into(), 100)],
    )]);
    assert_eq!(out, vec!["high".to_string(), "mid".to_string(), "low".to_string()]);

    // clamp: A(5개, 1개 5000+4개 10) vs B(10개 각 200), C=15 → B가 A보다 많음
    let mut a = vec![("lock".to_string(), 5000u64)];
    a.extend((0..4).map(|i| (format!("a{i}"), 10)));
    let b: Vec<(String, u64)> = (0..10).map(|i| (format!("b{i}"), 200)).collect();
    let out = balance_commits(vec![("A".into(), a), ("B".into(), b)]);
    let na = out.iter().filter(|s| *s == "lock" || s.starts_with('a')).count();
    let nb = out.iter().filter(|s| s.starts_with('b')).count();
    assert!(nb > na, "clamp: 저활동 A가 lockfile로 상위 불가 na={na} nb={nb}");

    // cap 이하 전부 포함
    let out = balance_commits(vec![
        ("A".into(), vec![("a0".into(), 1), ("a1".into(), 1)]),
        ("B".into(), vec![("b0".into(), 1)]),
        ("C".into(), vec![("c0".into(), 1)]),
    ]);
    assert_eq!(out.len(), 4);

    // 중복 제목 1회만
    let out = balance_commits(vec![
        ("A".into(), vec![("dup".into(), 100), ("a1".into(), 100)]),
        ("B".into(), vec![("dup".into(), 100), ("b1".into(), 100)]),
    ]);
    assert_eq!(out.iter().filter(|s| *s == "dup").count(), 1, "중복 1회: {out:?}");

    // repo 과다: churn 0 repo 20개 → cap개만
    let groups: Vec<(String, Vec<(String, u64)>)> =
        (0..20).map(|i| (format!("r{i:02}"), vec![(format!("c{i:02}"), 0)])).collect();
    assert_eq!(balance_commits(groups).len(), 12);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor balance_commits_behaviors`
Expected: FAIL — `cannot find function balance_commits` (컴파일 에러).

- [ ] **Step 3: 상수 + `balance_commits` 구현**

기존 상수 `crates/core/src/diary/mod.rs:48` `const WORK_LOG_CAP: usize = 8;` **아래에** 추가(WORK_LOG_CAP은 아직 유지):

```rust
const WORK_LOG_TITLE_CAP: usize = 12;   // work_log에 실을 커밋 제목 최대 개수
const WORK_LOG_CHURN_CLAMP: u64 = 400;  // 커밋당 churn 상한 (lockfile·생성물 인플레이션 방어)
```

`collect_work_log`(`crates/core/src/diary/mod.rs:471`) **바로 위에** 함수 추가. Task 2 전까지는
테스트에서만 쓰이므로 `#[allow(dead_code)]`를 단다(Task 2에서 제거):

```rust
/// repo별 그룹(각 (제목, raw churn))을 받아 clamp된 churn으로 floor + 비례 배분하고,
/// repo 내부는 clamp된 churn 내림차순으로 골라 평평한 제목 리스트(≤ TITLE_CAP)를 반환.
/// label은 churn 동률 시 결정론적 tiebreak용(host+cwd 등 안정 문자열).
#[allow(dead_code)] // Task 2에서 collect_work_log가 사용 → 제거
fn balance_commits(mut groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<String> {
    let n = groups.len();
    if n == 0 {
        return Vec::new();
    }
    let eff = |c: u64| c.min(WORK_LOG_CHURN_CLAMP);
    let counts: Vec<usize> = groups.iter().map(|(_, c)| c.len()).collect();
    // repo 가중치 = clamp된 churn 합(0 방지 위해 최소 1) — 배분 비례의 기준.
    let weight: Vec<u64> = groups
        .iter()
        .map(|(_, c)| c.iter().map(|(_, ch)| eff(*ch)).sum::<u64>().max(1))
        .collect();

    // repo 순서: churn 비중 큰 순 → 커밋수 desc → label asc (host-편향 없는 결정론).
    let order = {
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            weight[b]
                .cmp(&weight[a])
                .then(counts[b].cmp(&counts[a]))
                .then(groups[a].0.cmp(&groups[b].0))
        });
        idx
    };
    // repo 내부: clamp된 churn 내림차순(stable → 동률은 git 최신순 유지).
    for (_, cs) in &mut groups {
        cs.sort_by(|a, b| eff(b.1).cmp(&eff(a.1)));
    }

    // 슬롯 배분: floor 1개씩(cap 초과 시 order 앞쪽부터), 남은 슬롯은 D'Hondt(최고평균)로 churn 비례.
    let cap = WORK_LOG_TITLE_CAP;
    let mut quota = vec![0usize; n];
    let mut remaining = cap;
    for &i in &order {
        if remaining == 0 {
            break;
        }
        quota[i] = 1;
        remaining -= 1;
    }
    while remaining > 0 {
        let best = order
            .iter()
            .copied()
            .filter(|&i| quota[i] < counts[i])
            .max_by_key(|&i| weight[i] / (quota[i] as u64 + 1));
        match best {
            Some(i) => {
                quota[i] += 1;
                remaining -= 1;
            }
            None => break, // 커밋 총량 < cap
        }
    }

    // 채택: order 순으로 repo별 quota만큼 churn 순, 전역 중복 제목은 skip(슬롯 소비 안 함).
    let mut seen = std::collections::HashSet::new();
    let mut selected = Vec::new();
    for &i in &order {
        let mut take = quota[i];
        for (subj, _) in &groups[i].1 {
            if take == 0 {
                break;
            }
            if seen.insert(subj.clone()) {
                selected.push(subj.clone());
                take -= 1;
            }
        }
    }
    selected
}
```

> **주의 — `max_by_key` 동률**: Rust `Iterator::max_by_key`는 동률 시 **마지막** 요소를 반환한다.
> 여기선 `order`가 이미 weight 큰 순이라, 동률(같은 몫)이면 order 뒤쪽(weight 작은 repo)이 뽑힌다.
> 위 테스트는 이 동작으로 검증됨 — 바꾸지 말 것.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent-mentor balance_commits_behaviors`
Expected: PASS.

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): balance_commits — floor+churn 비례 커밋 선택(순수 함수)"
```

---

## Task 2: `git_commits_for` churn 반환 + `WorkLog.commit_count` + `collect_work_log` 배선

**Files:**
- Modify: `crates/core/src/diary/mod.rs`
  - `WorkLog` 구조체 `crates/core/src/diary/mod.rs:42`
  - `git_commits_for` `crates/core/src/diary/mod.rs:427`
  - `collect_work_log` 커밋 수집부 `crates/core/src/diary/mod.rs:499`~`533`
  - 상수 블록 `crates/core/src/diary/mod.rs:48`
  - 기존 테스트 `git_commits_for_reads_dated_authored_subjects` `crates/core/src/diary/mod.rs:1030`
- Test: 동일 파일 `mod tests`

**Interfaces:**
- Consumes: `balance_commits` (Task 1).
- Produces:
  - `fn git_commits_for(host: &str, cwd: &str, date: &str) -> Vec<(String, u64)>` (제목, raw churn)
  - `fn parse_numstat_log(out: &str) -> Vec<(String, u64)>`
  - `WorkLog { commits: Vec<String>, commit_count: usize, topics: Vec<String> }`
  - `const WORK_LOG_TOPIC_CAP: usize = 8;`

- [ ] **Step 1: `parse_numstat_log` 실패 테스트 작성**

```rust
#[test]
fn parse_numstat_log_sums_churn_per_commit() {
    let sample = "\u{1e}feat: a\n5\t2\tsrc/a.rs\n3\t0\tsrc/b.rs\n\n\u{1e}fix: b\n1\t1\tREADME.md\n";
    let out = super::parse_numstat_log(sample);
    assert_eq!(out, vec![("feat: a".to_string(), 10), ("fix: b".to_string(), 2)]);

    // 바이너리("-\t-") 는 0, 커밋 제목만 있고 변경 없으면 0
    let bin = "\u{1e}bin only\n-\t-\tlogo.png\n\u{1e}empty\n";
    assert_eq!(
        super::parse_numstat_log(bin),
        vec![("bin only".to_string(), 0), ("empty".to_string(), 0)]
    );
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor parse_numstat_log`
Expected: FAIL — `cannot find function parse_numstat_log`.

- [ ] **Step 3: `parse_numstat_log` 구현**

`git_commits_for`(`crates/core/src/diary/mod.rs:427`) **위에** 추가:

```rust
/// `git log --numstat --format=%x1e%s` 출력을 (제목, insertions+deletions) 목록으로 파싱.
/// `\x1e`로 시작하는 줄은 새 커밋 제목, 그 외 줄은 직전 커밋의 numstat("<add>\t<del>\t<path>").
fn parse_numstat_log(out: &str) -> Vec<(String, u64)> {
    let mut commits: Vec<(String, u64)> = Vec::new();
    for line in out.lines() {
        if let Some(subj) = line.strip_prefix('\u{1e}') {
            commits.push((subj.trim().to_string(), 0));
        } else if let Some(last) = commits.last_mut() {
            let mut it = line.split('\t');
            if let (Some(a), Some(d)) = (it.next(), it.next()) {
                last.1 += a.trim().parse::<u64>().unwrap_or(0) + d.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }
    commits.into_iter().filter(|(s, _)| !s.is_empty()).collect()
}
```

- [ ] **Step 4: `parse_numstat_log` 통과 확인**

Run: `cargo test -p agent-mentor parse_numstat_log`
Expected: PASS.

- [ ] **Step 5: `git_commits_for` churn 반환 — 실패 테스트 작성**

기존 테스트 `git_commits_for_reads_dated_authored_subjects`(`crates/core/src/diary/mod.rs:1030`)를
튜플 반환에 맞게 **교체**하고, churn>0 검증을 추가:

```rust
#[test]
fn git_commits_for_reads_dated_authored_subjects() {
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let run = |args: &[&str]| {
        Command::new("git").args(["-C", dir]).args(args)
            .env("GIT_AUTHOR_DATE", "2026-07-08T12:00:00")
            .env("GIT_COMMITTER_DATE", "2026-07-08T12:00:00")
            .output().unwrap()
    };
    Command::new("git").args(["init", "-q", dir]).output().unwrap();
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "t"]);
    // 3줄짜리 파일 추가 → churn = 3
    std::fs::write(tmp.path().join("f.txt"), "l1\nl2\nl3\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "feat: work_log 다이어리 반영"]);

    let subs = super::git_commits_for("Windows", dir, "2026-07-08");
    assert!(
        subs.iter().any(|(s, c)| s.contains("work_log 다이어리 반영") && *c == 3),
        "제목+churn: {subs:?}"
    );
    assert!(super::git_commits_for("Windows", dir, "2026-07-09").is_empty(), "다른 날짜엔 없음");
    let nogit = tempfile::tempdir().unwrap();
    assert!(super::git_commits_for("Windows", nogit.path().to_str().unwrap(), "2026-07-08").is_empty());
}
```

- [ ] **Step 6: 실패 확인**

Run: `cargo test -p agent-mentor git_commits_for_reads_dated_authored_subjects`
Expected: FAIL — 타입 불일치(`Vec<String>` vs 튜플 패턴) 컴파일 에러.

- [ ] **Step 7: `git_commits_for` 반환 타입 변경**

`crates/core/src/diary/mod.rs:427` 시그니처와 말미(`crates/core/src/diary/mod.rs:457`~`467`)를 교체.
시그니처: `-> Vec<String>` → `-> Vec<(String, u64)>`. args의 `"--format=%s"`를 `"--numstat"` +
`"--format=%x1e%s"`로, match 반환을 `parse_numstat_log`로:

```rust
fn git_commits_for(host: &str, cwd: &str, date: &str) -> Vec<(String, u64)> {
    // ... (상단 next/wsl_distro/run/email 로직 그대로) ...
    let mut args: Vec<String> = vec![
        "-C".into(), cwd.into(), "log".into(), "--no-merges".into(), "--numstat".into(),
        "--format=%x1e%s".into(),
        format!("--since={date} 00:00:00"), format!("--until={next} 00:00:00"),
    ];
    if !email.is_empty() {
        args.push(format!("--author={email}"));
    }
    match run(&args) {
        Some(out) => parse_numstat_log(&out),
        None => Vec::new(),
    }
}
```

- [ ] **Step 8: `git_commits_for` 통과 확인 (아직 collect_work_log는 깨진 상태)**

Run: `cargo test -p agent-mentor git_commits_for_reads_dated_authored_subjects`
Expected: 이 테스트는 PASS하지만 **`collect_work_log`가 타입 불일치로 컴파일 에러** → 다음 스텝에서 배선.
(전체 빌드는 아직 실패해도 됨; Step 9~10에서 해소.)

- [ ] **Step 9: `WorkLog`에 `commit_count` 추가 + 상수 + `collect_work_log` 배선**

(a) `WorkLog` 구조체(`crates/core/src/diary/mod.rs:42`) — 필드 추가 + 주석:

```rust
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkLog {
    pub commits: Vec<String>,   // churn 우선+repo 비례로 고른 커밋 제목(최대 TITLE_CAP)
    pub commit_count: usize,    // 그날 총 커밋 수(cap 전) — 일기 목표 길이 산정용
    pub topics: Vec<String>,    // 폴백/보조: 브랜치명·정제된 첫 프롬프트
}
```

(b) 상수(`crates/core/src/diary/mod.rs:48`) — `WORK_LOG_CAP` **제거**, `WORK_LOG_TOPIC_CAP` 추가.
Task 1의 `#[allow(dead_code)]`도 `balance_commits`에서 제거(이제 아래에서 사용):

```rust
const WORK_LOG_TITLE_CAP: usize = 12;   // work_log에 실을 커밋 제목 최대 개수
const WORK_LOG_TOPIC_CAP: usize = 8;    // topics 최대 개수 (기존 동작 유지)
const WORK_LOG_CHURN_CLAMP: u64 = 400;  // 커밋당 churn 상한
```

(c) `collect_work_log` 커밋 수집부(`crates/core/src/diary/mod.rs:499`~`533`) — `flat_map`+`truncate`
블록을 그룹핑+`balance_commits`로 교체. `host_cwds` 구성/정렬은 그대로 두고 그 다음부터:

```rust
    host_cwds.sort();
    host_cwds.dedup();
    let groups: Vec<(String, Vec<(String, u64)>)> = host_cwds
        .iter()
        .map(|(h, c)| (format!("{h}\u{0}{c}"), git_commits_for(h, c, date)))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let commit_count = groups.iter().map(|(_, v)| v.len()).sum();
    let commits = balance_commits(groups);

    // 토픽: 브랜치(main/master/HEAD 제외) + 정제된 첫 프롬프트 — 여러 갈래면 멀티태스킹 신호
    let mut topics: Vec<String> = Vec::new();
    for (_, _, _, br, fp) in &rows {
        if let Some(b) = br {
            if !matches!(b.as_str(), "main" | "master" | "HEAD" | "") {
                topics.push(b.clone());
            }
        }
        if let Some(p) = fp.as_deref().and_then(clean_prompt) {
            topics.push(p);
        }
    }
    topics.sort();
    topics.dedup();
    topics.truncate(WORK_LOG_TOPIC_CAP);

    WorkLog { commits, commit_count, topics }
}
```

- [ ] **Step 10: 전체 컴파일 + 회귀 테스트 통과 확인**

Run: `cargo test -p agent-mentor diary`
Expected: PASS. 특히 `collect_work_log_recovers_cwd_from_known_project_mapping`(commits는 여전히
`Vec<String>`, 1 repo·1 커밋 → floor 포함)와 `collect_work_log_falls_back_to_branch_and_prompt`
(commits 비어있고 topics 존재)가 무수정 통과해야 한다. `WorkLog::default()` 사용처(`crates/core/src/diary/mod.rs:796` 부근)는
`commit_count` 기본 0으로 자동 처리.

- [ ] **Step 11: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): git_commits_for churn 반환 + work_log 수집을 balance_commits로 배선"
```

---

## Task 3: 일기 길이 적응 — `diary_length` + `build_system_prompt` 주입

**Files:**
- Modify: `crates/core/src/diary/mod.rs`
  - `build_system_prompt` `crates/core/src/diary/mod.rs:602`
  - `render_diary` 호출부 `crates/core/src/diary/mod.rs:665`
  - `build_system_prompt` 테스트 호출부(~8곳): `crates/core/src/diary/mod.rs:824`, `1258`, `1287`, `1288`, `1294`, `1304`, `1314`, `1326`
- Test: 동일 파일 `mod tests`

**Interfaces:**
- Consumes: `WorkLog.commit_count` (Task 2).
- Produces:
  - `fn diary_length(commit_count: usize) -> (usize, &'static str)` — (목표 문자수, 문단 문구)
  - `pub fn build_system_prompt(cfg: &DiaryConfig, commit_count: usize) -> String`

- [ ] **Step 1: 실패 테스트 작성**

```rust
#[test]
fn diary_length_bands() {
    assert_eq!(super::diary_length(0), (400, "2~3"));
    assert_eq!(super::diary_length(3), (400, "2~3"));
    assert_eq!(super::diary_length(4), (550, "3"));
    assert_eq!(super::diary_length(10), (550, "3"));
    assert_eq!(super::diary_length(11), (750, "4"));
    assert_eq!(super::diary_length(999), (750, "4"));
}

#[test]
fn build_system_prompt_length_adapts_to_commit_count() {
    let cfg = DiaryConfig::default();
    assert!(super::build_system_prompt(&cfg, 2).contains("400자"), "가벼운 날 400자");
    assert!(super::build_system_prompt(&cfg, 7).contains("550자"), "보통 날 550자");
    assert!(super::build_system_prompt(&cfg, 20).contains("750자"), "바쁜 날 750자 상한");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor diary_length_bands build_system_prompt_length_adapts`
Expected: FAIL — `diary_length` 없음 + `build_system_prompt` 인자 개수 불일치(컴파일 에러).

- [ ] **Step 3: `diary_length` 추가 + `build_system_prompt` 시그니처·형식 줄 변경**

`build_system_prompt`(`crates/core/src/diary/mod.rs:602`) **위에** 추가:

```rust
/// 그날 총 커밋 수 → (일기 목표 문자 수, 문단 수 문구). 스펙 §2a 밴드.
fn diary_length(commit_count: usize) -> (usize, &'static str) {
    match commit_count {
        0..=3 => (400, "2~3"),
        4..=10 => (550, "3"),
        _ => (750, "4"),
    }
}
```

`build_system_prompt` 시그니처에 `commit_count: usize` 추가하고, 함수 첫 줄에서 밴드 계산 후
`format!`의 형식 줄(`crates/core/src/diary/mod.rs:642`)과 named args를 교체:

```rust
pub fn build_system_prompt(cfg: &DiaryConfig, commit_count: usize) -> String {
    let (target, paras) = diary_length(commit_count);
    format!(
        // ... (앞부분 그대로) ...
        // 아래 "형식:" 줄만 교체:
        //   기존: "형식: 일기는 2~4문단, 전체 500자 안팎으로 쓰세요 \
        //   신규:
        "형식: 일기는 {paras}문단 내외, 전체 {target}자 안팎으로 쓰세요 \
         (작업 내용을 담느라 한 문단 늘어도 좋지만 여전히 간결하게). \
         그날의 핵심을 골라 쓰고 덜 중요한 사실은 과감히 버리세요. \
         이모지는 문단마다 1~2개, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.",
        honorific = cfg.honorific,
        tone = cfg.tone,
        voice = voice_guidance(),
        target = target,
        paras = paras,
    )
}
```

> 나머지 format 본문(페르소나·정밀도의 선·occasions·recent_diaries·재료 안내)은 **그대로**. 위는
> 마지막 "형식:" 블록과 named args 추가만 보여준 것.

- [ ] **Step 4: `render_diary` 호출부 갱신**

`crates/core/src/diary/mod.rs:665`:

```rust
    let system = build_system_prompt(cfg, brief.work_log.commit_count);
```

- [ ] **Step 5: 테스트 호출부 갱신**

`build_system_prompt(&cfg)` / `build_system_prompt(&DiaryConfig::default())` 호출 ~8곳
(`crates/core/src/diary/mod.rs:824`, `1258`, `1287`, `1288`, `1294`, `1304`, `1314`, `1326`)에
커밋 수 인자를 추가한다. 내용(페르소나·work_log 언급 등)만 검증하는 테스트는 `0`을 넘긴다:
예) `build_system_prompt(&DiaryConfig::default(), 0)`.

이 중 기존 길이 문구("500자" 또는 "2~4문단")를 검증하는 assert가 있으면 밴드 문구로 갱신하거나
`diary_length`/새 테스트로 커버되므로 제거. (Step 6에서 grep로 확인.)

- [ ] **Step 6: 잔존 리터럴 확인**

Run: `git grep -n "500자\|2~4문단" crates/core/src/diary/mod.rs`
Expected: 결과 없음(소스 형식 줄·테스트에서 모두 제거됨). 남아 있으면 밴드 값으로 갱신.

- [ ] **Step 7: 전체 테스트 통과 확인**

Run: `cargo test -p agent-mentor diary`
Expected: PASS (신규 2개 + 기존 전부).

- [ ] **Step 8: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 일기 길이를 커밋 수 밴드로 적응(400/550/750)"
```

---

## Self-Review 결과

- **Spec coverage:** §2a 길이밴드 → Task 3. §2b floor+churn비례 → Task 1(`balance_commits`). §2c clamp/churn우선 → Task 1(`eff`, 내부 정렬) + Task 2(`git_commits_for` churn). §2d topics 무변경 → Task 2 Step 9(c). §3b `commit_count` → Task 2. §3c numstat → Task 2. §3d `balance_commits` → Task 1. §3e 배선 → Task 2. §3f 길이주입 → Task 3. §4 테스트 전부 태스크에 포함. 갭 없음.
- **Placeholder scan:** 코드 스텝 전부 완전한 코드. `// ...` 는 "기존 코드 그대로" 표시(생략이 아니라 무변경 지시)로, 변경 라인은 모두 명시됨.
- **Type consistency:** `balance_commits(Vec<(String, Vec<(String, u64)>)>) -> Vec<String>`, `git_commits_for(..) -> Vec<(String, u64)>`, `parse_numstat_log(&str) -> Vec<(String, u64)>`, `diary_length(usize) -> (usize, &'static str)`, `build_system_prompt(&DiaryConfig, usize) -> String`, `WorkLog{commits, commit_count, topics}` — 태스크 간 일치 확인.
