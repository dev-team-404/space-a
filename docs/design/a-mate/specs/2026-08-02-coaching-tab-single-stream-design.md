# 코칭 탭 단일 스트림 재설계 — 확정된 결정과 착수 전 확인 사항

> **상태: 착수 가능 (⑥ 머지 직후).** 선행이던 PR③(처분·수명 모델, #155)은 머지됐고
> PR⑥(소식 파이프라인, #156)이 이 문서와 같은 PR에 있다.
> 이 문서는 구현 계획이 아니다 — 2026-08-02 세션에서 **확정한 결정과 그때 실측한 코드베이스 사실**을
> 기록해 재조사를 막는 것이 목적이다. 착수 시 이 문서를 근거로 `writing-plans`로 계획을 쓴다.
>
> **2026-08-03 갱신**: §3의 실측은 `dd6a...`(#152) 기준이라 ③·⑥이 바꾼 것들이 낡아 있었다.
> 바뀐 항목마다 어느 PR이 바꿨는지 표시해 두었다 — 표시가 없는 항목은 재검증 후에도 유효하다.

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
  - **⑥ 이후 문법 B의 슬롯이 늘었다**(unification 스펙 §4.2 표에는 없는 것들): ①📊 근거 줄(`personal`, 문법 A와 같은 슬롯) ②기한 칩 `⏳ MM-DD까지` ③고정 슬롯의 `[확인]` 버튼. ①은 배움 카드에서 개인화가 사라지는 것을 막으려고 넣었고 — 두 문법이 "근거 → 내용"으로 수렴한다 — ②③은 §6.4 고정 슬롯 전용이다. 단일 스트림에서 문법 B를 다시 정리할 때 이 세 슬롯을 슬롯 표에 반영할 것.
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

`store.rs` `replace_content_items` 끝. **③·⑤로 조건이 바뀌었지만 결론은 그대로다** — 이제 SQL 한 방이 아니라 Rust에서 걸러 개별 DELETE 한다:

```
SELECT id, trigger_tags FROM content_items WHERE status IN ('new','resolved')
  → 이번 랭킹에 있으면 보존
  → TTL로 스킵한 소스의 태그를 가지면 보존(⑤)
  → 나머지는 DELETE   ('resolved'는 ③이 추가 — 미방출 레슨 정리)
```

랭킹에서 빠진 `new` 행은 **삭제된다.** 다시 랭킹에 들면 `INSERT`가 새 `first_seen`을 찍는다 → 어제 본 카드가 오늘 신규처럼 맨 위로 튄다. 순수 최신순 정렬이 이 흔들림을 **카드 순서가 제멋대로 튀는 현상**으로 증폭시킨다. ③이 `resolved`를 대상에 더해 **삭제 범위는 오히려 넓어졌다.**

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

### 3.4 ~~`content_items.status`의 `'shown'`은 죽은 어휘다~~ → ③에서 제거됨

`valid_content_status`는 이제 `"new" | "resolved" | "dismissed"`다(`'shown'` 삭제, `'resolved'` 추가).
설정하는 코드가 없는 데다 되돌릴 UI 없이 영구 숨김이 가능해 D4와 같은 함정이라 ③이 어휘에서 뺐다.

→ 배지를 「안 본 개수」로 만들 때 §2.3의 localStorage 방식을 그대로 쓴다 — 살릴 값이 애초에 없다.

### 3.5 현행 배지·홈 위젯은 finding만 본다

| 위치 | 현행 |
|------|------|
| `App.svelte` | `activeCount = (await listFindings(false)).length` — 콘텐츠 미포함 |
| `App.svelte` 탭 렌더 | `{#if t.id === 'coach' && activeCount > 0}` |
| `home/SaveTop3.svelte` | `findings.slice(0, 3)` + `f.suggested_action` 렌더. `ContentItem`엔 `suggested_action`이 없다 → §2.4의 `oneLine` 필요 |

### 3.6 ~~스크롤 포커스에 `data-key`가 필요하다~~ → ⑥에서 추가됨

홈 위젯 클릭 시 해당 카드로 스크롤하는 경로는 `CoachTab`의 `focusKey` → `document.querySelector('[data-key=…]')`다. `LogCard`·`LearnCard` **양쪽에 `data-key`가 있다** — ⑥이 새 공지 알림의 딥링크를 위해 `LearnCard`에 붙였고, 같은 이유로 `focusKey` 대기 조건도 `all.length === 0 && tips.length === 0`으로 완화했다(finding이 0건이어도 소식 카드로 착지해야 한다).

### 3.7 콘텐츠 카드 상한은 프론트에만 있다 (⑥에서 4 → **6**)

`store.list_content`에는 **LIMIT이 없다**(`status='new' AND score >= 0`인 행 전부). 상한은 `coach-helpers.ts`의 `CONTENT_CARD_LIMIT`가 `partitionCoachItems`에서 가르기 전에 적용한다 — 원래 삭제된 `TipCard`의 `items[0] + items.slice(1, 4)`를 이어받은 4였는데, ⑥이 소스(로컬 공지)를 하나 더하면서 **6으로 올렸다**(4를 두면 공지 2건이 배움 카드를 전부 밀어낸다).

**고정 슬롯(최대 1건)은 이 상한 밖의 별도 칸**이다 — `CoachTab`이 고정된 항목을 빼고 `partitionCoachItems`에 넘긴다. 자르는 지점은 여전히 한 곳뿐이다.

단일 스트림에서도 **이 상한을 유지할지 재검토할 것.** 늘리려면 백엔드 LIMIT과 겹치지 않게 한 곳에서만 자른다.

### 3.8 `enrich_personal`은 태그로 붙어 레슨 본문과 어긋날 수 있다

`store.rs` `tip_personal_evidence`는 `trigger_tags`로 분기하고, `has("subagent")` 분기는 **카운트가 0이어도 항상 값을 반환**한다. `lesson-cache`의 태그가 `["context", "subagent", "personal"]`이라 캐시 재읽기 레슨에 서브에이전트 수치가 달린다.

→ PR②는 `personal`로 본문 근거를 **대체하지 않고 둘 다 싣는** 쪽으로 갔다(`toLessonCardView`). 근거 슬롯을 손댈 때 이 결정을 되돌리지 말 것.

---

## 4. 미결 질문

| # | 질문 | 비고 |
|---|------|------|
| 1 | 커리큘럼 팁의 사다리 축 라벨(`DIM_LABEL`: 「워크플로 자동화」 등)을 버릴까, 우측 보조 칩으로 남길까? | 배지가 분류 이름으로 고정되면 축 라벨이 갈 곳이 없다. Boris·팀은 본문 끝에 출처가 이미 있어 괜찮지만 축 라벨은 본문에 없어 완전히 사라진다. **⑥ 이후 배지가 하나 늘었다** — 긴급 팁(`lastShownEmergencyTip`)은 「공지」, 나머지 로컬 공지는 「소식」이다(unification 스펙 §6.1). 3분류로 고정하면 「공지」도 갈 곳이 없어지므로 축 라벨과 함께 판단할 것 |
| 2 | §3.7의 카드 상한을 단일 스트림에서도 유지할까? | **전제가 바뀌었다** — ⑥이 4 → 6으로 올렸고, 고정 슬롯 1건은 상한 밖이다 |
| 3 | ~~③이 확정할 처분 카드 표시를 단일 스트림에서 어디에 둘까?~~ | **③ 머지로 확정**: 토글 없이 **섹션 하단에 톤다운 접힌 줄**, 해결함 `✔`(7일)·무시 `◷`(영구), 각 줄에 `[실행취소]`. 섹션이 사라지면 이 줄들을 스트림 어디에 둘지가 남은 질문이다 |
| 4 | 고정 슬롯(⑥ §6.4)을 단일 스트림에서 어떻게 다룰까? | 「전체 `first_seen` 내림차순 한 줄」(§2.2)과 「최상단 고정 1건」이 정면으로 부딪힌다. 스트림 위의 별도 칸으로 남길지, 정렬에 편입할지 정해야 한다 |

---

## 5. PR 분할

| PR | 내용 | 의존 |
|----|------|------|
| **A (Rust)** | `first_seen` serde 노출 + `content_first_seen` 보존 테이블 | ③ 머지 완료 · ⑥(#156) 머지 후 착수 |
| **B (프론트)** | 단일 스트림·3분류·라인 색·탭 배지·홈 위젯 | A |

**~~C — 홈 알림 박스 타임스탬프~~ → ⑥(#156)에서 완료.**
`noticeStamp(ts, now)`가 `notices.ts`의 순수 함수로 들어갔고 단위 테스트가 붙었다(오늘=`HH:MM`, 과거=`MM-DD`, `title`에 전체). **`now`를 인자로 받는 것이 핵심** — 함수 안에서 `new Date()`를 만들면 자정 근처에서 테스트가 간헐 실패한다. `NoticeLog`는 그 시계를 `$state`로 들고 분 단위로 날짜 변화만 확인한다(상주 앱이 자정을 넘겨도 판정이 굳지 않게 — Codex P2).

---

## 6. ③·⑥ 결과 — 둘 다 완료

- **③(#155, 머지)**: 처분·수명 **모델**에 집중했다 — 재발은 `status_evidence_n` **초과** 판정, 레슨은 방출 기반 정리(프룬에 `resolved` 추가), `content_items.status_ts` 신설(`last_seen`은 재방출마다 갱신돼 7일 창이 안 닫힌다), `'shown'` 어휘 제거. 표시는 「섹션 하단 톤다운 줄」로 들어갔고 그 **위치**는 §4 질문 3으로 남았다.
- **⑥(#156)**: 로컬 공지가 「소식」으로 들어간다. 새로 생긴 것 중 이 재설계가 흡수해야 할 것 — 배지 「공지」(§4 질문 1), 고정 슬롯(§4 질문 4), 문법 B의 늘어난 슬롯 3종(§2.1), 상한 6(§3.7). `content_items`에 `summary_ko`·`title_ko`·`deadline`이 생겼고 **번역 캐시는 원문이 바뀌면 버려진다** — A가 `content_first_seen` 보존 테이블을 만들 때 같은 함정(id 고정·내용 갱신 소스)을 확인할 것.
