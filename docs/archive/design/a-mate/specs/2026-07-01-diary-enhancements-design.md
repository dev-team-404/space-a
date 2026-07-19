---
status: done
archived: 2026-07-19
---

# Agent Mentor — 다이어리 확장(유머 · 근거 · Occasions) 설계

> **상태**: 브레인스토밍 합의 완료 (2026-07-01)
> **범위**: 이미 구현된 다이어리 파이프라인(`docs/plans/2026-07-01-backend-vertical-slice.md`의 Task 9–11) 위에 얹는 확장. (1) 서사 톤을 더 유머러스하게, (2) "잘한 것/아쉬운 것"을 결정론적 근거·개선방향에 기반해 구체화, (3) **Occasions**(명절·기념일)를 로케일 인지로 계산해 서사에 반영.
> **관계**: 백엔드 세로 슬라이스가 선행 의존. 기존 "결정론적 계산 → 브리프 → LLM 서사" 파이프라인을 확장하며, 정밀도의 선(코칭 §1.4)을 그대로 지킨다.

---

## 0. 한 줄 요약

다이어리 브리프에 **결정론적으로 계산한 occasions(명절·기념일)** 를 추가하고, 각 finding에 **근거 수치 + 개선방향**을 채워, LLM이 더 유머러스하면서도 사실에 근거한 일기를 쓰게 한다. 온라인 캘린더/네트워크 없이 전부 로컬 계산·번들 데이터로.

---

## 1. 확정 결정 로그 (2026-07-01)

1. **Occasions = 결정론적 사실.** 명절/기념일은 규칙 Finding과 동일하게 백엔드가 계산해 브리프에 싣고, LLM은 서사에만 녹인다(수치·날짜 지어내기 금지). "한 번 계산 → 렌더링" 명제 유지.
2. **기념일 기준일(day 0) = 첫 세션 날짜 자동.** DB의 가장 이른 `sessions.first_ts`를 "주인과 처음 만난 날"로 삼는다(별도 설정 불필요, 다마고치 서사에 부합).
3. **마일스톤 = 50일 + 100일 단위 + N주년.** `days ∈ {50} ∪ {100,200,300,…}` 또는 `days % 365 == 0`.
4. **로케일 인지 + 자동 감지.** `DiaryConfig.locale`(BCP-47)이 명절 **적용 집합 + 표시명**을 결정. 기본값은 **OS 로케일 자동 감지**(`sys-locale`), 실패 시 `en`. 사용자가 config로 override 가능.
5. **음력 = 번들 정적 테이블(2025–2035), 로케일별 이름.** 설날/추석(음력 1/1, 8/15)의 양력 날짜를 `const` 테이블로 내장. 표시명은 ko/zh/ja별 룩업. 테이블 소진(2035 이후) 시 음력 명절만 조용히 생략(graceful degrade). 크레이트/네트워크 불필요.
6. **유머는 기본 상향, 톤 A/B/C는 감정 강도 다이얼로 유지.** 새 톤 프리셋을 만들지 않고 페르소나 프롬프트에 위트를 주입(YAGNI).
7. **근거/개선방향은 규칙에서 파생.** 각 finding의 `detail`(근거 수치)·`suggested_action`(개선 방향)은 브리프 조립 시 `rule_id`+evidence+prescription에서 결정론적으로 생성. LLM은 이 문자열의 사실만 인용.
8. **다중언어 서사 출력은 후속 페이즈(Non-goal).** 이번 페이즈의 서사 언어는 한국어(페르소나) 유지. 단, `locale`을 1급 필드로 두어 후속 페이즈에서 서사 언어 지역화가 깔끔히 확장되게 한다(§8).

---

## 2. Occasions 데이터 모델

```
Occasion { category: "holiday" | "milestone", label: String }   // label은 이미 로케일 지역화된 표시 문자열
```

`Brief`에 `occasions: Vec<Occasion>` 필드 추가. 한 날에 여러 occasion 가능(예: 크리스마스 + 함께한 지 300일).

계산 함수(순수, 결정론적):
```
compute_occasions(date: NaiveDate, anchor: Option<NaiveDate>, locale: &str, include_dev_days: bool) -> Vec<Occasion>
```
- **기념일(milestone)**: `anchor`가 있으면 `days = (date - anchor).num_days()`. `days == 50` 또는 (`days >= 100` and `days % 100 == 0`) → "함께한 지 {days}일". `days > 0` and `days % 365 == 0` → "함께한 지 {days/365}주년". (anchor 없거나 days<=0이면 기념일 없음.)
- **명절(holiday)**: §4 catalog에서 오늘 `(month, day)` 또는 음력 테이블 매칭 + 로케일 적용성 필터 → 지역화된 label.

