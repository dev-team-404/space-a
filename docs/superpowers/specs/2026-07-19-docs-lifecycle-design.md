# 문서 수명주기·아카이브 설계 (docs-lifecycle)

- **날짜**: 2026-07-19
- **범위**: 레포 공통 (docs/ 전체의 작업 문서 수명주기)
- **관련**: [docs/README.md](../../README.md) 문서 작성 규칙, ADR 0013(본 설계로 신설 예정)

## 배경 (Why)

에이전트 작업 문서(spec / plan / kickoff)가 세 루트에 흩어져 있고, 완료된 문서와
진행 중인 문서가 같은 디렉터리에 섞여 있다.

| 현재 작업 문서 루트 | 비고 |
|---|---|
| `docs/design/overview-mentor/{brainstroming,specs,plans}` | `brainstroming`은 오타 디렉터리 |
| `docs/superpowers/{specs,plans}` | superpowers 스킬 기본 경로 |
| `docs/design/overview-mentor/superpowers/plans` | 드리프트 사례 |

문제점:

1. **에이전트 컨텍스트 오염** — 완료된 plan이 활성 문서와 구분되지 않아, 에이전트가
   낡은 계획을 현재 상태로 오인할 수 있다.
2. **사람 가독성 저하** — 사람용 문서(`docs/highlevel/`, `docs/adr/`)와 달리 작업
   문서는 정제되지 않는데, 활성 목록에 수십 개가 쌓여 탐색이 어렵다.
3. **경로 드리프트 실재** — Rust 주석이 `docs/brainstroming/...`, plan이
   `docs/specs/...` 등 실제와 다른 경로를 참조하는 사례가 이미 존재한다.

## 설계 원칙 (피로도 최소화)

- 발동 단위는 **세션이 아니라 "plan 완료" 이벤트** — 사람당 많아야 주 1~2회.
- 실행 주체는 **에이전트** — 작업자가 체감하는 비용은 PR diff에 파일 이동이 보이는 정도.
- **빼먹어도 괜찮게** — 인라인 준수율 100%를 강제하지 않고, 누락은 주기 스윕이 줍는다.
- hook 강제 자동화·CI hard fail은 도입하지 않는다 (필요가 실측되면 후속 검토).

## 확정 결정

| 결정 사항 | 선택 |
|---|---|
| 아카이브 위치 | 단일 `docs/archive/` — 원 경로 구조를 그대로 미러 |
| 검증 층 | `/docs-sweep` 스킬이 검사 겸임, CI 신설 없음 |
| 기존 문서 처리 | 첫 스윕으로 후보 제안 → 사용자 승인 후 일괄 이동 |
| 스킬 구성 | 스킬 2개: `/docs-archive`, `/docs-sweep` |
| 오타 디렉터리 | 첫 일괄 아카이브 **이전에** rename (아카이브에 오타 박제 방지) |
| superpowers 저장 경로 | CLAUDE.md 선호 지침으로 컴포넌트 디렉터리로 오버라이드 |

## 컨벤션 (What)

### 1. 위치가 곧 상태

- 작업 문서 루트에 있으면 **활성**, `docs/archive/` 아래에 있으면 **완료·폐기**.
- 아카이브 경로는 원 경로를 미러한다.
  예: `docs/design/overview-mentor/plans/X.md` → `docs/archive/design/overview-mentor/plans/X.md`
- `docs/archive/`는 에이전트 기본 탐색에서 제외한다 (과거 이력 조사 시에만 명시적으로 접근).

### 2. frontmatter는 아카이브 시점에만

신규 문서에는 frontmatter를 요구하지 않는다 (작성 부담 0). 아카이브로 옮기는 순간
`/docs-archive`가 자동으로 스탬프를 찍는다:

```yaml
---
status: done | superseded
archived: 2026-07-19
---
```

- `done` — 구현이 완료되어 역할을 다한 문서.
- `superseded` — 새 문서로 대체된 문서 (가능하면 본문에 대체 문서 링크를 남긴다).

### 3. 신규 작업 문서 저장 위치

- spec / plan / kickoff는 해당 컴포넌트의
  `docs/design/<component>/{specs,plans,brainstorming}`에 저장한다.
  (superpowers 스킬의 기본 경로 `docs/superpowers/`를 오버라이드하는 선호 지침)
- 컴포넌트가 애매한 레포 공통 작업은 `docs/superpowers/{specs,plans}`를 허용한다.
- 기존 `docs/superpowers/` 문서는 옮기지 않는다 — 신규 유입만 컴포넌트 디렉터리로 유도.

## `/docs-archive` 스킬

