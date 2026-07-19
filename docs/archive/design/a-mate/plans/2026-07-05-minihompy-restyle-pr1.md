---
status: done
archived: 2026-07-19
---

# 미니홈피 대개편 PR① 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** chat 창을 클래식 미니홈피 레이아웃(타이틀바+TODAY/TOTAL, 좌측 로봇 초상+기분, 우측 세로 탭) + 파스텔 모던 스킨으로 전환하고, 홈(위젯 4종+마이룸)·코칭(advice 카드+명령 복사+상태 관리)·다이어리(달력) 탭을 완성하며, 마스코트 클릭→창 열기와 말풍선 정책(유의미·지속·X 닫기·파스텔)을 개편한다.

**Architecture:** 백엔드는 기존 테이블 재사용(신규 스키마 없음) — `findings.status` 컬럼으로 상태 관리, `daily_rollup`/`events`로 주간·모델 위젯. core에 결정론 함수(`coach::fix_command`)와 쿼리를 추가하고 src-tauri 커맨드로 배선. 프론트는 컴포넌트 구조 유지·확장 + 전역 CSS 토큰(`theme.css`)으로 재스킨. 순수 로직(달력·알림 로그·클릭/드래그 판별·말풍선 팩토리)은 별도 ts 모듈 + vitest.

**Tech Stack:** Tauri v2 + Rust(rusqlite), Svelte 5(runes) + Vite, vitest. 신규 npm 의존성: `marked`, `dompurify`(다이어리 마크다운 렌더+살균)만.

**스펙:** `docs/specs/2026-07-05-minihompy-restyle-design.md` (PR① = 스펙 §1·§2·§3·§4·§6·§8·§9)

## Global Constraints

- **빌드 환경 (모든 cargo 명령 전, Git Bash 필수):**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- Tauri **v2 API만**(v1 금지). Windows 전용(플랫폼 분기 불필요).
- store 접근 커맨드는 반드시 `#[tauri::command(async)]`(동기 커맨드는 메인 스레드 프리즈 — 1단계 학습).
- **에러 철학**: 말풍선으로 에러를 알리지 않는다. 파이프라인은 `eprintln!` + 조용한 재시도, 커맨드는 `Result<T, String>`.
- **스킨**: 파스텔 모던만 — 픽셀 폰트·3px 도트 보더·하드 섀도 금지. 색/radius/그림자는 `src/lib/theme.css` 토큰만 사용.
- UI 문구는 한국어, 마스코트 화법은 1인칭("주인" 호칭) 유지.
- 기존 테스트 무회귀: cargo workspace(core 98+app 4) + vitest. 수치는 태스크마다 늘어난다.
- 커밋은 태스크마다(스텝 안에 명시), 메시지는 한국어 요지 + `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

## 파일 구조 (전체 조감)

```
crates/core/src/
  coach.rs                     # 신규: fix_command (결정론, 테스트)
  store.rs                     # FindingRow.status, list_findings_current(include_hidden),
                               # set_finding_status, sum_est_tokens_saved(active만), model_mix_for_date
  lib.rs                       # pub mod coach;
src-tauri/src/
  commands.rs                  # user_name, CoachFinding, set_finding_status, get_week_summary,
                               # get_model_mix, get_today_occasions(+공유 헬퍼)
  geometry.rs                  # 신규: sanitize_pos (모니터 클램핑 순수 함수, 테스트)
  lib.rs                       # 커맨드 등록 + mascot 위치 복원에 sanitize_pos 적용
  pipeline.rs                  # list_findings_current(false) 호출 갱신, occasion 헬퍼 공유
src/
  lib/theme.css                # 신규: 파스텔 토큰 (양쪽 창 공용)
  App.svelte                   # 미니홈피 레이아웃 전면 개편 + 알림 기록 배선
  lib/notices.ts(+.test.ts)    # 신규: 알림 로그 순수 로직 + localStorage
  lib/ui/RobotPortrait.svelte  # 신규: 좌측 컬럼 로봇 초상
  lib/ui/HomeTab.svelte        # 개편: 스트립+위젯 그리드+마이룸+상태줄
  lib/ui/home/WeekTrend.svelte / ModelMix.svelte / SaveTop3.svelte / NoticeLog.svelte  # 신규 위젯
  lib/ui/MiniRoom.svelte       # 신규: 파스텔 방+128px 로봇+말풍선
  lib/ui/CoachTab.svelte       # 개편: advice 카드+복사+상태 관리
  lib/ui/DiaryTab.svelte       # 신규: 달력+본문
  lib/ui/calendar.ts(+.test.ts)# 신규: 월 그리드 순수 로직
  lib/api.ts                   # 타입·커맨드 추가
  Mascot.svelte                # 개편: 클릭/드래그, 지속 말풍선+X, occasion pull
  lib/robot/drag.ts(+.test.ts) # 신규: 클릭/드래그 임계값 판별
  lib/robot/bubble.ts(+.test)  # 개편: scan/BubbleQueue 제거, adviceBubble 추가
  lib/robot/anim.ts(+.test)    # 'scan' arm 제거
```

**태스크 경계 노트**: Task 1이 core 시그니처를 바꾸면서 src-tauri 호출부(pipeline.rs 1줄)도 같이 고쳐 매 태스크 종료 시 workspace가 항상 그린이다.

---

### Task 1: core — coach 모듈 + findings 상태·모델 분포 쿼리

**Files:**
- Create: `crates/core/src/coach.rs`
- Modify: `crates/core/src/lib.rs` (mod 선언 1줄)
- Modify: `crates/core/src/store.rs` (FindingRow, list_findings_current, sum_est_tokens_saved, 신규 2 fn)
- Modify: `src-tauri/src/pipeline.rs:85` (호출부 시그니처 맞춤)

**Interfaces:**
- Consumes: 기존 `findings` 테이블(`status TEXT NOT NULL DEFAULT 'new'`), `events`(`model_tier`, `ts` RFC3339, 토큰 컬럼).
- Produces (후속 태스크가 그대로 사용):
  - `agent_mentor::coach::fix_command(rule_id: &str, evidence: &serde_json::Value) -> Option<String>`
  - `FindingRow`에 `pub status: String` 필드 추가
  - `SqliteStore::list_findings_current(&self, include_hidden: bool) -> Result<Vec<FindingRow>>` — `false`면 `status='new'`만
  - `SqliteStore::set_finding_status(&self, dedup_key: &str, status: &str) -> Result<bool>` — 존재하면 true
  - `SqliteStore::sum_est_tokens_saved(&self) -> Result<u64>` — **`status='new'`만 합산으로 변경**(숨긴 건 절약가능 합계에서 제외)
  - `SqliteStore::model_mix_for_date(&self, date: &str) -> Result<Vec<(String, u64)>>` — (model_tier, tok_input+tok_output 합) 내림차순

- [ ] **Step 1: coach.rs 실패 테스트 작성**

`crates/core/src/coach.rs` 신규 생성:

```rust
//! 코칭 액션 — finding을 해결하는 복사 가능한 CLI 명령을 결정론적으로 생성.
//! 스펙 §3: R1/R2는 정의 위치와 무관하게 동작하는 명령 복사가 파일 열기보다 정확.

use serde_json::Value;

/// rule_id + evidence에서 해결 명령을 생성. R5/R9는 습관 교정이라 None.
pub fn fix_command(rule_id: &str, evidence: &Value) -> Option<String> {
    match rule_id {
        "R1" => evidence
            .get("server")
            .and_then(|v| v.as_str())
            .map(|s| format!("claude mcp remove {s}")),
        "R2" => evidence
            .get("plugin")
            .and_then(|v| v.as_str())
            .map(|p| format!("claude plugin disable {p}")),
        "R7" => Some("/model haiku".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::fix_command;
    use serde_json::json;

    #[test]
    fn r1_r2_r7_commands() {
        assert_eq!(
            fix_command("R1", &json!({"server": "playwright"})).as_deref(),
            Some("claude mcp remove playwright")
        );
        assert_eq!(
            fix_command("R2", &json!({"plugin": "vercel@claude-plugins-official"})).as_deref(),
            Some("claude plugin disable vercel@claude-plugins-official")
        );
        assert_eq!(fix_command("R7", &json!({})).as_deref(), Some("/model haiku"));
    }

    #[test]
    fn advice_only_and_missing_evidence_are_none() {
        assert_eq!(fix_command("R5", &json!({"path": "a.md"})), None);
        assert_eq!(fix_command("R9", &json!({})), None);
        assert_eq!(fix_command("R1", &json!({})), None); // server 키 없으면 None
        assert_eq!(fix_command("RX", &json!({})), None);
    }
}
```

`crates/core/src/lib.rs`의 기존 `pub mod` 나열부에 한 줄 추가(알파벳 순서 위치):

```rust
pub mod coach;
```

- [ ] **Step 2: 실패 확인**

```bash
cd /d/Project/agent-mentor && cargo test -p agent-mentor coach
```
Expected: 컴파일 성공·테스트 통과(신규 파일이라 RED 단계가 없음 — 테스트와 구현이 한 파일. 대신 아래 store 변경이 기존 테스트 게이트를 가짐)

- [ ] **Step 3: store.rs — status 표면 실패 테스트 먼저**

`crates/core/src/store.rs`의 `#[cfg(test)] mod tests`(파일 하단)에 추가:

```rust
    #[test]
    fn finding_status_roundtrip_and_filter() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let f = |key: &str| Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "srv".into(),
            evidence: serde_json::json!({"server": "srv"}),
            est_tokens_saved: 100, prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&f("k1"), "2026-07-05T00:00:00Z").unwrap();
        store.upsert_finding(&f("k2"), "2026-07-05T00:00:00Z").unwrap();

        // 기본: 둘 다 new
        assert_eq!(store.list_findings_current(false).unwrap().len(), 2);
        assert_eq!(store.list_findings_current(false).unwrap()[0].status, "new");

        // dismiss → active에서 빠지고 include_hidden엔 남음
        assert!(store.set_finding_status("k1", "dismissed").unwrap());
        assert_eq!(store.list_findings_current(false).unwrap().len(), 1);
        assert_eq!(store.list_findings_current(true).unwrap().len(), 2);
        // 없는 키는 false
        assert!(!store.set_finding_status("nope", "resolved").unwrap());

        // 재관측(upsert)돼도 status 유지
        store.upsert_finding(&f("k1"), "2026-07-05T01:00:00Z").unwrap();
        let all = store.list_findings_current(true).unwrap();
        let k1 = all.iter().find(|r| r.dedup_key == "k1").unwrap();
        assert_eq!(k1.status, "dismissed");
        assert_eq!(k1.occurrences, 2);

        // 절약가능 합계는 active만
        assert_eq!(store.sum_est_tokens_saved().unwrap(), 100);
    }

    #[test]
    fn model_mix_for_date_groups_by_tier() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = |uuid: &str, model: &str, inp: u64, out: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-05T10:00:00Z".into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { input: inp, output: out, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        };
        store.upsert_events(&[
            ev("u1", "claude-opus-4-8", 100, 50),
            ev("u2", "claude-opus-4-8", 10, 5),
            ev("u3", "claude-haiku-4-5-20251001", 20, 10),
        ]).unwrap();
        let mix = store.model_mix_for_date("2026-07-05").unwrap();
        assert_eq!(mix[0], ("opus".to_string(), 165)); // 100+50+10+5, 내림차순 첫 항목
        assert_eq!(mix[1], ("haiku".to_string(), 30));
        assert!(store.model_mix_for_date("2099-01-01").unwrap().is_empty());
    }
```

(필드는 `model.rs`의 `NormalizedEvent`(source_agent/schema_version/uuid/parent_uuid/source_file 포함)와 `TokenUsage`(Default 파생 확인됨) 실물 기준. dedup_key는 `upsert_events`가 uuid:offset으로 생성한다.)

- [ ] **Step 4: 실패 확인**

```bash
cargo test -p agent-mentor store
```
Expected: FAIL — `list_findings_current`에 인자 없음(시그니처), `status` 필드 없음, `set_finding_status`/`model_mix_for_date` 미정의

- [ ] **Step 5: store.rs 구현**

`FindingRow` 구조체(`store.rs:492` 부근)에 필드 추가:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindingRow {
    pub rule_id: String,
    pub severity: String,
    pub scope_host: Option<String>,
    pub scope_project: Option<String>,
    pub scope_kind: String,
    pub scope_ref: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
    pub dedup_key: String,
    pub last_seen: Option<String>,
    pub occurrences: u64,
    pub status: String,
}
```

`list_findings_current`(store.rs:396)를 교체:

```rust
    pub fn list_findings_current(&self, include_hidden: bool) -> Result<Vec<FindingRow>> {
        let sql = format!(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key,
                    last_seen, occurrences, status
             FROM findings {}
             ORDER BY est_tokens_saved DESC, dedup_key",
            if include_hidden { "" } else { "WHERE status='new'" }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?, r.get::<_, String>(5)?,
                r.get::<_, String>(6)?, r.get::<_, i64>(7)?,
                r.get::<_, Option<String>>(8)?, r.get::<_, String>(9)?,
                r.get::<_, Option<String>>(10)?, r.get::<_, i64>(11)?,
                r.get::<_, String>(12)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est, prescription_json, dedup_key, last_seen, occ, status) = row?;
            out.push(FindingRow {
                rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
                est_tokens_saved: est as u64,
                prescription: prescription_json.and_then(|s| serde_json::from_str(&s).ok()),
                dedup_key, last_seen,
                occurrences: occ as u64,
                status,
            });
        }
        Ok(out)
    }

    /// status: 'new' | 'resolved' | 'dismissed' (검증은 커맨드 층). 반환 = 해당 행 존재 여부.
    pub fn set_finding_status(&self, dedup_key: &str, status: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE findings SET status=?2 WHERE dedup_key=?1",
            params![dedup_key, status],
        )?;
        Ok(n > 0)
    }

    /// 오늘 tier별 토큰(입력+출력) 합. 내림차순.
    pub fn model_mix_for_date(&self, date: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT model_tier,
                    COALESCE(SUM(tok_input),0) + COALESCE(SUM(tok_output),0) AS toks
             FROM events
             WHERE substr(ts,1,10)=?1 AND model_tier IS NOT NULL
             GROUP BY model_tier ORDER BY toks DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
```

`sum_est_tokens_saved`(store.rs:390)의 SQL을 active 한정으로 변경:

```rust
    pub fn sum_est_tokens_saved(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(est_tokens_saved),0) FROM findings WHERE status='new'",
            [], |r| r.get(0))?;
        Ok(n as u64)
    }
