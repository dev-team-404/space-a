---
status: done
archived: 2026-07-19
---

# Diary Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 다이어리에 결정론적 occasions(명절·기념일)와 finding별 근거·개선방향을 추가하고 서사 톤을 더 유머러스하게 만들어, 로케일 인지로 계산된 사실을 LLM이 자연스럽게 녹인 일기를 생성한다.

**Architecture:** 기존 "결정론적 계산 → 브리프 → LLM 서사" 파이프라인 확장. 신규 순수 모듈 `diary/occasions.rs`가 날짜·앵커·로케일로 occasions를 계산하고, `assemble_brief`가 이를 `Brief`에 싣고 각 finding에 `detail`/`suggested_action`을 채운다. `build_system_prompt`는 유머·근거·occasions 사용을 지시할 뿐, 실제 사실은 브리프 JSON으로 전달된다(정밀도의 선 유지).

**Tech Stack:** Rust (edition 2021), `chrono`(NaiveDate 날짜 연산 — 이미 의존), `sys-locale`(OS 로케일 자동 감지 — 신규), `serde`/`serde_json`, `rusqlite`. 테스트는 크레이트 내장 `#[cfg(test)]`.

## Global Constraints

- **정밀도의 선(코칭 §1.4):** occasions·근거·개선방향은 전부 **결정론적**으로 계산해 브리프에 싣는다. LLM은 서사만; 브리프에 없는 수치/날짜를 지어내지 않는다.
- **로컬 전용:** occasions는 네트워크/온라인 캘린더 없이 번들 데이터 + `chrono` 날짜연산으로만. egress 없음.
- **이번 페이즈 서사 언어 = 한국어(페르소나).** 다중언어 서사 출력(i18n)은 후속 페이즈(스펙 §8). 단 `locale`은 1급 필드로 둔다.
- **기념일 기준일 = 첫 세션 날짜 자동** (`MIN(sessions.first_ts)`의 날짜). 별도 설정 없음.
- **마일스톤 규칙:** `days == 50` 또는 (`days >= 100` and `days % 100 == 0`) → "함께한 지 {days}일"; `days > 0` and `days % 365 == 0` → "함께한 지 {days/365}주년".
- **로케일 언어 판정 = BCP-47 접두어**(첫 `-` 앞). 적용성: Universal=전부, EastAsia/음력=`ko|zh|ja`, DevCulture=`include_dev_days` 플래그.
- **유머는 페르소나 상향으로 흡수** — 새 톤 프리셋 금지(톤 A/B/C는 감정 강도 유지).
- **빌드 환경(비표준):** Rust가 시스템 PATH에 없음. Git Bash에서 cargo 전 매번:
  ```
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- **선행 코드(이 슬라이스에 이미 존재):** `diary/mod.rs`에 `Brief{date,host,totals,findings}`, `BriefTotals`, `BriefFinding{rule_id,severity,evidence,est_tokens_saved,prescription}`, `assemble_brief(store,host,date)`, `DiaryConfig{vault_dir,tone,honorific}`+Default(vault "./diary", tone "B", honorific "주인"), `build_system_prompt(cfg)`, `generate_diary(store,engine,brief,cfg)`. `store.rs`에 `SqliteStore{pub conn}`, `findings_for_date`, `sessions(first_ts,...)`. `finding.rs`에 `Finding{rule_id,severity,...,evidence:Value,est_tokens_saved,prescription:Option<Prescription>}`.

---

## File Structure

```
src/diary/occasions.rs   # 신규: Occasion, catalog const, 음력 테이블, compute_occasions (순수)
src/diary/mod.rs         # 수정: pub mod occasions; Brief.occasions; BriefFinding.detail/suggested_action;
                         #        finding_advice(); assemble_brief(+cfg, occasions·advice); DiaryConfig(+locale,+include_dev_days);
                         #        resolve_locale(); build_system_prompt(유머·근거·occasions)
