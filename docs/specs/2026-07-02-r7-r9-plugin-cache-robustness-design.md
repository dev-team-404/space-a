# Agent Mentor — R7·R9 규칙 + 플러그인 캐시 읽기 견고성 설계

> **상태**: 브레인스토밍 합의 완료 (2026-07-02)
> **범위**: (B) Tier 0 규칙 카탈로그 확장 — R7(단순 작업에 Opus)·R9(웹 도구 남용). (C) 플러그인 캐시 읽기 견고성 — 다중 버전 활성 버전 선택 + completeness 전파.
> **관계**: 코칭 지능 설계(`docs/specs/2026-07-01-coaching-intelligence-design.md` §3.2 Finding, §5 규칙 카탈로그)와 인벤토리 reconciliation 설계(`docs/specs/2026-07-02-inventory-reconciliation-design.md` §7·§7.1)를 전제·후속한다.

---

## 0. 한 줄

현재 정규화 이벤트로 구현 가능한 규칙 R7·R9를 추가하고(세션 단위, 결정론적), 인벤토리 reconciliation의 알려진 한계(다중 버전 캐시 dir·중첩 `.mcp.json` 부분셋)를 닫아 R1 오탐/누락을 줄인다.

---

## 1. 배경 · 제약

### 1.1 이벤트 스키마 제약 (R7 설계의 linchpin)

`events` 테이블은 assistant 라인 1개를 `AssistantTurn`(offset+0) + `ToolCall`들(offset+1, +2…)로 분해해 저장한다. 이들은 **같은 uuid를 공유**하지만, uuid는 `dedup_key`(`uuid:offset`)에만 들어가고 **독립 컬럼이 아니다.** 따라서 SQL에서 "이 assistant 턴이 호출한 도구"를 직접 조인할 수단이 없다.

스펙 §5의 R7("출력<300토큰 & 단일 trivial 도구")은 턴 단위를 암시하나, 이를 그대로 구현하려면 (a) `uuid` 컬럼 추가 + 전체 재수집 또는 (b) offset 인접성 기반 gaps-and-islands 조인(취약)이 필요하다. **결정: R7을 세션 단위로 구현**(§2.1). R1 v0가 스펙과 의도적으로 이탈했듯(전체 기간 호출 0 등), R7도 v0 이탈로 문서화한다.

### 1.2 쿼리 가능한 컬럼

`events`: `session_id, host, project_id, ts, kind, model_family, model_tier, tok_input, tok_output, tok_cache_read, tok_cache_create, tok_eph_1h, web_search, web_fetch, tool_kind, tool_server, tool_tool, tool_target, raw_name, is_sidechain`.

- `web_search`/`web_fetch` = assistant_turn의 `usage.server_tool_use.{web_search,web_fetch}_requests` (서버측 웹 도구 호출 수). R9의 신호.
- `tool_kind ∈ {file_read, file_edit, file_write, search, execute, mcp_call, web_search, web_fetch, sub_agent, other}` (ToolCall 행). R7의 "도구 종류" 신호.

### 1.3 Opus↔Haiku 가격 (R7 비용등가 근거, claude-api 확인 2026-07-02)

| 토큰 종류 | Opus 4.x(4.6/4.7/4.8) | Haiku 4.5 | Haiku/Opus |
|---|---|---|---|
| input ($/1M) | 5.00 | 1.00 | 0.20 |
| output ($/1M) | 25.00 | 5.00 | 0.20 |
| cache_read (≈0.1×in) | 0.50 | 0.10 | 0.20 |
| cache_create (≈1.25×in) | 6.25 | 1.25 | 0.20 |

**모든 토큰 종류에서 Opus:Haiku = 5:1로 균일** → Haiku는 Opus 비용의 20%. 따라서 토큰 믹스와 무관하게 **SAVINGS_FRACTION = 0.8**(가중치 불필요). Opus 4.x 전체가 $5/$25라 상수는 견고하다.

---

## 2. B) 규칙 카탈로그 확장

각 규칙은 기존 패턴을 따른다: `Rule` trait 구현체(`src/rules/rN_*.rs`, struct + `Default` + `evaluate`), `cmd_rules`의 `RuleEngine`에 등록, `diary/mod.rs::finding_advice`에 arm 추가.

### 2.1 R7 — 단순 작업에 Opus (세션 단위)

**의도:** Opus로 짧고 기계적인 잔심부름만 한 세션을 지목 — "Haiku로 시켰으면 훨씬 쌌다"(다이어리 §7.1 "파일 이름 바꾸는 잔심부름"과 일치).

**탐지(세션별 집계):** 아래 3개를 **모두** 만족하는 세션.