- **트리거**: 구현 plan이 완료된 시점 (같은 PR에서 실행하는 것이 DoD). 사용자가
  명시적으로 호출하거나, CLAUDE.md 규칙에 따라 에이전트가 마무리 단계에서 호출.
- **입력**: 아카이브할 문서 경로(들). 인자가 없으면 현재 세션 맥락에서 완료된 작업의
  관련 문서(plan + 해당 spec/kickoff)를 식별해 확인 후 진행.
- **절차**:
  1. frontmatter 스탬프 (`status`, `archived`)
  2. `git mv`로 `docs/archive/` 미러 경로로 이동 (중간 디렉터리 생성)
  3. 레포 전체에서 옛 경로 참조를 grep → 새 경로로 갱신
  4. 검증: 옛 경로 참조 0건 확인
- **하지 않는 것**: 커밋 생성 (진행 중인 PR 흐름에 맡김), 문서 내용 수정
  (frontmatter 외), 완료 여부 판단 강행 (불확실하면 사용자에게 확인).

## `/docs-sweep` 스킬

- **트리거**: 명시적 호출만 (주기 실행은 운영자가 결정). 자동 발동 없음.
- **출력** (두 가지 보고, 직접 이동은 하지 않음):
  1. **완료 추정 후보 목록** — 활성 루트의 각 문서에 대해 git 이력(마지막 수정일,
     관련 구현 커밋 존재)을 근거로 완료로 추정되는 문서를 근거와 함께 나열.
  2. **구조 위반 보고** — `docs/archive/` 밖의 `status: done|superseded` frontmatter,
     깨진 문서 간 상대 링크, 미러 구조 불일치.
- **후속**: 사용자가 후보를 승인하면 `/docs-archive`를 호출해 이동.
- **하지 않는 것**: 문서 이동·수정 일체 (탐지는 자동, 판단은 사람).

## 규칙 기록 (3곳)

1. **`CLAUDE.md`** (루트) — 문서 작성 규칙 섹션에 추가:
   - 신규 spec/plan/kickoff 저장 위치 규칙 (위 컨벤션 3)
   - "구현 plan 완료 시 같은 PR에서 `/docs-archive` 실행" (DoD)
   - "`docs/archive/`는 기본 탐색에서 제외"
   - `AGENT.md`가 `@CLAUDE.md`를 임베드하므로 타 에이전트에게도 전달됨.
2. **`docs/README.md`** — 문서 작성 규칙 섹션에 아카이브 규칙 추가 (사람·비Claude 도구 커버).
3. **ADR** — `docs/adr/`에 본 컨벤션을 확정 결정으로 기록 (레포 구조 결정이므로 ADR 대상).
   번호는 구현 시점의 다음 번호를 사용 (현재 인덱스 기준 0013 예상).

## 실행 순서

| 단계 | 내용 | 검증 |
|---|---|---|
| ① 구축 | 스킬 2개 작성(`.claude/skills/`), CLAUDE.md·docs/README.md 규칙 추가, ADR 작성 | 스킬 문서 리뷰, 규칙 문구 확인 |
| ② rename | `brainstroming` → `brainstorming` `git mv` + 참조 파일 18개 갱신 (문서 15 + Rust 주석 3) | `brainstroming` 문자열 잔존 0건 |
| ③ 첫 스윕 | `/docs-sweep` 실행 → 완료 추정 후보 목록 제시 | 스킬 동작 검증 겸임 |
| ④ 일괄 아카이브 | 사용자 승인분만 `/docs-archive`로 이동 | 옛 경로 참조 0건, 미러 구조 일치 |

## 범위 제외 (후속 작업)

- **CLAUDE.md 모듈별 재구성** — 본 작업 완료 후 별도 진행
  (`claude-md-management:claude-md-improver` 스킬 사용). 수명주기 컨벤션이 입력이 됨.
- **CI 워크플로** — 스윕 운영으로 필요성이 입증되면 warning-only로 도입 검토.
- **`docs/superpowers/` 기존 문서의 컴포넌트 디렉터리 이전** — 하지 않음.
- **ADR 0003 중복 번호 정리** — `0003-a-lens-server-and-frontend-stack.md`와
  `0003-room-placement-geometry-v2.md`가 공존 (인덱스에는 후자만 등재). 본 작업과
  무관하므로 기록만 남긴다.

## 성공 기준

1. 활성 루트에는 진행 중 문서만, 완료 문서는 `docs/archive/` 미러 아래에 위치.
2. 이동된 문서의 옛 경로를 참조하는 곳이 레포 전체에 0건.
3. 신규 작업 문서가 컴포넌트 디렉터리에 생성됨 (이후 세션에서 관찰).
4. 작업자 추가 부담 없음 — 완료 PR에 파일 이동 커밋이 따라붙는 것 외에 새 수동 절차 없음.
