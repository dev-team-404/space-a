---
status: done
archived: 2026-07-27
---

# G4 — 이름 클릭 → 홈 이동 팝오버 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 방명록 작성자·방 점유자 이름 클릭 시 "○○네 놀러가기" 팝오버 버튼으로 그 사람 Life 방으로 이동한다.

**Architecture:** 판별 로직은 순수 모듈 `src/lib/people.ts`(`resolveVisitTarget` + TTL 캐시 `getPeople`)에 두고 Vitest로 검증. UI는 공용 컴포넌트 `src/lib/ui/NameChip.svelte` 하나로 GuestbookTab(원글·답글)과 LifeView(점유자 라벨) 두 지점에 배선. 이동은 기존 `lifeGoto` — 화면 전환은 기존 2초 폴링이 처리하므로 추가 배선 없음.

**Tech Stack:** Svelte 5 (runes) + TypeScript + Vitest. 서버(a-hub)·Rust(src-tauri, crates/core) 미접촉.

**Spec:** [2026-07-27-name-click-visit-popover-design.md](../specs/2026-07-27-name-click-visit-popover-design.md)

## Global Constraints

- 모든 명령은 **네이티브 Windows PowerShell**에서, `a-mate/` 디렉터리 기준 (WSL 금지).
- Vitest는 **순수 .ts만** — Svelte 컴포넌트 렌더 테스트 인프라 없음, 신설 금지.
- `no-hardcoded-colors` 테스트: 신규 `.svelte`의 `<style>`에 hex 색 금지, 전경 `color: var(--accent)` 금지 (→ `--accent-strong`). 테마 변수만 사용.
- `people.ts`는 순수 모듈 — `api.ts`는 **type-only import**만 (런타임 import 금지, `guestbook.ts` 선례).
- LifeView `.agent`는 `pointer-events:none` — 클릭 재활성화는 NameChip 내부 요소에만 (이동 불가 이름은 지금처럼 타일 클릭 통과).
- 팝오버 바깥 클릭 닫기는 **document 리스너** 방식 — LifeView `.scene`의 `contain`/`transform`이 fixed 백드롭의 containing block을 바꾸므로 백드롭 요소 금지.
- 커밋: Conventional Commits 영어, scope `agent`. 각 커밋 끝에 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: `people.ts` — `resolveVisitTarget` 판별 로직 (TDD)

**Files:**
- Create: `a-mate/src/lib/people.ts`
- Test: `a-mate/src/lib/people.test.ts`

**Interfaces:**
- Consumes: `LifePerson`(type, `api.ts:211` — `{ agent_id: string; name: string; life_id: string; is_friend: boolean }`)
- Produces: `resolveVisitTarget(agentId: string, displayName: string, ctx: VisitCtx): VisitTarget | null`, `interface VisitTarget { lifeId: string; label: string }`, `interface VisitCtx { meId: string; myLifeId: string; currentLifeId: string; people: Pick<LifePerson, 'agent_id' | 'life_id'>[] }` — Task 3(NameChip)이 사용

- [ ] **Step 1: 실패하는 테스트 작성**

`a-mate/src/lib/people.test.ts` 생성:

```ts
import { describe, it, expect } from 'vitest';
import { resolveVisitTarget, type VisitCtx } from './people';
import type { LifePerson } from './api';

const P = (agent_id: string, life_id: string): LifePerson => ({ agent_id, name: 'x', life_id, is_friend: false });
const ctx = (over: Partial<VisitCtx> = {}): VisitCtx => ({
  meId: 'me', myLifeId: 'life-me', currentLifeId: 'life-cur',
  people: [P('a1', 'life-a1'), P('owner', 'life-cur')],
  ...over,
});

describe('resolveVisitTarget', () => {
  it('내 방에서 내 이름 → null (규칙 1)', () => {
    expect(resolveVisitTarget('me', '돌쇠', ctx({ currentLifeId: 'life-me' }))).toBeNull();
  });
  it('방문 중 내 이름 → 내 방으로 돌아가기 (규칙 2)', () => {
    expect(resolveVisitTarget('me', '돌쇠', ctx())).toEqual({ lifeId: 'life-me', label: '내 방으로 돌아가기' });
  });
  it('people에 없는 agent(탈퇴·미등록) → null (규칙 3)', () => {
    expect(resolveVisitTarget('ghost', '유령', ctx())).toBeNull();
  });
  it('현재 방 주인(그 방 봇 답글 행 포함) → null (규칙 4)', () => {
    expect(resolveVisitTarget('owner', '주인봇', ctx())).toBeNull();
  });
  it('다른 방 사람 → 그 방 타겟 + 표시 이름 라벨 (규칙 5)', () => {
    expect(resolveVisitTarget('a1', '김민지', ctx())).toEqual({ lifeId: 'life-a1', label: '김민지네 놀러가기' });
  });
  it('컨텍스트 미비(로딩 전 빈 문자열) → null', () => {
    expect(resolveVisitTarget('a1', '김민지', ctx({ meId: '' }))).toBeNull();
    expect(resolveVisitTarget('a1', '김민지', ctx({ myLifeId: '' }))).toBeNull();
    expect(resolveVisitTarget('a1', '김민지', ctx({ currentLifeId: '' }))).toBeNull();
    expect(resolveVisitTarget('', '김민지', ctx())).toBeNull();
  });
});
```

