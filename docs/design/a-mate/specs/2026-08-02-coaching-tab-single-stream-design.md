# 코칭 탭 단일 스트림 재설계 — 확정된 결정과 착수 전 확인 사항

> **상태: A 완료 — 남은 것은 B 하나 (2026-08-03).** 선행 PR③(#155)·PR⑥(#156)에 이어 **A(Rust, `first_seen`)도 [#161](https://github.com/dev-team-404/space-a/pull/161)로 머지**됐다(`3a6981a`).
> ③이 남기고 간 결과는 §3.4·§4 질문 3·§6에, **⑥이 남기고 간 결과는 §2.1·§3.2·§3.6·§3.7·§4 질문 1·2·4·§5**에,
> **A가 남기고 간 결과는 §3.1·§3.2·§3.3·§4 질문 5·§5**에 반영돼 있다 — **다시 조사하지 말 것.**
> §3의 실측은 원래 `dd4a5bc`(#152) 기준이라 여섯 항목이 낡아 있었고,
> 바뀐 항목마다 어느 PR이 바꿨는지 표시해 두었다(표시가 없으면 재검증 후에도 유효하다).
>
> **B 착수 전에 §4 미결 질문 5개를 먼저 정해야 한다.** 특히 **질문 4·5는 서로 얽혀 있다**(둘 다 "무엇을 「새로 보이는 것」으로 볼지").
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
  - **⑥ 이후 문법 B의 슬롯이 셋 늘었다**(아카이브된 unification 스펙 §4.2 표에는 없다): ①**📊 근거 줄**(`personal` — 문법 A와 같은 슬롯) ②**기한 칩** `⏳ MM-DD까지` ③고정 슬롯의 **`[확인]` 버튼**. ①은 `coachTip`(🤖 맞춤 코칭 한 줄)을 폐지하면서 배움 카드에서 개인화가 통째로 사라지는 것을 막으려고 넣었다 — 두 문법이 "근거 → 내용"으로 수렴한다. ②③은 §6.4 고정 슬롯 전용이다. 단일 스트림에서 문법 B를 다시 정리할 때 이 셋을 슬롯 표에 반영할 것.
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

**⚠ A(#161)에서 드러난 한계 — `first_seen`은 「삽입 시각」이지 「활성화 시각」이 아니다.** 위 표는 여전히 맞지만, 값이 *처음 노출된* 시각이라는 보장은 없다. 카드가 **새로 보이기 시작하는** 두 전이가 이 값을 갱신하지 않는다:

| 전이 | 왜 노출이 늦는가 |
|------|------------------|
| `pending` → `new` | R6·R7 세션 후보는 `upsert_finding`의 `init_status`가 `pending`으로 넣고 **판정을 통과할 때** 비로소 노출된다. 판정 실패는 `attempts < 3`까지 재시도되므로 그 창이 며칠일 수 있다 |
| `resolved` → `new` | 「해결함」 뒤 재발 복귀(§5.1 `status_evidence_n` 초과) — 키도 severity도 그대로다 |

둘 다 `first_seen`은 과거에 머물러 **§2.3의 `first_seen > lastCoachSeenAt` 배지가 그 카드를 놓친다.** ③이 같은 전이를 알림 경로(`diff_findings`→`coach:finding`)에서 살려낸 것과 **같은 실패 모드**다 — 상태 전이는 저장이 아니라 알림 경로에서 샌다. **어떻게 다룰지는 §4 질문 5.**

콘텐츠 쪽의 같은 문제(노출된 적 없는 항목이 시각을 선점)는 **A에서 고쳤다** — §3.2 참조.

### 3.2 ~~콘텐츠 프룬이 `first_seen`을 리셋한다~~ → **A(#161)에서 완료**

`store.rs` `replace_content_items` 끝. **③·⑤로 조건이 바뀌었다** — SQL 한 방이 아니라 Rust에서 걸러 개별 DELETE 한다:

```
SELECT id, trigger_tags FROM content_items WHERE status IN ('new','resolved')
  → 이번 랭킹에 있으면 보존
  → TTL로 스킵한 소스의 태그를 가지면 보존 (⑤)
  → 나머지는 DELETE            ('resolved'는 ③이 추가 — 미방출 레슨 정리)
```

랭킹에서 빠진 행은 **삭제된다.** 다시 랭킹에 들면 `INSERT`가 새 `first_seen`을 찍는다 → 어제 본 카드가 오늘 신규처럼 맨 위로 튄다. 순수 최신순 정렬이 이 흔들림을 **카드 순서가 제멋대로 튀는 현상**으로 증폭시킨다. ③이 `resolved`를 대상에 더해 **삭제 범위는 오히려 넓어졌다.**

**⑥에서 같은 함정을 한 번 밟았다** — `content_items`의 파생 캐시(`summary_ko`·`title_ko`·`deadline`)를 `id`만 보고 보존했더니, `cc-changelog-latest`처럼 **id가 고정이고 내용만 갱신되는 소스**에서 화면이 영영 낡은 채로 굳었다(Codex P1). `content_first_seen`도 `id` 기준 보존 테이블이므로 **"이 id의 항목이 그때 그 항목이 맞는가"를 따져볼 것.** `first_seen`은 "처음 본 시각"이라 내용이 바뀌어도 유지가 맞다고 보이지만, 판단을 명시적으로 남길 것.

**A(#161)가 구현한 것 — 권장안 그대로 별도 테이블:**

```sql
CREATE TABLE IF NOT EXISTS content_first_seen (id TEXT PRIMARY KEY, ts TEXT NOT NULL);
```

- 큐레이션 때 `INSERT OR IGNORE`(`replace_content_items`의 **기존 트랜잭션 안** — 락 3단을 깨지 않고 `pipeline.rs` 호출부도 안 건드렸다), 조회 때 `LEFT JOIN` + `COALESCE(fs.ts, c.first_seen)`.
- **`CONTENT_COLS`의 모든 컬럼과 `ORDER BY`에 `c.` 접두사가 붙었다** — `id`가 양쪽 테이블에 있어 빠뜨리면 `ambiguous column name`으로 **런타임에** 깨진다(컴파일로는 안 잡힌다). 이 상수를 쓰는 쿼리는 셋(`list_content` ×2, `content_needing_translation`)이다.
- 마이그레이션은 **백필뿐**이다. 테이블 자체는 `SCHEMA`의 `CREATE TABLE IF NOT EXISTS`가 만든다(`open`이 매번 `execute_batch(SCHEMA)` → `migrate` 순서). 백필을 빼면 기존 사용자는 **마이그레이션 후 첫 프룬에서** 값을 잃는다 — 그때 `INSERT OR IGNORE`가 찍는 건 원래 시각이 아니라 그 큐레이션의 `now_ts`다.
- `status`에 `'stale'`을 더하는 대안은 예정대로 **피했다**(③이 같은 어휘에 `resolved`를 넣었다).

**id 고정·내용 갱신 소스에 대한 판단(위 ⑥ 경고에 대한 답):** **유지가 맞다.** 번역은 "그 원문의" 번역이라 원문이 바뀌면 버려야 하지만 `first_seen`은 "처음 본 시각"이어서 내용 갱신과 무관하다. **대가: 내용이 새로워진 `cc-changelog-latest`가 최신순 스트림 하단에 남는다** — §4 질문 4(고정 슬롯)와 정면으로 맞물리는 지점이다.

**⚠ A의 Codex 리뷰가 잡은 것 — 노출된 적 없는 항목이 시각을 선점하면 안 된다.** `content.rs`의 `rank()`는 점수로 걸러내지 않아서 `replace_content_items`에는 `list_content`가 `score >= 0`에서 숨기는 항목이 섞여 들어온다(통달 축 `SCORE_SUPPRESS=-1000`, 태그 미스 `SCORE_TAG_MISS=-600`, 프론티어보다 2단 이상 먼 축 `100 - dist*60`). 그런데 **프론티어가 이동하면 같은 팁의 점수가 음수→양수로 바뀐다** — 즉 "한 번도 안 보인 항목이 나중에 자격을 얻는" 경로가 실재한다. 그것까지 시딩하면 자격을 얻는 순간 이미 과거 시각을 들고 있어 하단에 묻히고 배지에도 안 잡힌다.

→ **`score >= 0`인 랭킹 항목만 시딩한다.** *남은 한계*: 축 쿨다운(같은 dimension의 `dismissed` 형제가 14일 이내)으로 숨는 항목은 **다른 행의 status·시각**에 달려 있어 쓰기 시점에 판단할 수 없다. 근본 해결은 조회 시점 시딩인데 그건 읽기 경로에 쓰기를 넣어야 해서 락 규율과 충돌한다.

*수용하는 트레이드오프*: 오래 사라졌다 돌아온 항목도 옛 `first_seen`을 유지해 하단에 묻힌다. 갭 길이로 예외를 두는 건 넣지 않는다(YAGNI).

### 3.3 ~~`first_seen`이 프론트에 노출되지 않는다~~ → **A(#161)에서 Rust 쪽 완료**

`FindingRow`·`ContentRow`에 serde 필드가 들어갔다. `CoachFinding`이 `FindingRow`를 `#[serde(flatten)]`하므로 커맨드 변경 없이 payload에 실린다.

**B가 할 일은 `api.ts` 타입 추가 한 줄씩이다** — `Finding`·`ContentItem` 인터페이스에 `first_seen?: string | null`. A는 Rust만 건드렸으므로 타입 선언은 아직 없다.

**이 한 번의 노출이 정렬과 배지 둘 다를 해결한다** — §2.3의 배지는 `first_seen`만 있으면 프론트만으로 구현된다. 단 **어떤 전이를 「새로 보임」으로 셀지는 §4 질문 5**로 남았다(§3.1 참조).

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

### 3.6 ~~스크롤 포커스에 `data-key`가 필요하다~~ → ⑥에서 추가됨

홈 위젯 클릭 시 해당 카드로 스크롤하는 경로는 `CoachTab`의 `focusKey` → `document.querySelector('[data-key=…]')`다. 이제 `LogCard`·`LearnCard` **양쪽에 있다** — ⑥이 새 공지 알림(§6.5)의 딥링크 때문에 `LearnCard`에 붙였고, 같은 이유로 `focusKey` 대기 조건도 `all.length === 0 && tips.length === 0`으로 완화했다(finding이 0건이어도 소식 카드로 착지해야 한다).

### 3.7 콘텐츠 카드 상한은 프론트에만 있다 (⑥에서 4 → **6**)

`store.list_content`에는 **LIMIT이 없다**(`status='new' AND score >= 0`인 행 전부). 상한은 `coach-helpers.ts`의 `CONTENT_CARD_LIMIT`가 `partitionCoachItems`에서 가르기 전에 적용한다 — 원래 삭제된 `TipCard`의 `items[0] + items.slice(1, 4)`를 이어받은 4였는데, ⑥이 소스(로컬 공지)를 하나 더하면서 **6으로 올렸다**. 4를 두면 공지 2건이 배움 카드를 전부 밀어낸다.

**고정 슬롯(최대 1건)은 이 상한 밖의 별도 칸**이다 — `CoachTab`이 고정된 항목을 빼고 `partitionCoachItems`에 넘긴다. 자르는 지점은 여전히 한 곳뿐이다.

**⑥ 실측(2026-08-03, 실제 `claude.json`)**: 로컬 공지 3건은 `SCORE_ANNOUNCEMENT + priority`로 320·321·325를 받았고, 프론티어 축 팁이 **한꺼번에 500점**이라 상한 6 안에 공지는 1장만 들어왔다. **문제가 아니라고 판단했다** — 처분이 큐를 비운다. 공지를 `✕`로 닫으면 영구히 빠지고(프룬은 `new`·`resolved`만 지운다), 팁을 하나 닫으면 **축 쿨다운 14일**이 걸려 그 축 4~5장이 한꺼번에 사라진다. 단일 스트림에서 상한을 재검토할 때 이 동역학을 전제로 삼을 것.

### 3.8 `enrich_personal`은 태그로 붙어 레슨 본문과 어긋날 수 있다

`store.rs` `tip_personal_evidence`는 `trigger_tags`로 분기하고, `has("subagent")` 분기는 **카운트가 0이어도 항상 값을 반환**한다. `lesson-cache`의 태그가 `["context", "subagent", "personal"]`이라 캐시 재읽기 레슨에 서브에이전트 수치가 달린다.

→ PR②는 `personal`로 본문 근거를 **대체하지 않고 둘 다 싣는** 쪽으로 갔다(`toLessonCardView`). 근거 슬롯을 손댈 때 이 결정을 되돌리지 말 것.

---

## 4. 미결 질문

| # | 질문 | 비고 |
|---|------|------|
| 1 | 커리큘럼 팁의 사다리 축 라벨(`DIM_LABEL`: 「워크플로 자동화」 등)을 버릴까, 우측 보조 칩으로 남길까? | 배지가 분류 이름으로 고정되면 축 라벨이 갈 곳이 없다. Boris·팀은 본문 끝에 출처가 이미 있어 괜찮지만 축 라벨은 본문에 없어 완전히 사라진다. **⑥이 배지를 하나 더 늘렸다** — 긴급 팁(`lastShownEmergencyTip`, `notice` 태그)은 「공지」, 나머지 로컬 공지는 「소식」이다. 3분류로 고정하면 「공지」도 갈 곳이 없으니 축 라벨과 **한 번에** 정할 것 |
| 2 | §3.7의 카드 상한을 단일 스트림에서도 유지할까? | **전제가 바뀌었다** — ⑥이 4 → 6으로 올렸고 고정 슬롯 1건은 상한 밖이다. §3.7의 실측 동역학(처분이 큐를 비운다)을 함께 볼 것 |
| 3 | 처분 줄을 단일 스트림에서 어디에 둘까? | **③(#155) 결과 도착 — 바로 아래 참조.** 판정은 다 끝났고 위치만 남았다 |
| **4** | **고정 슬롯(⑥ §6.4)을 단일 스트림에서 어떻게 다룰까?** | **신설.** 「전체 `first_seen` 내림차순 한 줄」(§2.2)과 「최상단 고정 1건」이 정면으로 부딪힌다. 스트림 위의 별도 칸으로 남길지, 정렬에 편입할지 정해야 한다. 편입하면 **기한 경과 자동 강등이 LLM 오추출의 안전망**이라는 성질(§6.4)을 무엇이 대신할지도 함께 정할 것 |
| **5** | **배지가 `pending`→`new`·재발 복귀를 세야 할까?** | **A(#161) Codex 리뷰에서 신설.** `first_seen`은 삽입 시각이지 활성화 시각이 아니라(§3.1) `first_seen > lastCoachSeenAt` 배지가 두 전이를 놓친다. 세지 않기로 하면 **③이 알림 경로에서 고친 재발 감지가 배지에서 다시 새는 것을 받아들이는 것**이다. 세기로 하면 활성화 시각을 **따로** 실어야 한다 — 이 컬럼을 노출 시점으로 바꾸면 더 이상 "처음 본 시각"이 아니게 되고 `content_first_seen`과 의미가 어긋난다. **질문 4와 함께 볼 것**: 둘 다 "무엇을 「새로 보이는 것」으로 볼지"의 문제다 |

### 질문 3에 대한 ③의 결과 (#155)

③은 **위치를 정하지 않고 판정만** 순수 함수로 확정했다. 섹션이 사라지면 「섹션 하단」 좌표가 무의미해지므로 의도적으로 남긴 몫이다.

**그대로 재사용할 것** (`coach-helpers.ts`, 전부 테스트 있음):

| 심볼 | 하는 일 |
|------|---------|
| `RESOLVED_WINDOW_DAYS = 7` | 「해결함」 복구 창 |
| `disposedLabel(status)` | `✔ 해결함` / `◷ 무시` / 처분이 아니면 `null` |
| `isDisposedVisible(status, statusTs, nowMs)` | 무시=영구, 해결함=7일. **처분 시각을 모르면 보이는 쪽**(실행취소를 뺏지 않는다) |
| `toDisposedRows(findings, content, nowMs)` | 룰 카드+개인 레슨을 `DisposedRow[]`로 병합. 문법 B와 판정 내부 상태(pending·rejected)는 제외 |
| `partitionCoachItems` | 처분 항목을 **카드 상한(§3.7, 현재 6) 적용 전에** 걷어낸다 |

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
| ~~**A (Rust)**~~ | `first_seen` serde 노출 + `content_first_seen` 보존 테이블 | **완료 — #161 머지**(`3a6981a`) |
| **B (프론트)** | 단일 스트림·3분류·라인 색·탭 배지·홈 위젯 + `api.ts` 타입(§3.3) | A 완료 → **§4 미결 질문 5개 해소만 남았다** |

**A가 B에게 남긴 것** (§3.1~§3.3 상세):
- `Finding`·`ContentItem` payload에 `first_seen`이 실린다. `api.ts` 타입 선언은 B가 추가한다.
- 콘텐츠는 프룬·재등장에도 처음 본 시각을 지키고, **노출된 적 없는 항목(`score < 0`)은 시각을 선점하지 않는다.**
- **finding은 그 보정이 없다** — `pending`→`new`, 재발 복귀가 `first_seen`을 과거에 남긴다(질문 5).

**~~C — 홈 알림 박스 타임스탬프~~ → ⑥(#156)에서 완료.**
`noticeStamp(ts, now)`가 `notices.ts`의 순수 함수로 들어갔고 단위 테스트가 붙었다(오늘=`HH:MM`, 과거=`MM-DD`, `title`에 전체).
**`now`를 인자로 받는 게 핵심** — 함수 안에서 `new Date()`를 만들면 자정 근처에서 테스트가 간헐 실패한다.
`NoticeLog`는 그 시계를 `$state`로 들고 분 단위로 **날짜 변화만** 확인한다(상주 앱이 자정을 넘겨도 판정이 굳지 않게 — Codex P2). `CoachTab`의 고정 슬롯 날짜도 같은 패턴이다.

---

## 6. ③·⑥ 계획에 생기는 여파

- **③ — 완료(#155).** 「처분 카드를 섹션 하단 톤다운 줄로 이동」이 계획이었으나 섹션이 사라지므로, ③은 처분·수명 **모델**(재발 감지·`status_evidence_n`·마이그레이션)에 집중하고 표시 위치를 남겨뒀다. 그 결과와 B가 이어받을 심볼·함정은 **§4 「질문 3에 대한 ③의 결과」**에 정리돼 있다.
- **⑥ — 완료(#156).** 로컬 공지가 「소식」 분류로 그대로 들어간다. B가 흡수해야 할 것은 넷이다: 배지 「공지」(§4 질문 1), 고정 슬롯(§4 질문 4), 문법 B의 늘어난 슬롯 3종(§2.1), 상한 6(§3.7).
  - `content_items`에 `summary_ko`·`title_ko`·`deadline`이 생겼고 `ContentItem`으로 노출된다. 카드는 `title_ko ?? title`, `summary_ko ?? body`로 폴백한다 — **엔진 미설정 사용자에게는 영어 원문이 그대로 보인다**(소식은 코칭이 아니라 정보 전달이라 값이 남는다).
  - `coachTip`(🤖 맞춤 코칭 한 줄)·`coach_tip` 커맨드·`coach_prompt`가 **제거**됐다. §6.3 표가 한국어 소스를 「그대로」로 못박아 LLM을 아예 부르지 않는 쪽으로 갔고, 그 자리는 §2.1 ①의 결정론 📊 근거 줄이 대신한다.
