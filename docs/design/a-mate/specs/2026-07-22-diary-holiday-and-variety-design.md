# 다이어리 공휴일 인식 + 다양성 개선 설계

- **컴포넌트**: a-mate (Agent Mentor) — `crates/core/src/diary`
- **작성일**: 2026-07-22
- **상태**: 설계 확정 (브레인스토밍 승인 완료)
- **관련 작업**: PR #88 "work_log 프로젝트 스코핑"과 별개

## 1. 배경 & 문제

마스코트 일기 엔진은 그날의 Claude Code 사용 기록을 분석해 1인칭 일기를 생성한다. 현재 두 가지 결함이 있다.

1. **한국 법정공휴일을 전혀 모른다.** "쉬는 날 판정"이 `is_weekend`(토/일)만 보므로,
   공휴일 무활동일을 "평일인데 일 없는 날"(idle)로 오판한다.
   - 확정 사례: **제헌절(7/17)이 2026년부터 법정공휴일로 재지정** → `2026-07-17`은 빨간날인데
     앱은 평일 무활동으로 처리한다. 이번 작업이 고쳐야 할 회귀 기준점.
2. **주말·공휴일·무활동(idle) 일기가 단조롭고 반복적이다.** 특히 무활동일 일기는
   반복 방지 컨텍스트가 아예 없고 소재 팔레트가 좁아('옆 동네 봇이랑 산책') 매번 비슷하다.

### 현황 (조사 결과)

| 요소 | 위치 | 현재 동작 |
|------|------|-----------|
| `Occasion{category, label}` | `occasions.rs` | 일기에 **언급할** 기념일 (쉬는 날 판정 아님) |
| `SOLAR_HOLIDAYS` | `occasions.rs` | 서양·개발 기념일만 (새해·발렌타인·핼러윈·크리스마스·연말·만우절·화이트데이·Pi Day·Star Wars Day) |
| `LUNAR_HOLIDAYS` | `occasions.rs` | 설날·추석만, 2025–2035 양력 하드코딩. 동아시아 로케일 언급용 |
| 한국 법정공휴일 | — | **전무** (삼일절·어린이날·부처님오신날·현충일·광복절·개천절·한글날·제헌절·명절 연휴·대체공휴일) |
| `WorkContext{is_weekend, active_hours, long_work}` | `mod.rs` | 쉬는 날 판정 = `is_weekend`만 |
| `build_system_prompt` | `mod.rs` | 위로·응원은 `long_work` 또는 `is_weekend`일 때만 |
| `IdleContext{date, is_weekend, days_idle, occasions}` | `mod.rs` | 무활동일용. **반복 방지 컨텍스트 없음** |
| `build_idle_prompt` | `mod.rs` | 상상 일기. 좁은 소재 메뉴 |
| `collect_recent_diaries` | `mod.rs` | 직전 3 달력일(`RECENT_DIARY_LOOKBACK=3`)만 → 주말·연간 반복엔 부족 |

## 2. 목표 & 비목표

**목표**
1. 한국 법정공휴일(연휴·대체공휴일 포함)을 인식해 **쉬는 날 판정**에 반영하고 일기에 **언급**한다.
   - 공휴일 무활동 → 주말처럼 쉬는 날로 처리(오판 제거).
   - 공휴일 근무 → 위로 트리거.
2. 주말·공휴일·무활동 일기의 단조로움을 줄인다.

**비목표 (YAGNI)**
- 대체공휴일 **규칙 엔진** 미구현 — 현행 규칙이 2026–2027에 실제 만들어내는 날만 열거.
- 10년치 테이블 미작성 — **2년(2026–2027)**이면 충분(사용자 결정).
- DB **스키마 변경 없음** — 과거 날짜의 idle 여부는 기존 `daily_rollup` 조회로 해결.
- 외부 공휴일 API 미사용 — 로컬·프라이버시·사외망 제약.

## 3. 확정 결정 (브레인스토밍)

| # | 결정 | 값 |
|---|------|-----|
| Q1 | 공휴일 처리 범위 | **(a)** 법정공휴일을 쉬는 날 판정에 반영 + 언급 |
| Q2 | 데이터 소스·범위 | 정적 하드코딩 테이블, **2026–2027**, 연휴·대체공휴일 포함. 갱신 도구 불필요 |
| Q3 | 단조로움 개선 | **맥락 인지 반복방지 + 소재 팔레트 확장** (조합) |
| Q4 | 신호 연결 | `is_holiday` 판정 신호 추가 + 언급은 `occasions` 재사용 + **mood 태그로 톤 통제** |

### 핵심 원칙 — 두 채널 분리