src/store.rs             # 수정: earliest_session_ts()
src/main.rs              # 수정: cmd_diary가 cfg를 assemble_brief에 전달
Cargo.toml               # 수정: sys-locale 의존성 추가
```

---

## Task 1: Occasions 계산 모듈 (occasions.rs)

**Files:**
- Create: `src/diary/occasions.rs`
- Modify: `src/diary/mod.rs` (파일 맨 위 `pub mod engine;` 아래에 `pub mod occasions;` 추가)

**Interfaces:**
- Consumes: `chrono::{NaiveDate, Datelike}`
- Produces:
  - `pub struct Occasion { pub category: String, pub label: String }` (Serialize, PartialEq)
  - `pub fn compute_occasions(date: NaiveDate, anchor: Option<NaiveDate>, locale: &str, include_dev_days: bool) -> Vec<Occasion>`

- [ ] **Step 1: `pub mod occasions;` 선언 추가**

`src/diary/mod.rs`의 첫 줄이 `pub mod engine;` 이다. 그 바로 아래에 추가:

```rust
pub mod occasions;
```

- [ ] **Step 2: 실패하는 테스트 작성**

`src/diary/occasions.rs`를 새로 만들고 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn labels(v: &[Occasion]) -> Vec<String> {
        v.iter().map(|o| o.label.clone()).collect()
    }

    #[test]
    fn milestone_50_100_300_and_anniversary() {
        let anchor = d(2026, 1, 1);
        assert!(labels(&compute_occasions(d(2026, 2, 20), Some(anchor), "ko", false))
            .contains(&"함께한 지 50일".to_string())); // 2026-01-01 + 50
        assert!(labels(&compute_occasions(anchor + chrono::Duration::days(100), Some(anchor), "ko", false))
            .contains(&"함께한 지 100일".to_string()));
        assert!(labels(&compute_occasions(anchor + chrono::Duration::days(300), Some(anchor), "ko", false))
            .contains(&"함께한 지 300일".to_string()));
        assert!(labels(&compute_occasions(anchor + chrono::Duration::days(365), Some(anchor), "ko", false))
            .contains(&"함께한 지 1주년".to_string()));
        // 비마일스톤
        assert!(compute_occasions(anchor + chrono::Duration::days(37), Some(anchor), "ko", false)
            .iter().all(|o| o.category != "milestone"));
        // anchor 없음 → 기념일 없음
        assert!(compute_occasions(d(2026, 3, 22), None, "ko", false)
            .iter().all(|o| o.category != "milestone"));
    }

    #[test]
    fn solar_holiday_localized() {
        assert!(labels(&compute_occasions(d(2026, 12, 25), None, "en-US", false))
            .contains(&"Christmas".to_string()));
        assert!(labels(&compute_occasions(d(2026, 12, 25), None, "ko-KR", false))
            .contains(&"크리스마스".to_string()));
        assert!(labels(&compute_occasions(d(2026, 1, 1), None, "ja", false))
            .contains(&"元日".to_string()));
    }

    #[test]
    fn white_day_east_asia_only_and_dev_overlap() {
        // 3/14 ko + dev on → 화이트데이 + Pi Day 둘 다
        let ko = labels(&compute_occasions(d(2026, 3, 14), None, "ko", true));
        assert!(ko.contains(&"화이트데이".to_string()));
        assert!(ko.contains(&"Pi Day".to_string()));
        // en(비동아시아) → 화이트데이 없음, dev off면 Pi Day도 없음
        let en = labels(&compute_occasions(d(2026, 3, 14), None, "en", false));
        assert!(!en.contains(&"화이트데이".to_string()));
        assert!(!en.contains(&"Pi Day".to_string()));
    }

    #[test]
    fn lunar_from_table_localized_and_east_asia_only() {
        // 2025 설날 = 2025-01-29 (검증된 앵커)
        assert!(labels(&compute_occasions(d(2025, 1, 29), None, "ko", false))
            .contains(&"설날".to_string()));
        assert!(labels(&compute_occasions(d(2025, 1, 29), None, "zh", false))
            .contains(&"春节".to_string()));
        // 2025 추석 = 2025-10-06
        assert!(labels(&compute_occasions(d(2025, 10, 6), None, "ko", false))
            .contains(&"추석".to_string()));
        // 비동아시아 로케일 → 음력 명절 생략
        assert!(compute_occasions(d(2025, 1, 29), None, "en", false).is_empty());
        // 테이블에 없는 연도 → 패닉 없이 음력 생략
        assert!(compute_occasions(d(2040, 2, 1), None, "ko", false).is_empty());
    }

    #[test]
    fn programmers_day_256th_day() {
        // 2025는 평년 → 256번째 날 = 9/13
        assert!(labels(&compute_occasions(d(2025, 9, 13), None, "en", true))
            .contains(&"Programmer's Day".to_string()));
        assert!(!labels(&compute_occasions(d(2025, 9, 13), None, "en", false))
            .contains(&"Programmer's Day".to_string()));
    }
}
```

- [ ] **Step 3: 테스트 실패 확인**

Run:
```
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
cargo test --lib occasions
```
Expected: FAIL — `Occasion`, `compute_occasions` 미정의.

- [ ] **Step 4: 구현 작성**

`src/diary/occasions.rs` 상단(테스트 위)에:

```rust
use chrono::{Datelike, NaiveDate};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Occasion {
    pub category: String, // "holiday" | "milestone"
    pub label: String,    // 이미 로케일 지역화된 표시 문자열
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Universal,
    EastAsia,
    DevCulture,
}

struct SolarHoliday {
    month: u32,
    day: u32,
    scope: Scope,
    names: &'static [(&'static str, &'static str)], // (lang, name)
}

const SOLAR_HOLIDAYS: &[SolarHoliday] = &[
    SolarHoliday { month: 1, day: 1, scope: Scope::Universal,
        names: &[("ko", "새해 첫날"), ("en", "New Year's Day"), ("zh", "元旦"), ("ja", "元日")] },
    SolarHoliday { month: 2, day: 14, scope: Scope::Universal,
        names: &[("ko", "발렌타인데이"), ("en", "Valentine's Day"), ("zh", "情人节"), ("ja", "バレンタインデー")] },
    SolarHoliday { month: 10, day: 31, scope: Scope::Universal,
        names: &[("ko", "핼러윈"), ("en", "Halloween"), ("zh", "万圣夜"), ("ja", "ハロウィン")] },
    SolarHoliday { month: 12, day: 25, scope: Scope::Universal,
        names: &[("ko", "크리스마스"), ("en", "Christmas"), ("zh", "圣诞节"), ("ja", "クリスマス")] },
    SolarHoliday { month: 12, day: 31, scope: Scope::Universal,
        names: &[("ko", "한 해의 마지막 날"), ("en", "New Year's Eve"), ("zh", "除夕"), ("ja", "大晦日")] },
    SolarHoliday { month: 4, day: 1, scope: Scope::Universal,
        names: &[("ko", "만우절"), ("en", "April Fools' Day"), ("zh", "愚人节"), ("ja", "エイプリルフール")] },
    SolarHoliday { month: 3, day: 14, scope: Scope::EastAsia,
        names: &[("ko", "화이트데이"), ("zh", "白色情人节"), ("ja", "ホワイトデー")] },
    SolarHoliday { month: 3, day: 14, scope: Scope::DevCulture, names: &[("en", "Pi Day")] },
    SolarHoliday { month: 5, day: 4, scope: Scope::DevCulture, names: &[("en", "Star Wars Day")] },
];

#[derive(Clone, Copy)]
enum LunarFestival {
    LunarNewYear,
    MidAutumn,
}

// 설날(음력1/1)·추석(음력8/15)의 양력 날짜. 2025–2027 검증됨.
// 이후 연도는 KASI(한국천문연구원) 공식 음양력 대조표에서 추가한다.
// 테이블에 없는 연도는 음력 명절을 조용히 생략한다(패닉 없음).
const LUNAR_HOLIDAYS: &[(i32, u32, u32, LunarFestival)] = &[
    (2025, 1, 29, LunarFestival::LunarNewYear), (2025, 10, 6, LunarFestival::MidAutumn),
    (2026, 2, 17, LunarFestival::LunarNewYear), (2026, 9, 25, LunarFestival::MidAutumn),
    (2027, 2, 6, LunarFestival::LunarNewYear), (2027, 9, 15, LunarFestival::MidAutumn),
];

fn lang_of(locale: &str) -> &str {
    locale.split('-').next().unwrap_or("en")
}

fn is_east_asian(lang: &str) -> bool {
    matches!(lang, "ko" | "zh" | "ja")
}

fn name_for(names: &[(&str, &str)], lang: &str) -> Option<String> {
    names
        .iter()
        .find(|(l, _)| *l == lang)
        .or_else(|| names.iter().find(|(l, _)| *l == "en"))
        .or_else(|| names.first())
        .map(|(_, n)| n.to_string())
}

fn scope_applies(scope: Scope, lang: &str, include_dev_days: bool) -> bool {
    match scope {
        Scope::Universal => true,
        Scope::EastAsia => is_east_asian(lang),
        Scope::DevCulture => include_dev_days,
    }
}

fn lunar_name(fest: LunarFestival, lang: &str) -> String {
    match (fest, lang) {
        (LunarFestival::LunarNewYear, "ko") => "설날",
        (LunarFestival::LunarNewYear, "zh") => "春节",
        (LunarFestival::LunarNewYear, "ja") => "旧正月",
        (LunarFestival::LunarNewYear, _) => "Lunar New Year",
        (LunarFestival::MidAutumn, "ko") => "추석",
        (LunarFestival::MidAutumn, "zh") => "中秋节",
        (LunarFestival::MidAutumn, "ja") => "十五夜",
        (LunarFestival::MidAutumn, _) => "Mid-Autumn",
    }
    .to_string()
}

pub fn compute_occasions(
    date: NaiveDate,
    anchor: Option<NaiveDate>,
    locale: &str,
    include_dev_days: bool,
) -> Vec<Occasion> {
    let lang = lang_of(locale);
    let mut out = Vec::new();

    // 기념일 (마일스톤)
    if let Some(a) = anchor {
        let days = (date - a).num_days();
        if days > 0 {
            if days == 50 || (days >= 100 && days % 100 == 0) {
                out.push(Occasion { category: "milestone".into(), label: format!("함께한 지 {days}일") });
            }
            if days % 365 == 0 {
                out.push(Occasion {
                    category: "milestone".into(),
                    label: format!("함께한 지 {}주년", days / 365),
                });
            }
        }
    }

    // 양력 고정 명절
    for h in SOLAR_HOLIDAYS {
        if h.month == date.month()
            && h.day == date.day()
            && scope_applies(h.scope, lang, include_dev_days)
        {
            if let Some(name) = name_for(h.names, lang) {
                out.push(Occasion { category: "holiday".into(), label: name });
            }
        }
    }

    // 프로그래머의 날: 연 256번째 날 (개발자 문화)
    if include_dev_days && date.ordinal() == 256 {
        out.push(Occasion { category: "holiday".into(), label: "Programmer's Day".into() });
    }

    // 음력 명절 (동아시아 로케일만)
    if is_east_asian(lang) {
        for (y, m, dd, fest) in LUNAR_HOLIDAYS {
            if *y == date.year() && *m == date.month() && *dd == date.day() {
                out.push(Occasion { category: "holiday".into(), label: lunar_name(*fest, lang) });
            }
        }
    }

    out
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test --lib occasions`
Expected: PASS (5 tests). 전체 `cargo test`도 그린 유지.