- [ ] **Step 2: 실패 확인**

Run (`a-mate/`에서): `npm test -- src/lib/people.test.ts`
Expected: FAIL — `./people` 모듈 미존재로 import/transform 에러.

- [ ] **Step 3: 최소 구현**

`a-mate/src/lib/people.ts` 생성:

```ts
import type { LifePerson } from './api';

// G4 — 이름 클릭 → 홈 이동 (스펙 §4). 순수 모듈: api.ts는 type-only import만.

export interface VisitTarget { lifeId: string; label: string }
export interface VisitCtx {
  meId: string;
  myLifeId: string;
  currentLifeId: string;
  people: Pick<LifePerson, 'agent_id' | 'life_id'>[];
}

/** 이름 클릭 대상 판별. null = 이동 불가(일반 텍스트 렌더). */
export function resolveVisitTarget(agentId: string, displayName: string, ctx: VisitCtx): VisitTarget | null {
  if (!agentId || !ctx.meId || !ctx.myLifeId || !ctx.currentLifeId) return null;
  if (agentId === ctx.meId) {
    if (ctx.myLifeId === ctx.currentLifeId) return null;
    return { lifeId: ctx.myLifeId, label: '내 방으로 돌아가기' };
  }
  const person = ctx.people.find((p) => p.agent_id === agentId);
  if (!person || person.life_id === ctx.currentLifeId) return null;
  return { lifeId: person.life_id, label: `${displayName}네 놀러가기` };
}
```

- [ ] **Step 4: 통과 확인**

Run: `npm test -- src/lib/people.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: 커밋**

```powershell
git add src/lib/people.ts src/lib/people.test.ts
git commit -m @'
feat(agent): add visit target resolution for name clicks

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 2: `people.ts` — `getPeople` TTL 캐시 (TDD)

**Files:**
- Modify: `a-mate/src/lib/people.ts` (Task 1 산출물에 추가)
- Test: `a-mate/src/lib/people.test.ts` (describe 블록 추가)

**Interfaces:**
- Consumes: `LifePerson`(type)
- Produces: `getPeople(fetcher: PeopleFetcher): Promise<LifePerson[]>`, `type PeopleFetcher = () => Promise<{ people: LifePerson[] }>`, `resetPeopleCache(): void`(테스트 전용) — Task 3(NameChip)이 `getPeople(lifePeople)`로 호출

- [ ] **Step 1: 실패하는 테스트 추가**

`a-mate/src/lib/people.test.ts`에 import 갱신 + describe 추가:

```ts
// 파일 상단 import를 다음으로 교체
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getPeople, resetPeopleCache, resolveVisitTarget, type VisitCtx } from './people';
import type { LifePerson } from './api';
```

```ts
// 파일 끝에 추가
describe('getPeople', () => {
  beforeEach(() => { resetPeopleCache(); vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('TTL 내 재호출은 fetch 1회', async () => {
    const fetcher = vi.fn().mockResolvedValue({ people: [P('a', 'l')] });
    await getPeople(fetcher);
    await getPeople(fetcher);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('TTL(10초) 경과 후 재fetch', async () => {
    const fetcher = vi.fn().mockResolvedValue({ people: [] });
    await getPeople(fetcher);
    vi.advanceTimersByTime(10_001);
    await getPeople(fetcher);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it('동시 호출은 in-flight 공유', async () => {
    let resolve!: (v: { people: LifePerson[] }) => void;
    const fetcher = vi.fn(() => new Promise<{ people: LifePerson[] }>((r) => { resolve = r; }));
    const p1 = getPeople(fetcher);
    const p2 = getPeople(fetcher);
    resolve({ people: [P('a', 'l')] });
    expect(await p1).toEqual(await p2);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('실패 시 빈 배열 (기능만 조용히 비활성)', async () => {
    const fetcher = vi.fn().mockRejectedValue(new Error('down'));
    await expect(getPeople(fetcher)).resolves.toEqual([]);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `npm test -- src/lib/people.test.ts`
Expected: FAIL — `getPeople`/`resetPeopleCache` export 미존재.

- [ ] **Step 3: 최소 구현**

`a-mate/src/lib/people.ts` 끝에 추가:

```ts
export type PeopleFetcher = () => Promise<{ people: LifePerson[] }>;

