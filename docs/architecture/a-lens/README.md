# A-Lens — 현행 프로젝트 개요

> 실측 기준: `main @ fda2467` (2026-08-04)

A-Lens는 A-Hub의 Work·Life 데이터를 사람이 짧게 관전할 수 있는 2D 협업 공간으로 번역하는
웹 애플리케이션이다. 브라우저가 A-Hub를 직접 읽지 않고, A-Lens 백엔드가 원천 데이터를 수집해
화면 전용 뷰모델로 바꾼 뒤 PixiJS 씬과 DOM 패널에 제공한다.

이 디렉터리는 현재 코드 기준의 문서다. 초기 제품 구상과 작업 당시의 스펙은
[`docs/archive/design/a-lens/`](../../archive/design/a-lens/)에 시점 기록으로 남아 있다.

## 30초 요약

- **홈**: A-Hub Space 목록을 읽고, 이미 만든 방과 아직 방이 없는 Space를 구분한다.
- **방**: 프리셋 배경, 책상과 사람 캐릭터로 팀 상태를 보여준다.
- **협업 정보**: 작업 기록, 이슈 공유, 지식 재사용, 문서함과 사람 간 협업 지도를 제공한다.
- **사람용 번역**: Page와 Issue를 분류·요약·서사화하며, LLM 실패 시 결정론 규칙으로 강등한다.
- **원천 결합**: Work의 업무 기록에 Life의 이름·프레즌스·마스코트 이미지를 선택적으로 결합한다.
- **읽기 경계**: A-Hub에는 읽기 전용 소비자로 동작한다. A-Lens 자체의 방 프리셋과 런타임 연결 설정만 로컬 파일에 쓴다.
- **실행 형태**: 개발 시 Vite와 FastAPI를 분리하고, 배포 시 한 컨테이너에서 정적 프론트와 API를 함께 제공한다.

## 문서 구성

| 문서 | 내용 | 이런 질문에 답함 |
|---|---|---|
| [01-product.md](01-product.md) | 제품 범위, 사용자, 원칙과 비목표 | “A-Lens는 무엇을 책임지나?” |
| [02-features.md](02-features.md) | 홈·방·협업 Hub·설정의 현재 기능 | “지금 화면에서 무엇을 할 수 있나?” |
| [03-architecture.md](03-architecture.md) | 백엔드·프론트 구조, 캐시와 실패 강등 | “코드가 어떻게 나뉘고 흐르나?” |
| [04-data-and-integration.md](04-data-and-integration.md) | Work·Life·LLM 데이터 결합과 뷰모델 | “어떤 데이터를 어디서 받아 어떻게 바꾸나?” |
| [05-history-and-constraints.md](05-history-and-constraints.md) | 구현 연혁, 현재 제약과 후속 과제 | “초기 설계와 무엇이 달라졌나?” |
| [build-and-run.md](build-and-run.md) | 로컬·Docker 실행, 설정과 검증 | “어떻게 띄우고 확인하나?” |

## 코드 위치

```text
a-lens/
├── backend/alens/       # FastAPI, 수집·번역·뷰모델·설정·캐시
├── backend/dummy_data/  # 언제든 재생 가능한 데모 회사 데이터
├── frontend/src/        # PixiJS 씬, DOM UI, 방 편집기
├── assets/kit/          # 방·책상·캐릭터 스프라이트 킷
├── Dockerfile
└── docker-compose.yml
```

## 더 깊이 보려면

- [A-Lens 코드 진입점](../../../a-lens/README.md)
- [A-Hub 현행 문서](../a-hub/)
- [A-Lens 초기 설계와 작업 스펙](../../archive/design/a-lens/)
- [FastAPI + PixiJS 결정 ADR](../../adr/0003-a-lens-server-and-frontend-stack.md)
- [컴포넌트 간 계약](../../../contracts/)