```

호출부 2곳 시그니처 맞춤:
- `src-tauri/src/commands.rs:57`: `guard.list_findings_current()` → `guard.list_findings_current(false)` (Task 2에서 다시 확장하지만 컴파일 유지용)
- `src-tauri/src/pipeline.rs:85`: `store.list_findings_current()?` → `store.list_findings_current(false)?`
  (의도: dismissed 상태 finding이 악화돼 diff에 잡혀도 **침묵** — 사용자가 숨긴 항목은 다시 조르지 않는다)

- [ ] **Step 6: 통과 확인 (workspace 전체)**

```bash
cargo test --workspace
```
Expected: PASS — core 기존 98 + 신규 4(coach 2, store 2) = 102, app 4. 경고 0.

- [ ] **Step 7: 커밋**

```bash
git add crates/core/src/coach.rs crates/core/src/lib.rs crates/core/src/store.rs src-tauri/src/commands.rs src-tauri/src/pipeline.rs
git commit -m "feat(core): coach::fix_command + findings 상태 관리·모델 분포 쿼리 (스펙 §3·§8)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: src-tauri — 커맨드 표면 확장 + 위치 클램핑

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Create: `src-tauri/src/geometry.rs`
- Modify: `src-tauri/src/lib.rs` (mod 선언, 커맨드 등록, 위치 복원 클램핑)
- Modify: `src-tauri/src/pipeline.rs` (occasion 계산 헬퍼 공유)

**Interfaces:**
- Consumes: Task 1의 `coach::fix_command`, `FindingRow.status`, `list_findings_current(bool)`, `set_finding_status`, `model_mix_for_date`. 기존 `diary::finding_advice`, `diary::occasions::compute_occasions`, `store.summary_for_date`.
- Produces (프론트 JSON 계약 — Task 3 api.ts가 그대로 미러링):
  - `Summary`에 `user_name: String` 추가
  - `list_findings(include_hidden: Option<bool>) -> Vec<CoachFinding>` — `CoachFinding` = FindingRow 평탄화 + `detail: String` + `suggested_action: String` + `fix_command: Option<String>`
  - `set_finding_status(dedup_key: String, status: String) -> ()` — status 화이트리스트 `new|resolved|dismissed`
  - `get_week_summary() -> Vec<DayStat>` — `DayStat { date, tok_input, tok_output, session_count }`, 7일 과거→오늘 순, 빈 날은 0
  - `get_model_mix() -> Vec<ModelMixEntry>` — `ModelMixEntry { tier: String, tokens: u64 }`
  - `get_today_occasions() -> Vec<String>` — 오늘 미통지 시에만 라벨 반환+통지 마킹, 이미 통지면 빈 벡터
  - `geometry::sanitize_pos(x, y, w, h, monitors: &[(i32,i32,i32,i32)]) -> bool` — 저장 위치가 어느 모니터에도 안 보이면 false(→기본 배치 폴백)

- [ ] **Step 1: geometry.rs 작성 (테스트 동봉)**

`src-tauri/src/geometry.rs` 신규:

```rust
//! mascot 창 위치 복원 검증 — 모니터 구성 변경으로 화면 밖에 저장된 좌표 방어 (스펙 §6).

/// 창 사각형(x,y,w,h)이 모니터 목록((mx,my,mw,mh)) 중 하나와 유의미하게(중심점 기준) 겹치면 true.
/// false면 호출측이 기본 우하단 배치로 폴백한다.
pub fn sanitize_pos(x: i32, y: i32, w: i32, h: i32, monitors: &[(i32, i32, i32, i32)]) -> bool {
    let (cx, cy) = (x + w / 2, y + h / 2);
    monitors.iter().any(|&(mx, my, mw, mh)| {
        cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
    })
}

#[cfg(test)]
mod tests {
    use super::sanitize_pos;

    #[test]
    fn inside_primary_is_ok() {
        assert!(sanitize_pos(100, 100, 160, 160, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn offscreen_after_monitor_removed_is_rejected() {
        // 좌표가 이전 보조 모니터(x=1920~) 영역 — 이제 primary만 남음
        assert!(!sanitize_pos(2200, 300, 160, 160, &[(0, 0, 1920, 1080)]));
        // 음수 영역(왼쪽 보조 제거)도 거부
        assert!(!sanitize_pos(-500, 300, 160, 160, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn secondary_monitor_still_ok() {
        assert!(sanitize_pos(2200, 300, 160, 160, &[(0, 0, 1920, 1080), (1920, 0, 1920, 1080)]));
    }
}
```

- [ ] **Step 2: commands.rs — 신규 커맨드 테스트 먼저**

`src-tauri/src/commands.rs`의 `#[cfg(test)] mod tests`에 추가:

```rust
    #[test]
    fn summary_includes_user_name() {
        let store = SqliteStore::open_in_memory().unwrap();
        let s = summary_inner(&store).unwrap();
        assert!(!s.user_name.is_empty()); // USERNAME env 또는 "user" 폴백
    }

    #[test]
    fn week_summary_is_7_days_oldest_first() {
        let store = SqliteStore::open_in_memory().unwrap();
        let days = week_summary_inner(&store).unwrap();
        assert_eq!(days.len(), 7);
        assert!(days[0].date < days[6].date);
        assert_eq!(days[6].date, chrono::Utc::now().format("%Y-%m-%d").to_string());
        assert_eq!(days[0].session_count, 0); // 빈 store는 0 채움
    }

    #[test]
    fn coach_findings_carry_advice_and_command() {
        use agent_mentor::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "playwright".into(),
            evidence: serde_json::json!({"server": "playwright"}),
            est_tokens_saved: 4200, prescription: None, dedup_key: "k1".into(),
        }, "2026-07-05T00:00:00Z").unwrap();
        let rows = coach_findings_inner(&store, true).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].detail.contains("playwright"));
        assert!(!rows[0].suggested_action.is_empty());
        assert_eq!(rows[0].fix_command.as_deref(), Some("claude mcp remove playwright"));
        assert_eq!(rows[0].row.status, "new");
    }

    #[test]
    fn set_finding_status_validates() {
        assert!(valid_finding_status("new") && valid_finding_status("resolved") && valid_finding_status("dismissed"));
        assert!(!valid_finding_status("gone") && !valid_finding_status(""));
    }

    #[test]
    fn occasions_gate_returns_empty_when_already_notified() {
        let store = SqliteStore::open_in_memory().unwrap();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        store.set_setting("occasion_notified_date", &today).unwrap();
        assert!(today_occasions_inner(&store).unwrap().is_empty());
    }
```

- [ ] **Step 3: 실패 확인**

```bash
cargo test -p agent-mentor-app
```
Expected: FAIL — `user_name` 필드, `week_summary_inner`, `coach_findings_inner`, `valid_finding_status`, `today_occasions_inner` 미정의

- [ ] **Step 4: commands.rs 구현**

`Summary` 구조체에 필드 추가 + `summary_inner` 수정:

```rust
#[derive(Debug, Serialize)]
pub struct Summary {
    pub date: String,
    pub user_name: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
    pub total_sessions: u64,
    pub est_tokens_saved_total: u64,
    pub last_scan: Option<String>,
}
```

`summary_inner`의 `Ok(Summary { ... })`에 한 줄 추가:

```rust
        user_name: std::env::var("USERNAME").unwrap_or_else(|_| "user".into()),
```

신규 타입·inner 함수·커맨드들 추가 (기존 `diary_inner` 아래):

