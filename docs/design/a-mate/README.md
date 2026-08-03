# Agent Mentor — 프로젝트 개요 문서

Windows·WSL의 Claude Code 사용 기록을 로컬에서 분석해, AI 코딩 에이전트를 **토큰 낭비 없이**
쓰도록 코칭하는 Windows 상주형 데스크톱 앱. 트레이에 조용히 상주하다가, 데스크톱 위 로봇
마스코트가 말풍선으로 조언하고, 매일 밤 에이전트 시점의 1인칭 일기를 쓴다.
여기에 **팀 지식 허브(a-hub)** 연동이 붙어, 배운 것을 팀에 넘기고 팀이 쓰면 알림으로 돌아온다.

이 디렉토리는 프로젝트를 처음 접하는 사람을 위한 **자기완결적 소개 문서 묶음**이다.

## 30초 요약

- **무엇**: "집계"가 아니라 "코칭" — MCP 대형 결과, 반복 지시, 모델 오남용 같은 낭비를
  결정론적 규칙으로 탐지하고, 조치 방법(복사 가능한 명령)까지 준비한다.
- **어떻게**: Tauri v2 + Rust 백엔드가 JSONL 트랜스크립트를 관대하게 파싱해 SQLite에 정규화 →
  규칙 엔진이 Finding을 내고 → 같은 Finding이 코칭 카드·마스코트 말풍선·1인칭 일기로 렌더링된다
  ("한 번 계산, 여러 갈래 렌더링").
- **왜 다른가**: WSL+Windows 통합을 GUI로, 대화 시작 전에 새는 always-on 토큰의 정적 분석,
  지표 대시보드가 아닌 **다마고치풍 동반자** UX, 그리고 **개인 발견이 팀 지식이 되는 루프**.
- **프라이버시**: 100% 로컬 처리 기본. 대화 원문은 DB에도 안 넣고(포인터 지연 로드),
  외부로 나가는 것은 결정론적으로 증류한 브리프와 숫자뿐이다.

## 문서 구성

| 문서 | 내용 | 이런 질문에 답함 |
|---|---|---|
| [01-product.md](01-product.md) | 제품 비전·포지셔닝·설계 원칙·페르소나·스택 | "이게 뭐 하는 물건이고 왜 이렇게 만들었나" |
| [02-features.md](02-features.md) | 기능 카탈로그 — 수집·인벤토리·규칙 3종·다이어리·큐레이션·마스코트·팀 연동·미니홈피 | "지금 뭐가 되나" |
| [`docs/architecture/a-mate.md`](../../architecture/a-mate.md) | **아키텍처** — 크레이트 구성·스캔 사이클·확장 심·스키마·커맨드/이벤트·외부 연동 | "코드가 어떻게 짜여 있나" |
| [04-history-and-roadmap.md](04-history-and-roadmap.md) | 개발 타임라인·E2E가 뒤집은 설계 교훈·유예된 확장 지점 | "어떤 시행착오를 겪었고 다음엔 뭘 할 수 있나" |
| [build-and-run.md](build-and-run.md) | 사전 요구사항·빌드·실행·트러블슈팅 | "어떻게 돌리나" |

> [03-architecture.md](03-architecture.md)는 `docs/architecture/a-mate.md`로 이전되어 링크만 남아 있다.

## 더 깊이 보려면

- [`specs/`](specs/) — 기능별 설계 스펙 (합의 시점의 기록 — 현재 상태와 다를 수 있다)
- [`plans/`](plans/) — 스펙을 태스크로 분해한 구현 플랜
- [`brainstorming/`](brainstorming/) — 킥오프 시드 문서 (열린 질문 상태의 원본)
- [`a-mate/CLAUDE.md`](../../../a-mate/CLAUDE.md) — 개발 제약 (Tauri v2 전용, Windows 전용, 어댑터 추상화 원칙)
- [`docs/highlevel/level-2-a-mate-data-flows.md`](../../highlevel/level-2-a-mate-data-flows.md) — a-hub로 무엇이 나가는지

> `specs/`·`plans/`는 **시점 기록**이라 현행화 대상이 아니다. "지금 어떻게 되어 있나"는
> 01/02/architecture 세 문서가 답한다.