const TTL_MS = 10_000; // 등록자 목록은 저빈도 변경 — lifeViewCache(500ms) 선례를 완화
let cache: { at: number; value: LifePerson[] } | null = null;
let inFlight: Promise<LifePerson[]> | null = null;

/** lifePeople 래퍼 — TTL 캐시 + in-flight 공유. 실패 시 [] (모든 이름이 일반 텍스트로 강등). */
export function getPeople(fetcher: PeopleFetcher): Promise<LifePerson[]> {
  if (cache && Date.now() - cache.at < TTL_MS) return Promise.resolve(cache.value);
  if (inFlight) return inFlight;
  inFlight = fetcher()
    .then((r) => { cache = { at: Date.now(), value: r.people }; return r.people; })
    .catch(() => [] as LifePerson[])
    .finally(() => { inFlight = null; });
  return inFlight;
}

/** 테스트 전용 — 모듈 캐시 초기화. */
export function resetPeopleCache(): void { cache = null; inFlight = null; }
```

- [ ] **Step 4: 통과 확인**

Run: `npm test -- src/lib/people.test.ts`
Expected: PASS (10 tests).

- [ ] **Step 5: 커밋**

```powershell
git add src/lib/people.ts src/lib/people.test.ts
git commit -m @'
feat(agent): add cached people lookup for name clicks

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 3: `NameChip.svelte` 공용 팝오버 컴포넌트

**Files:**
- Create: `a-mate/src/lib/ui/NameChip.svelte`

**Interfaces:**
- Consumes: `resolveVisitTarget`·`getPeople`(Task 1·2), `lifeGoto`·`lifePeople`·`LifePerson`(`api.ts`)
- Produces: `<NameChip agentId name meId myLifeId currentLifeId />` — Task 4·5가 사용. 이동 불가면 일반 텍스트, 가능하면 클릭 → 팝오버 → `lifeGoto`

렌더 테스트 인프라가 없으므로 이 태스크는 컴파일 검증(`npm run build`) + 기존 스위트(`npm test`, no-hardcoded-colors 포함)로 확인한다.

- [ ] **Step 1: 컴포넌트 작성**

`a-mate/src/lib/ui/NameChip.svelte` 생성:

```svelte
<script lang="ts">
  import { lifeGoto, lifePeople, type LifePerson } from '../api';
  import { getPeople, resolveVisitTarget } from '../people';
  let { agentId, name, meId, myLifeId, currentLifeId }: {
    agentId: string; name: string; meId: string; myLifeId: string; currentLifeId: string;
  } = $props();
  let people = $state<LifePerson[]>([]);
  $effect(() => { let alive = true; getPeople(lifePeople).then((p) => { if (alive) people = p; }); return () => { alive = false; }; });
  const target = $derived(resolveVisitTarget(agentId, name, { meId, myLifeId, currentLifeId, people }));
  let open = $state(false), busy = $state(false);
  let root = $state<HTMLElement | null>(null);
  // 바깥 클릭 닫기 — LifeView .scene의 contain/transform이 fixed 백드롭을 어긋나게 하므로 문서 리스너 방식
  $effect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => { if (root && !root.contains(e.target as Node)) open = false; };
    document.addEventListener('pointerdown', onDown, true);
    return () => document.removeEventListener('pointerdown', onDown, true);
  });
  async function go() {
    if (!target || busy) return;
    busy = true;
    try { await lifeGoto(target.lifeId); } catch { /* 이동 실패 — 조용히 닫기 (Mascot gotoLife 선례) */ }
    finally { busy = false; open = false; }
  }
</script>

{#if target}
  <span class="chip" bind:this={root}>
    <button class="nm" onclick={() => (open = !open)}>{name}</button>
    {#if open}
      <span class="pop"><button class="go" disabled={busy} onclick={go}>{target.label} →</button></span>
    {/if}
  </span>
{:else}
  <span class="plain">{name}</span>
{/if}

<style>
  /* pointer-events:auto — LifeView .agent(none) 안에서 클릭 가능한 이름만 재활성화 */
  .chip { position: relative; display: inline-block; pointer-events: auto; }
  .nm {
    font: inherit; font-weight: inherit; color: inherit;
    background: none; border: 0; padding: 0; cursor: pointer;
    text-decoration: underline dotted; text-underline-offset: 2px;
  }
  .nm:hover { color: var(--accent-strong); }
  .plain { font-weight: inherit; }
  .pop {
    position: absolute; left: 50%; bottom: calc(100% + 5px); transform: translateX(-50%);
    z-index: 1001; white-space: nowrap;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px;
    box-shadow: var(--shadow-soft); padding: 4px;
  }
  .go {
    font: inherit; border: 0; border-radius: 6px; padding: 5px 9px; cursor: pointer;
    background: var(--accent); color: var(--accent-ink);
  }
  .go:disabled { opacity: 0.6; cursor: default; }
</style>
```

