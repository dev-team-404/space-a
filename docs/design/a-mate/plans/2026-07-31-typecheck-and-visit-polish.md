# 묶음 ⑥ — 타입 체크 안전망 + 방문 소품 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 이 레포에 없던 타입 체크 단계(`svelte-check`)를 도입해 `npm test`에 편입하고, 같은 프론트 영역의 잔여 소품 2건(방문 말풍선 표정·방명록 워터마크 최초 시드)을 함께 해소한다.

**Architecture:** 3개 항목을 커밋 단위로 분리한다. Q3(타입 체크)를 먼저 두는 이유는 Q1이 `UnseenState.guestbookLastSeen`의 타입 시그니처를 `string` → `string | null`로 바꾸는 변경이라, 새 안전망이 소비처 누락을 잡아주는 자리이기 때문이다. Q1·Q2는 순수 함수 계층(`unseen.ts`·`anim.ts`)에 로직을 두고 `App.svelte`는 배선만 한다 — 컴포넌트에는 테스트가 없으므로 판정 로직이 컴포넌트로 새면 검증이 불가능해진다.

**Tech Stack:** Svelte 5(runes), TypeScript, Vite, Vitest, `svelte-check`.

**스펙:** [2026-07-31-typecheck-and-visit-polish-design.md](../specs/2026-07-31-typecheck-and-visit-polish-design.md)

## Global Constraints

