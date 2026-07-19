---
status: done
archived: 2026-07-19
---

# 다이어리 가독성 개선 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 다이어리를 짧게(2~3문단·350자 이내)·이모지 적당히 쓰도록 프롬프트를 보강하고, DiaryTab 본문을 카드+가독 타이포그래피로 바꾼다.

**Architecture:** 프롬프트는 `build_system_prompt` 끝에 형식 지시 문단 1개 추가(공유 조각 `voice_guidance` 불변 — 한마디·잡담 무영향). UI는 DiaryTab.svelte `<style>`의 `article` 규칙만 확장(기존 theme.css 토큰 재사용). 재생성은 코드 밖 절차(머지 후 앱DB diary_index 삭제 → startup backfill).

**Tech Stack:** Rust(core diary), Svelte 5(style만), vitest/cargo test.

**Spec:** `docs/specs/2026-07-10-diary-readability-design.md`

## Global Constraints

- **매 cargo 명령 전 (Git Bash) 필수:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- `voice_guidance()` 무변경 (3표면 공유 — 이모지 완화는 다이어리 프롬프트에만).
- 길이 지시 verbatim: "2~3문단, 전체 350자 이내" · 이모지 지시 verbatim: "문단당 0~1개".
- UI는 기존 theme.css 토큰만(신규 색·토큰 금지), `.body`·`.empty`·달력 무변경.
- 브랜치 `feat/diary-readability`(생성됨, 스펙 커밋 1291d75). 테스트·빌드 경고 0.

---

### Task 1: 프롬프트 형식 지시 (길이·이모지)

**Files:**
- Modify: `crates/core/src/diary/mod.rs` — `build_system_prompt`(302–327행) 포맷 문자열 끝, 테스트는 `system_prompt_has_humor_evidence_and_occasions_instructions`(573행 부근) 뒤

**Interfaces:**
- Consumes: 기존 `build_system_prompt(cfg) -> String`
- Produces: 없음 (프롬프트 내용 변경만 — 시그니처 불변)

- [ ] **Step 1: 실패하는 테스트 작성**

`system_prompt_has_humor_evidence_and_occasions_instructions` 테스트 뒤에 추가:

```rust
    #[test]
    fn system_prompt_directs_short_length_and_moderate_emoji() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("2~3문단"));   // 길이 상한(문단)
        assert!(p.contains("350자"));     // 길이 상한(글자)
        assert!(p.contains("골라"));      // 핵심만 골라 쓰기(장황함 차단)
        assert!(p.contains("이모지"));    // 이모지 지시
        assert!(p.contains("0~1개"));     // 문단당 사용량
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run (빌드 레시피 export 후):
```bash
cargo test -p agent-mentor system_prompt_directs_short
```
Expected: FAIL — `p.contains("2~3문단")` assertion 실패

- [ ] **Step 3: 구현**

`build_system_prompt` 포맷 문자열의 occasions 문단 끝을 다음으로 교체
(`비어있으면 언급하지 마세요."` → 형식 문단 추가):

```rust
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요(예: 오늘이 크리스마스이거나 함께한 지 100일 등). \
         비어있으면 언급하지 마세요. \
         \
         형식: 일기는 짧게 — 2~3문단, 전체 350자 이내로 쓰세요. \
         그날의 핵심 한두 가지만 골라 쓰고 나머지 사실은 과감히 버리세요. \
         이모지는 적당히 — 문단당 0~1개, 감정이 실리는 자리에만 쓰세요.",
```

- [ ] **Step 4: 테스트 통과 확인 (기존 프롬프트 마커 회귀 포함)**

```bash
cargo test -p agent-mentor system_prompt
```
Expected: 신규 1개 + 기존 `system_prompt_*` 3개 전부 PASS. 이어서:
```bash
cargo test -p agent-mentor
```
Expected: 전부 PASS, 경고 0

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 프롬프트 형식 지시 — 2~3문단·350자·이모지 적당히"
```

---

### Task 2: DiaryTab 본문 카드 + 타이포그래피

**Files:**
- Modify: `src/lib/ui/DiaryTab.svelte` — `<style>` 블록만(104행 `article :global(h1...)` 규칙 앞뒤)

**Interfaces:**
- Consumes: theme.css 토큰(`--frame-bg`, `--radius-m`, `--shadow-soft`)
- Produces: 없음 (스타일만)

- [ ] **Step 1: 구현**

`<style>`의 `.empty` 규칙 뒤·기존 `article :global(h1...)` 규칙 앞에 추가:

```css
  article {
    background: var(--frame-bg); border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft); padding: 18px 22px;
    max-width: 62ch; line-height: 1.75;
  }
  article :global(p + p) { margin-top: 0.9em; }
```

(마크업·`.body`·`.empty`·달력 무변경. 기존 `article :global(h1,h2,h3)` 규칙 유지.)

- [ ] **Step 2: 게이트**

```bash
npx vitest run && npm run build
```
Expected: vitest 49/49 PASS(회귀 — 신규 테스트 없음, style만), build 성공

- [ ] **Step 3: 커밋**

```bash
git add src/lib/ui/DiaryTab.svelte
git commit -m "feat(diary-ui): 본문 카드화 + 줄간격 1.75·문단 간격·가독 폭"
```

---

## 최종 게이트 + 마무리

- [ ] 전체 그린: `cargo test -p agent-mentor && cargo build -p agent-mentor-app` + 위 프론트 게이트, 경고 0
- [ ] push + PR(base=main)
- [ ] (머지 후, 코드 밖) 재생성 절차 — 스펙 §2c: 앱 종료 → `%APPDATA%\dev.agentmentor.app\agent-mentor.db` `diary_index` 삭제 → 실엔진으로 앱 실행 → backfill 7일 창 재생성 → 육안 (#17 한마디·#18 잡담 E2E 동시)
