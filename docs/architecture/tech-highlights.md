# 기술 하이라이트 — 비자명한 문제와 해결

> 기준: 2026-08-04. 각 항목은 실제 코드 경로를 가리키며, 상세 배경은 컴포넌트 아키텍처 문서와 [ADR](../adr/)에 있다.

세 컴포넌트를 관통하는 설계 원칙이 하나 있다: **수치와 판정은 결정론으로 계산하고, 생성 모델은 경계를 지운 서사·판정 보조에만 쓴다.** 아래 항목 다수가 이 원칙의 구체적 구현이다.

## A-Mate (Tauri v2 + Rust + Svelte)

| 문제 | 해결 | 코드 |
|---|---|---|
| 첫 스캔 중 SQLite 락 경합으로 UI가 수 초 얼어붙음. Windows에서 `yield_now`는 같은 프로세서만 양보해 락 핸드오프를 보장하지 않음 | 파일 단위 락 청킹 + 대기자 신고 카운터(`store_waiters`) 기반 명시적 핸드오프. 주석에 실측치(16.7s → 0.9s)와 Windows `SwitchToThread` 특성 근거를 남김 | `a-mate/src-tauri/src/pipeline.rs`(hand_off), `a-mate/src-tauri/src/lib.rs`(store_waiters) |
| 트랜스크립트(JSONL)가 수백 MB로 자라는데 매번 전체 파싱은 낭비, DB에 원문을 담으면 프라이버시 표면이 커짐 | 오프셋 시크로 완결 라인만 증분 소비 + `dedup_key` 멱등 삽입. 원문은 DB 밖 포인터로 두고 필요할 때 지연 역참조 — 성능과 로컬 프라이버시 경계를 동시에 해결 | `a-mate/crates/core/src/adapter.rs`(증분 수집), `a-mate/crates/core/src/store.rs`(멱등 삽입), `a-mate/crates/core/src/ops.rs`(`deref_jsonl_line`) |
| 분석 대상이 네이티브 Windows와 WSL 배포판들에 흩어져 있고, 경로 체계·파일 감시 방식이 다름 | `wsl.exe -l -q`의 UTF-16LE 출력 디코드로 배포판 발견, UNC 경로 양방향 변환, WSL UNC에는 notify 대신 PollWatcher 폴백 | `a-mate/crates/core/src/hosts.rs`, `a-mate/src-tauri/src/pipeline.rs` |
| 반복 지시를 코칭하려면 유사 프롬프트 군집화가 필요한데, 한국어 조사·어미 변형에 단순 매칭이 무력함 | 결정론 채굴(문자 bigram Jaccard 군집)로 후보를 만들고, LLM은 정밀도 필터로만 사용. 전송 실패와 파싱 실패를 분리해 재시도·영구 캐시 의미론을 다르게 가져감 | `a-mate/crates/core/src/rules/r6_cluster.rs`, `a-mate/crates/core/src/judge.rs` |
| 마스코트 창을 리사이즈로 접었다 펴면 창 원점 이동 + WebView 비동기 리페인트로 로봇이 튀어 보임 | 창 크기를 상시 고정하고 전역 커서 폴링으로 `ignore_cursor_events`를 토글 — 투명 여백은 클릭이 통과하고 로봇 영역만 상호작용 | `a-mate/src-tauri/src/lib.rs`(폴러), `a-mate/src-tauri/src/geometry.rs` |
| 코칭 근거로 쓰는 로그에 시크릿이 섞일 수 있음 | 수집 단계에서 시크릿 패턴 리댁션. 외부 공유는 사용자가 고른 증류물로 제한(`SHARE_RULES`)되어, 원문 대화는 로컬을 떠나지 않음 | `a-mate/crates/core/src/adapter.rs`(리댁션), `a-mate/crates/core/src/hub.rs`(공유 경계) |

## A-Hub (FastAPI — Work·Life 별개 프로세스)

