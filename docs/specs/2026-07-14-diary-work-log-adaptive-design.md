# 다이어리 work_log 작업량 적응 설계 스펙

- 작성: 2026-07-14 (브레인스토밍 산출물)
- 배경: WSL 조사 중 발견한 `collect_work_log`(`crates/core/src/diary/mod.rs`)의 커밋 축소 버그.
  바쁜 멀티-repo 날에 일부 repo(특히 WSL) 커밋이 일기에서 통째로 사라진다. roadmap 항목 19.
- 다음 단계: writing-plans → TDD 구현 → push+PR (base=main, 브랜치 `feat/diary-work-log-adaptive`)

## 1. 문제 (버그 재구성)

`collect_work_log`의 커밋 수집부:

```rust
host_cwds.sort();                       // (host, cwd) 정렬
let mut commits = host_cwds.iter()
    .flat_map(|(h,c)| git_commits_for(h,c,date))   // repo별 커밋을 이어붙임
    .collect();
commits.dedup();                        // ⚠ 인접 중복만 제거
commits.truncate(WORK_LOG_CAP);         // ⚠ 앞에서 8개만 남김
```

두 결함:

1. **Windows 우선정렬 → 축소**: host 문자열은 `"Windows"`(대문자 W=87) vs `"wsl:<distro>"`(소문자
   w=119)라 Windows repo가 항상 앞에 정렬된다. `flat_map`이 repo별로 커밋을 이어붙인 뒤
   `truncate(8)`이 앞 8개만 남기므로, 바쁜 날엔 앞쪽(Windows) repo가 8칸을 다 먹고 **뒤쪽(WSL)
   repo 작업은 일기에서 통째로 사라진다.**
2. **고정 상한 8**: repo 1개인 날과 repo 5개인 날을 똑같이 8개로 자른다 — 그날의 작업 폭을
   반영하지 못한다. (부수적으로 `Vec::dedup`은 인접 중복만 제거 → repo 간 동일 제목은 중복 잔존.)

`topics`(브랜치·정제 프롬프트) 역시 같은 `truncate(WORK_LOG_CAP)`를 쓰지만, host 그룹핑이 아니라
단순 알파벳 정렬이라 위 host-편향 버그는 없다(부차적 정리 대상).

## 2. 결정 사항 (브레인스토밍 논점 3건)

1. **문자 예산 기반 적응형 길이** — 단위는 커밋 개수가 아니라 **문자 수**. 커밋 제목 길이가
   제각각이라 "8개"는 브리프 부피·토큰을 못 잡지만, 문자 예산은 브리프 크기를 직접 상한한다.
   공식: `budget_chars = min(600, 250 × repo수)`. repo수 = 그날 커밋을 1개 이상 낸 distinct repo 수.

   | repo 수 | 예산 | 대략 커밋 수(제목 ~45자) |
   |---|---|---|
   | 1 | 250자 | ~5개 |
   | 2 | 500자 | ~10개 |
   | 3+ | 600자 (상한) | ~13개 |

2. **floor + 비례 배분** — "작업 많은 repo에 더 많은 지면"이 일기 서사(오늘 주로 뭘 했나)에
   맞다. 단, 커밋 적은 repo도 최소 1줄은 남겨 원래 버그(WSL repo 실종)를 방지한다.
   - **floor**: 각 repo의 **최신 커밋 1개**를 먼저 확보(모든 repo 대표).
   - **비례**: 남은 예산을 repo별 커밋 수 비율로 나눠 각 repo에서 최신순으로 더 채운다.
   - 순수 라운드로빈(매 라운드 동일 턴)은 기각 — repo가 많은 날 floor가 예산을 다 먹어
     바쁜 repo가 오히려 과소 대표된다. 순수 비례(floor 없음)도 기각 — 커밋 1개짜리 repo가
     사라져 원래 버그가 부분 재발.

3. **topics는 일관성만** — host-편향 버그가 없으므로 알고리즘은 그대로 두고, 고정 개수 상한(8)만
   문자 예산(300자)으로 교체한다. 라운드로빈/비례 불필요. (버그픽스가 아닌 일관성 정리.)

**출력 구조 불변**: `WorkLog { commits: Vec<String>, topics: Vec<String> }` 그대로. repo 그룹핑은
내부 선택용일 뿐 LLM엔 repo명 불필요 → 시스템 프롬프트(`build_system_prompt`)도 무변경.

## 3. 변경 상세 (`crates/core/src/diary/mod.rs`)

### 3a. 상수 교체

```rust
// 삭제: const WORK_LOG_CAP: usize = 8;
const WORK_LOG_PER_REPO_CHARS: usize = 250;   // repo당 문자 할당
const WORK_LOG_BUDGET_CAP_CHARS: usize = 600; // 전체 상한
const WORK_LOG_TOPIC_CHARS: usize = 300;      // topics 문자 상한
```

### 3b. 순수 선택 로직 추출 — `balance_commits`

git/WSL 없이 결정론적으로 단위 테스트 가능하도록, 커밋 선택을 순수 함수로 분리한다.