- [ ] **Step 6: (데이터 확장) 음력 테이블 2028–2035 채우기**

`LUNAR_HOLIDAYS`에 2028–2035년 설날·추석 양력 날짜를 **KASI 공식 음양력 대조표**(astro.kasi.re.kr)에서 확인해 추가한다. 이미 있는 2025–2027 검증 앵커(테스트가 고정)를 깨지 말 것. 확장 후 `cargo test --lib occasions` 재실행해 그린 확인.

- [ ] **Step 7: Commit**

```bash
git add src/diary/occasions.rs src/diary/mod.rs
git commit -m "feat: add deterministic occasions module (holidays + anniversaries)"
```

---

## Task 2: Finding 근거·개선방향 (finding_advice + BriefFinding 확장)

**Files:**
- Modify: `src/diary/mod.rs` (`BriefFinding`에 필드 2개, `finding_advice` 함수, `assemble_brief`의 finding 매핑)

**Interfaces:**
- Consumes: `serde_json::Value`
- Produces:
  - `BriefFinding`에 `pub detail: String`, `pub suggested_action: String` 추가.
  - `pub fn finding_advice(rule_id: &str, evidence: &serde_json::Value, est_tokens_saved: u64) -> (String, String)`

- [ ] **Step 1: 실패하는 테스트 작성**

`src/diary/mod.rs`의 `#[cfg(test)] mod tests`에 추가:

```rust
    #[test]
    fn finding_advice_r5_and_r1() {
        let (detail, action) = super::finding_advice(
            "R5",
            &serde_json::json!({"path": "report.xlsx", "count": 7}),
            7200,
        );
        assert!(detail.contains("report.xlsx"));
        assert!(detail.contains("7"));
        assert!(detail.contains("7200"));
        assert!(!action.is_empty());

        let (d1, a1) = super::finding_advice(
            "R1",
            &serde_json::json!({"server": "playwright"}),
            2500,
        );
        assert!(d1.contains("playwright"));
        assert!(a1.contains("playwright"));
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib diary::tests::finding_advice`
Expected: FAIL — `finding_advice` 미정의.

- [ ] **Step 3: `BriefFinding` 필드 추가**

