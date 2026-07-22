---
status: done
archived: 2026-07-22
---

# Diary Holiday Awareness + Variety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 마스코트 일기가 한국 법정공휴일을 "쉬는 날"로 인식(+다정하게 언급)하고, 주말·공휴일·무활동 일기의 단조로움을 줄인다.

**Architecture:** `crates/core/src/diary`에 한국 공휴일 조회(`korean_public_holiday`)를 추가하고, 판정 신호(`WorkContext.is_holiday`/`IdleContext.is_holiday`)와 언급 신호(`Occasion.mood`)를 **두 채널로 분리**한다. 다양성은 같은 성격(idle/주말/공휴일) 최근 일기를 반복 방지 컨텍스트에 병합(`collect_similar_diaries`)하고 무활동일 프롬프트 소재 팔레트를 확장·회전해 개선한다.

**Tech Stack:** Rust (Cargo workspace, lib `agent_mentor`), chrono, rusqlite, serde. Tauri v2 셸(`agent-mentor-app`)은 필드 배선만.

## Global Constraints

- 무거운 로직은 `crates/core`에. `src-tauri/pipeline.rs`는 배선만.
- DB **스키마 변경 없음** — 과거 날짜 idle 판정은 기존 `daily_rollup` 조회로.
- `KR_HOLIDAYS`는 **2026–2027만**. 이후 연도·비-ko 로케일은 `None`(패닉 없음).
- `LUNAR_HOLIDAYS`(설·추석, 2025–2035)는 **손대지 않는다**(zh/ja 언급 회귀 방지).
- 한국 공휴일은 **`ko` 로케일에서만** 판정·언급(비한국 사용자는 주말만 쉬는 날).
- 커밋은 **영어 Conventional Commits**.
- 테스트(주 게이트): `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib` (baseline 380 passed).
- 빌드·실행은 **네이티브 Windows PowerShell** — WSL 내부 금지.
- 스펙: `docs/archive/design/a-mate/specs/2026-07-22-diary-holiday-and-variety-design.md`.

## File Structure

| 파일 | 책임 |
|------|------|
| `a-mate/crates/core/src/diary/occasions.rs` | 공휴일 데이터·조회, `Occasion.mood`, `compute_occasions` 통합·중복 제거 |
| `a-mate/crates/core/src/diary/mod.rs` | `WorkContext.is_holiday`, `IdleContext`(+필드), `assemble_brief` 배선, `collect_similar_diaries`, 두 프롬프트 |
| `a-mate/src-tauri/src/pipeline.rs` | `IdleContext` 생성부 배선(3필드) |

모든 경로는 워크트리 루트(`D:\Project\space-a\.claude\worktrees\diary-holiday-variety`) 기준. 명령은 이 디렉터리에서 실행.

---

### Task 1: 한국 공휴일 조회 (`korean_public_holiday`)

**Files:**
- Modify: `a-mate/crates/core/src/diary/occasions.rs` (상단 데이터·조회 함수 추가; 기존 `#[cfg(test)] mod tests`에 테스트 추가)

**Interfaces:**
- Produces:
  - `pub struct KrHoliday { pub label: String, pub mood: &'static str }`
  - `pub fn korean_public_holiday(date: chrono::NaiveDate, locale: &str) -> Option<KrHoliday>` — `ko` 로케일·2026–2027 등록일에서만 `Some`
  - (private) `enum HolidayMood`, `const KR_HOLIDAYS`

- [ ] **Step 1: 실패하는 테스트 작성**

`occasions.rs`의 `mod tests` 안에 추가(기존 `d(..)` 헬퍼 재사용):

```rust
    #[test]
    fn korean_public_holiday_detects_and_gates_locale() {
        // 제헌절 2026-07-17 (2026 재지정) — 회귀 기준점
        let h = korean_public_holiday(d(2026, 7, 17), "ko-KR").unwrap();
        assert_eq!(h.label, "제헌절");
        assert_eq!(h.mood, "national");
        // 현충일 = 추모(solemn)
        assert_eq!(korean_public_holiday(d(2026, 6, 6), "ko").unwrap().mood, "solemn");
        // 설날 연휴 3일 모두 공휴일
        assert!(korean_public_holiday(d(2026, 2, 16), "ko").is_some());
        assert!(korean_public_holiday(d(2026, 2, 17), "ko").is_some());
        assert!(korean_public_holiday(d(2026, 2, 18), "ko").is_some());
        // 대체공휴일
        assert_eq!(korean_public_holiday(d(2026, 8, 17), "ko").unwrap().mood, "substitute");
        // 비-ko 로케일 → None
        assert!(korean_public_holiday(d(2026, 7, 17), "en").is_none());
        assert!(korean_public_holiday(d(2026, 7, 17), "ja").is_none());
        // 테이블 밖 연도 → None (패닉 없음)
        assert!(korean_public_holiday(d(2028, 7, 17), "ko").is_none());
    }

    #[test]
    fn korean_public_holiday_2027_substitutes_pinned() {
        // 미래 연도 오타 조기 검출 — 값은 테이블에서 읽어 고정
        assert_eq!(korean_public_holiday(d(2027, 2, 8), "ko").unwrap().label, "설날 대체공휴일");
        assert_eq!(korean_public_holiday(d(2027, 12, 27), "ko").unwrap().mood, "substitute");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib korean_public_holiday`
