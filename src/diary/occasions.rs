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
