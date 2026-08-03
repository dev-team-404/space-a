# 코칭 탭 단일 스트림 PR A (Rust) 구현 계획 — `first_seen` 노출과 보존

> **근거 스펙:** [2026-08-02-coaching-tab-single-stream-design.md](../specs/2026-08-02-coaching-tab-single-stream-design.md) §3.1~§3.3(A 명세) · §5(PR 분할) · §2.2·§2.3(A가 가능하게 하는 것)
> **범위:** Rust만. 프론트(`api.ts` 타입·정렬·배지·홈 위젯)는 **B의 몫**이다.

**Goal:** 코칭 스트림이 `first_seen` 하나로 최신순 정렬(§2.2)과 안 본 개수 배지(§2.3)를 만들 수 있게, Rust가 그 값을 **노출**하고 콘텐츠 프룬으로부터 **보존**한다.

**Architecture:** 두 갈래다.
1. `FindingRow`·`ContentRow`에 `first_seen` serde 필드를 더한다 — 두 테이블에 컬럼이 이미 있고 각 `ON CONFLICT`가 갱신하지 않으므로(§3.1) 노출만 하면 값은 신뢰할 수 있다.
2. 콘텐츠는 프룬이 행을 지워 재삽입 시 `first_seen`이 리셋된다(§3.2). `content_first_seen(id, ts)` 보존 테이블을 두고 큐레이션 때 `INSERT OR IGNORE`, 조회 때 `LEFT JOIN`으로 원래 값을 되살린다.

**Tech Stack:** Rust · rusqlite · serde · `crates/core/src/store.rs` 단일 파일

## Global Constraints

