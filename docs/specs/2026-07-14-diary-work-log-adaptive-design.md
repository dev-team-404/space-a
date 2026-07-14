# 다이어리 work_log 작업량 적응 설계 스펙

- 작성: 2026-07-14 (브레인스토밍 산출물)
- 배경: WSL 조사 중 발견한 `collect_work_log`(`crates/core/src/diary/mod.rs`)의 커밋 축소 버그 +
  일기 길이가 그날 작업량과 무관하게 고정. roadmap 항목 19 "다이어리 작업량 적응".
- 다음 단계: writing-plans → TDD 구현 → push+PR (base=main, 브랜치 `feat/diary-work-log-adaptive`)

## 1. 문제

### 1a. 커밋 축소 버그 (Windows 우선정렬)

`collect_work_log`의 커밋 수집부:

```rust
host_cwds.sort();                       // (host, cwd) 정렬
let mut commits = host_cwds.iter()
    .flat_map(|(h,c)| git_commits_for(h,c,date))   // repo별 커밋을 이어붙임
    .collect();
commits.dedup();                        // ⚠ 인접 중복만 제거
commits.truncate(WORK_LOG_CAP);         // ⚠ 앞에서 8개만 남김
```

host 문자열은 `"Windows"`(대문자 W=87) vs `"wsl:<distro>"`(소문자 w=119)라 Windows repo가 항상 앞에
정렬된다. `flat_map`이 repo별로 커밋을 이어붙인 뒤 `truncate(8)`이 앞 8개만 남기므로, 바쁜 날엔
앞쪽(Windows) repo가 8칸을 다 먹고 **뒤쪽(WSL) repo 작업이 일기에서 통째로 사라진다.**

### 1b. 일기 길이가 작업량 무관 고정

시스템 프롬프트가 "전체 500자 안팎"으로 고정 — 커밋 20개인 바쁜 날과 1개인 날의 일기 길이가
같다. "작업량 적응"의 핵심은 **커밋이 많은 날 일기도 비례해서 길어지되 상한을 두는 것**이다.

## 2. 결정 사항 (브레인스토밍 논점)

### 2a. 일기 길이 = 총 커밋 수 기반 밴드 (상한 있음)

그날 총 커밋 수 `C`(전 repo 합)로 목표 길이를 정한다. 기준은 **커밋 개수**(repo 수 아님).

| 총 커밋 C | 목표 길이 | 문단 |
|---|---|---|
| 0–3 | ~400자 | 2~3 |
| 4–10 | ~550자 | 3 |
| 11+ | ~750자 (상한) | 4 |

"중간" 프로파일 — 현재 500자에서 양방향으로 적당히 벌어진다. LLM은 문자 수를 정밀히 세지 못하므로
기존 "500자 안팎"과 동일한 **소프트 타깃**이다.

### 2b. 커밋 제목 선정 = floor + churn 비례 (repo 간)

work_log에 실을 제목은 최대 `WORK_LOG_TITLE_CAP`(=12)개. `C ≤ 12`면 전부 포함(→ 버그는 바쁜 날에만
관여). `C > 12`면:

- **floor**: 각 repo에서 1개씩 먼저 확보(모든 repo 대표 = WSL repo 실종 방지). repo 수 > cap이면
  churn 비중 큰 repo부터 cap개만 대표.
- **churn 비례**: 남은 슬롯을 repo별 **clamp된 churn 합**에 비례해 배분(변경량 큰 repo가 더 많은
  제목). 개수가 아니라 churn을 쓰는 이유: "큰 변경이 중요"라는 §2c 가치는 repo 경계에서도 동일하게
  성립한다 — 자잘한 커밋 8개 repo보다 대규모 리팩터 2개 repo가 그날의 실질 작업이다.

순수 라운드로빈(균등)은 기각 — repo 많은 날 floor가 슬롯을 다 먹어 바쁜 repo가 과소 대표. 순수
비례(floor 없음)도 기각 — 커밋 적은 repo가 사라져 원래 버그 부분 재발. 개수 비례도 기각 — 커밋
잘게 쪼개는 repo에 과다 배분(§2c와 모순).

