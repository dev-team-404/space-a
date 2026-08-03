---
status: done
archived: 2026-08-03
---

# 코칭 탭 통일 PR① — 잔재 정리 + 근거 칩 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 등록된 룰이 전부 `est_tokens_saved = 0`이라 항상 "~0 tok"으로 렌더되던 죽은 절약 표시를 4곳에서 걷어내고, 그 자리를 룰별 evidence 실수치를 보여주는 근거 칩으로 대체한다.

**Architecture:** 순수 함수 `evidenceChip(ruleId, evidence)`를 `coach-helpers.ts`에 추가하고 `CoachTab.svelte` 헤더가 그것을 렌더한다. 나머지 3곳(홈 위젯·마스코트 말풍선·홈 배지)은 표시만 삭제한다. 이 저장소엔 컴포넌트 테스트 라이브러리가 없으므로, `.svelte` 변경은 `no-hardcoded-colors.test.ts`가 쓰는 **소스 스캔 테스트** 패턴으로 회귀를 고정한다.

**Tech Stack:** Svelte 5 (runes) + TypeScript + Vitest. `npm test` = `svelte-check --threshold error && vitest run`.

## Global Constraints

- **작업 위치**: 워크트리 `D:\Project\space-a\.claude\worktrees\coaching-tab-unification`. 원본 저장소 루트로 `cd` 하지 않는다.
- **플랫폼**: Windows 전용. 빌드·테스트는 네이티브 PowerShell에서. WSL 안에서 빌드/실행 금지.
- **명령은 `a-mate/`에서 실행**한다. 테스트는 `npm test` 하나로 svelte-check + vitest가 함께 돈다.
- **`est_tokens_saved` 컬럼·룰 필드·정렬 사용은 남긴다.** `hub.rs:154`가 `est_tokens_saved > 0`을 공유 문턱 분기로 쓰고 있어 제거는 이번 범위 밖이다. **UI 표시만** 걷어낸다.
- **근거 칩 재료는 룰별 evidence 실수치만.** `occurrences`·`last_seen`은 스캔마다 갱신되는 스캔 지표라 절대 쓰지 않는다 (스펙 §1.2 D5).
- **없는 정보를 지어내지 않는다.** evidence 키가 없거나 숫자가 아니거나 0이면 칩을 **생략**한다. 0이나 추정치를 표시하지 않는다.
- **CSS는 하드코딩 hex 금지.** `var(--...)`만 쓴다 (`no-hardcoded-colors.test.ts`가 강제). 전경 `color`에 `var(--accent)` 금지 — `var(--accent-strong)`을 쓴다.
- **커밋 메시지는 Conventional Commits, 영어.** `<type>(<scope>): <subject>` — 소문자 시작, 마침표 없음.
- **베이스라인은 이미 확인됨** (2026-08-02): `npm test` exit 0, `cargo test` exit 0. 이 PR은 Rust를 건드리지 않는다.

**참조 스펙:** `docs/archive/design/a-mate/specs/2026-08-02-coaching-tab-unification-design.md` §1.2(D1·D2·D3·D5), §4.1(근거 칩), §8(잔재 정리 목록), §11(PR 분할).

---

## File Structure

| 파일 | 책임 | 변경 |
|------|------|------|
| `a-mate/src/lib/ui/coach-helpers.ts` | 코칭 카드용 순수 헬퍼 (evidence 파싱·제목 매핑) | `evidenceChip()` 추가, `COACH_TITLE` 정리 |
| `a-mate/src/lib/ui/coach-helpers.test.ts` | 위 헬퍼의 단위 테스트 | 케이스 추가·은퇴 룰 테스트 교체 |
| `a-mate/src/lib/ui/CoachTab.svelte` | 코칭 탭 렌더링 | 헤더의 `~N tok` → 근거 칩 |
| `a-mate/src/lib/ui/home/SaveTop3.svelte` | 홈 위젯 — 코칭 카드 바로가기 3건 | 절약 수치 삭제, 제목 교정 |
| `a-mate/src/lib/ui/HomeTab.svelte` | 홈 탭 | 죽은 「절약 가능」 배지 삭제 |
| `a-mate/src/lib/robot/bubble.ts` | 마스코트 말풍선 문구 팩토리 | `(~N tok)` 삭제, `RULE_LINE` 정리 |
| `a-mate/src/lib/robot/bubble.test.ts` | 말풍선 단위 테스트 | 은퇴 룰 케이스 교체 |
| `a-mate/src/lib/no-savings-display.test.ts` | **신규** — 절약 수치 표시 회귀 방지 소스 스캔 | 신규 생성 |