- **마이그레이션에 `DELETE FROM events/sessions/ingest_state/daily_rollup`을 붙이지 않는다.** 릴리스마다 콜드 스캔이 되돌아온 사고(#146)의 원인이다. `judgment_json`·`summary_ko` 선례대로 스키마 추가만.
- **`content_items.status`에 `'stale'`을 추가하지 않는다** — ③(#155)이 같은 어휘에 `resolved`를 넣었다. 별도 테이블이 그 자리를 피한다(§3.2).
- **락 규율:** 큐레이션 3단(fetch=락 밖 / persist=짧은 락)을 유지한다. 보존 테이블 쓰기는 `replace_content_items`의 기존 트랜잭션 안에서 한다 — 호출부(`pipeline.rs`)를 건드리지 않는다.
- **`pipeline.rs`의 before/`run_rules`/after는 한 락 블록을 유지한다**(③이 만든 제약). A는 이 블록을 건드리지 않는다.
- 빌드·테스트는 네이티브 Windows PowerShell에서. WSL 금지.

**베이스라인(이 워크트리에서 실측, `2d0ac95`):** `npm test` = 296 passed / 29 files(`Tests N passed` 줄 확인 — node_modules 없이 돌면 거짓 통과한다). `cargo test` = 660(core) + 48(app) = 708 passed / 0 failed.

---

## Task 1: `FindingRow.first_seen` 노출

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` — `FindingRow` 구조체, `list_findings_current`
- Test: 같은 파일 `mod tests`

`CoachFinding`(`src-tauri/src/commands.rs`)이 `#[serde(flatten)] row: FindingRow`라 구조체 필드 추가만으로 커맨드 payload에 실린다.

**Interfaces:**
- Produces: `FindingRow.first_seen: Option<String>` — B의 정렬·배지 재료.

- [ ] **Step 1: 실패 테스트** — 직렬화 payload에 실리고, 재스캔(`ON CONFLICT`) 후에도 원래 값을 지킨다
- [ ] **Step 2: RED 확인** — `cargo test -p agent_mentor finding_first_seen` → 컴파일 실패(필드 없음)
- [ ] **Step 3: 최소 구현** — `FindingRow`에 필드 추가, `list_findings_current`의 SELECT·매핑에 `first_seen` 추가
- [ ] **Step 4: GREEN 확인**
- [ ] **Step 5: 커밋** — `feat(agent): expose finding first_seen to the frontend`

---

## Task 2: `content_first_seen` 보존 테이블 + `ContentRow.first_seen` 노출

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` — `SCHEMA`, `replace_content_items`, `CONTENT_COLS`, `content_row_from`, `list_content`, `content_needing_translation`, `ContentRow`
- Test: 같은 파일 `mod tests`

**핵심 설계:**

```sql
CREATE TABLE IF NOT EXISTS content_first_seen (id TEXT PRIMARY KEY, ts TEXT NOT NULL);
```

- 큐레이션(`replace_content_items`): 랭킹 아이템마다 `INSERT OR IGNORE INTO content_first_seen (id, ts) VALUES (?, now_ts)` — 기존 트랜잭션 안.
- 조회: `FROM content_items c LEFT JOIN content_first_seen fs ON fs.id = c.id`, 컬럼은 `COALESCE(fs.ts, c.first_seen) AS first_seen`.
  - `id`가 양쪽 테이블에 있으므로 **모든 컬럼·`ORDER BY`에 `c.` 접두사를 붙인다** — 없으면 `ambiguous column name` 런타임 에러.
- `COALESCE` 폴백이 보존 테이블에 없는 묵은 행(마이그레이션 이전)을 받아낸다.

**⚠ id 고정·내용 갱신 소스 함정 (§3.2 끝, ⑥ Codex P1):** `cc-changelog-latest`는 id가 고정이고 새 릴리스마다 `title`·`body`만 바뀐다. ⑥은 번역 캐시를 id만 보고 보존했다가 화면이 영영 낡는 버그를 냈다. **`first_seen`은 반대로 유지가 맞다** — 의미가 "처음 본 시각"이고, 내용이 갱신됐다고 그 시각이 바뀌지는 않는다. 대가로 **내용이 새로워진 고정-id 소식은 최신순 스트림 하단에 남는다.** 이 트레이드오프는 의도된 것이고, 그 소스의 노출은 §4 질문 4(고정 슬롯)가 담당한다. 코드 주석과 PR 본문에 명시로 남긴다.

**Interfaces:**
- Produces: `ContentRow.first_seen: Option<String>`

두 단계로 나눠 돈다 — 노출이 먼저 GREEN이 돼야 보존 테스트가 "값이 틀렸다"로 실패할 수 있다(테이블 없이 실패하면 컴파일 에러라 RED의 질이 떨어진다).

**2-1 노출**
- [ ] **Step 1: 실패 테스트** — 큐레이션 1회 뒤 `list_content` 행의 직렬화 payload에 `first_seen`이 실린다
- [ ] **Step 2: RED 확인** — `first_seen`이 `null`이라 실패
- [ ] **Step 3: 최소 구현** — `ContentRow.first_seen` + `CONTENT_COLS`에 `c.first_seen` (보존 테이블 없이)
- [ ] **Step 4: GREEN 확인**

**2-2 보존**
- [ ] **Step 5: 실패 테스트(A의 핵심)** — 프룬으로 행이 지워졌다 돌아와도 `first_seen`이 원래 값
- [ ] **Step 6: RED 확인** — 재삽입 시각이 나와 실패
- [ ] **Step 7: 최소 구현** — 보존 테이블 + `INSERT OR IGNORE` + `LEFT JOIN`/`COALESCE`
- [ ] **Step 8: GREEN 확인** — 콘텐츠 관련 기존 테스트 전부 포함
- [ ] **Step 9: 커밋** — `feat(agent): preserve content first_seen across prune`

---

## Task 3: 마이그레이션 — 시딩 + 행 수 보존 검증

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` — `migrate`
- Test: 같은 파일 `mod tests`

`open`이 `SCHEMA`(→`CREATE TABLE IF NOT EXISTS`) 뒤에 `migrate`를 부르므로 테이블 자체는 기존 DB에서도 생긴다. `migrate`가 할 일은 **기존 `content_items.first_seen` 백필**이다:

```sql
INSERT OR IGNORE INTO content_first_seen (id, ts)
  SELECT id, first_seen FROM content_items WHERE first_seen IS NOT NULL;
```

백필이 없으면 기존 사용자는 마이그레이션 후 **첫 프룬에서** `first_seen`을 잃는다(`INSERT OR IGNORE`가 그때의 `now_ts`를 찍으므로) — A가 고치려는 버그가 그대로 남는다. `INSERT OR IGNORE`라 idempotent하므로 버전 게이트 없이 매 실행 무해하다.

- [ ] **Step 1: 실패 테스트** — 구 스키마 DB(보존 테이블 없음, 행 있음)를 열면 ① `events`/`sessions`/`ingest_state`/`daily_rollup` 행 수가 **테이블별로** 보존되고 ② 기존 `first_seen`이 백필된다
- [ ] **Step 2: RED 확인**
- [ ] **Step 3: 최소 구현**
- [ ] **Step 4: GREEN 확인** — `cargo test` 전체
- [ ] **Step 5: 커밋** — `feat(agent): backfill content first_seen without recollecting`

---

## Task 4: 뮤테이션 규율 점검

구현 전에 통과해 버린 가드가 있으면 조건을 뒤집어 실패하는지 보고 되돌린다.

**⚠ ③ 실측 교훈:** 미검출이 나오면 "가드가 중복이네"로 넘기지 말고 **그 테스트가 이름값을 하는지** 다시 본다 — 다른 가드에 가려 정작 주장하는 걸 증명하지 못하고 있을 수 있다.

- [ ] Task 1~3의 각 테스트에 대해 구현을 한 군데씩 뒤집어 RED를 재확인하고 되돌린다
- [ ] `cargo test` · `npm test` 전체 녹색

---

## Task 5: DoD

- [ ] `docs-archive` 스킬로 **이 계획 문서**를 아카이브 (ADR 0013). 단일 스트림 **스펙 자체는 B가 남았으므로 아카이브하지 않는다.**
- [ ] PR 생성 — 제목·본문 한국어

## A 다음에 남는 것

- **B (프론트)** — 단일 스트림·3분류·라인 색·탭 배지·홈 위젯. A + §4 미결 질문 4개 해소가 선행. 특히 **질문 4(고정 슬롯 vs 순수 최신순 정렬)** 는 Task 2의 트레이드오프와 정면으로 맞물리므로 B 계획 전에 정해야 한다.
- `api.ts`의 `Finding`·`ContentItem`에 `first_seen` 타입 추가 — B의 첫 걸음.