1. **Opus 전용** — 그 세션의 `assistant_turn` 중 `model_family='opus'`가 ≥1개이고, opus가 아닌 assistant_turn이 0개. (이미 Haiku/Sonnet를 섞어 썼으면 지목하지 않음 — 나깅 방지.)
2. **가벼운 출력** — `SUM(tok_output) < max_output_tokens`.
3. **사소한 도구만** — `tool_call` 개수가 `1..=max_tool_calls` 범위이고, "무거운 도구"가 0개.
   - 무거운 도구 = `tool_kind IN ('sub_agent','mcp_call','web_search','web_fetch')` **또는** 세션의 `SUM(web_search)+SUM(web_fetch) > 0`(서버측 웹).
   - "도구 ≥1"이 순수 대화형 Opus 턴(짧지만 깊은 답변 — Opus가 맞을 수 있음)과 "기계적 잔심부름"을 가른다. 스펙 §5의 "& trivial 도구" 취지.

> **v0 이탈 명시:** 스펙 §5는 턴 단위 "출력<300 & 단일 trivial 도구". v0는 세션 단위(§1.1 스키마 제약). 턴 단위 정밀 탐지(uuid 컬럼)는 유예.

**임계값(struct 필드, 전부 튜닝 가능):**

```rust
pub struct R7OpusTrivial {
    pub max_output_tokens: u64,    // v0 = 700
    pub max_tool_calls: u64,       // v0 = 5
    pub savings_fraction_pct: u64, // v0 = 80  (§1.3 근거)
}
```

**evidence:**

```json
{ "model": "opus", "turns": 3, "tok_output": 420,
  "tool_calls": 2, "tools": ["file_read","file_edit"],
  "billable_tokens": 61000, "note": "비용-등가 추정(Opus↔Haiku 5:1 가격비)" }
```

- `model`: `model_family`(= `"opus"`). raw 모델 ID 문자열은 `events`에 저장되지 않으므로(store.rs `flatten`은 family/tier만 기록) family로 표시.
- `tools`: 그 세션에서 등장한 distinct `tool_kind` (오름차순).

**est_tokens_saved (비용등가):** `round(billable × savings_fraction_pct / 100)`, `billable = SUM(tok_input + tok_output + tok_cache_read + tok_cache_create)` (그 세션 전체). 상주 컨텍스트가 큰 세션일수록 Opus 프리미엄이 커 값이 커진다(크기 비례). "절약 토큰 수"가 아니라 "Haiku였다면 아낄 비용의 토큰-등가"임을 evidence.note로 명시.

**prescription:** `{ kind: "switch_model", payload: {"from":"opus","to":"haiku"} }` (기존 확립된 kind).

**severity:** `suggest`. (실시간 "명백한 모델 오남용" 넛지는 별도 스코프.)

**scope:** session. `scope_kind="session"`, `scope_ref=session_id`, `scope_host`, `scope_project`. `dedup_key = "R7|{session}"`.

### 2.2 R9 — 웹 도구 남용 (세션 단위)

**의도:** 한 세션에서 서버측 웹 도구(web_search/web_fetch)를 과도하게 호출 → 캐싱/로컬 소스로 왕복 토큰을 아낄 여지.

**탐지:** 세션별 `SUM(web_search + web_fetch)` (assistant_turn의 server_tool_use 카운트) `>= threshold`.

**임계값:**

```rust
pub struct R9WebOveruse {
    pub threshold: u64,                    // v0 = 15  (세션당 웹 호출 과다)
    pub heuristic_tokens_per_request: u64, // v0 = 2000
}
```

**evidence:**

```json
{ "web_search": 12, "web_fetch": 6, "total_requests": 18,
  "note": "세션당 서버 웹 도구 호출 과다" }
```

**est_tokens_saved:** `total_requests × heuristic_tokens_per_request`. 캐싱/로컬로 줄일 수 있는 웹 페이로드 근사(러프, 상한 성격 — 실제로 모든 웹 호출을 없애진 못함). 단조증가·단순. v0 이후 히스토리로 보정.

**prescription:** **없음(advice-only).** 근거: "캐싱/로컬"은 자동 적용 가능한 결정론적 액션이 아니므로 정밀도의 선(코칭 설계 §1.4)상 처방 카드(적용 버튼)로 승격 부적합. R5처럼 evidence + finding_advice만 제공. `Finding.prescription = None`.

**severity:** `suggest`. **scope:** session. `dedup_key = "R9|{session}"`.

### 2.3 finding_advice arm 추가 (`diary/mod.rs`)

`finding_advice(rule_id, evidence, est_tokens_saved) -> (detail, suggested_action)`에 R7·R9 arm 추가(기존 R1·R5 패턴 동일).

- **R7:**
  - detail: `"이 세션은 전부 Opus인데 출력 {tok_output}토큰·도구 {tool_calls}회의 가벼운 작업이었어요 (~{est}토큰 비용-등가)"`
  - action: `"이런 잔심부름은 Haiku로 전환하면 같은 결과를 훨씬 싸게 낼 수 있어요"`
