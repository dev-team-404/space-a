# E — 공식 마켓플레이스 plugin 추천 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 은퇴한 R2(미사용 plugin)·R12(미사용 skill)의 의도를 "LLM work-kind 매칭 + 인벤토리 상태 매핑 + 공식 마켓플레이스 카탈로그"로 되살려, 작업 관련성 있는 plugin 추천 카드를 기존 콘텐츠 큐레이션에 얹는다.

**Architecture:** LLM은 세션의 작업 성격(work-kind)만 판정해 캐시하고(파이프라인 별도 스텝, 판정 패스 아님), 추천 카드 생성은 순수 결정론이다 — 캐시된 work-kind + 큐레이션 상수 테이블 + 인벤토리(설치/사용) + 카탈로그(② 존재 검증)를 `run_curation`이 집계해 `content_items`로 노출한다(쿨다운·dismissal 재사용). 카탈로그는 `ClaudeChangelogSource` 형제로 가장자리에서 관대하게 fetch한다.

**Tech Stack:** Rust (crates/core + src-tauri), rusqlite(JSON1), ureq, serde_json. 프론트엔드 변경 없음(ContentRow 계약 불변).

**스펙:** [2026-07-22-coaching-value-redesign-design.md](../specs/2026-07-22-coaching-value-redesign-design.md) §4 E, §2(불변 계약), §6(테스트)

## Global Constraints

- **불변 계약(스펙 §2):** 근거 = 사용자가 직접 친 프롬프트 전문(`prompt_events` — 이미 sidechain·meta·도구결과 제외)만. 에이전트/서브에이전트 산출물 채점 금지. 판정 프롬프트에 경계 명시.
- **Fail-safe 침묵:** 엔진 미설정 → work-kind 판정 no-op. 카탈로그 fetch 실패 → ② 추천 없음(에러 아님). LLM 확신 없음 → 빈 배열(추천 없음). 절약 수치 주장 없음.
- **저빈도·나깅 방지:** `MIN_MATCHED_SESSIONS = 2`(⚠ CALIBRATE), work-kind당 카드 1장, 이미 사용 중이면 침묵, 카드 dismiss는 id 기반 영구(콘텐츠 인프라 재사용).
- **프라이버시·네트워크:** 카탈로그 = 공개 read-only GET(사용자 데이터 미전송, 15s 타임아웃). LLM 매칭 = 설정된 Engine(온프레)에만.
- **락 규율:** resolve/gather/persist 짧은 락, LLM·네트워크 I/O는 락 밖 (`run_coaching_judgments` 미러).
- **카탈로그 소스(§8 핀, 2026-07-23 실측):** `https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json` — 200 OK, ~158KB, 425 plugins. 스키마 `{plugins:[{name, description, author?, category?, source, homepage?}]}`.
- 커밋은 Conventional Commits·영어, scope=`agent`. Windows 네이티브에서 `cargo test`.
- **세션 단위 판정의 예외 근거:** E는 B처럼 세션이 판정의 본질 단위(LLM이 "세션 작업 성격" 판정). never-clear 사용자는 kinds 다중 라벨로 부분 보상되며, 근본 교정은 F(컨텍스트 위생)가 담당(스펙 §2-3, §4 B와 동일 논리).

## 설계 결정 기록 (조사로 핀된 것)

| 미확정(§8) | 결정 | 근거 |
|---|---|---|
| 카탈로그 URL·스키마 | 위 Global Constraints의 raw URL. `plugins[]`의 name 필수, 나머지 관대 | 로컬 `~/.claude/plugins/known_marketplaces.json`이 공식 마켓플레이스 = GitHub `anthropics/claude-plugins-official`임을 확인, raw URL 200 실측 |
| 캐시 TTL | 없음 — 스캔마다 fetch (changelog 관행 그대로), 15s 타임아웃(boris 선례) | `fetch_feed_items`가 스캔마다 changelog/boris를 fetch하는 것이 기존 관행 |
| work-kind 스키마 | 고정 vocabulary 4종(아래), LLM은 다중 라벨 분류만. 카드 생성은 결정론 | 425개 카탈로그를 LLM 프롬프트에 넣는 대신, 큐레이션 상수 테이블(정밀도의 선) + 카탈로그는 ② 존재 검증·homepage 링크용 |
| v1 큐레이션 대상 | **스킬 제공형 + MCP 서버명 판정 가능형만**: frontend-design, chrome-devtools-mcp, superpowers, context7(mcp_server="context7") | `plugin_inventory`는 스킬 제공 plugin만 수록 → MCP 전용(playwright)·커맨드 전용(pr-review-toolkit)은 설치/사용 판정 불가 → 오추천("이미 깔린 걸 깔아라") 방지 위해 제외. 4종 모두 공식 카탈로그 실재 확인(2026-07-23) |

**v1 work-kind vocabulary:**

| kind | 라벨 | 추천 plugin | 설치/사용 감지 |
|---|---|---|---|
| `frontend_ui` | 프론트엔드 UI·스타일링 | frontend-design | 스킬(plugin_inventory + `tool_target LIKE 'frontend-design:%'`) |
| `web_debugging` | 웹페이지 디버깅·브라우저 자동화 | chrome-devtools-mcp | 스킬 (동상) |
| `library_docs` | 외부 라이브러리 문서·API 탐색 | context7 | MCP(`mcp_inventory`/`tool_server='context7'` — standalone 동명 서버 포함) |
| `workflow_planning` | 대형 구현·리팩터링 | superpowers | 스킬 (동상) — R12 `LargeImplNoSkill` 의도 승계 |

**데이터 흐름:**

```
[파이프라인, 스캔마다]
 maybe_judge_work_kinds (신규):  엔진 해석(락) → pending 세션+프롬프트(락)
   → LLM 분류(락 밖) → session_work_kinds 캐시(락)          # 세션당 1회, cap 10
 maybe_curate_content (기존 확장): fetch_feed_items + fetch_marketplace_catalog(락 밖)
   → run_curation(락): … + plugin_reco_items(store, catalog) → content_items → content:ready
```

`plugin_reco_items` (순수 결정론): 관찰창 14일 내 판정 세션을 (host, kind)로 묶어
`>= MIN_MATCHED_SESSIONS`이면 → 큐레이션 테이블 조회 → **사용 중이면 침묵 / 설치+미사용 → ① / 미설치+카탈로그 실재 → ② / 미설치+카탈로그 부재 → 침묵**. 카드는 `personal` 태그(SCORE_PERSONAL) + `base_priority -2`(실측 사건 레슨 아래, 프론티어 위), `dimension: None`(마스터 억제·축 쿨다운 미적용, dismiss는 id 영구).

## File Structure

