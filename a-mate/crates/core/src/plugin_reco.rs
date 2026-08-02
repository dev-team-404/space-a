//! E — 공식 마켓플레이스 plugin 추천 (스펙 2026-07-22 §4 E).
//! 판정 패스(finding)가 아니라 콘텐츠 큐레이션에 얹는다: LLM은 세션의 작업 성격(work-kind)만
//! 판정해 캐시하고(src-tauri 파이프라인 별도 스텝), 추천 카드 생성은 순수 결정론 —
//! 캐시된 work-kind + 큐레이션 테이블(curation.rs) + 인벤토리 상태 + 카탈로그(② 존재 검증).
//! 불변 계약: 근거 = 사용자 프롬프트(prompt_events)뿐 / 이미 쓰는 plugin 침묵 / 실패 관대.

use crate::content::{CatalogEntry, ContentItem, ItemKind};
use crate::curation::{plugin_recos_for, work_kind_label, WORK_KINDS};
use crate::store::SqliteStore;
use anyhow::Result;

/// 관찰창(일) — R7 롤업과 동일.
pub const WORK_KIND_WINDOW_DAYS: i64 = 14;
/// 스캔당 판정 배치 상한 — 판정 패스 관행(10)과 동일.
pub const WORK_KIND_BATCH_CAP: usize = 10;
/// 같은 work-kind 세션이 이만큼 모여야 발화(저빈도 원칙, R12 min_sessions 선례). ⚠ CALIBRATE
pub const MIN_MATCHED_SESSIONS: usize = 2;

/// work-kind 판정 프롬프트 (system, user). R7Judge와 같은 증거 경계 — 사용자 요청만 본다.
pub fn work_kind_prompt(prompts: &[String]) -> (String, String) {
    let vocab = WORK_KINDS
        .iter()
        .map(|(k, l)| format!("- {k}: {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!(
        "당신은 Claude Code 세션의 작업 성격을 분류하는 심사관입니다.\n\
         사용자가 이 세션에서 시킨 작업이 아래 종류 중 어디에 해당하는지 고르세요(복수 가능).\n\
         {vocab}\n\
         사용자 요청 자체의 성격만 보세요 — 에이전트가 무엇을 어떻게 했는지는 판단 대상이 아닙니다.\n\
         명확히 해당하는 것만 고르고, **확신이 없으면 빈 배열**을 반환하세요.\n\
         아래 JSON 객체 하나만 출력(코드펜스·사족 금지):\n\
         {{\"work_kinds\": [\"kind_key\", ...]}}"
    );
    let joined = prompts
        .iter()
        .map(|p| format!("- \"{}\"", p.replace('"', "'")))
        .collect::<Vec<_>>()
        .join("\n");
    (system, format!("사용자 요청:\n{joined}"))
}

/// verdict JSON → vocabulary 검증된 work-kind 목록(정렬·dedup). `work_kinds` 필드가 없거나
/// 배열이 아니면 None = 형식 불량(attempts 재시도 계약). 미지 kind는 조용히 버린다.
pub fn parse_work_kinds(verdict: &serde_json::Value) -> Option<Vec<String>> {
    let arr = verdict.get("work_kinds")?.as_array()?;
    let mut kinds: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|k| WORK_KINDS.iter().any(|(key, _)| key == k))
        .map(String::from)
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    Some(kinds)
}

