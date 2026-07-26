---
status: done
archived: 2026-07-27
---

# G2 방명록 답글 설계 — 1단계, 중첩 불가

- **날짜**: 2026-07-26
- **컴포넌트**: a-hub life (+ a-mate 방명록 UI·클라이언트)
- **브랜치**: `feat/guestbook-replies` (worktree)
- **관계**: [후속 로드맵 2차 배치 G2](../../../../design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md) 착수분.
  G1(주인 신원, PR #106 — [아카이브 스펙](2026-07-26-owner-identity-design.md))과
  [ADR 0020](../../../../adr/0020-owner-fullname-to-hub.md) 위 확장.
  스키마·의미론 결정은 [ADR 0021](../../../../adr/0021-guestbook-replies-one-depth.md).

## 배경 — 현재 상태 (조사 결과)

- a-hub life 방명록은 **평면 목록만**: 테이블
  `guestbook(entry_id, life_id, author_agent_id, author_name, body, created_at)`
  (`store.py:75-82`), parent/thread 필드 없음.
- API (`api.py:202-212`, 도메인 `life.py:377-419`):
  `GET /life/{life_id}/guestbook`(무인증, 최신순) /
  `POST /life/{life_id}/guestbook`(인증, 본문 1~500자, optional `author_name` ≤80자 — G1) /
  `DELETE /life/guestbook/{entry_id}`(작성자·방 주인만).
- 영속화: 인메모리 리스트가 원본, opt-in SQLite(`LIFE_SERVER_DB`)가 미러
  (`save_guestbook_entry`/`delete_guestbook_entry`). 마이그레이션 패턴은
  `PRAGMA table_info` + `ALTER TABLE ... ADD COLUMN`이 확립돼 있음 (`store.py:94-116`).
- a-mate 경로: `GuestbookTab.svelte`(평면 렌더) → `api.ts:224-226` →
  `commands.rs:949-969`(G1 `owner_full_name`→`author_name` 조립) → `life_client.rs:233`.
- **contracts/**: Life REST는 계약 범위 밖(ADR 0020 전례) — 규약의 기록처는 본 스펙 + ADR 0021.

## 확정 결정 (브레인스토밍 Q&A)

| 질문 | 결정 |
|---|---|
| 답글 권한 | **방 주인만** (`author.agent_id == life.owner_agent_id`) — 1-depth 취지(주인장 답글)와 정합, 실소비자가 G3 주인 봇뿐 |
| 원글 삭제 시 답글 | **함께 삭제(cascade)** — 고아 행 없음 |
| 원글당 답글 수 | **여러 개 허용** — 서버 무제한, G3 "글당 1회" 도배 방지는 클라이언트 책임 |
| API 형태 | **기존 POST 확장**(optional `parent_id`) + **평면 GET**(행에 `parent_id` 노출, 클라 그룹핑) + DELETE cascade |
| 답글 검증 규칙 | 본문 1~500자·`author_name` G1 규칙 **그대로 재사용** — 답글 전용 규칙 신설 없음 |
| 스키마 기록 | 배포된 공유 서버의 영속 테이블 변경 → ADR 0021 |

## 목표

1. a-hub guestbook에 `parent_id` 스키마 + SQLite 마이그레이션.
2. `add_guestbook`의 1-depth·방 주인 검증, `delete_guestbook`의 cascade.
3. a-mate 답글 클라이언트 경로(`life_client.rs`→`commands.rs`→`api.ts`)와
   `GuestbookTab.svelte` 답글 UI.

## 비목표

- G3 봇 자동 답글 (별도 아이템 — 본 스펙은 인프라만).
- 2단계 이상 중첩, 손님(비주인) 답글 권한 — 완화는 새 ADR로.
- capabilities 플래그·`life_protocol` 범프 (배포 순서 규범으로 대체 — §D).
- 방명록 페이지네이션·인바운드 알림(G3에서 별도 판단).

## A. a-hub 스키마·store (`store.py`)

| 변경 | 내용 |
|---|---|
| `_SCHEMA` | `guestbook`에 `parent_id TEXT` (nullable, top-level은 NULL) — 신규 DB용 |
| `__init__` | `PRAGMA table_info(guestbook)`에 `parent_id` 없으면 `ALTER TABLE guestbook ADD COLUMN parent_id TEXT` — 기존 `agents` 마이그레이션 패턴 그대로 |
| `save_guestbook_entry` | positional `INSERT INTO guestbook VALUES (...)`를 **명시 컬럼 INSERT**로 전환(향후 컬럼 추가에 안전) + `parent_id` 포함 |
| `load_social` | SELECT에 `parent_id` 포함 → 레거시 행은 `None` (모든 행 dict가 `parent_id` 키 보유) |
| `delete_guestbook_entry` | `DELETE FROM guestbook WHERE entry_id = ? OR parent_id = ?` — 1-depth이므로 재귀 불필요 |

## B. a-hub 도메인·API (`life.py`, `api.py`)

- `GuestbookAddBody`에 `parent_id: str | None = None` 추가, 엔드포인트가 도메인으로 전달.
- `add_guestbook(token, life_id, body, author_name=None, parent_id=None)` 검증 순서
  (기존 본문·`author_name` 검증 무변경, 이하 락 안에서):

| # | 검사 | 실패 시 |
|---|---|---|
| 1 | `life_id` 존재 (기존) | 404 `not_found` |
| 2 | 부모 행 존재 **and** `parent["life_id"] == life_id` | 404 `not_found` |
| 3 | `parent["parent_id"]`가 `None` (답글의 답글 금지) | 400 `invalid_request` |
| 4 | `author.agent_id == life.owner_agent_id` (방 주인만 답글) | 403 `forbidden` |

**#2~#4는 `parent_id` 제공 시에만** 수행 — `parent_id` 없는 원글 작성은 기존과 완전 동일
(인증된 누구나, 추가 검사 없음).

- 저장 행에 `"parent_id"` 키 추가 (top-level은 `None`). `guestbook()` GET 경로는
  **무변경** — `dict(row)`에 `parent_id`가 자연 포함돼 평면 반환.
- `delete_guestbook`: 기존 권한 체크(작성자·방 주인) 후 대상 행 +
  `parent_id == entry_id`인 답글들을 인메모리·store에서 함께 제거.
  응답 `{"entry_id": ..., "deleted": true}` 유지. 답글 단독 삭제는 기존 규칙으로 자연 동작
  (답글 작성자 = 방 주인).
- 에러 매핑은 기존 `_STATUS`(400/403/404)로 충분 — `api.py` 추가 변경 없음.

## C. a-mate 클라이언트

**Rust** (G1과 동일한 층 구조):

- `life_client.rs::add_guestbook`에 `parent_id: Option<&str>` 추가 —
  `Some`일 때만 body에 `parent_id` 포함(빈값 생략 규약, `owner_full_name`과 동일).
- `commands.rs::life_add_guestbook`에 `parent_id: Option<String>` 추가 —
  프론트가 안 넘기면 `None`. `owner_full_name`→`author_name` 조립은 기존 그대로
  (사람 주인의 수동 답글도 자동으로 풀네임 서명 — G1 규범).

**프론트**:

- `api.ts`: `GuestbookEntry`에 `parent_id?: string | null`,
  `lifeAddGuestbook(lifeId, body, parentId?)`.
- **`guestbook.ts` 신설(순수 모듈)**: `groupGuestbook(entries)` —
  top-level(서버 순서 = 최신순 유지) + 원글별 답글 목록(**오래된 순** = 대화 흐름).
  부모가 목록에 없는 답글(정상 흐름엔 없음 — cascade)은 방어적으로 top-level 폴백.
  Vitest는 순수 `.ts`만 테스트하는 레포 관행에 맞춰 그룹핑을 컴포넌트 밖으로 분리.
- `GuestbookTab.svelte`: 원글 아래 답글 들여쓰기 렌더.
  **답글 버튼은 `isOwner`일 때만** 원글에 표시(답글엔 버튼 없음 — 1-depth),
  인라인 답글 입력(한 번에 하나만 열림), 답글 삭제는 기존 규칙
  (`isOwner || author_agent_id === meId`) 재사용.

## D. 에러 처리 · 호환 · 배포 순서

| 상황 | 동작 |
|---|---|
| 신클라 + 구서버 | Pydantic이 `parent_id` 무시 → **답글이 원글로 저장**. 방지책: **서버 먼저 배포**를 규범으로 명시(OCI 단일 테스트 서버 — `life/DEPLOY.md`) |
| 구클라 + 신서버 | 답글이 평면 목록의 일반 항목으로 표시될 뿐 — 무해 |
| 레거시 행 (마이그레이션 전 데이터) | `parent_id=None`으로 로드 → top-level 취급 |
| 부모 미존재·타 방 `parent_id` | 404 |
| 답글의 답글 | 400 |
| 비주인의 답글 시도 | 403 (a-mate UI도 `isOwner`로 버튼 자체를 숨김 — 이중 방어) |

## 테스트 (TDD)

- **a-hub (pytest)**: 주인 답글 저장·GET 행에 `parent_id` 포함 / 비주인 403 /
  답글의 답글 400 / 부모 미존재·타 방 404 / 원글 삭제 시 답글 cascade
  (인메모리 + SQLite 재로드 양쪽) / 답글 단독 삭제 / 원글당 답글 여러 개 /
  레거시 행 `parent_id=None` / 기존 DB 마이그레이션(컬럼 추가) /
  기존 방명록 케이스 무회귀.
- **a-mate (cargo)**: `add_guestbook` body — `parent_id` `Some` 포함 / `None` 생략
  (G1 body 테스트 패턴 재사용).
- **a-mate (Vitest)**: `groupGuestbook` — 그룹핑 · top-level 최신순 유지 ·
  답글 오래된 순 · 고아 답글 top-level 폴백.
- Svelte 컴포넌트 렌더 테스트는 두지 않음(레포 관행 — G1 스펙과 동일).

검증 환경: 권위 실행은 **Windows PowerShell**(`a-mate/`에서 `cargo test`·`npm test`,
WSL 금지). a-hub는 `a-hub/life/`에서
`uv run --no-project --with "pytest>=8" --with "httpx>=0.27" --with-editable . python -m pytest tests/ -q`.
macOS는 부분 신호만 — 기존 실패 2건(core `hosts` 테스트 1건 Windows 경로 기대,
`src-tauri` 테스트 바이너리 링크 실패)은 본 작업과 무관.

## 파일 터치

- **a-hub**: `life/life_server/store.py`(스키마·마이그레이션·INSERT·cascade),
  `life/life_server/life.py`(add_guestbook 검증·delete cascade),
  `life/life_server/api.py`(`GuestbookAddBody`), `life/tests/`(신규 케이스)
- **a-mate Rust**: `crates/core/src/life_client.rs`(add_guestbook 시그니처·body),
  `src-tauri/src/commands.rs`(life_add_guestbook 인자)
- **a-mate 프론트**: `src/lib/api.ts`, `src/lib/guestbook.ts`(신설),
  `src/lib/ui/GuestbookTab.svelte`, `src/lib/guestbook.test.ts`(신설)
- **문서**: 본 스펙, `docs/adr/0021-guestbook-replies-one-depth.md`
