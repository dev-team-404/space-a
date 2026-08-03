//! 로컬 공지 소스 — Claude Code가 `~/.claude.json`에 캐시해 둔 시작 공지 (스펙 §6.1).
//!
//! 네트워크 0. 시작 공지는 `maxImpressions`만큼만 스쳐 지나가고 사라지므로(실측:
//! `fable-5-promo-2-4-team`은 상한 5인데 이미 10회 노출) 사용자는 더 이상 볼 수 없다.
//! A-Mate가 이걸 모아두는 자리다.
//!
//! **관대한 파싱이 필수다(§6.6).** `tengu_startup_announcements`는 비공식 내부
//! 캐시(GrowthBook 피처 플래그)라 스키마가 예고 없이 바뀔 수 있다. 필드 누락·타입
//! 불일치는 **그 항목만 skip**, 경로가 통째로 사라지면 소식 카드만 침묵한다.

use crate::content::{ContentItem, ItemKind};
use serde_json::Value;

/// 로컬 공지 1건. `title`은 실측상 없는 항목이 있다(`opus-5-launch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAnnouncement {
    pub id: String,
    pub title: Option<String>,
    pub text: String,
    pub priority: i64,
    /// `announcementImpressions`의 노출 횟수 — **참고 신호로만** 저장한다.
    /// 노출 소진 여부와 무관하게 카드는 만든다(소진된 공지야말로 모아둘 값이 있다).
    pub impressions: Option<i64>,
    /// `lastShownEmergencyTip` 유래 — 배지가 「공지」로 갈린다.
    pub emergency: bool,
}

/// 카드 식별 태그 — 프룬·섹션 분리·번역 대상 판정이 전부 이 태그를 본다.
pub const TAG_ANNOUNCEMENT: &str = "announcement";
/// 긴급 팁 전용 태그 — 프론트 배지를 「공지」로 가른다 (스펙 §4.2·§6.1).
pub const TAG_NOTICE: &str = "notice";

const ID_PREFIX: &str = "cc-announce-";

fn nonempty_str(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// `cachedGrowthBookFeatures.tengu_startup_announcements` 배열을 관대하게 읽는다.
/// `id`·`text`가 비어 있지 않은 문자열인 항목만 취하고, 나머지는 조용히 skip.
pub fn parse_announcements(claude_json: &Value) -> Vec<LocalAnnouncement> {
    let impressions = claude_json.get("announcementImpressions");
    let Some(arr) = claude_json
        .get("cachedGrowthBookFeatures")
        .and_then(|f| f.get("tengu_startup_announcements"))
        .and_then(|a| a.as_array())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let id = nonempty_str(item.get("id"))?;
            let text = nonempty_str(item.get("text"))?;
            Some(LocalAnnouncement {
                impressions: impressions.and_then(|m| m.get(&id)).and_then(|v| v.as_i64()),
                id,
                title: nonempty_str(item.get("title")),
                text,
                // 타입이 어긋나면 0 — 항목을 버리진 않는다(정렬 순서만 잃는다).
                priority: item.get("priority").and_then(|p| p.as_i64()).unwrap_or(0),
                emergency: false,
            })
        })
        .collect()
}

/// `lastShownEmergencyTip`은 배열이 아니라 단일 문자열이다 — 별도 항목으로 취급한다.
/// id는 본문 해시라 문구가 바뀌면 새 공지가 된다(같은 문구는 한 번만 알린다).
pub fn parse_emergency_tip(claude_json: &Value) -> Option<LocalAnnouncement> {
    let text = nonempty_str(claude_json.get("lastShownEmergencyTip"))?;
    Some(LocalAnnouncement {
        id: format!("emergency-{}", hash8(&text)),
        title: None,
        text,
        // 장애 공지는 프로모보다 위 — 같은 소식 묶음 안에서만 앞선다.
        priority: 5,
        impressions: None,
        emergency: true,
    })
}

fn hash8(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}

/// 호스트별 공지를 하나로 — `id` dedup(먼저 온 호스트 우선), `priority` 내림차순.
/// 동점은 id 오름차순으로 안정화한다(Windows·WSL 순서가 흔들려도 카드 순서는 고정).
pub fn merge_announcements(per_host: Vec<Vec<LocalAnnouncement>>) -> Vec<LocalAnnouncement> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<LocalAnnouncement> = Vec::new();
    for host in per_host {
        for a in host {
            if seen.insert(a.id.clone()) {
                out.push(a);
            }
        }
    }
    out.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    out
}