| 채널 | 역할 | 의미/톤 의존 | 담당 |
|------|------|:---:|------|
| **판정** (rest-day) | 공휴일 무활동=쉬는 날, 공휴일 근무=위로 | 불필요 (boolean) | `WorkContext.is_holiday`, `IdleContext.is_holiday` |
| **언급** (mention) | 공휴일을 다정하게 언급 | 필요 → **우리가 mood로 톤 통제** | `Occasion.mood` |

판정은 이름·의미가 필요 없어 **모델 독립**. 언급의 톤만 우리가 데이터로 고정하고(예: 현충일=추모),
문구는 모델이 가볍게. 이로써 "무슨 날인지"의 정의가 모델 학습지식에 좌우되지 않는다.

## 4. 데이터 설계 (`occasions.rs`)

### 4.1 한국 공휴일 테이블 (2026–2027, `ko` 로케일 전용)

현행 대체공휴일 규칙(3·1절·어린이날·부처님오신날·광복절·개천절·한글날·성탄절은 토/일 겹칠 때,
설·추석 연휴는 일요일·중복 시 → 다음 비공휴일 평일로 대체. 신정·현충일·제헌절은 대체 비대상)을
2026–2027 실제 요일에 적용해 결정론적으로 열거한다. **아래 목록은 2026-07-22 사용자 확인 완료**
(사외망이라 공식 달력 대조 불가 → 사용자가 권위 있는 검증자).

| 날짜 | label | mood |
|------|-------|------|
| 2026-01-01 | 신정 | festive |
| 2026-02-16 | 설날 연휴 | family |
| 2026-02-17 | 설날 | family |
| 2026-02-18 | 설날 연휴 | family |
| 2026-03-01 | 삼일절 | national |
| 2026-03-02 | 삼일절 대체공휴일 | substitute |
| 2026-05-05 | 어린이날 | festive |
| 2026-05-24 | 부처님 오신 날 | festive |
| 2026-05-25 | 부처님 오신 날 대체공휴일 | substitute |
| 2026-06-06 | 현충일 | solemn |
| 2026-07-17 | 제헌절 | national |
| 2026-08-15 | 광복절 | national |
| 2026-08-17 | 광복절 대체공휴일 | substitute |
| 2026-09-24 | 추석 연휴 | family |
| 2026-09-25 | 추석 | family |
| 2026-09-26 | 추석 연휴 | family |
| 2026-10-03 | 개천절 | national |
| 2026-10-05 | 개천절 대체공휴일 | substitute |
| 2026-10-09 | 한글날 | national |
| 2026-12-25 | 성탄절 | festive |
| 2027-01-01 | 신정 | festive |
| 2027-02-05 | 설날 연휴 | family |
| 2027-02-06 | 설날 | family |
| 2027-02-07 | 설날 연휴 | family |
| 2027-02-08 | 설날 대체공휴일 | substitute |
| 2027-03-01 | 삼일절 | national |
| 2027-05-05 | 어린이날 | festive |
| 2027-05-13 | 부처님 오신 날 | festive |
| 2027-06-06 | 현충일 | solemn |
| 2027-07-17 | 제헌절 | national |
| 2027-08-15 | 광복절 | national |
| 2027-08-16 | 광복절 대체공휴일 | substitute |
| 2027-09-14 | 추석 연휴 | family |
| 2027-09-15 | 추석 | family |
| 2027-09-16 | 추석 연휴 | family |
| 2027-10-03 | 개천절 | national |
| 2027-10-04 | 개천절 대체공휴일 | substitute |
| 2027-10-09 | 한글날 | national |
| 2027-10-11 | 한글날 대체공휴일 | substitute |
| 2027-12-25 | 성탄절 | festive |
| 2027-12-27 | 성탄절 대체공휴일 | substitute |

### 4.2 mood 카테고리 → 언급 톤 매핑

| mood | 대상 | 언급 톤 |
|------|------|--------|
| `festive` | 신정·어린이날·성탄절·부처님오신날 | 가볍고 즐겁게 |
| `national` | 삼일절·광복절·개천절·한글날·제헌절 | 담백한 자긍심 (파티 아님) |
| `solemn` | 현충일 | 조용·담백, 능청 자제, 추모 |
| `family` | 설날·추석 (연휴 포함) | 따뜻한 명절 분위기 |
| `substitute` | 대체공휴일 | "○○ 대체공휴일, 하루 더 쉬는 날" |

### 4.3 Rust 구조 (구현 지침)

```rust
#[derive(Clone, Copy)]
enum HolidayMood { Festive, National, Solemn, Family, Substitute }
impl HolidayMood { fn as_str(self) -> &'static str { /* "festive" 등 */ } }

// (year, month, day, label, mood). 2026-07-22 사용자 확인. 이후 연도는 조용히 생략(패닉 없음).
const KR_HOLIDAYS: &[(i32, u32, u32, &str, HolidayMood)] = &[ /* §4.1 그대로 */ ];

pub struct KrHoliday { pub label: String, pub mood: &'static str }

/// ko 로케일에서만 Some. 그 외 로케일·미등록 날짜는 None.
pub fn korean_public_holiday(date: NaiveDate, locale: &str) -> Option<KrHoliday>;
```