`src/diary/mod.rs`의 `BriefFinding` 정의를 교체:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct BriefFinding {
    pub rule_id: String,
    pub severity: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
    pub detail: String,
    pub suggested_action: String,
}
```

- [ ] **Step 4: `finding_advice` 구현**

`src/diary/mod.rs`의 `assemble_brief` 위(또는 파일 상단 함수 영역)에 추가:

```rust
/// rule_id + evidence에서 사람이 읽는 근거(detail)와 개선방향(suggested_action)을 결정론적으로 생성.
/// 정밀도의 선: 여기서 만든 사실만 서사에 인용된다.
pub fn finding_advice(
    rule_id: &str,
    evidence: &serde_json::Value,
    est_tokens_saved: u64,
) -> (String, String) {
    match rule_id {
        "R5" => {
            let path = evidence.get("path").and_then(|v| v.as_str()).unwrap_or("어떤 파일");
            let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("`{path}`를 {count}회 반복해서 읽음 (~{est_tokens_saved}토큰)"),
                "한 번만 읽고 그 내용을 기억해 두면 다음엔 그 토큰을 아낄 수 있어요".to_string(),
            )
        }
        "R1" => {
            let server = evidence.get("server").and_then(|v| v.as_str()).unwrap_or("어떤 서버");
            (
                format!("MCP 서버 `{server}`가 상주하는데 호출 0회 (~{est_tokens_saved}토큰 추정)"),
                format!("안 쓰는 `{server}`를 설정에서 제거하면 매 세션 상주 토큰을 아껴요"),
            )
        }
        _ => (format!("{evidence}"), String::new()),
    }
}
```

- [ ] **Step 5: `assemble_brief`의 finding 매핑에 detail/suggested_action 채우기**

`src/diary/mod.rs`의 `assemble_brief` 안에서 findings를 `BriefFinding`으로 만드는 `.map(|f| BriefFinding { ... })` 클로저를 교체:

```rust
    let findings = store
        .findings_for_date(date)?
        .into_iter()
        .map(|f| {
            let (detail, suggested_action) = finding_advice(&f.rule_id, &f.evidence, f.est_tokens_saved);
            BriefFinding {
                rule_id: f.rule_id,
                severity: f.severity.as_str().to_string(),
                evidence: f.evidence,
                est_tokens_saved: f.est_tokens_saved,
                prescription: f.prescription.map(|p| serde_json::json!({
                    "kind": p.kind, "payload": p.payload
                })),
                detail,
                suggested_action,
            }
        })
        .collect();
```

> 주의: `finding_advice`는 `f.evidence`를 참조하고 이후 `BriefFinding.evidence: f.evidence`가 소유권을 가져가므로, 위 코드처럼 advice를 **먼저** 계산한 뒤 구조체를 만든다.

- [ ] **Step 6: 테스트 통과 확인**

Run: `cargo test --lib diary`
Expected: PASS (기존 diary 테스트 + `finding_advice`). 전체 `cargo test` 그린.

- [ ] **Step 7: Commit**

```bash
git add src/diary/mod.rs
git commit -m "feat: enrich brief findings with deterministic detail and suggested_action"
```

---

## Task 3: Locale + Config + anchor + Brief.occasions

**Files:**
- Modify: `Cargo.toml` (`sys-locale` 의존성)
- Modify: `src/store.rs` (`earliest_session_ts`)
- Modify: `src/diary/mod.rs` (`DiaryConfig` 필드, `resolve_locale`, `Brief.occasions`, `assemble_brief` 시그니처+occasions)

**Interfaces:**
- Consumes: `occasions::{Occasion, compute_occasions}`(Task 1), `finding_advice`(Task 2), `chrono::NaiveDate`
- Produces:
  - `Brief`에 `pub occasions: Vec<Occasion>`.
  - `DiaryConfig`에 `pub locale: Option<String>`, `pub include_dev_days: bool`. Default: `locale: None`, `include_dev_days: true`.
  - `pub fn resolve_locale(cfg: &DiaryConfig) -> String`
  - `SqliteStore::earliest_session_ts(&self) -> Result<Option<String>>`
  - `assemble_brief` 시그니처 변경: `assemble_brief(store: &SqliteStore, host: &str, date: &str, cfg: &DiaryConfig) -> Result<Brief>`

- [ ] **Step 1: `sys-locale` 의존성 추가**

`Cargo.toml`의 `[dependencies]`에 추가:

```toml
sys-locale = "0.3"
```

- [ ] **Step 2: 실패하는 테스트 작성**

`src/store.rs`의 `#[cfg(test)] mod tests`에 추가:

```rust
    #[test]
    fn earliest_session_ts_returns_min_first_ts() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sid: &str, uuid: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sid.into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        };
        store.upsert_events(&[
            mk("s2", "u2", "2026-03-10T09:00:00Z"),
            mk("s1", "u1", "2026-01-05T09:00:00Z"),
        ]).unwrap();
        assert_eq!(store.earliest_session_ts().unwrap().as_deref(), Some("2026-01-05T09:00:00Z"));
    }
```

그리고 `src/diary/mod.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn assemble_brief_includes_occasions_from_anchor() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        // 첫 세션 = 2026-01-01 → anchor
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-01-01T09:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.rebuild_rollup().unwrap();

        let cfg = DiaryConfig { locale: Some("ko-KR".into()), ..DiaryConfig::default() };
        // 2026-04-11 = 2026-01-01 + 100일
        let brief = assemble_brief(&store, "Windows", "2026-04-11", &cfg).unwrap();
        assert!(brief.occasions.iter().any(|o| o.label == "함께한 지 100일"));
    }
```

