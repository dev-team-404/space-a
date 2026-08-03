---
status: done
archived: 2026-08-03
---

# 코칭 탭 통일 PR② — 표현 통일(2단 섹션 + 카드 문법) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 코칭 탭을 근거 출처로 2단(「내 로그에서」/「배움 · 소식」) 분리하고, 「오늘의 배움」 컨테이너 1장에 4개 항목이 뭉쳐 있던 구조를 항목마다 독립 카드로 해체해 카드 문법을 2종으로 통일한다.

**Architecture:** 이 저장소엔 컴포넌트 테스트 라이브러리가 없다. 따라서 **분기·정규화 로직을 전부 `.ts` 순수 함수로 빼고** Svelte 컴포넌트는 뷰모델을 받아 그리기만 한다. `partitionCoachItems()`가 항목을 두 섹션으로 가르고, `toLogCardView()`/`toLearnCardView()`가 `CoachFinding`·`ContentItem`이라는 다른 두 타입을 공통 뷰모델로 정규화한다. 행동 버튼은 기존 핸들러가 있는 `CoachTab`에 남기고 Svelte 5 snippet으로 카드에 주입한다.

**Tech Stack:** Svelte 5 (runes + snippets) + TypeScript + Vitest. `npm test` = `svelte-check --threshold error && vitest run`.

## Global Constraints

- **작업 위치**: 워크트리 `.claude/worktrees/coaching-tab-two-sections` (base `236991b` = PR①·④ 머지된 main). 원본 저장소 루트로 `cd` 하지 않는다.
- **플랫폼**: Windows 전용. 빌드·테스트는 네이티브 PowerShell에서. WSL 안에서 금지.
- **명령은 `a-mate/`에서 실행**한다. `npm test` 하나로 svelte-check + vitest가 함께 돈다.
- **베이스라인 확인 필수**: 새 워크트리엔 `node_modules`가 없다. `npm ci`를 먼저 돌려라.
  ⚠ 설치 전 `npm test`는 *"'svelte-check'은(는) 내부 또는 외부 명령이 아닙니다"* 만 출력하고 **종료 코드 0**으로 끝난다. 종료 코드를 믿지 말고 출력에 `Tests  N passed`가 있는지 눈으로 확인한다. 기준값(`236991b` 실측): **프론트 239건, `cargo test` 652건**(604 + 48).
- **이 PR은 Rust를 건드리지 않는다.** 백엔드 변경은 PR③·⑥ 몫이다.
- **개인 레슨의 처분은 기존 `✕`(dismissed) 그대로 둔다.** `[해결함]`을 붙이려면 `content_items`에 `resolved` 어휘가 필요한데 그건 PR③이다. 같은 섹션에서 finding은 `[해결함][무시]`, 레슨은 `[✕]`인 **의도된 중간 상태**다.
- **CSS는 하드코딩 hex 금지.** `var(--...)`만. 전경 `color`에 `var(--accent)` 금지 — `var(--accent-strong)`을 쓴다 (`no-hardcoded-colors.test.ts`가 강제).
- **절약 수치를 다시 렌더하지 않는다.** `no-savings-display.test.ts`가 막고 있다.
- **커밋 메시지는 Conventional Commits, 영어.**

**참조 스펙:** `docs/design/a-mate/specs/2026-08-02-coaching-tab-unification-design.md` §3(최종 구조), §4(카드 문법 2종), §11(PR 분할).

---

## File Structure

| 파일 | 책임 | 변경 |
|------|------|------|
| `a-mate/src/lib/ui/coach-helpers.ts` | 코칭 카드 순수 헬퍼 | 본문 파싱·섹션 분리·뷰모델 정규화 추가 |
| `a-mate/src/lib/ui/coach-helpers.test.ts` | 위 단위 테스트 | 케이스 추가 |
| `a-mate/src/lib/ui/coach/LogCard.svelte` | **신규** — 문법 A(로그 카드) 렌더 | 생성 |
| `a-mate/src/lib/ui/coach/LearnCard.svelte` | **신규** — 문법 B(카드뉴스) 렌더 | 생성 |
| `a-mate/src/lib/ui/CoachTab.svelte` | 코칭 탭 조립 — 2단 섹션·행동 버튼·모달 | 재구성 |
| `a-mate/src/lib/ui/home/TipCard.svelte` | 「오늘의 배움」 컨테이너 | **삭제** (`CoachTab`이 유일한 사용처) |

---

## Task 1: 레슨 본문 파서 `splitLessonBody()`