| 파일 | 책임 |
|---|---|
| Modify `a-mate/crates/core/src/content.rs` | `CatalogEntry` + `MarketplaceCatalogSource`(fetch/parse — changelog 형제) |
| Modify `a-mate/crates/core/src/curation.rs` | `WORK_KINDS` vocabulary + `PluginReco` 큐레이션 테이블(코드 상수만) |
| Modify `a-mate/crates/core/src/store.rs` | `session_work_kinds` 테이블 + pending/persist/조회 + 설치·사용 감지 쿼리 |
| Create `a-mate/crates/core/src/plugin_reco.rs` | E 본체: 판정 프롬프트/파싱 + `plugin_reco_items` 결정론 매핑 |
| Modify `a-mate/crates/core/src/lib.rs` | `pub mod plugin_reco;` |
| Modify `a-mate/crates/core/src/ops.rs` | `run_curation` catalog 파라미터 + `fetch_marketplace_catalog` 가장자리 헬퍼 |
| Modify `a-mate/crates/core/src/main.rs`, `a-mate/crates/core/examples/curation_server.rs` | `run_curation` 시그니처 추종 |
| Modify `a-mate/src-tauri/src/pipeline.rs` | `maybe_judge_work_kinds` 스텝 + 카탈로그 fetch 배선 |

---

### Task 1: MarketplaceCatalogSource (content.rs)

