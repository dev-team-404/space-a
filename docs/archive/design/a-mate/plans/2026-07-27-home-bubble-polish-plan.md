---
status: done
archived: 2026-07-27
---

# 홈·말풍선 소품 일괄 (묶음 ①: P5 + H1 + H3) 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 말풍선 한국어 어절 줄바꿈(P5), 홈 aside "오늘의 한마디" 단일화(H1), 잡담 LLM 우선화(H3)를 커밋 3개·PR 1개로 배송.

**Architecture:** P5·H1은 Svelte 컴포넌트 CSS/마크업 국소 수정(로직 무변경). H3는 `bubble.ts`의 후보 구성 함수 1곳 + `crates/core/src/mascot.rs`의 상수·프롬프트 수정 — 파이프라인/커맨드 배선은 불변.

**Tech Stack:** Svelte 5 + Vite + TypeScript(Vitest), Rust(cargo test). 스펙: [2026-07-27-home-bubble-polish-design.md](../specs/2026-07-27-home-bubble-polish-design.md)

## Global Constraints

- 작업 디렉토리: 격리 워크트리 `D:\Project\space-a\.claude\worktrees\feat+home-bubble-polish`, 브랜치 `feat/home-bubble-polish` (base main 54b0982). 명령은 그 안의 `a-mate/`에서.
- **네이티브 Windows PowerShell**에서 빌드·테스트 (`npm test`, `npm run build`, `cargo test`) — **WSL 금지** (a-mate/CLAUDE.md).
- LifeView/MiniLife는 `no-hardcoded-colors` ALLOWLIST(v1 아웃-스코프) — **색상 값 추가·수정 금지**, 줄바꿈 속성만.
- 말풍선 `max-width`·`line-clamp` **불변** (실화면 확인 후 후속).
- 커밋: 영어 Conventional Commits, scope `agent`, 항목당 1커밋(P5/H1/H3 분리).
- 베이스라인(확인 완료): 프론트 Vitest 166/166, Rust 553/553 (512+41) 녹색.

---

### Task 1: P5 — 말풍선 한국어 줄바꿈 (CSS만)

**Files:**
- Modify: `a-mate/src/lib/ui/LifeView.svelte` (`.agent-bubble`, 스타일 블록 내 1곳)
- Modify: `a-mate/src/lib/ui/MiniLife.svelte` (`.bubble`)
- Modify: `a-mate/src/Mascot.svelte` (`.bubble .text`)

**Interfaces:**
- Consumes: 없음 (CSS만)
- Produces: 없음 (후속 태스크와 독립)

- [ ] **Step 1: LifeView `.agent-bubble`에 `word-break:keep-all` 추가**

한 줄로 압축된 스타일 문자열 안에서 아래 치환 (Edit old→new):

```
old: white-space:pre-wrap;overflow-wrap:anywhere;box-shadow:0 2px 6px #342d2438
new: white-space:pre-wrap;overflow-wrap:anywhere;word-break:keep-all;box-shadow:0 2px 6px #342d2438
```

`overflow-wrap:anywhere`는 유지 — keep-all 병용 시 평소엔 어절 경계 줄바꿈, max-width(160px)를 넘는 단일 초장 토큰만 비상 개행.

- [ ] **Step 2: MiniLife `.bubble`에 줄바꿈 속성 추가**

```
old:     padding: 7px 12px; font-size: 12px; text-align: center;
new:     padding: 7px 12px; font-size: 12px; text-align: center;
    word-break: keep-all; overflow-wrap: anywhere;
```

- [ ] **Step 3: Mascot `.bubble .text`에 줄바꿈 속성 추가**

```
old:     cursor: pointer; text-align: left; padding: 0;
new:     cursor: pointer; text-align: left; padding: 0;
    word-break: keep-all; overflow-wrap: anywhere;
```

(파일 내 `.bubble .text` 블록은 한 곳뿐. `.bubble-editor`와 혼동 주의.)

- [ ] **Step 4: 검증**

```powershell
cd D:\Project\space-a\.claude\worktrees\feat+home-bubble-polish\a-mate
npm test        # 기대: 19 files, 166 passed (no-hardcoded-colors 포함)
npm run build   # 기대: 오류 없이 완료
```

- [ ] **Step 5: 커밋**