- 실행 위치: `a-mate/`. 프론트 명령은 그 디렉터리에서 돌린다.
- 플랫폼 **Windows 전용**. 빌드·실행은 네이티브 Windows PowerShell/cmd에서 (WSL 금지).
- 스타일은 **전역 CSS 토큰(`var(--…)`)만** — 하드코딩 색 금지 (`src/lib/no-hardcoded-colors.test.ts`가 강제). 이번 작업은 스타일 미접촉이라 해당 없음.
- 모든 백엔드 호출은 `src/lib/api.ts`에 래핑 (기존 컨벤션).
- 커밋 메시지: **Conventional Commits, 영어**. scope는 `frontend`. 본문 끝에
  `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>` 트레일러 포함
  (레포 최근 커밋 #137·#139·#140의 실제 관행).
- `svelte-check` 실행 임계는 **`--threshold error`** — 경고 13건은 출력에 남기되 실패로 치지 않는다.
- Rust·a-hub·`contracts/` **미접촉**.
- 베이스라인 (2026-07-31 워크트리 실측): 프론트 `npm test` 210 passed / 26 files, `cargo test` exit 0.

---

### Task 1: svelte-check 도입 + 드러난 실오류 해소 (Q3)

**Files:**
- Modify: `a-mate/package.json` (devDependencies + scripts)
- Modify: `a-mate/tsconfig.json`
- Modify: `a-mate/src/lib/api.ts:137-142` (`HubSettings`)
- Modify: `a-mate/src/lib/interior/catalog.ts:47`
- Modify: `a-mate/src/lib/interior/catalog.test.ts:152,169`

**Interfaces:**
- Produces: `npm run check` (단독 실행), `npm test` = `svelte-check --threshold error && vitest run`. Task 2·3은 이 `npm test`로 검증한다.
- Produces: `HubSettings.api_key: string` — 이후 태스크는 이 필드를 건드리지 않는다.

- [ ] **Step 1: devDependency 설치**

```bash
cd a-mate
npm install --save-dev svelte-check@^4 @types/node@^24
```

`@types/node`는 **런타임 메이저에 맞춘다** — 이 환경은 Node v24.11.0이다 (2026-07-31 실측). `svelte-check`는 최신이 4.7.4라 `^4`가 맞다.

- [ ] **Step 2: tsconfig에 skipLibCheck·node 타입 추가**

`a-mate/tsconfig.json`을 아래로 교체한다.

```jsonc
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "verbatimModuleSyntax": true,
    "skipLibCheck": true,
    "types": ["vite/client", "node"]
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`skipLibCheck`는 `node_modules`의 `.d.ts` 오류 71건(서로 다른 패키지의 전역 타입 충돌 — `svelte`↔`esrap`의 `Node` 중복 등)을 제외한다. 우리 코드에서 그 타입을 *사용하는* 지점은 계속 검사된다.

- [ ] **Step 3: 검사를 돌려 실패를 확인한다**

Run: `cd a-mate && npx svelte-check --threshold error`

Expected: FAIL. 진단 4건이 남아야 한다 (`@types/node` 설치로 `no-hardcoded-colors.test.ts` 3건은 이미 사라진 상태):

```
src/lib/interior/catalog.ts:48         Conversion of type ... ground ... number[] ... [number, number]
src/lib/interior/catalog.test.ts:152   'sourceAxes' is possibly 'undefined'
src/lib/interior/catalog.test.ts:169   Conversion of type ... (동일 원인)
src/lib/ui/settings/ConnectionGroup.svelte:18   Property 'api_key' does not exist on type 'HubSettings'
```

실제 출력이 이와 다르면 **멈추고 보고한다** — `@types/node`가 테스트 파일을 처음 검사하면서 새 오류가 드러날 수 있고, 그건 계획 수정 사유다.

- [ ] **Step 4: HubSettings에 api_key 선언 추가**

`a-mate/src/lib/api.ts:137`의 인터페이스를 아래로 바꾼다.

```ts
export interface HubSettings {
  url: string;
  api_key: string;
  user: string;
  connected: boolean;
  life_id: string;
}
```

백엔드 `src-tauri/src/commands.rs:817`의 `hub_settings_get`이 `api_key: get("hub_api_key")`를 **이미 내려주고 있다.** 선언을 실제에 맞추는 것이지 동작 변경이 아니다 — Rust는 건드리지 않는다.

- [ ] **Step 5: AssetMetric의 ground 타입 완화**

`a-mate/src/lib/interior/catalog.ts:47`을 바꾼다.

```ts
type AssetMetric = { width: number; height: number; ground: number[] };
```

`as unknown as`를 쓰지 않는 이유: 이중 캐스팅은 JSON에 길이 3 배열이 와도 통과시키는 거짓말을 남긴다. `ground` 사용처는 `catalog.ts:310,311,319,321`과 테스트 2곳뿐이고 전부 인덱스 접근(`ground[0]`/`ground[1]`)이라 튜플일 필요가 없다. `noUncheckedIndexedAccess`가 꺼져 있으므로 인덱싱 결과는 `number`로 유지된다.

- [ ] **Step 6: 테스트 파일의 인라인 타입도 동일하게 완화**

`a-mate/src/lib/interior/catalog.test.ts:169`를 바꾼다.

```ts
    const metrics = assetMetricsJson as Record<string, { width: number; height: number; ground: number[] }>;
```

- [ ] **Step 7: 테스트의 옵셔널 체이닝 누락 수정**

`a-mate/src/lib/interior/catalog.test.ts:152`를 바꾼다.

```ts
        const axes = sourceAxes?.[source];
```

`FLOOR_SOURCE_AXES_BY_ASSET`은 바깥이 `Partial<Record<string, …>>`이라 `Object.entries()`가 뽑는 값이 `undefined`일 수 있다. 프로덕션 코드(`catalog.ts:283`)는 이미 `FLOOR_SOURCE_AXES_BY_ASSET[assetId]?.[view.source]`로 처리하고 있고 테스트만 빠뜨렸다 — 같은 형태로 맞춘다.

- [ ] **Step 8: 검사가 통과하는지 확인**

Run: `cd a-mate && npx svelte-check --threshold error`

Expected: PASS — `svelte-check found 0 errors and 13 warnings in N files`. 경고는 남아 있어도 정상이다(`--threshold error`).

- [ ] **Step 9: package.json 스크립트에 편입**

`a-mate/package.json`의 `scripts`를 바꾼다.

```jsonc
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "tauri": "tauri",
    "check": "svelte-check --threshold error",
    "test": "svelte-check --threshold error && vitest run"
  },