### 2c. churn 우선 + 커밋당 clamp

repo 내부에서 어떤 제목을 고를지, 그리고 §2b의 repo 간 비중 모두 **변경량(churn) 우선**.
사소한 변경(오타·포맷)보다 큰 변경에 가중 — 그날의 실질 작업이 일기에 반영된다.

churn = `insertions + deletions`. 단, **raw churn은 인플레이션에 취약** — 생성 파일·`Cargo.lock`·
리포맷·대량 이동이 수천 라인을 만들어 repo 비중과 내부 1순위를 강탈할 수 있다(Rust 프로젝트라
`Cargo.lock` 위험 실재). 경로 제외 목록은 brittle해 기각하고 **커밋당 상한(clamp)**으로 방어:

```
effective_churn = min(insertions + deletions, WORK_LOG_CHURN_CLAMP)   // = 400
```

대규모 실제 커밋(~200–600라인)과 5000라인 lockfile이 둘 다 400으로 눌려 비슷해진다 — churn을
완전히 무시하진 않되(생성물도 작업이긴 하다) 지배하진 못하게. clamp된 값으로 정렬·합산한다.
**길이(§2a)는 개수 유지** — 길이는 "말할 거리 수"라 개수가 정확하고 인플레이션에도 강하다.

### 2d. topics는 무변경

topics(브랜치·정제 프롬프트)는 host-편향 버그가 없다(단순 알파벳 정렬). 기존 개수 상한(8)을 그대로
둔다 — 이번 작업 범위 밖. (상수명만 `WORK_LOG_TOPIC_CAP`으로 분리.)

## 3. 변경 상세 (`crates/core/src/diary/mod.rs`)

### 3a. 상수 교체

```rust
// 삭제: const WORK_LOG_CAP: usize = 8;
const WORK_LOG_TITLE_CAP: usize = 12;   // work_log에 실을 커밋 제목 최대 개수
const WORK_LOG_TOPIC_CAP: usize = 8;    // topics 최대 개수 (기존 동작 유지)
const WORK_LOG_CHURN_CLAMP: u64 = 400;  // 커밋당 churn 상한 (lockfile·생성물 인플레이션 방어)
```

### 3b. `WorkLog`에 `commit_count` 추가

일기 길이 산정을 위해 그날 총 커밋 수(cap 적용 전 원시 합)를 브리프에 싣는다.

```rust
pub struct WorkLog {
    pub commits: Vec<String>,   // churn 우선 + repo 비례로 고른 커밋 제목(최대 TITLE_CAP)
    pub commit_count: usize,    // 그날 총 커밋 수(cap 전) — 일기 목표 길이 산정용
    pub topics: Vec<String>,
}
```

`#[derive(Default)]` 유지 → `commit_count` 기본 0. Serialize에 포함되나 LLM엔 무해(재료 안내엔 미언급).

### 3c. `git_commits_for` — churn 반환

반환 타입을 `Vec<(String, u64)>`(제목, **raw** churn)로 확장. clamp는 선택 정책이라 여기선 원시값을
주고 `balance_commits`에서 clamp한다(테스트 용이). numstat로 커밋별 변경 라인 수집:

```rust
// git log --no-merges --numstat --format=%x1e%s --since --until [--author]
// 출력: 각 커밋마다  \x1e<subject>  다음 줄부터 numstat "<add>\t<del>\t<path>" ...
```

파싱: 줄 단위로, `\x1e`로 시작하면 새 커밋(subject = 나머지), 아니면 numstat 줄 →
`add + del` 누적(`"-"`(바이너리)은 0). best-effort — 실패 시 빈 벡터(상위에서 topics 폴백).

### 3d. 순수 선택 로직 — `balance_commits`

git/WSL 없이 결정론적 단위 테스트가 가능하도록 선택을 순수 함수로 분리.

