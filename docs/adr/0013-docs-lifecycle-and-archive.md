# ADR 0013: 작업 문서 수명주기와 `docs/archive/` 미러를 도입한다

- 상태: 채택
- 날짜: 2026-07-19

## 배경

에이전트 작업 문서(spec / plan / kickoff)가 여러 루트(`docs/design/<component>/…`,
`docs/superpowers/…`)에 흩어져 있고, 완료된 문서와 진행 중인 문서가 같은 디렉터리에
구분 없이 쌓여 있었다.

- 에이전트가 완료된 plan을 현재 상태로 오인할 수 있다 (컨텍스트 오염).
- 활성 목록에 수십 개가 쌓여 사람의 탐색이 어렵다.
- 문서·코드 주석이 실제와 다른 경로를 참조하는 드리프트가 이미 발생했다.

설계 논의와 확정 결정은
[당시 문서 생명주기 설계](../archive/design/common/specs/2026-07-19-docs-lifecycle-design.md) 참조.

## 결정

- **위치가 곧 상태.** 작업 문서 루트에 있으면 활성, `docs/archive/` 아래에 있으면 완료·폐기.
  아카이브 경로는 원 경로를 그대로 미러한다 (`docs/<rest>` → `docs/archive/<rest>`).
- **frontmatter는 아카이브 시점에만.** 신규 문서에는 요구하지 않고, 옮기는 순간
  `status: done|superseded`와 `archived: YYYY-MM-DD`를 스탬프한다.
- **신규 작업 문서 위치.** 해당 컴포넌트의 `docs/design/<component>/{specs,plans,brainstorming}`.
  컴포넌트가 애매한 레포 공통 작업은 `docs/design/common/{specs,plans}`.
  `docs/superpowers/`는 동결한다 (신규 유입 없음, 기존 문서는 완료 시 아카이브로만 이동).
- **스킬 2개로 집행.** `.claude/skills/docs-archive`(스탬프→이동→참조 갱신→검증),
  `.claude/skills/docs-sweep`(완료 후보·구조 위반 탐지 보고만, 이동 금지).
- **DoD.** 구현 plan이 완료되면 같은 PR에서 docs-archive를 실행한다.
- **`docs/archive/`는 기본 탐색에서 제외.** 과거 이력 조사 시에만 명시적으로 접근.
- **강제 장치는 두지 않는다.** hook 자동 실행·CI hard fail 없음 — 발동은 "plan 완료"
  이벤트에만, 누락은 주기 스윕이 탐지한다 (작업자 피로도 최소화).

## 결과

- 활성 루트에는 진행 중 문서만 남아 에이전트·사람 모두 현재 상태를 신뢰할 수 있다.
- 문서 이동 시 참조 갱신 비용이 생긴다 — docs-archive가 파일명 기준 grep으로 처리하고
  옛 경로 참조 0건을 검증한다.
- 준수율 100%를 강제하지 않으므로 일부 완료 문서가 활성 루트에 남을 수 있다 —
  주기적 docs-sweep이 후보로 보고해 따라잡는다.

## 검증

- docs-sweep 실행 시 완료 추정 후보와 근거가 보고서로 생성된다.
- 일괄 아카이브 후 레포 전체에서 옛 경로를 가리키는 참조가 0건이다.
- 이후 세션에서 신규 작업 문서가 규칙 위치에 생성된다.