/// 본문 끝의 `Learn more: <url>`(또는 맨 뒤 URL)을 떼어 (본문, URL)로 가른다.
/// 문법 B의 주 CTA가 「전문 보기 →」라 URL이 없으면 CTA 없는 카드가 되고,
/// 본문에 URL이 남으면 CTA와 중복된다 (계획 D6).
pub fn split_learn_more(text: &str) -> (String, Option<String>) {
    let Some(pos) = text.rfind("http://").or_else(|| text.rfind("https://")) else {
        return (text.trim().to_string(), None);
    };
    let url = text[pos..].split_whitespace().next().unwrap_or("").trim_end_matches(['.', ',']);
    if url.len() <= "https://".len() {
        return (text.trim().to_string(), None);
    }
    // URL 앞의 안내 문구("Learn more:", "자세히:")까지 같이 떼어낸다 — 남으면 문장이 끊긴다.
    let head = text[..pos].trim_end();
    let head = match head.rfind(['.', '!', '?', '\n']) {
        Some(end) if head[end + 1..].trim().ends_with(':') => head[..=end].trim_end(),
        _ => head.trim_end_matches(':').trim_end(),
    };
    (head.to_string(), Some(url.to_string()))
}

/// 제목이 없는 공지의 폴백 — 본문 첫 문장(최대 60자).
fn fallback_title(text: &str) -> String {
    let first = text.split_terminator(['.', '\n']).next().unwrap_or(text).trim();
    let base = if first.is_empty() { text.trim() } else { first };
    if base.chars().count() > 60 {
        format!("{}…", base.chars().take(60).collect::<String>().trim_end())
    } else {
        base.to_string()
    }
}

/// 공지 → 큐레이션 아이템. 번역은 하지 않는다(파이프라인의 별도 스텝, 스펙 §6.3) —
/// 엔진이 없으면 이 원문이 그대로 노출된다.
pub fn announcement_items(anns: &[LocalAnnouncement]) -> Vec<ContentItem> {
    anns.iter()
        .map(|a| {
            let (body, url) = split_learn_more(&a.text);
            let mut trigger_tags = vec![TAG_ANNOUNCEMENT.to_string()];
            if a.emergency {
                trigger_tags.push(TAG_NOTICE.to_string());
            }
            ContentItem {
                id: format!("{ID_PREFIX}{}", a.id),
                kind: ItemKind::News,
                title: a.title.clone().unwrap_or_else(|| fallback_title(&body)),
                body,
                source_url: url,
                dimension: None, // 소식은 축 무관 — 마스터 억제·축 쿨다운 대상이 아니다
                trigger_tags,
                base_priority: a.priority,
            }
        })
        .collect()
}