`assemble_brief`가 store에서 anchor(전역 최소 `first_ts`의 날짜)와 locale을 확보해 `compute_occasions`를 호출하고 결과를 브리프에 싣는다.

---

## 3. Locale 처리

- `DiaryConfig.locale: Option<String>`. `None`이면 런타임에 `sys-locale::get_locale()`로 자동 감지, 그것도 실패하면 `"en"`.
- 언어 판정은 **접두어**(첫 `-` 앞)로: `ko` / `zh` / `ja` / 그 외.
- 적용성:
  - **Universal 명절**: 모든 로케일.
  - **EastAsia 명절 + 음력 명절**: 언어 ∈ {ko, zh, ja}.
  - **DevCulture 명절**: `include_dev_days`가 참이면 모든 로케일.
- 표시명: catalog 엔트리의 `names[lang]`, 없으면 영어/기본명으로 폴백.

---

## 4. Holiday Catalog (번들 정적 데이터)

### 4.1 세계 공통 (양력 고정, 모든 로케일)
| 날짜 | ko | en | zh | ja |
|---|---|---|---|---|
| 1/1 | 새해 첫날 | New Year's Day | 元旦 | 元日 |
| 2/14 | 발렌타인데이 | Valentine's Day | 情人节 | バレンタインデー |
| 10/31 | 핼러윈 | Halloween | 万圣夜 | ハロウィン |
| 12/25 | 크리스마스 | Christmas | 圣诞节 | クリスマス |
| 12/31 | 한 해의 마지막 날 | New Year's Eve | 除夕(阳历) | 大晦日 |
| 4/1 | 만우절 | April Fools' Day | 愚人节 | エイプリルフール |

### 4.2 동아시아 양력 (언어 ∈ {ko, zh, ja})
| 날짜 | ko | zh | ja |
|---|---|---|---|
| 3/14 | 화이트데이 | 白色情人节 | ホワイトデー |

### 4.3 개발자 문화 (`include_dev_days` 시, 모든 로케일)
| 날짜 | label(공통, 영어 기반) | 비고 |
|---|---|---|
| 3/14 | Pi Day | 파이데이 |
| 5/4 | Star Wars Day ("May the 4th") | |
| 연 256번째 날 (평년 9/13, 윤년 9/12) | Programmer's Day | |

> 3/14는 화이트데이(동아시아)와 파이데이(개발자)가 겹칠 수 있음 — 둘 다 적용되면 각각 occasion으로 방출(중복 허용).

### 4.4 음력 (번들 테이블, 언어 ∈ {ko, zh, ja})
- **설날(음력 1/1)**: ko=설날 · zh=春节 · ja=旧正月
- **추석(음력 8/15)**: ko=추석 · zh=中秋节 · ja=十五夜(お月見)

테이블 형태(예시, 실제 값은 구현 시 한국 공식 달력에서 채움):
```
const LUNAR_HOLIDAYS: &[(year: i32, month: u32, day: u32, festival: LunarFestival)] = &[
    (2026, 2, 17, LunarNewYear), (2026, 9, 25, MidAutumn),
    (2027, 2,  6, LunarNewYear), (2027, 9, 15, MidAutumn),
    // … 2025–2035
];
enum LunarFestival { LunarNewYear, MidAutumn }
```
`(year, month, day)`가 오늘과 일치하고 언어가 동아시아면, festival+lang으로 지역화 label 방출. 테이블에 없는 연도는 음력 명절 생략.

---

## 5. 브리프/프롬프트 변경 (유머 + 근거/개선방향)

### 5.1 유머 (톤)
`build_system_prompt`의 페르소나에 위트/장난기 지시를 상시 추가("가볍고 유머러스하게, 다마고치풍 능청"). 톤 A/B/C는 감정 강도 다이얼로 유지.

### 5.2 근거/개선방향 (정밀도의 선 유지)
`BriefFinding`에 `detail: String`(근거 수치)과 `suggested_action: String`(개선 방향) 추가. 브리프 조립 시 결정론적으로 생성:
```
finding_advice(rule_id, evidence, prescription) -> (detail, suggested_action)
```
- **R5**: detail = "`{path}`를 {count}회 반복해서 읽음(~{est}토큰)". suggested_action = "한 번만 읽고 그 내용을 기억해 두면 다음엔 아낄 수 있어요".
- **R1**: detail = "MCP 서버 `{server}`가 상주(~{est}토큰)하는데 호출 0회". suggested_action = "안 쓰는 `{server}`를 설정에서 제거하면 매 세션 상주 토큰을 아껴요".
- 알 수 없는 rule_id → detail = evidence 요약, suggested_action = 빈 문자열(서사에서 생략).