- **R9:**
  - detail: `"이 세션에서 웹 도구를 {total_requests}회 호출했어요 (검색 {web_search}+페치 {web_fetch}, ~{est}토큰)"`
  - action: `"반복 조회는 결과를 캐싱하거나 로컬 소스(예: 로컬 문서·context7 캐시)를 쓰면 웹 왕복 토큰을 아껴요"`

evidence에서 필드 추출은 R1/R5 arm과 동일하게 `get(...).and_then(...)` 관대 파싱, 누락 시 합리적 기본값.

### 2.4 배선 (`main.rs::cmd_rules`)

```rust
let engine = RuleEngine::new(vec![
    Box::new(R5RepeatedRead::default()),
    Box::new(R1UnusedMcp::default()),
    Box::new(R7OpusTrivial::default()),
    Box::new(R9WebOveruse::default()),
]);
```

---

## 3. C) 플러그인 캐시 읽기 견고성

reconciliation 설계 §7·§7.1의 후속 티켓.

### 3.1 (a) `find_plugin_mcp_files` — 활성 버전(mtime 최신) 선택

**문제:** 현재는 한 플러그인의 **모든** 버전 dir의 `.mcp.json`을 읽어 union → 신버전에서 드롭된 서버가 구버전 dir에 잔존하면 R1 오탐(reconciliation §7). snapshot-replace로 안 고쳐진다(구버전 서버가 현재셋에 포함되어 유지됨).

**수정:** 버전 서브디렉터리 중 **mtime 최신 1개**의 `.mcp.json`만 읽는다. 비-semver 버전명(`unknown`/`0.44.0`/해시 혼재)이라 "최신 버전" 판별은 mtime으로.

- 테스트 용이성 위해 순수 헬퍼 분리:
  ```rust
  fn pick_active_version(candidates: Vec<(PathBuf, SystemTime)>) -> Option<PathBuf>
  ```
  최대 mtime의 경로 반환. 이걸 단위 테스트(fs 무관)하고, fs 열거는 얇게 감싼다.
- mtime 조회 실패 dir는 최하위 우선(비교에서 짐). 만약 모든 후보의 mtime을 못 얻으면 기존 동작(전체 읽기)으로 폴백 — 안전한 상위집합, 드묾.
- 반환 형태 변경: `Vec<PathBuf>` → `(Option<PathBuf>, bool /*complete*/)`. `complete=false`는 **버전 dir 열거(read_dir) 자체가 IO 에러**일 때만(플러그인 dir 부재(NotFound)는 `complete=true`, 서버 없음). 활성 `.mcp.json`의 read/parse 실패 판정은 이 파일을 실제로 읽는 `plugin_servers`(§3.2)가 담당한다.

### 3.2 (b) completeness 전파 + `cmd_inventory` 스킵

**문제:** 최상위 `read_json_guarded`는 `claude.json`·`settings.json`만 커버. 중첩 `.mcp.json`(`plugin_servers`의 플러그인 캐시, `resolve_project_servers`의 `enableAllProjectMcpServers` 프로젝트)이 **존재하나 읽기/파싱 실패**하면 부분셋 반환 → `replace_host_inventory`가 그 부분셋으로 파괴적 교체 → 여전히 활성인 서버가 사라져 R1 false-negative(reconciliation §7.1).

**수정(최상위 가드 철학을 중첩까지 확장):**

```rust
pub struct HostInventory {
    pub entries: Vec<(String, Vec<McpServer>)>,
    pub complete: bool,
}

fn plugin_servers(settings: &Value, plugins_cache_dir: &Path) -> (Vec<McpServer>, bool);
pub fn resolve_project_servers(cfg: &ProjectMcpConfig) -> (Vec<McpServer>, bool);
pub fn collect_host_inventory(
    claude_json: &Value, settings: &Value, plugins_cache_dir: &Path,
) -> HostInventory;   // complete = 모든 하위 complete의 AND
```

**completeness 판정 (`read_json_guarded` 미러):**

| 상황 | 판정 |
|---|---|
| 파일/디렉터리 **부재(NotFound)** | complete (정당한 빈 기여) — 캐시 dir 부재도 정당(서버 없는 플러그인 다수) |
| **존재하나 IO/파싱 실패** (read_dir 에러, `.mcp.json` read/parse 실패) | **incomplete** |

**`cmd_inventory` 변경:**

```rust
let inv = collect_host_inventory(&claude_json, &settings, &cache);
if !inv.complete {
    eprintln!("warn: host {} 중첩 .mcp.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
    continue;   // 기존 행 유지
}
store.replace_host_inventory(&hs.host, &inv.entries)?;
```

