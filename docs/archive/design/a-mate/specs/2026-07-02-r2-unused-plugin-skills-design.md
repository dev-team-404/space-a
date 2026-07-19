---
status: done
archived: 2026-07-19
---

# R2 — 미사용 플러그인(스킬 제공) 규칙 설계

**Goal:** Tier 0 규칙 R2("미사용 plugin/skill 상주 비용", coaching-intelligence-design §5)를 구현한다. enabled이지만 그 스킬·MCP를 한 번도 쓰지 않은 플러그인을 **플러그인별로 집계**해 지목하고, `disable_plugin` 처방을 제시한다.

**참조:** `docs/specs/2026-07-01-coaching-intelligence-design.md` §5(R2 행), R1 구현(`src/rules/r1_unused_mcp.rs`), 인벤토리(`src/inventory.rs`), 어댑터(`src/adapter.rs`).

---

## 1. 배경 & R1과의 관계

R1은 `mcp_inventory`의 MCP **서버** 중 `mcp_call` 0회 + 상주비용 유의미한 것을 서버 단위로 지목한다(플러그인 출처 `source="plugin"` 포함). 그러나:

- 인벤토리는 **MCP 서버만** 추적한다. 스킬/슬래시커맨드만 제공하고 MCP 서버가 없는 플러그인(예: `superpowers`, `sc`)은 인벤토리에 전혀 안 잡혀 상주 비용이 보이지 않는다.
- 스킬 하나의 상주 비용(시스템프롬프트의 name+description 한 줄 ≈ 수십 토큰)은 작지만, **enabled 플러그인이 제공하는 스킬 수십 개**가 매 세션 상주하면 합산 비용은 유의미하다.

**R2 = 스킬을 제공하는 enabled 플러그인 중, 그 플러그인의 어떤 것도 쓰지 않은 것**을 플러그인별 집계로 지목한다. R1과 겹치지 않는다:

- MCP만 제공하는 플러그인의 미사용 서버 → **R1**(서버 단위)이 담당. R2는 스킬 제공 플러그인만 본다.
- 플러그인의 MCP 서버가 **쓰이면** R2는 침묵한다(플러그인이 사용 중이므로 "끄라"는 위험 처방 방지).

별도의 "플러그인 단위 미사용" 규칙은 만들지 않는다(R1 + R2로 커버되어 중복).

## 2. 범위

**IN:**
- 어댑터가 `Skill` 툴 호출의 스킬명을 이벤트에 캡처(스킬 사용 신호).
- enabled 플러그인의 활성버전 `skills/*/SKILL.md`(+ `.mcp.json`) 스캔 → 신규 `plugin_inventory` 테이블(플러그인별 스킬 수·상주 토큰 추정·MCP 서버명), host 단위 원자 교체 + completeness 전파.
- R2 규칙 + `finding_advice` R2 arm + `cmd_rules` 등록 + 수집 배선.

**OUT(유예):**
- 사용자/프로젝트 로컬 스킬(`~/.claude/skills/`, `<project>/.claude/skills/`) — 끌 "플러그인"이 없어 처방 불가. R2 대상 아님.
- 슬래시커맨드/훅/에이전트 등 스킬 외 플러그인 제공물의 상주 비용(스킬만 v0 대상).
- 정확한 토큰 카운트(BPE) — chars/4 heuristic으로 근사(R1의 flat heuristic과 동일 정신).
- 스킬 사용의 세밀 귀속(어느 세션/프로젝트) — R2는 host 단위 "썼나/안 썼나"만.

## 3. 데이터 모델

### 3.1 스킬 사용 신호 (어댑터 확장)

현재 `src/adapter.rs`의 tool_use 파싱은 `target`에 `input.file_path`/`input.command`만 담고, `Skill` 툴은 `ToolKind::Other("Skill")`로 떨어져 **어느 스킬인지 유실**된다.

- `ToolKind`에 `Skill { name: String }` 배리언트 추가.
- `ToolKind::from_raw_name`은 `"Skill"`을 별도 처리하지 않는다(입력이 필요하므로). 대신 어댑터 파싱 루프에서 `raw_name=="Skill"`이면 `input.skill`(문자열)을 읽어 `ToolKind::Skill { name }`으로 만들고 `target`에도 스킬명을 기록한다.
- `tool_kind_str`(`src/store.rs`)에 `Skill { .. } => "skill"` arm 추가. events 테이블 스키마 변경 없음(기존 `tool_kind`/`tool_target` 컬럼 사용).
- 스킬명 형식: `"plugin:skill"`(예: `superpowers:brainstorming`). 네임스페이스(첫 `:` 앞)가 플러그인을 가리킨다.