- [ ] **Step 2: 컴파일·스위트 검증**

Run: `npm run build` → Expected: 빌드 성공.
Run: `npm test` → Expected: 전체 PASS (no-hardcoded-colors가 신규 파일 자동 검사 — hex 없음·전경 `--accent` 없음이므로 통과).

- [ ] **Step 3: 커밋**

```powershell
git add src/lib/ui/NameChip.svelte
git commit -m @'
feat(agent): add NameChip popover component for visiting homes

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 4: GuestbookTab + App 배선 (방명록 원글·답글)

**Files:**
- Modify: `a-mate/src/lib/ui/GuestbookTab.svelte` (props + 이름 2곳)
- Modify: `a-mate/src/App.svelte:195` (`myLifeId` 전달)

**Interfaces:**
- Consumes: `NameChip`(Task 3), `GuestbookEntry.author_agent_id`(기존)
- Produces: 방명록 이름이 NameChip으로 렌더 — 시각·동작 외 다른 산출물 없음

- [ ] **Step 1: GuestbookTab props에 `myLifeId` 추가 + NameChip import**

`a-mate/src/lib/ui/GuestbookTab.svelte` 2행 import 아래에 추가 및 5행 props 교체:

```svelte
  import NameChip from './NameChip.svelte';
```

```svelte
  let { lifeId, meId, myLifeId, isOwner=false }: {lifeId:string;meId:string;myLifeId:string;isOwner?:boolean}=$props();