```powershell
git add src/lib/ui/LifeView.svelte src/lib/ui/MiniLife.svelte src/Mascot.svelte
git commit -m "fix(agent): keep korean word boundaries in bubble wrapping"
```

---

### Task 2: H1 — 홈 aside "오늘의 한마디" 단일화

**Files:**
- Modify: `a-mate/src/App.svelte` (script `mood` 파생, 마크업 aside, 스타일 3블록)

**Interfaces:**
- Consumes: 없음
- Produces: 없음 (`mood`·`.diary`·`.more`는 이 파일 밖에서 미사용 — 확인 완료)

- [ ] **Step 1: `mood` 파생값 제거 (script)**

아래 블록을 통째로 삭제:

```ts
  const mood = $derived(
    summary && summary.est_tokens_saved_total > 0 ? '절약할 게 보여요…' : '평화로워요'
  );
```

- [ ] **Step 2: 마크업 교체 — 버튼 박스 → 클릭 불가 카드, mood 블록 삭제**

```svelte
old:
        {#if dailyLine && !visiting}
          <button class="diary" onclick={() => (tab = 'diary')} title="오늘의 일기 전체 보기">
            <span class="cap">📔 오늘의 일기</span>
            <span class="daily-line">{dailyLine}</span>
            <span class="more">더 보기 →</span>
          </button>
        {/if}
        <!-- mood는 내 로컬 데이터 — 남의 미니홈피에서 보이면 주인 것으로 오독된다 -->
        {#if !visiting}
          <p class="mood">“{mood}”</p>
        {/if}

new:
        {#if dailyLine && !visiting}
          <div class="daily">
            <span class="cap">💬 오늘의 한마디</span>
            <span class="daily-line">{dailyLine}</span>
          </div>
        {/if}
```

- [ ] **Step 3: 스타일 교체 — `.mood`·`.diary`·`.more` 제거, `.daily` 추가, keep-all 반영**

```css
old:
  .mood { margin: 0; font-size: 12px; color: var(--ink-soft); text-align: center; }
  /* 일기 카드 — 긴 일기를 4줄로 접고(…) 클릭 시 다이어리 탭으로. 좁은 프로필 칸 가독성 */
  .diary {
    width: 100%; display: flex; flex-direction: column; gap: 5px; text-align: left;
    background: var(--panel2); border: 1px solid var(--line); border-left: 2px solid var(--accent);
    border-radius: var(--radius-s); padding: 8px 10px; cursor: pointer; font: inherit; color: inherit;
  }
  .diary:hover { border-color: var(--accent); }
  .diary .cap { font-size: 10px; color: var(--ink-soft); letter-spacing: 0.3px; }
  .daily-line {
    margin: 0; font-size: 12px; color: var(--ink); line-height: 1.55;
    overflow-wrap: break-word; word-break: break-word;
    display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
  .diary .more { font-size: 10px; color: var(--accent-strong); }

new:
  /* 한마디 카드 — LLM 오늘의 한마디를 4줄로 접는(…) 표시 전용. 좁은 프로필 칸 가독성 */
  .daily {
    display: flex; flex-direction: column; gap: 5px;
    background: var(--panel2); border: 1px solid var(--line); border-left: 2px solid var(--accent);
    border-radius: var(--radius-s); padding: 8px 10px;
  }
  .daily .cap { font-size: 10px; color: var(--ink-soft); letter-spacing: 0.3px; }
  .daily-line {
    margin: 0; font-size: 12px; color: var(--ink); line-height: 1.55;
    overflow-wrap: break-word; word-break: keep-all;
    display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
```

(버튼→div 전환으로 `width:100%`·`cursor`·`font:inherit`·hover가 불필요해져 함께 제거 — flex column stretch가 폭을 채운다.)

- [ ] **Step 4: 검증**

```powershell
cd D:\Project\space-a\.claude\worktrees\feat+home-bubble-polish\a-mate
npm test        # 기대: 166 passed
npm run build   # 기대: 오류 없음 + svelte 미사용 CSS 경고 없어야 함(.mood/.diary 잔재 검출용)
```

- [ ] **Step 5: 커밋**

```powershell
git add src/App.svelte
git commit -m "feat(agent): merge daily diary box and mood into one daily-line card"
```

---

### Task 3: H3 — CHATTER 자유 생성 (LLM 우선 + 풀 12 + 다양화 지시)