```rust
/// repo별로 그룹핑된 커밋 제목(각 그룹은 git log 순 = 최신 우선)을 받아,
/// floor(각 repo 최신 1개) + 남은 예산 비례 배분으로 골라 평평한 리스트로 반환.
/// label은 커밋 수 동률 시 결정론적 tiebreak용(host+cwd 등 안정 문자열).
fn balance_commits(mut groups: Vec<(String, Vec<String>)>) -> Vec<String> {
    let n = groups.len();
    if n == 0 { return Vec::new(); }
    let budget = (WORK_LOG_PER_REPO_CHARS * n).min(WORK_LOG_BUDGET_CAP_CHARS);
    let total: usize = groups.iter().map(|(_, c)| c.len()).sum();

    // 커밋 많은 repo 우선(busiest-first), 동률은 label 오름차순 — host-편향 없는 결정론적 순서.
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

    let clen = |s: &str| s.chars().count();
    let mut seen = std::collections::HashSet::new();
    let mut selected: Vec<String> = Vec::new();
    let mut used = 0usize;

    // Phase 1 — floor: 각 repo 최신 unique 커밋 1개 (busiest-first). 안 들어가면 skip, 다음 repo 계속.
    for (_, commits) in &groups {
        if let Some(c) = commits.iter().find(|c| !seen.contains(c.as_str())) {
            if used + clen(c) <= budget {
                seen.insert(c.clone());
                used += clen(c);
                selected.push(c.clone());
            }
        }
    }

    // Phase 2 — 비례 배분: 남은 예산을 repo별 커밋 수 비율로. 각 repo 최신순, quota 초과 시 그 repo 종료.
    let remaining = budget.saturating_sub(used);
    for (_, commits) in &groups {
        let quota = if total == 0 { 0 } else { remaining * commits.len() / total };
        let mut spent = 0usize;
        for c in commits.iter().filter(|c| !seen.contains(c.as_str())) {
            let l = clen(c);
            if spent + l <= quota && used + l <= budget {
                seen.insert(c.clone());
                used += l;
                spent += l;
                selected.push(c.clone());
            } else {
                break; // 최신순이라 첫 미적합에서 종료(짧은 옛 커밋 찾아 건너뛰지 않음)
            }
        }
    }
    selected
}
```

설계 노트:
- **host-편향 소멸**: 선택이 host 정렬순이 아니라 커밋 수(+ floor 보장)에 의존하므로, WSL repo가
  통째로 밀려나는 구조가 사라진다. 동률 tiebreak의 알파벳 순서는 floor로 모두 대표된 뒤라 무해.
- **비례 quota 미사용분은 방치**: 작은 repo가 quota를 다 못 쓴 잔여 예산은 다른 repo로 재분배하지
  않는다(단순성). 큰 repo일수록 quota·커밋이 많아 미사용이 드물어 낭비는 미미. (v2 여지)
- **floor overflow**(repo 수 > 예산 수용량, 예: 20+개 repo): busiest-first라 커밋 많은 repo부터
  대표되고 나머지는 빠진다 — 예산 한계상 불가피. `used ≤ budget` 불변식은 유지.

### 3c. `collect_work_log` 배선

커밋 수집부만 교체(cwd 복원·`host_cwds` 구성은 그대로):

```rust
// 커밋: 그날 활동한 distinct (host, repo)를 repo별 그룹으로 — flatten 대신 그룹 유지
host_cwds.sort();
host_cwds.dedup();
let groups: Vec<(String, Vec<String>)> = host_cwds
    .iter()
    .map(|(h, c)| (format!("{h}\u{0}{c}"), git_commits_for(h, c, date)))
    .filter(|(_, v)| !v.is_empty())
    .collect();
let commits = balance_commits(groups);
```

`topics`부는 개수 상한만 문자 예산으로:

```rust
topics.sort();
topics.dedup();
// truncate(WORK_LOG_CAP) 대신 문자 예산으로 컷 (항목은 통째로 유지)
let mut used = 0usize;
topics.retain(|t| {
    let l = t.chars().count();
    if used + l <= WORK_LOG_TOPIC_CHARS { used += l; true } else { false }
});
```

## 4. 테스트

### 4a. 신규 — `balance_commits` 단위 테스트 (git/WSL 불필요)

- **floor 보장**: `[("A", 10개), ("B", 1개)]` → B의 1개가 반드시 결과에 포함(실종 금지).
- **비례**: `[("A", 짧은 20개), ("B", 2개), ("C", 1개)]` → A가 슬롯 과반, B·C 각 ≥1.
- **예산 상한**: 임의 그룹 → `결과 문자합 ≤ min(600, 250×repo수)`.
- **단일 repo**: `[("A", 다수)]` → 문자합 ≤ 250.
- **중복 제거**: 두 repo에 동일 제목 → 결과에 1회만.
- **빈 입력**: `[]` → `[]`.
- **floor overflow**: 동일 1커밋 repo 20개 → 문자합 ≤ 600, 커밋 많은(동률이면 label순) 것부터.

### 4b. 기존 유지 (회귀 방지)

- `collect_work_log_falls_back_to_branch_and_prompt` — topics 폴백·main 제외·노이즈 제외.
- `collect_work_log_recovers_cwd_from_known_project_mapping` — cwd 복원 후 커밋 1개 노출
  (1 repo·1 커밋 → budget 250 내, floor로 포함).
- `git_commits_for_reads_dated_authored_subjects` — `git_commits_for` 무변경.

### 4c. 테스트 불가 (문서화)

실제 WSL 경로(`wsl -d <distro> -- git`)는 CI/테스트 환경에 WSL이 없어 단위 테스트 불가 — 기존
`git_commits_for`도 동일. host-편향 제거는 `balance_commits`가 host 순서에 무관함으로 구조적 보장.

## 5. 비목표 (YAGNI)

- 비례 잔여 예산 재분배, 커밋 timestamp 기반 교차-repo 정렬, 커밋 중요도 가중 — 모두 미도입.
- 시스템 프롬프트·`WorkLog` 스키마·프론트 변경 없음.