```rust
/// repo별 그룹(각 (제목, raw churn))을 받아 clamp된 churn으로 floor + 비례 배분하고,
/// repo 내부는 clamp된 churn 내림차순으로 골라 평평한 제목 리스트(≤ TITLE_CAP)를 반환.
/// label은 churn 동률 시 결정론적 tiebreak용(host+cwd 등 안정 문자열).
fn balance_commits(mut groups: Vec<(String, Vec<(String, u64)>)>) -> Vec<String> {
    let n = groups.len();
    if n == 0 { return Vec::new(); }

    let eff = |c: u64| c.min(WORK_LOG_CHURN_CLAMP);
    let counts: Vec<usize> = groups.iter().map(|(_, c)| c.len()).collect();
    // repo 가중치 = clamp된 churn 합(0 방지 위해 최소 1) — 배분 비례의 기준.
    let weight: Vec<u64> =
        groups.iter().map(|(_, c)| c.iter().map(|(_, ch)| eff(*ch)).sum::<u64>().max(1)).collect();

    // repo: churn 비중 큰 순, 동률은 커밋수 desc → label asc (host-편향 없는 결정론).
    let order = {
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            weight[b].cmp(&weight[a])
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
        if remaining == 0 { break; }
        quota[i] = 1; remaining -= 1;
    }
    while remaining > 0 {
        // quota < counts 인 repo 중 weight/(quota+1) 최대, 동률은 order 앞쪽(=weight 큰 쪽).
        let best = order.iter().copied()
            .filter(|&i| quota[i] < counts[i])
            .max_by_key(|&i| weight[i] / (quota[i] as u64 + 1));
        match best {
            Some(i) => { quota[i] += 1; remaining -= 1; }
            None => break, // 커밋 총량 < cap: 더 채울 것 없음
        }
    }

    // 채택: order 순으로 repo별 quota만큼 churn 순, 전역 중복 제목은 skip(슬롯 소비 안 함).
    let mut seen = std::collections::HashSet::new();
    let mut selected = Vec::new();
    for &i in &order {
        let mut take = quota[i];
        for (subj, _) in &groups[i].1 {
            if take == 0 { break; }
            if seen.insert(subj.clone()) {
                selected.push(subj.clone());
                take -= 1;
            }
        }
    }
    selected
}
```

설계 노트:
- **host-편향 소멸**: 선택이 host 정렬순이 아니라 churn 가중(+floor)에 의존 → WSL repo 통째 실종 구조 제거.
- **churn 비례**: floor 후 D'Hondt(최고평균 `weight/(seats+1)`)로 남은 슬롯을 churn 비중에 비례 배분 —
  변경량 큰 repo가 더 많은 제목. `max_by_key`의 정수 나눗셈은 근사지만 휴리스틱엔 충분, 동률은 order로 결정론.
- **clamp**: `eff()`로 커밋당 churn을 400에 눌러 정렬·합산 → lockfile 한 방이 repo 비중·내부 1순위 강탈 불가.
- 잔여 슬롯 재분배 없음(단순성) — D'Hondt가 큰 repo부터 채워 낭비 거의 없음.

### 3e. `collect_work_log` 배선

커밋 수집부만 교체(cwd 복원·`host_cwds` 구성은 그대로):

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
// ... topics: 기존 sort/dedup 후 truncate(WORK_LOG_TOPIC_CAP) ...
WorkLog { commits, commit_count, topics }
```

### 3f. 일기 목표 길이 주입 — `build_system_prompt`

```rust
/// 총 커밋 수 → (목표 문자 수, 문단 수 문구). §2a 밴드.
fn diary_length(commit_count: usize) -> (usize, &'static str) {
    match commit_count {
        0..=3 => (400, "2~3"),
        4..=10 => (550, "3"),
        _ => (750, "4"),
    }
}

