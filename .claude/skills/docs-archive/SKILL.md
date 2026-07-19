---
name: docs-archive
description: Use when a working doc (plan/spec/kickoff) in this repo has served its purpose — implementation merged, design superseded, or a docs-sweep proposal approved — and must leave the active docs roots. Also invoked as the Definition-of-Done step when finishing an implementation plan's PR.
---

# docs-archive

완료·폐기된 작업 문서를 `docs/archive/` 미러로 옮기는 절차. 컨벤션의 근거는
`docs/design/common/specs/2026-07-19-docs-lifecycle-design.md` 참조.

**핵심 원칙: 위치가 곧 상태.** 활성 루트에 있으면 진행 중, `docs/archive/` 아래에 있으면 완료.

## 입력

- 인자: 아카이브할 문서 경로(들).
- 인자가 없으면 현재 세션에서 완료된 작업의 관련 문서(plan + 해당 spec/kickoff)를
  식별해 목록을 제시하고 확인을 받은 뒤 진행한다.
- **완료 여부가 불확실한 문서는 절대 강행하지 않는다** — 사용자에게 확인.

## 절차 (문서마다)

1. **frontmatter 스탬프** — 파일 맨 앞에 추가(기존 frontmatter가 있으면 필드만 병합):
   ```yaml
   ---
   status: done        # 또는 superseded
   archived: YYYY-MM-DD
   ---
   ```
   `superseded`인 경우 본문 상단에 대체 문서 링크를 한 줄 남긴다.
2. **미러 경로로 이동** — `docs/<rest>` → `docs/archive/<rest>`. 중간 디렉터리를 만들고
   `git mv`로 이동한다 (이력 보존).
3. **참조 갱신** — 파일명(basename)으로 레포 전체를 grep한다. 상대 링크는 옛 경로
   문자열 검색으로 잡히지 않으므로 **반드시 파일명 기준으로 검색**할 것.
   참조하는 각 파일에서 링크를 새 위치 기준 상대 경로로 재계산해 수정한다.
   (docs 밖의 참조 — 코드 주석 등 — 도 동일하게 갱신)
4. **검증** — 파일명 grep을 다시 실행해, 남은 참조가 전부 새 경로(`docs/archive/...`)를
   가리키는지 확인. 옛 경로를 가리키는 참조 0건이어야 완료.

## 하지 않는 것

- **커밋을 만들지 않는다** — 진행 중인 PR/커밋 흐름에 맡긴다.
- frontmatter 외에 문서 본문을 수정하지 않는다.
- 이미 `docs/archive/` 아래에 있는 문서를 다시 옮기지 않는다 (중첩 방지).

## 흔한 실수

| 실수 | 방지 |
|---|---|
| 옛 전체 경로 문자열로만 grep → 상대 링크 놓침 | 파일명(basename)으로 검색 |
| 상대 링크 깊이 재계산 오류 | 수정 후 링크 대상 파일 존재를 확인 |
| Rust/TS 주석 속 경로 참조 누락 | grep 범위는 docs/가 아니라 레포 전체 |