/// 판정된 work-kind + 인벤토리 상태 → 추천 ContentItem (순수 결정론 — LLM·네트워크 없음).
/// ① 설치+enabled·미사용 → "다음엔 써봐라" / ② 미설치·카탈로그 실재 → "깔아봐라" /
/// 사용 중(스킬 호출·동명 MCP 포함) 또는 카탈로그 부재(②) → 침묵. kind당 카드 1장.
pub fn plugin_reco_items(
    store: &SqliteStore,
    catalog: &[CatalogEntry],
    now_ts: &str,
) -> Result<Vec<ContentItem>> {
    use std::collections::{BTreeMap, BTreeSet};
    let now = chrono::DateTime::parse_from_rfc3339(now_ts)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    let window_start = (now - chrono::Duration::days(WORK_KIND_WINDOW_DAYS)).to_rfc3339();

    // (host, kind) → 매칭 세션 id 목록 (judged가 최신 활동순이라 [0]이 가장 최근)
    let judged = store.judged_work_kind_sessions(&window_start)?;
    let mut by_host_kind: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for row in &judged {
        for kind in &row.kinds {
            by_host_kind
                .entry((row.host.clone(), kind.clone()))
                .or_default()
                .push(row.session_id.clone());
        }
    }

    let mut out: Vec<ContentItem> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    for ((host, kind), sessions) in by_host_kind {
        if sessions.len() < MIN_MATCHED_SESSIONS {
            continue;
        }
        let Some(label) = work_kind_label(&kind) else { continue };
        // 근거 인용: 가장 최근 매칭 세션을 연 사용자 프롬프트 한 줄 (60자 축약)
        let example: String = store
            .session_lead_prompt(&sessions[0])?
            .unwrap_or_default()
            .chars()
            .take(60)
            .collect();
        let evidence = format!(
            "당신 로그: 최근 세션 {}개가 {label} 작업이었어요 (예: \"{example}\").",
            sessions.len()
        );
        for reco in plugin_recos_for(&kind) {
            let id = format!("plugin-reco-{host}-{}", reco.plugin);
            if seen_ids.contains(&id) {
                continue; // 다른 kind가 같은 plugin을 이미 추천
            }
            if store.plugin_used_recently(&host, reco.plugin, reco.mcp_server, &window_start)? {
                continue; // 이미 사용 중 → 침묵
            }
            let installed = store.plugin_installed(&host, reco.plugin, reco.mcp_server)?;
            let (title, body, source_url) = if installed {
                // ① 설치+enabled인데 그 작업에 미사용 — 로컬 인벤토리+용도만으로 성립
                (
                    format!("다음 {label} 작업엔 `{}` 플러그인을 써보세요", reco.plugin),
                    format!(
                        "{evidence}\n설치된 `{}` 플러그인({})이 이 작업들에 쓰이지 않았어요 — \
                         다음엔 세션에서 불러 활용해 보세요.",
                        reco.plugin, reco.purpose_ko
                    ),
                    Some("https://code.claude.com/docs/en/plugins".to_string()),
                )
            } else {
                // ② 미설치 — 부재가 **확인**될 때만 (Codex 리뷰: enabled-only 인벤토리로 부재
                // 추론 금지). 설치 전수 스냅숏(enabledPlugins, disabled 포함)이 존재하고 그 안에
                // 없어야 한다. 스냅숏 부재(미스캔)·스냅숏에 있음(disabled이거나 enabled인데
                // 인벤토리 미포착) = 불확실 → 침묵. 카탈로그 실재도 필수(fetch 실패 = 침묵).
                let Some(installed_map) = store.installed_plugins_map(&host)? else { continue };
                let key_prefix = format!("{}@", reco.plugin);
                let installed_any = installed_map
                    .as_object()
                    .map(|m| m.keys().any(|k| k.starts_with(&key_prefix)))
                    .unwrap_or(false);
                if installed_any {
                    continue;
                }
                let Some(entry) = catalog.iter().find(|e| e.name == reco.plugin) else { continue };
                (
                    format!("{label} 작업에 유용한 공식 플러그인: `{}`", reco.plugin),
                    format!(
                        "{evidence}\n공식 마켓플레이스의 `{}`({})가 이런 작업을 도와줘요 — \
                         `/plugin install {}@claude-plugins-official`로 설치해 써보세요.",
                        reco.plugin, reco.purpose_ko, reco.plugin
                    ),
                    entry
                        .homepage
                        .clone()
                        .or_else(|| Some("https://code.claude.com/docs/en/plugins".to_string())),
                )
            };
            seen_ids.insert(id.clone());
            let mut trigger_tags = vec!["personal".into(), "plugin-reco".into(), kind.clone()];
            if !installed {
                // ②만 카탈로그가 있어야 만들어진다 — 마켓플레이스 fetch를 TTL로 스킵한 스캔에서
                // 프룬 면제를 받아야 하는 카드가 이것뿐이다(`ops::FEED_SOURCES`). ①은 로컬
                // 인벤토리만으로 매 스캔 재생성되므로 면제하면 "이미 사용 중이면 침묵"이 밀린다.
                trigger_tags.push("plugin-reco-catalog".into());
            }
            out.push(ContentItem {
                id,
                kind: ItemKind::Tip,
                title,
                body,
                source_url,
                dimension: None, // 개인 사건형 — 마스터 억제·축 쿨다운 미적용, dismiss는 id 영구
                trigger_tags,
                base_priority: -2, // 실측 사건 레슨(0)보단 아래, 프론티어 안내(-5)보단 위
            });
            break; // work-kind당 1장 — 첫 비침묵 후보만 (나깅 방지)
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_kind_prompt_lists_vocabulary_and_guards_evidence_boundary() {
        let (system, user) = work_kind_prompt(&["로그인 화면 버튼 스타일 다듬어줘".to_string()]);
        for (kind, _) in WORK_KINDS {
            assert!(system.contains(kind), "vocabulary {kind} 누락");
        }
        assert!(system.contains("확신이 없으면 빈 배열"), "정밀도 우선 지시");
        assert!(system.contains("에이전트가"), "증거 경계(에이전트 산출물 채점 금지) 명시");
        assert!(system.contains("work_kinds"), "JSON 스키마 명시");
        assert!(user.contains("버튼 스타일"), "사용자 프롬프트 전문 포함");
    }

    #[test]
    fn parse_work_kinds_validates_against_vocabulary() {
        // 유효 kind만 통과 + 미지 kind 조용히 드랍 + dedup
        assert_eq!(
            parse_work_kinds(&serde_json::json!({"work_kinds": ["frontend_ui", "nonsense", "frontend_ui"]})),
            Some(vec!["frontend_ui".to_string()])
        );
        // 빈 배열 = 유효한 "해당 없음" 판정 (재시도 아님)
        assert_eq!(parse_work_kinds(&serde_json::json!({"work_kinds": []})), Some(vec![]));
        // 필드 없음·배열 아님 = 형식 불량(None → attempts 재시도 계약)
        assert_eq!(parse_work_kinds(&serde_json::json!({})), None);
        assert_eq!(parse_work_kinds(&serde_json::json!({"work_kinds": "frontend_ui"})), None);
    }

    // ── 인벤토리 상태 매핑 (①②침묵 — 스펙 §4 E 표) ──

    use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage, ToolKind};
    use crate::store::SqliteStore;

    fn ev(sid: &str, uuid: &str, offset: u64, ts: &str, kind: EventKind) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: offset, msg_id: None,
            kind,
        }
    }

    /// frontend_ui로 판정된 세션 n개 시드 (프롬프트 + 턴 + work-kind 캐시)
    fn seed_frontend_sessions(store: &SqliteStore, n: usize, now: &str) {
        for i in 0..n {
            let sid = format!("fe{i}");
            store.upsert_events(&[
                ev(&sid, &format!("{sid}-p"), 0, now,
                    EventKind::UserPrompt { preview: format!("컴포넌트 {i} 스타일을 다듬어줘"), is_command: false }),
                ev(&sid, &format!("{sid}-t"), 1, now, EventKind::AssistantTurn {
                    model: NormModel::from_raw_id("claude-sonnet-4-6"),
                    usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
                }),
            ]).unwrap();
            store.set_session_work_kinds(&sid, "Windows", "d--proj", now,
                Some(&["frontend_ui".to_string()]), 1, now).unwrap();
        }
    }

    fn official_catalog() -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            name: "frontend-design".into(),
            description: "Design guidance".into(),
            category: Some("design".into()),
            homepage: Some("https://example.com/fd".into()),
        }]
    }

    fn install_frontend_design(store: &mut SqliteStore) {
        store.replace_plugin_inventory("Windows", &[crate::inventory::PluginRecord {
            plugin_key: "frontend-design@claude-plugins-official".into(),
            namespace: "frontend-design".into(), skill_count: 1, resident_tokens: 100,
            skills: vec!["frontend-design".into()], mcp_servers: vec![],
        }]).unwrap();
    }

    /// 설치 전수 스냅숏 "스캔됨·설치 0개" — ② 부재 확인 게이트 통과용
    fn mark_plugin_scan_empty(store: &SqliteStore) {
        store.set_installed_plugins("Windows", &serde_json::json!({})).unwrap();
    }

    #[test]
    fn reco_case2_not_installed_recommends_install_with_catalog() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        mark_plugin_scan_empty(&store); // 부재 확인됨(스캔됨·설치 0개)
        let items = plugin_reco_items(&store, &official_catalog(), &now).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.id, "plugin-reco-Windows-frontend-design");
        assert!(it.title.contains("frontend-design"));
        assert!(it.body.contains("/plugin install frontend-design@claude-plugins-official"),
            "② 설치 안내: {}", it.body);
        assert!(it.body.contains("당신 로그"), "실측 근거 인용: {}", it.body);
        assert!(it.body.contains("스타일을 다듬어줘"), "세션 프롬프트 예시 인용: {}", it.body);
        assert!(it.trigger_tags.iter().any(|t| t == "personal"), "personal 스코어 경로");
        assert_eq!(it.source_url.as_deref(), Some("https://example.com/fd"), "카탈로그 homepage");
        assert!(it.dimension.is_none(), "축 쿨다운·마스터 억제 미적용(개인 사건형)");
    }

    #[test]
    fn reco_case2_silent_without_catalog_entry() {
        // 카탈로그 fetch 실패(빈 벡터) 또는 목록 이탈 → ② 침묵 (관대 폴백, 스펙 §6)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        mark_plugin_scan_empty(&store);
        assert!(plugin_reco_items(&store, &[], &now).unwrap().is_empty());
    }

    #[test]
    fn reco_case2_suppressed_without_install_snapshot() {
        // 설치 전수 스냅숏(enabledPlugins)이 아직 없음 = 부재 미확인 → ② 억제 (Codex 리뷰:
        // enabled-only인 plugin_inventory로 부재를 추론하면 안 된다)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
    }

    #[test]
    fn reco_case2_suppressed_when_installed_but_not_enabled() {
        // 설치됐지만 disabled(사용자가 의도적으로 끔) → "깔아라"는 오추천 → 침묵 (Codex 리뷰)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        store.set_installed_plugins("Windows",
            &serde_json::json!({"frontend-design@claude-plugins-official": false})).unwrap();
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
        // enabled인데 스킬 인벤토리에 안 잡히는 형태(스캔 실패·MCP 전용 버전) = 불확실 → 침묵
        store.set_installed_plugins("Windows",
            &serde_json::json!({"frontend-design@claude-plugins-official": true})).unwrap();
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
    }

    #[test]
    fn reco_case1_installed_but_unused_suggests_trying_it() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        install_frontend_design(&mut store);
        // 카탈로그 없어도 ①은 성립 (로컬 인벤토리 + 큐레이션 용도로 충분 — 스펙 §4 E)
        let items = plugin_reco_items(&store, &[], &now).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.id, "plugin-reco-Windows-frontend-design");
        assert!(it.body.contains("설치된"), "① 문구: {}", it.body);
        assert!(!it.body.contains("/plugin install"), "①은 설치 안내 아님: {}", it.body);
    }

    #[test]
    fn only_catalog_dependent_recos_carry_the_prune_exemption_tag() {
        let now = chrono::Utc::now().to_rfc3339();
        // ② 미설치 추천은 카탈로그 없이 만들 수 없다 → 마켓플레이스 TTL 스킵 시 프룬 면제 대상
        let store = SqliteStore::open_in_memory().unwrap();
        seed_frontend_sessions(&store, 2, &now);
        mark_plugin_scan_empty(&store);
        let case2 = plugin_reco_items(&store, &official_catalog(), &now).unwrap();
        assert!(
            case2[0].trigger_tags.iter().any(|t| t == "plugin-reco-catalog"),
            "② 태그: {:?}", case2[0].trigger_tags
        );
        // ① 설치+미사용은 카탈로그와 무관하게 매 스캔 재생성된다. 면제받으면 "이미 사용 중이면
        // 침묵"이 TTL 창만큼 지연되므로 태그를 달지 않는다.
        let mut store = SqliteStore::open_in_memory().unwrap();
        seed_frontend_sessions(&store, 2, &now);
        install_frontend_design(&mut store);
        let case1 = plugin_reco_items(&store, &[], &now).unwrap();
        assert!(
            case1[0].trigger_tags.iter().all(|t| t != "plugin-reco-catalog"),
            "① 태그: {:?}", case1[0].trigger_tags
        );
    }

    #[test]
    fn reco_silent_when_plugin_already_used() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
        install_frontend_design(&mut store);
        // 관찰창 내 스킬 호출 → 이미 사용 중 → 침묵 (스펙 §4 E 표)
        store.upsert_events(&[ev("fe0", "fe0-skill", 2, &now, EventKind::ToolCall {
            kind: ToolKind::Skill { name: "frontend-design:frontend-design".into() },
            raw_name: "Skill".into(),
            target: Some("frontend-design:frontend-design".into()),
            tool_use_id: None,
        })]).unwrap();
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
    }

    #[test]
    fn reco_silent_below_min_matched_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 1, &now); // 1 < MIN_MATCHED_SESSIONS(2)
        mark_plugin_scan_empty(&store); // 침묵 원인이 세션 수임을 고정
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
    }

    #[test]
    fn reco_silent_without_any_verdicts() {
        // 엔진이 한 번도 안 돌았음(판정 캐시 없음) = fail-safe 침묵
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        assert!(plugin_reco_items(&store, &official_catalog(), &now).unwrap().is_empty());
    }

    #[test]
    fn reco_one_card_per_work_kind() {
        // 같은 kind 세션이 많아도 카드는 plugin당 1장·kind당 1장 (나깅 방지)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 5, &now);
        mark_plugin_scan_empty(&store);
        let items = plugin_reco_items(&store, &official_catalog(), &now).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].body.contains("5개"), "매칭 세션 수 인용: {}", items[0].body);
    }
}