```

CI(`.github/workflows`)가 없는 레포라 `check` 스크립트만 두면 아무도 돌리지 않아 사문화된다. `npm test`에 묶어 기존 검증 관행(`cargo test`/`npm test`/`npm run build`)에 자동 편입시킨다. 단독 실행용 `check`도 함께 남긴다.

- [ ] **Step 10: 전체 검증**

Run: `cd a-mate && npm test`

Expected: svelte-check 0 errors 통과 후 vitest **210 passed** (베이스라인 유지 — 이 태스크는 테스트를 추가하지 않는다).

Run: `cd a-mate && npm run build`

Expected: exit 0.

- [ ] **Step 11: 커밋**

```bash
git add a-mate/package.json a-mate/package-lock.json a-mate/tsconfig.json a-mate/src/lib/api.ts a-mate/src/lib/interior/catalog.ts a-mate/src/lib/interior/catalog.test.ts
git commit -F- <<'EOF'
feat(frontend): add a type-check step and fix what it found

The repo had no type checking at all: `build` is `vite build`
(transpile-only), `svelte-check` was not a dependency, and nothing called
`tsc`. Deleting a key from a `Record<Notice['kind'], string>` map still
built and still passed all 189 tests, so the compile-time safety that
bundle 4 assumed simply did not exist.

Measuring before deciding changed the approach. Of 78 errors, 71 are in
library .d.ts files — different packages declaring the same globals — which
`skipLibCheck` excludes and which we could not fix anyway. That left 7
diagnostics from 4 causes, so the staged rollout the roadmap braced for is
unnecessary.

One cause is a real drift: `hub_settings_get` sends `api_key` and the
settings screen reads it, but `HubSettings` never declared the field. The
runtime works; the declaration was false. Two more are a JSON-derived
`number[]` cast to a `[number, number]` tuple — widened rather than
double-cast, since every use is an index access and `as unknown as` would
let a length-3 array through. The last is a test missing the optional
chaining its production counterpart already has.

Wired into `npm test` rather than left as a standalone script: with no CI
in this repo, a script nobody runs is a script that decays.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 2: 방문 말풍선의 마스코트 표정 (Q2)

**Files:**
- Modify: `a-mate/src/lib/robot/anim.ts:16-17`
- Test: `a-mate/src/lib/robot/anim.test.ts:5-10`

**Interfaces:**
- Consumes: Task 1의 `npm test`(타입 체크 포함).
- Produces: 없음 — `resolveState`의 시그니처는 그대로다.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/robot/anim.test.ts`의 첫 `it` 블록에 한 줄을 추가한다.

```ts
  it('말풍선이 최우선이다', () => {
    expect(resolveState({ bubbleKind: 'finding', hour: 3 })).toBe('alert');
    expect(resolveState({ bubbleKind: 'diary', hour: 3 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'occasion', hour: 12 })).toBe('happy');
    expect(resolveState({ bubbleKind: 'chatter', hour: 12 })).toBe('talk');
    expect(resolveState({ bubbleKind: 'visit', hour: 3 })).toBe('happy');
  });