```rust
#[derive(Debug, Serialize)]
pub struct CoachFinding {
    #[serde(flatten)]
    pub row: agent_mentor::store::FindingRow,
    pub detail: String,
    pub suggested_action: String,
    pub fix_command: Option<String>,
}

pub fn coach_findings_inner(store: &SqliteStore, include_hidden: bool) -> anyhow::Result<Vec<CoachFinding>> {
    Ok(store
        .list_findings_current(include_hidden)?
        .into_iter()
        .map(|row| {
            let (detail, suggested_action) =
                agent_mentor::diary::finding_advice(&row.rule_id, &row.evidence, row.est_tokens_saved);
            let fix_command = agent_mentor::coach::fix_command(&row.rule_id, &row.evidence);
            CoachFinding { row, detail, suggested_action, fix_command }
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    pub date: String,
    pub tok_input: u64,
    pub tok_output: u64,
    pub session_count: u64,
}

pub fn week_summary_inner(store: &SqliteStore) -> anyhow::Result<Vec<DayStat>> {
    let today = chrono::Utc::now().date_naive();
    let mut out = Vec::with_capacity(7);
    for i in (0..7).rev() {
        let date = (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let d = store.summary_for_date(&date)?;
        out.push(DayStat {
            date,
            tok_input: d.tok_input,
            tok_output: d.tok_output,
            session_count: d.session_count,
        });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct ModelMixEntry {
    pub tier: String,
    pub tokens: u64,
}

pub(crate) fn valid_finding_status(s: &str) -> bool {
    matches!(s, "new" | "resolved" | "dismissed")
}

/// 오늘 occasions — 하루 1회 게이트 포함. 반환하는 순간 통지된 것으로 마킹한다
/// (호출자는 mascot 웹뷰 = 표시 주체). 이미 통지됐으면 빈 벡터.
pub fn today_occasions_inner(store: &SqliteStore) -> anyhow::Result<Vec<String>> {
    use agent_mentor::diary::occasions::compute_occasions;
    use agent_mentor::diary::{resolve_locale, DiaryConfig};

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if store.get_setting("occasion_notified_date")?.as_deref() == Some(today.as_str()) {
        return Ok(vec![]);
    }
    let Ok(date) = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d") else { return Ok(vec![]) };
    let anchor = store.earliest_session_ts()?.and_then(|ts| {
        ts.get(..10).and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    });
    let locale = resolve_locale(&DiaryConfig::default());
    let labels: Vec<String> = compute_occasions(date, anchor, &locale, true)
        .into_iter().map(|o| o.label).collect();
    if !labels.is_empty() {
        store.set_setting("occasion_notified_date", &today)?;
    }
    Ok(labels)
}
```

커맨드 배선 (기존 `list_findings` 커맨드를 교체하고 나머지는 추가):

```rust
#[tauri::command(async)]
pub fn list_findings(state: State<AppState>, include_hidden: Option<bool>) -> Result<Vec<CoachFinding>, String> {
    let guard = lock(&state)?;
    coach_findings_inner(&*guard, include_hidden.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn set_finding_status(state: State<AppState>, dedup_key: String, status: String) -> Result<(), String> {
    if !valid_finding_status(&status) {
        return Err(format!("허용되지 않은 상태: {status}"));
    }
    let guard = lock(&state)?;
    guard.set_finding_status(&dedup_key, &status)
        .map_err(|e| e.to_string())
        .and_then(|found| if found { Ok(()) } else { Err(format!("finding 없음: {dedup_key}")) })
}

#[tauri::command(async)]
pub fn get_week_summary(state: State<AppState>) -> Result<Vec<DayStat>, String> {
    let guard = lock(&state)?;
    week_summary_inner(&*guard).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_model_mix(state: State<AppState>) -> Result<Vec<ModelMixEntry>, String> {
    let guard = lock(&state)?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(guard.model_mix_for_date(&date).map_err(|e| e.to_string())?
        .into_iter().map(|(tier, tokens)| ModelMixEntry { tier, tokens }).collect())
}

#[tauri::command(async)]
pub fn get_today_occasions(state: State<AppState>) -> Result<Vec<String>, String> {
    let guard = lock(&state)?;
    today_occasions_inner(&*guard).map_err(|e| e.to_string())
}
```

- [ ] **Step 5: pipeline.rs — occasion push 제거 (pull 단일화)**

**push emit을 남기면 시작 레이스가 그대로 남는다**: 웹뷰 로드 전에 emit+마킹이 일어나면 이후 pull이 빈 값을 받는다. 따라서 occasion 전달은 **pull(`get_today_occasions`) 단일 경로**로 하고(마스코트가 마운트 시·scan:done마다 pull — Task 8), 파이프라인의 push는 삭제한다:

- `maybe_notify_occasions` 함수(pipeline.rs:162~203) **전체 삭제**
- `run_pipeline_once` 내 호출 `maybe_notify_occasions(app, &state.store);`(pipeline.rs:103) **삭제**
- `should_notify_occasion` 함수와 `mod tests`(pipeline.rs:206~225) **삭제** — 게이트는 `today_occasions_inner`가 내장

`occasion:today` 이벤트는 사라지지 않는다 — Task 8에서 마스코트가 pull 성공 시 같은 이름으로 **재방송**해 chat 창의 알림 로그(App.svelte `onOccasionToday`)가 계속 동작한다.

- [ ] **Step 6: lib.rs — 등록 + 클램핑 적용**

`src-tauri/src/lib.rs` 상단 mod 나열에 추가:

```rust
#[cfg_attr(test, allow(dead_code))]
mod geometry;
```

`invoke_handler`의 `generate_handler![...]`에 추가:

```rust
                commands::set_finding_status,
                commands::get_week_summary,
                commands::get_model_mix,
                commands::get_today_occasions,
```

mascot 위치 복원 블록(lib.rs:52~57)을 클램핑 검증 포함으로 교체:

```rust
                        let mut restored = false;
                        if let Some(p) = pos {
                            if let Some((x, y)) = p.split_once(',') {
                                if let (Ok(x), Ok(y)) = (x.parse::<i32>(), y.parse::<i32>()) {
                                    let monitors: Vec<(i32, i32, i32, i32)> = w
                                        .available_monitors()
                                        .map(|ms| ms.iter().map(|m| {
                                            let p = m.position();
                                            let s = m.size();
                                            (p.x, p.y, s.width as i32, s.height as i32)
                                        }).collect())
                                        .unwrap_or_default();
                                    let scale = w.scale_factor().unwrap_or(1.0);
                                    let side = (160.0 * scale) as i32;
                                    if geometry::sanitize_pos(x, y, side, side, &monitors) {
                                        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                                        restored = true;
                                    }
                                }
                            }
                        }
                        if !restored {
                            if let Ok(Some(mon)) = w.primary_monitor() {
                                let size = mon.size();
                                let mpos = mon.position();
                                // 창 160×160 + 여백 16px, 작업표시줄(대략 하단 48px) 위 (스펙 §1)
                                let x = mpos.x + size.width as i32 - 160 - 16;
                                let y = mpos.y + size.height as i32 - 160 - 64;
                                let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                            }
                        }
```

(기존 `else if let Ok(Some(mon))` 분기는 위 `if !restored` 블록으로 흡수 — 저장 좌표가 화면 밖이어도 기본 배치로 폴백된다.)

- [ ] **Step 7: 통과 확인**

```bash
cargo test --workspace && cargo build
```
Expected: PASS — app 테스트 4 − 1(pipeline occasion 테스트 삭제) + 신규 8(geometry 3, commands 5) = 11, core 102 무회귀, 경고 0, 빌드 성공

- [ ] **Step 8: 커밋**

```bash
git add src-tauri/src/commands.rs src-tauri/src/geometry.rs src-tauri/src/lib.rs src-tauri/src/pipeline.rs
git commit -m "feat(app): 코칭 advice·상태 커맨드 + 주간/모델 위젯 커맨드 + occasion pull + 위치 클램핑 (스펙 §6·§8)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 프론트 기반 — api.ts·theme.css·미니홈피 레이아웃(App.svelte)

**Files:**
- Modify: `src/lib/api.ts`
- Create: `src/lib/theme.css`
- Create: `src/lib/notices.ts`, `src/lib/notices.test.ts`
- Create: `src/lib/ui/RobotPortrait.svelte`
- Modify: `src/App.svelte`

**Interfaces:**
- Consumes: Task 2의 커맨드 JSON 계약 전부.
- Produces (후속 태스크가 import):
  - api.ts: `CoachFinding`, `DayStat`, `ModelMixEntry` 타입, `listFindings(includeHidden?)`, `setFindingStatus(dedupKey, status)`, `getWeekSummary()`, `getModelMix()`, `getTodayOccasions()`, `Summary.user_name`
  - theme.css 토큰: `--bg-grad --frame-bg --ink --ink-soft --accent --pastel-lav --pastel-mint --pastel-cream --pastel-coral --radius-s --radius-m --radius-l --shadow-soft`
  - notices.ts: `Notice { ts: string; kind: 'finding'|'diary'|'occasion'; text: string }`, `pushNotice(list, n): Notice[]`(최대 20, 최신 앞), `loadNotices(): Notice[]`, `saveNotices(list): void`
  - App.svelte가 `HomeTab`에 `{ summary, onGotoCoach }`, `CoachTab`에 `{ focusKey }` prop을 전달 (Task 4·6이 이 계약으로 구현)

- [ ] **Step 1: notices.ts 실패 테스트**

`src/lib/notices.test.ts` 신규:

```ts
import { describe, expect, it } from 'vitest';
import { pushNotice, type Notice } from './notices';

const n = (text: string): Notice => ({ ts: '2026-07-05T10:00:00Z', kind: 'finding', text });

describe('pushNotice', () => {
  it('최신이 앞, 최대 20건 유지', () => {
    let list: Notice[] = [];
    for (let i = 0; i < 25; i++) list = pushNotice(list, n(`알림 ${i}`));
    expect(list.length).toBe(20);
    expect(list[0].text).toBe('알림 24');
    expect(list[19].text).toBe('알림 5');
  });
  it('원본 배열을 변형하지 않는다', () => {
    const orig: Notice[] = [n('a')];
    const next = pushNotice(orig, n('b'));
    expect(orig.length).toBe(1);
    expect(next[0].text).toBe('b');
  });
});
```

- [ ] **Step 2: 실패 확인**

```bash
npx vitest run src/lib/notices.test.ts
```
Expected: FAIL — `./notices` 모듈 없음

- [ ] **Step 3: notices.ts 구현**

`src/lib/notices.ts` 신규:

```ts
export interface Notice {
  ts: string;
  kind: 'finding' | 'diary' | 'occasion';
  text: string;
}

const MAX = 20;
const KEY = 'agent-mentor.notices';

export function pushNotice(list: Notice[], n: Notice): Notice[] {
  return [n, ...list].slice(0, MAX);
}

export function loadNotices(): Notice[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '[]') as Notice[];
  } catch {
    return [];
  }
}

export function saveNotices(list: Notice[]): void {
  localStorage.setItem(KEY, JSON.stringify(list));
}
```

- [ ] **Step 4: 통과 확인**

```bash
npx vitest run src/lib/notices.test.ts
```
Expected: PASS 2

- [ ] **Step 5: api.ts 확장**

`src/lib/api.ts` — `Summary`에 `user_name: string;` 필드 추가(`date` 아래). `Finding` 인터페이스 **아래에** 추가:

```ts
export interface CoachFinding extends Finding {
  status: 'new' | 'resolved' | 'dismissed';
  detail: string;
  suggested_action: string;
  fix_command: string | null;
}

export interface DayStat {
  date: string;
  tok_input: number;
  tok_output: number;
  session_count: number;
}