개인 실전 레슨의 `body`는 `당신 로그: … / • 원리: … / • 이렇게: …` 구조다. 문법 A의 슬롯(📊 근거 / 원리 / ➜ 행동)에 나눠 담으려면 쪼개야 한다. 마커가 없는 레슨(칭찬·plugin 추천)도 있으므로 **폴백이 필수**다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts`
- Test: `a-mate/src/lib/ui/coach-helpers.test.ts`

**Interfaces:**
- Consumes: 없음
- Produces:
  ```ts
  export interface LessonBody { evidence: string | null; principle: string | null; action: string | null }
  export function splitLessonBody(body: string): LessonBody
  ```
  Task 2의 `toLogCardView()`가 소비한다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`coach-helpers.test.ts` 상단 import에 `splitLessonBody`를 추가하고, 파일 끝에 아래를 붙인다.

```ts
describe('splitLessonBody', () => {
  it('원리·이렇게 마커로 세 조각을 나눈다', () => {
    const r = splitLessonBody(
      '당신 로그: 최근 세션에서 PowerShell 3회 오류가 났다가 회복했어요.\n' +
      '• 원리: 계획 없이 바로 손대면 헤매요.\n' +
      '• 이렇게: Shift+Tab으로 계획부터 세우세요.'
    );
    expect(r.evidence).toBe('당신 로그: 최근 세션에서 PowerShell 3회 오류가 났다가 회복했어요.');
    expect(r.principle).toBe('계획 없이 바로 손대면 헤매요.');
    expect(r.action).toBe('Shift+Tab으로 계획부터 세우세요.');
  });
  it('여러 줄에 걸친 조각을 이어 붙인다', () => {
    const r = splitLessonBody('앞줄\n• 원리: 첫 줄\n이어지는 줄\n• 이렇게: 행동');
    expect(r.principle).toBe('첫 줄 이어지는 줄');
    expect(r.action).toBe('행동');
  });
  it('👉 첫걸음도 행동 마커로 인정한다 (lesson-frontier)', () => {
    const r = splitLessonBody('당신 로그: 남은 다음 단계는 스킬 재사용이에요.\n👉 첫걸음: SKILL.md를 하나 만들어 보세요.');
    expect(r.evidence).toBe('당신 로그: 남은 다음 단계는 스킬 재사용이에요.');
    expect(r.principle).toBeNull();
    expect(r.action).toBe('SKILL.md를 하나 만들어 보세요.');
  });
  it('마커가 없으면 전부 근거로 — 본문을 잃지 않는다', () => {
    const body = '당신 로그: 최근 세션 5개가 UI 작업이었어요.\n설치된 플러그인이 쓰이지 않았어요.';
    const r = splitLessonBody(body);
    expect(r.evidence).toBe('당신 로그: 최근 세션 5개가 UI 작업이었어요. 설치된 플러그인이 쓰이지 않았어요.');
    expect(r.principle).toBeNull();
    expect(r.action).toBeNull();
  });
  it('빈 본문은 전부 null', () => {
    expect(splitLessonBody('')).toEqual({ evidence: null, principle: null, action: null });
    expect(splitLessonBody('   \n  ')).toEqual({ evidence: null, principle: null, action: null });
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts`
Expected: FAIL — `splitLessonBody is not a function`

- [ ] **Step 3: 최소 구현을 작성한다**

`coach-helpers.ts`의 `evidenceChip` 아래에 추가한다.

```ts
/** 개인 실전 레슨 본문을 문법 A의 슬롯으로 쪼갠다 (스펙 §4.1).
 * 마커(`• 원리:` / `• 이렇게:` / `👉 첫걸음:`)가 없는 레슨도 있으므로
 * 그때는 전부 근거로 넘긴다 — 본문을 잃지 않는 것이 우선이다. */
export interface LessonBody {
  evidence: string | null;
  principle: string | null;
  action: string | null;
}

const PRINCIPLE_MARK = '• 원리:';
const ACTION_MARKS = ['• 이렇게:', '👉 첫걸음:'];

export function splitLessonBody(body: string): LessonBody {
  const buckets: Record<keyof LessonBody, string[]> = { evidence: [], principle: [], action: [] };
  let current: keyof LessonBody = 'evidence';
  for (const raw of body.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    const actionMark = ACTION_MARKS.find((m) => line.startsWith(m));
    if (line.startsWith(PRINCIPLE_MARK)) {
      current = 'principle';
      buckets.principle.push(line.slice(PRINCIPLE_MARK.length).trim());
    } else if (actionMark) {
      current = 'action';
      buckets.action.push(line.slice(actionMark.length).trim());
    } else {
      buckets[current].push(line);
    }
  }
  const join = (parts: string[]): string | null => {
    const s = parts.filter(Boolean).join(' ').trim();
    return s === '' ? null : s;
  };
  return { evidence: join(buckets.evidence), principle: join(buckets.principle), action: join(buckets.action) };
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts`
Expected: PASS

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(frontend): parse lesson body into card grammar slots"
```

---

## Task 2: 섹션 분리 + 카드 뷰모델

`CoachFinding`과 `ContentItem`은 타입이 다르다. 문법 A 카드 하나가 둘 다 그리려면 공통 뷰모델로 정규화해야 한다. 이 로직을 `.ts`에 두는 것이 컴포넌트 테스트 없이 검증할 수 있는 유일한 길이다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts`
- Test: `a-mate/src/lib/ui/coach-helpers.test.ts`

**Interfaces:**
- Consumes: `splitLessonBody`(Task 1), `coachTitle`·`evidenceChip`(PR① 기존)
- Produces:
  ```ts
  export interface LogCardView {
    key: string;                       // finding.dedup_key | content.id
    source: 'finding' | 'lesson';
    icon: string;                      // ⚠ / 💡 / ℹ
    title: string;
    evidence: string | null;           // 📊 줄
    chip: string | null;               // 근거 칩 (finding 전용)
    reason: string | null;             // 🧭 코치 판정 (finding 전용)
    principle: string | null;          // 원리 (lesson 전용)
    action: string | null;             // ➜ 행동
    sourceUrl: string | null;
  }
  export interface LearnCardView {
    id: string;
    badge: string;                     // 소식 / Boris / 팀 / 축 이름 / 배움
    title: string;
    summary: string | null;
    sourceUrl: string | null;
  }
  export function toLogCardView(f: CoachFinding): LogCardView
  export function toLessonCardView(c: ContentItem): LogCardView
  export function toLearnCardView(c: ContentItem): LearnCardView
  export function partitionCoachItems(
    findings: CoachFinding[], content: ContentItem[],
  ): { log: LogCardView[]; learn: LearnCardView[] }
  ```
  Task 3의 컴포넌트와 Task 4의 `CoachTab`이 소비한다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`coach-helpers.test.ts` import에 `partitionCoachItems`·`toLogCardView`·`toLessonCardView`·`toLearnCardView`를 추가하고, 파일 끝에 붙인다. `CoachFinding`·`ContentItem` 타입 import도 `../api`에서 추가한다.

```ts
const finding = (over: Partial<CoachFinding> = {}): CoachFinding => ({
  rule_id: 'R6', severity: 'warn', scope_host: 'Windows', scope_project: null,
  scope_kind: 'pattern', scope_ref: 'pattern:ab', evidence: { session_count: 4 },
  est_tokens_saved: 0, prescription: null, dedup_key: 'R6|Windows|ab',
  last_seen: null, occurrences: 1, status: 'new',
  detail: '같은 지시를 4개 세션에서 반복했어요', suggested_action: '스킬로 묶으세요',
  fix_command: null, session: null, ...over,
});

const content = (over: Partial<ContentItem> = {}): ContentItem => ({
  id: 'lesson-struggle', kind: 'tip', dimension: null, title: '도구 오류 3회',
  body: '당신 로그: 오류가 났어요.\n• 원리: 계획 없이 손대면 헤매요.\n• 이렇게: 플랜 모드를 쓰세요.',
  source_url: 'https://example.test/guide', trigger_tags: ['personal', 'plan'],
  score: 550, status: 'new', personal: null, ...over,
});

describe('partitionCoachItems', () => {
  it('룰 finding과 personal 레슨은 로그 섹션, 나머지는 배움 섹션', () => {
    const { log, learn } = partitionCoachItems(
      [finding()],
      [content(), content({ id: 'T-L1-1', trigger_tags: [], dimension: 'context_hygiene', title: '내장 팁' })],
    );
    expect(log.map((v) => v.key)).toEqual(['R6|Windows|ab', 'lesson-struggle']);
    expect(learn.map((v) => v.id)).toEqual(['T-L1-1']);
  });
  it('finding이 레슨보다 앞에 온다', () => {
    const { log } = partitionCoachItems([finding()], [content()]);
    expect(log[0].source).toBe('finding');
    expect(log[1].source).toBe('lesson');
  });
  it('한쪽이 비어도 동작한다', () => {
    expect(partitionCoachItems([], []).log).toEqual([]);
    expect(partitionCoachItems([], []).learn).toEqual([]);
  });
});

describe('toLogCardView', () => {
  it('finding을 문법 A 슬롯으로 정규화한다', () => {
    const v = toLogCardView(finding({ judgment: { reason: '재사용 가치 높음' } }));
    expect(v.source).toBe('finding');
    expect(v.icon).toBe('⚠');
    expect(v.title).toContain('같은 지시');
    expect(v.evidence).toBe('같은 지시를 4개 세션에서 반복했어요');
    expect(v.chip).toBe('4개 세션');
    expect(v.reason).toBe('재사용 가치 높음');
    expect(v.principle).toBeNull();      // 원리는 레슨 전용
    expect(v.action).toBe('스킬로 묶으세요');
  });
  it('severity에 따라 아이콘이 갈린다', () => {
    expect(toLogCardView(finding({ severity: 'suggest' })).icon).toBe('💡');
    expect(toLogCardView(finding({ severity: 'info' })).icon).toBe('ℹ');
  });
  it('레슨은 본문을 쪼개 담고 칩·판정이 없다', () => {
    const v = toLessonCardView(content());
    expect(v.source).toBe('lesson');
    expect(v.evidence).toBe('당신 로그: 오류가 났어요.');
    expect(v.principle).toBe('계획 없이 손대면 헤매요.');
    expect(v.action).toBe('플랜 모드를 쓰세요.');
    expect(v.chip).toBeNull();
    expect(v.reason).toBeNull();
    expect(v.sourceUrl).toBe('https://example.test/guide');
  });
  it('레슨의 personal 필드가 있으면 본문 근거보다 우선한다', () => {
    const v = toLessonCardView(content({ personal: '당신 로그: 실측 한 줄' }));
    expect(v.evidence).toBe('당신 로그: 실측 한 줄');
  });
});

describe('toLearnCardView', () => {
  it('출처별 배지', () => {
    expect(toLearnCardView(content({ kind: 'news', trigger_tags: ['changelog'] })).badge).toBe('소식');
    expect(toLearnCardView(content({ trigger_tags: ['boris'] })).badge).toBe('Boris');
    expect(toLearnCardView(content({ trigger_tags: ['team'] })).badge).toBe('팀');
    expect(toLearnCardView(content({ trigger_tags: [], dimension: 'skill_reuse' })).badge).toBe('스킬로 반복 줄이기');
    expect(toLearnCardView(content({ trigger_tags: [], dimension: null })).badge).toBe('배움');
  });
  it('본문이 비면 summary는 null', () => {
    expect(toLearnCardView(content({ trigger_tags: [], body: '  ' })).summary).toBeNull();
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/ui/coach-helpers.test.ts`
Expected: FAIL — `partitionCoachItems is not a function` 외 다수

- [ ] **Step 3: 구현한다**

`coach-helpers.ts`에 추가한다. 파일 상단 import를 `import type { CoachFinding, ContentItem, SessionCtxItem } from '../api';` 로 넓힌다.

```ts
/** 역량 사다리 축 → 한국어 배지 (TipCard에서 이관) */
const DIM_LABEL: Record<string, string> = {
  model_literacy: '모델 고르기',
  context_hygiene: '컨텍스트 정리',
  skill_reuse: '스킬로 반복 줄이기',
  automation: '워크플로 자동화',
  orchestration: '작업 위임',
};

export interface LogCardView {
  key: string;
  source: 'finding' | 'lesson';
  icon: string;
  title: string;
  evidence: string | null;
  chip: string | null;
  reason: string | null;
  principle: string | null;
  action: string | null;
  sourceUrl: string | null;
}

export interface LearnCardView {
  id: string;
  badge: string;
  title: string;
  summary: string | null;
  sourceUrl: string | null;
}

const severityIcon = (s: CoachFinding['severity']): string =>
  s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ';

const orNull = (s: string | null | undefined): string | null => {
  const t = (s ?? '').trim();
  return t === '' ? null : t;
};

/** 룰 finding → 문법 A. 원리는 레슨 전용 슬롯이라 항상 null. */
export function toLogCardView(f: CoachFinding): LogCardView {
  return {
    key: f.dedup_key,
    source: 'finding',
    icon: severityIcon(f.severity),
    title: coachTitle(f.rule_id, f.evidence),
    evidence: orNull(f.detail),
    chip: evidenceChip(f.rule_id, f.evidence),
    reason: orNull(f.judgment?.reason),
    principle: null,
    action: orNull(f.suggested_action),
    sourceUrl: null,
  };
}

/** 개인 실전 레슨 → 문법 A. 칩·판정은 룰 전용이라 null.
 * `personal`(store의 enrich_personal)이 있으면 본문에서 뽑은 근거보다 우선한다 — 실측 수치라서. */
export function toLessonCardView(c: ContentItem): LogCardView {
  const parts = splitLessonBody(c.body);
  return {
    key: c.id,
    source: 'lesson',
    icon: '💡',
    title: c.title,
    evidence: orNull(c.personal) ?? parts.evidence,
    chip: null,
    reason: null,
    principle: parts.principle,
    action: parts.action,
    sourceUrl: orNull(c.source_url),
  };
}

/** 커리큘럼·외부 팁·소식 → 문법 B(카드뉴스). */
export function toLearnCardView(c: ContentItem): LearnCardView {
  const has = (t: string) => c.trigger_tags?.includes(t) ?? false;
  const badge = c.kind === 'news' || has('changelog')
    ? '소식'
    : has('boris')
      ? 'Boris'
      : has('team')
        ? '팀'
        : c.dimension
          ? (DIM_LABEL[c.dimension] ?? c.dimension)
          : '배움';
  return { id: c.id, badge, title: c.title, summary: orNull(c.body), sourceUrl: orNull(c.source_url) };
}

/** 근거 출처로 두 섹션을 가른다 (스펙 §3).
 * 「내 로그에서」 = 룰 finding + `personal` 태그 콘텐츠. finding을 앞에 둔다 —
 * 처방·행동 버튼이 붙어 바로 실행 가능한 쪽이기 때문. */
export function partitionCoachItems(
  findings: CoachFinding[],
  content: ContentItem[],
): { log: LogCardView[]; learn: LearnCardView[] } {
  const isPersonal = (c: ContentItem) => c.trigger_tags?.includes('personal') ?? false;
  return {
    log: [...findings.map(toLogCardView), ...content.filter(isPersonal).map(toLessonCardView)],
    learn: content.filter((c) => !isPersonal(c)).map(toLearnCardView),
  };
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npm test`
Expected: PASS — svelte-check 오류 0, vitest 전체 통과

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(frontend): normalize coaching items into two card view models"
```

---

## Task 3: 카드 컴포넌트 2종

뷰모델을 받아 그리기만 한다. 행동 버튼은 기존 핸들러(복사·스킬 초안·세션 상세·처분)가 `CoachTab`에 있으므로 **snippet으로 주입**받는다 — 카드는 순수 표현으로 남고 `CoachTab`의 로직을 옮기지 않는다.

**Files:**
- Create: `a-mate/src/lib/ui/coach/LogCard.svelte`
- Create: `a-mate/src/lib/ui/coach/LearnCard.svelte`

**Interfaces:**
- Consumes: `LogCardView`·`LearnCardView` (Task 2)
- Produces:
  - `LogCard` props: `{ view: LogCardView; showLinks: boolean; actions?: Snippet; extra?: Snippet }`
  - `LearnCard` props: `{ view: LearnCardView; showLinks: boolean; coaching?: string | null; onDismiss: (id: string) => void }`
  - Task 4의 `CoachTab`이 소비한다.

- [ ] **Step 1: `LogCard.svelte`를 만든다**

`a-mate/src/lib/ui/coach/LogCard.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { LogCardView } from '../coach-helpers';

  let { view, showLinks, actions, extra }: {
    view: LogCardView;
    showLinks: boolean;
    /** 행동·처분 버튼 — 핸들러가 CoachTab에 있어 주입받는다 */
    actions?: Snippet;
    /** 포함 세션 목록·원본 데이터 등 finding 전용 부가 영역 */
    extra?: Snippet;
  } = $props();
</script>

<article class="card" class:warn={view.icon === '⚠'} data-key={view.key}>
  <header>
    <span class="title">{view.icon} {view.title}</span>
    {#if view.chip}<span class="chip">{view.chip}</span>{/if}
  </header>
  {#if view.evidence}<p class="evidence">📊 {view.evidence}</p>{/if}
  {#if view.principle}<p class="principle">{view.principle}</p>{/if}
  {#if view.reason}<p class="judgment">🧭 코치 판정: {view.reason}</p>{/if}
  {@render extra?.()}
  {#if view.action}<p class="how">➜ {view.action}</p>{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
  {#if view.sourceUrl && showLinks}
    <a class="more" href={view.sourceUrl} target="_blank" rel="noreferrer">왜 그런지 더 보기 →</a>
  {/if}
</article>

<style>
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border-left: 4px solid var(--pastel-mint);
  }
  .card.warn { border-left-color: var(--pastel-coral); }
  header { display: flex; justify-content: space-between; gap: 8px; align-items: baseline; }
  .title { font-weight: 600; }
  .chip {
    color: var(--ink-soft); font-size: 11px; white-space: nowrap; flex: none;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px; padding: 1px 7px;
  }
  .evidence { margin: 6px 0 2px; font-size: 12px; color: var(--ink); line-height: 1.6; white-space: pre-line; }
  .principle { margin: 2px 0; font-size: 12px; color: var(--ink-soft); line-height: 1.7; white-space: pre-line; }
  .judgment { margin: 2px 0 6px; font-size: 12px; color: var(--accent-strong); }
  .how { margin: 4px 0 8px; font-size: 13px; white-space: pre-line; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  /* 각주 크기 — 주 CTA는 행동 버튼이고 링크는 보조다 (스펙 §4.1) */
  .more { display: block; margin-top: 8px; font-size: 11px; color: var(--ink-soft); text-decoration: none; }
  .more:hover { color: var(--accent-strong); text-decoration: underline; }
</style>
```

> **링크 열기 방식**: 기존 `TipCard`는 `@tauri-apps/plugin-opener`의 `openUrl`을 썼다(Tauri 웹뷰가 외부 `<a>` 네비게이션을 막기 때문). `LearnCard`에서 그 방식을 그대로 유지하고, `LogCard`도 같은 헬퍼를 쓰도록 Step 2에서 맞춘다.

- [ ] **Step 2: `LearnCard.svelte`를 만들고 두 카드의 링크 열기를 통일한다**

`a-mate/src/lib/ui/coach/LearnCard.svelte`:

```svelte
<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { LearnCardView } from '../coach-helpers';

  let { view, showLinks, coaching = null, onDismiss }: {
    view: LearnCardView;
    showLinks: boolean;
    /** 엔진 맞춤 코칭 한 줄 (최상단 1건에만) */
    coaching?: string | null;
    onDismiss: (id: string) => void;
  } = $props();

  // Tauri 웹뷰는 외부 <a> 네비게이션을 막는다 — 시스템 브라우저로 연다
  async function open(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }
</script>

<article class="card">
  <header>
    <span class="badge">{view.badge}</span>
    <button class="x" title="이 항목 그만 보기" onclick={() => onDismiss(view.id)}>✕</button>
  </header>
  <h3>{view.title}</h3>
  {#if coaching}<p class="coach">🤖 {coaching}</p>{/if}
  {#if view.summary}<p class="summary">{view.summary}</p>{/if}
  {#if view.sourceUrl && showLinks}
    <a class="more" href={view.sourceUrl} onclick={(e) => { e.preventDefault(); open(view.sourceUrl!); }}>
      전문 보기 →
    </a>
  {/if}
</article>

<style>
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border: 1px solid var(--line); border-left: 3px solid var(--accent);
    display: flex; flex-direction: column; gap: 6px;
  }
  header { display: flex; align-items: center; gap: 8px; }
  .badge {
    font-size: 10px; font-weight: 700; color: var(--accent-ink); white-space: nowrap;
    background: var(--pastel-mint); border-radius: 999px; padding: 3px 10px;
  }
  .x {
    margin-left: auto; border: none; background: none; cursor: pointer;
    color: var(--ink-soft); font-size: 12px; padding: 2px 4px; line-height: 1;
  }
  .x:hover { color: var(--accent-strong); }
  h3 { margin: 0; font-size: 14px; color: var(--ink); font-weight: 700; line-height: 1.4; }
  .coach { margin: 0; font-size: 12.5px; color: var(--lav); line-height: 1.6; }
  .summary { margin: 0; font-size: 12px; color: var(--ink-soft); line-height: 1.7; white-space: pre-line; }
  /* 문법 B는 전문 링크가 주 CTA — 문법 A의 각주 링크와 반대다 (스펙 §4.2) */
  .more { font-size: 12px; color: var(--accent-strong); text-decoration: none; font-weight: 600; width: fit-content; }
  .more:hover { text-decoration: underline; }
</style>
```

`LogCard.svelte`의 링크도 같은 방식으로 바꾼다. `<script>`에 import와 헬퍼를 추가하고:

```svelte
  import { openUrl } from '@tauri-apps/plugin-opener';
  async function open(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }
```

마크업의 링크를 교체:
```svelte
    <a class="more" href={view.sourceUrl} onclick={(e) => { e.preventDefault(); open(view.sourceUrl!); }}>왜 그런지 더 보기 →</a>
```

- [ ] **Step 3: 타입 검사를 통과하는지 확인한다**

Run: `cd a-mate; npm run check`
Expected: 오류 0 (아직 아무도 이 컴포넌트를 쓰지 않으므로 미사용 경고만 없으면 된다)

- [ ] **Step 4: 커밋**

```bash
git add a-mate/src/lib/ui/coach/LogCard.svelte a-mate/src/lib/ui/coach/LearnCard.svelte
git commit -m "feat(frontend): add log and learn card components"
```

---

## Task 4: `CoachTab` 2단 재구성 + `TipCard` 삭제

**Files:**
- Modify: `a-mate/src/lib/ui/CoachTab.svelte`
- Delete: `a-mate/src/lib/ui/home/TipCard.svelte`
- Test: `a-mate/src/lib/no-tipcard.test.ts` (신규 — 삭제 회귀 방지)

**Interfaces:**
- Consumes: `partitionCoachItems`(Task 2), `LogCard`·`LearnCard`(Task 3)
- Produces: 없음 (최종 조립)

- [ ] **Step 1: 실패하는 회귀 테스트를 작성한다**

`a-mate/src/lib/no-tipcard.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// 「오늘의 배움」 컨테이너는 항목마다 독립 카드로 해체됐다 (스펙 §4.1.1).
// 컨테이너가 되살아나면 두 섹션의 카드 문법이 다시 어긋난다.

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (/\.(svelte|ts)$/.test(name)) out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
  }
}

describe('TipCard 컨테이너 해체', () => {
  const base = process.cwd();

  it('TipCard.svelte가 존재하지 않는다', () => {
    expect(existsSync(join(base, 'src/lib/ui/home/TipCard.svelte'))).toBe(false);
  });

  it('TipCard를 import하는 곳이 없다', () => {
    const files: string[] = [];
    walk(join(base, 'src'), base, files);
    const offenders = files.filter((rel) => /TipCard/.test(readFileSync(join(base, rel), 'utf8')));
    expect(offenders, `TipCard 참조 발견: ${offenders.join(', ')}`).toEqual([]);
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cd a-mate; npx vitest run src/lib/no-tipcard.test.ts`
Expected: FAIL — 두 케이스 모두 실패 (파일이 있고 `CoachTab.svelte`가 import 중)

- [ ] **Step 3: `CoachTab.svelte`를 재구성한다**

`<script>`에서 바꿀 것:

```ts
  // 제거
  import TipCard from './home/TipCard.svelte';
  const icon = (s: CoachFinding['severity']) => (s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ');

  // 추가
  import LogCard from './coach/LogCard.svelte';
  import LearnCard from './coach/LearnCard.svelte';
  import { partitionCoachItems } from './coach-helpers';
  import { getSettings, coachTip, setContentStatus } from '../api';

  // 내부망이면 외부 링크를 숨긴다 (TipCard에서 이관 — 두 카드가 각각 부르지 않도록 여기서 한 번만)
  let showLinks = $state(true);
  getSettings().then((s) => (showLinks = s.docs_reachable !== 'false')).catch(() => {});

  const sections = $derived(partitionCoachItems(active, tips));
  const findingByKey = $derived(new Map(active.map((f) => [f.dedup_key, f])));

  // 엔진 맞춤 코칭 — 배움 섹션 최상단 1건에만 (TipCard의 기존 동작 보존).
  // personal 항목은 이미 로그 섹션으로 갈라져 여기 오지 않는다.
  let coaching = $state<string | null>(null);
  let coachLoading = $state(false);
  $effect(() => {
    const top = tips.find((t) => !(t.trigger_tags?.includes('personal') ?? false));
    coaching = null;
    if (!top) return;
    coachLoading = true;
    coachTip(top)
      .then((s) => { coaching = s?.trim() || null; })
      .catch(() => { coaching = null; })
      .finally(() => { coachLoading = false; });
  });

  async function dismissLearn(id: string) {
    try { await setContentStatus(id, 'dismissed'); } catch { /* 무시 */ }
    tips = tips.filter((t) => t.id !== id);
  }
```

기존 `onTipDismissed`는 `dismissLearn`이 대체하므로 삭제한다.

마크업 `<section class="coach">`의 앞부분을 교체한다. 기존 `<TipCard …/>` 한 줄과 `{#if active.length === 0}…{#each active}…{/each}{/if}` 블록 전체를 아래로 바꾼다.

```svelte
<section class="coach">
  {#if sections.log.length === 0 && sections.learn.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {/if}

  {#if sections.log.length > 0}
    <h2 class="section">내 로그에서 <span class="count">{sections.log.length}</span></h2>
    {#each sections.log as v (v.key)}
      {@const f = findingByKey.get(v.key)}
      <LogCard view={v} {showLinks}>
        {#snippet extra()}
          {#if f && f.scope_kind === 'session' && sessionLine(f)}
            <p class="session">📂 {sessionLine(f)}</p>
          {:else if f && f.scope_kind === 'project' && f.scope_project}
            <p class="session">📂 {f.scope_project}</p>
          {/if}
          {#if f && f.scope_kind === 'project' && sessionIdsOf(f.evidence).length > 0}
            <button class="raw-toggle" onclick={() => toggleExpand(f)}>
              {expanded === f.dedup_key ? '▾' : '▸'} 포함 세션 {totalSessionsOf(f.evidence, sessionIdsOf(f.evidence).length)}건
            </button>
            {#if expanded === f.dedup_key}
              <ul class="session-list">
                {#each ctxCache[f.dedup_key] ?? [] as s (s.session_id)}
                  <li><button onclick={() => (detail = { id: s.session_id, title: ctxLine(s) })}>{ctxLine(s)}</button></li>
                {/each}
                {#if totalSessionsOf(f.evidence, 0) > sessionIdsOf(f.evidence).length}
                  <li class="more">최신 {sessionIdsOf(f.evidence).length}건 표시 중 (전체 {totalSessionsOf(f.evidence, 0)}건)</li>
                {/if}
              </ul>
            {/if}
          {/if}
        {/snippet}
        {#snippet actions()}
          {#if f}
            {#if f.fix_command}
              <button class="cmd" onclick={() => copy(f)}>{copied === f.dedup_key ? '복사됨!' : `📋 ${f.fix_command}`}</button>
            {/if}
            {#if skillifiable(f)}
              <button class="skillify" onclick={() => makeDraft(f)}>🧩 스킬 초안 만들기</button>
            {/if}
            {#if f.scope_kind === 'session'}
              <button onclick={() => openDetail(f)}>세션 상세</button>
            {/if}
            <button onclick={() => mark(f, 'resolved')}>해결함</button>
            <button onclick={() => mark(f, 'dismissed')}>무시</button>
          {:else}
            <!-- 개인 레슨: 처분 어휘 확장(resolved)은 PR③ — 지금은 기존 ✕만 -->
            <button onclick={() => dismissLearn(v.key)}>✕ 그만 보기</button>
          {/if}
        {/snippet}
      </LogCard>
    {/each}
  {/if}

  {#if sections.learn.length > 0}
    <h2 class="section">배움 · 소식</h2>
    {#each sections.learn as v, i (v.id)}
      <LearnCard
        view={v}
        {showLinks}
        coaching={i === 0 ? (coachLoading ? '맞춤 코칭 생각 중…' : coaching) : null}
        onDismiss={dismissLearn}
      />
    {/each}
  {/if}
```

`{#if hidden.length > 0}` 이하(숨긴 항목·모달·초안 모달)는 **그대로 둔다.** 하단 접힌 줄로 옮기는 것은 PR③이다.

`<style>`에서 `LogCard`로 옮겨간 규칙(`.card`, `.card.warn`, `header`, `.title`, `.chip`, `.why`, `.judgment`, `.how`, `.actions` 중 카드 내부 배치)은 삭제하고, `CoachTab`에 남아야 할 것만 유지한다 — `.coach`, `.empty`, `.card.muted`(숨긴 항목), `.session`, `.raw-toggle`, `.session-list`, `.hidden-toggle`, 초안 모달 일체, 그리고 snippet이 렌더하는 버튼 스타일(`.actions button` 계열은 `LogCard`의 `.actions` 안에 들어가므로 **`:global()`로 감싸거나** `LogCard`로 옮겨야 한다 — Svelte는 미사용 셀렉터를 제거한다).

> **주의**: snippet 내용은 `CoachTab`의 스타일 스코프에 속하지만 `LogCard`의 DOM 안에 렌더된다. `.actions button` 같은 자손 셀렉터는 Svelte가 "미사용"으로 판단해 제거할 수 있다. 버튼 스타일은 `LogCard.svelte`로 옮기고 `:global(.actions button)` 대신 `.actions :global(button)`을 쓰는 편이 안전하다. `npm run check` 경고와 실제 렌더를 함께 확인할 것.

새 섹션 헤더 스타일을 추가한다:
```css
  .section {
    margin: 6px 0 0; font-size: 12px; font-weight: 700; color: var(--ink-soft);
    display: flex; align-items: baseline; gap: 6px;
  }
  .section .count { font-size: 11px; color: var(--accent-strong); }
```

- [ ] **Step 4: `TipCard.svelte`를 삭제한다**

```bash
git rm a-mate/src/lib/ui/home/TipCard.svelte
```

- [ ] **Step 5: 테스트가 통과하는지 확인한다**

Run: `cd a-mate; npm test`
Expected: PASS — svelte-check 오류 0, vitest 전체 통과 (`no-tipcard` 2건 포함)

svelte-check가 미사용 CSS 셀렉터를 경고하면 그 규칙을 지운다(`--threshold error`라 경고는 실패시키지 않지만, 우리 변경이 만든 고아이므로 정리한다).

- [ ] **Step 6: 실제 화면을 확인한다**

Run: `cd a-mate; npm run tauri dev`

눈으로 확인할 것:
- 「내 로그에서」에 룰 카드 + 개인 레슨이 **각각 독립 카드**로 나열된다
- 레슨 카드에 `📊 근거` / `원리` / `➜ 행동`이 모두 보인다 (본문이 잘리지 않았는지)
- 「배움 · 소식」 최상단 카드에만 `🤖` 코칭 줄이 뜬다
- 링크가 시스템 브라우저로 열린다
- 「숨긴 항목 N개 보기」가 그대로 동작한다

- [ ] **Step 7: 커밋**

```bash
git add a-mate/src/lib/ui/CoachTab.svelte a-mate/src/lib/no-tipcard.test.ts
git commit -m "refactor(frontend): split the coaching tab into two evidence sections"
```

---

## 완료 조건

- [ ] `cd a-mate; npm test` — svelte-check 오류 0, vitest 전체 통과
- [ ] `cd a-mate; cargo test` — 652건 통과 (이 PR은 Rust 무변경, 회귀 확인용)
- [ ] 코칭 탭이 「내 로그에서」/「배움 · 소식」 두 섹션으로 갈린다
- [ ] 「오늘의 배움」 컨테이너가 사라지고 항목마다 독립 카드가 된다
- [ ] 레슨 본문(`• 원리` / `• 이렇게`)이 통일된 슬롯에 담기고 **내용이 잘리지 않는다**
- [ ] `TipCard.svelte`가 삭제되고 참조가 0건
- [ ] `~0 tok`이 여전히 어디에도 없다 (`no-savings-display.test.ts` 통과)

## 이 PR에서 하지 않는 것

- **개인 레슨의 `[해결함]` 처분** — `content_items`에 `resolved` 어휘가 필요하다. PR③
- **처분 카드를 섹션 하단 톤다운 줄로 이동** — 현행 「숨긴 항목 N개 보기」 토글 유지. PR③
- **재발 감지·7일 창·마이그레이션** — PR③
- **로컬 공지 소식·기한 고정 슬롯·번역 캐시** — PR⑥
- **`coachTip` 호출 캐시화** — 현행 매번 호출 유지. PR⑥ (§6.3)
- **피드 TTL·프룬 범위** — 병렬 진행 중인 PR⑤