```

`hour: 3`인 것이 핵심이다 — 새벽 시간대라야 `default` 분기의 `sleep`으로 떨어지는 현재 버그를 잡는다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run: `cd a-mate && npx vitest run src/lib/robot/anim.test.ts`

Expected: FAIL — `expected 'sleep' to be 'happy'`.

- [ ] **Step 3: visit 분기를 추가한다**

`a-mate/src/lib/robot/anim.ts:16-17`을 바꾼다.

```ts
    case 'diary':
    case 'occasion':
    case 'visit': return 'happy';
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cd a-mate && npx vitest run src/lib/robot/anim.test.ts`

Expected: PASS.

- [ ] **Step 5: 전체 검증**

Run: `cd a-mate && npm test`

Expected: 0 errors + **210 passed** (테스트 수는 그대로 — 기존 `it`에 단언만 추가했다).

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src/lib/robot/anim.ts a-mate/src/lib/robot/anim.test.ts
git commit -F- <<'EOF'
fix(frontend): stop announcing visits with a sleeping face

`resolveState` had no `visit` case, so visit bubbles fell through to the
default branch, which judges by clock alone: between 01:00 and 07:00 the
mascot said "someone dropped by!" while asleep. `BubbleKind` has carried
`visit` since bundle 4; only the expression mapping was missing.

Treated like `diary` and `occasion` — someone visiting is good news.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 3: 방명록 읽음 워터마크의 최초 시드 (Q1)

**Files:**
- Modify: `a-mate/src/lib/unseen.ts:1-32,59-68`
- Modify: `a-mate/src/lib/unseen.test.ts`
- Modify: `a-mate/src/App.svelte:152-166`

**Interfaces:**
- Consumes: Task 1의 `npm test`. 이 태스크의 타입 시그니처 변경이 소비처를 깨뜨리면 `svelte-check`가 잡는다 — 그것이 Q3를 먼저 둔 이유다.
- Produces: `UnseenState.guestbookLastSeen: string | null`, `parseUnseen(raw: string | null): UnseenState`(인자 1개), `loadUnseen(): UnseenState`(인자 없음), `newGuestbookIds(entries, myAgentId, lastSeenIso: string | null): string[]`.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/unseen.test.ts`에서 `parseUnseen` describe 블록을 아래로 교체하고, `guestbook unseen` 블록에 케이스 하나를 추가한다.

```ts
describe('parseUnseen', () => {
  it('정상 저장분 복원', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
    expect(parseUnseen(raw)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: '2026-07-01T00:00:00+00:00' });
  });
  it('없음·손상은 미시드(null) — 클라이언트 시계로 워터마크를 만들지 않는다', () => {
    expect(parseUnseen(null)).toEqual({ diaryDates: [], guestbookLastSeen: null });
    expect(parseUnseen('{broken').guestbookLastSeen).toBeNull();
    expect(parseUnseen('{"diaryDates":"x"}').diaryDates).toEqual([]);
  });
  it('저장된 null을 복원하며 diaryDates를 보존한다', () => {
    const raw = JSON.stringify({ diaryDates: ['2026-07-28'], guestbookLastSeen: null });
    expect(parseUnseen(raw)).toEqual({ diaryDates: ['2026-07-28'], guestbookLastSeen: null });
  });
});
```

세 번째 케이스가 회귀 방지의 핵심이다. 검증이 `typeof v.guestbookLastSeen === 'string'`만 통과시키면 저장된 `null`에서 **전체 상태를 버려 `diaryDates`까지 날린다.**

`guestbook unseen` 블록의 기존 `클리어 = lastSeen 갱신` 케이스를 고치고, 미시드 케이스를 추가한다.

```ts
  it('미시드(null)면 기존 글을 신규로 세지 않는다 — 이미 있는 건 다 본 것', () => {
    const ids = newGuestbookIds(
      [e('a', 'other', '2026-07-01T00:00:00+00:00'), e('b', 'other', '2026-07-29T12:00:00+00:00')],
      'me', null,
    );
    expect(ids).toEqual([]);
  });
  it('클리어 = lastSeen 갱신', () => {
    const s = clearGuestbookSeen(parseUnseen(null), '2026-07-30T00:00:00+00:00');
    expect(s.guestbookLastSeen).toBe('2026-07-30T00:00:00+00:00');
  });
```