> **주의(reconciliation §7.1):** `enableAllProjectMcpServers` 경로는 실측상 항상 false(휴면)라 (b)의 프로젝트 부분은 사실상 방어적. 저확률·자가치유(다음 성공 실행 시 복구)지만 견고성을 위해 배선한다.

---

## 4. 에러 처리 (데이터 파운데이션 철학 일치)

- 파싱/읽기 실패는 관대 처리: R7/R9는 순수 집계 SQL이라 실패 없음. C는 "읽기 실패 → incomplete → 스킵(기존 유지)"으로 하드 실패 금지.
- DB/트랜잭션 오류는 `?`로 전파.

---

## 5. 테스트

### 5.1 R7 (`src/rules/r7_opus_trivial.rs`)

- `r7_flags_all_opus_trivial_session`: opus turn 2개(출력 합 <700) + file_read/file_edit tool_call → 지목, prescription=switch_model, est = billable×0.8.
- `r7_silent_when_mixed_model`: opus + sonnet turn 혼재 → 침묵.
- `r7_silent_when_heavy_tool`: opus + mcp_call(또는 sub_agent) tool_call → 침묵.
- `r7_silent_when_server_web_used`: opus turn에 web_search>0 → 침묵.
- `r7_silent_when_no_tool`: opus turn만, tool_call 0 → 침묵(순수 대화 제외).
- `r7_silent_when_output_too_large`: opus + tool_call이나 SUM(tok_output) ≥ 700 → 침묵.
- `r7_est_tokens_saved_is_cost_equivalent`: billable 검증(0.8 배).

### 5.2 R9 (`src/rules/r9_web_overuse.rs`)

- `r9_flags_session_over_threshold`: 한 세션 web_search+web_fetch 합 ≥15 → 지목, prescription=None, est = total×2000, evidence total/web_search/web_fetch.
- `r9_silent_under_threshold`: 합 <15 → 침묵.
- `r9_scoped_per_session`: 두 세션 각각 8·18 → 18 세션만 지목.

### 5.3 finding_advice (`src/diary/mod.rs` tests)

- `finding_advice_r7`: detail에 tok_output·tool_calls·est 포함, action 비어있지 않음.
- `finding_advice_r9`: detail에 total_requests·web_search·web_fetch 포함, action 비어있지 않음.

### 5.4 C (`src/inventory.rs`, `src/main.rs` tests)

- `pick_active_version_picks_latest_mtime`: (path, mtime) 목록 순수 함수 테스트 — 최대 mtime 경로. 동점/빈 목록 처리.
- `find_plugin_mcp_files_reads_only_active_version`: 다중 버전 dir(mtime 상이, `filetime` dev-dep로 명시 설정) → 최신 dir의 서버만.
- `plugin_servers_complete_when_files_ok`: 정상 → complete=true.
- `plugin_servers_incomplete_on_corrupt_mcp_json`: 활성 `.mcp.json`이 깨진 JSON → complete=false.
- `plugin_servers_complete_when_cache_dir_absent`: 캐시 dir 부재 → complete=true, 빈 기여.
- `resolve_project_servers_incomplete_on_corrupt_when_enable_all`: enable_all=true + 프로젝트 `.mcp.json` 깨짐 → complete=false.
- `collect_host_inventory_complete_is_and_of_parts`: 하나라도 incomplete면 전체 incomplete.
- 기존 테스트(`plugin_servers_*`, `collect_host_inventory_*`) 구조분해를 튜플/struct 반환에 맞게 업데이트(단일 버전 케이스라 값 불변).
- cmd_inventory 스킵 통합: 스모크/수동(중첩 `.mcp.json` 하나 손상 후 `inventory` 재실행 → 그 호스트 기존 행 유지 확인).

---

## 6. Non-goals (이 설계에서 안 함)

- R3/R4/R8 (ToolResult/UserPrompt 이벤트 선행 필요).
- R7 턴 단위 정밀 탐지(events에 `uuid` 컬럼 추가 + 재수집).
- 규칙별 `est_tokens_saved` 사용자 히스토리 기반 임계값 보정(코칭 §9 유예 유지).
- 옵트인 정확 프로브 / 서버별 정확 귀속(데이터 파운데이션 §8 유예).
- R9 URL 단위 중복 탐지(server_tool_use는 카운트만, target 없음).

---

## 7. 첫 스프린트 (구현 순서)

1. R7·R9 규칙 + 단위 테스트 + finding_advice arm → *검증: 실엔진 없이 in-memory 스토어로 참 Finding.*
2. cmd_rules 배선 → *검증: 실 히스토리에서 R7/R9가 참으로 뜨나(.env.ref 실엔진 다이어리로 서사 확인).*
3. C (a) pick_active_version + find_plugin_mcp_files → (b) completeness 전파 + cmd_inventory 스킵 → *검증: 다중 버전/손상 케이스 단위 테스트 + 스모크.*
