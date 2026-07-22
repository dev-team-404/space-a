# 다이어리 프로젝트 스코핑 설계

- **작성일**: 2026-07-22
- **상태**: 승인 (구현 대기)
- **컴포넌트**: a-mate (`crates/core/src/diary`)
- **관련**: 반복 코칭 재설계와 무관 — 다이어리 서사 품질 개선 건

## 1. 배경 — 문제와 근본 원인

Windows·WSL 양쪽, 그리고 여러 프로젝트를 하루에 함께 돌리면, 생성된 일기 안에서
서로 다른 프로젝트 이야기가 라벨 없이 뒤섞인다("A 프로젝트 작업 얘기를 하다가 중간에
B 프로젝트가 튀어나온다"). 프로젝트 구분이 안 된다.

### 근본 원인 (조사 확정)

LLM에 넘어가는 유일한 데이터는 `Brief` 구조체다(`diary/mod.rs`). 그런데 그날 한 작업을
담는 `work_log: WorkLog`가 **프로젝트 경계 정보를 버린다**:

- `WorkLog.commits: Vec<String>` — 여러 repo의 커밋 제목을 `balance_commits`로 모으면서
  **어느 프로젝트/repo 소속인지 라벨을 전부 떼어낸** 평탄한 문자열 리스트.
- `WorkLog.topics: Vec<String>` — 브랜치명·첫 프롬프트를 프로젝트 표시 없이 정렬·dedup한 flat 리스트.

결과적으로 LLM은 "커밋 제목 N개, 토픽 M개"만 받고 각 항목이 어느 프로젝트 것인지 알 수 없다.
게다가 시스템 프롬프트는 *"종류별로 문단을 나누지 말고 하나의 자연스러운 하루 이야기로 엮으라"*고
지시한다. **정보가 없어서 못 섞는 게 아니라 못 나누는 것** — 파이프라인이 프로젝트 라벨을
버리고, 프롬프트는 통합을 강제한다.

### 실측 근거 (36 활동일 · 245 세션, 로컬 DB)

| 지표 | 값 |
|------|-----|
| 멀티프로젝트 날 (하루 ≥2개 프로젝트) | 20/36일 (56%) |
| 진짜 동시작업 날 (서로 다른 프로젝트 세션의 시간이 실제로 겹침) | 19/36일 (53%) |
| 하루 프로젝트 수 분포 | 1개 44% · 2개 22% · 3개 28% · 4개+ 6% |
| worktree가 프로젝트 수를 부풀린 날 | 0/36일 |

- 문제는 예외가 아니라 절반 이상의 날에 일어난다. 멀티프로젝트 날은 거의 전부가
  "번갈아"가 아니라 **진짜 동시에** 돌린 날이다.
- 하루 프로젝트 수는 대부분 2~3개 — cwd/repo 단위로 나눠도 산만하지 않다.
  걱정했던 worktree/모노레포 과분할은 실측상 없다(모노레포 `space-a`는 자연히 하나로 묶임).
- 오히려 반대 문제가 관찰됨: 같은 프로젝트가 접근 경로에 따라 쪼개진다
  (WSL에서 연 `agent-meter` vs Windows에서 `\\wsl.localhost\...` 경로로 연 `agent-meter`).

## 2. 목표 / 비목표

### 목표
- 일기 한 편 안에서 각 작업이 **어느 프로젝트 것인지** 자연스럽게 드러난다(라벨 없이 섞이지 않는다).
- 여러 프로젝트를 **동시에 병행**한 날은 그 분주함이 서사에 담긴다.
- 같은 프로젝트의 WSL·Windows(WSL경로) 접근은 **하나의 프로젝트로 통합**된다.

### 비목표 (범위 밖)
- 프로젝트별 **별도 일기 파일** 생성 — 하지 않는다. "주인의 하루" = 하루 한 편 유지.
- 프로젝트별 **소제목/구획 나열** — 하지 않는다. 나열이 아니라 서사 안에서 구분.
- 모노레포 **내부 컴포넌트 세분화**(space-a → a-mate/a-hub/a-lens) — 하지 않는다.
- host(Windows/WSL)를 **별도 분리축**으로 — 하지 않는다. 각 프로젝트는 사실상 한 환경에 고정.
- `tool_usage`·`findings`·`totals`·`occasions`·`recent_diaries` — 하루 전체 합산 그대로.
- 저장 구조(`{date}.md`, `diary_index` PK `(date, scope)`)·UI(`DiaryTab.svelte`)·idle 일기 — 변경 없음.

## 3. 설계

### 3.1 데이터 구조 (`diary/mod.rs`)

평탄한 `WorkLog`를 프로젝트별 묶음으로 교체한다.

```rust
/// 그날 한 작업 — 프로젝트별로 묶어 LLM이 경계를 인식하게 한다.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkLog {
    pub projects: Vec<ProjectWork>, // 그날 활동한 프로젝트들, 첫 활동 시각순
    pub commit_count: usize,        // 전체 커밋 수(cap 전) — 일기 길이 산정용, 기존 유지
    pub concurrent: bool,           // 서로 다른 프로젝트 세션의 시간이 실제로 겹쳤나
}

/// 한 프로젝트의 그날 작업 소재.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectWork {
    pub name: String,         // 사람이 읽는 이름 = cwd basename ("space-a", "agent-meter")
    pub commits: Vec<String>, // 이 프로젝트의 커밋 제목(balance_commits 배분 몫, 시간/청 내림차순)
    pub topics: Vec<String>,  // 이 프로젝트의 브랜치·정제된 첫 프롬프트(폴백/보조)
}
```

`WorkLog`는 이미 `Serialize`이므로 `Brief` JSON에 자동 반영된다 → LLM이 프로젝트 경계를 본다.
`commit_count`는 `diary_length` 산정에 그대로 쓰이므로 이름·의미 유지(전체 커밋 수).

### 3.2 프로젝트 식별 규칙

**정규화 키(canonical key)** 로 세션을 그룹핑한다. 목적은 "같은 프로젝트를 하나로".

| 세션 유형 | host | cwd 예 | canonical key |
|-----------|------|--------|---------------|
| WSL 직접 | `wsl:Ubuntu-22.04` | `/home/jayb/work/agent-meter` | `Ubuntu-22.04:/home/jayb/work/agent-meter` |
| Windows에서 WSL경로 | `Windows` | `\\wsl.localhost\Ubuntu-22.04\home\jayb\work\agent-meter` | `Ubuntu-22.04:/home/jayb/work/agent-meter` |
| 일반 Windows | `Windows` | `D:\Project\space-a` | `win:d:\project\space-a` |

- WSL 통합: Windows cwd가 `\\wsl.localhost\<distro>\...` 또는 `\\wsl$\<distro>\...`이면 UNC를
  리눅스 경로로 역변환해 WSL 직접 세션과 같은 키로 묶는다. `hosts.rs`의 WSL 경로↔UNC 변환
  로직을 재활용(필요 시 역함수 추가).
- **이름(`name`)** = 키가 가리키는 경로의 basename. 위 예는 모두 `agent-meter` / `space-a`.
- 일반 Windows 키는 대소문자 무시(경로 정규화). WSL(리눅스) 경로 키는 대소문자 유지.
- 이름 충돌(서로 다른 프로젝트가 같은 basename): canonical key는 경로 전체라 여전히 구분되므로
  데이터는 섞이지 않는다. 표시 이름만 겹치는 엣지케이스이며 실측 데이터엔 없음 —
  필요해지면 상위 디렉토리를 덧붙여 구분(예: `work/web`).

### 3.3 수집 로직 (`collect_work_log`)

1. 그날(로컬 날짜) 세션들을 canonical key로 그룹핑(§3.2).
2. 각 프로젝트에 대해 그 repo(들)의 git 커밋을 기존 방식으로 수집(`git_commits_for`,
   host별 Windows/WSL 분기 유지). `balance_commits`의 repo별 공정 배분(D'Hondt)은 유지하되,
   **flat으로 합치지 않고 프로젝트별 몫을 그대로 각 `ProjectWork.commits`에 담는다.**
3. topics(비-main 브랜치 + 정제된 첫 프롬프트)도 프로젝트별로 귀속.
4. **노이즈 필터**: 커밋 0개 **그리고** 유의미한 topic 0개인 프로젝트는 제외
   (temp/드라이브 루트/홈에서의 우발적 세션 대부분이 여기 해당).
5. **정렬**: 프로젝트별 첫 활동 시각(`min(first_ts)`)순 → "오전 A, 오후 B" 서사가 자연스럽게.
6. **`concurrent` 판정**: 서로 다른 프로젝트에 속한 두 세션의 `[first_ts, last_ts]` 구간이
   겹치면 `true`. 단일 프로젝트 날은 `false`.

`WORK_LOG_TITLE_CAP`(12)은 전체 커밋 제목 상한으로 유지. `balance_commits`가 이미 repo별
배분을 하므로, 프로젝트별 몫의 합이 cap을 넘지 않는다.

### 3.4 프롬프트 (`build_system_prompt`)

- `work_log` 설명을 "프로젝트별 작업"으로 갱신.
- 다음 취지의 지침을 추가한다:
  > `work_log.projects`는 그날 작업한 프로젝트별 묶음입니다. 여러 프로젝트를 오갔으면
  > 각 작업이 **어느 프로젝트 것인지** 자연스럽게 드러내세요(라벨 없이 섞으면 실패).
  > 단, 프로젝트마다 문단을 딱딱 나누지 말고 하루 흐름으로 엮으세요
  > (예: "오전엔 space-a 다이어리를 손봤고, 오후엔 agent-meter 쪽으로 넘어갔다").
  > `concurrent`가 true면 두 일을 **동시에 오간** 분주함도 슬쩍 담으세요
  > (예: "두 프로젝트를 왔다 갔다 하느라 정신없었네").
- 기존 철학은 **유지**: "종류별 문단 나열 금지", "그날을 가장 잘 말해주는 중심 줄기 하나 +
  나머지는 곁들이듯". 프로젝트 구분은 나열이 아니라 서사 안의 전환으로 표현.
- 길이 산정(`diary_length`)은 `commit_count` 기반 그대로 — 커밋이 많은(=여러 프로젝트로
  분주한) 날은 자동으로 길어진다. 프로젝트 수로 별도 조정하지 않는다(YAGNI).

## 4. 테스트 계획

core 단위 테스트(in-memory store + `MockEngine`), 기존 `diary` 테스트 스타일을 따른다.

- **분리**: 두 repo의 세션·커밋을 넣으면 `WorkLog.projects`가 2개로 나뉘고 각 커밋이
  올바른 프로젝트에 귀속되는지.
- **WSL 통합**: WSL 직접 세션과 Windows-WSL경로 세션(같은 프로젝트)이 하나의
  `ProjectWork`로 묶이는지.
- **concurrent 판정**: 시간이 겹치는 두 프로젝트 → `true`; 시간이 안 겹치는(순차) → `false`;
  단일 프로젝트 → `false`.
- **노이즈 필터**: 커밋·topic 모두 없는 프로젝트가 제외되는지.
- **직렬화**: `Brief` JSON에 `work_log.projects[].name`이 실리는지.
- **프롬프트**: `build_system_prompt`가 새 프로젝트 지침 문구를 포함하는지(문자열 포함 확인).
- **회귀**: 단일 프로젝트 날의 브리프 구조가 기존과 동등한 정보를 담는지(커밋·토픽 보존).

## 5. 영향 범위 요약

| 파일 | 변경 |
|------|------|
| `crates/core/src/diary/mod.rs` | `WorkLog`/`ProjectWork` 구조, `collect_work_log`, `balance_commits` 반환형, `build_system_prompt` 문구 |
| `crates/core/src/hosts.rs` | (필요 시) UNC→WSL 경로 역변환 헬퍼 |
| 테스트 | 위 테스트 계획 |

`src-tauri`·프론트엔드·저장 스키마·다른 rules는 변경 없음.