export interface ModelMixEntry {
  tier: string;
  tokens: number;
}
```

커맨드 래퍼 — 기존 `listFindings` 줄을 교체하고 아래 추가:

```ts
export const listFindings = (includeHidden = false) =>
  invoke<CoachFinding[]>('list_findings', { includeHidden });
export const setFindingStatus = (dedupKey: string, status: 'new' | 'resolved' | 'dismissed') =>
  invoke<void>('set_finding_status', { dedupKey, status });
export const getWeekSummary = () => invoke<DayStat[]>('get_week_summary');
export const getModelMix = () => invoke<ModelMixEntry[]>('get_model_mix');
export const getTodayOccasions = () => invoke<string[]>('get_today_occasions');
```

`onNewFindings`의 제네릭도 `CoachFinding[]`이 아닌 기존 `Finding[]` 유지(파이프라인은 FindingRow를 emit — advice 필드 없음).
⚠ `Finding`에 `status: string` 필드도 추가해야 한다(FindingRow에 추가됐으므로):

```ts
export interface Finding {
  rule_id: string;
  severity: 'info' | 'suggest' | 'warn';
  scope_host: string | null;
  scope_project: string | null;
  scope_kind: string;
  scope_ref: string;
  evidence: unknown;
  est_tokens_saved: number;
  prescription: { kind: string; payload: unknown } | null;
  dedup_key: string;
  last_seen: string | null;
  occurrences: number;
  status: string;
}
```

- [ ] **Step 6: theme.css 작성**

`src/lib/theme.css` 신규:

```css
/* 파스텔 모던 토큰 — chat·mascot 양쪽 창 공용. 색·radius·그림자는 반드시 여기서만. */
:root {
  --bg-grad: linear-gradient(135deg, #eae5f7 0%, #dcefe9 100%);
  --frame-bg: #fffdfa;
  --ink: #4a4668;
  --ink-soft: #8b86a8;
  --accent: #7c6fd0;
  --pastel-lav: #cfc6ec;
  --pastel-mint: #bfe3d9;
  --pastel-cream: #fdf1cf;
  --pastel-coral: #f6cfc6;
  --radius-s: 8px;
  --radius-m: 12px;
  --radius-l: 16px;
  --shadow-soft: 0 2px 10px rgba(74, 70, 104, 0.14);
}
```

- [ ] **Step 7: RobotPortrait.svelte 작성**

`src/lib/ui/RobotPortrait.svelte` 신규:

```svelte
<script lang="ts">
  import { getMascotSeed } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  let canvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    getMascotSeed().then((spec: RobotSpec) => drawRobot(ctx, spec, frameAt('idle', 300)));
  });
</script>

<div class="portrait">
  <canvas bind:this={canvas} width="128" height="128"></canvas>
</div>

<style>
  .portrait {
    background: var(--pastel-mint);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 10px;
    display: flex;
    justify-content: center;
  }
  canvas { width: 96px; height: 96px; image-rendering: pixelated; }
</style>
```

(`frameAt('idle', 300)`: t=300ms는 깜빡임 구간(0~200ms) 밖의 눈 뜬 정지 프레임.)

- [ ] **Step 8: App.svelte 미니홈피 레이아웃으로 교체**

`src/App.svelte` 전체 교체:

```svelte
<script lang="ts">
  import './lib/theme.css';
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import DiaryTab from './lib/ui/DiaryTab.svelte';
  import RobotPortrait from './lib/ui/RobotPortrait.svelte';
  import {
    getSummary, listFindings, onScanDone, onGotoTab,
    onNewFindings, onDiaryReady, onOccasionToday, type Summary,
  } from './lib/api';
  import { loadNotices, pushNotice, saveNotices, type Notice } from './lib/notices';

  type Tab = 'home' | 'diary' | 'coach' | 'chat';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'home', label: '홈' },
    { id: 'diary', label: '다이어리' },
    { id: 'coach', label: '코칭' },
    { id: 'chat', label: '채팅' },
  ];

  let tab = $state<Tab>('home');
  let summary = $state<Summary | null>(null);
  let activeCount = $state(0);
  let coachFocus = $state<string | null>(null);
  let notices = $state<Notice[]>(loadNotices());

  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
  }
  refresh();
  onScanDone(() => refresh());
  onGotoTab((t) => {
    if (t === 'home' || t === 'diary' || t === 'coach' || t === 'chat') tab = t;
  });

  // 알림 히스토리 기록 (스펙 §2 — 창이 숨김이어도 수신됨)
  function record(kind: Notice['kind'], text: string) {
    notices = pushNotice(notices, { ts: new Date().toISOString(), kind, text });
    saveNotices(notices);
  }
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && record('finding', `코칭 지적 ${rows.length}건이 도착했어요`)),
      onDiaryReady((date) => record('diary', `${date} 일기가 나왔어요`)),
      onOccasionToday((labels) => labels.length && record('occasion', `오늘은 ${labels[0]}!`)),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  function gotoCoach(dedupKey: string) {
    coachFocus = dedupKey;
    tab = 'coach';
  }

  const mood = $derived(
    summary && summary.est_tokens_saved_total > 0 ? '절약할 게 보여요…' : '평화로워요'
  );
</script>

