---
status: done
archived: 2026-07-26
---

# G1 주인 신원 체계 설계 — 호칭 표시 + 풀네임 식별

- **날짜**: 2026-07-26
- **컴포넌트**: a-mate (+ a-hub life 방명록 API 소폭)
- **브랜치**: `feat/amate-owner-identity` (worktree)
- **관계**: [후속 로드맵 2차 배치 G1](../plans/2026-07-26-life-social-diary-followups-roadmap.md) 착수분.
  PR #104의 신원 배관([아카이브 스펙](2026-07-26-bot-tab-mascot-identity-design.md) §F) 위 확장.
  프라이버시 경계 결정은 [ADR 0020](../../../../adr/0020-owner-fullname-to-hub.md).

## 배경 — 현재 상태 (조사 결과)

신원 설정이 셋으로 나뉘어 있고, **주인(사람)을 식별하는 이름은 없다**:

| 키 | 의미 | 쓰이는 곳 |
|---|---|---|
| `user_name` | 봇(마스코트) 이름 | Life `name`(register/rename), 방 헤더 `App.svelte:161` "{name}님의 미니홈피", 방명록 작성자(아래) |
| `owner_title` | 호칭 (기본 "주인", PR #104) | 프롬프트·대사에서 주인 지칭 — 표시 전용 |
| `owner_os_user` | OS 계정명 (PR #104 §F) | register/rename payload의 숨은 식별자 (서버 미소비 forward 필드) |

- **방명록 작성자**: a-hub `add_guestbook`(`life.py:383`)이 `author_name`을 **서버에서 등록된
  agent name(=봇 이름)으로 파생**한다. 클라이언트가 작성자 문자열을 넘길 방법이 없어, 사람이
  남겨도 봇이 남겨도 봇 이름으로 찍힌다. `GuestbookTab.svelte`는 `entry.author_name`을 그대로 표시.
- **호칭의 한계**: "주인"·"대장" 같은 호칭은 여러 사용자가 같을 수 있어 사람 구분이 안 된다.
- **contracts/**: C1(MCP)·C2(시각화 REST)·C4(admin)만 있고 **Life REST API는 계약 범위 밖** —
  payload 규약의 기록처는 본 스펙 + ADR 0020이다.

## 확정 결정 (브레인스토밍 Q&A)

| 질문 | 결정 |
|---|---|
| 사람이 직접 남긴 방명록 작성자 표시 | **풀네임만** (예: "홍길동") |
| 방 헤더 "{name}님의 미니홈피" | **봇 이름 유지** (PR #104 '나=봇' 결정 존속; 풀네임은 식별 용도에만) |
| register/rename payload 구성 | **`owner_full_name` 추가, `owner_os_user` 유지** (역할 분리: 자동 식별자 vs 사람이 읽는 신원) |
| 작성자 이름 조립 위치 | **클라이언트(a-mate) 조립** — 서버는 optional `author_name` 수용 + 검증만 |
| 실명 전송·저장 경계 | **ADR 0020으로 기록** |

## 목표

1. 주인 풀네임(실명)을 새 로컬 설정 `owner_full_name`으로 저장 — 설정 UI + 프로필 API.
2. Life register/rename payload에 `owner_full_name`을 forward 필드로 실음.
3. a-hub 방명록 POST에 optional `author_name` 추가, 사람 작성 경로가 풀네임을 전달.
4. 봇 작성자 포맷 `{owner_title}님의 {user_name}`을 규범으로 확정 (구현은 P3/G3).

## 비목표

- a-hub가 `owner_full_name`을 저장·소비 (G2/G3에서 필요 시 승격 — YAGNI).
- 봇 자동 방명록 작성 경로 구현 (P3/G3 — 지금 만들면 죽은 코드).
- 방 헤더·점유자 라벨 등 기존 표시 변경 (G4에서 이름 표기 정합).
- `user_name`/`owner_title`/`owner_os_user`의 의미 변경.

## 데이터 (로컬 설정 키)

| 키 | 내용 | 기본 | 비고 |
|---|---|---|---|
| `owner_full_name` | **신설** — 주인 풀네임(실명) | 빈값(미설정) | trim 저장, 빈값 허용(=옵트인). 마이그레이션 없음 |

## A. 설정 · 프로필 API (a-mate)

- `commands.rs`: `Profile` 구조체에 `owner_full_name: String` 추가.
  `profile_get` 반환 포함, `profile_set` 저장(trim, 빈값이면 미설정으로 저장).
- `MeGroup.svelte` "마스코트 정보" 섹션: **"주인 이름"** 입력 필드 신설 — 호칭 필드 옆,
  placeholder "홍길동", `maxlength="80"`(서버 방명록 한도와 정합 — §D의 400을 UI에서 예방),
  힌트 *"실명 — 방명록 서명과 신원 확인에 쓰여요 (선택)"*.
- `api.ts`: `Profile` 타입·`profileSet` 인자에 `owner_full_name` 추가.

## B. Life payload (a-mate → a-hub, forward 필드)

- `life_client.rs`: `register_profile`·`rename` body에 `owner_full_name` 추가 —
  **빈값이면 생략, 서버는 모르는 필드를 무시**(하위호환). `owner_os_user`와 동일 규약.
- 호출 지점 시그니처 갱신: `commands.rs`의 재연결 rename(`:740` 부근), register(`:769` 부근),
  `profile_set`의 rename(`:1623` 부근). 이름·풀네임 어느 쪽이 바뀌어도 rename에 최신 값 동봉.
- 서버 저장·소비는 이번 범위 밖 — payload에 실리는 것까지가 G1.

## C. 방명록 작성자 (a-hub + a-mate)

**a-hub** (`api.py`, `life.py`):

- `POST /life/{life_id}/guestbook` body에 **optional `author_name`** 추가.
- 규칙: 미제공 또는 trim 후 빈값 → 현행 `author.name`(봇 이름) fallback /
  trim 후 1~80자 → 그대로 저장 / 80자 초과 → `InvalidRequest`(400).
- GET·DELETE·권한(`author_agent_id` 기준) 불변. guestbook 스키마 불변(컬럼 기존).

**a-mate** (`commands.rs`, `life_client.rs`):

- `life_add_guestbook` 커맨드가 설정에서 `owner_full_name`을 읽어, 비어있지 않으면
  `author_name`으로 전달. **프론트 `GuestbookTab.svelte`·`api.ts` 방명록 경로는 무변경**
  (조립이 커맨드 층이므로).
- `life_client.rs::add_guestbook`에 `author_name: Option<&str>` 파라미터 추가.

**봇 작성 포맷 (규범만, 구현은 P3/G3)**: 봇이 자동으로 남길 때 작성자는
`{owner_title}님의 {user_name}` (예: "대장님의 둘쇠"). P3/G3 구현 시 본 스펙을 따른다.

## D. 에러 처리 · 호환 · 폴백

| 상황 | 동작 |
|---|---|
| 신클라 + 구서버 (author_name 미지원) | Pydantic이 모르는 필드 무시 → 봇 이름 fallback, 무해 |
| 구클라 + 신서버 | 필드 없음 → 봇 이름 fallback (현행 동일) |
| `owner_full_name` 미설정 | 방명록 = 봇 이름(현행), payload = 필드 생략 — 어떤 경로도 실패하지 않음 |
| author_name 80자 초과 | 서버 400. a-mate는 설정 저장 시 trim만 하고 길이 제한은 서버 응답에 위임 |

## E. 프라이버시 — ADR 0020

실명이 ① 허브로 전송되기 시작하고 ② 타인 방 방명록 행에 **영구 스냅샷**으로 저장된다
(개명·설정 삭제해도 소급 수정 없음, 항목 삭제로만 제거). 옵트인(빈값 기본)이며,
LLM 엔진 경계(ADR 0019)와는 별개의 허브 경계 결정 — 상세·대안은
[ADR 0020](../../../../adr/0020-owner-fullname-to-hub.md).

## 테스트 (TDD)

- **a-hub (pytest)**: `author_name` 전달 시 저장·반환 / 생략 시 `author.name` fallback /
  공백만 → fallback / 80자 초과 → 400 / 삭제 권한 불변.
- **Rust (cargo)**: `owner_full_name` 설정 왕복·trim / `profile_get`·`profile_set` 포함 /
  `register_profile`·`rename` body에 포함·빈값 생략 / `add_guestbook` body에
  `author_name` 포함·`None` 생략.
- **프론트**: 컴포넌트 렌더 테스트 인프라가 없어(Vitest는 순수 `.ts` 모듈만 — 레포 관행)
  Svelte 필드 렌더 테스트는 두지 않는다. 값 왕복은 Rust `profile` 테스트가 커버하고,
  기존 Vitest 스위트 무회귀만 확인한다.

검증 환경: 권위 실행은 **Windows PowerShell**(`cargo test` 워크스페이스 · `npm test`).
macOS 로컬은 부분 신호만 — core 크레이트의 `hosts` 테스트 1건(Windows 경로 기대)과
`src-tauri` 링크는 macOS에서 원래 실패한다(기존 이슈, 본 작업과 무관).

## 파일 터치

- **a-mate 프론트**: `src/lib/ui/settings/MeGroup.svelte`, `src/lib/api.ts`
- **a-mate Rust**: `src-tauri/src/commands.rs`(Profile·profile_get/set·life_add_guestbook·rename/register 호출), `crates/core/src/life_client.rs`(register_profile·rename·add_guestbook body)
- **a-hub**: `life/life_server/api.py`(body 모델), `life/life_server/life.py`(add_guestbook)
- **문서**: 본 스펙, `docs/adr/0020-owner-fullname-to-hub.md`