- [ ] **Step 3: 테스트 실패 확인**

Run: `cargo test --lib store::tests::earliest_session_ts` 및 `cargo test --lib diary::tests::assemble_brief_includes_occasions`
Expected: FAIL — `earliest_session_ts` 미정의 / `assemble_brief` 인자 개수 불일치 / `Brief.occasions` 없음.

- [ ] **Step 4: `earliest_session_ts` 구현**

`src/store.rs`의 `impl SqliteStore` 안에 추가:

```rust
    pub fn earliest_session_ts(&self) -> Result<Option<String>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT MIN(first_ts) FROM sessions", [], |r| r.get(0))
            .ok()
            .flatten();
        Ok(v)
    }
```

> `MIN(first_ts)`는 행이 없으면 NULL 1행을 돌려주므로 `get::<_, Option<String>>`가 `None`을 담는다. `.ok().flatten()`으로 처리.

- [ ] **Step 5: `DiaryConfig` 확장 + `resolve_locale`**

`src/diary/mod.rs`의 `DiaryConfig`와 `Default`를 교체:

```rust
#[derive(Debug, Clone)]
pub struct DiaryConfig {
    pub vault_dir: PathBuf,
    pub tone: String,
    pub honorific: String,
    pub locale: Option<String>,
    pub include_dev_days: bool,
}

impl Default for DiaryConfig {
    fn default() -> Self {
        DiaryConfig {
            vault_dir: PathBuf::from("./diary"),
            tone: "B".to_string(),
            honorific: "주인".to_string(),
            locale: None,
            include_dev_days: true,
        }
    }
}

/// cfg.locale이 있으면 사용, 없으면 OS 로케일 자동 감지, 그것도 실패하면 "en".
pub fn resolve_locale(cfg: &DiaryConfig) -> String {
    cfg.locale
        .clone()
        .or_else(sys_locale::get_locale)
        .unwrap_or_else(|| "en".to_string())
}
```

- [ ] **Step 6: `Brief.occasions` 필드 + `assemble_brief` 시그니처/구현**

`src/diary/mod.rs` 상단 import에 추가: `use crate::diary::occasions::{compute_occasions, Occasion};` 및 `use chrono::NaiveDate;`.

`Brief` 정의 교체:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
    pub occasions: Vec<Occasion>,
}
```

`assemble_brief` 시그니처를 `date: &str` 다음에 `cfg: &DiaryConfig`를 받도록 바꾸고, 함수 끝에서 occasions를 계산해 `Brief`에 싣는다. 함수 끝부분(`Ok(Brief { ... })`) 직전에:

```rust
    let locale = resolve_locale(cfg);
    let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
    let anchor = store
        .earliest_session_ts()?
        .and_then(|ts| NaiveDate::parse_from_str(&ts[..10.min(ts.len())], "%Y-%m-%d").ok());
    let occasions = match today {
        Some(d) => compute_occasions(d, anchor, &locale, cfg.include_dev_days),
        None => Vec::new(),
    };

    Ok(Brief { date: date.to_string(), host: host.to_string(), totals, findings, occasions })
