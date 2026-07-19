---
name: docs-sweep
description: Use when asked to sweep, tidy, review, or health-check the repo's working docs (plans/specs/kickoffs) — finding completed or stale documents still sitting in active roots, or detecting docs structure violations. Detection and reporting only.
---

# docs-sweep

활성 문서 루트를 스캔해 (1) 완료로 추정되는 문서 후보와 (2) 구조 위반을 **보고만** 하는
스킬. 컨벤션의 근거는 `docs/design/common/specs/2026-07-19-docs-lifecycle-design.md` 참조.

**핵심 원칙: 탐지는 자동, 판단은 사람.** 이 스킬은 파일을 이동·수정하지 않는다.
이동은 사용자가 후보를 승인한 뒤 docs-archive 스킬로 수행한다.

## 스캔 대상 (활성 루트)

`docs/` 아래의 `specs/`, `plans/`, `brainstorming/` 디렉터리 전부.
단, `docs/archive/` 아래는 제외한다.

## 절차

1. **문서 나열** — 활성 루트의 모든 `.md` 파일.
2. **문서별 완료 신호 수집** (근거는 보고서에 그대로 적는다):
   - `git log -1 --format=%cs -- <file>` — 마지막 수정일
   - plan의 체크박스 상태 — `- [ ]` 잔존 여부 (전부 `- [x]`면 강한 완료 신호)
   - 문서가 다루는 기능의 구현 커밋이 이미 머지되었는지 (`git log --oneline` 대조)
   - handoff/continuation 문서는 후속 문서가 존재하면 superseded 후보
3. **구조 위반 검사**:
   - `docs/archive/` 밖에 있는데 frontmatter가 `status: done|superseded`인 문서
   - 문서 간 깨진 상대 링크 (링크 대상 파일 부재)
   - `docs/archive/` 아래인데 원 루트 구조와 미러가 어긋난 경로
4. **보고서 출력**:

   | 문서 | 마지막 수정 | 근거 | 추천 |
   |---|---|---|---|
   | plans/X.md | 2026-07-01 | 체크박스 완료, 구현 커밋 abc123 머지됨 | done |

   - 확신이 낮은 문서는 후보에서 빼지 말고 "판단 필요"로 별도 분류한다.
   - 위반 목록은 후보 표와 분리해 보고.
5. **마무리** — 사용자 승인을 기다린다. 승인된 문서에 대해서만 docs-archive 스킬을 호출.

## 하지 않는 것

- 파일 이동·수정·삭제 일체. "확실해 보이는" 문서도 예외 없음 — 보고서에만 담는다.
- 완료 판단의 강행. 근거가 약하면 "판단 필요"로 분류하는 것이 정답이다.
