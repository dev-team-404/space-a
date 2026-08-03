---
status: done
archived: 2026-08-03
---

# 코칭 탭 단일 스트림 (B) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 코칭 탭의 2단 섹션(「내 로그에서」/「배움 · 소식」)을 `first_seen` 내림차순 한 줄 스트림으로 바꾸고, 분류는 좌측 라인 색·배지·보조 칩으로만 나타낸다. 아울러 공지에 수명을 줘 **해소된 장애 안내가 최상단에 눌러앉지 않게** 한다(§2.6).

**Architecture:** 판정·조립 로직은 전부 순수 함수에 둔다 — 항목→뷰모델 변환은 기존 `coach-helpers.ts`에, 뷰모델들→스트림·배지 조립은 신규 `coach-stream.ts`에. Svelte 컴포넌트는 렌더만 한다(이 저장소엔 컴포넌트 테스트 라이브러리가 없어 렌더 검증이 불가능하므로, 검증 가능한 것을 전부 순수 함수로 밀어내는 것이 유일한 안전망이다). Rust는 건드리지 않는다.

**Tech Stack:** Svelte 5 (runes) · TypeScript · Vitest · Tauri v2

**근거 스펙:** [`docs/archive/design/a-mate/specs/2026-08-02-coaching-tab-single-stream-design.md`](../specs/2026-08-02-coaching-tab-single-stream-design.md) — §2가 확정된 결정, §3이 실측한 코드베이스 사실, §4가 다섯 질문의 확정된 답이다. 절 번호는 그 문서를 가리킨다.

## Global Constraints