### 4.4 `compute_occasions` 통합 & 중복 제거

- `ko`면 `KR_HOLIDAYS`의 그날 항목을 `occasions`에 추가한다(`category:"holiday"`, `mood:Some(..)`).
- **`LUNAR_HOLIDAYS`는 손대지 않는다** → ko/zh/ja 설·추석 언급은 2035까지 그대로 유지(언급 회귀 없음).
- **중복 제거**: 2026–2027 ko에서 설·추석 당일은 KR 테이블과 LUNAR 양쪽에 같은 label로 잡힌다.
  `category=="holiday"` 항목을 label로 dedupe하되, **mood를 가진 KR 엔트리를 우선** 남긴다.
- 결과:
  - 2026–2027 ko: 설·추석은 KR(mood=family) + 전 법정공휴일·대체공휴일, `is_holiday` 작동.
  - 2028+ ko: 설·추석은 LUNAR 언급만(mood 없음 → 기본 가벼운 톤), `is_holiday` 미작동(주말만).
    graceful degradation — 기존 대비 회귀 없음.

## 5. 자료구조 변경 (`mod.rs`)

```
Occasion       += mood: Option<String>
                  #[serde(skip_serializing_if = "Option::is_none")]  // 재미 기념일 JSON 불변
WorkContext    += is_holiday: bool          // is_weekend와 대칭 (쉬는 날 판정)
IdleContext    += is_holiday: bool
IdleContext    += recent_diaries: Vec<RecentDiary>   // 현재 idle은 반복 방지 컨텍스트가 0
```

