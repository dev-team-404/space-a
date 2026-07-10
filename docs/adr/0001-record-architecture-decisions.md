# 0001. Architecture Decision Records 사용

- 상태: 채택(Accepted)
- 날짜: 2026-07-10

## 배경

SPACE-A는 초기 구조 정비 단계이며, 레포 구성·스택·데이터 모델 등 되돌리기 어려운
결정들이 앞으로 이어진다. 이런 결정의 배경과 근거를 남길 방법이 필요하다.

## 결정

주요 아키텍처 결정을 `docs/adr/`에 ADR(Architecture Decision Record)로 기록한다.

- 파일명: `NNNN-title.md` (예: `0002-monorepo-structure.md`)
- 각 ADR은 **배경 / 결정 / 결과**를 포함한다.
- 한 번 채택된 ADR은 수정하지 않고, 바뀌면 새 ADR로 대체(Superseded)한다.

## 결과

- 결정의 맥락이 코드와 함께 버전 관리된다.
- 새로 합류하는 사람/에이전트가 "왜 이렇게 했는지"를 추적할 수 있다.

## 다음에 기록할 ADR (예정)

- 레포 구성: 모노레포 vs 멀티레포
- Backend / Frontend / Agent 스택 선택
- 이슈/해결사례 공유 스키마 설계