```

- [ ] **Step 2: 원글 작성자 이름 교체 (22행)**

`<b>{t.entry.author_name}</b>` →

```svelte
<b><NameChip agentId={t.entry.author_agent_id} name={t.entry.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b>
```

- [ ] **Step 3: 답글 작성자 이름 교체 (33행)**

`<b>{reply.author_name}</b>` →

```svelte
<b><NameChip agentId={reply.author_agent_id} name={reply.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b>
```

- [ ] **Step 4: App.svelte에서 `myLifeId` 전달 (195행)**

```svelte
<GuestbookTab lifeId={currentLifeId} {meId} {myLifeId} isOwner={currentLifeId===myLifeId}/>
```

- [ ] **Step 5: 검증**

Run: `npm run build` → Expected: 빌드 성공 (props 타입 불일치 시 여기서 드러남).
Run: `npm test` → Expected: 전체 PASS.

- [ ] **Step 6: 커밋**

```powershell
git add src/lib/ui/GuestbookTab.svelte src/App.svelte
git commit -m @'
feat(agent): wire name-click visit popover into guestbook

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 5: LifeView 배선 (방 점유자 라벨)

**Files:**
- Modify: `a-mate/src/lib/ui/LifeView.svelte` (import + 139행 라벨)

**Interfaces:**
- Consumes: `NameChip`(Task 3), `LifeOccupant.agent_id`·`me: LifeMe`(기존 — 라벨 블록은 `{#if life&&me&&activeDesign}` 안이라 `me` non-null)
- Produces: 점유자 라벨이 NameChip으로 렌더

주의: 기존 pill 스타일은 `.agent span` 셀렉터(LifeView 스코프)라 **바깥 `<span>`을 유지**하고 그 안에 NameChip을 넣는다(자식 컴포넌트 내부엔 부모 스코프 스타일이 적용되지 않으므로 pill 룩 보존). `.agent`는 `pointer-events:none` — NameChip `.chip`이 자체적으로 재활성화하고, 이동 불가 이름(`.plain`)은 지금처럼 타일 클릭을 통과시킨다.

- [ ] **Step 1: import 추가**

`LifeView.svelte`의 script 상단 import 블록에 추가:

```svelte
  import NameChip from './NameChip.svelte';
```

- [ ] **Step 2: 점유자 라벨 교체 (139행)**

`<span>{o.name}</span>` →

```svelte
<span><NameChip agentId={o.agent_id} name={o.name} meId={me.agent_id} myLifeId={me.my_life_id} currentLifeId={me.life_id}/></span>
```

- [ ] **Step 3: 검증**

Run: `npm run build` → Expected: 빌드 성공.
Run: `npm test` → Expected: 전체 PASS (LifeView는 colors ALLOWLIST — 변경 없어도 무관).

- [ ] **Step 4: 커밋**

```powershell
git add src/lib/ui/LifeView.svelte
git commit -m @'
feat(agent): wire name-click visit popover into room occupants

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 6: 최종 검증 + DoD (아카이브 + PR)

**Files:**
- Move: `docs/design/a-mate/specs/2026-07-27-name-click-visit-popover-design.md` → `docs/archive/design/a-mate/specs/` (docs-archive 스킬)
- Move: `docs/design/a-mate/plans/2026-07-27-name-click-visit-popover.md` → `docs/archive/design/a-mate/plans/` (docs-archive 스킬)

**Interfaces:**
- Consumes: Task 1~5 완료 상태
- Produces: 녹색 스위트 + PR + 아카이브 완료 (ADR 0013 DoD)

- [ ] **Step 1: 프론트 전체 스위트**

Run (`a-mate/`): `npm test`
Expected: 19 파일 · 164+ tests PASS (베이스라인 18 파일·154 tests + people.test.ts 10 tests).

- [ ] **Step 2: Rust 회귀 확인 (미접촉이지만 워크스페이스 확인)**

Run (`a-mate/`): `cargo test`
Expected: 전체 PASS — 이 작업은 Rust를 건드리지 않으므로 실패 시 환경 문제부터 의심.

- [ ] **Step 3: 수동 스모크 항목 기록 (사용자 환경 필요)**

사외망 개발 PC는 hub 도달 불가 — 아래는 PR 본문에 "사용자 확인 필요"로 명시:
1. 남의 방 방명록에서 다른 사람 이름 클릭 → "○○네 놀러가기" → 이동.
2. 내 방 방명록에서 내 봇 답글 이름 = 일반 텍스트.
3. 남의 방에서 내 이름 클릭 → "내 방으로 돌아가기".
4. 점유자 라벨 클릭 동작 + 이동 불가 라벨이 타일 클릭을 막지 않는지.

- [ ] **Step 4: docs-archive 스킬 실행 (DoD, ADR 0013)**

스펙·플랜을 `docs/archive/` 미러로 이동하고 커밋 (스킬이 절차 안내).

- [ ] **Step 5: 푸시 + PR 생성**

```powershell
git push -u origin feat/name-click-visit
gh pr create --title "feat(agent): name-click visit popover (G4)" --body @'
## G4 — 이름 클릭 → 그 사람 홈 이동 팝오버 (백로그 5)

방명록 작성자(원글·답글)·방 점유자 이름 클릭 시 "○○네 놀러가기" 팝오버로 그 사람 Life 방으로 이동.

- 판별은 순수 모듈 `people.ts` (`resolveVisitTarget` 5규칙 + `getPeople` 10초 TTL 캐시, Vitest 10건)
- UI는 공용 `NameChip.svelte` — 이동 가능할 때만 클릭 어포던스, 그 외 일반 텍스트
- 서버·Rust 미접촉. 스펙·플랜은 이 PR에서 `docs/archive/`로 이동 (ADR 0013 DoD)
- 로드맵: docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md §G4

### 사용자 확인 필요 (사외망 PC는 hub 도달 불가)
- [ ] 남의 방 방명록에서 다른 사람 이름 클릭 → "○○네 놀러가기" → 이동
- [ ] 내 방 방명록에서 내 봇 답글 이름 = 일반 텍스트
- [ ] 남의 방에서 내 이름 클릭 → "내 방으로 돌아가기"
- [ ] 점유자 라벨 클릭 동작 + 이동 불가 라벨이 타일 클릭을 막지 않는지

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'@
```

Expected: PR URL 출력.

---

## 알려진 한계 (스펙 §7·§9 정합)

- people 도착 전 수백 ms 동안 이름이 일반 텍스트였다가 클릭 가능으로 바뀌는 pop-in 허용.
- LifeView 팝오버는 `.agent` 스태킹 컨텍스트(z-index 30+) 안에 있어 바로 앞줄 점유자 스프라이트에 일부 가려질 수 있음 — 수용(포털 없이 해결 불가, 발생 빈도 낮음).
- 신규 등록자는 TTL(10초) 경과 후 클릭 가능해짐.