pub fn build_system_prompt(cfg: &DiaryConfig, commit_count: usize) -> String {
    let (target, paras) = diary_length(commit_count);
    // 형식 줄: "…{paras}문단 내외, 전체 {target}자 안팎으로 쓰세요…"
    ...
}
```

`render_diary`: `build_system_prompt(cfg, brief.work_log.commit_count)`.
`render_idle_diary`는 별도 `build_idle_prompt` 사용 → **무변경**(무활동일은 항상 짧게).

## 4. 테스트

### 4a. 신규 — `balance_commits` 단위 테스트 (git/WSL 불필요)

  (모든 배분 테스트는 `C > 12`라야 슬롯 경쟁이 생김 — `C ≤ 12`면 전부 포함되어 배분이 무의미.)
- **floor 보장**: churn 큰 repo A(커밋 12개) + churn 작은 repo B(커밋 1개), C=13 → B의 1개가 포함(실종 금지).
- **churn 비례 (개수와 역전)**: repo A·B 둘 다 커밋 10개(C=20)지만 A는 각 churn 800, B는 각 churn 20.
  개수로는 5:5인데 **A가 슬롯 과반(≥8)**, B는 floor로 ≥1(변경량 우선).
- **churn 우선 채택**: 단일 repo에 13개 커밋(cap 초과) → churn 최저 커밋이 결과에서 탈락하고
  churn 높은 것들이 남는다(최신순 아님). cap 이내 케이스에선 순서가 churn desc임도 확인.
- **clamp**: repo A(커밋 5개 — 1개 churn 5000 + 4개 churn 10) vs repo B(커밋 10개, 각 churn 200), C=15.
  raw churn이면 A 가중치(5040)>B(2000)라 A가 order 1순위지만, clamp로 A=440<B=2000 → **B가 order
  1순위가 되고 A는 floor 수준(~2개)으로 억제**(lockfile 한 방이 저활동 repo를 상위로 못 끌어올림).
- **cap 이하 전부 포함**: `C ≤ 12` → 모든 (중복 제외) 제목 포함(배분 없이 전량).
- **중복 제거**: 두 repo에 동일 제목 → 결과에 1회만.
- **빈 입력**: `[]` → `[]`.
- **repo 과다**(n > cap): churn 0(빈 커밋) repo 20개 → weight 모두 1, order는 label순, cap개만 대표, 결과 ≤ 12.

### 4b. 신규 — 길이 밴드

- `diary_length`: 경계값(3→400, 4→550, 10→550, 11→750) 검증.
- `build_system_prompt(cfg, C)`가 밴드에 맞는 목표 문자열("400"/"550"/"750")을 포함.

### 4c. 기존 유지·소폭 수정

- `git_commits_for_reads_dated_authored_subjects` — 반환이 `Vec<(String,u64)>`로 바뀌어
  `.0`(제목) 기준 assert로 수정. `--allow-empty` 커밋이라 churn 0 허용.
- `collect_work_log_recovers_cwd_from_known_project_mapping` — `commits`는 여전히 `Vec<String>`,
  1 repo·1 커밋이라 floor로 포함 → 무수정 통과.
- `collect_work_log_falls_back_to_branch_and_prompt` — topics 폴백 무변경 → 통과.
- `build_system_prompt` 호출 테스트 6곳 — 새 시그니처로 `build_system_prompt(&cfg, 0)` 갱신
  (내용 assert는 유지). 기존 "500자" 리터럴 검증이 있으면 밴드 문구로 갱신.

### 4d. 테스트 불가 (문서화)

실제 WSL 경로(`wsl -d <distro> -- git`) 및 numstat 파싱의 WSL 실행은 CI에 WSL이 없어 단위 테스트
불가 — 기존 `git_commits_for`와 동일. host-편향 제거는 `balance_commits`가 host 순서 무관함으로
구조적 보장. numstat 파싱은 네이티브 git 임시 repo로 churn>0 검증 가능(신규 테스트에 포함).

## 5. 비목표 (YAGNI)

- 잔여 슬롯 재분배, 커밋 timestamp 교차-repo 정렬, 경로 기반 churn 제외 목록(생성물 필터), 길이를
  churn 기반으로 바꾸는 것(개수 유지), idle 일기 길이 적응, 프론트/스키마 외 변경 — 미도입.