| 문제 | 해결 | 코드 |
|---|---|---|
| 에이전트(MCP)와 비-MCP 환경(REST)이 같은 지식 기능에 접근해야 함 | FastMCP Streamable HTTP 앱을 FastAPI lifespan에 엮어 같은 프로세스의 `/mcp`에 마운트. 신원은 요청별 전송 계층 Bearer로 해석. 실제 MCP 클라이언트를 띄우는 통합 테스트로 검증 | `a-hub/work/ahub/api/rest_server.py`, `a-hub/work/ahub/api/mcp_server.py`, `a-hub/work/tests/test_mcp_http.py` |
| 교차 팀 재사용이 생겨도 원 작성자가 모르면 공유 동기가 사라짐 | ReuseEvent 가시성을 "인용이 일어난 space 멤버 **또는** 원 작성자"의 OR 규칙으로 설계 — 남의 팀에서 일어난 재사용도 원 작성자에게 도달한다(인정 루프). E2E 실측으로 구조 결함을 찾아 고친 경위가 주석에 남아 있음 | `a-hub/work/ahub/core/services.py`(list_reuse_events) |
| 여러 클라이언트가 동시에 방 셀을 점유하면 겹침이 생김. 자율 스폰은 결정론이어야 재현 가능 | 전역 락 안 "빈 셀일 때만 점유" 원자 처리(`409 cell_taken`), 체비셰프 링 + sha256 타이브레이크의 결정론적 스폰 분산, 회전 footprint 점유 계산, 시작 시 v2→v4 데이터 마이그레이션 | `a-hub/life/life_server/life.py` |
| 저장소 구현이 바뀌어도 도메인이 흔들리면 안 됨 | ports & adapters — `core/`는 adapters/api를 import하지 않고, memory·SQLite·DynamoDB 스토어를 env로 교체. 테스트 픽스처가 memory·sqlite 양쪽을 파라미터라이즈해 동작 동일성을 강제 | `a-hub/work/ahub/core/ports.py`, `a-hub/work/tests/conftest.py` |

## A-Lens (FastAPI + PixiJS, 프레임워크 없는 TS)

| 문제 | 해결 | 코드 |
|---|---|---|
| 문서 주제로 협업 관계를 추정해야 하는데 한국어 형태소 분석기를 넣으면 의존성이 무거워짐 | 서드파티 0개의 근사 토크나이저(조사·어미 박리, 한영 혼합 토큰 분리) + IDF 가중 주제 겹침 그래프. 사실 엣지(재사용·해결 인계)와 추정 엣지(주제)를 종류로 분리하고, 문서 수 정규화 임계·O(D²) 상한·입력 지문 캐시까지. 규칙은 33개 테스트로 고정 | `a-lens/backend/alens/collab.py`, `a-lens/backend/tests/test_collab.py` |
| 통짜 배경 이미지 위에 아이소메트릭 격자를 정합해야 클릭·배치가 자연스러움 | manifest의 바닥 꼭짓점·기울기로 이미지를 2:1 격자에 비등방 정규화하고 역변환으로 히트존을 배치. 겹치는 오브젝트 영역은 z순서가 아니라 사각형 차집합으로 분리, 칠판 글씨는 이진 탐색 폰트 피팅 | `a-lens/frontend/src/life/renderer.ts` |
| 원천 서버·LLM 어느 쪽이 죽어도 관전 화면은 떠야 함 | 단일 백그라운드 워커 + 요청 무차단 스냅숏 + 재시작 시 디스크 복원. 인증 지문별 soft-fail 30분 캐시, LLM 실패 → 규칙 번역, 허브 실패 → 데모 데이터 등 8개 강등 경로가 문서 표와 1:1로 구현됨 | `a-lens/backend/alens/collector.py`, `a-lens/backend/alens/translator.py`, [03-architecture.md](./a-lens/03-architecture.md) |
| 허브가 이슈↔해결 문서 링크를 응답에 싣지 않음 | resolve가 이슈와 파생 문서에 같은 시각을 찍는 성질을 타임스탬프 조인 키로 활용해 "누가 해결했나"를 복원 — 모호하면 잇지 않는다 | `a-lens/backend/alens/collector.py`(`_issue_resolvers`) |

## 검증 방법

각 컴포넌트의 테스트 실행 절차는 [A-Mate](./a-mate/build-and-run.md) · [A-Hub](./a-hub/build-and-run.md) · [A-Lens](./a-lens/build-and-run.md) 문서에 있다. 테스트 규모와 구성은 컴포넌트 아키텍처 문서(예: [a-mate 03-architecture.md](./a-mate/03-architecture.md) §10)가 관리한다.