`diary unseen` 블록의 `parseUnseen(null, NOW)`도 `parseUnseen(null)`로 바꾼다. `NOW` 상수는 `newGuestbookIds` 인자로 계속 쓰이므로 남긴다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run: `cd a-mate && npx vitest run src/lib/unseen.test.ts`

Expected: FAIL — `parseUnseen(null)`이 아직 `nowIso`를 요구하고 `guestbookLastSeen`이 `undefined`가 되어 `toEqual`이 어긋난다.

- [ ] **Step 3: unseen.ts를 nullable 시드로 바꾼다**

`a-mate/src/lib/unseen.ts:1-32`를 아래로 교체한다.

```ts
/** N1 탭 뱃지의 읽음 상태 (스펙 §5) — localStorage 영속은 lastSeen(방명록)·날짜 set(다이어리)만.
 *  방명록 unseen 카운트는 세션 내 entry_id set으로 재계산(부트스트랩+이벤트 dedup — 중복 카운트 구조적 차단). */
export interface UnseenState {
  diaryDates: string[]; // 아직 안 본 신규 일기 날짜들 — 일기는 앱 실행 중에만 생성되므로 이벤트 누적으로 완결
  guestbookLastSeen: string | null; // 마지막 확인 시각(서버 created_at 서식). null = 아직 시드 안 됨
}

const KEY = 'agent-mentor.tab-unseen';

/** 저장 원문 파싱 — 없음·손상은 **미시드(null)**. 클라이언트 시계로 워터마크를 만들면
 *  서버 created_at과의 사전순 비교가 시계 오차만큼 어긋난다(Q1). 시드는 첫 서버 조회가 한다. */
export function parseUnseen(raw: string | null): UnseenState {
  try {
    const v = JSON.parse(raw ?? 'null');
    if (
      v && Array.isArray(v.diaryDates) && v.diaryDates.every((d: unknown) => typeof d === 'string')
      && (v.guestbookLastSeen === null || typeof v.guestbookLastSeen === 'string')
    ) {
      return { diaryDates: v.diaryDates, guestbookLastSeen: v.guestbookLastSeen };
    }
  } catch {
    /* 손상 → 초기화 */
  }
  return { diaryDates: [], guestbookLastSeen: null };
}

export function loadUnseen(): UnseenState {
  return parseUnseen(localStorage.getItem(KEY));
}
```

- [ ] **Step 4: newGuestbookIds가 null을 받게 한다**

`a-mate/src/lib/unseen.ts:59-68`을 아래로 교체한다.

```ts
/** lastSeen 이후의 타인 글 entry_id — 부트스트랩(서버 조회)과 이벤트 payload 양쪽에 같은 판정.
 *  `lastSeenIso`가 null(미시드)이면 기존 글은 전부 "이미 본 것"으로 보고 아무것도 세지 않는다. */
export function newGuestbookIds(
  entries: { entry_id: string; author_agent_id: string; created_at: string }[],
  myAgentId: string,
  lastSeenIso: string | null,
): string[] {
  if (lastSeenIso === null) return [];
  return entries
    .filter((e) => e.author_agent_id !== myAgentId && e.created_at > lastSeenIso)
    .map((e) => e.entry_id);
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cd a-mate && npx vitest run src/lib/unseen.test.ts`

Expected: PASS.

- [ ] **Step 6: App.svelte 배선 — 첫 조회로 시드한다**

`a-mate/src/App.svelte:151-166`을 아래로 바꾼다.