- **빌드·테스트는 네이티브 Windows PowerShell에서.** WSL 안에서 실행 금지 (`a-mate/CLAUDE.md`).
- 작업 디렉터리는 `a-mate/`. 테스트는 `npm test` (= `svelte-check --threshold error && vitest run`).
- **베이스라인(2026-08-03, `18322ab`): 프론트 29파일 / 296 테스트 통과, cargo 713 통과.** 태스크마다 이 수가 줄지 않아야 한다.
- `npm test`는 `node_modules`가 없으면 `'svelte-check'은(는) 내부 또는 외부 명령이 아닙니다`를 출력하고 **종료 코드 0으로 거짓 통과**한다. 출력에 `Tests  N passed` 줄이 있는지 눈으로 확인할 것.
- 커밋: Conventional Commits, **영어**. `<type>(<scope>): <subject>` — scope는 `agent`.
- **뮤테이션 규율**: 새로 쓴 가드가 구현 전에 통과하면, 조건을 뒤집어 실패하는지 보고 되돌린다. 미검출이 나오면 "가드가 중복이네"로 넘기지 말고 그 테스트가 무엇을 주장하는지 다시 볼 것.
- **테스트 환경에 DOM이 없다.** `vite.config.ts`에 `test.environment` 설정이 없고 `jsdom`·`happy-dom`도 의존성에 없어 vitest는 **node 환경**으로 돈다 → `localStorage`가 **정의되지 않는다.** 기존 `loadPinAcks`·`savePinAcks`에 테스트가 없는 이유가 이것이다. localStorage를 만지는 함수에는 테스트를 쓰지 말고, **판정·계산을 그 함수 밖 순수 함수로 밀어내 거기에 테스트를 붙인다.**
- **컴포넌트 테스트 라이브러리도 없다** (`@testing-library/svelte` 없음). 렌더 검증은 불가능하고 `svelte-check`의 타입·미사용 CSS 검사가 유일한 자동 안전망이다.
- **`enrich_personal`은 태그로 붙어 레슨 본문과 어긋날 수 있다**(§3.8). `toLessonCardView`가 `personal`과 본문 근거를 **둘 다 싣는** 결정을 되돌리지 말 것.
- Rust(`crates/`, `src-tauri/`)는 이 PR의 범위가 아니다. A(#161)가 이미 `first_seen`을 payload에 실었다(§3.3).

---

## File Structure

| 파일 | 책임 | 상태 |
|------|------|------|
| `a-mate/src/lib/api.ts` | Tauri 커맨드 바인딩·payload 타입 | 수정 — `first_seen` 선언 |
| `a-mate/src/lib/ui/coach-helpers.ts` | **항목 → 카드 뷰모델** 변환, 처분 판정 | 수정 — 분류·보조 칩·`firstSeen` |
| `a-mate/src/lib/ui/coach-stream.ts` | **뷰모델들 → 한 줄 스트림·배지 키** 조립, 공지 수명 판정 | **신규** |
| `a-mate/src/lib/ui/coach-stream.test.ts` | 위의 단위 테스트 | **신규** |
| `a-mate/src/lib/ui/CoachTab.svelte` | 코칭 탭 렌더 | 수정 — 섹션 제거, 스트림 |
| `a-mate/src/lib/ui/coach/LogCard.svelte` | 문법 A 카드 | 수정 — 분류 배지, `.warn` 제거 |
| `a-mate/src/lib/ui/coach/LearnCard.svelte` | 문법 B 카드 | 수정 — 분류 배지 + 보조 칩, 라인 색 |
| `a-mate/src/App.svelte` | 셸·탭 배지 | 수정 — 안 본 개수 |
| `a-mate/src/lib/ui/HomeTab.svelte` | 홈 탭 | 수정 — 콘텐츠도 조회 |
| `a-mate/src/lib/ui/home/SaveTop3.svelte` | 「지금 볼 코칭」 위젯 | 수정 — 세 분류 |

**왜 `coach-stream.ts`를 새로 만드나**: `coach-helpers.ts`는 이미 385줄이고 책임이 "한 항목을 어떻게 그릴까"다. 스트림 조립은 "여러 항목을 어떤 순서로 몇 개나"라 축이 다르고, 배지 키 계산은 `App.svelte`도 쓴다(`CoachTab`이 아니라). 같은 파일에 넣으면 두 소비자가 한 덩어리를 공유하게 된다.

---

### Task 1: `first_seen`을 프론트 타입과 뷰모델에 싣는다

정렬·상한의 유일한 축이다. A(#161)가 payload에 실었지만 `api.ts` 타입 선언이 없어 TypeScript가 존재를 모른다(§3.3).

**Files:**
- Modify: `a-mate/src/lib/api.ts` — `Finding`·`ContentItem` 인터페이스
- Modify: `a-mate/src/lib/ui/coach-helpers.ts` — `LogCardView`·`LearnCardView` + 변환 함수 3개
- Test: `a-mate/src/lib/ui/coach-helpers.test.ts`

**Interfaces:**
- Consumes: 없음 (첫 태스크)
- Produces: `Finding.first_seen?: string | null`, `ContentItem.first_seen?: string | null`, `LogCardView.firstSeen: string | null`, `LearnCardView.firstSeen: string | null`

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/ui/coach-helpers.test.ts`의 `content` 팩토리 정의(현재 145~150줄) **바로 아래**에 추가:

```ts
describe('뷰모델의 firstSeen', () => {
  it('세 변환 모두 first_seen을 그대로 싣는다 — 정렬·상한의 유일한 축', () => {
    expect(toLogCardView(finding({ first_seen: '2026-08-01T00:00:00Z' })).firstSeen)
      .toBe('2026-08-01T00:00:00Z');
    expect(toLessonCardView(content({ first_seen: '2026-08-02T00:00:00Z' })).firstSeen)
      .toBe('2026-08-02T00:00:00Z');
    expect(toLearnCardView(content({ trigger_tags: [], first_seen: '2026-08-03T00:00:00Z' })).firstSeen)
      .toBe('2026-08-03T00:00:00Z');
  });

  it('값이 없으면 null — 없는 시각을 지어내지 않는다', () => {
    expect(toLogCardView(finding()).firstSeen).toBeNull();
    expect(toLessonCardView(content()).firstSeen).toBeNull();
    expect(toLearnCardView(content({ trigger_tags: [] })).firstSeen).toBeNull();
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-helpers
```

Expected: FAIL — `Property 'first_seen' does not exist on type 'Partial<CoachFinding>'` (svelte-check 단계) 또는 `expected undefined to be '2026-08-01T00:00:00Z'`

- [ ] **Step 3: `api.ts`에 타입을 선언한다**

`Finding` 인터페이스에서 `last_seen: string | null;` **바로 위**에 추가:

```ts
  /** 처음 관측한 시각 — ON CONFLICT가 갱신하지 않아 보존된다(스펙 §3.1). 단일 스트림 정렬축. */
  first_seen?: string | null;
```

`ContentItem` 인터페이스에서 `status_ts?: string | null;` **바로 아래**에 추가:

```ts
  /** 처음 노출된 시각. 프룬으로 행이 지워졌다 돌아와도 `content_first_seen`이 지킨다(§3.2). */
  first_seen?: string | null;
```

- [ ] **Step 4: 뷰모델에 필드를 더한다**

`coach-helpers.ts`의 `LogCardView` 인터페이스 마지막 필드(`sourceUrl: string | null;`) 아래에 추가:

```ts
  /** 스트림 정렬축 (§2.2). 값이 없으면 null — 정렬에서 맨 뒤로 간다. */
  firstSeen: string | null;
```

`LearnCardView` 인터페이스 마지막 필드(`deadline: string | null;`) 아래에도 같은 줄을 추가한다(주석 포함).

세 변환 함수의 반환 객체에 필드를 더한다:

- `toLogCardView` — `sourceUrl: null,` 뒤에 `firstSeen: f.first_seen ?? null,`
- `toLessonCardView` — `sourceUrl: orNull(c.source_url),` 뒤에 `firstSeen: c.first_seen ?? null,`
- `toLearnCardView` — `deadline: orNull(c.deadline),` 뒤에 `firstSeen: c.first_seen ?? null,`

- [ ] **Step 5: 테스트 통과 확인**

```powershell
npm test
```

Expected: `Tests  302 passed (302)` — 기존 296 + 신규 2개 테스트(assert 6). 실제 수는 다를 수 있으나 **296보다 줄면 안 된다**.

- [ ] **Step 6: 커밋**

```powershell
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(agent): expose first_seen on coach card view models"
```

---

### Task 2: 분류(`CoachKind`)와 보조 칩을 배지에서 갈라낸다

현행 `toLearnCardView`의 배지 한 칸이 7종(공지/소식/Boris/팀/축 라벨 5종/배움)을 다 짊어지고 있다. 분류 배지 3종 + 보조 칩으로 가른다(§2.1·§4.1).

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts`
- Test: `a-mate/src/lib/ui/coach-helpers.test.ts`

**Interfaces:**
- Consumes: Task 1의 `LogCardView.firstSeen`·`LearnCardView.firstSeen`
- Produces:
  - `type CoachKind = 'coaching' | 'learning' | 'news'`
  - `KIND_LABEL: Record<CoachKind, string>` — `{ coaching: '코칭', learning: '학습', news: '소식' }`
  - `contentKind(c: ContentItem): CoachKind`
  - `sourceChip(c: ContentItem): string | null`
  - `LogCardView.badge: string` (언제나 `'코칭'`), `LearnCardView.kind: CoachKind`, `LearnCardView.badge: string`, `LearnCardView.chip: string | null`
  - **`LearnCardView`에서 옛 7종 `badge`는 사라진다** — 같은 이름이지만 값의 뜻이 바뀐다.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`coach-helpers.test.ts`의 기존 `describe('toLearnCardView', …)`가 있으면 그 **위**에, 없으면 파일 끝에 추가:

```ts
describe('contentKind — 세 분류 유도 (§2.1)', () => {
  it('personal 태그 레슨은 코칭 — 근거가 내 로그다', () => {
    expect(contentKind(content({ trigger_tags: ['personal', 'plan'] }))).toBe('coaching');
  });

  it('notice·changelog·kind=news는 소식', () => {
    expect(contentKind(content({ trigger_tags: ['notice'] }))).toBe('news');
    expect(contentKind(content({ trigger_tags: ['changelog'] }))).toBe('news');
    expect(contentKind(content({ trigger_tags: [], kind: 'news' }))).toBe('news');
  });

  it('커리큘럼·Boris·팀·그 밖은 학습', () => {
    expect(contentKind(content({ trigger_tags: [], dimension: 'automation' }))).toBe('learning');
    expect(contentKind(content({ trigger_tags: ['boris'] }))).toBe('learning');
    expect(contentKind(content({ trigger_tags: ['team'] }))).toBe('learning');
    expect(contentKind(content({ trigger_tags: [] }))).toBe('learning');
  });

  it('personal이 news보다 세다 — 내 로그 근거가 출처보다 우선한다', () => {
    expect(contentKind(content({ trigger_tags: ['personal', 'changelog'] }))).toBe('coaching');
  });
});

describe('sourceChip — 배지에서 갈라낸 세부 출처 (§4.1)', () => {
  it('축 라벨을 살린다 — 본문 어디에도 없는 유일한 표시', () => {
    expect(sourceChip(content({ trigger_tags: [], dimension: 'automation' }))).toBe('워크플로 자동화');
    expect(sourceChip(content({ trigger_tags: [], dimension: 'context_hygiene' }))).toBe('컨텍스트 정리');
  });

  it('모르는 축은 원래 값을 그대로 — 매핑이 비어도 정보를 잃지 않는다', () => {
    expect(sourceChip(content({ trigger_tags: [], dimension: 'unknown_axis' }))).toBe('unknown_axis');
  });

  it('공지·Boris·팀은 칩으로 남는다', () => {
    expect(sourceChip(content({ trigger_tags: ['notice'] }))).toBe('공지');
    expect(sourceChip(content({ trigger_tags: ['boris'] }))).toBe('Boris');
    expect(sourceChip(content({ trigger_tags: ['team'] }))).toBe('팀');
  });

  it('일반 소식과 출처 없는 배움은 칩이 없다 — 폴백 「배움」을 버렸다', () => {
    expect(sourceChip(content({ trigger_tags: ['changelog'] }))).toBeNull();
    expect(sourceChip(content({ trigger_tags: [], kind: 'news' }))).toBeNull();
    expect(sourceChip(content({ trigger_tags: [] }))).toBeNull();
  });
});

describe('카드 뷰모델의 배지·칩', () => {
  it('문법 B는 분류 이름을 배지로, 세부 출처를 칩으로 싣는다', () => {
    const v = toLearnCardView(content({ trigger_tags: [], dimension: 'automation' }));
    expect(v.kind).toBe('learning');
    expect(v.badge).toBe('학습');
    expect(v.chip).toBe('워크플로 자동화');
  });

  it('긴급 공지는 소식 배지 + 공지 칩', () => {
    const v = toLearnCardView(content({ trigger_tags: ['notice'] }));
    expect(v.badge).toBe('소식');
    expect(v.chip).toBe('공지');
  });

  it('문법 A는 언제나 코칭 배지 — finding도 레슨도', () => {
    expect(toLogCardView(finding()).badge).toBe('코칭');
    expect(toLessonCardView(content()).badge).toBe('코칭');
  });
});
```

테스트 파일 상단 import 목록에 `contentKind`, `sourceChip`을 더한다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-helpers
```

Expected: FAIL — `contentKind is not a function` / `sourceChip is not a function`

- [ ] **Step 3: 분류·칩 함수를 구현한다**

`coach-helpers.ts`의 `DIM_LABEL` 상수 **바로 아래**에 추가:

```ts
/** 카드 분류 — 좌측 라인 색과 배지가 이것을 나타낸다 (§2.1). */
export type CoachKind = 'coaching' | 'learning' | 'news';

export const KIND_LABEL: Record<CoachKind, string> = {
  coaching: '코칭',
  learning: '학습',
  news: '소식',
};

/** 콘텐츠 항목의 분류. 룰 finding은 언제나 'coaching'이라 이 함수를 타지 않는다.
 * `personal`을 먼저 보는 것이 중요하다 — 근거가 내 로그면 출처가 무엇이든 코칭이다. */
export function contentKind(c: ContentItem): CoachKind {
  const has = (t: string) => c.trigger_tags?.includes(t) ?? false;
  if (has('personal')) return 'coaching';
  if (has('notice') || c.kind === 'news' || has('changelog')) return 'news';
  return 'learning';
}

/** 분류 배지 옆 보조 칩 — 세부 출처. 없으면 null이고 그때는 렌더하지 않는다 (§4.1).
 * 옛 폴백 `'배움'`은 분류 배지와 겹쳐 버렸다. 판정 순서는 옛 배지 로직 그대로다. */
export function sourceChip(c: ContentItem): string | null {
  const has = (t: string) => c.trigger_tags?.includes(t) ?? false;
  if (has('notice')) return '공지';
  if (c.kind === 'news' || has('changelog')) return null;
  if (has('boris')) return 'Boris';
  if (has('team')) return '팀';
  if (c.dimension) return DIM_LABEL[c.dimension] ?? c.dimension;
  return null;
}
```

- [ ] **Step 4: 뷰모델을 고친다**

`LogCardView` 인터페이스에서 `source: 'finding' | 'lesson';` 아래에 추가:

```ts
  /** 분류 배지 — 문법 A는 언제나 '코칭'이다 (§2.1). */
  badge: string;
```

`LearnCardView` 인터페이스에서 `badge: string;` 줄을 다음 셋으로 **교체**한다:

```ts
  kind: CoachKind;
  /** 분류 배지 — '학습' 또는 '소식'. 옛 7종 배지를 대체한다 (§4.1). */
  badge: string;
  /** 보조 칩 — 공지·Boris·팀·축 라벨. 없으면 렌더하지 않는다. */
  chip: string | null;
```

`toLogCardView`의 반환 객체에서 `source: 'finding',` 아래에 `badge: KIND_LABEL.coaching,` 추가.
`toLessonCardView`의 반환 객체에서 `source: 'lesson',` 아래에 같은 줄 추가.

`toLearnCardView`에서 기존 `const has = …`부터 `const badge = … : '배움';`까지의 블록 전체를 **삭제**하고, 반환 객체의 `badge,`를 다음 셋으로 교체한다:

```ts
    kind,
    badge: KIND_LABEL[kind],
    chip: sourceChip(c),
```

함수 본문 첫 줄에 `const kind = contentKind(c);`를 둔다. 최종 형태:

```ts
export function toLearnCardView(c: ContentItem): LearnCardView {
  const kind = contentKind(c);
  return {
    id: c.id,
    kind,
    badge: KIND_LABEL[kind],
    chip: sourceChip(c),
    title: orNull(c.title_ko) ?? c.title,
    // 📊 근거 — 커리큘럼은 지도, 내 로그는 GPS. 문법 A와 같은 슬롯을 써서 두 카드가
    // "근거 → 내용" 구조로 수렴한다. 재료는 store가 이미 채워 보내는 결정론 수치라
    // LLM이 필요 없고 매 조회마다 최신이다 — 캐시가 굳지 않는다.
    // 재료가 없으면 줄을 생략한다(근거 칩과 같은 규칙: 0·추정치를 지어내지 않는다).
    evidence: orNull(c.personal),
    summary: orNull(c.summary_ko) ?? orNull(c.body),
    sourceUrl: orNull(c.source_url),
    deadline: orNull(c.deadline),
    firstSeen: c.first_seen ?? null,
  };
}
```

- [ ] **Step 5: 옛 배지를 검사하던 기존 테스트를 고친다**

`describe('toLearnCardView', …)` 안의 두 케이스가 옛 7종 배지를 검사한다. **삭제하지 말고** 새 규약으로 고쳐 남긴다 — 그 케이스들이 지키던 성질(출처가 화면에 남는다)은 여전히 유효하다.

`it('출처별 배지', …)` 블록 전체를 다음으로 교체:

```ts
  // 배지는 분류 3종으로 고정되고 세부 출처는 칩으로 내려갔다 (§4.1).
  it('출처는 배지가 아니라 칩으로 남는다', () => {
    const news = toLearnCardView(content({ kind: 'news', trigger_tags: ['changelog'] }));
    expect(news.badge).toBe('소식');
    expect(news.chip).toBeNull();

    const boris = toLearnCardView(content({ trigger_tags: ['boris'] }));
    expect(boris.badge).toBe('학습');
    expect(boris.chip).toBe('Boris');

    const team = toLearnCardView(content({ trigger_tags: ['team'] }));
    expect(team.badge).toBe('학습');
    expect(team.chip).toBe('팀');

    const axis = toLearnCardView(content({ trigger_tags: [], dimension: 'skill_reuse' }));
    expect(axis.badge).toBe('학습');
    expect(axis.chip).toBe('스킬로 반복 줄이기');

    // 옛 폴백 '배움'은 분류 배지와 겹쳐 버렸다 — 칩이 없는 것이 곧 "출처 없음"이다.
    const plain = toLearnCardView(content({ trigger_tags: [], dimension: null }));
    expect(plain.badge).toBe('학습');
    expect(plain.chip).toBeNull();
  });
```

`it('로컬 공지는 소식, 긴급 팁은 공지 배지', …)` 블록 전체를 다음으로 교체:

```ts
  // ⑥ §6.1 — 긴급 팁(lastShownEmergencyTip)만 「공지」로 갈린다. 이제 칩에서 갈린다.
  it('로컬 공지는 소식, 긴급 팁은 소식 배지 + 공지 칩', () => {
    const plain = toLearnCardView(content({ kind: 'news', trigger_tags: ['announcement'] }));
    expect(plain.badge).toBe('소식');
    expect(plain.chip).toBeNull();

    const urgent = toLearnCardView(content({ kind: 'news', trigger_tags: ['announcement', 'notice'] }));
    expect(urgent.badge).toBe('소식');
    expect(urgent.chip).toBe('공지');
  });
```

- [ ] **Step 6: 뮤테이션 확인**

`contentKind`의 `if (has('personal')) return 'coaching';` 줄을 잠시 지우고:

```powershell
npm test -- --run coach-helpers
```

Expected: FAIL — 「personal이 news보다 세다」와 「personal 태그 레슨은 코칭」이 깨진다. 확인했으면 줄을 되돌린다.

- [ ] **Step 7: 전체 통과 확인 후 커밋**

```powershell
npm test
```

Expected: `Tests  N passed` — N ≥ 296.

```powershell
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts
git commit -m "feat(agent): split the card badge into a kind badge and a source chip"
```

---

### Task 3: 스트림 조립 — 정렬과 상한을 같은 축으로

`first_seen` 내림차순 한 줄, 상한 6도 같은 축(§2.2·§4.2).

**Files:**
- Create: `a-mate/src/lib/ui/coach-stream.ts`
- Create: `a-mate/src/lib/ui/coach-stream.test.ts`

**Interfaces:**
- Consumes: Task 2의 `CoachKind`·`contentKind`, Task 1의 `firstSeen`, 기존 `CONTENT_CARD_LIMIT`·`toLogCardView`·`toLessonCardView`·`toLearnCardView`
- Produces:
  - `type StreamCard = { kind: 'coaching'; key: string; firstSeen: string | null; log: LogCardView } | { kind: 'learning' | 'news'; key: string; firstSeen: string | null; learn: LearnCardView }`
  - `compareFirstSeen(a: string | null | undefined, b: string | null | undefined): number`
  - `buildCoachStream(findings: CoachFinding[], content: ContentItem[]): StreamCard[]`

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/ui/coach-stream.test.ts`를 새로 만든다:

```ts
import { describe, expect, it } from 'vitest';
import { buildCoachStream, compareFirstSeen } from './coach-stream';
import { CONTENT_CARD_LIMIT } from './coach-helpers';
import type { CoachFinding, ContentItem } from '../api';

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

/** 학습 카드 하나 — first_seen과 id만 다르게 찍는다 */
const tip = (id: string, firstSeen: string | null, over: Partial<ContentItem> = {}): ContentItem =>
  content({ id, trigger_tags: [], dimension: 'automation', first_seen: firstSeen, ...over });

describe('compareFirstSeen', () => {
  it('최신이 앞 — 내림차순이다', () => {
    expect(compareFirstSeen('2026-08-03T00:00:00Z', '2026-08-01T00:00:00Z')).toBeLessThan(0);
    expect(compareFirstSeen('2026-08-01T00:00:00Z', '2026-08-03T00:00:00Z')).toBeGreaterThan(0);
    expect(compareFirstSeen('2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')).toBe(0);
  });

  it('시각을 모르는 항목은 맨 뒤 — 최신인 척하지 않는다', () => {
    expect(compareFirstSeen(null, '2020-01-01T00:00:00Z')).toBeGreaterThan(0);
    expect(compareFirstSeen('2020-01-01T00:00:00Z', null)).toBeLessThan(0);
    expect(compareFirstSeen(null, null)).toBe(0);
    expect(compareFirstSeen(undefined, null)).toBe(0);
  });
});

describe('buildCoachStream', () => {
  it('분류와 무관하게 first_seen 내림차순 한 줄로 깐다 (§2.2)', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F1', first_seen: '2026-08-02T00:00:00Z' })],
      [
        tip('T-new', '2026-08-03T00:00:00Z'),
        tip('T-old', '2026-08-01T00:00:00Z'),
      ],
    );
    expect(stream.map((c) => c.key)).toEqual(['content:T-new', 'finding:F1', 'content:T-old']);
  });

  it('행동 가능한 코칭이 소식 아래로 밀리는 것을 의도적으로 수용한다', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F1', first_seen: '2026-08-01T00:00:00Z' })],
      [tip('N1', '2026-08-05T00:00:00Z', { trigger_tags: ['changelog'], dimension: null })],
    );
    expect(stream[0].kind).toBe('news');
    expect(stream[1].kind).toBe('coaching');
  });

  it('키는 이름공간을 붙인다 — dedup_key와 콘텐츠 id가 충돌할 수 있다', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'X', first_seen: '2026-08-02T00:00:00Z' })],
      [tip('X', '2026-08-01T00:00:00Z')],
    );
    expect(new Set(stream.map((c) => c.key)).size).toBe(2);
  });

  it('personal 레슨은 코칭 분류로 문법 A 뷰모델을 싣는다', () => {
    const [card] = buildCoachStream([], [content({ first_seen: '2026-08-01T00:00:00Z' })]);
    expect(card.kind).toBe('coaching');
    expect(card.kind === 'coaching' && card.log.source).toBe('lesson');
  });

  it('활성만 스트림에 오른다 — 처분 항목은 아래 별도 줄이다 (§2.5)', () => {
    const stream = buildCoachStream(
      [finding({ dedup_key: 'F-done', status: 'resolved', first_seen: '2026-08-09T00:00:00Z' }),
       finding({ dedup_key: 'F-pend', status: 'pending', first_seen: '2026-08-09T00:00:00Z' })],
      [tip('T-gone', '2026-08-09T00:00:00Z', { status: 'dismissed' })],
    );
    expect(stream).toEqual([]);
  });

  it('상한은 콘텐츠에만, 그리고 first_seen 기준으로 자른다 (§4.2)', () => {
    // 오래된 고득점 팁이 상한을 채우고 방금 온 항목이 뒤에 있는 배치.
    // score 기준으로 자르면 최신 항목이 통째로 사라진다.
    const older = Array.from({ length: CONTENT_CARD_LIMIT }, (_, i) =>
      tip(`old-${i}`, '2026-08-01T00:00:00Z', { score: 500 }));
    const fresh = tip('fresh', '2026-08-09T00:00:00Z', { score: 300 });
    const stream = buildCoachStream([], [...older, fresh]);
    expect(stream).toHaveLength(CONTENT_CARD_LIMIT);
    expect(stream[0].key).toBe('content:fresh');
  });

  it('룰 finding은 상한 대상이 아니다 — 콘텐츠가 상한을 채워도 전부 남는다', () => {
    const many = Array.from({ length: CONTENT_CARD_LIMIT + 3 }, (_, i) =>
      tip(`t-${i}`, `2026-08-0${(i % 9) + 1}T00:00:00Z`));
    const findings = Array.from({ length: 5 }, (_, i) =>
      finding({ dedup_key: `F${i}`, first_seen: '2026-08-02T00:00:00Z' }));
    const stream = buildCoachStream(findings, many);
    expect(stream.filter((c) => c.key.startsWith('finding:'))).toHaveLength(5);
    expect(stream.filter((c) => c.key.startsWith('content:'))).toHaveLength(CONTENT_CARD_LIMIT);
  });

  it('first_seen이 같으면 키로 안정 정렬한다 — 새로고침마다 순서가 튀지 않게', () => {
    const a = buildCoachStream([], [tip('b', '2026-08-01T00:00:00Z'), tip('a', '2026-08-01T00:00:00Z')]);
    const b = buildCoachStream([], [tip('a', '2026-08-01T00:00:00Z'), tip('b', '2026-08-01T00:00:00Z')]);
    expect(a.map((c) => c.key)).toEqual(b.map((c) => c.key));
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — `Failed to resolve import "./coach-stream"`

- [ ] **Step 3: `coach-stream.ts`를 구현한다**

```ts
// 코칭 탭의 **한 줄 스트림** 조립 (스펙 §2.2·§2.3).
//
// `coach-helpers`가 "한 항목을 어떻게 그릴까"라면 여기는 "여러 항목을 어떤 순서로 몇 개나"다.
// 배지 키 계산은 CoachTab이 아니라 App.svelte(셸)가 쓰므로 소비자도 다르다.
import type { CoachFinding, ContentItem } from '../api';
import {
  CONTENT_CARD_LIMIT, contentKind, toLearnCardView, toLessonCardView, toLogCardView,
  type LearnCardView, type LogCardView,
} from './coach-helpers';

/** 스트림의 한 칸. 분류에 따라 문법 A(log) 또는 문법 B(learn) 뷰모델을 싣는다.
 * `key`는 **이름공간이 붙은** 키다 — Svelte `{#each}` 키와 배지 집합에 쓴다.
 * 카드의 `data-key`(딥링크용)는 접두사 없는 원래 키라서 서로 다르다. */
export type StreamCard =
  | { kind: 'coaching'; key: string; firstSeen: string | null; log: LogCardView }
  | { kind: 'learning' | 'news'; key: string; firstSeen: string | null; learn: LearnCardView };

/** `first_seen` 내림차순 비교자. 값이 없는 항목은 **맨 뒤**로 보낸다 —
 * 시각을 모르는 카드가 최신인 척하며 상단을 차지하면 상한에도 먼저 들어간다. */
export function compareFirstSeen(
  a: string | null | undefined,
  b: string | null | undefined,
): number {
  const x = a ?? '';
  const y = b ?? '';
  if (x === y) return 0;
  if (x === '') return 1;
  if (y === '') return -1;
  return x < y ? 1 : -1;
}

/** 활성 finding + 활성 콘텐츠를 한 줄로 조립한다.
 *
 * **상한은 콘텐츠에만, `first_seen` 기준으로** 적용한다(§4.2). score 기준으로 자르면
 * 자르는 축과 보여주는 축이 어긋나 "맨 위가 1시간 전인데 30분 전 항목이 없다"가 생긴다.
 * 룰 finding은 지금도 상한 대상이 아니다.
 *
 * 고정 슬롯 항목은 호출부가 **미리 빼고** 넘긴다 — 그 항목은 스트림 밖 별도 칸이고(§2.5),
 * `pinnedNewsItem`이 score 순 배열을 전제하므로 정렬보다 먼저 골라내야 한다. */
export function buildCoachStream(
  findings: CoachFinding[],
  content: ContentItem[],
): StreamCard[] {
  const capped = content
    .filter((c) => c.status === 'new')
    .slice()
    .sort((a, b) => compareFirstSeen(a.first_seen, b.first_seen) || a.id.localeCompare(b.id))
    .slice(0, CONTENT_CARD_LIMIT);

  const cards: StreamCard[] = [];
  for (const f of findings.filter((f) => f.status === 'new')) {
    cards.push({
      kind: 'coaching',
      key: `finding:${f.dedup_key}`,
      firstSeen: f.first_seen ?? null,
      log: toLogCardView(f),
    });
  }
  for (const c of capped) {
    const kind = contentKind(c);
    const key = `content:${c.id}`;
    const firstSeen = c.first_seen ?? null;
    if (kind === 'coaching') {
      cards.push({ kind, key, firstSeen, log: toLessonCardView(c) });
    } else {
      cards.push({ kind, key, firstSeen, learn: toLearnCardView(c) });
    }
  }
  return cards.sort(
    (a, b) => compareFirstSeen(a.firstSeen, b.firstSeen) || a.key.localeCompare(b.key),
  );
}
```

- [ ] **Step 4: 테스트 통과 확인**

```powershell
npm test -- --run coach-stream
```

Expected: PASS — `Tests  9 passed (9)`

- [ ] **Step 5: 뮤테이션 확인**

`buildCoachStream`의 콘텐츠 `.sort(...)` 호출을 잠시 지워(= score 순 그대로 자르기) 실행한다:

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — 「상한은 콘텐츠에만, 그리고 first_seen 기준으로 자른다」가 깨진다. **이 테스트가 이 태스크의 핵심 주장**이므로 반드시 실패해야 한다. 확인 후 되돌린다.

- [ ] **Step 6: 커밋**

```powershell
npm test
git add a-mate/src/lib/ui/coach-stream.ts a-mate/src/lib/ui/coach-stream.test.ts
git commit -m "feat(agent): assemble the coach stream sorted and capped by first_seen"
```

---

### Task 4: 공지 수명 — 기한이 있으면 그때까지, 없으면 며칠

해소된 장애 공지가 최고 점수(325)로 최상단에 남는 것을 실환경에서 발견해 들어온 요구다(§2.6). 공지 데이터엔 **발행 시각이 없어** `first_seen`("우리가 처음 본 때")으로 잰다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-helpers.ts` — `validDeadline`을 export로
- Modify: `a-mate/src/lib/ui/coach-stream.ts`
- Modify: `a-mate/src/lib/ui/coach-stream.test.ts`

**Interfaces:**
- Consumes: Task 3의 `coach-stream.ts`
- Produces:
  - `ANNOUNCEMENT_TTL_DAYS = 7`, `EMERGENCY_TTL_DAYS = 2`
  - `isAnnouncementLive(c: ContentItem, today: string, nowMs: number): boolean`
  - `liveContent(content: ContentItem[], today: string, nowMs: number): ContentItem[]`
  - `localDateString(d: Date): string`
  - `coach-helpers`의 `validDeadline(d: string): boolean` — private에서 export로 승격

**설계 결정**: `liveContent`는 **수명만** 거른다. `status` 필터는 하지 않는다 — `listContent(false)`가 백엔드에서 이미 `status='new'`만 주고, `buildCoachStream`·`activeCoachKeys`가 각자 방어적으로 한 번 더 본다. 그리고 **`filter`라 순서를 보존**하므로 `pinnedNewsItem`이 전제하는 score 내림차순이 깨지지 않는다(§2.2 파이프라인).

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`coach-stream.test.ts`의 `tip` 헬퍼 아래에 공지 팩토리와 시계 상수를 더한다:

```ts
/** 로컬 공지 하나. content 팩토리의 기본 태그가 personal이라 반드시 덮어쓴다. */
const ann = (over: Partial<ContentItem> = {}): ContentItem =>
  content({
    id: 'cc-announce-x', kind: 'news', dimension: null,
    trigger_tags: ['announcement'], title: '공지', body: '본문', ...over,
  });

const TODAY = '2026-08-10';
const NOW_MS = Date.parse('2026-08-10T00:00:00Z');
const daysBefore = (n: number) => new Date(NOW_MS - n * 24 * 60 * 60 * 1000).toISOString();
```

파일 끝에 추가:

```ts
describe('isAnnouncementLive — 공지 수명 (§2.6)', () => {
  it('공지가 아닌 항목은 이 규칙과 무관하다', () => {
    expect(isAnnouncementLive(tip('T1', daysBefore(400)), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(content({ first_seen: daysBefore(400) }), TODAY, NOW_MS)).toBe(true);
  });

  it('기한이 있으면 그 날까지 — 당일은 살아 있고 지나면 내린다', () => {
    expect(isAnnouncementLive(ann({ deadline: '2026-08-10', first_seen: daysBefore(1) }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ deadline: '2026-08-09', first_seen: daysBefore(1) }), TODAY, NOW_MS)).toBe(false);
  });

  it('기한이 일수 규칙을 이긴다 — 「명시한 기간 동안만」이 우선이다', () => {
    // 처음 본 지 30일이 지났지만 공지가 명시한 기한이 아직 남았다
    const long = ann({ deadline: '2026-09-01', first_seen: daysBefore(30) });
    expect(isAnnouncementLive(long, TODAY, NOW_MS)).toBe(true);
  });

  it('기한이 없으면 일반 공지는 7일', () => {
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(6) }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(8) }), TODAY, NOW_MS)).toBe(false);
  });

  it('기한이 없으면 장애 공지는 2일 — 「마지막으로 표시한」 스냅샷이라 만료를 알 수 없다', () => {
    const emerg = (fs: string) => ann({ trigger_tags: ['announcement', 'notice'], first_seen: fs });
    expect(isAnnouncementLive(emerg(daysBefore(1)), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(emerg(daysBefore(3)), TODAY, NOW_MS)).toBe(false);
    // 같은 나이라도 일반 공지는 아직 살아 있다 — 둘을 가르는 것이 이 규칙의 핵심이다
    expect(isAnnouncementLive(ann({ first_seen: daysBefore(3) }), TODAY, NOW_MS)).toBe(true);
  });

  it('깨진 기한은 무시하고 일수 규칙으로 — LLM 오추출 방어', () => {
    for (const bad of ['2026-13-40', '곧', '', '  ']) {
      expect(isAnnouncementLive(ann({ deadline: bad, first_seen: daysBefore(8) }), TODAY, NOW_MS), bad).toBe(false);
      expect(isAnnouncementLive(ann({ deadline: bad, first_seen: daysBefore(1) }), TODAY, NOW_MS), bad).toBe(true);
    }
  });

  it('처음 본 시각을 모르면 보이는 쪽 — 시각이 없다고 카드를 뺏지 않는다', () => {
    expect(isAnnouncementLive(ann({ first_seen: null }), TODAY, NOW_MS)).toBe(true);
    expect(isAnnouncementLive(ann({ first_seen: '언젠가' }), TODAY, NOW_MS)).toBe(true);
  });
});

describe('liveContent', () => {
  it('수명이 끝난 공지만 걷어내고 나머지는 그대로 둔다', () => {
    const rows = [
      ann({ id: 'a-live', first_seen: daysBefore(1) }),
      ann({ id: 'a-dead', first_seen: daysBefore(9) }),
      tip('T-old', daysBefore(400)),
    ];
    expect(liveContent(rows, TODAY, NOW_MS).map((c) => c.id)).toEqual(['a-live', 'T-old']);
  });

  it('순서를 보존한다 — pinnedNewsItem이 score 내림차순을 전제한다', () => {
    const rows = [ann({ id: 'p3', score: 325 }), ann({ id: 'p2', score: 321 }), ann({ id: 'p1', score: 320 })]
      .map((c) => ({ ...c, first_seen: daysBefore(1) }));
    expect(liveContent(rows, TODAY, NOW_MS).map((c) => c.id)).toEqual(['p3', 'p2', 'p1']);
  });
});

describe('localDateString', () => {
  it('로컬 날짜를 YYYY-MM-DD로 — deadline과 같은 축으로 비교하기 위해', () => {
    expect(localDateString(new Date(2026, 7, 3))).toBe('2026-08-03');
    expect(localDateString(new Date(2026, 11, 25))).toBe('2026-12-25');
  });
});
```

import 줄에 `ANNOUNCEMENT_TTL_DAYS`가 아니라 `isAnnouncementLive`, `liveContent`, `localDateString`을 더한다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — `isAnnouncementLive is not a function`

- [ ] **Step 3: `validDeadline`을 export로 올린다**

`coach-helpers.ts`에서:

```ts
/** 기한 문자열이 실제 달력 날짜인가 — `2026-13-40`·`곧` 같은 오추출을 거른다. */
export function validDeadline(d: string): boolean {
```

(`function` 앞에 `export`만 붙인다. 본문은 그대로.)

- [ ] **Step 4: 수명 판정을 구현한다**

`coach-stream.ts`의 import에 `validDeadline`을 더하고, 파일 끝에 추가:

```ts
// ── 공지 수명 (§2.6) ───────────────────────────────────────────────────────
//
// 공지는 시한부인데 **데이터에 발행 시각이 없다** — GrowthBook 캐시에도
// `lastShownEmergencyTip`에도 날짜 필드가 없다. 우리가 가진 유일한 시각은
// `first_seen`("우리가 처음 본 때")이라 그것으로 잰다.

/** 기한 없는 일반 공지(프로모션·릴리스)가 화면에 남는 기간. */
export const ANNOUNCEMENT_TTL_DAYS = 7;
/** 기한 없는 장애 공지가 남는 기간. `lastShownEmergencyTip`은 「마지막으로 표시한」
 * 스냅샷이지 「지금 유효한」 값이 아니고 Claude Code가 그 키를 지우지 않는다 —
 * 해소된 장애가 영원히 최고 점수로 남는다. 만료 신호가 없으므로
 * "장애는 길어야 하루이틀"이라는 성질로 대신한다. */
export const EMERGENCY_TTL_DAYS = 2;

const DAY_MS = 24 * 60 * 60 * 1000;
/** Rust `announcements::TAG_ANNOUNCEMENT`·`TAG_NOTICE`와 짝이다. */
const TAG_ANNOUNCEMENT = 'announcement';
const TAG_NOTICE = 'notice';

const hasTag = (c: ContentItem, t: string) => c.trigger_tags?.includes(t) ?? false;

/** 이 공지를 아직 보여줄까. 공지가 아닌 항목은 언제나 `true`(이 규칙의 대상이 아니다).
 *
 * 기한이 **일수 규칙을 이긴다** — 사용자가 정한 것은 "명시한 기간 동안만"이다.
 * 기한을 모르거나 깨졌으면 일수로 떨어진다(기본 폐쇄가 아니라 기본 노출: 시각을
 * 모른다고 카드를 뺏지 않는다 — `isDisposedVisible`과 같은 규약). */
export function isAnnouncementLive(c: ContentItem, today: string, nowMs: number): boolean {
  if (!hasTag(c, TAG_ANNOUNCEMENT)) return true;
  const d = c.deadline?.trim();
  if (d && validDeadline(d)) return d >= today;
  if (!c.first_seen) return true;
  const ts = Date.parse(c.first_seen);
  if (Number.isNaN(ts)) return true;
  const ttl = hasTag(c, TAG_NOTICE) ? EMERGENCY_TTL_DAYS : ANNOUNCEMENT_TTL_DAYS;
  return nowMs - ts < ttl * DAY_MS;
}

/** 수명이 끝난 공지를 걷어낸다. **`status` 필터는 하지 않는다** — 백엔드
 * `listContent(false)`가 이미 걸렀고 스트림·배지가 각자 한 번 더 본다.
 * `filter`라 **순서가 보존**되므로 `pinnedNewsItem`이 전제하는 score 내림차순이 깨지지 않는다. */
export function liveContent(content: ContentItem[], today: string, nowMs: number): ContentItem[] {
  return content.filter((c) => isAnnouncementLive(c, today, nowMs));
}

/** 로컬 날짜 `YYYY-MM-DD`. `deadline`이 로컬 달력 날짜라 UTC로 비교하면 하루가 어긋난다. */
export function localDateString(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
```

- [ ] **Step 5: 테스트 통과 확인**

```powershell
npm test -- --run coach-stream
```

Expected: PASS

- [ ] **Step 6: 뮤테이션 확인**

`isAnnouncementLive`에서 `const ttl = hasTag(c, TAG_NOTICE) ? EMERGENCY_TTL_DAYS : ANNOUNCEMENT_TTL_DAYS;`를 `const ttl = ANNOUNCEMENT_TTL_DAYS;`로 잠시 바꾼다:

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — 「기한이 없으면 장애 공지는 2일」이 깨진다. 되돌린다.

- [ ] **Step 7: 커밋**

```powershell
npm test
git add a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-stream.ts a-mate/src/lib/ui/coach-stream.test.ts
git commit -m "feat(agent): expire announcements by deadline or age"
```

---

### Task 5: 탭 배지 — 활성 키 집합 diff

`first_seen` 시각 비교로는 `pending`→`new`와 재발 복귀를 놓친다(§3.1·§4.5). 활성 키 집합으로 센다.

**Files:**
- Modify: `a-mate/src/lib/ui/coach-stream.ts`
- Modify: `a-mate/src/lib/ui/coach-stream.test.ts`

**Interfaces:**
- Consumes: Task 3·4의 `coach-stream.ts`
- Produces:
  - `activeCoachKeys(findings: CoachFinding[], content: ContentItem[]): string[]`
  - `unseenCoachCount(activeKeys: readonly string[], seen: readonly string[]): number`
  - `parseSeenCoachKeys(raw: string | null): string[]` — 저장값 해석(테스트 대상)
  - `loadSeenCoachKeys(): string[]` — localStorage 읽기 (테스트 불가)
  - `saveSeenCoachKeys(keys: readonly string[]): string[]` — localStorage 쓰기 (테스트 불가)

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`coach-stream.test.ts` 끝에 추가:

```ts
describe('activeCoachKeys — 배지가 세는 대상 (§2.3)', () => {
  it('활성 finding과 활성 콘텐츠를 이름공간 붙인 키로 모은다', () => {
    const keys = activeCoachKeys(
      [finding({ dedup_key: 'F1' })],
      [tip('T1', '2026-08-01T00:00:00Z')],
    );
    expect(keys).toEqual(['finding:F1', 'content:T1']);
  });

  it('처분·판정중 항목은 세지 않는다 — ③이 지키라고 한 성질', () => {
    const keys = activeCoachKeys(
      [finding({ dedup_key: 'F-r', status: 'resolved' }),
       finding({ dedup_key: 'F-d', status: 'dismissed' }),
       finding({ dedup_key: 'F-p', status: 'pending' }),
       finding({ dedup_key: 'F-x', status: 'rejected' })],
      [tip('T-d', '2026-08-01T00:00:00Z', { status: 'dismissed' })],
    );
    expect(keys).toEqual([]);
  });

  it('상한을 적용하기 전의 활성 전부를 센다 (§2.3)', () => {
    const many = Array.from({ length: CONTENT_CARD_LIMIT + 4 }, (_, i) =>
      tip(`t-${i}`, '2026-08-01T00:00:00Z'));
    expect(activeCoachKeys([], many)).toHaveLength(CONTENT_CARD_LIMIT + 4);
  });
});

describe('unseenCoachCount — 안 본 개수', () => {
  it('본 적 없는 키만 센다', () => {
    expect(unseenCoachCount(['a', 'b', 'c'], ['a'])).toBe(2);
    expect(unseenCoachCount(['a', 'b'], ['a', 'b'])).toBe(0);
  });

  it('첫 실행이면 전부 안 본 것 — 처음 보는 게 맞다', () => {
    expect(unseenCoachCount(['a', 'b'], [])).toBe(2);
  });

  it('사라진 키가 집합에 남아 있어도 개수를 늘리지 않는다', () => {
    expect(unseenCoachCount(['a'], ['a', 'gone-1', 'gone-2'])).toBe(0);
  });

  it('resolved→new 복귀를 잡는다 — first_seen 비교가 놓치던 전이 (§4.5)', () => {
    // 어제 「해결함」을 눌러 활성 집합에서 빠졌고, 오늘 재발로 같은 키가 돌아왔다.
    // first_seen은 그대로 과거라 시각 비교로는 안 잡힌다.
    const revived = finding({ dedup_key: 'F1', status: 'new', first_seen: '2026-07-01T00:00:00Z' });
    const seenWhileResolved = activeCoachKeys([finding({ dedup_key: 'F1', status: 'resolved' })], []);
    expect(seenWhileResolved).toEqual([]);
    expect(unseenCoachCount(activeCoachKeys([revived], []), seenWhileResolved)).toBe(1);
  });

  it('pending→new 통과를 잡는다 — 판정이 며칠 걸려도', () => {
    const seenWhilePending = activeCoachKeys([finding({ dedup_key: 'F2', status: 'pending' })], []);
    const passed = finding({ dedup_key: 'F2', status: 'new', first_seen: '2026-07-01T00:00:00Z' });
    expect(unseenCoachCount(activeCoachKeys([passed], []), seenWhilePending)).toBe(1);
  });
});

describe('parseSeenCoachKeys — 저장값 해석 (localStorage 밖의 순수 부분)', () => {
  it('문자열 배열만 통과시킨다', () => {
    expect(parseSeenCoachKeys('["finding:F1","content:T1"]')).toEqual(['finding:F1', 'content:T1']);
    expect(parseSeenCoachKeys('["a",42,null,"b"]')).toEqual(['a', 'b']);
  });

  it('없거나 깨진 값은 빈 배열 — 배지 때문에 앱이 죽지 않는다', () => {
    expect(parseSeenCoachKeys(null)).toEqual([]);
    expect(parseSeenCoachKeys('{oops')).toEqual([]);
    expect(parseSeenCoachKeys('{"not":"an array"}')).toEqual([]);
    expect(parseSeenCoachKeys('"just a string"')).toEqual([]);
  });
});
```

import 줄에 `activeCoachKeys`, `unseenCoachCount`, `parseSeenCoachKeys`를 더한다.

⚠ **`loadSeenCoachKeys`·`saveSeenCoachKeys` 자체에는 테스트를 쓰지 않는다** — 이 저장소의 vitest는 node 환경이라 `localStorage`가 정의되지 않는다(Global Constraints). 그래서 해석 부분만 `parseSeenCoachKeys`로 떼어내 테스트한다. 이 분리가 이 태스크에서 유일하게 검증 가능한 저장 로직이다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — `activeCoachKeys is not a function`

- [ ] **Step 3: 구현한다**

`coach-stream.ts` 끝에 추가:

```ts
// ── 탭 배지 (§2.3·§4.5) ────────────────────────────────────────────────────
//
// `first_seen > lastCoachSeenAt` 시각 비교는 쓰지 않는다. `first_seen`은 삽입 시각이라
// `pending`→`new`와 재발 복귀(`resolved`→`new`)를 둘 다 놓치는데, 그 둘이야말로
// 카드가 새로 보이기 시작하는 순간이다. 대신 "마지막으로 봤을 때 활성이던 키 집합"과
// 비교한다 — 두 전이 모두 저장 시점에 활성이 아니었으므로 공짜로 잡힌다.

const SEEN_KEYS_STORAGE = 'agent-mentor.coachSeenKeys';

/** 배지가 세는 대상 = **상한 적용 전의 활성 항목 전부**(§2.3).
 * 화면에 보이는 것만 세려면 고정 슬롯 판정(pinAcks·today)을 셸로 끌어와야 해서
 * 배보다 배꼽이 커진다. 상한 밖 항목까지 "봤다"고 치는 대가는 작다. */
export function activeCoachKeys(findings: CoachFinding[], content: ContentItem[]): string[] {
  return [
    ...findings.filter((f) => f.status === 'new').map((f) => `finding:${f.dedup_key}`),
    ...content.filter((c) => c.status === 'new').map((c) => `content:${c.id}`),
  ];
}

export function unseenCoachCount(
  activeKeys: readonly string[],
  seen: readonly string[],
): number {
  const s = new Set(seen);
  return activeKeys.filter((k) => !s.has(k)).length;
}

/** 저장값 해석. **localStorage에서 떼어낸 이유는 테스트다** — 이 저장소의 vitest는
 * node 환경이라 `localStorage`가 없어 `loadSeenCoachKeys`는 직접 검증할 수 없다. */
export function parseSeenCoachKeys(raw: string | null): string[] {
  try {
    const v = JSON.parse(raw ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

export function loadSeenCoachKeys(): string[] {
  return parseSeenCoachKeys(localStorage.getItem(SEEN_KEYS_STORAGE));
}

/** **덮어쓴다** — 누적하면 처분·프룬으로 사라진 키가 영원히 남아 집합이 무한히 자란다.
 * 실제로 저장된 목록을 돌려주므로 호출부 상태와 localStorage가 어긋나지 않는다
 * (`savePinAcks`와 같은 규약). */
export function saveSeenCoachKeys(keys: readonly string[]): string[] {
  const kept = [...keys];
  localStorage.setItem(SEEN_KEYS_STORAGE, JSON.stringify(kept));
  return kept;
}
```

- [ ] **Step 4: 테스트 통과 확인**

```powershell
npm test -- --run coach-stream
```

Expected: PASS

- [ ] **Step 5: 뮤테이션 확인**

`activeCoachKeys`의 `.filter((f) => f.status === 'new')`를 `.filter(() => true)`로 잠시 바꾼다:

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — 「처분·판정중 항목은 세지 않는다」와 재발/pending 두 케이스가 깨진다. 되돌린다.

- [ ] **Step 6: 커밋**

```powershell
npm test
git add a-mate/src/lib/ui/coach-stream.ts a-mate/src/lib/ui/coach-stream.test.ts
git commit -m "feat(agent): count unseen coach cards by active key set diff"
```

---

### Task 6: 카드 컴포넌트 — 분류 배지·보조 칩·라인 색

라인 색이 분류를 뜻하므로 severity를 색으로 쓸 수 없다(§2.1).

**Files:**
- Modify: `a-mate/src/lib/ui/coach/LogCard.svelte`
- Modify: `a-mate/src/lib/ui/coach/LearnCard.svelte`

**Interfaces:**
- Consumes: Task 2의 `LogCardView.badge`·`LearnCardView.badge`·`LearnCardView.chip`·`LearnCardView.kind`
- Produces: 렌더 변경만 — props 시그니처는 그대로다(`view`·`showLinks`·스니펫·`pinned`·`onAck`·`onDismiss`)

컴포넌트 테스트 라이브러리가 없어 렌더 검증은 불가능하다. `npm test`의 `svelte-check`가 타입·미사용 CSS를 잡는 것이 유일한 자동 검증이다.

- [ ] **Step 1: `LogCard`에서 severity 색을 걷어내고 분류 배지를 넣는다**

`<article>` 줄에서 `class:warn={view.icon === '⚠'}`을 **제거**한다:

```svelte
<article class="card" data-key={view.key}>
```

`<header>` 안의 첫 줄을 배지 + 제목으로 바꾼다:

```svelte
  <header>
    <span class="badge">{view.badge}</span>
    <span class="title">{view.icon} {view.title}</span>
    {#if view.chip}<span class="chip">{view.chip}</span>{/if}
  </header>
```

스타일에서 `.card.warn { border-left-color: var(--pastel-coral); }` 줄을 **삭제**하고, `header` 규칙을 배지가 들어갈 수 있게 고친 뒤 `.badge`를 더한다:

```css
  header { display: flex; gap: 8px; align-items: baseline; }
  .badge {
    font-size: 10px; font-weight: 700; color: var(--accent-ink); white-space: nowrap; flex: none;
    background: var(--pastel-mint); border-radius: 999px; padding: 3px 10px;
  }
  .title { font-weight: 600; flex: 1; }
```

(옛 `header`는 `justify-content: space-between`으로 근거 칩을 오른쪽 끝에 붙였다. 배지가 왼쪽에 오면서 `.title`이 남는 폭을 먹고 칩이 자연히 오른쪽에 남는다.)

- [ ] **Step 2: `LearnCard`에 보조 칩과 분류 라인 색을 넣는다**

`<article>` 줄에 분류 클래스를 더한다:

```svelte
<article class="card" class:pinned class:learning={view.kind === 'learning'} data-key={view.id}>
```

`<header>`의 배지 뒤에 보조 칩을 넣는다:

```svelte
  <header>
    <span class="badge">{view.badge}</span>
    {#if view.chip}<span class="chip">{view.chip}</span>{/if}
    {#if deadlineChip}<span class="deadline">{deadlineChip}</span>{/if}
```

스타일에 두 규칙을 더한다 — `.card` 규칙 바로 아래에:

```css
  /* 라인 색이 분류를 뜻한다 (§2.1). 기본은 소식(--accent), 학습은 라벤더. */
  .card.learning { border-left-color: var(--pastel-lav); }
```

`.deadline` 규칙 위에:

```css
  /* 보조 칩 — 세부 출처(공지·Boris·팀·축 라벨). 분류 배지보다 한 단 낮은 위계다 (§4.1) */
  .chip {
    font-size: 10px; color: var(--ink-soft); white-space: nowrap;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 999px; padding: 2px 8px;
  }
```

- [ ] **Step 3: 타입·미사용 CSS 검사**

```powershell
npm test
```

Expected: PASS. `svelte-check`가 `Unused CSS selector` 경고를 내면(예: 지우다 만 `.warn`) 해당 규칙을 지운다. `--threshold error`라 경고로는 실패하지 않으므로 **출력을 눈으로 읽을 것.**

- [ ] **Step 4: 커밋**

```powershell
git add a-mate/src/lib/ui/coach/LogCard.svelte a-mate/src/lib/ui/coach/LearnCard.svelte
git commit -m "feat(agent): render kind badge, source chip and kind-colored rail on cards"
```

---

### Task 7: `CoachTab` — 섹션을 걷어내고 한 줄로 깐다

**Files:**
- Modify: `a-mate/src/lib/ui/CoachTab.svelte`

**Interfaces:**
- Consumes: Task 3의 `buildCoachStream`·`StreamCard`, Task 4의 `liveContent`·`localDateString`, Task 5의 `saveSeenCoachKeys`·`activeCoachKeys`
- Produces: 렌더 변경만. `focusKey`·`onChanged` props는 그대로다.

- [ ] **Step 1: import와 파생값을 바꾼다**

`coach-helpers` import에서 `partitionCoachItems`를 빼고 `coach-stream` import를 더한다:

```ts
  import { ctxLine, loadPinAcks, pinnedNewsItem, savePinAcks, sessionIdsOf, toDisposedRows, toLearnCardView, totalSessionsOf, type DisposedRow } from './coach-helpers';
  import { activeCoachKeys, buildCoachStream, liveContent, localDateString, saveSeenCoachKeys } from './coach-stream';
```

`localToday` 지역 함수를 지우고 공용 함수를 쓴다 — Task 4가 같은 계산을 `localDateString`으로 내놨으므로 두 벌을 두지 않는다:

```ts
  let today = $state(localDateString(new Date()));
  $effect(() => {
    const id = setInterval(() => {
      const d = localDateString(new Date());
      if (d !== today) today = d;
    }, 60_000);
    return () => clearInterval(id);
  });
```

`const pinned = $derived(pinnedNewsItem(tips, pinAcks, today));` 줄 **위**에 수명 필터를 넣고, 고정 슬롯이 그 결과를 보게 한다:

```ts
  // 수명이 끝난 공지를 먼저 걷어낸다 (§2.6) — 스트림·고정 슬롯·배지가 같은 목록을 본다.
  // `liveContent`는 filter라 score 내림차순이 보존된다(pinnedNewsItem의 전제).
  const live = $derived(liveContent(tips, today, nowMs));
  const pinned = $derived(pinnedNewsItem(live, pinAcks, today));
```

`const sections = $derived(partitionCoachItems(...))` 줄을 다음으로 **교체**한다:

```ts
  // 한 줄 스트림 (§2.2) — 고정 항목은 미리 뺀다. `pinnedNewsItem`이 score 순 배열을
  // 전제하므로(⑥ §6.4의 priority 규칙) **정렬보다 먼저** 골라내야 한다.
  const stream = $derived(buildCoachStream(active, live.filter((t) => t.id !== pinned?.id)));
```

⚠ `nowMs`(1시간 눈금 시계)와 `today`(분 단위 날짜) 선언이 `live`보다 **위에** 있어야 한다. 현재 파일에서 `nowMs`는 처분 줄 근처(아래쪽)에 선언돼 있으므로 **`pinned` 위로 끌어올린다.** `$derived`는 선언 순서를 따진다.

`findingByKey`는 그대로 둔다 — 스트림 카드에서 원본 finding을 찾는 데 계속 쓴다.

- [ ] **Step 2: 탭이 보이는 동안 본 것으로 기록한다**

`const disposed = $derived(...)` 줄 아래에 추가:

```ts
  // 탭이 열려 있는 동안 목록이 바뀌면 그때그때 본 것으로 친다 (§2.3).
  // 상한 적용 **전**의 활성 전부를 저장하되 수명이 끝난 공지는 뺀다 — 셸의 배지가 같은 기준으로 센다.
  $effect(() => {
    saveSeenCoachKeys(activeCoachKeys(all, liveContent(allTips, today, nowMs)));
    onChanged?.();
  });
```

⚠ `onChanged?.()`를 여기서 부르는 이유: 셸(`App.svelte`)의 배지가 localStorage를 다시 읽어야 0이 된다. 이 호출이 없으면 탭을 열어도 배지가 그대로 남는다.

- [ ] **Step 3: 마크업을 한 줄 스트림으로 바꾼다**

`<!-- 스펙 §3: 「내 로그에서」가 0건이면 … -->` 주석부터 `{#if sections.learn.length > 0}` 블록 끝(`{/if}`)까지를 통째로 다음으로 **교체**한다:

```svelte
  <!-- 섹션 헤더 없음 — 분류는 좌측 라인 색·배지가 나타낸다 (§2.1). 정렬은 first_seen 내림차순. -->
  {#if stream.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {/if}

  {#each stream as c (c.key)}
    {#if c.kind === 'coaching'}
      {@const f = findingByKey.get(c.log.key)}
      <LogCard view={c.log} {showLinks}>
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
            <button onclick={() => mark(f.dedup_key, 'resolved')}>해결함</button>
            <button onclick={() => mark(f.dedup_key, 'dismissed')}>무시</button>
          {:else}
            <!-- 개인 레슨도 같은 두 처분 — 「해결함」은 방출이 멈추면 정리되고(§5.2),
                 「무시」는 영구다. 둘 다 하단 처분 줄에서 실행취소할 수 있다. -->
            <button onclick={() => markLesson(c.log.key, 'resolved')}>해결함</button>
            <button onclick={() => markLesson(c.log.key, 'dismissed')}>무시</button>
          {/if}
        {/snippet}
        {#snippet footer()}
          {#if f}
            <!-- occurrences는 스캔 횟수라 "N회 관측"이 카드 본문에선 오독을 유발 → 여기 원본 데이터로 (스펙 §1.2 D5) -->
            <button class="raw-toggle" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
              {open === f.dedup_key ? '▾' : '▸'} 원본 데이터
            </button>
            {#if open === f.dedup_key}
              <p class="why">스캔에서 {f.occurrences}회 관측 · 마지막 {f.last_seen ?? '–'}</p>
              <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
            {/if}
          {/if}
        {/snippet}
      </LogCard>
    {:else}
      <LearnCard view={c.learn} {showLinks} onDismiss={dismissLearn} />
    {/if}
  {/each}
```

고정 슬롯 블록(`{#if pinnedView}`)과 처분 줄 블록(`{#if disposed.length > 0}`)은 **위치·내용 그대로 둔다** — §2.5가 확정한 배치가 이미 그 자리다.

- [ ] **Step 4: 내 변경이 만든 죽은 코드를 지운다**

`CoachTab`의 `<style>`에서 섹션 헤더 규칙을 삭제한다:

```css
  .section { … }
  .section .count { … }
```

`partitionCoachItems`는 이 태스크로 **마지막 호출부를 잃는다**(`CoachTab`이 유일한 소비자였다). `coach-helpers.ts`에서 그 함수를 지우고, `coach-helpers.test.ts`의 `describe('partitionCoachItems', …)` 블록도 지운다.

- ⚠ `isPersonal` 헬퍼는 **지우지 말 것** — `toDisposedRows`가 계속 쓴다.
- ⚠ `CONTENT_CARD_LIMIT`도 **남긴다** — `coach-stream`이 쓴다.
- ⚠ 지우는 테스트가 지키던 성질(상한이 콘텐츠에만 걸린다·고정 슬롯은 상한 밖이다)은 **Task 3의 스트림 테스트가 이어받았다.** 성질이 사라지는 게 아니라 주장하는 자리가 옮겨간 것이다. 옮겨간 자리에 그 주장이 실제로 있는지 확인하고 지울 것.

- [ ] **Step 5: 검사**

```powershell
npm test
```

Expected: PASS, `Tests  N passed` (N ≥ 296 + 신규분). `svelte-check`가 `partitionCoachItems` 미사용 import나 `Unused CSS selector ".section"`을 잡으면 지운다.

- [ ] **Step 6: 커밋**

```powershell
git add a-mate/src/lib/ui/CoachTab.svelte
git commit -m "feat(agent): lay the coach tab out as one time-ordered stream"
```

---

### Task 8: 셸 배지 — 「미처리 개수」에서 「안 본 개수」로

**Files:**
- Modify: `a-mate/src/App.svelte`

**Interfaces:**
- Consumes: Task 5의 `activeCoachKeys`·`unseenCoachCount`·`loadSeenCoachKeys`, Task 4의 `liveContent`·`localDateString`
- Produces: 없음 (최종 소비자)

- [ ] **Step 1: import를 더한다**

api import 목록에 `listContent`를 더하고(`listFindings` 옆), 새 import 줄을 추가한다:

```ts
  import { activeCoachKeys, liveContent, loadSeenCoachKeys, localDateString, unseenCoachCount } from './lib/ui/coach-stream';
```

- [ ] **Step 2: 상태를 바꾼다**

`let activeCount = $state(0);` 줄을 다음으로 **교체**한다:

```ts
  // N1 탭 뱃지 — 코칭은 「안 본 개수」다(§2.3). 처분하지 않아도 탭을 열면 사라진다.
  // 시각 비교가 아니라 활성 키 집합 diff라 pending→new·재발 복귀도 잡힌다(§4.5).
  let coachKeys = $state<string[]>([]);
  let seenCoachKeys = $state<string[]>(loadSeenCoachKeys());
  const unseenCoach = $derived(unseenCoachCount(coachKeys, seenCoachKeys));
```

- [ ] **Step 3: `refresh()`가 콘텐츠까지 읽게 한다**

`refresh()` 안의 `activeCount = …` 줄을 다음으로 교체한다:

```ts
    const [fs, cs] = await Promise.all([
      listFindings(false).catch(() => []),
      listContent(false).catch(() => []),
    ]);
    // 수명이 끝난 공지는 화면에 없으므로 배지도 세지 않는다 (§2.6).
    // 셸에는 CoachTab 같은 눈금 시계를 두지 않는다 — refresh()는 함수라 호출 시점에
    // 계산되고(반응성 함정 없음), 스캔·큐레이션마다 다시 돈다. 유휴 상태로 자정을
    // 넘겨 잠깐 낡는 것은 배지 정밀도로 감수한다.
    const now = new Date();
    coachKeys = activeCoachKeys(fs, liveContent(cs, localDateString(now), now.getTime()));
    // CoachTab이 열려 있는 동안 저장한 집합을 다시 읽는다 — 탭을 보면 배지가 0이 된다.
    seenCoachKeys = loadSeenCoachKeys();
```

- [ ] **Step 4: 콘텐츠 갱신에도 반응하게 한다**

`refresh()` 선언 아래 `onScanDone(() => refresh());` 줄(현재 200줄)이 있다. **그 바로 아래**에 한 줄을 더한다:

```ts
  // 큐레이션이 끝나면 콘텐츠 쪽 활성 목록이 바뀐다 — 배지가 따라와야 한다.
  onContentReady(() => refresh());
```

api import 목록에 `onContentReady`를 추가한다.

⚠ 큐레이션은 노출 목록을 바꾸기만 하는 게 아니라 행을 지우기도 하므로 **빈 payload로도 emit된다**(§4.6). 리스너는 payload를 보지 않는다.

- [ ] **Step 5: 배지 렌더를 바꾼다**

```svelte
            {#if t.id === 'coach' && unseenCoach > 0}<span class="badge">{unseenCoach}</span>{/if}
```

- [ ] **Step 6: 검사**

```powershell
npm test
```

Expected: PASS. `svelte-check`가 `activeCount` 미사용을 잡으면 남은 참조를 지운다.

- [ ] **Step 7: 커밋**

```powershell
git add a-mate/src/App.svelte
git commit -m "feat(agent): switch the coach tab badge to an unseen count"
```

---

### Task 9: 홈 「지금 볼 코칭」 위젯 — 세 분류 모두

지금은 finding만 본다(§3.5). 콘텐츠도 싣고 분류 배지를 붙인다(§2.4).

**Files:**
- Modify: `a-mate/src/lib/ui/coach-stream.ts`
- Modify: `a-mate/src/lib/ui/coach-stream.test.ts`
- Modify: `a-mate/src/lib/ui/home/SaveTop3.svelte`
- Modify: `a-mate/src/lib/ui/HomeTab.svelte`

**Interfaces:**
- Consumes: Task 3의 `buildCoachStream`, Task 4의 `liveContent`·`localDateString`
- Produces:
  - `interface CoachWidgetRow { key: string; kind: CoachKind; badge: string; oneLine: string }`
  - `toWidgetRows(findings: CoachFinding[], content: ContentItem[], limit?: number): CoachWidgetRow[]`
  - `SaveTop3` props가 `{ findings }` → `{ rows }`로 바뀐다.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`coach-stream.test.ts` 끝에 추가:

```ts
describe('toWidgetRows — 홈 위젯 (§2.4)', () => {
  it('세 분류를 모두 싣는다 — finding만 보던 현행을 대체한다', () => {
    const rows = toWidgetRows(
      [finding({ dedup_key: 'F1', first_seen: '2026-08-02T00:00:00Z' })],
      [
        tip('N1', '2026-08-03T00:00:00Z', { trigger_tags: ['changelog'], dimension: null, title: '릴리스 노트' }),
        tip('L1', '2026-08-01T00:00:00Z', { title: 'hooks 쓰기' }),
      ],
    );
    expect(rows.map((r) => r.kind)).toEqual(['news', 'coaching', 'learning']);
    expect(rows.map((r) => r.badge)).toEqual(['소식', '코칭', '학습']);
  });

  it('코칭 finding의 한 줄은 처방이다', () => {
    const [r] = toWidgetRows([finding({ suggested_action: '스킬로 묶으세요' })], []);
    expect(r.oneLine).toBe('스킬로 묶으세요');
  });

  it('개인 레슨의 한 줄은 「이렇게」, 없으면 제목', () => {
    const [withAction] = toWidgetRows([], [content({ id: 'L-a', first_seen: '2026-08-01T00:00:00Z' })]);
    expect(withAction.oneLine).toBe('플랜 모드를 쓰세요.');

    const [noAction] = toWidgetRows([], [
      content({ id: 'L-b', body: '마커 없는 본문', title: '레슨 제목', first_seen: '2026-08-01T00:00:00Z' }),
    ]);
    expect(noAction.oneLine).toBe('레슨 제목');
  });

  it('학습·소식의 한 줄은 제목', () => {
    const [r] = toWidgetRows([], [tip('T1', '2026-08-01T00:00:00Z', { title: 'hooks 쓰기' })]);
    expect(r.oneLine).toBe('hooks 쓰기');
  });

  it('키는 접두사 없는 원래 키 — 카드의 data-key와 맞아야 딥링크가 착지한다', () => {
    const rows = toWidgetRows(
      [finding({ dedup_key: 'R6|Windows|ab', first_seen: '2026-08-02T00:00:00Z' })],
      [tip('T1', '2026-08-01T00:00:00Z')],
    );
    expect(rows.map((r) => r.key)).toEqual(['R6|Windows|ab', 'T1']);
  });

  it('기본 상한은 3건', () => {
    const many = Array.from({ length: 5 }, (_, i) => tip(`t-${i}`, `2026-08-0${i + 1}T00:00:00Z`));
    expect(toWidgetRows([], many)).toHaveLength(3);
  });
});
```

import 줄에 `toWidgetRows`를 더한다.

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
npm test -- --run coach-stream
```

Expected: FAIL — `toWidgetRows is not a function`

- [ ] **Step 3: 구현한다**

`coach-stream.ts` 끝에 추가(`CoachKind` 타입 import를 `coach-helpers` import 목록에 더한다):

```ts
// ── 홈 「지금 볼 코칭」 위젯 (§2.4) ─────────────────────────────────────────

export interface CoachWidgetRow {
  /** **접두사 없는** 원래 키 — 카드의 `data-key`와 같아야 딥링크가 그 카드에 착지한다. */
  key: string;
  kind: CoachKind;
  badge: string;
  oneLine: string;
}

/** 스트림 상위 N건을 위젯 행으로. 정렬·상한은 탭과 같은 규칙을 쓴다 — 두 화면이
 * 어긋나면 "홈에 있는데 탭에 없다"가 생긴다. */
export function toWidgetRows(
  findings: CoachFinding[],
  content: ContentItem[],
  limit = 3,
): CoachWidgetRow[] {
  return buildCoachStream(findings, content)
    .slice(0, limit)
    .map((c) =>
      c.kind === 'coaching'
        ? { key: c.log.key, kind: c.kind, badge: c.log.badge, oneLine: c.log.action ?? c.log.title }
        : { key: c.learn.id, kind: c.kind, badge: c.learn.badge, oneLine: c.learn.title },
    );
}
```

- [ ] **Step 4: 테스트 통과 확인**

```powershell
npm test -- --run coach-stream
```

Expected: PASS

- [ ] **Step 5: `SaveTop3`를 고친다**

파일 전체를 다음으로 교체한다:

```svelte
<script lang="ts">
  import type { CoachWidgetRow } from '../coach-stream';
  let { rows, onGoto }: { rows: CoachWidgetRow[]; onGoto: (key: string) => void } = $props();
</script>

<div class="widget">
  <h3>지금 볼 코칭</h3>
  {#if rows.length === 0}
    <p class="empty">지금은 지적할 게 없어요. 완벽해요!</p>
  {:else}
    <ol>
      {#each rows as r (r.key)}
        <li>
          <button class={r.kind} onclick={() => onGoto(r.key)}>
            <span class="badge">{r.badge}</span>
            <span class="action">{r.oneLine}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li button {
    width: 100%; display: flex; align-items: center; gap: 8px; text-align: left;
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-cream); color: var(--ink);
    border-radius: var(--radius-s); padding: 7px 10px;
    /* 탭과 같은 시각 언어 — 좌측 라인 색이 분류를 뜻한다 (§2.4) */
    border-left: 3px solid var(--pastel-mint);
  }
  li button.learning { border-left-color: var(--pastel-lav); }
  li button.news { border-left-color: var(--accent); }
  li button:hover { background: var(--pastel-lav); }
  .badge { color: var(--accent-strong); font-weight: 700; white-space: nowrap; flex: none; }
  .action { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
```

(순위 숫자 `1·2·3`은 분류 배지로 대체한다 — 스트림이 시간순이라 순위에 뜻이 없다.)

- [ ] **Step 6: `HomeTab`이 콘텐츠도 읽게 한다**

import에 `listContent`와 `type ContentItem`을 더하고, 새 import 줄을 추가한다:

```ts
  import { liveContent, localDateString, toWidgetRows } from './coach-stream';
```

`let findings = $state<CoachFinding[]>([]);` 아래에 추가:

```ts
  let tips = $state<ContentItem[]>([]);
  // 수명이 끝난 공지는 위젯에서도 뺀다 (§2.6) — 탭과 어긋나면 "홈에 있는데 탭에 없다"가 생긴다.
  // `new Date()`는 반응성 의존이 아니라 findings·tips가 바뀔 때만 다시 잰다. 공지 수명은
  // 일 단위라 그 정밀도면 충분하고, 홈은 스캔·큐레이션마다 load()로 갱신된다.
  const coachRows = $derived.by(() => {
    const now = new Date();
    return toWidgetRows(findings, liveContent(tips, localDateString(now), now.getTime()));
  });
```

`load()`의 `Promise.all` 배열에 `listContent(false).catch(() => [] as ContentItem[]),`를 더하고 구조 분해와 대입을 맞춘다:

```ts
  async function load() {
    const [d, f, c, settings] = await Promise.all([
      getWeekSummary().catch(() => [] as DayStat[]),
      listFindings(false).catch(() => [] as CoachFinding[]),
      listContent(false).catch(() => [] as ContentItem[]),
      getSettings().catch(() => ({}) as Record<string, string>),
    ]);
    days = d; findings = f; tips = c;
    honorific = settings['owner_title']?.trim() || '주인';
    notices = loadNotices();
  }
```

마크업의 위젯 호출을 바꾼다:

```svelte
    <SaveTop3 rows={coachRows} onGoto={onGotoCoach} />
```

⚠ `topAdvice`(마스코트 대사용, `findings[0].suggested_action`)는 **건드리지 않는다** — 이 PR의 범위 밖이다.

- [ ] **Step 7: 검사 후 커밋**

```powershell
npm test
```

Expected: PASS, `Tests  N passed` (N ≥ 296 + 신규분).

```powershell
git add a-mate/src/lib/ui/coach-stream.ts a-mate/src/lib/ui/coach-stream.test.ts a-mate/src/lib/ui/home/SaveTop3.svelte a-mate/src/lib/ui/HomeTab.svelte
git commit -m "feat(agent): show all three kinds in the home coach widget"
```

---

### Task 10: 마무리 — 전체 검증과 DoD 아카이브

**Files:**
- Move: `docs/design/a-mate/specs/2026-08-02-coaching-tab-single-stream-design.md` → 아카이브
- Move: `docs/design/a-mate/plans/2026-08-03-coaching-tab-single-stream-b.md` → 아카이브

- [ ] **Step 1: 전체 테스트**

```powershell
npm test
cargo test
```

Expected: 프론트 `Tests  N passed` (N ≥ 296), cargo `713 passed; 0 failed`. **Rust는 건드리지 않았으므로 713에서 변하면 안 된다.**

- [ ] **Step 2: 실환경 확인**

```powershell
npm run tauri dev
```

눈으로 확인할 것 — 자동 검증이 불가능한 부분이다:

1. 코칭 탭에 섹션 헤더가 없고 카드가 시간순 한 줄로 깔린다
2. 좌측 라인 색이 분류마다 다르다 (코칭=민트 / 학습=라벤더 / 소식=액센트)
3. 커리큘럼 팁에 축 라벨 칩(「워크플로 자동화」 등)이 보인다
4. 고정 슬롯이 있으면 스트림 **위**에, 처분 줄이 있으면 **아래**에 있다
5. 탭을 열면 코칭 배지가 사라진다 (처분하지 않아도)
6. 홈 위젯 행을 누르면 해당 카드로 스크롤한다 — **세 분류 모두**
7. **공지 수명(§2.6)** — 개발 PC의 실제 데이터로 확인할 수 있다. 세 공지 모두 `first_seen`이 `2026-08-03T06:45Z`이고 `deadline`이 `null`이므로:
   - 장애 공지(`cc-announce-emergency-…`, 「공지」 칩)는 **2026-08-05 06:45 이후** 사라진다
   - 나머지 둘은 **2026-08-10 06:45 이후** 사라진다
   - 그 전에 확인하려면 시스템 시계를 옮기지 말고 DB에서 `first_seen`을 과거로 바꿔 본다:
     `UPDATE content_items SET first_seen='2026-07-01T00:00:00+00:00' WHERE id LIKE 'cc-announce-%'`
     (⚠ `content_first_seen` 보존 테이블이 조회에서 우선하므로 **그쪽도 같이** 바꿔야 한다 — §3.2의 `COALESCE(fs.ts, c.first_seen)`)

- [ ] **Step 3: 스펙과 계획을 아카이브한다 (ADR 0013)**

`docs-archive` 스킬을 실행한다. 대상은 **둘 다**다 — B가 6-PR + A·B 재설계의 마지막이라 스펙도 수명이 끝난다.

아카이브 시 대체 관계를 적는다: 이 스펙이 구현되면서 아카이브된 unification 스펙의 **§3·§4.2**를 대체했다.

- [ ] **Step 4: 커밋**

```powershell
git add docs
git commit -m "docs(archive): retire the single-stream spec and plan"
```

- [ ] **Step 5: PR을 올린다**

⚠ **이 브랜치는 #163 위에 쌓여 있다.** PR 본문에 그 사실과, #163이 먼저 머지되면 diff에서 스펙 변경분이 줄어든다는 점을 적는다.

⚠ 이 저장소는 세션이 병렬로 돈다 — **내 PR을 내가 머지하지 않는다.** 후속 커밋을 push하기 전에 `gh pr view <n> --json state,mergedAt`로 머지 여부를 먼저 확인한다.

제목·본문은 한국어. 본문 끝에 `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

---

## B 다음에 남는 것 (이 PR 범위 밖)

- **③·⑥ GUI 실환경 스모크** (번역 스텝·고정 슬롯·말풍선·알림 로그) — 미실시. B가 UI를 갈아엎으므로 B 뒤에 한 번에 하는 편이 낫다.
- **⑤(#151) 실환경 스모크** — life 서버 재배포 선행.
