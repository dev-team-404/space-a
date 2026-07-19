---
status: done
archived: 2026-07-19
---

# 다이어리 다양성 + UI 픽스 4건 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ① 브리프에 직전 며칠 일기 발췌(`recent_diaries`)를 실어 LLM이 어제와 다른 이야기를 쓰게 하고 프롬프트로 반복 금지를 지시, ② 이모지 지시를 "문단마다 1개 정도"로 상향, ③ DiaryTab 본문 카드 배경을 크림으로, ④ HomeTab 그리드 아이템 `min-width:0`으로 컬럼 고정.

**Architecture:** backend는 `crates/core/src/diary/mod.rs`에 국한 — `Brief`에 `recent_diaries` 필드+`RecentDiary` 타입, `assemble_brief`에서 vault 파일 읽어 채움, `build_system_prompt`에 지시 2건(recent_diaries·이모지). `voice_guidance` 무변경(3표면 공유). front는 DiaryTab/HomeTab `<style>`만(기존 theme.css 토큰). 재생성은 코드 밖(머지 후 diary_index 창 삭제, findings 유지).

**Tech Stack:** Rust(core diary), Svelte 5(style만), vitest/cargo test.

**Spec:** `docs/specs/2026-07-10-diary-variety-ui-fixes-design.md`

## Global Constraints

- **매 cargo 명령 전 (Git Bash) 필수:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- `voice_guidance()` 무변경 (이모지 완화는 다이어리 프롬프트에만).
- `recent_diaries`: 직전 3일·오래된 것부터·500자 char 경계 캡·토큰 푸터 제외·읽기 실패 스킵. 로컬 파일만(프라이버시 불변).
- UI는 기존 theme.css 토큰만(신규 색 금지). DiaryTab는 `article` 배경 한 곳, HomeTab는 `.grid > :global(*)` 한 줄.
- 브랜치 `feat/diary-variety-ui-fixes`(생성됨). 테스트·빌드 경고 0. 커밋 태스크 단위.

---

### Task 0: 문서 (스펙·플랜·킥오프)

- [ ] 스펙 `docs/specs/2026-07-10-diary-variety-ui-fixes-design.md`, 플랜(본 파일), 킥오프 `docs/brainstorming/2026-07-10-diary-variety-ui-fixes-kickoff.md` 커밋.

```bash
git add docs/specs/2026-07-10-diary-variety-ui-fixes-design.md docs/plans/2026-07-10-diary-variety-ui-fixes.md docs/brainstorming/2026-07-10-diary-variety-ui-fixes-kickoff.md
git commit -m "docs(diary): 다양성+UI 픽스 4건 스펙·플랜·킥오프"
```

---

### Task 1: `Brief.recent_diaries` + 프롬프트 지시 (fix 1·2, backend)

**Files:**
- Modify: `crates/core/src/diary/mod.rs` — `Brief` 구조체·신규 `RecentDiary`, `assemble_brief`, `build_system_prompt`, 테스트

**Interfaces:**
- Produces: `Brief.recent_diaries: Vec<RecentDiary>` (Serialize — 온프렘 엔진 user 메시지에 실림)
- Consumes: 기존 `store.diary_path_for(date) -> Result<Option<String>>`

- [ ] **Step 1: 실패하는 테스트 작성** (`mod tests` 안에 추가)
  - `assemble_brief_includes_recent_diaries` — persist_diary로 07-08·07-09 심고 07-10 브리프 → 2건, 오래된 것부터, 발췌에 본문 포함·"토큰" 미포함.
  - `assemble_brief_recent_diaries_absent_when_none` — 일기 없음 → 빈 벡터.
  - `assemble_brief_recent_diaries_caps_excerpt_at_500_chars` — "가"×700 → `chars().count()==500`.
  - `system_prompt_directs_recent_diary_variety` — `recent_diaries`·"되풀이하지"·"다른 이야기" 포함.
  - 기존 `system_prompt_directs_short_length_and_moderate_emoji`: `assert!(p.contains("0~1개"))` → `assert!(p.contains("문단마다 1개"))`.
  - 기존 `generate_diary_writes_md_with_token_footer_and_index`의 수동 `Brief { … }`에 `recent_diaries: vec![]` 추가.

- [ ] **Step 2: 테스트 실패 확인** — `cargo test -p agent-mentor recent_diaries` / `... system_prompt_directs` → 컴파일 실패(신규 필드·타입 없음) 후 assertion 실패.

- [ ] **Step 3: 구현**
  - `RecentDiary { date, excerpt }` 타입 + `Brief`에 `recent_diaries` 필드.
  - `collect_recent_diaries(store, today)` + `cap_chars(s, max)` 헬퍼(char 경계). 토큰 푸터 `\n\n*—` 앞까지만.
  - `assemble_brief`에서 `today` 재사용해 채우고 `Brief { …, recent_diaries }` 반환.
  - `build_system_prompt` 포맷 문자열: occasions 문단 뒤 recent_diaries 지시 문단 추가, 이모지 줄 교체.

- [ ] **Step 4: 그린 확인** — `cargo test -p agent-mentor` 전부 PASS, 경고 0.

- [ ] **Step 5: 커밋**
```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 브리프 recent_diaries 참조 + 프롬프트 다양성·이모지 지시 상향"
```

---

### Task 2: DiaryTab 본문 카드 배경 크림 (fix 3, front)

**Files:** Modify `src/lib/ui/DiaryTab.svelte` `<style>` `article` 규칙 배경 1곳.

- [ ] **Step 1: 구현** — `article { background: var(--frame-bg) … }` → `var(--pastel-cream)`.
- [ ] **Step 2: 게이트** — `npx vitest run && npm run build` (회귀 그린·빌드 성공).
- [ ] **Step 3: 커밋**
```bash
git add src/lib/ui/DiaryTab.svelte
git commit -m "fix(diary-ui): 본문 카드 배경 크림 — 흰 프레임 위 대비 확보"
```

---

### Task 3: HomeTab 그리드 컬럼 고정 (fix 4, front)

**Files:** Modify `src/lib/ui/HomeTab.svelte` `<style>` — `.grid` 뒤 `.grid > :global(*) { min-width: 0; }` 추가.

- [ ] **Step 1: 구현** — 한 줄 추가(주석 포함).
- [ ] **Step 2: 게이트** — `npx vitest run && npm run build`.
- [ ] **Step 3: 커밋**
```bash
git add src/lib/ui/HomeTab.svelte
git commit -m "fix(home): 그리드 아이템 min-width:0 — 긴 top3에 컬럼 밀림·가로 스크롤 해소"
```

---

## 최종 게이트 + 마무리

- [ ] 전체 그린: `cargo test -p agent-mentor && cargo build -p agent-mentor-app` + 프론트 게이트, 경고 0.
- [ ] push + PR(base=main).
- [ ] (머지 후, 코드 밖) 재생성 — 스펙 §2e: 앱 종료 → `diary_index` 창(−7~−1) 삭제(**findings 유지**) → 실엔진으로 앱 실행 → backfill 재생성 → 육안(다른 일기·이모지·크림 카드·홈 그리드 고정).