/// 가장자리 — 전 호스트의 `claude.json`을 읽어 공지를 모은다. **네트워크 0, 락 밖에서 호출**.
/// 읽기 실패·파싱 실패는 그 호스트만 빈 목록(하드 에러 금지, §6.6).
pub fn collect_local_announcements() -> Vec<LocalAnnouncement> {
    let per_host = crate::hosts::enumerate_hosts()
        .iter()
        .map(|hs| {
            let Some(json) = crate::ops::read_json_guarded(&hs.claude_json()) else {
                return Vec::new();
            };
            let mut v = parse_announcements(&json);
            v.extend(parse_emergency_tip(&json));
            v
        })
        .collect();
    merge_announcements(per_host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 2026-08-02 실측 형태 (필드 이름·중첩 경로 고정).
    fn real_shape() -> Value {
        json!({
            "cachedGrowthBookFeatures": {
                "tengu_startup_announcements": [
                    {
                        "id": "fable-5-promo-2-4-team",
                        "title": "Fable 5 is now a standard part of your Team plan",
                        "text": "You can use up to 50% of your weekly usage limit on Fable 5. Run /model and select Fable to use it. Learn more: https://support.claude.com/en/articles/15424964",
                        "maxImpressions": 5,
                        "priority": 1
                    },
                    {
                        "id": "opus-5-launch",
                        "text": "Tackle your toughest work with Opus 5. Switch anytime with /model.",
                        "maxImpressions": 3,
                        "priority": 0,
                        "accentBar": false
                    }
                ]
            },
            "announcementImpressions": { "fable-5-promo-2-4-team": 10, "opus-5-launch": 3 },
            "lastShownEmergencyTip": "Claude Fable 5 is currently unavailable. Please use Opus 4.8."
        })
    }

    #[test]
    fn parses_real_shape_with_impressions_and_optional_title() {
        let got = parse_announcements(&real_shape());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "fable-5-promo-2-4-team");
        assert_eq!(got[0].priority, 1);
        assert_eq!(got[0].impressions, Some(10));
        assert!(got[0].title.is_some());
        // title이 없는 항목도 버리지 않는다 (실측: opus-5-launch)
        assert_eq!(got[1].title, None);
        assert!(!got[1].emergency);
    }

    #[test]
    fn skips_items_with_missing_or_mistyped_fields_but_keeps_the_rest() {
        let v = json!({
            "cachedGrowthBookFeatures": { "tengu_startup_announcements": [
                { "title": "id 없음", "text": "버려진다" },
                { "id": "no-text" },
                { "id": 42, "text": "id가 숫자" },
                { "id": "bad-text", "text": ["배열"] },
                { "id": "  ", "text": "id가 공백" },
                { "id": "ok", "text": "살아남는다", "priority": "높음" }
            ]}
        });
        let got = parse_announcements(&v);
        assert_eq!(got.len(), 1, "불량 항목만 skip되고 나머지는 남는다: {got:?}");
        assert_eq!(got[0].id, "ok");
        // priority 타입 불일치는 항목을 버리지 않는다 — 정렬 순서만 기본값으로 떨어진다
        assert_eq!(got[0].priority, 0);
    }

    #[test]
    fn missing_path_or_wrong_container_type_yields_empty() {
        assert!(parse_announcements(&json!({})).is_empty());
        assert!(parse_announcements(&Value::Null).is_empty());
        assert!(parse_announcements(&json!({"cachedGrowthBookFeatures": {}})).is_empty());
        assert!(parse_announcements(
            &json!({"cachedGrowthBookFeatures": {"tengu_startup_announcements": "문자열"}})
        )
        .is_empty());
        assert!(parse_emergency_tip(&json!({})).is_none());
        assert!(parse_emergency_tip(&json!({"lastShownEmergencyTip": ""})).is_none());
    }

    #[test]
    fn emergency_tip_is_a_separate_item_keyed_by_its_text() {
        let a = parse_emergency_tip(&real_shape()).unwrap();
        assert!(a.emergency);
        assert!(a.id.starts_with("emergency-"));
        // 같은 문구는 같은 id (한 번만 알린다), 다른 문구는 다른 id
        let b = parse_emergency_tip(&real_shape()).unwrap();
        assert_eq!(a.id, b.id);
        let c = parse_emergency_tip(&json!({"lastShownEmergencyTip": "다른 장애"})).unwrap();
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn merge_dedups_by_id_across_hosts_and_sorts_by_priority_desc() {
        let win = parse_announcements(&real_shape());
        let wsl = parse_announcements(&real_shape()); // 같은 공지가 양쪽에
        let merged = merge_announcements(vec![win, wsl]);
        assert_eq!(merged.len(), 2, "호스트 간 id dedup");
        assert_eq!(merged[0].id, "fable-5-promo-2-4-team", "priority 1 먼저");
        assert_eq!(merged[1].id, "opus-5-launch");
    }

    #[test]
    fn merge_is_stable_for_equal_priority() {
        let mk = |id: &str, p: i64| LocalAnnouncement {
            id: id.into(), title: None, text: "t".into(), priority: p,
            impressions: None, emergency: false,
        };
        let merged = merge_announcements(vec![vec![mk("b", 0), mk("a", 0), mk("c", 2)]]);
        let ids: Vec<&str> = merged.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"], "동점은 id 오름차순으로 고정");
    }

    #[test]
    fn learn_more_url_is_split_out_of_the_body() {
        let (body, url) = split_learn_more(
            "Run /model and select Fable to use it. Learn more: https://support.claude.com/x",
        );
        assert_eq!(url.as_deref(), Some("https://support.claude.com/x"));
        assert!(!body.contains("http"), "본문에 URL이 남지 않는다: {body}");
        assert!(!body.contains("Learn more"), "안내 문구도 함께 떨어진다: {body}");
        assert!(body.ends_with("use it."));
        // URL이 없는 본문은 그대로
        let (b2, u2) = split_learn_more("Switch anytime with /model.");
        assert_eq!(b2, "Switch anytime with /model.");
        assert_eq!(u2, None);
    }

    #[test]
    fn items_carry_announcement_tag_and_news_kind() {
        let anns = merge_announcements(vec![{
            let mut v = parse_announcements(&real_shape());
            v.extend(parse_emergency_tip(&real_shape()));
            v
        }]);
        let items = announcement_items(&anns);
        assert_eq!(items.len(), 3);
        for it in &items {
            assert!(matches!(it.kind, ItemKind::News));
            assert!(it.trigger_tags.iter().any(|t| t == TAG_ANNOUNCEMENT));
            assert!(it.dimension.is_none(), "소식은 축 무관 — 축 쿨다운 대상 아님");
            assert!(!it.title.trim().is_empty(), "제목 없는 카드는 만들지 않는다");
        }
        // 긴급 팁만 「공지」 배지 태그를 갖는다
        let notice: Vec<_> = items.iter().filter(|i| i.trigger_tags.iter().any(|t| t == TAG_NOTICE)).collect();
        assert_eq!(notice.len(), 1);
        // 제목 없는 공지는 본문 첫 문장으로 채운다
        let opus = items.iter().find(|i| i.id.ends_with("opus-5-launch")).unwrap();
        assert_eq!(opus.title, "Tackle your toughest work with Opus 5");
    }
}
