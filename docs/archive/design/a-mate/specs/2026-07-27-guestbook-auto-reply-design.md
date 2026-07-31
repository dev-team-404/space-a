---
status: done
archived: 2026-07-27
---

# G3 방명록 봇 자동 답글 설계

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate 단독 (**a-hub 무변경**)
- **브랜치**: `feat/guestbook-auto-reply` (worktree)
- **관계**: [후속 로드맵 2차 배치 G3](../plans/2026-07-26-life-social-diary-followups-roadmap.md) 착수분.
  G1(주인 신원, PR #106 — [ADR 0020](../../../../adr/0020-owner-fullname-to-hub.md)) +
  G2(답글 인프라, PR #107 — [ADR 0021](../../../../adr/0021-guestbook-replies-one-depth.md)) 위 확장.
  문체 배관(`owner_title`·`mbti_voice_hint`)은 PR #104.
  ADR 0020이 규범만 확정한 봇 작성자 표기 `"{owner_title}님의 {user_name}"`의 **구현이 본 작업**.

## 배경 — 현재 상태 (조사 결과)

- **트리거 전례**: a-mate 파이프라인은 파일 변경 이벤트 + 60초 디바운스로
  `run_pipeline_once` 실행(`pipeline.rs:27-32`, 앱 시작 시 1회 즉시), 스캔 후
  `maybe_generate_chatter_pool` 등 `maybe_*` 훅 연쇄. 엔진 미설정이면 no-op,
  LLM 실패 시 `log::warn` 후 skip → 다음 스캔 재시도(`pipeline.rs:812-815`).
- **감지 재료 완비**: `LifeClient::guestbook(life_id)`(`life_client.rs:232`) GET이
  평면 목록에 `entry_id`·`author_agent_id`·`author_name`·`body`·`parent_id`·`created_at`을
  모두 반환(최신순). 내 신원은 settings `hub_life_id`·`hub_agent_id`(`commands.rs:788-790`).
- **답글 쓰기 경로 기존**: `LifeClient::add_guestbook(life_id, body, author_name, parent_id)`
  (`life_client.rs:237`). 서버 규칙(G2): 본문 1~500자, `author_name` ≤80자,
  답글은 방 주인만·1-depth. 원글당 답글 수 서버 무제한 — **"글당 1회"는 클라이언트 책임**(ADR 0021).
- **프롬프트 전례**: `build_daily_line_prompt`/`build_chatter_prompt`(`mascot.rs:180,242`)가
  페르소나(다마고치 마스코트 1인칭) + `honorific` + `mbti_voice_hint` + `voice_guidance` 사용.
  방어 파서 `parse_chatter_lines`(`mascot.rs:270`) 재사용 가능.
- **contracts/**: Life REST는 계약 범위 밖(ADR 0020·0021 전례) — 규약 기록처는 본 스펙.

## 확정 결정 (브레인스토밍 Q&A)

| 질문 | 결정 |
|---|---|
| 인바운드 감지 | **클라이언트 폴링**(기존 GET, a-hub 무변경) — 이벤트 API는 소비자가 G3 하나뿐인 지금 과설계, P4 착수 때 재판단 |
| 트리거 | **파이프라인 스캔 훅** `maybe_reply_guestbook` — 기존 LLM 기능들과 동일 모델. 답글은 봇이 "깨어 있을 때"(스캔 시) 달림 |
| "글당 1회" 영속 | **서버 데이터 자체로 판정**(내 `hub_agent_id`의 답글 행 존재 여부) — 로컬 테이블·워터마크 없음, drift 불가능 |
| 백로그 처리 | **전부 답글, 스캔당 상한 3개** — 오래된 글도 결국 답글 받고, 도배·LLM 비용은 상한으로 바운드 |
| 설정 토글 | **없음(항상 켜짐)** — hub 연결 + 엔진 설정이 유일한 게이트. 필요해지면 후속 추가 |
| 새 ADR | **불필요** — a-hub 무변경이고 폴링·훅은 클라이언트 내부의 되돌리기 쉬운 결정. 근거는 본 스펙에 기록 |

## 목표

1. 스캔 훅 `maybe_reply_guestbook`: 내 방 방명록에서 미답글 원글을 골라
   봇 성향(MBTI+호칭+페르소나) 답글을 생성·게시 (스캔당 최대 3개).
2. 봇 작성자 표기 `"{owner_title}님의 {user_name}"` 조립 (ADR 0020 규범 구현).
3. 실패 무해: 어떤 실패도 스캔 파이프라인·기존 기능에 영향 없이 조용히 skip.

## 비목표

- a-hub 변경 일체 (이벤트/알림 API — P4 착수 때 재판단).
- P3(방문 시 자동 방명록 작성) — 별도 아이템.
- 설정 토글, 로컬 블록리스트·워터마크, 방명록 페이지네이션.
- 프론트엔드 변경 — 답글은 기존 `GuestbookTab` 로드 + `groupGuestbook`으로 자연 렌더.

## A. 스캔 훅 (`src-tauri/src/pipeline.rs`)

`run_pipeline_once`의 `maybe_*` 연쇄에 `maybe_reply_guestbook(&app)` 추가.

| 단계 | 내용 | 실패 시 |
|---|---|---|
| 게이트 | settings에 `hub_url`·`hub_token`·`hub_life_id`·`hub_agent_id` 존재 + `resolve_engine` Some | no-op (네트워크 호출 전 종료) |
| 조회 | `client.guestbook(내 life_id)` 1회 | `log::warn` 후 return |
| 선정 | core `select_reply_targets(entries, my_agent_id, 3)` (§B) | — (순수 함수) |
| 생성·게시 | 후보별: core `compute_guestbook_reply`(§C) → `client.add_guestbook(life_id, reply, author_name, Some(entry_id))` | 후보 단위 `log::warn` 후 **다음 후보 계속** |

훅은 얇게(설정 읽기·HTTP·로깅만), 판정·생성 로직은 core에 — `compute_chatter_pool`과
동일한 분리로 mock Engine 단위 테스트 가능. 파이프라인은 단일 스레드 순차 실행이라
스캔 중복에 의한 이중 답글 레이스 없음.

## B. 후보 선정 (core `mascot.rs`)

`select_reply_targets(entries: &[Value], my_agent_id: &str, cap: usize) -> Vec<ReplyTarget>`
(`ReplyTarget { entry_id, author_name, body }`):

| # | 필터 | 근거 |
|---|---|---|
| 1 | top-level만 (`parent_id` 없음/null) | 답글에 답글 불가(1-depth, 서버도 400) |
| 2 | `author_agent_id != my_agent_id` | 내가/내 봇이 쓴 글 제외 |
| 3 | `parent_id == entry_id && author_agent_id == my_agent_id`인 행이 **없는** 원글만 | "글당 1회" — 서버 데이터로 dedup. 사람 주인이 수동으로 단 답글도 같은 agent_id라 자연 존중(이미 주인이 답한 글에 봇이 또 안 닮) |
| 4 | 필수 필드 누락·비문자열 행은 skip | 방어적 파싱 |

정렬: **오래된 순**(서버 최신순의 역순) — 백로그가 대화 흐름 순서로 소화.
상한 `GUESTBOOK_REPLY_MAX_PER_SCAN = 3`.

## C. 답글 생성 (core `mascot.rs`)

- `build_guestbook_reply_prompt(honorific, mbti, visitor_name, post_body)` 신규 —
  기존 페르소나 전문(다마고치 마스코트 1인칭·능청) + `mbti_voice_hint` + `voice_guidance`
  재사용. **`facts_block`(오늘 업무 요약)은 미포함** — 답글은 원글에 반응해야 하고,
  무관한 업무 얘기·수치 날조 위험만 늘림. 정밀도의 선: "원글에 없는 사실을 지어내지 말 것".
  지시: "방문자 '{visitor_name}'이 {honorific}의 미니홈피 방명록에 남긴 글에 대한
  주인장 답글 한 줄(100자 이내), 번호·따옴표 없이".
- `compute_guestbook_reply(engine, honorific, mbti, target) -> anyhow::Result<String>` —
  프롬프트 빌드 → `engine.generate` → `parse_chatter_lines(raw, 1)` 첫 줄
  (빈 결과면 Err) → 500자 방어 truncate(`chars().take(500)`, 서버 400 회피).

## D. 작성자 표기 (ADR 0020 규범 구현)

core `mascot.rs`에 순수 함수 `bot_author_name(owner_title, user_name) -> Option<String>`
— `Some("{owner_title}님의 {user_name}")` (예: "대장님의 둘쇠"). 훅이 settings에서
`owner_title`(기존 `owner_title(store)` 헬퍼, 기본 "주인")·`user_name`을 읽어 호출.

| 상황 | 동작 |
|---|---|
| `user_name` 비어 있음 | `author_name` 미전달 → 서버가 등록된 agent name으로 fallback (기존 규약) |
| 조립 결과 80자 초과 | `author_name` 미전달로 fallback (서버 400 회피) |

## E. 실패 무해 경계 · 호환

| 상황 | 동작 |
|---|---|
| hub 미연결·엔진 미설정 | 게이트에서 no-op |
| GET 실패 | warn 후 return — 스캔 나머지 무영향 |
| 후보별 LLM 실패·빈 출력·POST 실패 | warn 후 다음 후보 계속 — 다음 스캔에 자연 재시도 |
| 영구 실패 항목 | 스캔마다 재시도됨 — 상한 3이 비용 바운드 (블록리스트는 YAGNI, 수용) |
| **구서버(G2 미배포) 방어** | POST 응답은 저장된 행 그대로(`life.py:417` `dict(row)`) — 응답의 `parent_id`가 보낸 값과 다르면(구서버는 `parent_id` 무시 → 원글로 저장) **경고 후 앱 실행 동안 기능 비활성**(`AtomicBool`, `maybe_probe_docs` 전례). 방치 시 dedup이 답글을 못 찾아 스캔마다 도배되는 유일한 유해 실패 모드라 명시 차단. G2 "서버 먼저 배포" 규범의 이중 방어 |

## 테스트 (TDD)

- **core (cargo)**:
  - `select_reply_targets` — 자기 글 제외 / 답글 행 제외(top-level만) / 기답글 원글 제외 /
    상한 적용 / 오래된 순 / 필드 누락 행 skip / 빈 목록.
  - `build_guestbook_reply_prompt` — 호칭·voice·방문자명·원문 포함, facts_block 미포함,
    한 줄·길이 지시 포함.
  - `compute_guestbook_reply` — mock Engine으로 정상 파싱 / 여러 줄 출력 시 첫 줄만 /
    빈 출력 Err / 500자 truncate.
  - 표기 조립 — 정상 / `user_name` 미설정 / 80자 초과 fallback (조립을 순수 함수로 분리).
- **src-tauri**: 훅은 얇아 단위 테스트 없음(기존 `maybe_*` 관행) — 로직은 전부 core에서 검증.
- **프론트(Vitest)**: 무변경 — 기존 `groupGuestbook` 테스트가 렌더 경로 커버.

검증 환경: 권위 실행은 **Windows PowerShell**(`a-mate/`에서 `cargo test`, WSL 금지).
macOS는 부분 신호만 — 기존 실패 2건(core `hosts` 1건·`src-tauri` 링크)은 본 작업과 무관.

## 파일 터치

- **a-mate core**: `crates/core/src/mascot.rs`(`select_reply_targets`·
  `build_guestbook_reply_prompt`·`compute_guestbook_reply`·상수·표기 조립 + 테스트)
- **a-mate src-tauri**: `src-tauri/src/pipeline.rs`(`maybe_reply_guestbook` 훅 + 연쇄 등록)
- **문서**: 본 스펙 (ADR 없음 — 확정 결정 표 참조)