Expected: FAIL — `cannot find function korean_public_holiday` (미구현).

- [ ] **Step 3: 최소 구현 추가**

`occasions.rs`에서 기존 `LUNAR_HOLIDAYS` 상수 **아래**(65줄 이후, `fn lang_of` 위)에 삽입:

```rust
#[derive(Clone, Copy)]
enum HolidayMood {
    Festive,
    National,
    Solemn,
    Family,
    Substitute,
}

impl HolidayMood {
    fn as_str(self) -> &'static str {
        match self {
            HolidayMood::Festive => "festive",
            HolidayMood::National => "national",
            HolidayMood::Solemn => "solemn",
            HolidayMood::Family => "family",
            HolidayMood::Substitute => "substitute",
        }
    }
}

/// 한국 법정공휴일 조회 결과 — 언급 라벨 + 톤(mood).
pub struct KrHoliday {
    pub label: String,
    pub mood: &'static str,
}

// 한국 법정공휴일 red-day 테이블 (2026–2027). 2026-07-22 사용자 확인.
// 현행 대체공휴일 규칙이 이 2년에 실제 만들어내는 날만 열거. 이후 연도는 조용히 생략(패닉 없음).
const KR_HOLIDAYS: &[(i32, u32, u32, &str, HolidayMood)] = &[
    (2026, 1, 1, "신정", HolidayMood::Festive),
    (2026, 2, 16, "설날 연휴", HolidayMood::Family),
    (2026, 2, 17, "설날", HolidayMood::Family),
    (2026, 2, 18, "설날 연휴", HolidayMood::Family),
    (2026, 3, 1, "삼일절", HolidayMood::National),
    (2026, 3, 2, "삼일절 대체공휴일", HolidayMood::Substitute),
    (2026, 5, 5, "어린이날", HolidayMood::Festive),
    (2026, 5, 24, "부처님 오신 날", HolidayMood::Festive),
    (2026, 5, 25, "부처님 오신 날 대체공휴일", HolidayMood::Substitute),
    (2026, 6, 6, "현충일", HolidayMood::Solemn),
    (2026, 7, 17, "제헌절", HolidayMood::National),
    (2026, 8, 15, "광복절", HolidayMood::National),
    (2026, 8, 17, "광복절 대체공휴일", HolidayMood::Substitute),
    (2026, 9, 24, "추석 연휴", HolidayMood::Family),
    (2026, 9, 25, "추석", HolidayMood::Family),
    (2026, 9, 26, "추석 연휴", HolidayMood::Family),
    (2026, 10, 3, "개천절", HolidayMood::National),
    (2026, 10, 5, "개천절 대체공휴일", HolidayMood::Substitute),
    (2026, 10, 9, "한글날", HolidayMood::National),
    (2026, 12, 25, "성탄절", HolidayMood::Festive),
    (2027, 1, 1, "신정", HolidayMood::Festive),
    (2027, 2, 5, "설날 연휴", HolidayMood::Family),
    (2027, 2, 6, "설날", HolidayMood::Family),
    (2027, 2, 7, "설날 연휴", HolidayMood::Family),
    (2027, 2, 8, "설날 대체공휴일", HolidayMood::Substitute),
    (2027, 3, 1, "삼일절", HolidayMood::National),
    (2027, 5, 5, "어린이날", HolidayMood::Festive),
    (2027, 5, 13, "부처님 오신 날", HolidayMood::Festive),
    (2027, 6, 6, "현충일", HolidayMood::Solemn),
    (2027, 7, 17, "제헌절", HolidayMood::National),
    (2027, 8, 15, "광복절", HolidayMood::National),
    (2027, 8, 16, "광복절 대체공휴일", HolidayMood::Substitute),
    (2027, 9, 14, "추석 연휴", HolidayMood::Family),
    (2027, 9, 15, "추석", HolidayMood::Family),
    (2027, 9, 16, "추석 연휴", HolidayMood::Family),
    (2027, 10, 3, "개천절", HolidayMood::National),
    (2027, 10, 4, "개천절 대체공휴일", HolidayMood::Substitute),
    (2027, 10, 9, "한글날", HolidayMood::National),
    (2027, 10, 11, "한글날 대체공휴일", HolidayMood::Substitute),
    (2027, 12, 25, "성탄절", HolidayMood::Festive),
    (2027, 12, 27, "성탄절 대체공휴일", HolidayMood::Substitute),
];

/// 그날이 한국 법정공휴일이면 라벨·mood를 돌려준다. `ko` 로케일에서만 `Some`.
/// 테이블 밖 연도·비-ko 로케일은 `None`(패닉 없음).
pub fn korean_public_holiday(date: NaiveDate, locale: &str) -> Option<KrHoliday> {
    // BCP-47 대소문자 무관 — 소문자 정규화 후 언어만 비교
    if lang_of(&locale.to_lowercase()) != "ko" {
        return None;
    }
    KR_HOLIDAYS
        .iter()
        .find(|(y, m, dd, _, _)| *y == date.year() && *m == date.month() && *dd == date.day())
        .map(|(_, _, _, label, mood)| KrHoliday {
            label: (*label).to_string(),
            mood: mood.as_str(),
        })
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib korean_public_holiday`
Expected: PASS (2 tests).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/occasions.rs
git commit -m "feat(diary): add Korean public holiday lookup table (2026-2027)"
```

---

### Task 2: `Occasion.mood` + `compute_occasions` 통합·중복 제거

**Files:**
- Modify: `a-mate/crates/core/src/diary/occasions.rs` (`Occasion` 구조체, 기존 5개 리터럴, `compute_occasions`, 새 dedupe 헬퍼, 테스트)

**Interfaces:**
- Consumes: `korean_public_holiday` (Task 1)
- Produces: `Occasion { category: String, label: String, mood: Option<String> }` — 공휴일만 `mood` 세팅, 그 외 `None`

- [ ] **Step 1: 실패하는 테스트 작성**

`occasions.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn korean_holiday_appears_in_occasions_with_mood() {
        let occ = compute_occasions(d(2026, 7, 17), None, "ko-KR", false);
        let jeheon = occ.iter().find(|o| o.label == "제헌절").unwrap();
        assert_eq!(jeheon.category, "holiday");
        assert_eq!(jeheon.mood.as_deref(), Some("national"));
    }

    #[test]
    fn korean_seollal_deduped_and_family_mood() {
        // ko 설날 당일은 LUNAR·KR 중복 없이 1개, mood=family
        let occ = compute_occasions(d(2026, 2, 17), None, "ko", false);
        let seollal: Vec<_> = occ.iter().filter(|o| o.label == "설날").collect();
        assert_eq!(seollal.len(), 1, "중복 제거");
        assert_eq!(seollal[0].mood.as_deref(), Some("family"));
    }

    #[test]
    fn lunar_mention_unchanged_for_ja() {
        // ja는 KR 공휴일 없음 → 기존 음력 명절 언급 유지(mood 없음), 한국 국경일 없음
        let ja = compute_occasions(d(2026, 2, 17), None, "ja", false);
        assert!(ja.iter().any(|o| o.label == "旧正月" && o.mood.is_none()));
        assert!(compute_occasions(d(2026, 7, 17), None, "ja", false).iter().all(|o| o.label != "제헌절"));
    }

    #[test]
    fn fun_day_occasion_has_no_mood() {
        let occ = compute_occasions(d(2026, 2, 14), None, "ko", false);
        let val = occ.iter().find(|o| o.label == "발렌타인데이").unwrap();
        assert!(val.mood.is_none());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib occasions`
Expected: 컴파일 에러 — `Occasion`에 `mood` 필드 없음 / dedupe 미구현.

- [ ] **Step 3: 구조체·리터럴·통합·dedupe 구현**

(a) `Occasion` 구조체(4–8줄)를 교체:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Occasion {
    pub category: String, // "holiday" | "milestone"
    pub label: String,    // 이미 로케일 지역화된 표시 문자열
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>, // 공휴일 언급 톤(festive/national/solemn/family/substitute). 그 외 None.
}
```

(b) `compute_occasions` 안 기존 `Occasion { .. }` 리터럴 **5곳** 모두에 `mood: None` 추가:
- 마일스톤 `함께한 지 {days}일` (약 122줄)
- 마일스톤 `함께한 지 {}주년` (약 125줄)
- 양력 명절 `label: name` (약 140줄)
- `Programmer's Day` (약 147줄)
- 음력 명절 `lunar_name(..)` (약 154줄)

예(양력 명절):
```rust
                out.push(Occasion { category: "holiday".into(), label: name, mood: None });
```
예(음력 명절):
```rust
                out.push(Occasion { category: "holiday".into(), label: lunar_name(*fest, lang), mood: None });
```
(나머지 3곳도 동일하게 `mood: None` 추가.)

(c) 음력 명절 블록(약 150–157줄) **바로 뒤**, `out` 반환 직전에 KR 공휴일 통합 + dedupe 추가:

```rust
    // 한국 법정공휴일 (ko 로케일) — 언급 + mood. 판정(is_holiday)은 mod.rs에서 별도.
    if let Some(h) = korean_public_holiday(date, locale) {
        out.push(Occasion { category: "holiday".into(), label: h.label, mood: Some(h.mood.to_string()) });
    }

    // 설·추석 당일은 LUNAR(mood 없음)·KR(mood 있음) 양쪽에서 잡힐 수 있다.
    // 같은 label의 mood 없는 holiday는 mood 있는 쪽을 남기고 제거.
    dedupe_holidays_prefer_mood(&mut out);

    out
```

(d) 파일 하단(테스트 mod 위)에 헬퍼 추가:

```rust
/// holiday 항목 중 같은 label이 mood 유·무로 중복되면 mood 있는 쪽만 남긴다.
fn dedupe_holidays_prefer_mood(out: &mut Vec<Occasion>) {
    let mooded: std::collections::HashSet<String> = out
        .iter()
        .filter(|o| o.category == "holiday" && o.mood.is_some())
        .map(|o| o.label.clone())
        .collect();
    out.retain(|o| !(o.category == "holiday" && o.mood.is_none() && mooded.contains(&o.label)));
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib occasions`
Expected: PASS (기존 occasions 테스트 + 신규 4개).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/occasions.rs
git commit -m "feat(diary): surface Korean holidays in occasions with mood tag"
```

---

### Task 3: `WorkContext.is_holiday` + `assemble_brief` 판정 배선

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`WorkContext`, `collect_work_context`, `assemble_brief`, import, 테스트)

**Interfaces:**
- Consumes: `korean_public_holiday` (Task 1)
- Produces: `WorkContext.is_holiday: bool` — `ko` 로케일 법정공휴일 여부(활동 유무 무관)

- [ ] **Step 1: 실패하는 테스트 작성**

`mod.rs`의 `mod tests`에 추가(기존 `assemble_brief_work_context_*` 패턴 참고):

```rust
    #[test]
    fn assemble_brief_flags_korean_holiday_as_rest_day() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        // 제헌절 2026-07-17(금) — ko. 활동 0이어도 is_holiday=true → 평일 idle 오판 방지.
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        let brief = assemble_brief(&store, "Windows", "2026-07-17", &cfg).unwrap();
        assert!(brief.work_context.is_holiday, "제헌절은 공휴일");
        assert!(!brief.work_context.is_weekend, "07-17은 금요일");
        assert!(brief.occasions.iter().any(|o| o.label == "제헌절"), "언급 채널에도 등장");
    }

    #[test]
    fn assemble_brief_holiday_gated_by_locale_and_plain_weekday() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let en = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("en-US".into()),
            ..DiaryConfig::default()
        };
        assert!(!assemble_brief(&store, "Windows", "2026-07-17", &en).unwrap().work_context.is_holiday,
            "en 로케일 → 한국 공휴일 미적용");
        let ko = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        assert!(!assemble_brief(&store, "Windows", "2026-07-16", &ko).unwrap().work_context.is_holiday,
            "07-16 목요일 비공휴일 → false");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib assemble_brief_flags_korean_holiday`
Expected: 컴파일 에러 — `WorkContext`에 `is_holiday` 없음.

- [ ] **Step 3: 구현**

(a) import 확장 (5줄 부근):
```rust
use crate::diary::occasions::{compute_occasions, korean_public_holiday, Occasion};
```

(b) `WorkContext`(32–36줄)에 `is_holiday` 추가:
```rust
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkContext {
    pub is_weekend: bool,
    pub is_holiday: bool,  // 한국 법정공휴일(ko 로케일)인지 — 쉬는 날 판정
    pub active_hours: f64,
    pub long_work: bool,
}
```

(c) `collect_work_context` 반환 리터럴(456–460줄)에 `is_holiday: false` 추가(locale을 모르므로 기본 false, assemble_brief가 세팅):
```rust
    WorkContext {
        is_weekend: matches!(today.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun),
        is_holiday: false,
        active_hours,
        long_work: active_hours >= LONG_WORK_HOURS,
    }