<div class="wall">
  <div class="homepy">
    <header class="titlebar">
      <h1>{summary?.user_name ?? '주인'}님의 미니홈피</h1>
      <div class="counter">
        TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
      </div>
    </header>
    <div class="body">
      <aside class="profile">
        <RobotPortrait />
        <p class="mood">“{mood}”</p>
      </aside>
      <main class="content">
        {#if tab === 'home'}
          <HomeTab {summary} onGotoCoach={gotoCoach} />
        {:else if tab === 'coach'}
          <CoachTab focusKey={coachFocus} />
        {:else if tab === 'diary'}
          <DiaryTab />
        {:else}
          <section class="placeholder">채팅은 준비 중이에요, 주인. (다음 PR에서 열려요)</section>
        {/if}
      </main>
      <nav class="tabs">
        {#each TABS as t (t.id)}
          <button class:active={tab === t.id} onclick={() => (tab = t.id)}>
            <span class="label">{t.label}</span>
            {#if t.id === 'coach' && activeCount > 0}<span class="badge">{activeCount}</span>{/if}
          </button>
        {/each}
      </nav>
    </div>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; height: 100%; }
  :global(body) {
    background: var(--bg-grad);
    color: var(--ink);
    font-family: 'Segoe UI', 'Malgun Gothic', sans-serif;
    font-size: 14px;
  }
  .wall { height: 100vh; padding: 18px 34px 18px 18px; box-sizing: border-box; }
  .homepy {
    height: 100%; display: flex; flex-direction: column;
    background: var(--frame-bg);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-soft);
  }
  .titlebar {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid var(--pastel-lav);
  }
  .titlebar h1 { margin: 0; font-size: 16px; font-weight: 600; }
  .counter { font-size: 12px; color: var(--ink-soft); }
  .counter b { color: var(--accent); }
  .body { flex: 1; display: flex; min-height: 0; position: relative; }
  .profile {
    width: 168px; padding: 16px 14px;
    border-right: 1px solid var(--pastel-lav);
    display: flex; flex-direction: column; gap: 12px;
  }
  .mood { margin: 0; font-size: 12px; color: var(--ink-soft); text-align: center; }
  .content { flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; }
  .tabs {
    position: absolute; right: -30px; top: 24px;
    display: flex; flex-direction: column; gap: 6px;
  }
  .tabs button {
    writing-mode: vertical-rl;
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    padding: 12px 7px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    box-shadow: var(--shadow-soft);
    display: flex; align-items: center; gap: 4px;
  }
  .tabs button.active { background: var(--frame-bg); font-weight: 600; color: var(--accent); }
  .badge {
    writing-mode: horizontal-tb;
    background: var(--pastel-coral); color: var(--ink);
    border-radius: 999px; font-size: 10px; padding: 1px 5px;
  }
  .placeholder { padding: 24px; }
</style>
```

⚠ 이 시점에서 `DiaryTab`·개편된 `HomeTab`·`CoachTab`이 아직 없으므로 **빌드가 깨진다**. 이 태스크에서는 최소 스텁을 만들어 그린을 유지한다:
- `src/lib/ui/DiaryTab.svelte`: `<section style="padding:24px">다이어리 준비 중…</section>` 한 줄 (Task 7이 교체)
- 기존 `HomeTab.svelte`에 prop 추가만: `let { summary, onGotoCoach }: { summary: Summary | null; onGotoCoach: (k: string) => void } = $props();` (svelte-check 경고 방지를 위해 `void onGotoCoach;` 한 줄 추가 가능. Task 4가 전면 교체)
- 기존 `CoachTab.svelte`에 prop 추가만: `let { focusKey = null }: { focusKey?: string | null } = $props();` + `void focusKey;` (Task 6이 전면 교체). **기존 CoachTab의 `f.dedup_key` 키 접근은 CoachFinding에도 그대로 존재하므로 컴파일 유지됨.**

- [ ] **Step 9: 빌드·테스트 확인**

```bash
npm run build && npx vitest run
```
Expected: 빌드 성공, vitest 기존 31 + 신규 2(notices) = 33 PASS

- [ ] **Step 10: 커밋**

```bash
git add src/lib/api.ts src/lib/theme.css src/lib/notices.ts src/lib/notices.test.ts src/lib/ui/RobotPortrait.svelte src/lib/ui/DiaryTab.svelte src/lib/ui/HomeTab.svelte src/lib/ui/CoachTab.svelte src/App.svelte
git commit -m "feat(front): 미니홈피 레이아웃(타이틀바·좌측 초상·우측 세로 탭) + 파스텔 토큰 + 알림 로그 (스펙 §1·§2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 홈 탭 — 오늘 스트립 + 위젯 4종

**Files:**
- Create: `src/lib/ui/home/WeekTrend.svelte`, `src/lib/ui/home/ModelMix.svelte`, `src/lib/ui/home/SaveTop3.svelte`, `src/lib/ui/home/NoticeLog.svelte`
- Modify: `src/lib/ui/HomeTab.svelte` (전면 교체)

**Interfaces:**
- Consumes: api.ts의 `getWeekSummary/getModelMix/listFindings`, notices.ts `loadNotices`, App의 prop 계약 `{ summary, onGotoCoach }`.
- Produces: 위젯 prop 계약 — `WeekTrend { days: DayStat[] }`, `ModelMix { mix: ModelMixEntry[] }`, `SaveTop3 { findings: CoachFinding[]; onGoto: (dedupKey: string) => void }`, `NoticeLog { notices: Notice[] }`. HomeTab 하단에 `<MiniRoom advice={...} />` 자리는 Task 5에서 채움(이 태스크에서는 주석 마커 `<!-- miniroom -->`만).

- [ ] **Step 1: 위젯 4종 작성**

`src/lib/ui/home/WeekTrend.svelte`:

```svelte
<script lang="ts">
  import type { DayStat } from '../../api';
  let { days }: { days: DayStat[] } = $props();
  const max = $derived(Math.max(...days.map((d) => d.tok_input + d.tok_output), 1));
  const dow = (date: string) => '일월화수목금토'[new Date(date + 'T00:00:00').getDay()];
</script>

<div class="widget">
  <h3>주간 추이</h3>
  <div class="bars">
    {#each days as d (d.date)}
      <div class="col" title={`${d.date} · ${(d.tok_input + d.tok_output).toLocaleString()} tok · ${d.session_count}세션`}>
        <div class="bar" style:height={`${Math.round(((d.tok_input + d.tok_output) / max) * 100)}%`}></div>
        <span class="dow">{dow(d.date)}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .bars { display: flex; gap: 6px; align-items: flex-end; height: 72px; }
  .col { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; height: 100%; gap: 3px; }
  .bar { width: 100%; min-height: 2px; background: var(--pastel-lav); border-radius: 4px 4px 0 0; }
  .col:last-child .bar { background: var(--accent); }
  .dow { font-size: 10px; color: var(--ink-soft); }
</style>
```

`src/lib/ui/home/ModelMix.svelte`:

```svelte
<script lang="ts">
  import type { ModelMixEntry } from '../../api';
  let { mix }: { mix: ModelMixEntry[] } = $props();
  const total = $derived(Math.max(mix.reduce((a, m) => a + m.tokens, 0), 1));
  const COLOR: Record<string, string> = {
    opus: 'var(--pastel-coral)', sonnet: 'var(--pastel-lav)', haiku: 'var(--pastel-mint)',
  };
  const color = (tier: string) => COLOR[tier] ?? 'var(--pastel-cream)';
  const pct = (t: number) => Math.round((t / total) * 100);
</script>

<div class="widget">
  <h3>오늘 모델 분포</h3>
  {#if mix.length === 0}
    <p class="empty">아직 오늘 기록이 없어요</p>
  {:else}
    <div class="stack">
      {#each mix as m (m.tier)}
        <div class="seg" style:width={`${pct(m.tokens)}%`} style:background={color(m.tier)}></div>
      {/each}
    </div>
    <ul class="legend">
      {#each mix as m (m.tier)}
        <li><span class="chip" style:background={color(m.tier)}></span>{m.tier} {pct(m.tokens)}%</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  .stack { display: flex; height: 14px; border-radius: 999px; overflow: hidden; }
  .legend { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 8px; font-size: 11px; color: var(--ink-soft); }
  .legend li { display: flex; align-items: center; gap: 4px; }
  .chip { width: 10px; height: 10px; border-radius: 3px; display: inline-block; }
</style>
```

`src/lib/ui/home/SaveTop3.svelte`:

```svelte
<script lang="ts">
  import type { CoachFinding } from '../../api';
  let { findings, onGoto }: { findings: CoachFinding[]; onGoto: (dedupKey: string) => void } = $props();
  const top3 = $derived(findings.slice(0, 3)); // 커맨드가 est_tokens_saved 내림차순 정렬을 보장
</script>

<div class="widget">
  <h3>절약 실천 top3</h3>
  {#if top3.length === 0}
    <p class="empty">지금은 지적할 게 없어요. 완벽해요!</p>
  {:else}
    <ol>
      {#each top3 as f, i (f.dedup_key)}
        <li>
          <button onclick={() => onGoto(f.dedup_key)}>
            <span class="rank">{i + 1}</span>
            <span class="action">{f.suggested_action}</span>
            <span class="save">~{f.est_tokens_saved.toLocaleString()}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li button {
    width: 100%; display: flex; align-items: center; gap: 8px; text-align: left;
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-cream); color: var(--ink);
    border-radius: var(--radius-s); padding: 7px 10px;
  }
  li button:hover { background: var(--pastel-lav); }
  .rank { color: var(--accent); font-weight: 700; }
  .action { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .save { color: var(--ink-soft); font-size: 11px; white-space: nowrap; }
</style>
```

`src/lib/ui/home/NoticeLog.svelte`:

```svelte
<script lang="ts">
  import type { Notice } from '../../notices';
  let { notices }: { notices: Notice[] } = $props();
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉' };
  const hhmm = (ts: string) => {
    const d = new Date(ts);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };
</script>

<div class="widget">
  <h3>최근 알림</h3>
  {#if notices.length === 0}
    <p class="empty">아직 알림이 없어요</p>
  {:else}
    <ul>
      {#each notices.slice(0, 6) as n (n.ts + n.text)}
        <li><span>{ICON[n.kind]}</span><span class="text">{n.text}</span><time>{hhmm(n.ts)}</time></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; font-size: 12px; }
  li { display: flex; gap: 6px; align-items: baseline; }
  .text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  time { color: var(--ink-soft); font-size: 10px; }
</style>
```

- [ ] **Step 2: HomeTab.svelte 전면 교체**

```svelte
<script lang="ts">
  import {
    getModelMix, getWeekSummary, listFindings, onScanDone, runScanNow,
    type CoachFinding, type DayStat, type ModelMixEntry, type Summary,
  } from '../api';
  import { loadNotices, type Notice } from '../notices';
  import WeekTrend from './home/WeekTrend.svelte';
  import ModelMix from './home/ModelMix.svelte';
  import SaveTop3 from './home/SaveTop3.svelte';
  import NoticeLog from './home/NoticeLog.svelte';

  let { summary, onGotoCoach }: { summary: Summary | null; onGotoCoach: (k: string) => void } = $props();

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);
  let days = $state<DayStat[]>([]);
  let mix = $state<ModelMixEntry[]>([]);
  let findings = $state<CoachFinding[]>([]);
  let notices = $state<Notice[]>([]);

  async function load() {
    [days, mix, findings] = await Promise.all([
      getWeekSummary().catch(() => [] as DayStat[]),
      getModelMix().catch(() => [] as ModelMixEntry[]),
      listFindings(false).catch(() => [] as CoachFinding[]),
    ]);
    notices = loadNotices();
  }
  load();

  $effect(() => {
    const p = onScanDone(() => { scanning = false; load(); });
    return () => { p.then((u) => u()); };
  });

  async function scan() {
    scanning = true;
    try {
      await runScanNow(); // 완료 신호는 scan:done 이벤트가 담당
    } catch {
      scanning = false;
    }
  }
</script>

<section class="home">
  <div class="strip">
    <span>세션 <b>{fmt(summary?.session_count)}</b></span>
    <span>입력 <b>{fmt(summary?.tok_input)}</b></span>
    <span>출력 <b>{fmt(summary?.tok_output)}</b></span>
    <span class="save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</span>
  </div>

  <div class="grid">
    <WeekTrend {days} />
    <ModelMix {mix} />
    <SaveTop3 {findings} onGoto={onGotoCoach} />
    <NoticeLog {notices} />
  </div>

  <!-- miniroom : Task 5에서 <MiniRoom> 배치 -->

  <footer class="status">
    {#if scanning}
      <span class="scanning">스캔 중…</span>
    {:else if summary?.last_scan}
      <span>마지막 스캔: {new Date(summary.last_scan).toLocaleString()}</span>
    {:else}
      <span>첫 수집 진행 중… (트랜스크립트 양에 따라 몇 분 걸릴 수 있어요)</span>
    {/if}
    <button onclick={scan} disabled={scanning}>{scanning ? '스캔 중…' : '지금 스캔'}</button>
  </footer>
</section>

<style>
  .home { padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; flex: 1; }
  .strip {
    display: flex; gap: 16px; flex-wrap: wrap; font-size: 12px; color: var(--ink-soft);
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 9px 14px;
  }
  .strip b { color: var(--ink); }
  .strip .save b { color: var(--accent); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .status {
    margin-top: auto; display: flex; justify-content: space-between; align-items: center;
    font-size: 12px; color: var(--ink-soft);
  }
  .status button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 6px 12px;
  }
  .status button:disabled { opacity: 0.6; cursor: default; }
  .scanning { animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.35; } }
</style>
```

- [ ] **Step 3: 빌드·테스트 확인**

```bash
npm run build && npx vitest run
```
Expected: 빌드 성공, vitest 33 무회귀

- [ ] **Step 4: 커밋**

```bash
git add src/lib/ui/home src/lib/ui/HomeTab.svelte
git commit -m "feat(front): 홈 탭 개편 — 오늘 스트립 + 주간추이·모델분포·절약top3·최근알림 위젯 (스펙 §2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 마이룸 (MiniRoom.svelte)

**Files:**
- Create: `src/lib/ui/MiniRoom.svelte`
- Modify: `src/lib/ui/HomeTab.svelte` (`<!-- miniroom -->` 마커 자리에 배치 + advice 전달)

**Interfaces:**
- Consumes: `getMascotSeed`, `drawRobot`, `frameAt`(state: `'idle' | 'happy'`), Task 4의 HomeTab 내부 상태 `findings`.
- Produces: `MiniRoom { advice: string | null }` — advice는 활성 findings 최상위의 `suggested_action`(없으면 null).

- [ ] **Step 1: MiniRoom.svelte 작성**

```svelte
<script lang="ts">
  import { getMascotSeed } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt, type MascotState } from '../robot/anim';

  let { advice }: { advice: string | null } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let mode = $state<MascotState>('idle');
  let line = $state('오늘도 화이팅이에요, 주인!');

  const CHATTER = [
    '오늘도 화이팅이에요, 주인!',
    '토큰은 아끼라고 있는 거예요',
    '주인, 물 한 잔 마시고 해요',
    '커밋은 자주, 후회는 짧게',
    '이 방 아늑하죠? 제 방이에요',
  ];
  const HAPPY = ['히히, 간지러워요!', '주인 최고!', '또 눌러 봐요!'];
  const pick = (arr: string[]) => arr[Math.floor(Math.random() * arr.length)];

  // 대사 순환: 기분/advice/잡담 (스펙 §2)
  $effect(() => {
    const t = setInterval(() => {
      if (mode !== 'idle') return;
      line = advice && Math.random() < 0.5 ? advice : pick(CHATTER);
    }, 45_000);
    return () => clearInterval(t);
  });

  function poke() {
    mode = 'happy';
    line = pick(HAPPY);
    setTimeout(() => (mode = 'idle'), 2500);
  }

  $effect(() => {
    if (!canvas || !spec) return;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    const loop = (t: number) => {
      drawRobot(ctx, spec!, frameAt(mode, t));
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  getMascotSeed().then((s) => (spec = s));
</script>

<div class="room">
  <div class="window"></div>
  <div class="plant">🪴</div>
  <div class="bubble">{line}</div>
  <button class="robot" onclick={poke} aria-label="로봇 쓰다듬기">
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  </button>
  <div class="rug"></div>
  <div class="floor"></div>
</div>

<style>
  .room {
    position: relative; height: 190px; overflow: hidden;
    background: linear-gradient(180deg, var(--pastel-lav) 0%, #e9e3f8 68%, transparent 68%);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
  }
  .floor {
    position: absolute; left: 0; right: 0; bottom: 0; height: 32%;
    background: var(--pastel-cream);
  }
  .rug {
    position: absolute; left: 50%; bottom: 8px; transform: translateX(-50%);
    width: 190px; height: 40px; border-radius: 50%;
    background: var(--pastel-mint); z-index: 1;
  }
  .window {
    position: absolute; left: 22px; top: 18px; width: 64px; height: 52px;
    background: var(--pastel-mint); border-radius: var(--radius-s);
    box-shadow: inset 0 0 0 4px var(--frame-bg);
  }
  .plant { position: absolute; right: 20px; bottom: 46px; font-size: 26px; z-index: 2; }
  .robot {
    position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%);
    border: none; background: none; padding: 0; cursor: pointer; z-index: 2;
  }
  canvas { width: 112px; height: 112px; image-rendering: pixelated; display: block; }
  .bubble {
    position: absolute; left: 50%; top: 12px; transform: translateX(-50%);
    max-width: 65%; z-index: 3;
    background: var(--frame-bg); color: var(--ink);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 7px 12px; font-size: 12px; text-align: center;
  }
</style>
```

- [ ] **Step 2: HomeTab에 배치**

`HomeTab.svelte`의 script에 import·derived 추가:

```ts
  import MiniRoom from './MiniRoom.svelte';
```

```ts
  const topAdvice = $derived(findings.length > 0 ? findings[0].suggested_action : null);
```

`<!-- miniroom : Task 5에서 <MiniRoom> 배치 -->` 마커를 교체:

```svelte
  <MiniRoom advice={topAdvice} />
```

- [ ] **Step 3: 빌드 확인**

```bash
npm run build && npx vitest run
```
Expected: 빌드 성공, vitest 33 무회귀

- [ ] **Step 4: 커밋**

```bash
git add src/lib/ui/MiniRoom.svelte src/lib/ui/HomeTab.svelte
git commit -m "feat(front): 홈 하단 마이룸 — 파스텔 방+128px 로봇, 클릭 리액션, 대사 순환 (스펙 §2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 코칭 탭 재설계

**Files:**
- Modify: `src/lib/ui/CoachTab.svelte` (전면 교체)

**Interfaces:**
- Consumes: `listFindings(true)`, `setFindingStatus`, `CoachFinding`(detail/suggested_action/fix_command/status), App prop `{ focusKey }`.
- Produces: 없음(리프 컴포넌트).

- [ ] **Step 1: CoachTab.svelte 전면 교체**

```svelte
<script lang="ts">
  import { listFindings, onNewFindings, setFindingStatus, type CoachFinding } from '../api';

  let { focusKey = null }: { focusKey?: string | null } = $props();

  let all = $state<CoachFinding[]>([]);
  let open = $state<string | null>(null);
  let showHidden = $state(false);
  let copied = $state<string | null>(null);

  const active = $derived(all.filter((f) => f.status === 'new'));
  const hidden = $derived(all.filter((f) => f.status !== 'new'));

  async function refresh() {
    all = await listFindings(true).catch(() => []);
  }
  refresh();
  $effect(() => {
    const p = onNewFindings(() => refresh());
    return () => { p.then((u) => u()); };
  });

  // 홈 '절약 top3'에서 진입 시 해당 카드로 스크롤 (스펙 §2)
  $effect(() => {
    if (!focusKey || all.length === 0) return;
    const el = document.querySelector(`[data-key="${CSS.escape(focusKey)}"]`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    open = focusKey;
  });

  async function copy(f: CoachFinding) {
    if (!f.fix_command) return;
    await navigator.clipboard.writeText(f.fix_command).catch(() => {});
    copied = f.dedup_key;
    setTimeout(() => (copied = null), 1500);
  }

  async function mark(f: CoachFinding, status: 'resolved' | 'dismissed' | 'new') {
    await setFindingStatus(f.dedup_key, status).catch(() => {});
    await refresh();
  }

  const icon = (s: CoachFinding['severity']) => (s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ');
  const TITLE: Record<string, string> = {
    R1: '안 쓰는 MCP 서버가 토큰을 먹고 있어요',
    R2: '안 쓰는 플러그인이 자리만 차지해요',
    R5: '같은 파일을 반복해서 읽고 있어요',
    R7: '가벼운 작업에 Opus는 과해요',
    R9: '웹 검색이 너무 잦아요',
  };
</script>

<section class="coach">
  {#if active.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {:else}
    {#each active as f (f.dedup_key)}
      <article class="card" class:warn={f.severity === 'warn'} data-key={f.dedup_key}>
        <header>
          <span class="title">{icon(f.severity)} {TITLE[f.rule_id] ?? '아낄 수 있는 게 보여요'}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </header>
        <p class="why">{f.detail} · {f.occurrences}회 관측</p>
        <p class="how">➜ {f.suggested_action}</p>
        <div class="actions">
          {#if f.fix_command}
            <button class="cmd" onclick={() => copy(f)}>
              {copied === f.dedup_key ? '복사됨!' : `📋 ${f.fix_command}`}
            </button>
          {/if}
          <button onclick={() => mark(f, 'resolved')}>해결함</button>
          <button onclick={() => mark(f, 'dismissed')}>무시</button>
        </div>
        <button class="raw-toggle" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
          {open === f.dedup_key ? '▾' : '▸'} 원본 데이터
        </button>
        {#if open === f.dedup_key}
          <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
        {/if}
      </article>
    {/each}
  {/if}

  {#if hidden.length > 0}
    <button class="hidden-toggle" onclick={() => (showHidden = !showHidden)}>
      숨긴 항목 {hidden.length}개 {showHidden ? '접기' : '보기'}
    </button>
    {#if showHidden}
      {#each hidden as f (f.dedup_key)}
        <article class="card muted" data-key={f.dedup_key}>
          <header>
            <span class="title">{f.status === 'resolved' ? '✔ 해결함' : '✕ 무시'} · {TITLE[f.rule_id] ?? f.rule_id}</span>
            <button onclick={() => mark(f, 'new')}>다시 보기</button>
          </header>
          <p class="why">{f.detail}</p>
        </article>
      {/each}
    {/if}
  {/if}
</section>

<style>
  .coach { padding: 14px 16px; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; }
  .empty { color: var(--ink-soft); }
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border-left: 4px solid var(--pastel-mint);
  }
  .card.warn { border-left-color: var(--pastel-coral); }
  .card.muted { opacity: 0.75; border-left-color: var(--pastel-lav); }
  header { display: flex; justify-content: space-between; gap: 8px; align-items: baseline; }
  .title { font-weight: 600; }
  .save { color: var(--accent); font-size: 12px; white-space: nowrap; }
  .why { margin: 6px 0 2px; font-size: 12px; color: var(--ink-soft); }
  .how { margin: 2px 0 8px; font-size: 13px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .actions button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 5px 10px;
  }
  .actions .cmd { background: var(--pastel-cream); font-family: Consolas, monospace; }
  .raw-toggle {
    margin-top: 8px; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 11px; color: var(--ink-soft); padding: 0;
  }
  pre {
    background: #f4f1fa; border-radius: var(--radius-s);
    padding: 8px; overflow-x: auto; font-size: 11px; margin: 6px 0 0;
  }
  .hidden-toggle {
    align-self: flex-start; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 12px; color: var(--ink-soft); text-decoration: underline; padding: 0;
  }
  header button {
    border: none; cursor: pointer; font: inherit; font-size: 11px;
    background: var(--pastel-mint); border-radius: var(--radius-s); padding: 3px 8px;
  }
</style>
```

- [ ] **Step 2: 빌드 확인**

```bash
npm run build && npx vitest run
```
Expected: 빌드 성공, vitest 33 무회귀

- [ ] **Step 3: 커밋**

```bash
git add src/lib/ui/CoachTab.svelte
git commit -m "feat(front): 코칭 탭 재설계 — 무엇이→왜→어떻게 카드, 명령 복사, 해결함/무시 (스펙 §3)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 다이어리 탭 — 달력 + 마크다운 본문

**Files:**
- Create: `src/lib/ui/calendar.ts`, `src/lib/ui/calendar.test.ts`
- Modify: `src/lib/ui/DiaryTab.svelte` (Task 3 스텁 교체)
- Modify: `package.json` (marked·dompurify 추가 — `npm i` 스텝에서)

**Interfaces:**
- Consumes: 기존 커맨드 `list_diary_dates`/`get_diary`(api.ts에 래퍼 추가 필요 — 아래 Step 5).
- Produces: `monthGrid(year, month) -> CalCell[42]`, `CalCell { date: string; day: number; inMonth: boolean }`, `shiftMonth(year, month, delta) -> [number, number]`.

- [ ] **Step 1: calendar.ts 실패 테스트**

`src/lib/ui/calendar.test.ts` 신규:

```ts
import { describe, expect, it } from 'vitest';
import { monthGrid, shiftMonth } from './calendar';

describe('monthGrid', () => {
  it('일요일 시작 6주(42칸), 2026-07은 수요일 시작', () => {
    const g = monthGrid(2026, 7); // 2026-07-01 = 수요일
    expect(g.length).toBe(42);
    expect(g[0]).toEqual({ date: '2026-06-28', day: 28, inMonth: false }); // 앞 일요일
    expect(g[3]).toEqual({ date: '2026-07-01', day: 1, inMonth: true });
    expect(g[33]).toEqual({ date: '2026-07-31', day: 31, inMonth: true });
    expect(g[34].inMonth).toBe(false); // 8월 칸
  });
  it('연도 경계 패딩', () => {
    const g = monthGrid(2026, 1); // 2026-01-01 = 목요일
    expect(g[4]).toEqual({ date: '2026-01-01', day: 1, inMonth: true });
    expect(g[0].date).toBe('2025-12-28');
  });
});

describe('shiftMonth', () => {
  it('연도 넘김', () => {
    expect(shiftMonth(2026, 1, -1)).toEqual([2025, 12]);
    expect(shiftMonth(2026, 12, 1)).toEqual([2027, 1]);
    expect(shiftMonth(2026, 7, -1)).toEqual([2026, 6]);
  });
});
```

- [ ] **Step 2: 실패 확인**

```bash
npx vitest run src/lib/ui/calendar.test.ts
```
Expected: FAIL — `./calendar` 모듈 없음

- [ ] **Step 3: calendar.ts 구현**

```ts
// 월 달력 그리드 — UTC 기반 계산으로 타임존 무관 결정성.
export interface CalCell {
  date: string; // YYYY-MM-DD
  day: number;
  inMonth: boolean;
}

const iso = (d: Date) => d.toISOString().slice(0, 10);

/** 일요일 시작, 항상 6주(42칸). month는 1~12. */
export function monthGrid(year: number, month: number): CalCell[] {
  const first = new Date(Date.UTC(year, month - 1, 1));
  const start = new Date(first);
  start.setUTCDate(1 - first.getUTCDay()); // 그 주 일요일로
  const cells: CalCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start);
    d.setUTCDate(start.getUTCDate() + i);
    cells.push({ date: iso(d), day: d.getUTCDate(), inMonth: d.getUTCMonth() === month - 1 });
  }
  return cells;
}

export function shiftMonth(year: number, month: number, delta: number): [number, number] {
  const d = new Date(Date.UTC(year, month - 1 + delta, 1));
  return [d.getUTCFullYear(), d.getUTCMonth() + 1];
}
```

- [ ] **Step 4: 통과 확인**

```bash
npx vitest run src/lib/ui/calendar.test.ts
```
Expected: PASS 3

- [ ] **Step 5: 의존성 설치 + api.ts 래퍼**

```bash
npm i marked dompurify
```

`src/lib/api.ts`에 추가(다른 invoke 래퍼들 옆):

```ts
export const listDiaryDates = () => invoke<string[]>('list_diary_dates');
export const getDiary = (date: string) => invoke<string | null>('get_diary', { date });
```

- [ ] **Step 6: DiaryTab.svelte 구현 (스텁 교체)**

```svelte
<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { getDiary, listDiaryDates, onDiaryReady } from '../api';
  import { monthGrid, shiftMonth } from './calendar';

  const now = new Date();
  let year = $state(now.getFullYear());
  let month = $state(now.getMonth() + 1);
  let dates = $state<Set<string>>(new Set());
  let selected = $state<string | null>(null);
  let html = $state<string | null>(null);

  const grid = $derived(monthGrid(year, month));

  async function loadDates() {
    dates = new Set(await listDiaryDates().catch(() => []));
  }
  loadDates();
  $effect(() => {
    const p = onDiaryReady(() => loadDates());
    return () => { p.then((u) => u()); };
  });

  async function pick(date: string) {
    if (!dates.has(date)) return;
    selected = date;
    const text = await getDiary(date).catch(() => null);
    // 일기는 우리 엔진(LLM) 산출물 — 웹뷰 주입 전 반드시 살균 (스펙 §4)
    html = text ? DOMPurify.sanitize(await marked.parse(text)) : null;
  }

  function nav(delta: number) {
    [year, month] = shiftMonth(year, month, delta);
  }
</script>

<section class="diary">
  <div class="cal">
    <header>
      <button onclick={() => nav(-1)}>‹</button>
      <b>{year}. {String(month).padStart(2, '0')}</b>
      <button onclick={() => nav(1)}>›</button>
    </header>
    <div class="dow">
      {#each ['일', '월', '화', '수', '목', '금', '토'] as d (d)}<span>{d}</span>{/each}
    </div>
    <div class="cells">
      {#each grid as c (c.date)}
        <button
          class:out={!c.inMonth}
          class:has={dates.has(c.date)}
          class:sel={selected === c.date}
          disabled={!dates.has(c.date)}
          onclick={() => pick(c.date)}
        >
          {c.day}{#if dates.has(c.date)}<i class="dot"></i>{/if}
        </button>
      {/each}
    </div>
  </div>
  <div class="body">
    {#if html}
      <article>{@html html}</article>
    {:else if selected}
      <p class="empty">이 날 일기를 불러오지 못했어요.</p>
    {:else}
      <p class="empty">도트 찍힌 날짜를 눌러 보세요. 그날의 일기가 나와요.</p>
    {/if}
  </div>
</section>

<style>
  .diary { display: flex; gap: 14px; padding: 14px 16px; flex: 1; min-height: 0; }
  .cal {
    width: 238px; flex-shrink: 0; align-self: flex-start;
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px;
  }
  .cal header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  .cal header button { border: none; background: var(--pastel-lav); border-radius: var(--radius-s); cursor: pointer; font: inherit; padding: 2px 9px; }
  .dow, .cells { display: grid; grid-template-columns: repeat(7, 1fr); }
  .dow span { text-align: center; font-size: 10px; color: var(--ink-soft); padding: 2px 0; }
  .cells button {
    position: relative; border: none; background: none; font: inherit; font-size: 11px;
    padding: 6px 0; color: var(--ink); border-radius: var(--radius-s);
  }
  .cells button.out { color: var(--pastel-lav); }
  .cells button.has { cursor: pointer; background: var(--pastel-cream); }
  .cells button.sel { background: var(--accent); color: #fff; }
  .cells button:disabled { cursor: default; }
  .dot {
    position: absolute; left: 50%; bottom: 1px; transform: translateX(-50%);
    width: 4px; height: 4px; border-radius: 50%; background: var(--accent);
  }
  .cells button.sel .dot { background: #fff; }
  .body { flex: 1; overflow-y: auto; min-width: 0; }
  .empty { color: var(--ink-soft); }
  article :global(h1), article :global(h2), article :global(h3) { font-size: 15px; }
</style>
```

- [ ] **Step 7: 빌드·테스트 확인**

```bash
npm run build && npx vitest run
```
Expected: 빌드 성공, vitest 33 + 3(calendar) = 36 PASS

- [ ] **Step 8: 커밋**

```bash
git add src/lib/ui/calendar.ts src/lib/ui/calendar.test.ts src/lib/ui/DiaryTab.svelte src/lib/api.ts package.json package-lock.json
git commit -m "feat(front): 다이어리 탭 — 미니 달력(도트)+날짜별 본문, marked+DOMPurify 렌더 (스펙 §4)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: 마스코트 개편 — 클릭 열기·지속 말풍선·정책·파스텔

**Files:**
- Create: `src/lib/robot/drag.ts`, `src/lib/robot/drag.test.ts`
- Modify: `src/lib/robot/bubble.ts`, `src/lib/robot/bubble.test.ts`
- Modify: `src/lib/robot/anim.ts`, `src/lib/robot/anim.test.ts`
- Modify: `src/lib/api.ts` (`emitOccasionToday` 재방송 래퍼 1줄)
- Modify: `src/Mascot.svelte` (전면 교체)

**Interfaces:**
- Consumes: Task 2 `getTodayOccasions`, Task 3 api.ts(`listFindings` → CoachFinding.detail).
- Produces:
  - drag.ts: `isDrag(x0, y0, x1, y1, threshold?) -> boolean` (기본 threshold 4px)
  - bubble.ts: `BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter'`(**'scan' 제거**), `Bubble { kind, text, tab, key?: string }`, `adviceBubble({ dedup_key, detail }) -> Bubble`, **`scanBubble`·`BubbleQueue` 삭제**, 나머지 팩토리 유지

- [ ] **Step 1: drag.ts 실패 테스트**

`src/lib/robot/drag.test.ts` 신규:

```ts
import { describe, expect, it } from 'vitest';
import { isDrag } from './drag';

describe('isDrag — 클릭 vs 드래그 판별 (스펙 §6, 4px 임계값)', () => {
  it('임계값 이내는 클릭', () => {
    expect(isDrag(100, 100, 100, 100)).toBe(false);
    expect(isDrag(100, 100, 103, 102)).toBe(false); // √13 ≈ 3.6
  });
  it('임계값 초과는 드래그', () => {
    expect(isDrag(100, 100, 105, 100)).toBe(true); // 5 > 4
    expect(isDrag(100, 100, 97, 97)).toBe(true); // √18 ≈ 4.2
  });
  it('커스텀 임계값', () => {
    expect(isDrag(0, 0, 5, 0, 6)).toBe(false);
  });
});
```

- [ ] **Step 2: 실패 확인**

```bash
npx vitest run src/lib/robot/drag.test.ts
```
Expected: FAIL — `./drag` 모듈 없음

- [ ] **Step 3: drag.ts 구현 + 통과 확인**

`src/lib/robot/drag.ts` 신규:

```ts
/** pointerdown→현재 좌표 이동 거리가 임계값을 넘으면 드래그로 판정 (스펙 §6). */
export function isDrag(x0: number, y0: number, x1: number, y1: number, threshold = 4): boolean {
  return Math.hypot(x1 - x0, y1 - y0) > threshold;
}
```

```bash
npx vitest run src/lib/robot/drag.test.ts
```
Expected: PASS 3

- [ ] **Step 4: bubble.ts 정책 개편 (테스트 먼저 수정)**

`src/lib/robot/bubble.test.ts` 전면 교체:

```ts
import { describe, expect, it } from 'vitest';
import { adviceBubble, chatterBubble, diaryBubble, findingBubble, occasionBubble } from './bubble';

describe('bubble 팩토리', () => {
  it('finding: 최대 절약 1건 + 외 N건, coach 탭', () => {
    const b = findingBubble([
      { rule_id: 'R5', est_tokens_saved: 100, severity: 'suggest' },
      { rule_id: 'R1', est_tokens_saved: 30000, severity: 'warn' },
    ]);
    expect(b.tab).toBe('coach');
    expect(b.text).toContain('외 1건');
    expect(b.text.includes('MCP')).toBe(true); // R1 문구가 대표
  });
  it('advice: detail을 싣고 dedup_key를 반복 방지 키로', () => {
    const b = adviceBubble({ dedup_key: 'k1', detail: '`playwright`가 상주하는데 호출 0회' });
    expect(b.kind).toBe('finding');
    expect(b.tab).toBe('coach');
    expect(b.key).toBe('k1');
    expect(b.text).toContain('playwright');
  });
  it('diary는 diary 탭, occasion은 첫 라벨, chatter는 수치 삽입', () => {
    expect(diaryBubble('2026-07-02').tab).toBe('diary');
    expect(occasionBubble(['크리스마스', '함께한 지 100일']).text).toContain('크리스마스');
    expect(chatterBubble(0, { session_count: 7 }).text).toContain('7');
    expect(chatterBubble(3, null).tab).toBe('home');
  });
});
```

`src/lib/robot/bubble.ts` 수정:
- 1행 타입에서 `'scan'` 제거: `export type BubbleKind = 'finding' | 'diary' | 'occasion' | 'chatter';`
- `Bubble` 인터페이스에 `key?: string;` 필드 추가
- `BubbleQueue` 클래스(11~23행)와 `MAX_QUEUE`, `scanBubble` 함수(67~73행) **삭제**
- `adviceBubble` 추가:

```ts
/** realtime_advice 옵트인: 스캔 후 최상위 활성 advice를 말풍선으로 (스펙 §6).
 *  key(dedup_key)로 직전과 같은 조언 반복을 호출측에서 방지한다. */
export function adviceBubble(f: { dedup_key: string; detail: string }): Bubble {
  return { kind: 'finding', tab: 'coach', text: `주인, ${f.detail}`, key: f.dedup_key };
}
```

`src/lib/robot/anim.ts:19`의 `case 'scan':` 줄 삭제(`case 'chatter': return 'talk';`만 남김).
`src/lib/robot/anim.test.ts:11-12`의 scan 케이스 테스트 블록 삭제.

- [ ] **Step 5: vitest 전체 확인**

```bash
npx vitest run
```
Expected: PASS — 기존 36 − 삭제(BubbleQueue 1, scanBubble 1, anim scan 1) + 신규(drag 3, advice 1) = 39개 근방(실측 기준). `scan`/`BubbleQueue` 참조가 남아 있으면 컴파일 에러 — Mascot.svelte는 다음 스텝에서 고친다(이 시점 vitest는 .ts만 대상이라 통과).

- [ ] **Step 6: api.ts 재방송 래퍼 + Mascot.svelte 전면 교체**

`src/lib/api.ts` 상단 import를 `import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';`로 바꾸고 래퍼 추가:

```ts
// 마스코트가 pull한 occasion을 chat 창 알림 로그용으로 재방송 (pull 단일화 — 플랜 Task 2 Step 5)
export const emitOccasionToday = (labels: string[]) => emit('occasion:today', labels);
```

`src/Mascot.svelte` 전면 교체:

```svelte
<script lang="ts">
  import { getCurrentWindow, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
  import './lib/theme.css';
  import {
    emitOccasionToday, getMascotSeed, getSettings, getSummary, getTodayOccasions,
    listFindings, openChatTab, setSetting,
    onDiaryReady, onNewFindings, onScanDone, onSettingsChanged,
  } from './lib/api';
  import { drawRobot, type RobotSpec } from './lib/robot/render';
  import { frameAt, resolveState, type BubbleKind } from './lib/robot/anim';
  import { adviceBubble, chatterBubble, diaryBubble, findingBubble, occasionBubble, type Bubble } from './lib/robot/bubble';
  import { isDrag } from './lib/robot/drag';

  const win = getCurrentWindow();
  const BASE = { w: 160, h: 160 };
  const EXPANDED = { w: 320, h: 230 };

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let bubble = $state<Bubble | null>(null);
  let chatterLevel = $state('low');
  let realtimeAdvice = $state(false);
  let lastAdviceKey: string | null = null;

  // 지속 말풍선 (스펙 §6): 자동 소멸 없음 — 교체/X/본문 클릭까지 유지
  async function showBubble(b: Bubble) {
    const wasShowing = bubble !== null;
    bubble = b;
    if (!wasShowing) await expand(true);
  }
  async function closeBubble(openTab: boolean) {
    const b = bubble;
    if (!b) return;
    bubble = null;
    await expand(false);
    if (openTab) openChatTab(b.tab);
  }

  async function expand(on: boolean) {
    // 캐릭터(창 우하단 고정)가 화면상 제자리를 지키도록 위치 보정 (델타는 물리 픽셀로 환산)
    const scale = await win.scaleFactor();
    const pos = await win.outerPosition();
    const dw = Math.round((EXPANDED.w - BASE.w) * scale);
    const dh = Math.round((EXPANDED.h - BASE.h) * scale);
    if (on) {
      await win.setPosition(new PhysicalPosition(pos.x - dw, pos.y - dh));
      await win.setSize(new LogicalSize(EXPANDED.w, EXPANDED.h));
    } else {
      await win.setSize(new LogicalSize(BASE.w, BASE.h));
      await win.setPosition(new PhysicalPosition(pos.x + dw, pos.y + dh));
    }
  }

  async function loadSettings() {
    const s = await getSettings().catch(() => ({}) as Record<string, string>);
    chatterLevel = s['chatter_level'] ?? 'low';
    realtimeAdvice = s['realtime_advice'] === 'on';
  }

  // occasion: pull 단일 경로 (시작 레이스 방어 — 스펙 §6). 게이트(하루 1회)는 백엔드가 가짐.
  // pull 성공 시 chat 알림 로그를 위해 같은 이벤트명으로 재방송한다.
  async function pullOccasions() {
    const labels = await getTodayOccasions().catch(() => [] as string[]);
    if (labels.length) {
      showBubble(occasionBubble(labels));
      emitOccasionToday(labels).catch(() => {});
    }
  }

  // 트리거 배선 (스펙 §6: 새/악화 advice·다이어리·occasion·잡담만 — 스캔 요약 대사 없음)
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && showBubble(findingBubble(rows))),
      onDiaryReady((date) => showBubble(diaryBubble(date))),
      onScanDone(async () => {
        pullOccasions(); // 자정 넘김 대비 — 게이트 덕에 하루 1회만 유효
        if (!realtimeAdvice) return;
        const rows = await listFindings(false).catch(() => []);
        const top = rows[0];
        if (top && top.dedup_key !== lastAdviceKey) {
          lastAdviceKey = top.dedup_key;
          showBubble(adviceBubble(top));
        }
      }),
      onSettingsChanged(() => loadSettings()),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  // 마운트 시 1회 pull
  pullOccasions();

  // 잡담 타이머 — 말풍선 떠 있는 동안엔 침묵
  $effect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const [min, max] = chatterLevel === 'normal' ? [20, 40] : [60, 120];
      const delayMin = min + Math.random() * (max - min);
      timer = setTimeout(async () => {
        const hour = new Date().getHours();
        if (chatterLevel !== 'off' && !(hour >= 1 && hour < 7) && bubble === null) {
          const summary = await getSummary().catch(() => null);
          showBubble(chatterBubble(Math.floor(Math.random() * 10), summary));
        }
        schedule();
      }, delayMin * 60_000);
    };
    schedule();
    return () => clearTimeout(timer);
  });

  // 위치 저장 (이동 1초 디바운스)
  $effect(() => {
    let t: ReturnType<typeof setTimeout>;
    const p = win.onMoved(({ payload }) => {
      clearTimeout(t);
      t = setTimeout(() => {
        if (bubble !== null) return; // 말풍선 확장 중 임시 좌표는 저장하지 않음
        setSetting('mascot_pos', `${payload.x},${payload.y}`).catch(() => {});
      }, 1000);
    });
    return () => { clearTimeout(t); p.then((u) => u()); };
  });

  // 클릭 vs 드래그 (스펙 §6): drag-region 대신 수동 판별 — 클릭이면 홈피 열기
  let downAt: { x: number; y: number } | null = null;
  function onPointerDown(e: PointerEvent) {
    downAt = { x: e.screenX, y: e.screenY };
  }
  function onPointerMove(e: PointerEvent) {
    if (!downAt) return;
    if (isDrag(downAt.x, downAt.y, e.screenX, e.screenY)) {
      downAt = null;
      win.startDragging(); // 이후는 OS가 이동을 소유
    }
  }
  function onPointerUp() {
    if (downAt) {
      downAt = null;
      openChatTab('home');
    }
  }

  // 렌더 루프
  $effect(() => {
    if (!canvas || !spec) return;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    const loop = (t: number) => {
      const state = resolveState({ bubbleKind: (bubble?.kind ?? null) as BubbleKind | null, hour: new Date().getHours() });
      const f = frameAt(state, t);
      drawRobot(ctx, spec!, f);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  getMascotSeed().then((s) => (spec = s));
  loadSettings();
</script>

<div class="stage" class:expanded={bubble !== null}>
  {#if bubble}
    <div class="bubble">
      <button class="text" onclick={() => closeBubble(true)}>{bubble.text}</button>
      <button class="x" aria-label="닫기" onclick={() => closeBubble(false)}>×</button>
    </div>
  {/if}
  <div
    class="robot"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
  >
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; background: transparent; overflow: hidden; }
  .stage { width: 100vw; height: 100vh; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-end; }
  .robot { width: 128px; height: 128px; margin: 0 16px 16px 0; cursor: pointer; touch-action: none; }
  canvas { width: 128px; height: 128px; image-rendering: pixelated; }
  .bubble {
    display: flex; align-items: flex-start; gap: 2px;
    max-width: 280px; margin: 8px 12px 0 0; padding: 9px 6px 9px 12px;
    background: var(--frame-bg); color: var(--ink);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    font: 12px 'Segoe UI', 'Malgun Gothic', sans-serif;
  }
  .bubble .text {
    border: none; background: none; font: inherit; color: inherit;
    cursor: pointer; text-align: left; padding: 0;
    display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .bubble .x {
    border: none; background: none; cursor: pointer; padding: 0 4px;
    font-size: 13px; line-height: 1; color: var(--ink-soft);
  }
  .bubble .x:hover { color: var(--ink); }
</style>
```

주의: 기존 `data-tauri-drag-region` 속성 2개(div·canvas)는 **완전히 제거**됐다(수동 판별과 충돌).

- [ ] **Step 7: 전체 게이트**

```bash
npx vitest run && npm run build && cargo test --workspace
```
Expected: vitest 전체 PASS(스텝 5 수치 + Mascot 컴파일 통과), 빌드 성공, cargo 113(core 102+app 11) 무회귀

- [ ] **Step 8: 커밋**

```bash
git add src/lib/robot/drag.ts src/lib/robot/drag.test.ts src/lib/robot/bubble.ts src/lib/robot/bubble.test.ts src/lib/robot/anim.ts src/lib/robot/anim.test.ts src/lib/api.ts src/Mascot.svelte
git commit -m "feat(front): 마스코트 개편 — 클릭→홈피, 지속 말풍선+X, 스캔 대사 제거, 파스텔 스타일 (스펙 §6)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 최종 검증 게이트 + 수동 E2E 체크리스트

**Files:** 없음(검증만). 발견된 결함은 이 태스크에서 수정 커밋.

- [ ] **Step 1: 자동 게이트 전체 실행**

```bash
cargo test --workspace && cargo build && npx vitest run && npm run build && npx svelte-check --no-tsconfig 2>/dev/null || npx svelte-check
```
Expected: cargo 113 무경고, vitest 전체 PASS, 빌드 2종 성공. (svelte-check 미설치면 생략 가능 — vite build의 svelte 플러그인이 컴파일 에러를 잡는다)

- [ ] **Step 2: 개발 실행으로 수동 E2E (사용자 또는 컨트롤러)**

```bash
npm run tauri dev
```

체크리스트(스펙 §1~§6 대응):
1. chat 창: 파스텔 배경 위 흰 프레임, 타이틀바 "{사용자명}님의 미니홈피" + TODAY/TOTAL 숫자
2. 우측 세로 탭 4개, 활성 탭 색 연결, 코칭 탭 뱃지 수 = 활성 finding 수
3. 좌측: 로봇 초상(내 로봇과 동일 외형), 기분 한마디
4. 홈: 스트립 수치, 주간 추이 7개 바(오늘 강조), 모델 분포 스택 바, 절약 top3 클릭 → 코칭 탭 해당 카드로 스크롤, 최근 알림
5. 마이룸: 로봇 idle 애니, 클릭 → happy 모션+대사 변경, 45초 후 대사 순환
6. 코칭: 카드 3단 구조(제목/왜/어떻게), 📋 버튼 → 클립보드에 명령, [해결함]/[무시] → 카드 사라짐+"숨긴 항목 N개" 표시, 복원 동작, 절약가능 합계가 숨김 반영
7. 다이어리: 달력 도트, 날짜 클릭 → 마크다운 렌더된 본문(`##` 등 원시 노출 없음), 월 이동
8. 마스코트: 드래그 이동 정상(4px 이상), 짧은 클릭 → chat 창 홈 탭 열림, 재시작 시 위치 복원(모니터 밖 좌표는 우하단 폴백)
9. 말풍선: 자동으로 사라지지 않음, X로 닫힘, 본문 클릭 → 관련 탭, 스캔해도 "수집 완료" 류 대사 없음, 파스텔 둥근 스타일
10. 트레이 "지금 스캔" → 코칭·홈 갱신 (기존 무회귀)

- [ ] **Step 3: progress 원장 갱신 + 최종 커밋**

`.superpowers/sdd/progress.md`에 PR① 원장 추가(태스크별 상태·게이트 수치·E2E 결과). 잔여 이슈는 "후속 과제"에 기록.

```bash
git add .superpowers/sdd/progress.md
git commit -m "docs: 미니홈피 PR① 최종 게이트·E2E 체크 기록

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 플랜 셀프리뷰 기록

- **스펙 커버리지**: §1(Task 3) §2(Task 3·4·5) §3(Task 1·2·6) §4(Task 7) §6(Task 2·8) §8(Task 1·2·3) §9(각 태스크 게이트+Task 9). §5·§7은 PR② 별도 플랜.
- **타입 일관성**: `CoachFinding`(rust flatten ↔ ts extends), `DayStat`/`ModelMixEntry` snake_case 직렬화 ↔ ts 필드 일치, `listFindings(includeHidden)` ↔ rust `include_hidden: Option<bool>`(Tauri v2 camelCase 자동 변환) 확인.
- **알려진 유예(의도)**: dismissed finding의 악화는 말풍선 침묵(사용자 의사 존중), occasion은 push 제거·pull 단일화(마스코트 창이 숨김이어도 웹뷰는 pull을 수행 — 하루치가 조용히 소비될 수 있으나 재방송으로 chat 알림 로그엔 남음), 진행률 바는 상태줄 자리만(PR②).