**Files:**
- Modify: `a-mate/crates/core/src/content.rs` (BorisTipsSource 블록 뒤에 추가)
- Test: 같은 파일 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub struct CatalogEntry { pub name: String, pub description: String, pub category: Option<String>, pub homepage: Option<String> }`, `pub struct MarketplaceCatalogSource { pub url: String }` (+Default), `pub fn parse_json(&self, raw: &str) -> Vec<CatalogEntry>`, `pub fn fetch(&self) -> Result<Vec<CatalogEntry>>`

- [ ] **Step 1: 실패하는 테스트 작성** — content.rs 테스트 모듈에 추가:

```rust
    // ── E: 마켓플레이스 카탈로그 ──

    #[test]
    fn marketplace_catalog_parses_entries_generously() {
        let src = MarketplaceCatalogSource::default();
        let raw = r#"{
            "name": "claude-plugins-official",
            "plugins": [
                {"name": "frontend-design", "description": "Design guidance",
                 "category": "design", "homepage": "https://example.com/fd"},
                {"description": "이름 없음 — skip"},
                {"name": "context7"}
            ]
        }"#;
        let entries = src.parse_json(raw);
        assert_eq!(entries.len(), 2, "name 없는 항목은 관대하게 skip");
        assert_eq!(entries[0].name, "frontend-design");
        assert_eq!(entries[0].category.as_deref(), Some("design"));
        assert_eq!(entries[0].homepage.as_deref(), Some("https://example.com/fd"));
        assert_eq!(entries[1].name, "context7");
        assert_eq!(entries[1].description, "", "description 없어도 항목 유지");
        // 공식 카탈로그 원본 URL 고정 (스펙 §8 핀)
        assert!(src.url.contains("anthropics/claude-plugins-official"));
    }

    #[test]
    fn marketplace_catalog_malformed_yields_empty() {
        let src = MarketplaceCatalogSource::default();
        assert!(src.parse_json("{not json").is_empty());
        assert!(src.parse_json(r#"{"plugins": "nope"}"#).is_empty());
        assert!(src.parse_json("{}").is_empty());
        assert!(src.parse_json("").is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor marketplace_catalog`
Expected: FAIL (컴파일 에러 — `MarketplaceCatalogSource` 미정의)

- [ ] **Step 3: 최소 구현** — content.rs의 `HubKnowledgeSource` 블록 앞(Boris 블록 뒤)에 추가:

```rust
// ─────────────────────────────────────────────────────────────────────────
// E — 공식 마켓플레이스 카탈로그 (②미설치 plugin 추천 재료. changelog 소스의 형제).
// ContentItem이 아니라 CatalogEntry를 반환 — 카탈로그는 노출물이 아니라 매칭 재료.
// 공개 read-only GET(사용자 데이터 미전송). 관대: 파싱 실패·필드 누락 skip, 실패는
// 호출부(ops::fetch_marketplace_catalog)가 빈 벡터로 계속(② 추천만 침묵).
// ─────────────────────────────────────────────────────────────────────────

/// 공식 마켓플레이스 plugin 한 항목 — E ②(미설치 추천)의 존재 검증·링크 재료.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub homepage: Option<String>,
}

pub struct MarketplaceCatalogSource {
    pub url: String,
}

impl Default for MarketplaceCatalogSource {
    fn default() -> Self {
        MarketplaceCatalogSource {
            // 스펙 §8 핀(2026-07-23 실측): 200 OK, ~158KB, 425 plugins.
            url: "https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json".into(),
        }
    }
}

impl MarketplaceCatalogSource {
    /// marketplace.json을 관대하게 파싱 — plugins[]에서 name 있는 항목만.
    pub fn parse_json(&self, raw: &str) -> Vec<CatalogEntry> {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) else { return Vec::new() };
        let Some(plugins) = json.get("plugins").and_then(|p| p.as_array()) else { return Vec::new() };
        plugins
            .iter()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(CatalogEntry {
                    name,
                    description: p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                    category: p.get("category").and_then(|c| c.as_str()).map(String::from),
                    homepage: p.get("homepage").and_then(|h| h.as_str()).map(String::from),
                })
            })
            .collect()
    }

    /// 네트워크 fetch — 백그라운드 스캔을 막지 않도록 타임아웃 필수(boris 선례).
    pub fn fetch(&self) -> Result<Vec<CatalogEntry>> {
        let body = ureq::get(&self.url)
            .timeout(std::time::Duration::from_secs(15))
            .call()?
            .into_string()?;
        Ok(self.parse_json(&body))
    }
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor marketplace_catalog`
Expected: PASS 2건

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/content.rs
git commit -m "feat(agent): add official marketplace catalog source (E)"
```

---

### Task 2: work-kind vocabulary + 큐레이션 테이블 (curation.rs)

**Files:**
- Modify: `a-mate/crates/core/src/curation.rs` (plugin_purpose 뒤에 추가)

**Interfaces:**
- Produces: `pub const WORK_KINDS: &[(&str, &str)]`, `pub fn work_kind_label(kind: &str) -> Option<&'static str>`, `pub struct PluginReco { pub plugin: &'static str, pub purpose_ko: &'static str, pub mcp_server: Option<&'static str> }`, `pub fn plugin_recos_for(work_kind: &str) -> &'static [PluginReco]`
- 주의: 기존 `WorkPattern`/`SkillRecommendationSource`(은퇴한 R12용)는 **무변경 보존** — 스캐폴드의 "큐레이션 상수 테이블" 패턴만 승계한다.

- [ ] **Step 1: 실패하는 테스트 작성** — curation.rs 테스트 모듈에 추가:

```rust
    #[test]
    fn work_kind_vocabulary_maps_to_official_plugins() {
        // vocabulary의 모든 kind는 라벨과 최소 1개 추천을 가진다
        for (kind, label) in WORK_KINDS {
            assert!(!plugin_recos_for(kind).is_empty(), "{kind}에 추천 없음");
            assert_eq!(work_kind_label(kind), Some(*label));
            assert!(!label.is_empty());
        }
        assert!(plugin_recos_for("unknown_kind").is_empty());
        assert!(work_kind_label("unknown_kind").is_none());
        // v1 핵심 매핑 고정 (스킬 제공형 + MCP 판정 가능형만 — 계획 §설계 결정)
        assert_eq!(plugin_recos_for("frontend_ui")[0].plugin, "frontend-design");
        assert_eq!(plugin_recos_for("workflow_planning")[0].plugin, "superpowers");
        let c7 = &plugin_recos_for("library_docs")[0];
        assert_eq!(c7.plugin, "context7");
        assert_eq!(c7.mcp_server, Some("context7"), "MCP 제공형은 서버명으로 설치/사용 감지");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor work_kind_vocabulary`
Expected: FAIL (컴파일 에러 — `WORK_KINDS` 미정의)

- [ ] **Step 3: 최소 구현** — curation.rs의 `plugin_purpose` 함수 뒤에 추가:

```rust
/// E — LLM이 세션 프롬프트에서 판정하는 작업 성격(work-kind) vocabulary. (key, 한국어 라벨).
/// 항목 추가는 plugin_recos_for와 짝으로. 정밀도의 선: 카드 생성은 이 상수 테이블이 담당.
pub const WORK_KINDS: &[(&str, &str)] = &[
    ("frontend_ui", "프론트엔드 UI·스타일링"),
    ("web_debugging", "웹페이지 디버깅·브라우저 자동화"),
    ("library_docs", "외부 라이브러리 문서·API 탐색"),
    ("workflow_planning", "대형 구현·리팩터링"),
];

pub fn work_kind_label(kind: &str) -> Option<&'static str> {
    WORK_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l)
}

/// work-kind에 유용한 공식 마켓플레이스 plugin (①설치 미사용·②미설치 공용 큐레이션).
/// v1은 설치/사용을 신뢰 판정할 수 있는 plugin만: 스킬 제공형(plugin_inventory 수록) 또는
/// mcp_server 명시형(mcp_inventory/tool_server로 감지). 커맨드 전용(pr-review-toolkit 등)은
/// 감지 불가라 제외 — "이미 깔린 걸 깔아라" 오추천 방지.
pub struct PluginReco {
    pub plugin: &'static str,          // 공식 마켓플레이스 plugin name (@ 앞부분)
    pub purpose_ko: &'static str,      // 카드에 쓸 한 줄 용도
    pub mcp_server: Option<&'static str>, // MCP 제공형의 서버명 (스킬 제공형은 None)
}

pub fn plugin_recos_for(work_kind: &str) -> &'static [PluginReco] {
    match work_kind {
        "frontend_ui" => &[PluginReco {
            plugin: "frontend-design", purpose_ko: "UI 디자인·시각 완성도 가이드", mcp_server: None,
        }],
        "web_debugging" => &[PluginReco {
            plugin: "chrome-devtools-mcp", purpose_ko: "웹페이지 디버깅·성능 분석", mcp_server: None,
        }],
        "library_docs" => &[PluginReco {
            plugin: "context7", purpose_ko: "라이브러리 최신 문서 조회", mcp_server: Some("context7"),
        }],
        "workflow_planning" => &[PluginReco {
            plugin: "superpowers", purpose_ko: "브레인스토밍→플랜→TDD 개발 워크플로", mcp_server: None,
        }],
        _ => &[],
    }
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor work_kind_vocabulary`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/curation.rs
git commit -m "feat(agent): add work-kind vocabulary and plugin reco curation table (E)"
```

---

### Task 3: session_work_kinds 테이블 + 스토어 쿼리 (store.rs)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` — `SCHEMA` 상수에 테이블 추가(`memories` 테이블 뒤), 메서드는 `set_content_status` 근처에 추가, 테스트는 store 테스트 모듈에

**Interfaces:**
- Produces:
  - `pub struct WorkKindCandidate { pub session_id: String, pub host: String, pub project_id: String, pub prev_attempts: u32, pub last_ts: String }`
  - `pub struct JudgedWorkKinds { pub session_id: String, pub host: String, pub project_id: String, pub kinds: Vec<String> }`
  - `pub fn pending_work_kind_sessions(&self, window_start: &str, cap: usize) -> Result<Vec<WorkKindCandidate>>`
  - `pub fn set_session_work_kinds(&self, session_id: &str, host: &str, project_id: &str, last_ts: &str, kinds: Option<&[String]>, attempts: u32, now_ts: &str) -> Result<()>` — `kinds=None`은 형식 불량 시도 기록(attempts만)
  - `pub fn judged_work_kind_sessions(&self, window_start: &str) -> Result<Vec<JudgedWorkKinds>>` — 최신 활동순
  - `pub fn session_lead_prompt(&self, session_id: &str) -> Result<Option<String>>`
  - `pub fn plugin_installed(&self, host: &str, plugin: &str, mcp_server: Option<&str>) -> Result<bool>`
  - `pub fn plugin_used_recently(&self, host: &str, plugin: &str, mcp_server: Option<&str>, window_start: &str) -> Result<bool>`
- 참고: `SCHEMA`는 open 시 `CREATE TABLE IF NOT EXISTS`로 실행되므로 기존 DB에 자동 생성 — 백필 불필요, 재수집 마이그레이션 불필요.

- [ ] **Step 1: 실패하는 테스트 작성** — store.rs 테스트 모듈에 추가:

```rust
    // ── E: session_work_kinds + plugin 설치/사용 감지 ──

    fn wk_prompt_event(sid: &str, ts: &str, preview: &str) -> crate::model::NormalizedEvent {
        crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(format!("{sid}-p-{ts}")), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: 0, msg_id: None,
            kind: crate::model::EventKind::UserPrompt { preview: preview.into() },
        }
    }

    fn wk_turn(sid: &str, uuid: &str, ts: &str) -> crate::model::NormalizedEvent {
        crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: 1, msg_id: None,
            kind: crate::model::EventKind::AssistantTurn {
                model: crate::model::NormModel::from_raw_id("claude-sonnet-4-6"),
                usage: crate::model::TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }
    }

    #[test]
    fn work_kind_pending_persist_and_judged_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let old = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let window = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
        // s1: 창 내 + 프롬프트 + 턴 → pending
        store.upsert_events(&[
            wk_prompt_event("s1", &recent, "로그인 화면 버튼 스타일 다듬어줘"),
            wk_turn("s1", "s1-t", &recent),
            // s2: 창 밖 → 제외
            wk_prompt_event("s2", &old, "옛날 세션의 실질 프롬프트입니다"),
            wk_turn("s2", "s2-t", &old),
            // s3: 프롬프트 없음(턴만) → 제외
            wk_turn("s3", "s3-t", &recent),
        ]).unwrap();
        let pending = store.pending_work_kind_sessions(&window, 10).unwrap();
        assert_eq!(pending.len(), 1, "창 내·프롬프트 있는 세션만: {pending:?}");
        assert_eq!(pending[0].session_id, "s1");
        assert_eq!(pending[0].host, "Windows");
        assert_eq!(pending[0].prev_attempts, 0);

        // 판정 저장 → pending에서 빠지고 judged에 나타난다
        let kinds = vec!["frontend_ui".to_string()];
        store.set_session_work_kinds("s1", "Windows", "d--proj", &recent,
            Some(&kinds), 1, &recent).unwrap();
        assert!(store.pending_work_kind_sessions(&window, 10).unwrap().is_empty());
        let judged = store.judged_work_kind_sessions(&window).unwrap();
        assert_eq!(judged.len(), 1);
        assert_eq!(judged[0].kinds, kinds);
        assert_eq!(judged[0].project_id, "d--proj");
        // 창 밖 judged는 제외
        assert!(store.judged_work_kind_sessions(&recent).unwrap().is_empty());
    }

    #[test]
    fn work_kind_attempts_lifecycle_excludes_after_three_failures() {
        let store = SqliteStore::open_in_memory().unwrap();
        let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let window = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
        store.upsert_events(&[
            wk_prompt_event("s1", &recent, "판정이 계속 실패하는 세션입니다"),
            wk_turn("s1", "s1-t", &recent),
        ]).unwrap();
        // 형식 불량 2회 — 여전히 pending(재시도), attempts 누적
        store.set_session_work_kinds("s1", "Windows", "d--proj", &recent, None, 1, &recent).unwrap();
        store.set_session_work_kinds("s1", "Windows", "d--proj", &recent, None, 2, &recent).unwrap();
        let p = store.pending_work_kind_sessions(&window, 10).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].prev_attempts, 2);
        // 3회째 — 영구 제외(침묵), judged에도 없음
        store.set_session_work_kinds("s1", "Windows", "d--proj", &recent, None, 3, &recent).unwrap();
        assert!(store.pending_work_kind_sessions(&window, 10).unwrap().is_empty());
        assert!(store.judged_work_kind_sessions(&window).unwrap().is_empty());
    }

    #[test]
    fn session_lead_prompt_returns_first_substantive() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut e1 = wk_prompt_event("s1", "2026-07-20T10:00:00Z", "첫 실질 프롬프트로 작업을 엽니다");
        e1.source_offset = 0;
        let mut e2 = wk_prompt_event("s1", "2026-07-20T11:00:00Z", "나중에 온 교정 프롬프트입니다");
        e2.source_offset = 100;
        store.upsert_events(&[e2, e1]).unwrap(); // 삽입 순서 무관
        assert_eq!(store.session_lead_prompt("s1").unwrap().unwrap(),
            "첫 실질 프롬프트로 작업을 엽니다");
        assert!(store.session_lead_prompt("none").unwrap().is_none());
    }

    #[test]
    fn plugin_installed_via_inventory_or_mcp_server() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[crate::inventory::PluginRecord {
            plugin_key: "frontend-design@claude-plugins-official".into(),
            namespace: "frontend-design".into(), skill_count: 1, resident_tokens: 100,
            skills: vec!["frontend-design".into()], mcp_servers: vec![],
        }]).unwrap();
        assert!(store.plugin_installed("Windows", "frontend-design", None).unwrap());
        assert!(!store.plugin_installed("Windows", "superpowers", None).unwrap());
        assert!(!store.plugin_installed("WSL:u", "frontend-design", None).unwrap(), "호스트 분리");
        // MCP 제공형: mcp_inventory의 동명 서버로 감지 (standalone 설정 포함 — 이미 있으면 ②금지)
        store.conn.execute(
            "INSERT INTO mcp_inventory (host, project_id, server, source) VALUES ('Windows','*','context7','plugin')",
            [],
        ).unwrap();
        assert!(store.plugin_installed("Windows", "context7", Some("context7")).unwrap());
        assert!(!store.plugin_installed("Windows", "playwright", Some("playwright")).unwrap());
    }

    #[test]
    fn plugin_used_recently_detects_skill_and_mcp_and_respects_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let old = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let window = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
        let tool = |sid: &str, uuid: &str, kind: crate::model::ToolKind, raw: &str,
                    target: Option<&str>, ts: &str| crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: 2, msg_id: None,
            kind: crate::model::EventKind::ToolCall {
                kind, raw_name: raw.into(), target: target.map(String::from), tool_use_id: None,
            },
        };
        store.upsert_events(&[
            // 스킬 호출 (frontend-design)
            tool("s1", "u1", crate::model::ToolKind::Skill { name: "frontend-design:frontend-design".into() },
                "Skill", Some("frontend-design:frontend-design"), &recent),
            // standalone 동명 MCP 호출 (context7)
            tool("s2", "u2", crate::model::ToolKind::from_raw_name("mcp__context7__query-docs"),
                "mcp__context7__query-docs", None, &recent),
            // 하네스 plugin_ 접두 MCP 호출 (context7 플러그인 경유)
            tool("s3", "u3", crate::model::ToolKind::from_raw_name("mcp__plugin_context7_context7__query-docs"),
                "mcp__plugin_context7_context7__query-docs", None, &recent),
            // 창 밖 스킬 호출 (superpowers) — 사용으로 안 침
            tool("s4", "u4", crate::model::ToolKind::Skill { name: "superpowers:brainstorming".into() },
                "Skill", Some("superpowers:brainstorming"), &old),
        ]).unwrap();
        assert!(store.plugin_used_recently("Windows", "frontend-design", None, &window).unwrap());
        assert!(store.plugin_used_recently("Windows", "context7", Some("context7"), &window).unwrap());
        assert!(!store.plugin_used_recently("Windows", "superpowers", None, &window).unwrap(),
            "창 밖 사용은 최근 사용 아님");
        assert!(!store.plugin_used_recently("WSL:u", "frontend-design", None, &window).unwrap(),
            "호스트 분리");
    }
```

주의: `plugin_used_recently`의 `plugin_` 접두 감지는 s3 케이스(`tool_server = "plugin_context7_context7"`)를 커버해야 한다 — 테스트에서 s2를 지워도(standalone 없음) context7 사용이 감지되는지 별도 assert를 넣어도 좋다.

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor work_kind_ session_lead plugin_installed plugin_used`
(개별 이름으로 각각 실행해도 됨) Expected: FAIL (컴파일 에러 — 메서드 미정의)

- [ ] **Step 3: 최소 구현**

SCHEMA 상수의 `memories` 테이블 뒤에 추가:

```sql
CREATE TABLE IF NOT EXISTS session_work_kinds (
  session_id TEXT PRIMARY KEY,
  host TEXT NOT NULL, project_id TEXT NOT NULL,
  kinds_json TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_ts TEXT,
  judged_at TEXT
);
```

store.rs impl 블록(예: `set_content_status` 뒤)에 추가:

```rust
    // ── E: 세션 work-kind 판정 캐시 + plugin 설치/사용 감지 ──────────────────

    /// E — work-kind 판정 대기 세션: 관찰창 내 활동·main-chain 턴 ≥1·실질 프롬프트 ≥1,
    /// 아직 미판정(kinds_json NULL) + attempts < 3(3회 형식 불량 = 영구 침묵). 최신 활동순.
    pub fn pending_work_kind_sessions(
        &self,
        window_start: &str,
        cap: usize,
    ) -> Result<Vec<WorkKindCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.session_id, COALESCE(MAX(e.host),''), COALESCE(MAX(e.project_id),''),
                    COALESCE(MAX(w.attempts), 0), COALESCE(MAX(e.ts),'')
             FROM events e
             LEFT JOIN session_work_kinds w ON w.session_id = e.session_id
             WHERE e.is_sidechain = 0
             GROUP BY e.session_id
             HAVING MAX(e.ts) >= ?1
                AND SUM(CASE WHEN e.kind='assistant_turn' THEN 1 ELSE 0 END) >= 1
                AND EXISTS (SELECT 1 FROM prompt_events p WHERE p.session_id = e.session_id)
                AND MAX(w.kinds_json) IS NULL
                AND COALESCE(MAX(w.attempts), 0) < 3
             ORDER BY MAX(e.ts) DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![window_start, cap as i64], |r| {
            Ok(WorkKindCandidate {
                session_id: r.get(0)?,
                host: r.get(1)?,
                project_id: r.get(2)?,
                prev_attempts: r.get::<_, i64>(3)? as u32,
                last_ts: r.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// E — 판정 결과 저장. kinds=None은 형식 불량 시도 기록(attempts만 증가; kinds_json은
    /// NULL 유지 → attempts>=3이면 pending에서 자연 제외 = 영구 침묵, judged에도 안 나옴).
    pub fn set_session_work_kinds(
        &self,
        session_id: &str,
        host: &str,
        project_id: &str,
        last_ts: &str,
        kinds: Option<&[String]>,
        attempts: u32,
        now_ts: &str,
    ) -> Result<()> {
        let kinds_json = kinds.map(serde_json::to_string).transpose()?;
        self.conn.execute(
            "INSERT INTO session_work_kinds
                (session_id, host, project_id, kinds_json, attempts, last_ts, judged_at)
             VALUES (?1,?2,?3,?4,?5,?6, CASE WHEN ?4 IS NOT NULL THEN ?7 END)
             ON CONFLICT(session_id) DO UPDATE SET
                kinds_json = COALESCE(?4, kinds_json), attempts = ?5, last_ts = ?6,
                judged_at = CASE WHEN ?4 IS NOT NULL THEN ?7 ELSE judged_at END",
            params![session_id, host, project_id, kinds_json, attempts as i64, last_ts, now_ts],
        )?;
        Ok(())
    }

    /// E — 관찰창 내 판정 완료 세션의 work-kind (빈 배열 포함 — 소비자가 거른다). 최신 활동순.
    pub fn judged_work_kind_sessions(&self, window_start: &str) -> Result<Vec<JudgedWorkKinds>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, host, project_id, kinds_json FROM session_work_kinds
             WHERE kinds_json IS NOT NULL AND last_ts >= ?1
             ORDER BY last_ts DESC",
        )?;
        let rows = stmt.query_map(params![window_start], |r| {
            let kinds_json: String = r.get(3)?;
            Ok(JudgedWorkKinds {
                session_id: r.get(0)?,
                host: r.get(1)?,
                project_id: r.get(2)?,
                kinds: serde_json::from_str(&kinds_json).unwrap_or_default(),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// E — 세션을 연 첫 실질 프롬프트(카드 근거 인용용). prompt_events는 이미 사람 발화만.
    pub fn session_lead_prompt(&self, session_id: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.conn
            .query_row(
                "SELECT preview FROM prompt_events WHERE session_id = ?1
                 ORDER BY COALESCE(ts,''), source_offset LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// E — 설치+enabled 여부. plugin_inventory(스킬 제공형) 또는, MCP 제공형이면
    /// mcp_inventory의 동명 서버(standalone 설정 포함 — 이미 갖고 있으면 ② 금지)로 판정.
    pub fn plugin_installed(&self, host: &str, plugin: &str, mcp_server: Option<&str>) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM plugin_inventory WHERE host=?1 AND plugin_key LIKE ?2 || '@%'",
            params![host, plugin],
            |r| r.get(0),
        )?;
        if n > 0 {
            return Ok(true);
        }
        if let Some(server) = mcp_server {
            let m: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM mcp_inventory WHERE host=?1 AND server=?2",
                params![host, server],
                |r| r.get(0),
            )?;
            return Ok(m > 0);
        }
        Ok(false)
    }

    /// E — 관찰창 내 이 호스트에서 plugin 사용 흔적 (이미 쓰면 침묵 — 스펙 §4 E):
    /// 스킬 호출(`ns:skill` target) / 하네스 plugin 접두 MCP(`plugin_<name>_<server>`) /
    /// 큐레이션 명시 서버명(standalone 동명 서버 포함) / 설치 인벤토리가 선언한 서버명.
    pub fn plugin_used_recently(
        &self,
        host: &str,
        plugin: &str,
        mcp_server: Option<&str>,
        window_start: &str,
    ) -> Result<bool> {
        use rusqlite::OptionalExtension;
        // 설치된 plugin이 선언한 MCP 서버명 + 큐레이션 명시 서버명의 합집합
        let mut servers: Vec<String> = self
            .conn
            .query_row(
                "SELECT mcp_servers_json FROM plugin_inventory
                 WHERE host=?1 AND plugin_key LIKE ?2 || '@%'",
                params![host, plugin],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        if let Some(s) = mcp_server {
            if !servers.iter().any(|x| x == s) {
                servers.push(s.to_string());
            }
        }
        let servers_json = serde_json::to_string(&servers)?;
        let used: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events
             WHERE host=?1 AND is_sidechain=0 AND kind='tool_call' AND ts >= ?2
               AND ( (tool_kind='skill' AND tool_target LIKE ?3 || ':%')
                  OR (tool_kind='mcp_call' AND (
                        tool_server LIKE 'plugin\\_' || ?3 || '\\_%' ESCAPE '\\'
                     OR tool_server IN (SELECT value FROM json_each(?4)) )) )",
            params![host, window_start, plugin, servers_json],
            |r| r.get(0),
        )?;
        Ok(used > 0)
    }
```

구조체는 `ContentRow` 정의 근처에 추가:

```rust
/// E — work-kind 판정 대기 세션 (pending_work_kind_sessions 반환 행).
#[derive(Debug, Clone)]
pub struct WorkKindCandidate {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    pub prev_attempts: u32,
    pub last_ts: String,
}

/// E — 판정 완료 세션의 work-kind (judged_work_kind_sessions 반환 행).
#[derive(Debug, Clone)]
pub struct JudgedWorkKinds {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    pub kinds: Vec<String>,
}
```

주의: rust 문자열 리터럴 안의 SQL LIKE ESCAPE — `'plugin\\_'`는 rust 이스케이프 후 SQL에 `plugin\_`로 전달된다. rusqlite `OptionalExtension`은 파일 상단이 아니라 함수 내 `use`로 (기존 스타일 확인 후 맞춤).

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor work_kind session_lead_prompt plugin_installed plugin_used`
Expected: PASS 5건 (+기존 회귀 무손상: `cargo test -p agent-mentor` 전체 그린)

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add session work-kind cache and plugin usage queries (E)"
```

---

### Task 4: 판정 프롬프트 + verdict 파싱 (plugin_reco.rs 신규)

**Files:**
- Create: `a-mate/crates/core/src/plugin_reco.rs`
- Modify: `a-mate/crates/core/src/lib.rs` (`pub mod plugin_reco;` 추가 — 알파벳 순서 위치)

**Interfaces:**
- Consumes: `curation::{WORK_KINDS, plugin_recos_for, work_kind_label}` (Task 2)
- Produces: `pub const WORK_KIND_WINDOW_DAYS: i64 = 14`, `pub const WORK_KIND_BATCH_CAP: usize = 10`, `pub const MIN_MATCHED_SESSIONS: usize = 2`, `pub fn work_kind_prompt(prompts: &[String]) -> (String, String)`, `pub fn parse_work_kinds(verdict: &serde_json::Value) -> Option<Vec<String>>`

- [ ] **Step 1: 실패하는 테스트 작성** — plugin_reco.rs를 테스트 포함으로 생성:

```rust
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
}
```

- [ ] **Step 2: lib.rs에 모듈 등록 후 실패 확인**

lib.rs의 모듈 선언에 `pub mod plugin_reco;` 추가 (알파벳 순 — `pipeline` 뒤).

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor plugin_reco::`
Expected: FAIL (컴파일 에러 — `work_kind_prompt` 미정의)

- [ ] **Step 3: 최소 구현** — plugin_reco.rs 상수 아래·tests 위에 추가:

```rust
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
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor plugin_reco::`
Expected: PASS 2건

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/plugin_reco.rs a-mate/crates/core/src/lib.rs
git commit -m "feat(agent): add work-kind judging prompt and verdict parsing (E)"
```

---

### Task 5: 인벤토리 상태 매핑 → 추천 카드 (plugin_reco.rs)

**Files:**
- Modify: `a-mate/crates/core/src/plugin_reco.rs`

**Interfaces:**
- Consumes: Task 3 store 메서드 전부, Task 1 `CatalogEntry`, Task 2 큐레이션 테이블
- Produces: `pub fn plugin_reco_items(store: &SqliteStore, catalog: &[CatalogEntry], now_ts: &str) -> Result<Vec<ContentItem>>` — Task 6의 `run_curation`이 호출

- [ ] **Step 1: 실패하는 테스트 작성** — plugin_reco.rs tests 모듈에 추가:

```rust
    use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage, ToolKind};
    use crate::store::SqliteStore;

    fn ev(sid: &str, uuid: &str, offset: i64, ts: &str, kind: EventKind) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: offset as u64, msg_id: None,
            kind,
        }
    }

    /// frontend_ui로 판정된 세션 n개 시드 (프롬프트 + 턴 + work-kind 캐시)
    fn seed_frontend_sessions(store: &SqliteStore, n: usize, now: &str) {
        for i in 0..n {
            let sid = format!("fe{i}");
            store.upsert_events(&[
                ev(&sid, &format!("{sid}-p"), 0, now,
                    EventKind::UserPrompt { preview: format!("컴포넌트 {i} 스타일을 다듬어줘") }),
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

    #[test]
    fn reco_case2_not_installed_recommends_install_with_catalog() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_frontend_sessions(&store, 2, &now);
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
        assert!(plugin_reco_items(&store, &[], &now).unwrap().is_empty());
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
        let items = plugin_reco_items(&store, &official_catalog(), &now).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].body.contains("5개"), "매칭 세션 수 인용: {}", items[0].body);
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor plugin_reco::`
Expected: FAIL (컴파일 에러 — `plugin_reco_items` 미정의)

- [ ] **Step 3: 최소 구현** — plugin_reco.rs에 추가:

```rust
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
                // ② 미설치 — 공식 카탈로그에 실재할 때만 (fetch 실패·목록 이탈 = 침묵)
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
            out.push(ContentItem {
                id,
                kind: ItemKind::Tip,
                title,
                body,
                source_url,
                dimension: None, // 개인 사건형 — 마스터 억제·축 쿨다운 미적용, dismiss는 id 영구
                trigger_tags: vec!["personal".into(), "plugin-reco".into(), kind.clone()],
                base_priority: -2, // 실측 사건 레슨(0)보단 아래, 프론티어 안내(-5)보단 위
            });
            break; // work-kind당 1장 — 첫 비침묵 후보만 (나깅 방지)
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor plugin_reco::`
Expected: PASS 9건 (Task 4의 2건 포함)

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/plugin_reco.rs
git commit -m "feat(agent): map work-kind and inventory state to plugin reco cards (E)"
```

---

### Task 6: run_curation 편입 + 카탈로그 가장자리 fetch (ops.rs + 호출부)

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` — `run_curation` 시그니처 확장, `fetch_marketplace_catalog` 추가, 기존 테스트 호출부 갱신
- Modify: `a-mate/crates/core/src/main.rs` — `run_curation` 호출부(있다면) 갱신
- Modify: `a-mate/crates/core/examples/curation_server.rs` — `run_curation` 호출부 갱신 (`&[]` 전달)

**Interfaces:**
- Consumes: `plugin_reco::plugin_reco_items` (Task 5), `content::{CatalogEntry, MarketplaceCatalogSource}` (Task 1)
- Produces: `pub fn run_curation(store, feed_items: Vec<ContentItem>, catalog: &[CatalogEntry], now_ts: &str) -> Result<Vec<ContentRow>>`, `pub fn fetch_marketplace_catalog() -> Vec<CatalogEntry>` — Task 7의 파이프라인이 호출

- [ ] **Step 1: 실패하는 테스트 작성** — ops.rs 테스트 모듈에 추가:

```rust
    #[test]
    fn run_curation_surfaces_plugin_reco_and_preserves_dismissal() {
        use crate::content::CatalogEntry;
        use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage};
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        // frontend_ui 판정 세션 2개 시드 (프롬프트+턴+판정 캐시)
        for i in 0..2 {
            let sid = format!("fe{i}");
            store.upsert_events(&[
                NormalizedEvent {
                    source_agent: "claude-code".into(), schema_version: "t".into(),
                    host: "Windows".into(), project_id: "d--proj".into(),
                    session_id: sid.clone(), uuid: Some(format!("{sid}-p")), parent_uuid: None,
                    is_sidechain: false, ts: Some(now.clone()),
                    source_file: format!("{sid}.jsonl"), source_offset: 0, msg_id: None,
                    kind: EventKind::UserPrompt { preview: "버튼 컴포넌트 스타일 다듬어줘".into() },
                },
                NormalizedEvent {
                    source_agent: "claude-code".into(), schema_version: "t".into(),
                    host: "Windows".into(), project_id: "d--proj".into(),
                    session_id: sid.clone(), uuid: Some(format!("{sid}-t")), parent_uuid: None,
                    is_sidechain: false, ts: Some(now.clone()),
                    source_file: format!("{sid}.jsonl"), source_offset: 1, msg_id: None,
                    kind: EventKind::AssistantTurn {
                        model: NormModel::from_raw_id("claude-sonnet-4-6"),
                        usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
                    },
                },
            ]).unwrap();
            store.set_session_work_kinds(&sid, "Windows", "d--proj", &now,
                Some(&["frontend_ui".to_string()]), 1, &now).unwrap();
        }
        let catalog = vec![CatalogEntry {
            name: "frontend-design".into(), description: "".into(),
            category: None, homepage: None,
        }];
        // ② 카드가 큐레이션 노출 목록에 오른다 (personal 스코어 → 상단권)
        let visible = run_curation(&store, vec![], &catalog, &now).unwrap();
        let reco = visible.iter().find(|r| r.id.starts_with("plugin-reco-"))
            .expect("plugin 추천 카드 노출");
        assert!(reco.trigger_tags.iter().any(|t| t == "personal"));
        // dismiss → 재큐레이션에도 다시 안 뜬다 (기존 dismissal 인프라 재사용 검증)
        let reco_id = reco.id.clone();
        store.set_content_status(&reco_id, "dismissed", &now).unwrap();
        let again = run_curation(&store, vec![], &catalog, &now).unwrap();
        assert!(again.iter().all(|r| r.id != reco_id), "dismiss된 추천은 재노출 금지");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor run_curation_surfaces_plugin_reco`
Expected: FAIL (컴파일 에러 — run_curation 인자 3개)

- [ ] **Step 3: 구현**

`run_curation` 시그니처·본문 수정 (ops.rs:196 부근):

```rust
pub fn run_curation(
    store: &SqliteStore,
    feed_items: Vec<crate::content::ContentItem>,
    catalog: &[crate::content::CatalogEntry],
    now_ts: &str,
) -> Result<Vec<crate::store::ContentRow>> {
```

본문의 `items.extend(feed_items);` **앞**에 추가:

```rust
    // E — plugin 추천 (결정론: 캐시된 work-kind + 인벤토리 + 카탈로그. LLM 판정은 파이프라인
    // 별도 스텝이 캐시해 둠 — 엔진 미설정이면 캐시가 비어 자연 침묵)
    items.extend(crate::plugin_reco::plugin_reco_items(store, catalog, now_ts)?);
```

`fetch_feed_items` 아래에 가장자리 헬퍼 추가:

```rust
/// E 카탈로그 fetch — 부수효과는 가장자리(파이프라인이 락 밖에서 호출). 실패는 관대:
/// 빈 벡터 = ② 추천만 침묵(에러 아님), ①·나머지 큐레이션은 계속 (스펙 §6).
pub fn fetch_marketplace_catalog() -> Vec<crate::content::CatalogEntry> {
    match crate::content::MarketplaceCatalogSource::default().fetch() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[curation] 마켓플레이스 카탈로그 fetch 실패(계속): {e}");
            Vec::new()
        }
    }
}
```

기존 호출부 갱신 (컴파일 에러 나는 곳 전부):
- ops.rs 기존 테스트들: `run_curation(&store, vec![], "…")` → `run_curation(&store, vec![], &[], "…")`
- `a-mate/crates/core/src/main.rs`의 run_curation 호출(있다면): `&[]` 또는 `&ops::fetch_marketplace_catalog()` (CLI 디버그 — 파이프라인과 동일하게 fetch 권장)
- `a-mate/crates/core/examples/curation_server.rs`: `&[]` 전달

- [ ] **Step 4: 통과 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml -p agent-mentor` 와 `cargo build --manifest-path a-mate/Cargo.toml --examples`
Expected: 전체 PASS (신규 1건 포함, 기존 run_curation 테스트 회귀 그린)

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/ops.rs a-mate/crates/core/src/main.rs a-mate/crates/core/examples/curation_server.rs
git commit -m "feat(agent): feed marketplace catalog into curation pipeline (E)"
```

---

### Task 7: 파이프라인 배선 — work-kind 판정 스텝 + 카탈로그 fetch (src-tauri)

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs`

**Interfaces:**
- Consumes: `agent_mentor::plugin_reco::{work_kind_prompt, parse_work_kinds, WORK_KIND_BATCH_CAP, WORK_KIND_WINDOW_DAYS}`, `agent_mentor::judge::extract_verdict_json`, `agent_mentor::ops::fetch_marketplace_catalog`, `store.pending_work_kind_sessions / session_user_prompts / set_session_work_kinds`
- 참고: 이 파일은 `#[cfg(not(test))]` 런타임 전용 — 단위 테스트 없음(기존 관행). 핵심 로직은 Task 3~5에서 테스트 완료.

- [ ] **Step 1: maybe_curate_content에 카탈로그 fetch 추가**

`// ① 락 밖: 피드 소스 네트워크 fetch (실패해도 빈 벡터)` 블록을 다음으로 교체:

```rust
        // ① 락 밖: 피드 소스 + E 마켓플레이스 카탈로그 네트워크 fetch (실패해도 빈 벡터)
        let feed = agent_mentor::ops::fetch_feed_items(hub_src);
        let catalog = agent_mentor::ops::fetch_marketplace_catalog();
        let now = chrono::Utc::now().to_rfc3339();
```

그리고 `run_curation(&store, feed, &now)` 호출을 `run_curation(&store, feed, &catalog, &now)`로.

- [ ] **Step 2: maybe_judge_work_kinds 스텝 추가**

`run_pipeline_once`의 `run_coaching_judgments(app, &state.store);` 다음 줄에 삽입:

```rust
                // E — 세션 work-kind 판정(LLM) 캐시. 엔진 없으면 no-op, 카드는 다음 스캔의
                // 큐레이션이 집계 (판정 패스와 별개 — 스펙 §4 E "큐레이션에 얹음")
                maybe_judge_work_kinds(&state.store);
```

`run_coaching_judgments` 함수 정의 뒤에 추가:

```rust
    /// E — 세션 work-kind 판정(LLM, 큐레이션 매칭 재료). 판정 패스(finding)와 별개지만
    /// 락 규율은 동일: ①엔진 해석·②후보+프롬프트는 짧은 락, ③generate는 락 밖, ④저장 짧은 락.
    /// 엔진 미설정이면 no-op(fail-safe 침묵). verdict는 세션당 1회 캐시(재판정 없음).
    /// 전송 실패 = attempts 미증가(다음 스캔 재시도) / 형식 불량 = attempts++(3회면 영구 침묵).
    fn maybe_judge_work_kinds(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::diary::engine::Engine as _;
        use agent_mentor::judge::extract_verdict_json;
        use agent_mentor::plugin_reco::{
            parse_work_kinds, work_kind_prompt, WORK_KIND_BATCH_CAP, WORK_KIND_WINDOW_DAYS,
        };
        // ① 엔진 (짧은 락)
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };
        let window_start =
            (chrono::Utc::now() - chrono::Duration::days(WORK_KIND_WINDOW_DAYS)).to_rfc3339();

        // ② 후보 + 프롬프트 (짧은 락, SQL만)
        let batch: Vec<(agent_mentor::store::WorkKindCandidate, String, String)> =
            match store_mutex.lock() {
                Ok(store) => store
                    .pending_work_kind_sessions(&window_start, WORK_KIND_BATCH_CAP)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|c| {
                        let prompts = store.session_user_prompts(&c.session_id, 5).ok()?;
                        if prompts.is_empty() { return None; }
                        let (sys, usr) = work_kind_prompt(&prompts);
                        Some((c, sys, usr))
                    })
                    .collect(),
                Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
            };
        if batch.is_empty() { return; }

        // ③ 락 밖: 판정 (LLM 네트워크 I/O)
        let mut results = Vec::new();
        for (c, sys, usr) in batch {
            match engine.generate(&sys, &usr) {
                Ok(out) => results.push((c, out.text)),
                // 전송 실패 — attempts 미증가, 다음 스캔 재시도
                Err(e) => log::warn!("work-kind 판정 전송 실패({}): {e}", c.session_id),
            }
        }

        // ④ 저장 (짧은 락)
        let now = chrono::Utc::now().to_rfc3339();
        match store_mutex.lock() {
            Ok(store) => {
                for (c, text) in results {
                    let attempts = c.prev_attempts + 1;
                    let kinds = extract_verdict_json(&text).ok()
                        .and_then(|v| parse_work_kinds(&v));
                    let _ = store.set_session_work_kinds(
                        &c.session_id, &c.host, &c.project_id, &c.last_ts,
                        kinds.as_deref(), attempts, &now,
                    );
                }
            }
            Err(e) => log::warn!("store lock poisoned: {e}"),
        }
    }
```

- [ ] **Step 3: 컴파일 + 전체 테스트 확인**

Run: `cargo test --manifest-path a-mate/Cargo.toml` (워크스페이스 전체 — src-tauri 포함 컴파일)
Expected: 전체 PASS

- [ ] **Step 4: Commit**

```bash
git add a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(agent): wire work-kind judging and catalog fetch into pipeline (E)"
```

---

### Task 8: 최종 검증 + PR (DoD)

- [ ] **Step 1: Rust 전체 검증 (네이티브 Windows)**

Run: `cargo test --manifest-path a-mate/Cargo.toml`
Expected: 전체 PASS (0 failed)

- [ ] **Step 2: 프론트엔드 검증** (ContentRow 계약 불변이므로 회귀 확인용)

Run (a-mate/에서): `npm install` (워크트리 최초 1회) → `npm test`
Expected: Vitest 전체 PASS

- [ ] **Step 3: src-tauri 빌드 확인**

Run: `cargo build --manifest-path a-mate/Cargo.toml`
Expected: 빌드 성공 (agent-mentor-app 포함)

- [ ] **Step 4: docs-archive (같은 PR에서, ADR 0013 DoD)**

`docs-archive` 스킬 실행 → 이 계획 문서를 `docs/archive/design/a-mate/plans/`로 미러 이동, 커밋:

```bash
git commit -m "docs(archive): archive completed E marketplace plugin reco plan"
```

- [ ] **Step 5: PR 생성**

```bash
git push -u origin <branch>
gh pr create --title "feat(agent): official marketplace plugin recommendations (E)" --body "..."
```

PR 본문에 포함: 스펙 링크(§4 E), 설계 결정 표(카탈로그 URL 핀·v1 큐레이션 대상 한정 근거), 검증 증거(cargo test/npm test/build 결과), main 기준선 테스트 픽스(선행 커밋) 언급.

---

## Self-Review 체크 (작성 시 수행)

- **스펙 커버리지:** §4 E ①②침묵 매핑(Task 5), 카탈로그 소스 편입(Task 1·6·7), LLM work-kind 매칭(Task 4·7), 큐레이션 스캐폴드 일반화(Task 2), 쿨다운·dismissal 재사용(Task 6 테스트), §6 E 테스트 4항목 전부 매핑 완료.
- **§8 미확정:** 카탈로그 URL·스키마 실측 핀(Global Constraints). `MIN_MATCHED_SESSIONS=2` ⚠ CALIBRATE로 표시(후속 캘리브레이션 대상).
- **타입 일관성:** `plugin_reco_items(store, catalog, now_ts)` — Task 5 정의 = Task 6 호출. `PluginReco.mcp_server` — Task 2 정의 = Task 3·5 소비. `WorkKindCandidate`/`JudgedWorkKinds` — Task 3 정의 = Task 5·7 소비.
- **의도적 스코프 제외(YAGNI):** 카탈로그 캐시 TTL(관행 없음), MCP 전용·커맨드 전용 plugin의 설치 감지(인벤토리 확장 필요 — 후속), work-kind vocabulary 확장, 프론트엔드 변경.
