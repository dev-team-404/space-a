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
    // 2028–2035: verified from time.is/calendar/{year}/South_Korea (sourcing official KR public holidays)
    (2028, 1, 27, LunarFestival::LunarNewYear), (2028, 10, 3, LunarFestival::MidAutumn),
    (2029, 2, 13, LunarFestival::LunarNewYear), (2029, 9, 22, LunarFestival::MidAutumn),
    (2030, 2, 3, LunarFestival::LunarNewYear),  (2030, 9, 12, LunarFestival::MidAutumn),
    (2031, 1, 23, LunarFestival::LunarNewYear), (2031, 10, 1, LunarFestival::MidAutumn),
    (2032, 2, 11, LunarFestival::LunarNewYear), (2032, 9, 19, LunarFestival::MidAutumn),
    (2033, 1, 31, LunarFestival::LunarNewYear), (2033, 9, 8, LunarFestival::MidAutumn),
    (2034, 2, 19, LunarFestival::LunarNewYear), (2034, 9, 27, LunarFestival::MidAutumn),
    (2035, 2, 8, LunarFestival::LunarNewYear),  (2035, 9, 16, LunarFestival::MidAutumn),
];

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
    // BCP-47 언어 태그는 대소문자 무관 — 소문자로 정규화해 매칭 실패를 방지
    let lang_normalized = lang_of(locale).to_lowercase();
    let lang = lang_normalized.as_str();
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
    fn locale_matching_is_case_insensitive() {
        // BCP-47은 대소문자 무관 — 대문자 로케일도 동아시아/로케일 명절이 매칭돼야 함
        assert!(labels(&compute_occasions(d(2025, 1, 29), None, "KO-KR", false))
            .contains(&"설날".to_string()));
        assert!(labels(&compute_occasions(d(2026, 12, 25), None, "KO", false))
            .contains(&"크리스마스".to_string()));
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

    #[test]
    fn lunar_2028_and_2035_pinned() {
        // 값은 LUNAR_HOLIDAYS 상수에서 읽어 고정 — 미래 테이블 수정 시 오타 조기 검출용
        assert!(labels(&compute_occasions(d(2028, 1, 27), None, "ko", false))
            .contains(&"설날".to_string()));
        assert!(labels(&compute_occasions(d(2035, 9, 16), None, "ko", false))
            .contains(&"추석".to_string()));
    }

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
}