```svelte
  // N1 탭 뱃지 — 다이어리는 날짜 set(영속), 방명록은 entry_id set(세션) + lastSeen(영속)
  let unseen = $state(loadUnseen());
  let gbUnseenIds = $state(new Set<string>());
  // 관측한 타인 글의 최신 서버 시각 — 클리어 시 워터마크로 쓴다(클라이언트 시계 배제, 단조 증가)
  let gbMaxSeenAt = $state<string | null>(null);
  const unseenDiary = $derived(unseen.diaryDates.length);
  const unseenGuestbook = $derived(gbUnseenIds.size);

  // 부트스트랩(서버 조회)과 guestbook:new 이벤트의 공통 반영 — entry_id dedup이라 중복 카운트 없음
  function observeGuestbook(rows: { entry_id: string; author_agent_id: string; created_at: string }[]) {
    const fresh = newGuestbookIds(rows, meId, unseen.guestbookLastSeen);
    if (fresh.length) gbUnseenIds = new Set([...gbUnseenIds, ...fresh]);
    const at = maxCreatedAt(rows, meId);
    if (at && (!gbMaxSeenAt || at > gbMaxSeenAt)) gbMaxSeenAt = at;
    // 최초 시드 — 첫 성공 조회의 서버 시각으로 워터마크를 세운다("이미 있는 건 다 본 것").
    // 글이 없으면 세울 값이 없으므로 미시드로 남고, 다음 첫 글이 정상적으로 신규가 된다.
    if (unseen.guestbookLastSeen === null && at) {
      unseen = clearGuestbookSeen(unseen, at);
      saveUnseen(unseen);
    }
  }
```

기존 152-153줄의 `loadUnseen(new Date().toISOString())` + 즉시 `saveUnseen(unseen)`은 **삭제한다.** 그 두 줄의 목적이 "초기 lastSeen을 클라이언트 시계로 고정"이었고, 그것이 바로 이 태스크가 없애는 동작이다.

- [ ] **Step 7: 전체 검증**

Run: `cd a-mate && npm test`

Expected: svelte-check **0 errors** + vitest **212 passed** (신규 2건: 저장된 null 복원 / 미시드 무카운트).

타입 체크가 `App.svelte`나 다른 소비처에서 오류를 뱉으면 그것이 이 태스크의 핵심 가치다 — 고치고 다시 돌린다.

Run: `cd a-mate && npm run build`

Expected: exit 0.

- [ ] **Step 8: 커밋**