```

그리고 함수 시그니처 줄을 교체:

```rust
pub fn assemble_brief(
    store: &SqliteStore,
    host: &str,
    date: &str,
    cfg: &DiaryConfig,
) -> Result<Brief> {
```

- [ ] **Step 7: 기존 `assemble_brief` 호출부/테스트 갱신**

`src/diary/mod.rs`의 기존 테스트 `assemble_brief_collects_findings_and_totals`에서 호출을 `assemble_brief(&store, "Windows", "2026-07-01", &DiaryConfig::default())`로 바꾸고, 그 테스트의 `Brief` 사용 부분은 그대로 둔다(occasions 필드는 무시).

- [ ] **Step 8: 테스트 통과 확인**

Run: `cargo test --lib store` 및 `cargo test --lib diary`
Expected: PASS. 전체 `cargo test` 그린. (main.rs는 아직 옛 시그니처로 호출 → Task 5에서 갱신하므로, 이 시점에 `cargo build --lib`는 통과하나 `cargo build`(바이너리 포함)는 실패할 수 있다. 그렇다면 Step 9에서 임시로 main.rs 호출만 맞춰도 되고, Task 5에서 정식 처리한다. 최소 통과 기준은 `cargo test --lib` 그린.)

> main.rs 컴파일을 즉시 통과시키려면 `src/main.rs`의 `assemble_brief(&store, "Windows", &date)` 호출을 `assemble_brief(&store, "Windows", &date, &cfg)`로 바꾸되, `cfg`는 이미 그 함수(`cmd_diary`)에서 `DiaryConfig::default()`로 만들어진다(호출 순서만 cfg 생성 뒤로). Task 5에서 최종 정리.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/store.rs src/diary/mod.rs src/main.rs
git commit -m "feat: locale resolution, session anchor, and occasions in the brief"
```

---

## Task 4: 프롬프트 강화 (유머 + 근거 + occasions)

**Files:**
- Modify: `src/diary/mod.rs` (`build_system_prompt`)

**Interfaces:**
- Consumes: `DiaryConfig`
- Produces: `build_system_prompt(cfg: &DiaryConfig) -> String` (시그니처 불변, 내용 강화)

- [ ] **Step 1: 실패하는 테스트 작성**

`src/diary/mod.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn system_prompt_has_humor_evidence_and_occasions_instructions() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("주인"));   // 호칭
        assert!(p.contains("유머"));   // 유머 지시
        assert!(p.contains("detail")); // 근거 필드 사용 지시
        assert!(p.contains("suggested_action")); // 개선방향 필드 사용 지시
        assert!(p.contains("occasions")); // 기념일/명절 사용 지시
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --lib diary::tests::system_prompt_has_humor`
Expected: FAIL — 현재 프롬프트에 "유머"/"detail"/"occasions" 문자열 없음.

- [ ] **Step 3: `build_system_prompt` 교체**

`src/diary/mod.rs`의 `build_system_prompt`를 교체:

```rust
pub fn build_system_prompt(cfg: &DiaryConfig) -> String {
    format!(
        "당신은 사용자의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         1인칭으로 하루를 회고하는 일기를 씁니다. 사용자를 '{honorific}'이라고 부릅니다. \
         톤 프리셋은 '{tone}'(A=감성, B=균형, C=분석)이며, 톤과 무관하게 기본적으로 \
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
         브리프에 없는 구체적 수치를 지어내지 마세요. \
         '잘한 것'과 '아쉬운 것'은 각 finding의 `detail`(근거 수치)과 `suggested_action`(개선 방향)에 \
         근거해 구체적으로 써서, 무엇을 왜 그렇게 하면 좋은지 주인이 바로 알 수 있게 하세요. \
         자유로운 소감은 서사에만 담고 행동 지시로 승격하지 마세요. \
         \
         브리프의 `occasions` 배열이 비어있지 않으면(기념일·명절), 일기의 도입이나 마무리에 \
         자연스럽고 다정하게 언급하세요(예: 오늘이 크리스마스이거나 함께한 지 100일 등). \
         비어있으면 언급하지 마세요.",
        honorific = cfg.honorific,
        tone = cfg.tone,
    )
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib diary`
Expected: PASS. 전체 `cargo test` 그린.

- [ ] **Step 5: Commit**

```bash
git add src/diary/mod.rs
git commit -m "feat: humor, evidence-grounded advice, and occasions in diary system prompt"
```

---

## Task 5: CLI 배선 + 실측/실엔진 검증

**Files:**
- Modify: `src/main.rs` (`cmd_diary`가 cfg를 `assemble_brief`에 전달)

**Interfaces:**
- Consumes: 모든 이전 태스크.
- Produces: `agent-mentor diary [date]`가 occasions·근거·유머를 반영한 일기를 생성.

- [ ] **Step 1: `cmd_diary` 갱신**

`src/main.rs`의 `cmd_diary`에서 `DiaryConfig`를 먼저 만들고 `assemble_brief`에 넘기도록 확인/수정. 핵심 두 줄:

```rust
    let cfg = DiaryConfig::default();
    let brief = assemble_brief(store, "Windows", &date, &cfg)?;
```

(엔진 선택·`generate_diary(store, engine.as_ref(), &brief, &cfg)` 호출은 기존과 동일. `date`는 인자 없으면 오늘 UTC.)

- [ ] **Step 2: 빌드 + 전체 테스트**

Run: `cargo build && cargo test`
Expected: OK, 전체 테스트 그린(경고 0 지향).

- [ ] **Step 3: 실측 재수집 + 규칙 (근거 확인용)**

Run:
```
rm -f ./agent-mentor.db
cargo run -- ingest
cargo run -- rules
```
Expected: R5/R1 Finding 출력. (이 DB가 다음 스텝 다이어리의 findings 원천.)

- [ ] **Step 4: occasion을 강제로 맞춘 다이어리 생성 (Mock)**

앵커(첫 세션 날짜)를 확인하고 그로부터 100일째 날짜로 다이어리를 생성해 마일스톤이 뜨는지 본다:
```
# 앵커 확인
sqlite3 ./agent-mentor.db "SELECT date(MIN(first_ts)) FROM sessions;"
# 위 결과 + 100일 = <milestone-date> 를 계산해:
cargo run -- diary <milestone-date>
```
Expected: 생성된 `./diary/<milestone-date>.md`에 "함께한 지 100일" 취지의 서사가 포함. (엔진 미설정 시 Mock이지만, 브리프 JSON에 occasion이 실려 Mock도 세션 수 등은 반영. Mock 서사는 단순하므로 occasion 반영 확인은 실엔진 스텝에서 확실히.)

크리스마스 등 고정 명절도 확인: `cargo run -- diary 2026-12-25` → 브리프 occasions에 "크리스마스" 포함(그날 findings/토큰이 없어도 occasion은 뜸).

- [ ] **Step 5: 실제 엔진으로 검증 (수동)**

`.env.ref`(gitignored)의 자격증명을 매핑해 실엔진으로 생성. `host.docker.internal`이 네이티브에서 미해석이면 `localhost`로 대체:
```
export AGENT_MENTOR_ENGINE_URL="$(grep -E '^LLM_BASE_URL=' .env.ref | cut -d= -f2- | sed 's#host.docker.internal#localhost#')"
export AGENT_MENTOR_ENGINE_KEY="$(grep -E '^LLM_API_KEY=' .env.ref | cut -d= -f2-)"
export AGENT_MENTOR_ENGINE_MODEL="$(grep -E '^LLM_MODEL=' .env.ref | cut -d= -f2-)"
cargo run -- diary <milestone-date>
```
Expected(검증 포인트):
- 서사가 이전보다 **유머러스**함(다마고치풍 능청).
- '잘한 것/아쉬운 것'이 finding의 **근거 수치 + 개선방향**을 구체적으로 인용(예: "report.xlsx를 7번이나… 한 번만 읽으면…").
- occasion("함께한 지 100일" 또는 명절)이 도입/마무리에 자연스럽게 등장.
- 토큰 자기계량 푸터 유지. API 키는 출력하지 말 것.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire diary CLI to pass config (locale, dev-days) into brief assembly"
```

---

## Spec Coverage Map (self-review)

| 스펙 요구 | 태스크 |
|---|---|
| §1.1 occasions 결정론적, 브리프로 전달 | Task 1 + Task 3 |
| §1.2 기준일 = 첫 세션 자동 | Task 3 (earliest_session_ts) |
| §1.3 마일스톤 50+100단위+주년 | Task 1 (compute_occasions) |
| §1.4 로케일 인지+자동감지 | Task 3 (resolve_locale, sys-locale) |
| §1.5 음력 번들 테이블·로케일별 이름·graceful degrade | Task 1 (LUNAR_HOLIDAYS, lunar_name) |
| §1.6 유머 기본상향, 새 톤 없음 | Task 4 (build_system_prompt) |
| §1.7 근거/개선방향 규칙 파생 | Task 2 (finding_advice) |
| §1.8 i18n 후속(로케일 1급 필드) | Task 3 (locale 필드 유지, 서사 언어는 불변) |
| §2 Occasion 모델·compute_occasions | Task 1 |
| §3 Locale 접두어·적용성 | Task 1 (scope_applies, is_east_asian) + Task 3 |
| §4 Holiday catalog(공통/동아시아/개발자/음력) | Task 1 |
| §5.1 유머 | Task 4 |
| §5.2 detail/suggested_action | Task 2 |
| §5.3 occasions 주입 | Task 4 (프롬프트) + Task 3 (브리프 데이터) |
| §6 파일 구조 | Task 1–5 |
| §7 테스트 전략 | 각 태스크 TDD + Task 5 수동 |

**명시적 유예(스펙 §8):** 다중언어 서사 출력, 국가별 국경일 전수, 음력 테이블 2035+ 자동확장.

## Notes for the implementer
- 빌드는 항상 Global Constraints의 env 블록을 먼저 export하고 Git Bash에서 실행.
- Task 3에서 `assemble_brief` 시그니처가 바뀌므로 main.rs가 잠시 안 맞을 수 있다. `cargo test --lib`로 라이브러리 그린을 확인하고, main.rs는 Task 3 Step 8 노트대로 즉시 맞추거나 Task 5에서 정리한다(둘 중 하나로 항상 `cargo build` 그린을 회복할 것).
- occasion label은 이미 로케일 지역화 완료 상태로 브리프에 실린다. 이번 페이즈 서사 언어는 한국어(페르소나)라, 예컨대 en 로케일이면 "Christmas"라는 영어 명절명이 한국어 서사에 섞일 수 있다 — 의도된 절충(i18n은 후속). 
- 음력 테이블 확장(Task 1 Step 6)은 KASI 공식 값만 쓸 것. 임의 계산·추정 금지.