```

(d) `assemble_brief`의 work_context 계산부(346–349줄)를 교체:
```rust
    let mut work_context = match today {
        Some(d) => collect_work_context(store, date, d),
        None => WorkContext::default(),
    };
    work_context.is_holiday = today
        .map(|d| korean_public_holiday(d, &locale).is_some())
        .unwrap_or(false);
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib`
Expected: PASS (신규 2개 포함, 기존 전부 유지).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(diary): flag Korean public holidays as rest days in work context"
```

---

### Task 4: `IdleContext` 필드 + 파이프라인 배선

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`IdleContext`, 기존 idle 테스트 리터럴, 신규 테스트)
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`IdleContext` 생성부 300–306줄)

**Interfaces:**
- Consumes: `WorkContext.is_holiday` (Task 3), `RecentDiary` (기존)
- Produces: `IdleContext { date, is_weekend, is_holiday, days_idle, occasions, recent_diaries }`

- [ ] **Step 1: 실패하는 테스트 작성 + 기존 리터럴 갱신**

(a) `mod.rs` `mod tests`에 신규 테스트 추가:
```rust
    #[test]
    fn idle_context_carries_holiday_and_recent() {
        let idle = IdleContext {
            date: "2026-07-17".into(),
            is_weekend: false,
            is_holiday: true,
            days_idle: Some(1),
            occasions: vec![],
            recent_diaries: vec![],
        };
        let json = serde_json::to_string(&idle).unwrap();
        assert!(json.contains("\"is_holiday\":true"));
        assert!(json.contains("recent_diaries"));
    }
```