```bash
git add a-mate/src/lib/unseen.ts a-mate/src/lib/unseen.test.ts a-mate/src/App.svelte
git commit -F- <<'EOF'
fix(frontend): seed the guestbook watermark from server time

On first run the badge watermark was initialised from the client clock and
immediately persisted. It is compared lexicographically against server-issued
`created_at`, so a clock skew of delta shifted the boundary: running fast hid
entries written during that window, running slow surfaced entries from before
the install.

It barely self-corrects. The watermark only moves to server time after a
badge is cleared, which requires a badge to appear and the user to open the
guestbook tab of their own room. Someone who never opens it keeps the seeded
value indefinitely.

`guestbookLastSeen` is now nullable, with null meaning not seeded yet, and the
first successful fetch seeds it from `max(created_at)` — everything already
there counts as read. An empty guestbook leaves it unseeded, which is correct:
the next entry should be new. `parseUnseen` accepts a stored null explicitly,
because folding it into the invalid branch would discard `diaryDates` along
with it.

The clamp alternative stays rejected: a user who has read everything satisfies
`lastSeen > max(created_at)` by definition, so clamping resurrects read
entries.

Only badge counts were affected. Home notices judge from the backend cursor,
which uses server time alone.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 4: 로드맵 갱신 + DoD 아카이브

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (8차 배치 Q1–Q3에 완료 표시 + 구현 결과)
- Move: 이 계획과 스펙을 `docs/archive/` 미러로 (docs-archive 스킬이 수행)

**Interfaces:**
- Consumes: Task 1–3의 커밋.
- Produces: 없음 (문서 전용).

- [ ] **Step 1: 로드맵 8차 배치에 완료 표시**

`docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md`의 Q1·Q2·Q3 제목 줄 끝에 각각 `— ✅ 완료(2026-07-31, 묶음 ⑥)`를 붙이고, 8차 배치 끝에 아래 문단을 추가한다. **실제 구현 결과와 어긋나면 실제를 따른다** — 아래는 계획 시점의 예상이며, 착지한 내용으로 고쳐 쓴다.

```markdown
**묶음 ⑥ 구현 결과(2026-07-31)**: Q3 도입 전 실측이 계획을 바꿨다 — 총 78 errors 중
**71건이 라이브러리 `.d.ts`**(패키지 간 전역 타입 충돌)라 `skipLibCheck: true`로 소거되고,
우리 코드는 **원인 4종 / 진단 7건**뿐이었다. 로드맵이 대비했던 단계 도입(`--threshold error`로
점진 축소)은 불필요했다. 게이트는 `npm test` 편입(`svelte-check --threshold error && vitest run`) —
CI가 없는 레포에서 단독 스크립트는 사문화되기 때문. 경고 13건은 출력에 남기되 실패로 치지 않는다
(`LifeView` `<polygon>` a11y 6건은 방 격자 키보드 대응이라 별도 UI 과제).
드러난 실오류 중 **`HubSettings.api_key` 누락은 진짜 드리프트**였다 — 백엔드
`commands.rs:817`이 실제로 보내고 설정 화면이 읽는데 TS 선언에만 없었다(런타임 정상, 선언이 거짓).
`catalog.ts`의 튜플 캐스팅은 `as unknown as` 대신 **`ground: number[]` 완화**로 처리 —
사용처가 전부 인덱스 접근이고 이중 캐스팅은 길이 3 배열도 통과시키는 거짓말이 남기 때문.
Q1은 `guestbookLastSeen`을 nullable로 바꿔 **첫 성공 조회의 `max(created_at)`으로 시드**한다.
`parseUnseen`이 저장된 `null`을 **명시적으로 허용**해야 한다는 점이 구현 중 확인됐다 —
무효 분기로 뭉개면 `diaryDates`까지 함께 버려지는 회귀가 생긴다.
Q2는 `resolveState`에 `visit` 분기 추가(3줄). 검증: 프론트 212 · `npm run build` · `cargo test` 그린.
```

- [ ] **Step 2: 로드맵 갱신 커밋**

```bash
git add docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md
git commit -F- <<'EOF'
docs(docs): record bundle 6 completion in the roadmap

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

- [ ] **Step 3: docs-archive 스킬로 작업 문서 아카이브**

`docs-archive` 스킬을 호출해 이번 묶음의 스펙·계획을 `docs/archive/` 미러로 옮긴다 (ADR 0013의 DoD 규칙). 로드맵 문서 자체는 **아카이브하지 않는다** — 아직 미완 항목(D1·X1)이 남아 있다.

- [ ] **Step 4: 최종 검증**

Run: `cd a-mate && npm test && npm run build`

Expected: 0 errors + 212 passed + build exit 0.

Run: `cd a-mate && cargo test`

Expected: exit 0 (미접촉 확인용).

---

## 검증 요약

| 시점 | 명령 | 기대 |
|---|---|---|
| 베이스라인 | `npm test` / `cargo test` | 210 passed / exit 0 (2026-07-31 실측) |
| Task 1 후 | `npm test` | svelte-check 0 errors + 210 passed |
| Task 2 후 | `npm test` | 0 errors + 210 passed (단언만 추가) |
| Task 3 후 | `npm test` | 0 errors + **212 passed** |
| 최종 | `npm run build` · `cargo test` | exit 0 |

## PR 체크리스트 (사용자 확인)

- [ ] **선택** — Q2 육안 확인: 01\~07시에 누군가 내 방에 다녀가면 마스코트가 자는 표정이 아니라 웃는 표정으로 알린다. 재현이 까다로워 단위 테스트로 덮었으므로 필수는 아니다.
- [ ] Q1 회귀 확인: 앱 재시작 후에도 다이어리 뱃지 상태가 유지된다 (저장된 `null` 복원 시 `diaryDates` 보존).
