# Agent Mentor — 인벤토리 Reconciliation 설계

> **상태**: 브레인스토밍 합의 완료 (2026-07-02)
> **범위**: `mcp_inventory`의 stale 행으로 인한 R1 오탐 수정. 데이터 파운데이션(PR #3, `feat/data-foundation`)의 후속 보정.
> **관계**: 데이터 파운데이션 설계(`docs/specs/2026-07-01-data-foundation-design.md` §5 스키마, §8 R1)를 전제한다.

---

## 0. 한 줄

`cmd_inventory` 재실행 시 각 호스트의 `mcp_inventory`를 **현재 활성 설정셋으로 원자적 교체(snapshot-replace)**해, 설정에서 제거된 MCP 서버·플러그인·프로젝트의 stale 행이 R1을 오탐시키지 않게 한다.

---

## 1. 문제

- `store::upsert_inventory`는 `INSERT ... ON CONFLICT DO UPDATE`만 하고 **DELETE가 없다**. 설정에서 MCP 서버를 빼거나 플러그인을 비활성화해도 `mcp_inventory` 행이 잔존한다.
- R1은 `mcp_inventory` 전 행을 읽어 "설정됐는데 미사용(호출 0 + 상주 비용)"을 지목한다. stale 행이 남으면 **이미 제거된 서버를 "제거하라"고 계속 지목** → 오탐.
- 정확한 오탐 조건: **한 번도 호출하지 않고 제거한 서버**(calls=0). 쓰다가 제거한 서버는 전체 기간 calls>0라 R1이 애초에 지목하지 않으므로 무해하다.
- stale 원천 3가지: (a) 프로젝트에서 서버 제거, (b) 플러그인 비활성화(`(*, server)` 잔존), (c) 프로젝트 자체가 claude.json에서 제거(그 행들이 재계산 대상에서 빠져 영원히 잔존).

---

## 2. 확정 결정

1. **snapshot-replace (Option B)** — 호스트별로 `mcp_inventory` 전 행을 삭제한 뒤 현재 활성셋을 재삽입한다. mark-and-sweep(A)·generation 마커(C)는 기각.
   - 근거: 옵트인 정확 프로브(데이터 파운데이션 §8, 서버별 `est_def_tokens` 캐싱)는 **유예** 상태라 오늘 보존해야 할 행별 컬럼이 없다(`est_def_tokens`/`probed_at`/`last_used_ts` 전부 NULL). YAGNI.
   - B→A 전환 비용은 작다: 프로브가 도입되면 `DELETE WHERE host=?` 를 "현재셋에 없는 것만 삭제"로 바꾸면 된다.
2. **원자적 교체(트랜잭션)** — DELETE와 재INSERT를 한 트랜잭션으로 묶는다. 중간 크래시 시 인벤토리가 빈 채로 남는 것을 방지.
3. **읽기 성공 가드** — 온라인 호스트인데 설정 읽기가 **일시 실패**하면(WSL UNC 글리치 등) 현재셋이 비어 오삭제 → R1 침묵(false negative) 위험. 따라서 `claude.json`·`settings.json`이 **(읽기+파싱 성공) 또는 (부재)** 일 때만 reconcile 한다. **존재하나 IO/파싱 실패**면 그 호스트를 스킵(기존 행 유지 + 경고 로깅). 오프라인 distro는 `enumerate_hosts`가 애초 열거하지 않으므로 이미 안전 — 남는 위험은 "온라인인데 읽기 일시 실패"뿐이고 이 가드가 그것만 처리한다.
4. **`events` 불변** — reconciliation은 `mcp_inventory`(현재 설정 스냅샷)만 건드린다. 사용 이력(`events`, 과거 `mcp__server__tool` 호출 등)은 역사적 사실이라 절대 삭제하지 않는다. 다이어리/통계가 이 이력에 의존한다.

---

## 3. 변경

### 3.1 `store.rs`
```rust
/// 한 호스트의 인벤토리를 현재 셋으로 원자 교체(트랜잭션: DELETE 후 재INSERT).
pub fn replace_host_inventory(
    &mut self,
    host: &str,
    entries: &[(String, Vec<crate::inventory::McpServer>)],
) -> Result<()>;
```
- 트랜잭션 안에서 `DELETE FROM mcp_inventory WHERE host=?1` 후 entries 전부 insert.
- 호스트 스코프 삭제라 다른 호스트 행은 불변.
- rusqlite `transaction()`은 `&mut Connection`을 요구 → `SqliteStore` 메서드는 `&mut self`. 기존 `upsert_inventory`는 유지(다른 경로/테스트에서 사용 가능).

### 3.2 `main.rs`
```rust
/// 읽기+파싱 성공 → Some(Value) / 파일 부재(NotFound) → Some(Null)(정당한 빈 설정)
/// / 존재하나 IO·파싱 실패 → None(스킵 신호).
fn read_json_guarded(path: &Path) -> Option<serde_json::Value>;
```
(구현은 `Result<Value,()>` 대신 관용적인 `Option<Value>` 사용 — 의미 동일, 유닛 에러 타입 회피.)
- `cmd_inventory`가 호스트별로 `claude.json`·`settings.json`을 `read_json_guarded`로 읽는다.
- **둘 중 하나라도 `None`**이면 `eprintln!` 경고 후 그 호스트 스킵(reconcile·upsert 안 함, 기존 행 유지).
- 둘 다 `Some`(값 또는 Null)이면 `collect_host_inventory` → `replace_host_inventory(host, &inv)`.
- `SqliteStore`를 `&mut`로 넘기도록 `cmd_*`/`main` 배선 조정.

---

## 4. 데이터 흐름

```
enumerate_hosts()
  └─ 각 host:
       read_json_guarded(claude.json), read_json_guarded(settings.json)
         ├─ 하나라도 None → eprintln 경고 + 스킵(기존 mcp_inventory 행 유지)
         └─ 둘 다 Some(값/Null) → collect_host_inventory(claude, settings, cache)
                             → replace_host_inventory(host, &inv)   // 트랜잭션: DELETE WHERE host + 재INSERT
```

---

## 5. 에러 처리

- **설정 읽기 실패(존재하나 IO/파싱)** → 그 호스트 스킵, 기존 행 유지, `eprintln!` 경고. (관대: 하드 실패 금지.)
- **파일 부재(NotFound) 또는 빈/공백 파일** → `Value::Null` → 빈 설정으로 정당 처리(그 스코프는 비게 됨). (빈/공백은 설정 초기화 등 정당한 케이스로, `read_to_string`은 성공하나 파싱은 실패하므로 명시적으로 Null 처리.)
- **트랜잭션/DB 오류** → `?`로 전파. 인프라 오류는 전파(데이터 파운데이션의 "파싱은 관대, DB는 전파" 철학과 일치).

---

## 6. 테스트

- `replace_host_inventory_drops_stale_keeps_other_hosts`: host H에 `(P,"A")`,`(P,"stale")` 시드 → `replace_host_inventory("H", &[("P", [A])])` → `(P,"A")`만 남고 `(P,"stale")` 제거; 다른 host `H2` 행 불변.
- `replace_host_inventory_empty_clears_host`: `replace_host_inventory("H", &[])` → H 행 전부 제거(빈 설정 반영). (호출자는 읽기 성공 시에만 호출하므로 빈 셋 = 정당한 "설정 없음".)
- `read_json_guarded`: 유효 JSON 파일 → `Ok(Value)`; 부재 경로 → `Ok(Null)`; 깨진 JSON 파일 → `Err(())`.
- 읽기 가드 통합("둘 중 하나 Err → 스킵")은 스모크/수동 확인: 설정에서 미사용 서버 하나를 빼고 `inventory` 재실행 → R1이 그 서버를 더는 지목하지 않는지.

---

## 7. Non-goals (이 설계에서 안 함)

- 옵트인 정확 프로브 / 차등 귀속 (데이터 파운데이션 §8, 유예 유지).
- 설정 변경 히스토리·"주인이 X를 뺐다" 코칭 신호 (별도 스코프; 파괴적 삭제 대신 generation 마커가 필요).
- mark-and-sweep 방식의 행별 컬럼 보존 (프로브 도입 전까지 불필요).
- `find_plugin_mcp_files`의 다중 버전 디렉터리 중 "활성 버전" 선택 — 이는 **현재셋 계산 정확도** 문제이지 삭제 로직 문제가 아니라 snapshot-replace로 고쳐지지 않는다(구버전 dir의 서버가 현재셋에 포함되어 그대로 유지됨). 드물고("플러그인이 버전 간 MCP 서버를 드롭 + 두 캐시 dir 잔존") "최신 버전" 판별이 비-semver(`unknown`/`0.44.0`/해시 혼재)라 비자명 → 별도 유예. 필요 시 mtime 최신 dir로 별도 처리.

---

## 7.1 알려진 한계 · 후속 (플러그인 캐시 읽기 견고성)

읽기 성공 가드(§2.3)는 **최상위** 설정 파일(`claude.json`·`settings.json`)만 커버한다. 그러나 `collect_host_inventory`는 내부적으로 **중첩** `.mcp.json`을 더 읽는다 — `plugin_servers`가 플러그인 캐시 `.mcp.json`을, `resolve_project_servers`가 `enableAllProjectMcpServers=true`일 때 프로젝트 `.mcp.json`을. 이 중첩 읽기는 실패 시 **조용히 스킵**한다. 따라서 최상위 파일은 읽혔는데 중첩 `.mcp.json` 하나가 일시 실패/손상되면 `collect_host_inventory`가 **부분셋**을 반환하고, `replace_host_inventory`가 그 부분셋으로 파괴적 교체 → 여전히 활성인 서버가 사라져 R1 false-negative(다음 성공 실행까지).

**판정: 후속 유예**(Codex 리뷰 지목). 근거: (a) 저확률 — 중첩 파일은 `settings.json`과 같은 `.claude` 마운트라 마운트 하이컵 시 최상위 가드가 먼저 잡음; 부모 성공+자식만 실패는 드묾. (b) 자가치유 — 다음 성공 실행 시 복구. (c) `enableAllProjectMcpServers` 경로는 현재 실측상 항상 false(휴면). (d) P2. **수정 방향**(후속): `plugin_servers`/`resolve_project_servers`/`collect_host_inventory`에 completeness 신호를 전파하고 `cmd_inventory`가 불완전 호스트를 스킵. `find_plugin_mcp_files` 다중 버전 이슈(§7)와 함께 "플러그인 캐시 읽기 견고성" 후속 티켓으로 묶는다.

---

## 8. 첫 스프린트

1. `replace_host_inventory` + 단위 테스트 → *검증: stale 행 제거, 타 호스트 불변, 빈 입력 시 호스트 클리어.*
2. `read_json_guarded` + `cmd_inventory` 배선(&mut 조정) → *검증: 설정에서 미사용 서버 하나를 빼고 `inventory` 재실행 시 R1이 그 서버를 안 지목; 읽기 실패 호스트는 기존 행 유지.*