### 3.2 플러그인 인벤토리 (스캔 + 테이블)

R2는 플러그인 단위로 동작하므로 인벤토리도 **per-plugin**으로 둔다(스킬 목록·MCP 서버명을 함께 담아, R2가 DB만으로 "스킬 사용 OR MCP 사용"을 판정하고 R1의 `mcp_inventory`를 건드리지 않게 한다).

**스캔:** `settings.json`의 `enabledPlugins`(값 `true`)마다 캐시 `<cache>/<marketplace>/<plugin>/<활성버전>/`를 읽는다.
- 활성버전 선택은 Task 6의 `pick_active_version`(mtime 최신) 재사용.
- **스킬**: `skills/*/SKILL.md`의 frontmatter `name`·`description`을 파싱, `resident_tokens ≈ (len(name)+len(description)) / 4`로 추정하여 스킬명·토큰 합산.
- **MCP 서버**: 그 플러그인의 `.mcp.json`을 기존 `find_plugin_mcp_files`/`parse_mcp_json`로 읽어 서버명 목록 확보(R1 비충돌 판정용).
- **스킬이 하나도 없는 플러그인(MCP 전용)은 `plugin_inventory`에 넣지 않는다** — R2 대상이 아니고 R1이 담당.

**테이블:** 신규
```sql
CREATE TABLE IF NOT EXISTS plugin_inventory (
  host TEXT NOT NULL,
  plugin_key TEXT NOT NULL,        -- "name@marketplace"
  namespace TEXT NOT NULL,         -- 스킬 호출 네임스페이스(v0: plugin name = key의 '@' 앞)
  skill_count INTEGER DEFAULT 0,
  resident_tokens INTEGER DEFAULT 0,
  skills_json TEXT NOT NULL,       -- JSON 배열: 스킬 로컬명(evidence·상위 N개)
  mcp_servers_json TEXT NOT NULL,  -- JSON 배열: 이 플러그인이 제공하는 MCP 서버명(사용 판정용)
  PRIMARY KEY (host, plugin_key)
);
```

**reconciliation/completeness:** R1 인벤토리(`replace_host_inventory`)와 동일 패턴 — host 단위 트랜잭션 DELETE+재INSERT로 stale 제거. 스캔이 IO/파싱 실패로 불완전하면 `HostInventory`처럼 `complete=false`를 전파하고, 수집 배선(§5)이 불완전 호스트의 `plugin_inventory` 교체를 스킵(파괴적 부분 교체로 인한 R2 오탐 방지).

### 3.3 스코프

`enabledPlugins`는 `~/.claude/settings.json`(호스트 단위), 스킬은 그 호스트 전 세션에 상주 → R2는 **host-global**. `scope_kind="host"`, `scope_ref=host`. R1의 글로벌 `"*"` 관례와 정합.

## 4. R2 규칙 로직

`R2UnusedPluginSkills { min_resident_tokens: u64, heuristic_note }` + `Default`(`min_resident_tokens=300`) + `impl Rule`.

**판정** (host × plugin_key 단위, `plugin_inventory` 각 행):
1. 행에서 `namespace`, `skill_count`, `resident_tokens`, `skills_json`, `mcp_servers_json`을 읽는다.
2. **사용됨** = 다음 중 하나라도 참이면:
   - 그 플러그인 네임스페이스의 스킬 호출 ≥1: `events` 중 `host=? AND kind='tool_call' AND tool_kind='skill' AND tool_target LIKE '{namespace}:%'`.
   - 그 플러그인 제공 MCP 서버 호출 ≥1(R1 비충돌): `mcp_servers_json`의 서버 중 하나라도 `events`에 `host=? AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?` 존재.
3. 사용됨이면 continue(침묵 — "쓰는 플러그인 끄기" 방지).
4. `resident_tokens < min_resident_tokens`면 continue.
5. 그 외 → Finding.