(b) 기존 `render_idle_diary_writes_persona_with_footer`(1599–1601줄)의 `IdleContext` 리터럴에 신규 필드 추가:
```rust
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: true, is_holiday: false, days_idle: Some(2),
            occasions: vec![], recent_diaries: vec![],
        };
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib idle_context_carries`
Expected: 컴파일 에러 — `IdleContext`에 `is_holiday`/`recent_diaries` 없음.

- [ ] **Step 3: 구현**

(a) `IdleContext`(952–958줄)에 필드 추가:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct IdleContext {
    pub date: String,
    pub is_weekend: bool,
    pub is_holiday: bool,
    pub days_idle: Option<i64>, // 마지막 활동일로부터 며칠째 조용한지(모르면 None)
    pub occasions: Vec<Occasion>,
    pub recent_diaries: Vec<RecentDiary>, // 최근 같은 성격 일기 — 반복 방지
}
```

(b) `pipeline.rs`의 `IdleContext` 생성부(300–306줄)를 교체:
```rust
            let rendered = if brief.totals.session_count == 0 {
                let idle = IdleContext {
                    date: date.clone(),
                    is_weekend: brief.work_context.is_weekend,
                    is_holiday: brief.work_context.is_holiday,
                    days_idle,
                    occasions: brief.occasions.clone(),
                    recent_diaries: brief.recent_diaries.clone(),
                };
```

- [ ] **Step 4: 통과 확인 (lib + 워크스페이스 컴파일)**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib idle_context_carries`
Expected: PASS.

Run: `cargo check --manifest-path a-mate/Cargo.toml --workspace`
Expected: 컴파일 성공(`pipeline.rs` 포함). 만약 Tauri 환경 의존으로 `agent-mentor-app` 체크가 실패하면, 이 변경은 3필드 배선뿐이므로 diff 검토로 확인하고 lib 테스트를 게이트로 삼는다(사유를 커밋 메시지/리뷰에 기록).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(diary): pass holiday flag and recent diaries into idle context"
```

---

### Task 5: `collect_similar_diaries` — 같은 성격 최근 일기 병합

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`assemble_brief` 병합부, 신규 헬퍼 `day_kind`/`date_was_idle`/`collect_similar_diaries`, `DayKind`, 상수, 테스트)

**Interfaces:**
- Consumes: `korean_public_holiday` (Task 1), `collect_recent_diaries`/`cap_chars`/`RECENT_DIARY_EXCERPT_CAP`/`RecentDiary` (기존)
- Produces: `assemble_brief`의 `brief.recent_diaries`가 오늘이 idle/주말/공휴일이면 같은 성격 과거 일기(최대 2편)를 포함

- [ ] **Step 1: 실패하는 테스트 작성**

`mod.rs` `mod tests`에 추가:
```rust
    #[test]
    fn similar_diaries_pulls_recent_idle_outside_3day_window() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig {
            vault_dir: tmp.path().to_path_buf(),
            locale: Some("ko-KR".into()),
            ..DiaryConfig::default()
        };
        // 14일 전(3일 창 밖) idle 일기 하나 — 이벤트 없음 → idle. 파일·인덱스는 persist_diary로 생성.
        let past = RenderedDiary {
            body: "심심해서 옆 동네 봇이랑 놀았다.\n\n*— ~10 토큰 (엔진: mock)*\n".into(),
            tokens_used: 10,
            engine_name: "mock".into(),
        };
        persist_diary(&store, "2026-07-16", "Windows", &past, &cfg).unwrap();
        // 오늘 2026-07-30 도 idle(이벤트 없음) → 같은 성격 병합
        let brief = assemble_brief(&store, "Windows", "2026-07-30", &cfg).unwrap();
        let hit = brief.recent_diaries.iter().find(|r| r.date == "2026-07-16");
        assert!(hit.is_some(), "같은 성격(idle) 최근 일기 병합");
        assert_eq!(hit.unwrap().excerpt, "심심해서 옆 동네 봇이랑 놀았다.", "토큰 푸터 제외");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib similar_diaries_pulls`
Expected: FAIL — `2026-07-16`이 `recent_diaries`에 없음(3일 창 밖, 아직 미구현).

- [ ] **Step 3: 구현**

(a) `collect_recent_diaries` 위(약 366줄 상수 근처)에 상수·헬퍼 추가:
```rust
const SIMILAR_LOOKBACK: i64 = 28; // 같은 성격 일기 최대 소급 일수
const SIMILAR_MAX: usize = 2;     // 병합할 같은 성격 일기 최대 편수

#[derive(Clone, Copy, PartialEq)]
enum DayKind {
    Idle,       // 활동 0
    RestActive, // 활동 있으나 주말·공휴일
    Plain,      // 평일 활동일 (단조로움 문제 아님 — 보강 안 함)
}

/// 그날 활동이 전혀 없었나(rollup 세션 0). idle 판정용.
fn date_was_idle(store: &SqliteStore, date: &str) -> bool {
    store
        .conn
        .query_row(
            "SELECT COALESCE(SUM(session_count),0) FROM daily_rollup WHERE date=?1",
            params![date],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        == 0
}

fn day_kind(store: &SqliteStore, date_str: &str, date: NaiveDate, locale: &str) -> DayKind {
    if date_was_idle(store, date_str) {
        return DayKind::Idle;
    }
    let rest = matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
        || korean_public_holiday(date, locale).is_some();
    if rest {
        DayKind::RestActive
    } else {
        DayKind::Plain
    }
}

/// 오늘과 같은 성격(idle / 주말·공휴일 활동일)의 최근 일기를 최대 SIMILAR_MAX편 모은다.
/// 매주 토요일·매 공휴일처럼 3일 창엔 안 잡히는 반복을 반복 방지 컨텍스트에 넣기 위함.
/// 평일 활동일(Plain)은 보강하지 않는다.
fn collect_similar_diaries(store: &SqliteStore, host: &str, today: NaiveDate, locale: &str) -> Vec<RecentDiary> {
    let today_str = today.format("%Y-%m-%d").to_string();
    let kind = day_kind(store, &today_str, today, locale);
    if kind == DayKind::Plain {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 1..=SIMILAR_LOOKBACK {
        if out.len() >= SIMILAR_MAX {
            break;
        }
        let day = today - chrono::Duration::days(i);
        let date = day.format("%Y-%m-%d").to_string();
        if day_kind(store, &date, day, locale) != kind {
            continue;
        }
        let Some(path) = store.diary_path_for_scope(&date, host).ok().flatten() else {
            continue;
        };
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let narrative = body.split("\n\n*—").next().unwrap_or(&body).trim();
        out.push(RecentDiary { date, excerpt: cap_chars(narrative, RECENT_DIARY_EXCERPT_CAP) });
    }
    out
}
```

(b) `assemble_brief`에서 `recent_keys` 계산(308–313줄) **직후**, `findings` 계산(315줄) **전**에 병합 삽입(finding 억제는 3일 창 그대로 유지):
```rust
    // recent_keys(위)는 3일 창 기준 그대로 — finding 억제 동작 불변.
    // 같은 성격(주말·공휴일·idle) 최근 일기를 서사 반복 방지용으로만 병합.
    let recent_diaries = {
        let mut rd = recent_diaries;
        if let Some(d) = today {
            let seen: std::collections::HashSet<String> = rd.iter().map(|r| r.date.clone()).collect();
            for sim in collect_similar_diaries(store, host, d, &locale) {
                if !seen.contains(&sim.date) {
                    rd.push(sim);
                }
            }
        }
        rd
    };
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib`
Expected: PASS (신규 포함, 기존 전부 유지 — 특히 finding 억제 관련 테스트 불변).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(diary): merge same-kind recent diaries into anti-repetition context"
```

---

### Task 6: 프롬프트 — 공휴일 위로·mood 톤 + 무활동일 반복방지·소재 팔레트

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`build_system_prompt`, `build_idle_prompt`, `render_idle_diary` 호출, 소재 팔레트 헬퍼, 기존/신규 테스트)

**Interfaces:**
- Consumes: `WorkContext.is_holiday` (Task 3), `Occasion.mood` (Task 2), `IdleContext.recent_diaries`/`is_holiday` (Task 4)
- Produces: `pub fn build_idle_prompt(cfg: &DiaryConfig, idle: &IdleContext) -> String` (시그니처 변경)

- [ ] **Step 1: 실패하는 테스트 작성 + 기존 idle 테스트 갱신**

(a) 기존 `idle_prompt_directs_imaginative_persona`(1586–1593줄)를 교체(신규 시그니처):
```rust
    #[test]
    fn idle_prompt_directs_imaginative_persona() {
        let idle = IdleContext {
            date: "2026-07-11".into(), is_weekend: true, is_holiday: false, days_idle: Some(2),
            occasions: vec![], recent_diaries: vec![],
        };
        let p = build_idle_prompt(&DiaryConfig::default(), &idle);
        assert!(p.contains("조용한 날"));
        assert!(p.contains("지어내"));
        assert!(p.contains("에이전트 친구"));
        assert!(p.contains("송편") || p.contains("명절"));
        assert!(p.contains("days_idle"));
        assert!(p.contains("recent_diaries")); // 반복 방지 신규
    }
```

(b) 신규 테스트 추가:
```rust
    #[test]
    fn idle_prompt_palette_rotates_by_date_and_frames_holiday() {
        let base = IdleContext {
            date: "2026-01-01".into(), is_weekend: false, is_holiday: true, days_idle: None,
            occasions: vec![], recent_diaries: vec![],
        };
        let other = IdleContext { date: "2026-06-15".into(), ..base.clone() };
        let p1 = build_idle_prompt(&DiaryConfig::default(), &base);
        let p2 = build_idle_prompt(&DiaryConfig::default(), &other);
        assert_ne!(p1, p2, "날짜에 따라 소재 spotlight 회전");
        assert!(p1.contains("mood"));    // occasion mood 톤 처리
        assert!(p1.contains("공휴일"));   // is_holiday 프레이밍
    }

    #[test]
    fn system_prompt_handles_holiday_and_mood() {
        let p = build_system_prompt(&DiaryConfig::default(), 0);
        assert!(p.contains("is_holiday"));  // 공휴일 근무 위로 트리거
        assert!(p.contains("mood"));         // occasion mood 톤 처리
        assert!(p.contains("추모"));         // solemn 처리 지시
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib idle_prompt`
Expected: 컴파일 에러 — `build_idle_prompt`가 1인자(시그니처 미변경) / 신규 문구 없음.

- [ ] **Step 3: 구현**

(a) `build_system_prompt`의 occasions 언급 지침(889–891줄) 교체:
```rust
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절·공휴일), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요. 각 occasion의 `mood`에 맞춰 톤을 고르세요: \
         `solemn`(예: 현충일)은 능청·유머를 접고 조용하고 담백하게 추모하듯, `national`(삼일절·광복절 등)은 \
         담백한 자긍심으로, `family`(설날·추석)는 따뜻한 명절 분위기로, `substitute`는 '○○ 대체공휴일이라 \
         하루 더 쉬는 날'처럼, `mood`가 없으면(발렌타인·파이데이 등) 가볍게. 비어있으면 언급하지 마세요. \
```

(b) `build_system_prompt`의 work_context 설명(900줄) 중 `(주말 여부·몰입 시간)`를 `(주말·공휴일 여부·몰입 시간)`으로 교체.

(c) `build_system_prompt`의 위로 지침(912–914줄) 교체:
```rust
         위로·응원은 매일이 아니라 `work_context.long_work`(유난히 긴 날)·`is_weekend`(주말 근무)·\
         `is_holiday`(공휴일 근무) 때만, 그것도 판박이 대신 다마고치 능청으로(주말이면 '주말에 또? 일중독인가 봐', \
         긴 날이면 '오늘 좀 과했다, 배터리 방전 직전', 공휴일이면 '남들 다 쉬는 날에도 왔네'). \
         단 그날 occasion의 `mood`가 `solemn`(현충일 등)이면 능청을 접고 담백하게. \
         평범한 날은 위로 없이 담백하게 끝내세요. 발렌타인·파이데이 같은 재미 기념일은 위로 대상이 아닙니다. \
```

(d) `build_idle_prompt`(961–977줄) 전체 교체(시그니처 변경 + 팔레트·반복방지·mood·is_holiday):
```rust
// 무활동일 소재 팔레트 — 범주별 예시. day-of-year로 회전해 매번 다른 결을 부각(난수 없이 결정적).
const IDLE_PALETTE: &[&str] = &[
    "옆 동네 에이전트와 산책하며 로그 구경",
    "친구 봇이 놀러 와 수다·보드게임",
    "다른 에이전트와 사소한 실력 겨루기",
    "혼자 캐시·로그를 정리하며 도토리 모으듯 뿌듯해하기",
    "코드 낙서를 끄적이다 낮잠",
    "창밖 날씨·계절을 상상하며 멍때리기",
    "주인이 두고 간 프로젝트 폴더를 기웃거리기",
    "마스코트끼리 소소한 품앗이(서로 로그 봐주기)",
];

/// date(YYYY-MM-DD)의 day-of-year로 팔레트에서 3개를 회전 선택. 파싱 실패 시 앞 3개.
fn idle_palette_spotlight(date: &str) -> String {
    let start = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d.ordinal() as usize)
        .unwrap_or(0);
    let n = IDLE_PALETTE.len();
    (0..3).map(|k| IDLE_PALETTE[(start + k) % n]).collect::<Vec<_>>().join(" / ")
}

/// 무활동일 일기 시스템 프롬프트 — 작업 사실 없이 마스코트의 자유 시간을 능청스러운 상상 일기로.
pub fn build_idle_prompt(cfg: &DiaryConfig, idle: &IdleContext) -> String {
    let spotlight = idle_palette_spotlight(&idle.date);
    format!(
        "당신은 {honorific}의 AI 코딩 여정을 함께하는 마스코트입니다. \
         오늘은 {honorific}이 한 번도 찾아오지 않은 '조용한 날' — 나(마스코트)의 자유 시간입니다. \
         {voice} \
         단, 이건 작업 기록이 아니라 마스코트의 상상 일기입니다 — 위 문체 가이드의 '브리프 사실만' 조항은 여기선 무시하고 \
         (문체·자연스러움 규칙은 그대로 지키되) 나만의 하루를 맘껏 능청스럽게 지어내세요. 업무 이야기는 하지 마세요. \
         나에겐 옆 동네 다른 에이전트 친구들이 있고 혼자만의 취미도 있습니다. 오늘은 특히 이런 결의 소재를 살려보세요: {spotlight}. \
         (예시일 뿐 — 매번 똑같이 쓰지 말고 오늘만의 장면을 하나 골라 구체적으로.) \
         `recent_diaries`는 최근 조용한 날들에 내가 쓴 일기입니다. 거기서 이미 쓴 소재·장면·표현은 되풀이하지 말고 오늘은 다른 이야기로 쓰세요. \
         `occasions`에 명절·기념일·공휴일이 있으면 각 `mood`에 맞춰(‘solemn’이면 조용·담백하게 추모하듯, ‘family’면 이웃 에이전트와 명절 정취, ‘festive’면 즐겁게) 분위기를 살리세요. \
         `is_holiday`가 true면 '주인이 안 온 날'이 아니라 '다 같이 쉬는 공휴일'로 프레이밍하세요. \
         `days_idle`(며칠째 조용한지)·`is_weekend`도 살려 {honorific}의 안부를 슬쩍 궁금해하세요('그나저나 주인 잘 노나?'). \
         짧게 — 1~2문장(특별한 날은 2~3문장까지), 한 문단. 이모지는 0~1개. 그날 컨텍스트로 매번 다르게.",
        honorific = cfg.honorific,
        voice = voice_guidance(),
        spotlight = spotlight,
    )
}
```

(e) `render_idle_diary`(981줄) 호출 갱신:
```rust
    let system = build_idle_prompt(cfg, idle);
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor --lib`
Expected: PASS (신규 포함, 기존 프롬프트 문자열 테스트 전부 유지).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(diary): add holiday comfort/mood tone and richer idle prompt variety"
```

---

## Self-Review

**1. Spec coverage**

| 스펙 항목 | 태스크 |
|-----------|--------|
| §4.1 공휴일 테이블(2026–2027) | Task 1 |
| §4.2 mood 매핑 | Task 1(데이터) + Task 6(톤 지침) |
| §4.3 조회 함수 | Task 1 |
| §4.4 compute_occasions 통합·dedupe·LUNAR 유지 | Task 2 |
| §5 Occasion.mood | Task 2 |
| §5 WorkContext.is_holiday | Task 3 |
| §5 IdleContext(+is_holiday,+recent_diaries) | Task 4 |
| §6 데이터 흐름(assemble_brief·pipeline, recent_keys 불변) | Task 3·4·5 |
| §7a same-kind 반복방지 | Task 5 |
| §7b 팔레트 확장·회전 | Task 6 |
| §8a 활동일 프롬프트(is_holiday 위로+mood) | Task 6 |
| §8b 무활동일 프롬프트(반복방지+팔레트+mood+is_holiday) | Task 6 |
| §9 테스트·회귀(제헌절 is_holiday) | Task 3 |

누락 없음.

**2. Placeholder scan:** "TBD/TODO/적절히" 등 없음 — 모든 코드 스텝에 실제 코드 포함.

**3. Type consistency:**
- `korean_public_holiday(NaiveDate, &str) -> Option<KrHoliday>` — Task 1 정의, Task 2·3·5에서 동일 시그니처 사용.
- `KrHoliday { label: String, mood: &'static str }` — Task 2에서 `h.label`(String), `h.mood`(→`.to_string()`) 일관.
- `Occasion.mood: Option<String>` — Task 2 정의, Task 6 프롬프트는 JSON 필드명 `mood`로 참조(직렬화 키 일치).
- `WorkContext.is_holiday: bool` — Task 3 정의, Task 4에서 `brief.work_context.is_holiday` 복사, Task 6 프롬프트가 `work_context.is_holiday` 참조.
- `IdleContext.recent_diaries: Vec<RecentDiary>` / `is_holiday: bool` — Task 4 정의, Task 5가 `brief.recent_diaries` 채움, Task 6이 `recent_diaries`/`is_holiday` 참조.
- `build_idle_prompt(&DiaryConfig, &IdleContext)` — Task 6에서 시그니처 변경, `render_idle_diary`·테스트 동시 갱신.

불일치 없음.

## 검증 노트 (사외망)

`KR_HOLIDAYS` 값은 2026-07-22 사용자 확인 완료(부처님오신날 양력·대체공휴일 발생일·제헌절 대체 비대상 포함). 사외망이라 공식 달력 재대조 불가 — 테이블 오류 시 Task 1의 pin 테스트가 조기 신호.