- `is_holiday`에는 이름이 필요 없다 — 언급은 `occasions`가 담당하므로 판정용 boolean만.
- 기존 `Occasion { category, label }` 생성부(SOLAR/LUNAR/milestone/Programmer's Day)는 `mood: None` 추가.
- 기존 `IdleContext` 리터럴(테스트 포함)에 새 필드 추가.

## 6. 데이터 흐름

```
assemble_brief(cfg, date):
  locale = resolve_locale(cfg)                 # ko-KR → "ko"
  kr = korean_public_holiday(today, locale)    # ko만 Some
  occasions += kr(label, mood)  → dedupe       # 언급 채널 (§4.4)
  work_context.is_holiday = kr.is_some()       # 판정 채널

  # 반복 방지 (순서 주의 — finding 억제 동작 불변 보장)
  recent_3 = collect_recent_diaries(3일)       # 기존
  recent_keys = ... from recent_3              # 기존 그대로 (finding 억제)
  similar = collect_similar_diaries(kind)      # 신규 (§7a)
  recent_diaries = merge(recent_3, similar) dedup by date, cap

pipeline.maybe_generate_diaries (idle 분기):
  IdleContext {
    is_holiday: brief.work_context.is_holiday,     # 신규
    recent_diaries: brief.recent_diaries.clone(),  # 신규
    occasions: brief.occasions.clone(),            # mood 포함(기존 필드)
    date, days_idle,
  }
```

- `WorkContext.is_holiday`는 `collect_work_context` 반환 후 `assemble_brief`에서 설정
  (locale이 그쪽에 있으므로 시그니처 변경 회피).
- `crates/core`가 주 수정처, `pipeline.rs`는 **필드 배선만**.

## 7. 다양성 개선

### 7a. 맥락 인지 반복 방지 (same-kind)

현재 `recent_diaries`는 직전 3 달력일만 → 토요일 일기는 평일 3일만 보고 지난 토요일을 못 본다.
무활동일은 참조가 아예 0.

- `collect_similar_diaries(store, host, today, locale, kind)` 추가: 오늘이 idle/주말/공휴일이면
  최대 `SIMILAR_LOOKBACK`(28)일 뒤로 스캔해 **같은 성격 일기 최대 `SIMILAR_MAX`(2)편**을 수집.
  - 과거 날짜 kind 판정: 주말(날짜 계산)·공휴일(`korean_public_holiday`)·idle(`daily_rollup.session_count==0` 조회).
  - kind 매칭: 오늘 idle → 과거 idle / 오늘 주말·공휴일(활동) → 과거 주말·공휴일.
  - 평일 활동일은 same-kind 보강 없음(3일 창으로 충분, 단조로움 문제 아님).
- `recent_keys`(finding 억제)는 **3일 창에서만** 산출 → 억제 동작 불변. same-kind는 서사 반복
  방지에만 병합(§6 순서).

### 7b. 소재 팔레트 확장 + 회전

- `build_idle_prompt`의 좁은 메뉴('옆 동네 봇 산책')를 범주화된 팔레트로 확장:
  이웃 에이전트 교류 / 혼자만의 취미 / 계절·날씨 상상 / 명절·기념일 특별활동 / 주인 안부.
- 생성 대상 `date`의 day-of-year로 **결정적 spotlight**(매번 다른 2~3개 범주 부각).
  `Date::now`/난수 없이 날짜만으로 회전 → 재현성 유지. 이를 위해 `build_idle_prompt`에 date(또는 day-of-year) 전달.
- 무활동일 공휴일은 mood에 맞춰(현충일=조용, 설날=송편) 프레이밍.

## 8. 프롬프트 변경

### 8a. 활동일 `build_system_prompt`

- 위로 트리거를 `long_work || is_weekend || **is_holiday**`로 확장.
  공휴일 근무는 `occasions`로 무슨 날인지 알고, **mood에 맞춰** 위로(현충일이면 능청 대신 담백).
- `occasions` 언급 지침에 **mood 처리 규칙**(§4.2 표) 추가.
- 기존 "발렌타인·파이데이는 위로 대상 아님" 유지(그것들은 mood 없음 → 가볍게).
- 시그니처 변경 없음(신호는 `brief` JSON: `work_context.is_holiday`, `occasions[].mood`).

### 8b. 무활동일 `build_idle_prompt`

- **반복 방지 지침 신규**: `recent_diaries`(직전 idle들)에서 이미 쓴 소재 되풀이 금지
  — 현재 idle 프롬프트엔 이 개념 자체가 없다.
- 공휴일 idle은 "주인이 안 왔네" 일반 프레이밍 대신 mood에 맞는 쉬는 날로.
- 소재 팔레트 확장 + 날짜 기반 spotlight(§7b).
- 짧은 출력(1~2문장, 특별한 날 2~3문장) 규칙은 유지.

## 9. 테스트 & 회귀

`cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib` (baseline: 380 passed).

**occasions.rs**
- `korean_public_holiday`: 제헌절 `2026-07-17` → Some(national), 현충일 → solemn, 설날 연휴 3일(ko),
  대체공휴일 샘플(`2026-08-17` 등), 부처님오신날, **비-ko 로케일 → None**, 미등록 연도(2028) → None.
- `compute_occasions`: ko 2026-02-17 설날 mood=family이며 label 중복 없음(dedupe); zh/ja 설·추석 언급 유지.
- 미래 연도 pin(오타 조기 검출): 2027 대체공휴일 2건 이상.

**mod.rs / assemble_brief**
- **회귀 핵심**: ko에서 `2026-07-17` → `work_context.is_holiday == true`
  (idle이 평일 오판이 아니라 쉬는 날로 처리됨).
- `is_holiday` 독립성: 평일 공휴일에 `is_holiday==true && is_weekend==false`.
- same-kind: 과거 idle 일기가 존재하면 새 idle의 `recent_diaries`에 등장.
- `recent_keys`(finding 억제)가 same-kind 병합 후에도 3일 창 기준으로 불변임을 확인.

**프롬프트 문자열 assert** (기존 테스트 패턴 준수)
- `build_system_prompt`: `is_holiday` 위로 + mood 지침 문구 포함.
- `build_idle_prompt`: `recent_diaries` 반복방지 + 팔레트 + mood 문구 포함.

## 10. 범위 / 파일

| 파일 | 변경 |
|------|------|
| `crates/core/src/diary/occasions.rs` | `HolidayMood`, `KR_HOLIDAYS`, `korean_public_holiday`, `compute_occasions` 통합·dedupe, `Occasion.mood` 사용 |
| `crates/core/src/diary/mod.rs` | `Occasion.mood`, `WorkContext.is_holiday`, `IdleContext`(+`is_holiday`,+`recent_diaries`), `assemble_brief` 배선, `collect_similar_diaries`, `build_system_prompt`·`build_idle_prompt` |
| `src-tauri/src/pipeline.rs` | `IdleContext` 필드 배선(3개) |

무거운 로직은 전부 `crates/core`. 스키마 변경 없음. 커밋은 영어 Conventional Commits.

## 11. 검증 노트 (사외망 제약)

§4.1 목록은 현행 규칙 + 요일 계산으로 도출해 **2026-07-22 사용자 확인**을 받았다. 특히:
- 부처님오신날 양력(2026-05-24, 2027-05-13),
- 제헌절 대체공휴일 비대상 가정,
- 각 대체공휴일 발생일.

향후 법·달력 변경 시 이 테이블만 갱신하면 된다(규칙 엔진 없음).