프롬프트는 "**잘한 것/아쉬운 것을 각 finding의 detail(근거 수치)과 suggested_action(개선 방향)에 근거해 구체적으로** 서술하되, 브리프에 없는 수치는 지어내지 말라"고 지시.

### 5.3 Occasions 주입
occasions가 비어있지 않으면 프롬프트에 목록 주입 → "오늘은 크리스마스이고, 우리가 함께한 지 300일이에요, 주인!" 식으로 서사 도입/마무리에 자연스럽게.

---

## 6. 파일/구조

- **신규 `src/diary/occasions.rs`**: `Occasion`, `OccasionKind`(또는 category 문자열), `LunarFestival`, catalog `const` 데이터, 음력 테이블, `compute_occasions(...)`. 순수 함수.
- **`src/diary/mod.rs`**: `Brief`에 `occasions` 필드, `BriefFinding`에 `detail`/`suggested_action` 필드; `assemble_brief`가 anchor·locale 확보 후 occasions 계산 + finding advice 생성; `build_system_prompt`에 유머·근거·occasions 지시 추가; `generate_diary`는 변경 최소(브리프가 이미 풍부).
- **`src/diary/advice.rs`**(또는 mod.rs 내 함수): `finding_advice(...)`.
- **`src/store.rs`**: `earliest_session_ts() -> Result<Option<String>>` 추가(전역 `MIN(first_ts)`).
- **`DiaryConfig`**: `locale: Option<String>`, `include_dev_days: bool`(기본 true) 필드 추가.
- **의존성**: `sys-locale`, `chrono`(이미 있음; `NaiveDate` 날짜 연산에 사용).

---

## 7. 테스트 전략

- **occasions (결정론적)**: 고정 날짜/앵커/로케일 입력 → 기대 Occasion 목록.
  - 마일스톤: anchor+50일 → "함께한 지 50일"; anchor+365 → "1주년"; anchor+300 → "함께한 지 300일"; anchor+37 → 없음.
  - 고정 명절: 12/25 → 크리스마스; 로케일 en → "Christmas", ko → "크리스마스".
  - 음력: 테이블의 설날 양력 날짜 → ko "설날"/zh "春节"; 비-동아시아 로케일 → 음력 생략.
  - 겹침: 3/14 + include_dev_days + ko → 화이트데이 & Pi Day 둘 다.
  - 테이블 밖 연도 음력 → 생략(패닉 없음).
- **finding_advice**: R5/R1 evidence → 기대 detail·suggested_action 문자열.
- **brief 조립**: occasions·advice가 브리프에 실리는지(Mock 엔진).
- **프롬프트**: `build_system_prompt`에 유머·근거 지시 포함 확인; occasions 주입 시 프롬프트/유저메시지에 반영 확인.
- **실제 엔진 수동 검증**: 특정 날짜(예: 크리스마스 + N일)로 다이어리 생성 → 유머·근거·occasion이 자연스럽게 반영되는지 육안.

---

## 8. Non-goals · 후속 페이즈

**이번 페이즈에서 안 함:**
- **다중언어 서사 출력(i18n).** 서사 언어는 한국어(페르소나) 유지. 로케일은 명절 적용/표시명까지만 좌우.
- 국가별 국경일/공휴일(광복절·独立記念日 등) 전수 — "세계인 공감" 세트 + 동아시아 + 개발자 문화로 한정.
- 음력 테이블 자동 확장(2035 이후) — 소진 시 생략, 추후 갱신.

**후속 페이즈 (설계 훅):**
- **다중언어 서사**: `build_system_prompt`를 `locale` → 언어별 페르소나/호칭/톤 템플릿으로 확장. `locale`·occasions label이 이미 지역화돼 있어, 페르소나 템플릿과 호칭 지역화만 추가하면 됨. 엔진 정책·정밀도의 선은 불변.
- 음력 명절 확장(단오 등), 국가별 로케일 명절 팩(옵트인).

---

## 9. kickoff/코칭 스펙과의 정합

- 정밀도의 선(코칭 §1.4): occasions·근거·개선방향 모두 **결정론적 사실**; LLM은 서사만. 수치 지어내기 금지 유지.
- 다이어리 flagship(코칭 §7): 톤 프리셋·호칭 설정 그대로, 유머는 페르소나 강화로 흡수. 다마고치풍 "기념일" 서사가 종단 성장 서사(코칭 §7.3)와 결이 맞음.
- 프라이버시(코칭 §6.3): occasions는 전부 로컬 계산·번들 데이터, egress 없음.
