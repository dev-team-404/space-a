# Agent Mentor — 아키텍처

> **이 문서는 [`docs/architecture/a-mate.md`](../../architecture/a-mate.md)로 옮겨졌습니다.**

아키텍처 설명이 두 곳에 있으면 반드시 한쪽이 먼저 낡는다. 실제로 이 문서는 2026-07-10 실측
기준으로 멈춰 있었고, 그 사이에 규칙 12종 → 3종, 커맨드 19개 → 77개, 팀 지식 허브·미니홈피·
인정 루프·콘텐츠 큐레이션이 들어오면서 내용이 크게 어긋났다.

그래서 **아키텍처 문서는 [`docs/architecture/`](../../architecture/) 한 곳으로 통일**하고,
이 파일은 링크만 남긴다.

| 찾는 것 | 문서 |
|---|---|
| 크레이트 구성 · 의존 방향 · 모듈 지도 | [`docs/architecture/a-mate.md`](../../architecture/a-mate.md) §1–2 |
| 스캔 사이클 · 락 규율 | 같은 문서 §3, §9 |
| 확장 심(SourceAdapter / Engine / Rule) | 같은 문서 §4 |
| 데이터 모델 · SQLite 스키마 | 같은 문서 §5 |
| Tauri 커맨드 · 이벤트 · 창 | 같은 문서 §6 |
| 외부 연동(a-hub · LLM · 이미지) | 같은 문서 §7 |
| 프론트엔드 구조 | 같은 문서 §8 |
| 테스트 · 실행 | 같은 문서 §10–11 |

기능 단위 설명은 [`02-features.md`](02-features.md), 빌드·실행은
[`build-and-run.md`](build-and-run.md)를 본다.

> 2026-07-10 시점의 원문이 필요하면 git 이력에서 이 파일의 이전 리비전을 참조한다.