---

## Task 1: 근거 칩 헬퍼 `evidenceChip()`

카드 헤더에 표시할 "근거의 크기" 문자열을 룰별 evidence에서 뽑는 순수 함수. 이 저장소의 `sessionIdsOf`·`totalSessionsOf`와 같은 방어적 파싱 관례를 따른다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts`
- Test: `a-mate/src/lib/ui/coach-helpers.test.ts`

**Interfaces:**
- Consumes: 없음 (첫 태스크)
- Produces: `export function evidenceChip(ruleId: string, evidence: unknown): string | null` — 표시할 칩 문자열, 또는 칩을 생략해야 하면 `null`. Task 2의 `CoachTab.svelte`가 소비한다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`a-mate/src/lib/ui/coach-helpers.test.ts` 맨 아래에 아래 블록을 **추가**한다. 파일 상단 import 줄도 `evidenceChip`을 포함하도록 바꾼다.

기존 1행:
```ts
import { coachTitle, ctxLine, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
```
바꾼 뒤:
```ts
import { coachTitle, ctxLine, evidenceChip, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
```

파일 끝에 추가:
```ts
describe('evidenceChip', () => {
  it('R6은 세션 수를 센다', () => {
    expect(evidenceChip('R6', { session_count: 4 })).toBe('4개 세션');
  });
  it('R7은 집계된 세션 수를 센다', () => {
    expect(evidenceChip('R7', { total_sessions: 5 })).toBe('5개 세션');
  });
  it('R8은 대형 결과 횟수를 센다', () => {
    expect(evidenceChip('R8', { large_result_count: 12 })).toBe('12회 대형 결과');
  });
  it('occurrences·last_seen은 스캔 지표라 칩 재료가 아니다', () => {
    // 룰별 evidence 키가 없으면 다른 값이 있어도 칩은 없다
    expect(evidenceChip('R6', { occurrences: 99, last_seen: '2026-08-02' })).toBeNull();
  });
  it('키가 없거나 숫자가 아니거나 0이면 칩을 생략한다', () => {
    expect(evidenceChip('R6', {})).toBeNull();
    expect(evidenceChip('R6', { session_count: '4' })).toBeNull();
    expect(evidenceChip('R6', { session_count: 0 })).toBeNull();
    expect(evidenceChip('R7', { total_sessions: Number.NaN })).toBeNull();
  });
  it('evidence가 객체가 아니면 칩을 생략한다', () => {
    expect(evidenceChip('R6', null)).toBeNull();
    expect(evidenceChip('R6', 'junk')).toBeNull();
    expect(evidenceChip('R6', undefined)).toBeNull();
  });
  it('칩 규칙이 없는 룰은 생략한다', () => {
    expect(evidenceChip('RX', { session_count: 4 })).toBeNull();
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts`
Expected: FAIL — `evidenceChip`이 `./coach-helpers`에 없어 svelte-check/TS 오류 또는 `evidenceChip is not a function`

- [ ] **Step 3: 최소 구현을 작성한다**

`a-mate/src/lib/ui/coach-helpers.ts`의 `totalSessionsOf` 함수 **바로 아래**에 추가한다 (evidence 파서들끼리 모아 둔다).

```ts
/** evidence에서 유한한 숫자 키를 안전하게 꺼낸다. 없거나 숫자가 아니면 null. */
function numberOf(evidence: unknown, key: string): number | null {
  if (typeof evidence !== 'object' || evidence === null) return null;
  const v = (evidence as Record<string, unknown>)[key];
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

/** 카드 헤더의 근거 칩 — "근거가 얼마나 굳어졌나"를 룰별 evidence 실수치로 보여준다.
 * 절약 토큰(est_tokens_saved)은 등록된 룰이 전부 0이라 표시하지 않는다.
 * occurrences·last_seen은 스캔마다 갱신되는 **스캔 지표**라 여기 쓰지 않는다.
 * 재료가 없으면 null — 0이나 추정치를 지어내지 않는다. */
export function evidenceChip(ruleId: string, evidence: unknown): string | null {
  const count = (key: string, suffix: string): string | null => {
    const v = numberOf(evidence, key);
    return v !== null && v > 0 ? `${v}${suffix}` : null;
  };
  switch (ruleId) {
    case 'R6':
      return count('session_count', '개 세션');
    case 'R7':
      return count('total_sessions', '개 세션');
    case 'R8':
      return count('large_result_count', '회 대형 결과');
    default:
      return null;
  }
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts`
Expected: PASS — `coach-helpers` 파일의 모든 테스트 통과

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(frontend): add evidence chip helper for coaching cards"
```

---

## Task 2: 절약 수치 표시 4곳 제거 + 근거 칩 적용

네 곳이 같은 죽은 값을 렌더한다. 하나의 논리적 변경이므로 한 태스크로 묶고, 소스 스캔 회귀 테스트가 RED → GREEN을 관장한다.

**Files:**
- Create: `a-mate/src/lib/no-savings-display.test.ts`
- Modify: `a-mate/src/lib/ui/CoachTab.svelte` (import 2행, 마크업 149-153행, `.save` 스타일 272행)
- Modify: `a-mate/src/lib/ui/home/SaveTop3.svelte` (4·8·18행, `.save` 스타일 40행)
- Modify: `a-mate/src/lib/ui/HomeTab.svelte` (80-82행)
- Modify: `a-mate/src/lib/robot/bubble.ts` (30행)

**Interfaces:**
- Consumes: `evidenceChip(ruleId: string, evidence: unknown): string | null` (Task 1)
- Produces: 없음 (UI 변경만). `findingBubble`·`SaveTop3`의 시그니처와 props는 **바꾸지 않는다** — 정렬은 그대로 두고 표시만 뺀다.

- [ ] **Step 1: 실패하는 회귀 테스트를 작성한다**

`a-mate/src/lib/no-savings-display.test.ts`를 새로 만든다. 경로 관례는 `no-hardcoded-colors.test.ts`(`process.cwd()` 기준)를 따른다.

```ts
import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// 등록된 룰(R6·R7·R8)이 전부 est_tokens_saved=0이라 "~N tok"은 언제나 "~0 tok"으로 렌더됐다.
// 절약 수치를 화면에 보여주는 코드가 다시 들어오는 것을 막는다 (스펙 §1.2 D1·D2).
// 정렬·타입 정의용 사용은 허용 — 사용자에게 **보여주는** 것만 금지한다.

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (/\.(svelte|ts)$/.test(name) && !name.endsWith('.test.ts')) {
      out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
    }
  }
}

/** .svelte에서 <script>·<style>를 뺀 마크업만 — 여기 남은 것은 곧 화면에 나간다. */
function markup(src: string): string {
  return src
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<style[\s\S]*?<\/style>/gi, '');
}

/** .ts에서 템플릿 리터럴만 — 사용자 문구를 조립하는 자리. */
function templateLiterals(src: string): string {
  return (src.match(/`[^`]*`/g) ?? []).join('\n');
}

const SAVINGS = /est_tokens_saved/;

describe('절약 수치는 화면에 표시하지 않는다', () => {
  const base = process.cwd();
  const files: string[] = [];
  walk(join(base, 'src'), base, files);

  it('스캔 대상 파일이 존재한다', () => {
    expect(files.length).toBeGreaterThan(10);
  });

  it('est_tokens_saved가 화면 문자열에 삽입되지 않는다', () => {
    const offenders: string[] = [];
    for (const rel of files) {
      const src = readFileSync(join(base, rel), 'utf8');
      const rendered = rel.endsWith('.svelte') ? markup(src) : templateLiterals(src);
      rendered.split(/\r?\n/).forEach((line) => {
        if (SAVINGS.test(line)) offenders.push(`${rel}  ${line.trim()}`);
      });
    }
    expect(offenders, `절약 수치 표시 발견:\n${offenders.join('\n')}`).toEqual([]);
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/no-savings-display.test.ts`
Expected: FAIL — offenders 4건이 나열된다:
```
lib/ui/CoachTab.svelte      <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
lib/ui/HomeTab.svelte       <span class="save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</span>
lib/ui/home/SaveTop3.svelte <span class="save">~{f.est_tokens_saved.toLocaleString()}</span>
lib/robot/bubble.ts         `${honorific}, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`
```

- [ ] **Step 3-a: `CoachTab.svelte` — 헤더를 근거 칩으로 교체한다**

2행 import에 `evidenceChip`을 추가한다.

기존:
```ts
  import { coachTitle, ctxLine, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
```
변경:
```ts
  import { coachTitle, ctxLine, evidenceChip, isHiddenFinding, sessionIdsOf, totalSessionsOf } from './coach-helpers';
```

148-153행의 `{#each}` 머리와 헤더를 교체한다. `{@const}`는 `{#each}`의 **직계 자식**이어야 하므로 `<article>` 앞에 둔다.

기존:
```svelte
    {#each active as f (f.dedup_key)}
      <article class="card" class:warn={f.severity === 'warn'} data-key={f.dedup_key}>
        <header>
          <span class="title">{icon(f.severity)} {coachTitle(f.rule_id, f.evidence)}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </header>
```
변경:
```svelte
    {#each active as f (f.dedup_key)}
      {@const chip = evidenceChip(f.rule_id, f.evidence)}
      <article class="card" class:warn={f.severity === 'warn'} data-key={f.dedup_key}>
        <header>
          <span class="title">{icon(f.severity)} {coachTitle(f.rule_id, f.evidence)}</span>
          {#if chip}<span class="chip">{chip}</span>{/if}
        </header>
```

272행의 `.save` 스타일을 `.chip`으로 바꾼다. 근거 칩은 성취가 아니라 중립 정보이므로 accent 대신 `--ink-soft`를 쓰고, `TipCard.svelte`의 `.minibadge`와 같은 알약 모양을 따른다.

기존:
```css
  .save { color: var(--accent-strong); font-size: 12px; white-space: nowrap; }
```
변경:
```css
  .chip {
    color: var(--ink-soft); font-size: 11px; white-space: nowrap; flex: none;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px; padding: 1px 7px;
  }
```

- [ ] **Step 3-b: `SaveTop3.svelte` — 절약 수치 삭제 + 제목 교정**

절약 수치가 사라지면 「절약 실천 top3」라는 제목이 화면 내용과 어긋난다. 이 위젯의 실제 역할(코칭 카드 바로가기)에 맞게 제목도 함께 고친다.

기존 4행:
```svelte
  const top3 = $derived(findings.slice(0, 3)); // 커맨드가 est_tokens_saved 내림차순 정렬을 보장
```
변경:
```svelte
  const top3 = $derived(findings.slice(0, 3)); // 커맨드가 정렬을 보장 — 앞에서 3건
```

기존 8행:
```svelte
  <h3>절약 실천 top3</h3>
```
변경:
```svelte
  <h3>지금 볼 코칭</h3>
```

기존 14-19행:
```svelte
        <li>
          <button onclick={() => onGoto(f.dedup_key)}>
            <span class="rank">{i + 1}</span>
            <span class="action">{f.suggested_action}</span>
            <span class="save">~{f.est_tokens_saved.toLocaleString()}</span>
          </button>
        </li>
```
변경:
```svelte
        <li>
          <button onclick={() => onGoto(f.dedup_key)}>
            <span class="rank">{i + 1}</span>
            <span class="action">{f.suggested_action}</span>
          </button>
        </li>
```

40행의 `.save` 규칙은 우리 변경으로 고아가 되므로 **삭제**한다.
```css
  .save { color: var(--ink-soft); font-size: 11px; white-space: nowrap; }
```

- [ ] **Step 3-c: `HomeTab.svelte` — 죽은 배지 삭제**

`> 0` 가드 때문에 이미 절대 렌더되지 않는 죽은 코드다. 80-82행 블록을 통째로 삭제한다.

```svelte
    {#if (summary?.est_tokens_saved_total ?? 0) > 0}
      <span class="save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</span>
    {/if}
```

이 삭제로 133행의 스타일이 고아가 된다. 함께 삭제한다.
```css
  .strip .save b { color: var(--accent-strong); }
```
(`HomeTab.svelte`에 `.save`를 쓰는 곳은 이 배지 하나뿐임을 확인했다. 삭제 후 `grep -n "\.save" src/lib/ui/HomeTab.svelte` 결과가 비어야 한다.)

- [ ] **Step 3-d: `bubble.ts` — 말풍선에서 `(~N tok)` 삭제**

기존 30행:
```ts
    text: `${honorific}, ${line} (~${top.est_tokens_saved.toLocaleString()} tok)${more}`,
```
변경:
```ts
    text: `${honorific}, ${line}${more}`,
```

`findingBubble`의 파라미터 타입과 23행의 정렬(`b.est_tokens_saved - a.est_tokens_saved`)은 **건드리지 않는다** — 값이 전부 0이면 안정 정렬이라 입력 순서(커맨드의 `est_tokens_saved DESC, dedup_key`)가 유지되어 결정론적이다. 정렬 기준 교체는 이 PR의 범위가 아니다.

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npm test`
Expected: PASS — svelte-check 오류 0, vitest 전체 통과. 특히 `no-savings-display.test.ts`의 offenders가 빈 배열.

`bubble.test.ts`가 실패하면 문구 단언이 `(~30,000 tok)`을 기대하고 있는지 확인한다. 현재 단언(`toContain('외 1건')`, `toContain('MCP')`, `startsWith('대장,')`)은 절약 수치를 검사하지 않으므로 그대로 통과해야 한다.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/no-savings-display.test.ts a-mate/src/lib/ui/CoachTab.svelte a-mate/src/lib/ui/home/SaveTop3.svelte a-mate/src/lib/ui/HomeTab.svelte a-mate/src/lib/robot/bubble.ts
git commit -m "fix(frontend): drop the dead ~0 tok savings display"
```

---

## Task 3: 은퇴 룰 문구 정리

`COACH_TITLE`에 은퇴 룰 7개(R1·R2·R5·R9·R10·R11·R12)가 죽은 채 남아 있다. `bubble.ts`의 `RULE_LINE`도 같은 문제인데다 **살아있는 R6·R8이 빠져 있어** 그 카드의 말풍선이 일반 문구로 새어 나간다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts` (`COACH_TITLE`, `subtypeOf`, `coachTitle`)
- Modify: `a-mate/src/lib/ui/coach-helpers.test.ts` (은퇴 룰 케이스 교체)
- Modify: `a-mate/src/lib/robot/bubble.ts` (`RULE_LINE`)
- Modify: `a-mate/src/lib/robot/bubble.test.ts` (은퇴 룰 케이스 교체)

**Interfaces:**
- Consumes: 없음
- Produces: `coachTitle(ruleId: string, evidence: unknown): string` — 시그니처 불변. 등록 룰(R6·R7·R8)만 고유 제목을 갖고 나머지는 `'아낄 수 있는 게 보여요'`로 폴백한다. `evidence` 파라미터는 **유지**한다 (Task 2의 `CoachTab.svelte` 호출부가 그대로 넘긴다).

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`a-mate/src/lib/ui/coach-helpers.test.ts`의 `describe('coachTitle', ...)` 블록(38-54행)을 통째로 아래로 교체한다.

```ts
describe('coachTitle', () => {
  it('등록된 룰만 고유 제목을 갖는다', () => {
    expect(coachTitle('R6', {})).toContain('같은 지시');
    expect(coachTitle('R7', {})).toContain('모델');
    expect(coachTitle('R8', {})).toContain('MCP');
  });
  it('은퇴한 룰은 폴백 제목 — 죽은 매핑을 남기지 않는다', () => {
    for (const retired of ['R1', 'R2', 'R5', 'R9', 'R10', 'R11', 'R12', 'R24']) {
      expect(coachTitle(retired, {}), retired).toBe('아낄 수 있는 게 보여요');
    }
  });
  it('알 수 없는 rule → 폴백', () => {
    expect(coachTitle('RX', {})).toBe('아낄 수 있는 게 보여요');
  });
});
```

`a-mate/src/lib/robot/bubble.test.ts`의 첫 두 케이스(5-14행, 22-29행)에서 은퇴 룰을 등록 룰로 바꾼다.

기존 5-14행:
```ts
  it('finding: 최대 절약 1건 + 외 N건, coach 탭, top dedup_key가 target', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest', dedup_key: 'k-r5' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn', dedup_key: 'k-r1' },
    ], '주인');
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
    expect(b.target).toBe('k-r1');
  });
```
변경:
```ts
  it('finding: 대표 1건 + 외 N건, coach 탭, top dedup_key가 target', () => {
    const b = findingBubble([
      { rule_id: 'R6', est_tokens_saved: 0, severity: 'warn', dedup_key: 'k-r6' },
      { rule_id: 'R8', est_tokens_saved: 0, severity: 'suggest', dedup_key: 'k-r8' },
    ], '주인');
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text).toContain('반복'); // R6 문구가 대표 (동점이면 입력 순서 유지)
    expect(b.target).toBe('k-r6');
    expect(b.text).not.toContain('tok'); // 절약 수치 표시 안 함
  });
```

> **주의**: 두 항목의 `est_tokens_saved`가 모두 0이면 `Array.prototype.sort`는 안정 정렬이라 **입력 순서가 유지**된다. 따라서 대표는 배열 첫 항목인 `k-r6`이고, 문구도 R6의 것이 나온다. 순서를 바꾸면 두 단언을 함께 바꿔야 한다.

기존 22-29행 중 첫 두 줄:
```ts
    const f = findingBubble([{ rule_id: 'R1', est_tokens_saved: 100, severity: 'warn', dedup_key: 'k' }], '대장');
```
변경:
```ts
    const f = findingBubble([{ rule_id: 'R6', est_tokens_saved: 0, severity: 'warn', dedup_key: 'k' }], '대장');
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts src/lib/robot/bubble.test.ts`
Expected: FAIL —
- `coachTitle('R1', {})`이 아직 `'안 쓰는 MCP 서버가 토큰을 먹고 있어요'`를 반환해 폴백 단언 실패
- `coachTitle('R8', {})`이 이미 'MCP'를 포함하므로 그 줄은 통과, `R7`의 '모델'도 통과
- `bubble.text`가 `'반복'`을 포함하지 않아 실패 (`RULE_LINE`에 R6이 없어 폴백 문구가 나감)

- [ ] **Step 3: 구현한다**

`a-mate/src/lib/ui/coach-helpers.ts`의 `COACH_TITLE`을 등록 룰만 남긴다.

기존 28-39행:
```ts
const COACH_TITLE: Record<string, string> = {
  R1: '안 쓰는 MCP 서버가 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '세션 안에서 같은 파일을 여러 번 다시 읽었어요',
  R7: '이 프로젝트, 가벼운 작업엔 시작 모델을 낮춰보세요',
  R9: '웹 검색이 너무 잦아요',
  R10: '자동화 파이프라인이 Opus로 돌고 있어요',
  R6: '같은 지시를 여러 세션에서 반복하고 있어요',
  R8: '큰 MCP 결과가 매번 컨텍스트를 잡아먹어요',
  R11: '거부한 뒤 결국 허용한 도구가 있어요',
  R12: '설치해둔 스킬이 놀고 있어요',
};
```
변경 (`ops::registered_rules()`가 등록한 R6·R7·R8만):
```ts
// ops::registered_rules()에 등록된 룰만 — 은퇴 룰(R1·R2·R5·R9·R10·R11·R12·R24)은
// 매 스캔 purge되어 카드가 뜨지 않으므로 제목도 두지 않는다.
const COACH_TITLE: Record<string, string> = {
  R6: '같은 지시를 여러 세션에서 반복하고 있어요',
  R7: '이 프로젝트, 가벼운 작업엔 시작 모델을 낮춰보세요',
  R8: '큰 MCP 결과가 매번 컨텍스트를 잡아먹어요',
};
```

41-52행의 `subtypeOf`와 `coachTitle`의 R5 분기를 삭제한다. `subtypeOf`는 R5 전용이라 우리 변경으로 고아가 된다.

기존 41-52행:
```ts
function subtypeOf(evidence: unknown): string | null {
  if (typeof evidence !== 'object' || evidence === null) return null;
  const s = (evidence as Record<string, unknown>).subtype;
  return typeof s === 'string' ? s : null;
}

export function coachTitle(ruleId: string, evidence: unknown): string {
  if (ruleId === 'R5' && subtypeOf(evidence) === 'cross_session_claude_md') {
    return '여러 세션에서 반복해 읽는 파일 — CLAUDE.md에 넣어두면 좋겠어요';
  }
  return COACH_TITLE[ruleId] ?? '아낄 수 있는 게 보여요';
}
```
변경:
```ts
/** 두 번째 인자는 룰별 evidence 분기 자리 — 지금은 분기가 없지만 호출부(CoachTab)
 * 시그니처를 유지해 R5 같은 subtype 분기가 다시 생길 때 흔들리지 않게 둔다. */
export function coachTitle(ruleId: string, _evidence: unknown): string {
  return COACH_TITLE[ruleId] ?? '아낄 수 있는 게 보여요';
}
```

`tsconfig.json`에 `noUnusedParameters`가 없어(`strict`만 켜져 있음) 미사용 파라미터는 오류가 아니다. `_` 접두는 의도를 드러내기 위한 관례일 뿐이다.

`a-mate/src/lib/robot/bubble.ts`의 `RULE_LINE`(11-17행)도 등록 룰로 맞춘다.

기존:
```ts
const RULE_LINE: Record<string, string> = {
  R1: '안 쓰는 MCP가 상주 토큰을 먹고 있어요',
  R2: '안 쓰는 플러그인이 자리만 차지해요',
  R5: '같은 파일을 반복해서 읽고 있어요',
  R7: '단순 작업에 Opus는 과해요',
  R9: '웹 검색을 너무 많이 돌렸어요',
};
```
변경:
```ts
// 등록 룰(R6·R7·R8)만 — 은퇴 룰은 카드가 안 뜨고, 빠진 룰은 아래 폴백 문구로 샌다.
const RULE_LINE: Record<string, string> = {
  R6: '같은 지시를 반복하고 있어요',
  R7: '단순 작업에 Opus는 과해요',
  R8: '큰 MCP 결과가 컨텍스트를 잡아먹어요',
};
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npm test`
Expected: PASS — svelte-check 오류 0, vitest 전체 통과

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts a-mate/src/lib/robot/bubble.ts a-mate/src/lib/robot/bubble.test.ts
git commit -m "chore(frontend): prune copy for retired coaching rules"
```

---

## 완료 조건

- [ ] `cd a-mate; npm test` — svelte-check 오류 0, vitest 전체 통과
- [ ] `cd a-mate; cargo test` — 변경 없음이지만 회귀 확인 (베이스라인 exit 0)
- [ ] 코칭 탭에 `~0 tok`이 어디에도 보이지 않는다
- [ ] R6 카드 헤더에 `N개 세션` 칩이 뜬다 (evidence에 `session_count`가 있을 때)
- [ ] 홈 위젯 제목이 「지금 볼 코칭」이고 우측 수치가 없다
- [ ] `docs-archive` 대상 아님 — PR②③④가 남아 있으므로 스펙은 활성 상태로 둔다

## 이 PR에서 하지 않는 것

스펙 §11의 PR② 이후 항목이다. 이 계획에 섞지 않는다.

- 2단 구조(「내 로그에서」/「배움 · 소식」) 분리 — PR②
- `TipCard` 해체·카드 문법 통일·개인 레슨 처분 편입 — PR②
- 수명 규칙·재발 감지·R6 앵커 드리프트 억제 — PR③
- 로컬 공지 소식 파이프라인·피드 TTL — PR④
- `est_tokens_saved` 컬럼 제거 — 범위 밖 (`hub.rs:154`가 사용 중)
- `findingBubble`·`notices.ts`의 정렬 기준 교체 — 값이 전부 0이라 결정론적이지만 의미는 없다. 정렬 기준을 무엇으로 바꿀지는 제품 결정이라 별도 논의