**Finding:**
- `rule_id="R2"`, `severity=Severity::Suggest`, `scope_host=Some(host)`, `scope_project=None`, `scope_kind="host"`, `scope_ref=host`.
- `evidence = { "plugin": plugin_key, "skill_count": n, "skills": [top 스킬명…], "resident_tokens": total, "note": "약(~) 추정 — chars/4 heuristic" }`.
- `est_tokens_saved = total resident_tokens`(세션당 절감 근사).
- `prescription = Some(Prescription { kind: "disable_plugin", payload: { "plugin": plugin_key } })` — `settings.json`의 `enabledPlugins["name@marketplace"]=false`로 적용 가능한 결정론적 액션.
- `dedup_key = "R2|{host}|{plugin_key}"`.

**네임스페이스 매칭:** 스킬 호출 이벤트의 `tool_target`은 `"<namespace>:<skill>"`. `plugin_inventory.namespace`(v0: `plugin_key`의 `@` 앞 = plugin name)로 `LIKE '{namespace}:%'` 매칭한다. name↔네임스페이스 불일치 가능성은 §8에서 실 히스토리로 검증하고, 필요 시 스캔 단계에서 `namespace`를 별칭 매핑으로 보정한다.

## 5. 다이어리 / 배선

- **`finding_advice`**(`src/diary/mod.rs`) R2 arm: evidence `{plugin, skill_count, resident_tokens}` 사용.
  - detail: `"플러그인 {plugin}의 스킬 {n}개(~{resident_tokens}토큰)를 한 번도 쓰지 않았어요"`.
  - action: `"안 쓰는 플러그인은 설정에서 비활성화하면 매 세션 상주 토큰을 아껴요"`.
- **`cmd_rules`**(`src/main.rs`): `RuleEngine::new` 벡터에 `R2UnusedPluginSkills::default()` 등록.
- **수집 배선**(`src/main.rs::cmd_inventory` 또는 인접): 각 호스트에 대해 플러그인 스캔(스킬+MCP서버) → `plugin_inventory` host 단위 원자 교체. 불완전(complete=false) 호스트는 스킵(경고 eprintln, R1 인벤토리 스킵 관례와 동일).

## 6. 테스트 전략

각 부분 TDD(RED 실제 캡처):
- **어댑터**: `Skill` 툴콜(input.skill="superpowers:brainstorming") → `ToolKind::Skill{name}`·`target` 캡처. 비-Skill 툴 회귀 없음.
- **플러그인 스캔**: tempdir에 `<plugin>/<ver>/skills/<s>/SKILL.md`(+선택적 `.mcp.json`) 생성, frontmatter 파싱·토큰 추정·활성버전 선택·MCP 서버명 수집 확인. 손상 SKILL.md → completeness=false. MCP만 있고 스킬 없는 플러그인 → `plugin_inventory` 제외.
- **R2 규칙**: (a) 미사용 스킬 플러그인 지목(집계 상주≥임계), (b) 스킬 1회라도 호출 시 침묵, (c) 그 플러그인 MCP 서버 호출 시 침묵(R1 비충돌), (d) 상주<임계 시 침묵, (e) host 스코프·`disable_plugin` 처방·dedup_key 검증.
- **completeness**: 스캔 실패 호스트 `plugin_inventory` 교체 스킵.
- 전체 `cargo test` 무경고 유지.

## 7. Non-goals (재확인)

§2 OUT 참조. 특히 사용자/프로젝트 로컬 스킬, 스킬 외 제공물, 정밀 BPE 카운트, 세밀 사용 귀속은 v0 범위 밖.

## 8. 열린 세부 — 실 히스토리 검증

스킬 호출 네임스페이스(예: `sc:analyze`의 `sc`)가 플러그인 디렉터리/`enabledPlugins` name과 다를 수 있다. 구현 후 `.env.ref` 실엔진 e2e로:
- 실제 `Skill` 툴콜이 이벤트에 `tool_kind='skill'`·`tool_target='<ns>:<skill>'`로 남는지,
- 네임스페이스가 `enabledPlugins`의 `name`과 매칭되는지

확인하고, 불일치 시 매칭 키(네임스페이스 별칭 매핑)를 보정한다. (R1/R5/R7/R9가 v0 이탈을 실검증으로 확정한 관례와 동일.)
