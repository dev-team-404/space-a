# 코칭 탭 단일 스트림 재설계 — 확정된 결정과 착수 전 확인 사항

> **상태: 착수 가능 (2026-08-03).** 선행이던 PR③(처분·수명 모델, #155)·PR⑥(소식 파이프라인, #156)이 **둘 다 머지**됐다.
> ③이 남기고 간 결과는 §3.4·§4 질문 3·§6에 반영돼 있다 — **다시 조사하지 말 것.**
> 이 문서는 구현 계획이 아니다 — 2026-08-02 세션에서 **확정한 결정과 그때 실측한 코드베이스 사실**을
> 기록해 재조사를 막는 것이 목적이다. 착수 시 이 문서를 근거로 `writing-plans`로 계획을 쓴다.

**대체 관계**: 구현되면 [2026-08-02-coaching-tab-unification-design.md](../../../archive/design/a-mate/specs/2026-08-02-coaching-tab-unification-design.md)의 **§3(최종 구조)·§4.2(문법 B 배치)** 를 대체한다. 같은 스펙의 §4.1(문법 A 슬롯)·§5(수명)·§6(소식)은 그대로 유효하다 — ⑥이 6-PR의 마지막이라 그 스펙은 아카이브됐지만 이 문서가 참조하는 절들은 여전히 근거다.

**왜 ③·⑥ 뒤인가**: ③은 처분 카드를 「섹션 하단 톤다운 줄」로 옮기는 설계였는데 이 재설계가 섹션을 없앤다. 먼저 하면 ③의 처분 UI를 두 번 설계하게 된다. ⑥의 로컬 공지는 아래 「소식」 분류로 자연히 편입된다.

---

## 1. 무엇을 바꾸는가

PR②(#152)가 코칭 탭을 근거 출처로 **2단 섹션**(「내 로그에서」/「배움 · 소식」)으로 갈랐다. 이를 **섹션 없는 한 줄 스트림**으로 바꾸고, 분류는 좌측 라인 색과 배지로만 나타낸다.

```
        PR② (현행, main)                    재설계 후
┌────────────────────────┐      ┌────────────────────────┐
│ § 내 로그에서 (5)       │      │ ┃코칭  같은 지시를 3개…  2분 전│
│    [카드][카드][카드]    │      │ ┃소식  Claude Code 새…  1시간 전│
│ ─────────────────────  │      │ ┃학습  포맷·검사는 hooks…  어제│
│ § 배움 · 소식           │      │ ┃코칭  도구 오류 3회 —…    어제│
│    [카드][카드]         │      │                        │
└────────────────────────┘      │ 섹션 헤더 없음          │
                                 └────────────────────────┘
```

**분리 기준이 "근거 출처"에서 "시간"으로 바뀐다.** 근거 출처는 색·배지로 남는다.

---

## 2. 확정된 결정

### 2.1 세 분류

`CoachKind = 'coaching' | 'learning' | 'news'` — 항목에서 순수 함수로 유도한다.

| 분류 | 재료 | 카드 문법 | 좌측 라인 |
|------|------|-----------|-----------|
| **코칭** | 룰 finding(R6·R7·R8) + `personal` 태그 레슨 | A (로그 카드) | `--pastel-mint` |
| **학습** | 커리큘럼 팁(`dimension`) + Boris + 팀 지식 | B (카드뉴스) | `--pastel-lav` |
| **소식** | `changelog`·`kind='news'` + 로컬 공지(⑥) | B (카드뉴스) | `--accent` |

- **카드 문법은 PR②의 2종을 그대로 유지한다.** 코칭 카드는 근거·행동·처분 버튼이 필요하고 소식 카드는 전문 링크가 주 CTA라 담을 것이 근본적으로 다르다. 통합하면 「전문 보기」가 각주 링크와 같은 위계로 내려가 약해진다.
- **배지는 분류 이름 3종으로 고정한다.**
- **라인 색이 분류를 뜻하므로 severity를 색으로 쓸 수 없다.** severity는 이미 아이콘(⚠/💡/ℹ)이 나타내므로 `LogCard`의 `.card.warn`(coral)을 제거한다.

### 2.2 정렬 — 전체 `first_seen` 내림차순

분류와 무관하게 한 줄로 깐다. 행동 가능한 코칭 카드가 소식 아래로 밀릴 수 있음을 **의도적으로 수용한다**(요청 사항).

### 2.3 탭 배지 = 안 본 개수

```
unseen = cards.filter(c => c.firstSeen > lastCoachSeenAt).length
```

`lastCoachSeenAt`은 localStorage 영속. 탭을 열면 현재 시각으로 갱신 → 배지 0. 처분하지 않아도 배지가 사라진다(현행 「미처리 개수」와 의미가 바뀐다). 다이어리·방명록 배지가 이미 쓰는 방식이다.

### 2.4 홈 「지금 볼 코칭」 위젯 — 세 분류 모두

뷰모델에 `kind`와 `oneLine`을 더한다.

| 분류 | `oneLine` |
|------|-----------|
| 코칭 (finding) | `suggested_action` |
| 코칭 (레슨) | `splitLessonBody().action`, 없으면 `title` |
| 학습 · 소식 | `title` |

위젯 행에도 분류 배지·라인 색을 넣어 탭과 같은 시각 언어를 쓴다.

---

## 3. 착수 전 알아야 할 코드베이스 사실

**모두 2026-08-02 `dd4a5bc`(#152 머지) 기준 실측이다.** 줄 번호는 밀릴 수 있으니 심볼로 찾을 것.

### 3.1 `first_seen`은 신뢰할 수 있다 — `last_seen`·`occurrences`와 다르다

스펙 §1.2 D5가 `occurrences`·`last_seen`을 「스캔 지표」라 못 쓴다고 못 박았는데, **`first_seen`은 예외다.** 두 테이블의 `ON CONFLICT`가 `first_seen`을 갱신하지 않기 때문이다.

| 위치 | `ON CONFLICT DO UPDATE SET` | `first_seen` |
|------|------------------------------|--------------|
| `store.rs` `upsert_finding` | `last_seen`, `occurrences+1`, `evidence_json`, `severity`, `prescription_json`, `est_tokens_saved` | **미갱신 → 보존** |
| `store.rs` `replace_content_items` | `score`, `title`, `body`, `source_url`, `trigger_tags`, `last_seen` | **미갱신 → 보존** |

두 테이블 스키마에 `first_seen TEXT`가 이미 있다(`findings`, `content_items`).

### 3.2 그런데 콘텐츠 프룬이 `first_seen`을 리셋한다 — 이게 유일한 진짜 작업

`store.rs` `replace_content_items` 끝:

```sql
DELETE FROM content_items WHERE status='new' AND id NOT IN (…랭킹된 id…)
```

랭킹에서 빠진 `new` 행은 **삭제된다.** 다시 랭킹에 들면 `INSERT`가 새 `first_seen`을 찍는다 → 어제 본 카드가 오늘 신규처럼 맨 위로 튄다. 순수 최신순 정렬이 이 흔들림을 **카드 순서가 제멋대로 튀는 현상**으로 증폭시킨다.

**권장 해법 — 별도 테이블로 격리:**

```sql
CREATE TABLE IF NOT EXISTS content_first_seen (id TEXT PRIMARY KEY, ts TEXT NOT NULL);
```

큐레이션 때 `INSERT OR IGNORE`, 조회 때 `LEFT JOIN`. 행이 지워졌다 돌아와도 원래 자리를 지킨다.

**`content_items.status`에 `'stale'`을 추가하는 방법은 피할 것.** PR③이 같은 status 어휘에 `resolved`를 추가하므로 두 작업이 같은 자리를 건드린다. 별도 테이블이 충돌을 피한다.

*수용하는 트레이드오프*: 오래 사라졌다 돌아온 항목도 옛 `first_seen`을 유지해 하단에 묻힌다. 갭 길이로 예외를 두는 건 넣지 않는다(YAGNI).

### 3.3 `first_seen`이 프론트에 노출되지 않는다

`api.ts`의 `Finding`·`ContentItem` 인터페이스에 `first_seen`이 **없다**(`last_seen`·`occurrences`만 있음). Rust serde 필드 추가가 선행돼야 한다.

**이 한 번의 노출이 정렬과 배지 둘 다를 해결한다** — §2.3의 배지는 `first_seen`만 있으면 프론트만으로 구현된다.

### 3.4 `content_items.status`의 `'shown'`은 죽은 어휘다

`commands.rs` `valid_content_status`가 `"new" | "shown" | "dismissed"`를 허용하지만 **`'shown'`을 설정하는 코드가 어디에도 없다.** 콘텐츠는 사용자가 닫기 전까지 영원히 `'new'`다.

→ 배지를 「안 본 개수」로 만들 때 `'shown'`을 살릴 필요가 없다. §2.3의 localStorage 방식이 더 싸다.

**③(#155)이 처리 완료 — `'shown'`은 어휘에서 제거됐다.** 최종 `valid_content_status`는 `new | resolved | dismissed`이고
`ContentItem['status']` 타입도 같다. 설정하는 코드가 없어 기존 DB에 그 값을 가진 행도 없었고, 데이터 마이그레이션도 없었다.
**B(프론트)는 세 값만 다루면 된다.** (아카이브된 unification 스펙 §5.6 표에는 「유지」로 적혀 있다 — 그쪽이 옛 판단이다.)

### 3.5 현행 배지·홈 위젯은 finding만 본다

| 위치 | 현행 |
|------|------|
| `App.svelte` | `activeCount = (await listFindings(false)).length` — 콘텐츠 미포함 |
| `App.svelte` 탭 렌더 | `{#if t.id === 'coach' && activeCount > 0}` |
| `home/SaveTop3.svelte` | `findings.slice(0, 3)` + `f.suggested_action` 렌더. `ContentItem`엔 `suggested_action`이 없다 → §2.4의 `oneLine` 필요 |

### 3.6 스크롤 포커스에 `data-key`가 필요하다

홈 위젯 클릭 시 해당 카드로 스크롤하는 경로는 `CoachTab`의 `focusKey` → `document.querySelector('[data-key=…]')`다. **`LogCard`엔 `data-key`가 있지만 `LearnCard`엔 없다.** 위젯이 학습·소식 카드도 다루게 되면 `LearnCard`에도 추가해야 한다.

### 3.7 콘텐츠 4건 상한은 프론트에만 있다

`store.list_content`에는 **LIMIT이 없다**(`status='new' AND score >= 0`인 행 전부). 상한은 `coach-helpers.ts`의 `CONTENT_CARD_LIMIT = 4`가 `partitionCoachItems`에서 가르기 전에 적용한다 — 삭제된 `TipCard`의 `items[0] + items.slice(1, 4)`를 이어받은 값이다.

단일 스트림에서도 **이 상한을 유지할지 재검토할 것.** 세 분류가 한 줄에 섞이면 4건은 답답할 수 있다. 늘리려면 백엔드 LIMIT과 겹치지 않게 한 곳에서만 자른다.

### 3.8 `enrich_personal`은 태그로 붙어 레슨 본문과 어긋날 수 있다

`store.rs` `tip_personal_evidence`는 `trigger_tags`로 분기하고, `has("subagent")` 분기는 **카운트가 0이어도 항상 값을 반환**한다. `lesson-cache`의 태그가 `["context", "subagent", "personal"]`이라 캐시 재읽기 레슨에 서브에이전트 수치가 달린다.

→ PR②는 `personal`로 본문 근거를 **대체하지 않고 둘 다 싣는** 쪽으로 갔다(`toLessonCardView`). 근거 슬롯을 손댈 때 이 결정을 되돌리지 말 것.

---

## 4. 미결 질문

| # | 질문 | 비고 |
|---|------|------|
| 1 | 커리큘럼 팁의 사다리 축 라벨(`DIM_LABEL`: 「워크플로 자동화」 등)을 버릴까, 우측 보조 칩으로 남길까? | 배지가 분류 이름으로 고정되면 축 라벨이 갈 곳이 없다. Boris·팀은 본문 끝에 출처가 이미 있어 괜찮지만 축 라벨은 본문에 없어 완전히 사라진다 |
| 2 | §3.7의 4건 상한을 단일 스트림에서도 유지할까? | |
| 3 | 처분 줄을 단일 스트림에서 어디에 둘까? | **③(#155) 결과 도착 — 바로 아래 참조.** 판정은 다 끝났고 위치만 남았다 |

### 질문 3에 대한 ③의 결과 (#155)

③은 **위치를 정하지 않고 판정만** 순수 함수로 확정했다. 섹션이 사라지면 「섹션 하단」 좌표가 무의미해지므로 의도적으로 남긴 몫이다.

**그대로 재사용할 것** (`coach-helpers.ts`, 전부 테스트 있음):

| 심볼 | 하는 일 |
|------|---------|
| `RESOLVED_WINDOW_DAYS = 7` | 「해결함」 복구 창 |
| `disposedLabel(status)` | `✔ 해결함` / `◷ 무시` / 처분이 아니면 `null` |
| `isDisposedVisible(status, statusTs, nowMs)` | 무시=영구, 해결함=7일. **처분 시각을 모르면 보이는 쪽**(실행취소를 뺏지 않는다) |
| `toDisposedRows(findings, content, nowMs)` | 룰 카드+개인 레슨을 `DisposedRow[]`로 병합. 문법 B와 판정 내부 상태(pending·rejected)는 제외 |
| `partitionCoachItems` | 처분 항목을 **4건 상한(§3.7) 적용 전에** 걷어낸다 |

**현행 배치(잠정)**: 「숨긴 항목 N개 보기」 토글을 없애고 탭 맨 아래에 톤다운 한 줄씩 바로 노출한다. B가 정할 것은 **이 줄들을 단일 스트림에 섞을지, 스트림 아래 따로 둘지**뿐이다 — `DisposedRow.source`(`finding`|`lesson`)가 실행취소를 어느 커맨드로 보낼지 이미 담고 있다.

**B가 밟기 쉬운 함정 둘**
- 처분 줄의 재료는 `listContent(**true**)`다. `listContent(false)`는 백엔드가 `status='new'`+점수+축 쿨다운으로 걸러 처분 행이 아예 오지 않는다. ③은 두 호출을 `Promise.all`로 병렬로 쓴다(`tips` / `allTips`).
- 7일 컷오프는 **시계가 흘러야** 닫힌다. `$derived` 안의 `Date.now()`는 반응성 의존이 아니고 파이프라인엔 주기 tick이 없어(`debounce_loop`는 로그 감시 트리거), ③은 1시간 눈금 `$state` 시계를 둔다. 옮길 때 이 시계를 빠뜨리면 만료가 멈춘다.

### §2.3 배지에 관해 ③이 확정한 것 (#155)

`partitionCoachItems`가 처분 항목을 걸러 카드 목록엔 활성만 남는다. 탭 배지(`App.svelte`)와 알림 로그는 여전히 `listFindings(false)`(백엔드 `status='new'`)를 쓴다 — §2.3의 `first_seen > lastCoachSeenAt` 방식으로 바꿀 때 **처분 줄이 `unseen`에 섞이지 않도록** 활성만 세는 성질을 유지할 것.

### §3.5에 더할 것 — 재발은 `coach:finding`에 실린다 (#155)

스캔의 before/after 스냅숏이 `store.finding_severities()`(전체)에서 `store.active_finding_severities()`(`status='new'`만)로 바뀌었다. `diff_findings`는 **새 키 또는 severity 상승**만 fresh로 보는데 재발 복귀(`resolved→new`)는 키도 severity도 그대로라 전체 스냅숏에선 보이지 않았고, 그래서 알림·말풍선이 뜨지 않았다.

**부수 제약**: `pipeline.rs`의 before/`run_rules`/after는 이제 **한 락 블록에 있어야 한다**. 쪼개면 사이에 낀 `set_finding_status`(사용자의 처분·실행취소)가 diff에 섞여 "새로 떴다"고 알린다. 예전 주석은 "쪼개도 안전하다"였는데 그 전제가 깨졌다.

`content:ready`도 **빈 목록일 때까지 무조건 emit**으로 바뀌었다 — 큐레이션은 노출 목록을 바꾸기만 하는 게 아니라 행을 지우기도 한다(미방출 `resolved` 레슨 프룬). 리스너를 늘릴 때 빈 payload를 전제할 것.

---

## 5. PR 분할

| PR | 내용 | 의존 |
|----|------|------|
| **A (Rust)** | `first_seen` serde 노출 + `content_first_seen` 보존 테이블 | ③·⑥ 머지 후 |
| **B (프론트)** | 단일 스트림·3분류·라인 색·탭 배지·홈 위젯 | A |

**C — 홈 알림 박스 타임스탬프 (완전 독립, 언제든)**
`home/NoticeLog.svelte`의 `hhmm()`이 항상 `HH:MM`만 찍어 어제 알림과 오늘 알림이 구분되지 않는다. 오늘=`HH:MM`, 과거=`MM-DD`로 바꾸고 `title` 속성에 날짜+시간 전체를 넣는다. **이 저장소엔 컴포넌트 테스트 라이브러리가 없으므로** 포맷 로직은 `notices.ts`의 순수 함수로 빼고 단위 테스트를 붙인다(PR②가 세운 규약).

---

## 6. ③·⑥ 계획에 생기는 여파

- **③ — 완료(#155).** 「처분 카드를 섹션 하단 톤다운 줄로 이동」이 계획이었으나 섹션이 사라지므로, ③은 처분·수명 **모델**(재발 감지·`status_evidence_n`·마이그레이션)에 집중하고 표시 위치를 남겨뒀다. 그 결과와 B가 이어받을 심볼·함정은 **§4 「질문 3에 대한 ③의 결과」**에 정리돼 있다.
- **⑥ — 완료(#156).** 로컬 공지가 「소식」 분류로 그대로 들어간다. 배지 문자열을 「공지」로 따로 둘지는 §4 질문 1과 함께 정한다.