**Files:**
- Modify: `a-mate/src/lib/robot/bubble.ts` (`chatterCandidates`)
- Test: `a-mate/src/lib/robot/bubble.test.ts` (신규 1건)
- Modify: `a-mate/crates/core/src/mascot.rs` (`CHATTER_POOL_SIZE`, `build_chatter_prompt`, 테스트 1건 추가)

**Interfaces:**
- Consumes: `pickChatter`/`chatterCandidates` 기존 시그니처 (변경 없음 — `Mascot.svelte:103` 호출부 무수정)
- Produces: `chatterCandidates(pool, summary, honorific)` — 의미 변경: 풀 비었을 때만 정적 CHATTER 렌더

- [ ] **Step 1: 실패하는 프론트 테스트 작성**

`bubble.test.ts`의 `describe('pickChatter', …)` 블록 끝에 추가 (`chatterCandidates`는 이미 import됨):

```ts
  it('풀이 있으면 정적 문구는 후보에서 빠진다 (LLM 우선, 정적은 폴백 전용)', () => {
    expect(chatterCandidates(['풀A', '풀B'], { session_count: 3 }, '주인')).toEqual(['풀A', '풀B']);
  });
```

- [ ] **Step 2: 실패 확인**

```powershell
npm test -- bubble
# 기대: FAIL — 현재 구현은 풀+정적 17개를 반환하므로 toEqual(['풀A','풀B']) 불일치
```

- [ ] **Step 3: `chatterCandidates` 구현 변경**

```ts
old:
/** 잡담 후보 전체 — LLM 풀(사용기록 연계) + 정적 큐레이션(잡담/응원, summary 렌더). */
export function chatterCandidates(
  pool: string[],
  summary: { session_count: number } | null,
  honorific: string,
): string[] {
  return [...pool, ...CHATTER.map((f) => f(summary?.session_count ?? null, honorific))];
}

new:
/** 잡담 후보 — LLM 풀(사용기록 연계)이 있으면 풀에서만 pick, 비면 정적 큐레이션 폴백
 *  (오프라인/엔진 미설정/오늘 활동 0건 → 백엔드가 빈 풀 캐시). */
export function chatterCandidates(
  pool: string[],
  summary: { session_count: number } | null,
  honorific: string,
): string[] {
  return pool.length ? [...pool] : CHATTER.map((f) => f(summary?.session_count ?? null, honorific));
}
```

- [ ] **Step 4: 프론트 테스트 통과 확인**

```powershell
npm test
# 기대: 167 passed — 기존 9건 중 '빈 풀이면 정적 후보만으로 pick'·'전 후보가 recent면
# recent 무시(기아 방지)'가 새 의미에서도 그대로 통과해야 함 (사전 검토 완료)
```

- [ ] **Step 5: 실패하는 Rust 테스트 작성**

`mascot.rs`의 `mod chatter_tests` 안, `chatter_prompt_without_signals_has_no_comic_block` 뒤에 추가:

```rust
    #[test]
    fn chatter_prompt_requests_topic_diversity() {
        // H3: 잡담끼리 결이 겹치지 않게 + 성향(문체)·작업 강도 축 명시 참조
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &Default::default(), 5);
        assert!(p.contains("겹치지 않게"));
        assert!(p.contains("셀프 개그"));
        assert!(p.contains("작업 강도"));
    }
```

- [ ] **Step 6: 실패 확인**

```powershell
cargo test -p agent_mentor chatter_prompt_requests_topic_diversity
# 기대: FAIL — "겹치지 않게" 미포함
```

- [ ] **Step 7: `CHATTER_POOL_SIZE` 12로 확대 + 프롬프트 다양화 지시**

```rust
old:
/// 잡담 풀 크기 — 스캔당 LLM 1회 호출로 배치 생성하는 잡담 개수 (스펙 §2).
pub const CHATTER_POOL_SIZE: usize = 5;

new:
/// 잡담 풀 크기 — 스캔당 LLM 1회 호출로 배치 생성하는 잡담 개수 (스펙 §2).
/// 묶음 ① H3: 프론트가 풀 우선(정적은 폴백 전용)이 되면서 체감 다양성 확보를 위해 5→12.
pub const CHATTER_POOL_SIZE: usize = 12;
```

`build_chatter_prompt` 마지막 지시문 치환:

```rust
old:
         위 요약을 재료로, 상주 마스코트가 가끔 툭 던질 가벼운 잡담·혼잣말을 {n}개 만드세요. \
         코칭 조언이나 보고처럼 굴지 마세요(조언은 다른 채널이 합니다). \
         한 줄에 하나씩, 각 40자 이내로, 번호·불릿·따옴표 없이 출력하세요.",

new:
         위 요약을 재료로, 상주 마스코트가 가끔 툭 던질 가벼운 잡담·혼잣말을 {n}개 만드세요. \
         {n}개끼리 주제와 결이 겹치지 않게 — 오늘 작업에 대한 관찰, 엉뚱한 궁금증, \
         자기(마스코트) 셀프 개그, 응원, 오늘 작업 강도에 대한 능청 등 서로 다른 각도로 만들되, \
         위 성향(문체)은 모든 잡담에 일관되게 유지하세요. \
         코칭 조언이나 보고처럼 굴지 마세요(조언은 다른 채널이 합니다). \
         한 줄에 하나씩, 각 40자 이내로, 번호·불릿·따옴표 없이 출력하세요.",
```

MBTI(`mbti_voice_hint`)·작업 강도(`comic_directives`)·오늘 사실(`facts_block`) 배선은 이미 프롬프트에 포함 — **수정하지 않는다** (사용자 요구 = 유지 + 다양화 지시가 그 축들을 참조).

- [ ] **Step 8: Rust 테스트 통과 확인**

```powershell
cargo test -p agent_mentor chatter
# 기대: chatter_tests 전부 PASS (기존 테스트는 n=5 명시 인자라 풀 크기 변경 무영향 — 확인 완료)
cargo test
# 기대: 워크스페이스 전체 녹색 (512+1=513, 41)
```

- [ ] **Step 9: 커밋**

```powershell
git add src/lib/robot/bubble.ts src/lib/robot/bubble.test.ts crates/core/src/mascot.rs
git commit -m "feat(agent): prefer llm chatter pool over static lines and widen pool"
```

---

### Task 4: 마무리 — 로드맵 기록·PR·DoD

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (H1·H3·P5 완료 기록)
- docs-archive 스킬이 spec/plan을 `docs/archive/` 미러로 이동

**Interfaces:**
- Consumes: Task 1–3 커밋 완료 상태
- Produces: PR 1개 (체크리스트에 실화면 확인 항목 포함)

- [ ] **Step 1: main 최신 반영** — `git fetch origin && git rebase origin/main` (로드맵 파일 병렬 세션 충돌 회피를 위해 기록 직전에 수행)
- [ ] **Step 2: 로드맵에 완료 기록** — P5·H1·H3 항목과 묶음 표 ①에 `✅ 완료(2026-07-27, feat/home-bubble-polish)` + 결정 요약 1줄(H1=클릭 불가 한마디 카드·mood 제거, H3=LLM 우선·풀 12) 추가, `docs(plan): record bundle 1 home bubble polish completion` 커밋
- [ ] **Step 3: docs-archive 스킬 실행** — 본 spec/plan을 `docs/archive/` 미러로 이동(ADR 0013), 같은 브랜치에 커밋
- [ ] **Step 4: 검증 총정리** — `npm test`·`npm run build`·`cargo test` 최종 녹색 확인
- [ ] **Step 5: PR 생성** — 제목 `feat(agent): home bubble polish bundle (P5+H1+H3)`. 본문에 스펙 링크 + **사용자 확인 체크리스트**: ① 방 화면 lifeSetBubble 어절 줄바꿈 ② 홈 미니룸·데스크톱 마스코트 말풍선 줄바꿈 ③ 홈 aside 한마디 카드(클릭 불가·mood 없음) ④ 잡담이 LLM 풀 위주로 도는지(엔진 연결 상태)

---

## Self-Review 결과

- **스펙 커버리지**: P5 3곳+daily-line(T1·T2), H1 카드 전환+mood 제거(T2), H3 LLM 우선+풀 12+다양화 지시+테스트(T3), 검증선·PR 체크리스트·DoD(T4) — 갭 없음.
- **플레이스홀더**: 없음 (모든 코드 스텝에 실제 old/new 코드 포함).
- **타입 일관성**: `chatterCandidates` 시그니처 불변으로 호출부(`pickChatter`) 무수정 — 확인 완료.
