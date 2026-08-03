use crate::finding::{Finding, Prescription, Severity};
use crate::model::{EventKind, NormalizedEvent, ToolKind};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY, host TEXT, project_id TEXT, agent TEXT,
  first_ts TEXT, last_ts TEXT, git_branch TEXT,
  cwd TEXT, first_prompt_preview TEXT, first_prompt_source_file TEXT, first_prompt_offset INTEGER, subagent_files INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  session_id TEXT NOT NULL, host TEXT NOT NULL, project_id TEXT NOT NULL,
  ts TEXT, source_offset INTEGER NOT NULL, kind TEXT NOT NULL,
  model_family TEXT, model_tier TEXT, model_raw TEXT,
  tok_input INTEGER DEFAULT 0, tok_output INTEGER DEFAULT 0,
  tok_cache_read INTEGER DEFAULT 0, tok_cache_create INTEGER DEFAULT 0,
  tok_eph_1h INTEGER DEFAULT 0, web_search INTEGER DEFAULT 0, web_fetch INTEGER DEFAULT 0,
  tool_kind TEXT, tool_server TEXT, tool_tool TEXT, tool_target TEXT, raw_name TEXT,
  is_sidechain INTEGER DEFAULT 0,
  source_file TEXT, tool_use_id TEXT, result_status TEXT,
  result_len INTEGER DEFAULT 0
);
-- 상관 서브쿼리의 상대는 언제나 session_id 다(struggle_sessions 는 세션당 3회 돈다).
-- 인덱스가 없으면 그 하나하나가 events 전체 스캔이 되어, 6만 행·292 세션에서 56초가 나온다(실측).
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE TABLE IF NOT EXISTS prompt_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  session_id TEXT NOT NULL, host TEXT NOT NULL, project_id TEXT NOT NULL,
  ts TEXT, source_file TEXT NOT NULL, source_offset INTEGER NOT NULL,
  norm60 TEXT NOT NULL, preview TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS daily_rollup (
  host TEXT NOT NULL, project_id TEXT NOT NULL, date TEXT NOT NULL,
  tok_input INTEGER DEFAULT 0, tok_output INTEGER DEFAULT 0,
  tok_cache_read INTEGER DEFAULT 0, tok_cache_create INTEGER DEFAULT 0,
  session_count INTEGER DEFAULT 0,
  PRIMARY KEY (host, project_id, date)
);
CREATE TABLE IF NOT EXISTS findings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  rule_id TEXT NOT NULL, severity TEXT NOT NULL,
  scope_host TEXT, scope_project TEXT, scope_kind TEXT, scope_ref TEXT,
  evidence_json TEXT NOT NULL, est_tokens_saved INTEGER DEFAULT 0,
  prescription_json TEXT, status TEXT NOT NULL DEFAULT 'new',
  first_seen TEXT, last_seen TEXT, occurrences INTEGER DEFAULT 1,
  judgment_json TEXT,
  status_evidence_n INTEGER, status_ts TEXT
);
CREATE TABLE IF NOT EXISTS mcp_inventory (
  host TEXT NOT NULL, project_id TEXT NOT NULL, server TEXT NOT NULL,
  source TEXT NOT NULL, tool_count INTEGER, est_def_tokens INTEGER,
  probed_at TEXT, last_used_ts TEXT,
  PRIMARY KEY (host, project_id, server)
);
CREATE TABLE IF NOT EXISTS plugin_inventory (
  host TEXT NOT NULL, plugin_key TEXT NOT NULL, namespace TEXT NOT NULL,
  skill_count INTEGER DEFAULT 0, resident_tokens INTEGER DEFAULT 0,
  skills_json TEXT NOT NULL, mcp_servers_json TEXT NOT NULL,
  PRIMARY KEY (host, plugin_key)
);
CREATE TABLE IF NOT EXISTS diary_index (
  date TEXT NOT NULL, scope TEXT NOT NULL, path TEXT NOT NULL,
  tokens_used INTEGER DEFAULT 0, engine TEXT,
  PRIMARY KEY (date, scope)
);
CREATE TABLE IF NOT EXISTS ingest_state (
  source_file TEXT PRIMARY KEY, last_offset INTEGER NOT NULL DEFAULT 0, last_mtime INTEGER
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY, value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS daily_line (
  date TEXT PRIMARY KEY, text TEXT NOT NULL, fingerprint TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS chatter_pool (
  date TEXT PRIMARY KEY, lines TEXT NOT NULL, fingerprint TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS content_items (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL, dimension TEXT, title TEXT NOT NULL, body TEXT NOT NULL,
  source_url TEXT, trigger_tags TEXT NOT NULL DEFAULT '[]',
  score INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL DEFAULT 'new',
  first_seen TEXT, last_seen TEXT,
  status_ts TEXT,
  -- 소식 파이프라인(⑥ 스펙 §6.3·§6.4): 아이템당 1회 번역 결과 캐시.
  -- 재큐레이션이 덮어쓰지 않는다(replace_content_items의 ON CONFLICT 목록에 없음).
  summary_ko TEXT, title_ko TEXT, deadline TEXT
);
CREATE TABLE IF NOT EXISTS personal_skill_inventory (
  host TEXT NOT NULL, scope TEXT NOT NULL, name TEXT NOT NULL,
  path TEXT NOT NULL, body_chars INTEGER DEFAULT 0,
  PRIMARY KEY (host, path)
);
CREATE TABLE IF NOT EXISTS host_settings (
  host TEXT PRIMARY KEY, default_model TEXT, effort_level TEXT, scanned_at TEXT
);
CREATE TABLE IF NOT EXISTS hub_share_state (
  dedup_key TEXT PRIMARY KEY,
  issue_id  TEXT,
  page_id   TEXT,
  shared_at TEXT
);
CREATE TABLE IF NOT EXISTS memories (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  text       TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT,
  source     TEXT NOT NULL DEFAULT 'chat'
);
CREATE TABLE IF NOT EXISTS session_work_kinds (
  session_id TEXT PRIMARY KEY,
  host TEXT NOT NULL, project_id TEXT NOT NULL,
  kinds_json TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_ts TEXT,
  judged_at TEXT
);
CREATE TABLE IF NOT EXISTS life_visits (
  life_id            TEXT NOT NULL,
  visited_at         TEXT NOT NULL,
  kind               TEXT NOT NULL,
  owner_name         TEXT,
  design_json        TEXT,
  diary_excerpt_json TEXT NOT NULL DEFAULT '[]',
  signed             INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (life_id, visited_at)
);
"#;

pub struct SqliteStore {
    pub conn: Connection,
}

/// 방문 1건 — 그 순간의 방 꾸밈·상대 공개 일기 발췌 스냅샷을 함께 담는다 (스펙 §2).
/// 일기 생성이 네트워크 없이 소재를 얻는 유일한 경로.
#[derive(Debug, Clone, PartialEq)]
pub struct LifeVisit {
    pub life_id: String,
    pub visited_at: String, // RFC3339. 날짜 버킷은 date(visited_at,'localtime')
    pub kind: String,       // "manual" | "auto"
    pub owner_name: Option<String>,
    pub design_json: Option<String>,
    pub diary_excerpt_json: String, // [{date, excerpt}] — 없으면 "[]"
    pub signed: bool,
}

fn row_to_life_visit(r: &rusqlite::Row) -> rusqlite::Result<LifeVisit> {
    Ok(LifeVisit {
        life_id: r.get(0)?,
        visited_at: r.get(1)?,
        kind: r.get(2)?,
        owner_name: r.get(3)?,
        design_json: r.get(4)?,
        diary_excerpt_json: r.get(5)?,
        signed: r.get::<_, i64>(6)? != 0,
    })
}

const LIFE_VISIT_COLS: &str =
    "life_id, visited_at, kind, owner_name, design_json, diary_excerpt_json, signed";

/// 스키마 마이그레이션. events.model_raw 추가 — 기존 행은 raw id를 소급할 수 없으므로
/// events/ingest_state/daily_rollup을 비워 다음 스캔에서 전체 재수집한다
/// (로컬 JSONL 파생 데이터라 손실 없음, findings·status·diary_index는 보존).
fn migrate(conn: &Connection) -> Result<()> {
    // v2 model_raw 마이그레이션(기존)
    let has_model_raw = conn
        .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name='model_raw'")?
        .exists([])?;
    if !has_model_raw {
        conn.execute_batch(
            "ALTER TABLE events ADD COLUMN model_raw TEXT;
             DELETE FROM events; DELETE FROM ingest_state; DELETE FROM daily_rollup;",
        )?;
    }
    // v2.1 수집 마이그레이션 — source_file 부재 시 컬럼 추가 + 전체 재수집(sessions 포함).
    let has_source_file = conn
        .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name='source_file'")?
        .exists([])?;
    if !has_source_file {
        conn.execute_batch(
            "ALTER TABLE events ADD COLUMN source_file TEXT;
             ALTER TABLE events ADD COLUMN tool_use_id TEXT;
             ALTER TABLE events ADD COLUMN result_status TEXT;
             ALTER TABLE sessions ADD COLUMN cwd TEXT;
             ALTER TABLE sessions ADD COLUMN first_prompt_preview TEXT;
             ALTER TABLE sessions ADD COLUMN first_prompt_source_file TEXT;
             ALTER TABLE sessions ADD COLUMN first_prompt_offset INTEGER;
             DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;",
        )?;
    }
    // R8 대형 결과 마이그레이션 — result_len 부재 시 컬럼 추가 + 이벤트 재수집(결과 크기 백필).
    let has_result_len = conn
        .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name='result_len'")?
        .exists([])?;
    if !has_result_len {
        conn.execute_batch(
            "ALTER TABLE events ADD COLUMN result_len INTEGER DEFAULT 0;
             DELETE FROM events; DELETE FROM ingest_state; DELETE FROM daily_rollup;",
        )?;
    }
    // v3 수집 마이그레이션 — subagent_files 부재 시 컬럼 추가 + 전체 재수집.
    // (Agent 툴 매핑·permission-mode·secret_flag·first_prompt 오염 수정이 라인 재해석을 요구 — 스펙 §4.4)
    let has_subagent_files = conn
        .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name='subagent_files'")?
        .exists([])?;
    if !has_subagent_files {
        conn.execute_batch(
            "ALTER TABLE sessions ADD COLUMN subagent_files INTEGER NOT NULL DEFAULT 0;
             DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;",
        )?;
    }
    // R6 판정 레이어(PR2) — findings.judgment_json 부재 시 컬럼만 추가(재수집 불필요, 판정은 새로 채워짐).
    let has_judgment = conn
        .prepare("SELECT 1 FROM pragma_table_info('findings') WHERE name='judgment_json'")?
        .exists([])?;
    if !has_judgment {
        conn.execute_batch("ALTER TABLE findings ADD COLUMN judgment_json TEXT;")?;
    }
    // 처분·수명 모델(PR③ 스펙 §5.3) — 컬럼 전용. `status_evidence_n`은 「해결함」 시점의 룰별
    // 근거 수치 스냅숏(재발 판정용), `status_ts`는 처분 시각(해결함 7일 창 계산용)이다.
    // judgment_json 선례대로 **재수집을 유발하지 않는다** — 기존 행은 NULL로 남고,
    // NULL 스냅숏은 재발 판정에서 제외되어 묵은 카드가 한꺼번에 되살아나지 않는다(§5.1).
    let has_status_evidence = conn
        .prepare("SELECT 1 FROM pragma_table_info('findings') WHERE name='status_evidence_n'")?
        .exists([])?;
    if !has_status_evidence {
        conn.execute_batch(
            "ALTER TABLE findings ADD COLUMN status_evidence_n INTEGER;
             ALTER TABLE findings ADD COLUMN status_ts TEXT;",
        )?;
    }
    let has_content_status_ts = conn
        .prepare("SELECT 1 FROM pragma_table_info('content_items') WHERE name='status_ts'")?
        .exists([])?;
    if !has_content_status_ts {
        conn.execute_batch("ALTER TABLE content_items ADD COLUMN status_ts TEXT;")?;
    }
    // 소식 파이프라인(⑥ 스펙 §6.3·§6.4) — content_items 번역 캐시. **컬럼만** 추가한다:
    // 번역은 다음 스캔에 새로 채워지므로 재수집이 필요 없고, 여기에 DELETE를 붙이면
    // 릴리스마다 콜드 스캔이 되돌아온다(#146). judgment_json 전례.
    let has_summary_ko = conn
        .prepare("SELECT 1 FROM pragma_table_info('content_items') WHERE name='summary_ko'")?
        .exists([])?;
    if !has_summary_ko {
        conn.execute_batch(
            "ALTER TABLE content_items ADD COLUMN summary_ko TEXT;
             ALTER TABLE content_items ADD COLUMN title_ko TEXT;
             ALTER TABLE content_items ADD COLUMN deadline TEXT;",
        )?;
    }
    // v3.1 재수집 — IDE 합성 블록(<ide_opened_file> 등) 프롬프트 오염 수정이 라인 재해석을 요구.
    // 스키마 변화가 없어 PRAGMA user_version(=1)으로 1회 트리거. 오염 preview에서 파생된
    // R6 finding만 삭제 (repeated_prompt가 '<'로 시작 = 합성 마커 확정).
    let user_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if user_version < 1 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM findings WHERE rule_id='R6'
               AND json_extract(evidence_json, '$.repeated_prompt') LIKE '<%';
             PRAGMA user_version = 1;",
        )?;
    }
    // v3.2 재수집 — prompt_events 신설(세션 내 전체 프롬프트 축적, R6 v2 스펙 §4)이 백필을 요구.
    if user_version < 2 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM prompt_events;
             PRAGMA user_version = 2;",
        )?;
    }
    // v3.3 정리 — R23 특이 토큰 가드 도입(에이전트 자율 루프 노이즈, 2026-07-20 사용자 판정).
    // 가드 이전에 쌓인 R23 finding을 전량 삭제해 새 기준으로 재산출.
    if user_version < 3 {
        conn.execute_batch(
            "DELETE FROM findings WHERE rule_id='R23';
             PRAGMA user_version = 3;",
        )?;
    }
    // v3.4 재수집 — prompt_events 사이드체인 제외(Codex 리뷰 P2)가 재구축을 요구.
    if user_version < 4 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM prompt_events;
             PRAGMA user_version = 4;",
        )?;
    }
    // v3.5 정리 — R23 겹침 가족 dedup·host당 상한 도입 전에 쌓인 카드 홍수(200+)를
    // 일괄 삭제해 새 기준으로 재산출 (2026-07-20 실사용 판정). 재수집 불필요.
    // dismissed/resolved는 사용자 기록(나깅 방지 쿨다운)이라 보존한다.
    if user_version < 5 {
        conn.execute_batch(
            "DELETE FROM findings WHERE rule_id='R23' AND status='new';
             PRAGMA user_version = 5;",
        )?;
    }
    // v3.6 재수집 — 논리 dedup 키 도입(데이터 위생 스펙 §3.1): resume 포크 복제본·
    // 다중 라인 usage 반복을 기존 uuid:offset 행에서 소급 제거할 수 없어 전체 재수집한다.
    // 오염된 데이터로 만들어진 R6/R23 활성('new') 카드도 정화 — dismissed/resolved는
    // 사용자 기록(나깅 방지 쿨다운)이라 보존 (v5 전례).
    if user_version < 6 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM prompt_events;
             DELETE FROM findings WHERE rule_id IN ('R6','R23') AND status='new';
             PRAGMA user_version = 6;",
        )?;
    }
    // v7 R6 판정 레이어(PR2 스펙 §4.6) — R23 룰 폐기: finding 전량 삭제(dismissed 포함,
    // 룰이 사라져 쿨다운 기록도 무의미). PR1 배포로 노출됐던 R6 'new' 카드는 판정을 거치도록
    // pending으로 되돌린다. dismissed/resolved는 사용자 기록이라 보존. 재수집 불필요.
    if user_version < 7 {
        conn.execute_batch(
            "DELETE FROM findings WHERE rule_id='R23';
             UPDATE findings SET status='pending' WHERE rule_id='R6' AND status='new';
             PRAGMA user_version = 7;",
        )?;
    }
    // v8 재수집 — 스킬/커맨드 호출 프롬프트 제외(어댑터 is_command, 2026-07-23 순환 오탐 판정)가
    // prompt_events 재구축을 요구한다. 기존 uuid:offset 행에서 소급 제거할 수 없어 전체 재수집.
    // 그 프롬프트로 만들어진 R6 활성('new') 카드도 정화 — dismissed/resolved는 사용자
    // 기록(나깅 방지 쿨다운)이라 보존 (v6 전례).
    if user_version < 8 {
        conn.execute_batch(
            "DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;
             DELETE FROM prompt_events;
             DELETE FROM findings WHERE rule_id='R6' AND status='new';
             PRAGMA user_version = 8;",
        )?;
    }
    Ok(())
}

/// 논리 dedup 키용 내용 해시 — r6/r23의 hash8과 같은 규약 (sha256 앞 4바이트 hex).
fn hash8(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<SqliteStore> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(SqliteStore { conn })
    }

    pub fn open_in_memory() -> Result<SqliteStore> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(SqliteStore { conn })
    }

    pub fn upsert_events(&self, evs: &[NormalizedEvent]) -> Result<usize> {
        // 루프 전체를 단일 트랜잭션으로 — 문장마다 붙는 암묵 트랜잭션+fsync를 없애
        // 수집(특히 전체 재수집) 성능을 확보하고 배치 원자성을 보장한다 (PR#77 리뷰).
        let tx = self.conn.unchecked_transaction()?;
        let mut inserted = 0usize;
        for e in evs {
            // 논리 dedup 키 (데이터 위생 스펙 §3.1) — resume 포크 복제본과 다중 라인
            // usage 반복이 DB에 들어오지 않게 한다. uuid는 복제 시 재발급되지만
            // tool_use_id·message.id·ts는 보존된다 (2026-07-21 실데이터 검증).
            // 식별자가 없으면 기존 uuid:offset 규칙으로 폴백.
            let fallback = || match &e.uuid {
                Some(u) => format!("{}:{}", u, e.source_offset),
                None => format!("{}:{}", e.source_file, e.source_offset),
            };
            let dedup_key = match &e.kind {
                EventKind::ToolCall { tool_use_id: Some(tid), .. } => {
                    format!("tc:{}:{}", e.host, tid)
                }
                EventKind::ToolResult { tool_use_id, .. } if !tool_use_id.is_empty() => {
                    format!("tr:{}:{}", e.host, tool_use_id)
                }
                EventKind::AssistantTurn { .. } => match &e.msg_id {
                    Some(m) => format!("at:{}:{}", e.host, m),
                    None => fallback(),
                },
                _ => fallback(),
            };
            // 세션 단위 필드는 sessions로만 라우팅 (events 미삽입)
            match &e.kind {
                EventKind::SessionMeta { cwd, git_branch } => {
                    self.conn.execute(
                        "INSERT INTO sessions
                           (session_id, host, project_id, agent, first_ts, last_ts, git_branch, cwd)
                         VALUES (?1,?2,?3,'claude-code',?4,?4,?5,?6)
                         ON CONFLICT(session_id) DO UPDATE SET
                           last_ts  = MAX(COALESCE(last_ts, ?4),  COALESCE(?4, last_ts)),
                           first_ts = MIN(COALESCE(first_ts, ?4), COALESCE(?4, first_ts)),
                           git_branch = COALESCE(sessions.git_branch, ?5),
                           cwd        = COALESCE(sessions.cwd, ?6)",
                        params![e.session_id, e.host, e.project_id, e.ts, git_branch, cwd],
                    )?;
                    continue;
                }
                EventKind::UserPrompt { preview, is_command } => {
                    self.conn.execute(
                        "INSERT INTO sessions
                           (session_id, host, project_id, agent, first_ts, last_ts,
                            first_prompt_preview, first_prompt_source_file, first_prompt_offset)
                         VALUES (?1,?2,?3,'claude-code',?4,?4,?5,?6,?7)
                         ON CONFLICT(session_id) DO UPDATE SET
                           last_ts  = MAX(COALESCE(last_ts, ?4),  COALESCE(?4, last_ts)),
                           first_ts = MIN(COALESCE(first_ts, ?4), COALESCE(?4, first_ts)),
                           first_prompt_preview     = COALESCE(sessions.first_prompt_preview, ?5),
                           first_prompt_source_file = COALESCE(sessions.first_prompt_source_file, ?6),
                           first_prompt_offset      = COALESCE(sessions.first_prompt_offset, ?7)",
                        params![e.session_id, e.host, e.project_id, e.ts,
                                preview, e.source_file, e.source_offset as i64],
                    )?;
                    // 세션 내 전체 프롬프트 축적 — R6 v2 재료 (스펙 §4.1). 8자 미만은 정규화가 거른다.
                    // 사이드체인(서브에이전트) 프롬프트는 사용자 지시가 아니므로 제외.
                    // 스킬/커맨드 호출(is_command)은 이미 코드화된 지시라 반복 마이닝 제외 — 단
                    // first_prompt(위 sessions 갱신)는 세션 대표로 보존 (스펙 §8, 2026-07-26).
                    if e.is_sidechain || *is_command {
                        continue;
                    }
                    if let Some(norm60) = crate::rules::r6_repeated_prompts::normalize(preview) {
                        // 포크 복제본은 ts·내용이 보존되므로 (host, project, ts, 내용해시)가
                        // 논리 식별자 — 같은 물리적 입력은 세션 파일이 몇 개든 1행.
                        let pe_key = match &e.ts {
                            Some(ts) => format!(
                                "up:{}:{}:{}:{}", e.host, e.project_id, ts, hash8(preview)
                            ),
                            None => dedup_key.clone(),
                        };
                        self.conn.execute(
                            "INSERT OR IGNORE INTO prompt_events
                               (dedup_key, session_id, host, project_id, ts,
                                source_file, source_offset, norm60, preview)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                            params![pe_key, e.session_id, e.host, e.project_id, e.ts,
                                    e.source_file, e.source_offset as i64, norm60, preview],
                        )?;
                    }
                    continue;
                }
                _ => {}
            }

            let (tool_use_id, result_status, result_len) = match &e.kind {
                EventKind::ToolCall { tool_use_id, .. } => (tool_use_id.clone(), None, 0i64),
                EventKind::ToolResult { tool_use_id, status, result_len } =>
                    (Some(tool_use_id.clone()), Some(status.as_str().to_string()), *result_len as i64),
                _ => (None, None, 0i64),
            };

            // 봉투 공통 + kind별 컬럼 추출
            let (kind_str, mfam, mtier, mraw, ti, to, tcr, tcc, e1h, ws, wf,
                 tkind, tsrv, ttool, ttarget, raw) = flatten(e);

            let n = self.conn.execute(
                "INSERT OR IGNORE INTO events
                 (dedup_key, session_id, host, project_id, ts, source_offset, kind,
                  model_family, model_tier, model_raw, tok_input, tok_output, tok_cache_read,
                  tok_cache_create, tok_eph_1h, web_search, web_fetch,
                  tool_kind, tool_server, tool_tool, tool_target, raw_name, is_sidechain,
                  source_file, tool_use_id, result_status, result_len)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)",
                params![
                    dedup_key, e.session_id, e.host, e.project_id, e.ts, e.source_offset as i64, kind_str,
                    mfam, mtier, mraw, ti, to, tcr, tcc, e1h, ws, wf,
                    tkind, tsrv, ttool, ttarget, raw, e.is_sidechain as i64,
                    e.source_file, tool_use_id, result_status, result_len
                ],
            )?;
            inserted += n;

            // sessions 갱신 (첫/마지막 ts)
            self.conn.execute(
                "INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch)
                 VALUES (?1,?2,?3,'claude-code',?4,?4,NULL)
                 ON CONFLICT(session_id) DO UPDATE SET
                   last_ts = MAX(COALESCE(last_ts, ?4),  COALESCE(?4, last_ts)),
                   first_ts = MIN(COALESCE(first_ts, ?4), COALESCE(?4, first_ts))",
                params![e.session_id, e.host, e.project_id, e.ts],
            )?;
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn get_offset(&self, source_file: &str) -> Result<u64> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_offset FROM ingest_state WHERE source_file = ?1",
                params![source_file],
                |r| r.get(0),
            )
            .ok();
        Ok(v.unwrap_or(0) as u64)
    }

    pub fn set_offset(&self, source_file: &str, offset: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ingest_state (source_file, last_offset) VALUES (?1, ?2)
             ON CONFLICT(source_file) DO UPDATE SET last_offset = ?2",
            params![source_file, offset as i64],
        )?;
        Ok(())
    }

    pub fn count_events(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn earliest_session_ts(&self) -> Result<Option<String>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT MIN(first_ts) FROM sessions", [], |r| r.get(0))?;
        Ok(v)
    }

    pub fn rebuild_rollup(&self) -> Result<()> {
        self.conn.execute("DELETE FROM daily_rollup", [])?;
        self.conn.execute(
            "INSERT INTO daily_rollup
                (host, project_id, date, tok_input, tok_output, tok_cache_read,
                 tok_cache_create, session_count)
             SELECT host, project_id, date(ts, 'localtime') AS d,
                    SUM(tok_input), SUM(tok_output), SUM(tok_cache_read),
                    SUM(tok_cache_create), COUNT(DISTINCT session_id)
             FROM events
             WHERE ts IS NOT NULL
             GROUP BY host, project_id, d",
            [],
        )?;
        Ok(())
    }

    pub fn rollup_for(&self, host: &str, project_id: &str, date: &str) -> Result<Option<RollupRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT tok_input, tok_output, tok_cache_read, tok_cache_create, session_count
                 FROM daily_rollup WHERE host=?1 AND project_id=?2 AND date=?3",
                params![host, project_id, date],
                |r| {
                    Ok(RollupRow {
                        tok_input: r.get::<_, i64>(0)? as u64,
                        tok_output: r.get::<_, i64>(1)? as u64,
                        tok_cache_read: r.get::<_, i64>(2)? as u64,
                        tok_cache_create: r.get::<_, i64>(3)? as u64,
                        session_count: r.get::<_, i64>(4)? as u64,
                    })
                },
            )
            .ok();
        Ok(row)
    }

    pub fn upsert_finding(&self, f: &Finding, now_ts: &str) -> Result<()> {
        let evidence = serde_json::to_string(&f.evidence)?;
        let presc = match &f.prescription {
            Some(p) => Some(serde_json::to_string(p)?),
            None => None,
        };
        // 판정 패스를 거치는 후보는 비노출(pending)로 시작 — R6 패턴, R7 세션 후보.
        // R7 프로젝트 카드는 롤업이 만드는 노출물이므로 'new'.
        let init_status = match (f.rule_id.as_str(), f.scope_kind.as_str()) {
            ("R6", _) => "pending",
            ("R7", "session") => "pending",
            _ => "new",
        };
        self.conn.execute(
            "INSERT INTO findings
                (dedup_key, rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est_tokens_saved, prescription_json, status,
                 first_seen, last_seen, occurrences)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?12,?11,?11,1)
             ON CONFLICT(dedup_key) DO UPDATE SET
                last_seen = ?11,
                occurrences = occurrences + 1,
                est_tokens_saved = ?9,
                evidence_json = ?8,
                severity = ?3,
                prescription_json = ?10",
            params![
                f.dedup_key, f.rule_id, f.severity.as_str(), f.scope_host, f.scope_project,
                f.scope_kind, f.scope_ref, evidence, f.est_tokens_saved as i64, presc, now_ts,
                init_status
            ],
        )?;
        // 재발 감지 (스펙 §5.1) — 「해결함」 뒤 근거 수치가 기준선을 **초과**하면 활성 복귀.
        // 방출 여부를 신호로 쓰면 안 된다: 룰은 관찰창만 보고 status를 안 보므로 매 스캔 같은
        // 묶음을 방출하고, 파이프라인은 60초 디바운스라 처분 1분 뒤 카드가 부활한다.
        // 기준선이 NULL인 행(마이그레이션 이전 처분)은 판정 대상이 아니다.
        if let Some(n) = crate::coach::recurrence_evidence_n(&f.rule_id, &f.evidence) {
            self.conn.execute(
                "UPDATE findings SET status='new', status_evidence_n=NULL, status_ts=NULL
                 WHERE dedup_key=?1 AND status='resolved'
                   AND status_evidence_n IS NOT NULL AND ?2 > status_evidence_n",
                params![f.dedup_key, n],
            )?;
        }
        Ok(())
    }

    pub fn count_findings(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM findings", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn upsert_inventory(
        &self,
        host: &str,
        project_id: &str,
        servers: &[crate::inventory::McpServer],
    ) -> Result<()> {
        for s in servers {
            self.conn.execute(
                "INSERT INTO mcp_inventory (host, project_id, server, source)
                 VALUES (?1,?2,?3,?4)
                 ON CONFLICT(host, project_id, server) DO UPDATE SET source = ?4",
                params![host, project_id, s.name, s.source],
            )?;
        }
        Ok(())
    }

    /// 한 호스트의 인벤토리를 현재 셋으로 원자 교체(트랜잭션: DELETE 후 재INSERT).
    /// entries = (project_id, servers) 목록. "*" 는 host-global 플러그인 스코프.
    /// events(사용 이력)는 건드리지 않는다.
    pub fn replace_host_inventory(
        &mut self,
        host: &str,
        entries: &[(String, Vec<crate::inventory::McpServer>)],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM mcp_inventory WHERE host = ?1", params![host])?;
        for (project, servers) in entries {
            for s in servers {
                tx.execute(
                    "INSERT INTO mcp_inventory (host, project_id, server, source)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(host, project_id, server) DO UPDATE SET source = ?4",
                    params![host, project, s.name, s.source],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 한 호스트의 플러그인 인벤토리를 현재 셋으로 원자 교체(트랜잭션 DELETE 후 재INSERT).
    /// events(사용 이력)는 건드리지 않는다.
    pub fn replace_plugin_inventory(
        &mut self,
        host: &str,
        records: &[crate::inventory::PluginRecord],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM plugin_inventory WHERE host = ?1", params![host])?;
        for r in records {
            let skills_json = serde_json::to_string(&r.skills)?;
            let mcp_json = serde_json::to_string(&r.mcp_servers)?;
            tx.execute(
                "INSERT INTO plugin_inventory
                 (host, plugin_key, namespace, skill_count, resident_tokens, skills_json, mcp_servers_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![host, r.plugin_key, r.namespace, r.skill_count as i64,
                        r.resident_tokens as i64, skills_json, mcp_json],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 개인 스킬 인벤토리 전체 교체 (host 단위) — 코칭 v3 §4.2
    pub fn replace_personal_skills(
        &mut self,
        host: &str,
        skills: &[crate::inventory::PersonalSkill],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM personal_skill_inventory WHERE host=?1", params![host])?;
        for s in skills {
            tx.execute(
                "INSERT OR REPLACE INTO personal_skill_inventory (host, scope, name, path, body_chars)
                 VALUES (?1,?2,?3,?4,?5)",
                params![host, s.scope, s.name, s.path, s.body_chars as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 호스트 설정 스냅숏 (기본 모델·effort) — R7 확장·R13 OutdatedModel 재료 (코칭 v3 §4.2)
    pub fn replace_host_settings(
        &self,
        host: &str,
        default_model: Option<&str>,
        effort_level: Option<&str>,
        now_ts: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO host_settings (host, default_model, effort_level, scanned_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(host) DO UPDATE SET
               default_model=?2, effort_level=?3, scanned_at=?4",
            params![host, default_model, effort_level, now_ts],
        )?;
        Ok(())
    }

    /// host의 distinct 세션 cwd 목록 (NULL 제외) — 프로젝트 스코프 개인 스킬 스캔용
    pub fn session_cwds(&self, host: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT cwd FROM sessions WHERE host=?1 AND cwd IS NOT NULL ORDER BY cwd",
        )?;
        let rows = stmt.query_map(params![host], |r| r.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn upsert_diary_index(
        &self,
        date: &str,
        scope: &str,
        path: &str,
        tokens: u64,
        engine: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO diary_index (date, scope, path, tokens_used, engine)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(date, scope) DO UPDATE SET
                path=?3, tokens_used=?4, engine=?5",
            params![date, scope, path, tokens as i64, engine],
        )?;
        Ok(())
    }

    pub fn active_servers(&self, host: &str, project_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT server FROM mcp_inventory WHERE host=?1 AND project_id=?2 ORDER BY server",
        )?;
        let rows = stmt.query_map(params![host, project_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 주어진 host+날짜의 Finding들 — 그 스코프가 그날 세션을 가졌으면 포함(behavior-date 기준,
    /// last_seen 아님). R5(session)는 세션 날짜, R1(project/host)은 그 스코프가 그날 활성일 때.
    /// 다이어리 브리프 재료: `diary <date>`가 그날 실제 행동의 코칭을 실도록.
    pub fn findings_for_date(&self, host: &str, date: &str) -> Result<Vec<Finding>> {
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key
             FROM findings f
             WHERE f.scope_host = ?1
               AND EXISTS (
                 SELECT 1 FROM sessions s
                 WHERE s.host = ?1 AND date(s.first_ts, 'localtime') = ?2
                   AND ( (f.scope_kind = 'session' AND s.session_id = f.scope_ref)
                      OR (f.scope_kind = 'project' AND s.project_id = f.scope_project)
                      OR (f.scope_kind = 'host') )
               )
             ORDER BY est_tokens_saved DESC",
        )?;
        let rows = stmt.query_map(params![host, date], |r| {
            let sev = match r.get::<_, String>(1)?.as_str() {
                "warn" => Severity::Warn,
                "suggest" => Severity::Suggest,
                _ => Severity::Info,
            };
            let evidence: serde_json::Value =
                serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or(serde_json::Value::Null);
            let presc: Option<Prescription> = r
                .get::<_, Option<String>>(8)?
                .and_then(|s| serde_json::from_str(&s).ok());
            Ok(Finding {
                rule_id: r.get(0)?,
                severity: sev,
                scope_host: r.get(2)?,
                scope_project: r.get(3)?,
                scope_kind: r.get(4)?,
                scope_ref: r.get(5)?,
                evidence,
                est_tokens_saved: r.get::<_, i64>(7)? as u64,
                prescription: presc,
                dedup_key: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// findings_for_date의 전 host 합산 버전 — 다이어리는 주인의 하루(Windows+WSL)라 host를 고정하지 않고
    /// finding의 scope_host가 그날 활동한 host와 일치하면 포함한다.
    /// status='new'만 — 판정 대기·판정 캐시(pending/confirmed/rejected 등 숨김 상태)는 다이어리에
    /// 새는 걸 막는다(스펙 §4 B ⓓ: 세션별 후보 카드는 다이어리에 안 띄움).
    pub fn findings_for_date_all(&self, date: &str) -> Result<Vec<Finding>> {
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key
             FROM findings f
             WHERE f.status = 'new'
               AND EXISTS (
                 SELECT 1 FROM sessions s
                 WHERE date(s.first_ts, 'localtime') = ?1 AND s.host = f.scope_host
                   AND ( (f.scope_kind = 'session' AND s.session_id = f.scope_ref)
                      OR (f.scope_kind = 'project' AND s.project_id = f.scope_project)
                      OR (f.scope_kind = 'host') )
               )
             ORDER BY est_tokens_saved DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            let sev = match r.get::<_, String>(1)?.as_str() {
                "warn" => Severity::Warn,
                "suggest" => Severity::Suggest,
                _ => Severity::Info,
            };
            let evidence: serde_json::Value =
                serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or(serde_json::Value::Null);
            let presc: Option<Prescription> = r
                .get::<_, Option<String>>(8)?
                .and_then(|s| serde_json::from_str(&s).ok());
            Ok(Finding {
                rule_id: r.get(0)?,
                severity: sev,
                scope_host: r.get(2)?,
                scope_project: r.get(3)?,
                scope_kind: r.get(4)?,
                scope_ref: r.get(5)?,
                evidence,
                est_tokens_saved: r.get::<_, i64>(7)? as u64,
                prescription: presc,
                dedup_key: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn summary_for_date(&self, date: &str) -> Result<DaySummary> {
        let row = self.conn.query_row(
            "SELECT COALESCE(SUM(session_count),0), COALESCE(SUM(tok_input),0),
                    COALESCE(SUM(tok_output),0), COALESCE(SUM(tok_cache_read),0),
                    COALESCE(SUM(tok_cache_create),0)
             FROM daily_rollup WHERE date=?1",
            params![date],
            |r| Ok(DaySummary {
                session_count: r.get::<_, i64>(0)? as u64,
                tok_input: r.get::<_, i64>(1)? as u64,
                tok_output: r.get::<_, i64>(2)? as u64,
                tok_cache_read: r.get::<_, i64>(3)? as u64,
                tok_cache_create: r.get::<_, i64>(4)? as u64,
            }),
        )?;
        Ok(row)
    }

    /// 그날(로컬 날짜) **이벤트가 발생한** 세션 목록 — 그날 첫 활동 시각 오름차순.
    ///
    /// 멤버십 정의를 `daily_rollup`과 일치시킨다 — 그쪽도
    /// `COUNT(DISTINCT session_id) … GROUP BY date(ts,'localtime')`이다(`rebuild_rollup`).
    /// `sessions.first_ts`(세션 시작일) 기준으로 뽑으면 두 방향으로 어긋난다:
    /// ① 자정을 넘긴 세션은 요약엔 잡히는데 목록에선 빠지고,
    /// ② `SessionMeta`·`UserPrompt`만 있는 세션은 `events` 행이 없어 요약엔 없는데 목록엔 남는다.
    ///
    /// `first_ts`는 세션 시작이 아니라 **그날 첫 활동 시각**이다(자정 넘긴 세션이 어제 시각으로
    /// 표시되지 않게). 프롬프트는 세션의 정체성이므로 세션 전체의 첫 프롬프트를 그대로 쓴다.
    pub fn sessions_for_date(&self, date: &str) -> Result<Vec<DaySession>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.session_id, s.project_id, s.cwd, MIN(e.ts) AS day_first_ts,
                    s.first_prompt_preview
               FROM sessions s
               JOIN events e ON e.session_id = s.session_id
              WHERE date(e.ts,'localtime')=?1
              GROUP BY s.session_id
              ORDER BY day_first_ts",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            let project_id: String = r.get(1)?;
            let cwd: Option<String> = r.get(2)?;
            Ok(DaySession {
                session_id: r.get(0)?,
                project: cwd
                    .as_deref()
                    .map(crate::hosts::path_basename)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(project_id),
                first_ts: r.get(3)?,
                first_prompt: r.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn total_sessions(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn sum_est_tokens_saved(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(est_tokens_saved),0) FROM findings WHERE status='new'",
            [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn list_findings_current(&self, include_hidden: bool) -> Result<Vec<FindingRow>> {
        let sql = format!(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key,
                    last_seen, occurrences, status, judgment_json, status_ts
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
                r.get::<_, Option<String>>(13)?,
                r.get::<_, Option<String>>(14)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est, prescription_json, dedup_key, last_seen, occ, status,
                 judgment_raw, status_ts) = row?;
            out.push(FindingRow {
                rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
                est_tokens_saved: est as u64,
                prescription: prescription_json.and_then(|s| serde_json::from_str(&s).ok()),
                dedup_key, last_seen,
                occurrences: occ as u64,
                status,
                status_ts,
                judgment: judgment_raw.and_then(|s| serde_json::from_str(&s).ok()),
            });
        }
        Ok(out)
    }

    /// status: 'new' | 'resolved' | 'dismissed' (검증은 커맨드 층). 반환 = 해당 행 존재 여부.
    ///
    /// 처분 수명 기록도 함께 남긴다 (스펙 §5.1/§5.3):
    /// - `resolved` → 그 순간의 룰별 근거 수치를 `status_evidence_n`에 스냅숏(재발 기준선) + 처분 시각
    /// - `dismissed` → 처분 시각만 (영구 침묵이라 기준선이 필요 없다)
    /// - 그 외(`new` = 실행취소, 판정 내부 상태) → 기준선·시각을 모두 지운다
    pub fn set_finding_status(&self, dedup_key: &str, status: &str, now_ts: &str) -> Result<bool> {
        let snapshot = if status == "resolved" {
            self.conn
                .query_row(
                    "SELECT rule_id, evidence_json FROM findings WHERE dedup_key=?1",
                    params![dedup_key],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?
                .and_then(|(rule_id, ev)| {
                    let ev: serde_json::Value = serde_json::from_str(&ev).ok()?;
                    crate::coach::recurrence_evidence_n(&rule_id, &ev)
                })
        } else {
            None
        };
        let ts = matches!(status, "resolved" | "dismissed").then_some(now_ts);
        let n = self.conn.execute(
            "UPDATE findings SET status=?2, status_evidence_n=?3, status_ts=?4 WHERE dedup_key=?1",
            params![dedup_key, status, snapshot, ts],
        )?;
        Ok(n > 0)
    }

    /// R6 판정 배치 대상 — pending & 시도 3회 미만, 최근 활동 순 상한 LIMIT.
    /// attempts는 judgment_json.$.attempts (NULL=0). (스펙 §4.2)
    pub fn pending_r6_for_judgment(&self, limit: usize) -> Result<Vec<JudgmentTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT dedup_key, scope_host,
                    json_extract(evidence_json,'$.repeated_prompt'),
                    COALESCE(json_extract(judgment_json,'$.attempts'), 0)
             FROM findings
             WHERE rule_id='R6' AND status='pending'
               AND COALESCE(json_extract(judgment_json,'$.attempts'), 0) < 3
             ORDER BY last_seen DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(JudgmentTarget {
                dedup_key: r.get(0)?,
                host: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                representative: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                prev_attempts: r.get::<_, i64>(3)? as u32,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// 범용 판정 배치 대상 — 주어진 rule_id의 pending & 시도 3회 미만, 최근 활동 순 상한.
    pub fn pending_for_judgment(&self, rule_id: &str, limit: usize) -> Result<Vec<crate::judge::PendingCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT dedup_key, COALESCE(scope_host,''), scope_project, evidence_json,
                    COALESCE(json_extract(judgment_json,'$.attempts'), 0)
             FROM findings
             WHERE rule_id=?1 AND status='pending'
               AND COALESCE(json_extract(judgment_json,'$.attempts'), 0) < 3
             ORDER BY last_seen DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![rule_id, limit as i64], |r| {
            let ev: String = r.get(3)?;
            Ok(crate::judge::PendingCandidate {
                dedup_key: r.get(0)?,
                scope_host: r.get(1)?,
                scope_project: r.get::<_, Option<String>>(2)?,
                evidence: serde_json::from_str(&ev).unwrap_or(serde_json::Value::Null),
                prev_attempts: r.get::<_, i64>(4)? as u32,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// 판정 결과 저장. new_status=Some → status 전환(worthy→'new', unworthy→'rejected'),
    /// None → status 불변(파싱 실패 시 pending 잔류). judgment_json은 항상 갱신.
    pub fn set_judgment(
        &self,
        dedup_key: &str,
        new_status: Option<&str>,
        judgment: &serde_json::Value,
    ) -> Result<()> {
        let j = serde_json::to_string(judgment)?;
        match new_status {
            Some(s) => self.conn.execute(
                "UPDATE findings SET status=?2, judgment_json=?3 WHERE dedup_key=?1",
                params![dedup_key, s, j],
            )?,
            None => self.conn.execute(
                "UPDATE findings SET judgment_json=?2 WHERE dedup_key=?1",
                params![dedup_key, j],
            )?,
        };
        Ok(())
    }

    /// v2 이행: 특정 룰의 특정 스코프 finding 일괄 삭제 (예: R7 세션 스코프 폐기 — 스펙 §3).
    /// 이번 평가에서 빠진 rule의 활성('new') finding을 내린다 — 순위 변동형 룰(R23)의
    /// host당 상한이 저장소에도 지켜지게. dismissed/resolved는 사용자 기록이라 보존.
    pub fn prune_new_findings_to_current(&self, rule_id: &str, keep: &[String]) -> Result<usize> {
        let mut sql = String::from("DELETE FROM findings WHERE rule_id=? AND status='new'");
        if !keep.is_empty() {
            sql.push_str(&format!(" AND dedup_key NOT IN ({})", vec!["?"; keep.len()].join(",")));
        }
        let params = std::iter::once(rule_id.to_string()).chain(keep.iter().cloned());
        let n = self.conn.execute(&sql, rusqlite::params_from_iter(params))?;
        Ok(n)
    }

    pub fn delete_findings_by_rule_and_scope(&self, rule_id: &str, scope_kind: &str) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM findings WHERE rule_id=?1 AND scope_kind=?2",
            params![rule_id, scope_kind],
        )?;
        Ok(n)
    }

    /// R6 채굴(A) — 이번 스캔에서 방출되지 않은 **활성(new/pending)** R6 패턴 카드를 정리한다.
    /// 느슨한 묶기로 앵커 키가 바뀌거나(별개 norm 병합·더 작은 norm이 앵커 교체) 관찰창 밖으로
    /// 밀려난 카드가 중복·유령으로 남는 것을 막는다(R7/R23 recency-prune 선례). 판정 캐시
    /// (rejected)와 사용자 기록(dismissed/resolved)은 보존한다 — 재판정 금지·사용자 의사 존중.
    pub fn prune_stale_r6_patterns(&self, emitted_keys: &[String]) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT dedup_key FROM findings
             WHERE rule_id='R6' AND scope_kind='pattern' AND status IN ('new','pending')",
        )?;
        let existing: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?;
        let emitted: std::collections::HashSet<&str> =
            emitted_keys.iter().map(|s| s.as_str()).collect();
        let mut removed = 0;
        for key in &existing {
            if !emitted.contains(key.as_str()) {
                self.conn
                    .execute("DELETE FROM findings WHERE dedup_key=?1", [key])?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// 「무시」된 R6 카드들의 `member_norms` 합집합 = 억제 집합 (스펙 §5.4).
    /// R6의 dedup_key는 사전순 최소 변형(앵커)에서 나오므로 묶음이 조금만 달라져도 키가
    /// 바뀐다 — 처분을 키가 아니라 **내용**에 붙여야 무시가 우회되지 않는다.
    /// `resolved`는 뺀다: 해결함 뒤에 또 잡혔다는 건 재발 신호라 떠야 한다.
    pub fn dismissed_r6_member_norms(&self) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT evidence_json FROM findings WHERE rule_id='R6' AND status='dismissed'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            // 관대한 파싱 — member_norms가 없던 시절의 묵은 행은 억제에 기여하지 않고 넘어간다.
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&row?) else { continue };
            if let Some(arr) = v.get("member_norms").and_then(|m| m.as_array()) {
                out.extend(arr.iter().filter_map(|x| x.as_str().map(str::to_string)));
            }
        }
        Ok(out)
    }

    /// 큐레이션 콘텐츠를 현재 랭킹으로 upsert. findings 선례처럼 **사용자 status는 보존**
    /// (dismissed는 재스캔에도 유지 — 나깅 방지). 점수·본문·last_seen만 갱신.
    pub fn replace_content_items(
        &self,
        ranked: &[(crate::content::ContentItem, i64)],
        now_ts: &str,
        skipped_source_tags: &[&str],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (item, score) in ranked {
            let tags = serde_json::to_string(&item.trigger_tags)?;
            let dim = item.dimension.map(|d| d.key());
            self.conn.execute(
                "INSERT INTO content_items
                    (id, kind, dimension, title, body, source_url, trigger_tags, score, status, first_seen, last_seen)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'new',?9,?9)
                 ON CONFLICT(id) DO UPDATE SET
                    score=?8, title=?4, body=?5, source_url=?6, trigger_tags=?7, last_seen=?9",
                params![item.id, item.kind.as_str(), dim, item.title, item.body,
                        item.source_url, tags, score, now_ts],
            )?;
        }
        // 피드에서 사라진 아이템 프룬(2026-07-19): 소식·외부 팁은 일시적 — 랭킹에 없으면
        // 낡은 점수로 상단을 점령한다. 단, 사용자가 「무시」한(dismissed) 행은 쿨다운 기록이라 보존.
        // 「해결함」(resolved)은 함께 프룬한다 — 개인 레슨의 수명은 **방출 기반**이라(스펙 §5.2)
        // 미방출이 곧 "고쳐졌다"는 신호다. 나중에 다시 방출되면 새 `new` 카드로 돌아온다.
        // 프룬 범위는 **이번에 참여한 소스로 한정**한다(2026-08-02, 스펙 §7.1): TTL로 fetch를
        // 스킵한 소스는 이번 랭킹에 항목을 하나도 못 싣기 때문에, 함께 지우면 스킵할 때마다 그
        // 소스의 카드가 통째로 사라졌다가 TTL 만료 후 되살아난다.
        if !ranked.is_empty() {
            let keep: std::collections::HashSet<&str> =
                ranked.iter().map(|(i, _)| i.id.as_str()).collect();
            let stale: Vec<String> = {
                let mut stmt = self
                    .conn
                    .prepare(
                        "SELECT id, trigger_tags FROM content_items
                         WHERE status IN ('new','resolved')",
                    )?;
                let rows = stmt.query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
                    .into_iter()
                    .filter(|(id, tags_json)| {
                        if keep.contains(id.as_str()) {
                            return false;
                        }
                        let tags: Vec<String> =
                            serde_json::from_str(tags_json).unwrap_or_default();
                        !tags.iter().any(|t| skipped_source_tags.contains(&t.as_str()))
                    })
                    .map(|(id, _)| id)
                    .collect()
            };
            for id in stale {
                self.conn.execute("DELETE FROM content_items WHERE id=?1", [&id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn content_row_from(r: &rusqlite::Row) -> rusqlite::Result<ContentRow> {
        let tags_json: String = r.get(6)?;
        Ok(ContentRow {
            id: r.get(0)?, kind: r.get(1)?, dimension: r.get(2)?,
            title: r.get(3)?, body: r.get(4)?, source_url: r.get(5)?,
            trigger_tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            score: r.get(7)?, status: r.get(8)?, status_ts: r.get(9)?,
            summary_ko: r.get(10)?, title_ko: r.get(11)?, deadline: r.get(12)?,
            personal: None, // list_content에서 enrich_personal로 채움
        })
    }

    /// `content_row_from`이 기대하는 컬럼 순서 — SELECT를 한 군데서만 정의한다.
    const CONTENT_COLS: &'static str =
        "id,kind,dimension,title,body,source_url,trigger_tags,score,status,status_ts,summary_ko,title_ko,deadline";

    /// 아직 번역되지 않은 외국어 소식 (스펙 §6.3). **한국어 소스는 애초에 담기지 않는다** —
    /// 이 목록이 비면 호출부가 Engine을 한 번도 부르지 않는다(= 아이템당 1회 보장).
    pub fn content_needing_translation(&self, limit: usize) -> Result<Vec<ContentRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM content_items
             WHERE status='new' AND summary_ko IS NULL
             ORDER BY score DESC, id",
            Self::CONTENT_COLS
        ))?;
        let rows = stmt.query_map([], Self::content_row_from)?;
        Ok(rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|r| crate::content::translate_kind_for(&r.trigger_tags).is_some())
            .take(limit)
            .collect())
    }

    /// 번역 결과 캐시. `title_ko`·`deadline`은 없을 수 있다(선택 필드·기본 폐쇄).
    /// 반환 = 해당 행 존재 여부.
    pub fn set_content_translation(
        &self,
        id: &str,
        title_ko: Option<&str>,
        summary_ko: &str,
        deadline: Option<&str>,
    ) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE content_items SET summary_ko=?2, title_ko=?3, deadline=?4 WHERE id=?1",
            params![id, summary_ko, title_ko, deadline],
        )?;
        Ok(n > 0)
    }

    /// 아직 알리지 않은 공지 id만 돌려주고 **같은 호출에서 통지 기록에 넣는다**(§6.5 "한 번만").
    /// 기록은 무한히 자라지 않게 최근 것부터 상한을 둔다 — 잘려나간 옛 id가 다시 알림을
    /// 받으려면 그 공지가 `claude.json`에 아직 남아 있어야 하므로 실질 재알림은 없다.
    pub fn take_unnotified_announcements(&self, ids: &[String]) -> Result<Vec<String>> {
        const KEEP: usize = 50;
        let raw = self.get_setting("announcement_notified_ids")?.unwrap_or_default();
        let mut known: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        let fresh: Vec<String> =
            ids.iter().filter(|id| !known.contains(id)).cloned().collect();
        if fresh.is_empty() {
            return Ok(fresh);
        }
        known.extend(fresh.iter().cloned());
        if known.len() > KEEP {
            known.drain(..known.len() - KEEP);
        }
        self.set_setting("announcement_notified_ids", &serde_json::to_string(&known)?)?;
        Ok(fresh)
    }

    /// 노출용 콘텐츠 목록. include_hidden=false면 status='new' + score≥0만,
    /// 그리고 **태그(축) 쿨다운**: 같은 dimension의 dismissed 형제가 cooldown_days 이내면 억제
    /// (팁 한 번 닫으면 그 축이 잠시 조용해짐 — 킥오프 §How 나깅 방지). 점수 내림차순.
    pub fn list_content(
        &self,
        now_ts: &str,
        cooldown_days: f64,
        include_hidden: bool,
    ) -> Result<Vec<ContentRow>> {
        if include_hidden {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {} FROM content_items ORDER BY score DESC, id",
                Self::CONTENT_COLS
            ))?;
            let rows = stmt.query_map([], Self::content_row_from)?;
            let out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            return Ok(self.enrich_personal(out));
        }
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM content_items c
             WHERE status='new' AND score >= 0
               AND NOT EXISTS (
                 SELECT 1 FROM content_items d
                 WHERE d.status='dismissed' AND d.dimension IS NOT NULL
                   AND d.dimension IS c.dimension
                   AND julianday(?1) - julianday(d.last_seen) < ?2
               )
             ORDER BY score DESC, id",
            Self::CONTENT_COLS
        ))?;
        let rows = stmt.query_map(params![now_ts, cooldown_days], Self::content_row_from)?;
        let out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(self.enrich_personal(out))
    }

    /// 노출 목록에 "당신 로그: …" 근거 줄을 채운다(신호 없으면 None 유지).
    fn enrich_personal(&self, mut rows: Vec<ContentRow>) -> Vec<ContentRow> {
        for r in &mut rows {
            r.personal = self.tip_personal_evidence(r);
        }
        rows
    }

    /// 팁을 사용자 실측 데이터로 접지한다 — findings/이벤트 수치로 "당신 로그: …" 한 줄 생성.
    /// 신호가 없으면 None(그 팁은 일반론만 표시). 커리큘럼=지도, 내 로그=GPS.
    fn tip_personal_evidence(&self, row: &ContentRow) -> Option<String> {
        let has = |t: &str| row.trigger_tags.iter().any(|x| x == t);
        let cnt = |sql: &str| -> u64 {
            self.conn
                .query_row(sql, [], |r| r.get::<_, i64>(0))
                .map(|n| n.max(0) as u64)
                .unwrap_or(0)
        };
        let active = || self.list_findings_current(false).unwrap_or_default();

        if has("mcp") {
            if let Some(f) = active().into_iter().find(|f| f.rule_id == "R1") {
                let server = f.evidence.get("server").and_then(|v| v.as_str()).unwrap_or("일부");
                let resident =
                    f.evidence.get("resident_tokens_total").and_then(|v| v.as_u64()).unwrap_or(0);
                return Some(format!(
                    "당신 로그: 미사용 MCP `{server}`가 상주 ~{}K토큰을 잡고 있어요 · 정리하면 세션당 ~{}토큰↓",
                    resident / 1000,
                    f.est_tokens_saved
                ));
            }
        }
        if has("model") {
            let opus = cnt(
                "SELECT COUNT(*) FROM events WHERE kind='assistant_turn' AND model_family='opus'",
            );
            let cheaper = cnt(
                "SELECT COUNT(*) FROM events WHERE kind='assistant_turn' AND model_family IN ('sonnet','haiku')",
            );
            if opus + cheaper > 0 {
                let pct = cheaper * 100 / (opus + cheaper);
                return Some(format!(
                    "당신 로그: 상위 모델 {opus}턴 vs 하위 모델 {cheaper}턴 (하위 {pct}%)"
                ));
            }
        }
        if has("skill") {
            let n = cnt(
                "SELECT COUNT(*) FROM events WHERE kind='tool_call' AND tool_kind='skill'",
            );
            return Some(if n > 0 {
                format!("당신 로그: 스킬을 {n}회 쓰고 있어요 — 반복 작업을 더 스킬로 옮겨보세요")
            } else {
                "당신 로그: 아직 스킬 호출 0회 — 반복되는 지시를 스킬로 만들어보세요".into()
            });
        }
        if has("subagent") {
            let n = cnt(
                "SELECT COUNT(*) FROM events WHERE is_sidechain=1 OR (kind='tool_call' AND tool_kind='sub_agent')",
            );
            return Some(if n > 0 {
                format!("당신 로그: 서브에이전트 {n}건 사용 중")
            } else {
                "당신 로그: 서브에이전트 사용 없음 — 긴 조사·구현을 위임하면 컨텍스트가 깨끗해져요".into()
            });
        }
        if has("hooks") || has("permission") {
            if let Some(f) = active().into_iter().find(|f| f.rule_id == "R11") {
                let friction = f
                    .evidence
                    .get("friction_events")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if friction > 0 {
                    return Some(format!(
                        "당신 로그: 권한 승인 마찰 {friction}건 감지 — 사전 허용/자동화로 줄일 수 있어요"
                    ));
                }
            }
        }
        None
    }

    /// status: 'new' | 'resolved' | 'dismissed' (검증은 커맨드 층). dismiss 시 last_seen 갱신해
    /// 쿨다운 기준 시각으로 삼는다. 반환 = 해당 행 존재 여부.
    ///
    /// `status_ts`는 「해결함」 7일 창의 기준 시각이다 (스펙 §5). `last_seen`으로 대신할 수 없다 —
    /// 재방출마다 `replace_content_items`가 갱신해 창이 영영 만료되지 않는다.
    /// 개인 레슨은 방출 기반이라 근거 수치 스냅숏(`status_evidence_n`)은 두지 않는다(§5.2).
    pub fn set_content_status(&self, id: &str, status: &str, now_ts: &str) -> Result<bool> {
        let status_ts = matches!(status, "resolved" | "dismissed").then_some(now_ts);
        let n = self.conn.execute(
            "UPDATE content_items SET status=?2, last_seen=?3, status_ts=?4 WHERE id=?1",
            params![id, status, now_ts, status_ts],
        )?;
        Ok(n > 0)
    }

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

    /// E — 설치 전수 스냅숏 저장(settings.json enabledPlugins 맵 그대로, disabled=false 포함).
    /// enabled-only인 plugin_inventory로는 부재를 추론할 수 없어(disabled·스캔 실패가 부재로
    /// 보임 — Codex 리뷰) ②(미설치 추천)의 부재 확인 게이트로 쓴다. 키 존재 = "스캔됨" 마커.
    pub fn set_installed_plugins(&self, host: &str, map: &serde_json::Value) -> Result<()> {
        self.set_setting(&format!("installed_plugins:{host}"), &map.to_string())
    }

    /// E — 설치 전수 스냅숏 조회. None = 이 호스트는 아직 스캔 안 됨(② 억제 신호).
    pub fn installed_plugins_map(&self, host: &str) -> Result<Option<serde_json::Value>> {
        Ok(self
            .get_setting(&format!("installed_plugins:{host}"))?
            .and_then(|s| serde_json::from_str(&s).ok()))
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
    /// sidechain 포함 — 서브에이전트 안에서만 쓴 plugin도 사용 중이다. 억제(침묵) 방향이라
    /// §2 계약(발화 근거 = main-chain)과 무관하고, R2 감지기도 sidechain을 셌다 (Codex 리뷰).
    /// 스킬 접두는 LIKE 대신 substr 비교 — plugin명의 `_`가 와일드카드로 오작동 방지(R2 선례).
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
        let skill_prefix = format!("{plugin}:");
        let mcp_prefix = format!("plugin_{plugin}_");
        let used: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events
             WHERE host=?1 AND kind='tool_call' AND ts >= ?2
               AND ( (tool_kind='skill' AND substr(tool_target, 1, length(?3)) = ?3)
                  OR (tool_kind='mcp_call' AND (
                        substr(tool_server, 1, length(?4)) = ?4
                     OR tool_server IN (SELECT value FROM json_each(?5)) )) )",
            params![host, window_start, skill_prefix, mcp_prefix, servers_json],
            |r| r.get(0),
        )?;
        Ok(used > 0)
    }

    /// 특정 하루의 모델 분포 — model_mix_for_range의 단일일 특수형.
    pub fn model_mix_for_date(&self, date: &str) -> Result<Vec<(String, u64)>> {
        self.model_mix_for_range(Some(date), date)
    }

    /// 기간 내 모델별(raw id 기준, 구 데이터는 family 폴백) 토큰(입력+출력) 합. 내림차순.
    /// from=None이면 하한 없음(전체). 로컬 날짜 버킷(date(ts,'localtime')), 양끝 포함.
    /// 0토큰 그룹 제외 — Claude Code 합성 메시지(model="<synthetic>", usage 전부 0) 등은 모델 사용이 아님.
    pub fn model_mix_for_range(&self, from: Option<&str>, to: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model_raw, model_family) AS m,
                    COALESCE(SUM(tok_input),0) + COALESCE(SUM(tok_output),0) AS toks
             FROM events
             WHERE date(ts, 'localtime') <= ?2
               AND (?1 IS NULL OR date(ts, 'localtime') >= ?1)
               AND COALESCE(model_raw, model_family) IS NOT NULL
             GROUP BY m HAVING toks > 0 ORDER BY toks DESC",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 세션 컨텍스트 — 세션 스코프 finding·집계 카드의 "어떤 작업인지" 표시용.
    /// (project_id, first_ts, cwd, first_prompt_preview)
    pub fn session_ctx(
        &self,
        session_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<String>, Option<String>)>> {
        self.conn
            .query_row(
                "SELECT project_id, first_ts, cwd, first_prompt_preview
                 FROM sessions WHERE session_id=?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn finding_severities(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT dedup_key, severity FROM findings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// **활성('new')** finding만의 key→severity — 스캔 전후로 비교해 "새로 뜬 카드"를 가린다.
    /// 전체가 아니라 활성만 보는 이유: 재발로 `resolved`→`new`가 된 카드는 키도 severity도
    /// 그대로라 전체 스냅숏에서는 아무 변화가 없다(스펙 §5.1의 복귀가 알림 없이 묻힌다).
    /// 노출 목록(`list_findings_current(false)`)과 같은 필터라 emit 대상과도 어긋나지 않는다.
    pub fn active_finding_severities(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT dedup_key, severity FROM findings WHERE status='new'")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn diary_dates(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT date FROM diary_index ORDER BY date")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn diary_path_for(&self, date: &str) -> Result<Option<String>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT path FROM diary_index WHERE date=?1 LIMIT 1", params![date], |r| r.get(0))
            .optional()?;
        Ok(v)
    }

    /// diary_path_for의 host(scope) 인지 버전. diary_index PK가 (date, scope)이므로
    /// 다중 host DB에서 date-only 조회는 다른 host의 일기를 집을 수 있다 — 브리프는 host 스코프라
    /// 최근 일기 참조도 같은 host로 좁힌다.
    pub fn diary_path_for_scope(&self, date: &str, scope: &str) -> Result<Option<String>> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM diary_index WHERE date=?1 AND scope=?2",
                params![date, scope],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    /// date 이전 마지막 활동(전 host, 세션>0)일로부터 며칠 지났는지. 활동 이력 없으면 None.
    /// 무활동일 일기가 "며칠째 조용한지"로 변화를 주는 데 쓴다(다이어리는 주인의 하루라 host 무관).
    pub fn days_since_last_active(&self, date: &str) -> Result<Option<i64>> {
        let last: Option<String> = self.conn.query_row(
            "SELECT MAX(date) FROM daily_rollup WHERE date<?1 AND session_count>0",
            params![date],
            |r| r.get::<_, Option<String>>(0),
        )?;
        let Some(last) = last else { return Ok(None) };
        let (Ok(d), Ok(l)) = (
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d"),
            chrono::NaiveDate::parse_from_str(&last, "%Y-%m-%d"),
        ) else {
            return Ok(None);
        };
        Ok(Some((d - l).num_days()))
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
            .optional()?;
        Ok(v)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        )?;
        Ok(())
    }

    // ── 주인 메모리 (memory.rs — 2026-07-22-owner-memory 스펙) ──

    pub fn add_memory(&self, text: &str, source: &str) -> Result<i64> {
        let text = text.trim();
        if text.is_empty() {
            anyhow::bail!("메모리 텍스트가 비어 있습니다");
        }
        let now = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.conn.execute(
            "INSERT INTO memories (text, created_at, source) VALUES (?1, ?2, ?3)",
            params![text, now, source],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_memories(&self) -> Result<Vec<crate::memory::Memory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, created_at, updated_at, source FROM memories
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::memory::Memory {
                id: r.get(0)?,
                text: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                source: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_memory(&self, id: i64, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            anyhow::bail!("메모리 텍스트가 비어 있습니다");
        }
        let now = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.conn.execute(
            "UPDATE memories SET text=?1, updated_at=?2 WHERE id=?3",
            params![text, now, id],
        )?;
        Ok(())
    }

    pub fn delete_memory(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM memories WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn count_memories(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    // ── a-hub 지식 공유 상태 (hub.rs — 스펙 2026-07-18-hub-knowledge-sharing §7) ──

    /// 이슈를 열었거나 발행까지 끝난 dedup_key 전부 — "다시 열지 않을" 집합.
    pub fn hub_shared_or_pending_keys(&self) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT dedup_key FROM hub_share_state")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = std::collections::HashSet::new();
        for k in rows {
            out.insert(k?);
        }
        Ok(out)
    }

    /// open은 됐는데 resolve(발행)가 안 된 것 — 다음 스캔이 재개한다 (중복 이슈 방지).
    pub fn hub_share_pending(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT dedup_key, issue_id FROM hub_share_state
             WHERE issue_id IS NOT NULL AND page_id IS NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn hub_mark_issue(&self, dedup_key: &str, issue_id: &str, now_ts: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO hub_share_state (dedup_key, issue_id, shared_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(dedup_key) DO UPDATE SET issue_id=?2, shared_at=?3",
            params![dedup_key, issue_id, now_ts],
        )?;
        Ok(())
    }

    pub fn hub_mark_published(&self, dedup_key: &str, page_id: &str, now_ts: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO hub_share_state (dedup_key, page_id, shared_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(dedup_key) DO UPDATE SET page_id=?2, shared_at=?3",
            params![dedup_key, page_id, now_ts],
        )?;
        Ok(())
    }

    /// 내가 허브에 발행한 페이지 id 집합 — 인정 루프가 "남이 인용한 게 내 것인지" 대조한다.
    /// `page_id`가 NULL인 행(이슈만 열고 아직 발행 전)은 제외한다.
    pub fn hub_published_page_ids(&self) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT page_id FROM hub_share_state WHERE page_id IS NOT NULL AND page_id <> ''")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// 콘텐츠 아이템 존재 여부 (레슨 칭찬 루프 — "이 레슨을 보여준 적 있나").
    pub fn content_item_exists(&self, id: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM content_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// 특정 로컬 날짜의 도구 오류(error+denied) 수 — 레슨 칭찬 루프용.
    pub fn errors_on_local_date(&self, date: &str) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events
             WHERE date(ts,'localtime')=?1 AND result_status IN ('error','denied')",
            params![date],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// 날짜 구간 합계 (주간 리포트용) — (세션수, 입력, 출력, 캐시읽기).
    pub fn range_totals(&self, from: &str, to: &str) -> Result<(u64, u64, u64, u64)> {
        self.conn
            .query_row(
                "SELECT COALESCE(SUM(session_count),0), COALESCE(SUM(tok_input),0),
                        COALESCE(SUM(tok_output),0), COALESCE(SUM(tok_cache_read),0)
                 FROM daily_rollup WHERE date >= ?1 AND date <= ?2",
                params![from, to],
                |r| Ok((
                    r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64, r.get::<_, i64>(3)? as u64,
                )),
            )
            .map_err(Into::into)
    }

    /// 텔레메트리(#46 목표 아키텍처): 특정 로컬 날짜의 MCP 서버별 호출 수 — 파생 카운트만.
    pub fn mcp_call_counts_for_date(&self, date: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool_server, COUNT(*) AS n FROM events
             WHERE date(ts, 'localtime') = ?1 AND tool_server IS NOT NULL
             GROUP BY tool_server ORDER BY n DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// R8 대형 MCP 결과 — 서버별로 큰(≥threshold자) tool_result를 집계.
    /// tool_result(r).result_len ↔ 같은 세션 tool_call(c).tool_server 조인. since 이후만.
    /// 반환: (server, 큰_결과_횟수, 총_문자수, 최대_문자수), 총합 내림차순.
    pub fn mcp_large_results(
        &self,
        threshold: u64,
        min_calls: u64,
        since_rfc3339: &str,
    ) -> Result<Vec<(String, u64, u64, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.tool_server AS srv, COUNT(*) AS n,
                    SUM(r.result_len) AS total, MAX(r.result_len) AS mx
             FROM events r
             JOIN events c ON c.tool_use_id = r.tool_use_id AND c.kind='tool_call'
               AND c.session_id = r.session_id
             WHERE r.kind='tool_result' AND c.tool_server IS NOT NULL
               AND r.result_len >= ?1 AND r.ts >= ?3
             GROUP BY c.tool_server
             HAVING n >= ?2
             ORDER BY total DESC",
        )?;
        let rows = stmt.query_map(
            params![threshold as i64, min_calls as i64, since_rfc3339],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64,
                    r.get::<_, i64>(3)? as u64,
                ))
            },
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 텔레메트리: 코칭 findings 상태별 개수 (채택·해결 흐름의 파생 신호).
    pub fn findings_status_counts(&self) -> Result<Vec<(String, u64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT status, COUNT(*) FROM findings GROUP BY status")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 세션 회고(스펙 2026-07-19): "고생 끝 해결" 후보 세션.
    /// 조건: 종료(last_ts < settled_before) ∧ 오류(error+denied) ≥ min_errors ∧ 규모 ≥ min_events.
    /// 회복 여부(last_result_ok)는 호출자가 필터 — 실패로 끝난 세션은 지식이 아니라 백로그감.
    /// "고생 끝 해결" 후보 세션.
    ///
    /// 종료 판정은 **둘 중 하나**만 만족하면 된다 (2026-07-30 개정):
    /// - `settled_before`: 마지막 활동이 이보다 오래됐다 = 세션이 조용해졌다 (원안).
    /// - `interval_closed_before`: 세션이 이보다 먼저 **시작**됐다 = 닫힌 날짜 구간이 있다.
    ///
    /// 후자를 더한 이유: 세션을 끄지 않고 며칠씩 이어 쓰는 사용자는 `last_ts`가 계속 갱신돼
    /// 원안만으로는 **영구히 후보가 되지 않았다**(실측 최장 232시간 세션).
    ///
    /// 구간 기준을 끄려면 `interval_closed_before`에 [`INTERVAL_CRITERION_OFF`]를 준다.
    /// 두 값을 같게 주는 것으로는 안 꺼진다 — `first_ts <= last_ts`이므로 조건이 오히려 넓어진다.
    pub fn struggle_sessions(
        &self,
        min_errors: u64,
        min_events: u64,
        settled_before: &str,
        interval_closed_before: &str,
    ) -> Result<Vec<StruggleSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.session_id, s.host, s.project_id, s.first_prompt_preview, s.first_ts, s.last_ts,
                    -- cwd 없는 옛 세션 보완: 같은 (host, project_id)를 cwd와 함께 기록한 세션에서 빌려온다
                    -- (일기 collect_work_log와 같은 규율).
                    COALESCE(s.cwd, (SELECT o.cwd FROM sessions o
                                      WHERE o.host=s.host AND o.project_id=s.project_id
                                        AND o.cwd IS NOT NULL LIMIT 1)) AS cwd,
                    (SELECT COUNT(*) FROM events e WHERE e.session_id=s.session_id
                       AND e.result_status IN ('error','denied')) AS errs,
                    (SELECT COUNT(*) FROM events e WHERE e.session_id=s.session_id) AS total,
                    (SELECT e.result_status FROM events e WHERE e.session_id=s.session_id
                       AND e.kind='tool_result' ORDER BY e.id DESC LIMIT 1) AS last_status
             FROM sessions s
             WHERE s.last_ts IS NOT NULL
               AND (s.last_ts < ?3 OR (s.first_ts IS NOT NULL AND s.first_ts < ?4))
               AND (SELECT COUNT(*) FROM events e WHERE e.session_id=s.session_id
                      AND e.result_status IN ('error','denied')) >= ?1
               AND (SELECT COUNT(*) FROM events e WHERE e.session_id=s.session_id) >= ?2
             ORDER BY s.last_ts DESC",
        )?;
        let rows = stmt.query_map(
            params![min_errors as i64, min_events as i64, settled_before, interval_closed_before],
            |r| {
                Ok((
                    r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, i64>(7)?, r.get::<_, i64>(8)?, r.get::<_, Option<String>>(9)?,
                ))
            },
        )?;
        let mut out = Vec::new();
        for row in rows {
            let (session_id, host, project_id, preview, first_ts, last_ts, cwd, errs, total, last_status) =
                row?;
            // 오류가 난 도구들 (call↔result 조인, 결정론 식별)
            let mut tstmt = self.conn.prepare(
                "SELECT COALESCE(c.raw_name, c.tool_kind, '?') AS t, COUNT(*) AS n
                 FROM events r
                 LEFT JOIN events c ON c.tool_use_id = r.tool_use_id AND c.kind='tool_call'
                   AND c.session_id = r.session_id
                 WHERE r.session_id=?1 AND r.kind='tool_result'
                   AND r.result_status IN ('error','denied')
                 GROUP BY t ORDER BY n DESC",
            )?;
            let tools = tstmt
                .query_map(params![session_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            out.push(StruggleSession {
                session_id,
                host: host.unwrap_or_default(),
                project_id: project_id.unwrap_or_default(),
                cwd,
                first_prompt_preview: preview,
                first_ts,
                last_ts,
                error_count: errs as u64,
                total_events: total as u64,
                error_tools: tools,
                last_result_ok: last_status.as_deref() == Some("ok"),
            });
        }
        Ok(out)
    }

    /// 한 세션의 사용자 프롬프트(최근 N개, 시간순). prompt_events는 이미 sidechain·meta·도구결과 제외.
    /// DESC로 최근 N개를 뽑은 뒤 reverse해 LLM 심사관에게는 시간 순서대로 보이게 한다.
    pub fn session_user_prompts(&self, session_id: &str, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT preview FROM prompt_events WHERE session_id=?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], |r| r.get::<_, String>(0))?;
        let mut out: Vec<String> = rows.collect::<std::result::Result<_, _>>()?;
        out.retain(|p| !p.trim().is_empty());
        out.reverse();
        Ok(out)
    }

    /// 하니스/스케줄러가 주입한 프롬프트 행을 지운다. 반환 = 지운 행 수.
    ///
    /// 수집 시점 필터(`r6_repeated_prompts::normalize`)는 **신규 수집분만** 막으므로,
    /// 필터 도입 전에 쌓인 행은 계속 R6 오탐 카드를 만든다. 매 스캔 idempotent하게 정리한다.
    /// 판정 기준은 `normalize`의 마커와 같아야 한다 — 양쪽이 어긋나면 카드가 되살아난다.
    pub fn purge_harness_injected_prompts(&self) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM prompt_events
             WHERE preview LIKE '%<scheduled-task%'
                OR preview LIKE '%<command-name>%'
                OR preview LIKE '%<local-command-stdout>%'
                OR preview LIKE '%<local-command-caveat>%'",
            [],
        )?;
        Ok(n)
    }

    /// R6 채굴(A) — 관찰창 내 (host, norm60, session)별 등장수 + 대표 preview.
    /// 미더가 Rust에서 norm 단위 집계 + 느슨한 군집화에 쓴다. ts NULL 행은 제외(기존 R6 SQL 동치).
    pub fn prompt_occurrence_rows(&self, cutoff: &str) -> Result<Vec<PromptOccRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT host, norm60, session_id, COUNT(*), MIN(preview)
             FROM prompt_events WHERE ts >= ?1
             GROUP BY host, norm60, session_id
             ORDER BY host, norm60, session_id",
        )?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok(PromptOccRow {
                host: r.get(0)?,
                norm60: r.get(1)?,
                session_id: r.get(2)?,
                occurrences: r.get::<_, i64>(3)? as u64,
                preview: r.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// R6 스킬 초안용(A) — 여러 정규화 지시(norm60)에 매칭되는 (session_id, preview) 전량.
    /// SQLite 변수 한도 대비 990개씩 청크. 세션·원문 중복 제거는 호출부(skill_draft) 책임.
    pub fn prompt_sessions_for_norms(
        &self,
        host: &str,
        norms: &[String],
    ) -> Result<Vec<(String, String)>> {
        if norms.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for chunk in norms.chunks(990) {
            let placeholders = std::iter::repeat("?").take(chunk.len()).collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT session_id, preview FROM prompt_events
                 WHERE host = ? AND norm60 IN ({placeholders}) ORDER BY id",
            );
            let mut binds: Vec<String> = Vec::with_capacity(chunk.len() + 1);
            binds.push(host.to_string());
            binds.extend(chunk.iter().cloned());
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    /// R6 스킬 초안용 — 주어진 세션들에서 실제로 쓴 도구(raw_name) 상위 집계.
    /// 반복 워크플로가 어떤 도구 시퀀스인지 = 초안 본문의 재료.
    pub fn tool_usage_for_sessions(&self, session_ids: &[String]) -> Result<Vec<(String, u64)>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        // SQLite 바인딩 변수 한도(SQLITE_LIMIT_VARIABLE_NUMBER, 기본 999) 초과 방지 —
        // 세션이 많아도 크래시하지 않도록 990개씩 청크로 조회하고 Rust에서 합산·상위 12개.
        use std::collections::HashMap;
        let mut merged: HashMap<String, u64> = HashMap::new();
        for chunk in session_ids.chunks(990) {
            let placeholders = std::iter::repeat("?").take(chunk.len()).collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT COALESCE(raw_name, tool_kind, '?') AS t, COUNT(*) AS n
                 FROM events
                 WHERE kind='tool_call' AND session_id IN ({placeholders})
                 GROUP BY t",
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter());
            let rows = stmt.query_map(params, |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
            })?;
            for row in rows {
                let (t, n) = row?;
                *merged.entry(t).or_insert(0) += n;
            }
        }
        let mut out: Vec<(String, u64)> = merged.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))); // 동점은 이름순(결정론)
        out.truncate(12);
        Ok(out)
    }

    /// hub_share_state에서 특정 접두사 키가 특정 날짜(shared_at 접두사)에 몇 건인지 — 일일 상한용.
    pub fn hub_share_count_on(&self, key_prefix: &str, date_prefix: &str) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM hub_share_state
             WHERE dedup_key LIKE ?1 || '%' AND shared_at LIKE ?2 || '%'",
            params![key_prefix, date_prefix],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// dedup_key로 단건 조회 (hub 재개 경로용).
    pub fn find_finding(&self, dedup_key: &str) -> Result<Option<FindingRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                        evidence_json, est_tokens_saved, prescription_json, dedup_key,
                        last_seen, occurrences, status, judgment_json, status_ts
                 FROM findings WHERE dedup_key=?1",
                params![dedup_key],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?, r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?, r.get::<_, i64>(7)?,
                        r.get::<_, Option<String>>(8)?, r.get::<_, String>(9)?,
                        r.get::<_, Option<String>>(10)?, r.get::<_, i64>(11)?,
                        r.get::<_, String>(12)?,
                        r.get::<_, Option<String>>(13)?,
                        r.get::<_, Option<String>>(14)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(
            |(rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
              evidence_json, est, prescription_json, dedup_key, last_seen, occ, status,
              judgment_raw, status_ts)| {
                FindingRow {
                    rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
                    est_tokens_saved: est as u64,
                    prescription: prescription_json.and_then(|s| serde_json::from_str(&s).ok()),
                    dedup_key, last_seen,
                    occurrences: occ as u64,
                    status,
                    status_ts,
                    judgment: judgment_raw.and_then(|s| serde_json::from_str(&s).ok()),
                }
            },
        ))
    }

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 방문 1행 기록. 같은 (life_id, visited_at) 재기록은 덮어쓴다(재시도 멱등).
    pub fn record_life_visit(&self, v: &LifeVisit) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT OR REPLACE INTO life_visits ({LIFE_VISIT_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7)"
            ),
            params![
                v.life_id,
                v.visited_at,
                v.kind,
                v.owner_name,
                v.design_json,
                v.diary_excerpt_json,
                v.signed as i64
            ],
        )?;
        Ok(())
    }

    /// 그 로컬 날짜의 방문 목록(오래된 순) — 일기 조립·자율 방문 중복 판정.
    pub fn life_visits_for_date(&self, date: &str) -> Result<Vec<LifeVisit>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {LIFE_VISIT_COLS} FROM life_visits
             WHERE date(visited_at,'localtime')=?1 ORDER BY visited_at"
        ))?;
        let rows = stmt.query_map(params![date], row_to_life_visit)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// 그 방문보다 이전, 같은 방의 가장 최근 행 — 인테리어 변화 감지 기준(스펙 §5).
    pub fn prev_life_visit(&self, life_id: &str, before: &str) -> Result<Option<LifeVisit>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {LIFE_VISIT_COLS} FROM life_visits
                     WHERE life_id=?1 AND visited_at < ?2 ORDER BY visited_at DESC LIMIT 1"
                ),
                params![life_id, before],
                row_to_life_visit,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 방별 마지막 방문 시각 — 자율 방문 대상 선정(가장 오래 안 간 방).
    pub fn last_visit_times(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT life_id, MAX(visited_at) FROM life_visits GROUP BY life_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// 보존 기간 초과 방문 삭제 — 삭제된 행 수를 돌려준다.
    pub fn prune_life_visits(&self, before: &str) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM life_visits WHERE visited_at < ?1", params![before])?)
    }

    pub fn get_daily_line(&self, date: &str) -> Result<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT text, fingerprint FROM daily_line WHERE date=?1",
                params![date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_daily_line(&self, date: &str, text: &str, fingerprint: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO daily_line (date, text, fingerprint) VALUES (?1,?2,?3)
             ON CONFLICT(date) DO UPDATE SET text=?2, fingerprint=?3",
            params![date, text, fingerprint],
        )?;
        Ok(())
    }

    pub fn get_chatter_pool(&self, date: &str) -> Result<Option<(Vec<String>, String)>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT lines, fingerprint FROM chatter_pool WHERE date=?1",
                params![date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((lines_json, fp)) => {
                // 손상된 JSON은 하드 페일 대신 빈 풀 폴백 (evidence/prescription 읽기 관례).
                // fp는 온전히 반환 — 사실이 바뀌면 다음 스캔이 손상 행을 덮어쓴다.
                let lines = serde_json::from_str(&lines_json).unwrap_or_default();
                Ok(Some((lines, fp)))
            }
            None => Ok(None),
        }
    }

    pub fn upsert_chatter_pool(&self, date: &str, lines: &[String], fingerprint: &str) -> Result<()> {
        let lines_json = serde_json::to_string(lines)?;
        self.conn.execute(
            "INSERT INTO chatter_pool (date, lines, fingerprint) VALUES (?1,?2,?3)
             ON CONFLICT(date) DO UPDATE SET lines=?2, fingerprint=?3",
            params![date, lines_json, fingerprint],
        )?;
        Ok(())
    }
}

/// R6 채굴(A) 재료 — 관찰창 내 (host, norm60, session)별 등장수·대표 preview.
pub struct PromptOccRow {
    pub host: String,
    pub norm60: String,
    pub session_id: String,
    pub occurrences: u64,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct RollupRow {
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
    pub session_count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DaySummary {
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
}

/// 그날 활동한 세션 1건 — 다이어리 일별 활동 패널용.
/// `project`는 **표시명**이다(`cwd`의 basename, 없으면 `project_id` 폴백) —
/// `project_id`는 `win:d:\project\space-a` 형태의 정규화 키라 그대로 보여줄 값이 아니다.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DaySession {
    pub session_id: String,
    pub project: String,
    /// **그날의 첫 활동 시각**(세션 시작 시각이 아니다 — 자정을 넘긴 세션도 있다).
    pub first_ts: String,
    /// 세션 전체의 첫 프롬프트(그날 첫 프롬프트가 아니다) — 세션의 정체성을 나타내므로.
    pub first_prompt: Option<String>,
}

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

/// content_items 한 행 — 프론트 팁/뉴스 카드용.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentRow {
    pub id: String,
    pub kind: String,
    pub dimension: Option<String>,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    pub trigger_tags: Vec<String>,
    pub score: i64,
    pub status: String,
    /// 처분 시각 — 「해결함」 7일 창 판정용 (스펙 §5). 미처분이면 None.
    pub status_ts: Option<String>,
    /// 번역 캐시 (스펙 §6.3·§6.4) — 아이템당 1회 생성, 재큐레이션에도 보존된다.
    /// 없으면 프론트가 원문(`title`·`body`)으로 폴백한다(엔진 미설정 사용자).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_ko: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_ko: Option<String>,
    /// 본문에서 뽑은 유효 기한 `YYYY-MM-DD`. 고정 슬롯 판정의 재료 (§6.4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    /// "당신 로그: …" — 사용자 실측 데이터로 접지한 근거 줄. list_content read 시점 계산.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personal: Option<String>,
}

/// `struggle_sessions`의 `interval_closed_before`에 주면 "닫힌 날짜 구간" 기준을 끈다 —
/// 어떤 타임스탬프도 빈 문자열보다 작지 않으므로 조건이 항상 거짓이 된다.
pub const INTERVAL_CRITERION_OFF: &str = "";

/// "고생 끝 해결" 후보 세션 (세션 회고 스펙 2026-07-19).
#[derive(Debug, Clone)]
pub struct StruggleSession {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    /// 세션 작업 디렉터리 — 프로젝트 이름의 유일한 출처. `project_id`(로그 디렉터리명)는
    /// 구분자가 전부 `-`로 뭉개져 basename을 되살릴 수 없다.
    pub cwd: Option<String>,
    /// 원문 프로즈 — Engine(로컬 생성 요약)까지만 간다. 허브 본문 직행 금지.
    pub first_prompt_preview: Option<String>,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub error_count: u64,
    pub total_events: u64,
    /// (도구명, 오류 횟수) — 결정론 식별
    pub error_tools: Vec<(String, u64)>,
    pub last_result_ok: bool,
}

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
    /// 처분 시각 — 「해결함」 7일 창을 프론트가 판정하는 재료 (스펙 §5). 미처분이면 None.
    pub status_ts: Option<String>,
    /// R6 판정 결과({worthy,reason,suggested_name,attempts,tokens}). 미판정이면 None.
    pub judgment: Option<serde_json::Value>,
}

/// R6 판정 배치 후보 — pending finding에서 뽑은 판정 재료 참조.
#[derive(Debug, Clone)]
pub struct JudgmentTarget {
    pub dedup_key: String,
    pub host: String,
    pub representative: String,
    pub prev_attempts: u32,
}

/// 한 파일을 offset부터 증분 수집. 반환값 = 신규 삽입 이벤트 수.
pub fn ingest_file(
    store: &SqliteStore,
    adapter: &dyn crate::adapter::SourceAdapter,
    file: &Path,
) -> Result<usize> {
    let file_key = file.to_string_lossy().to_string();
    let from = store.get_offset(&file_key)?;
    let (lines, new_offset) = adapter.read_incremental(file, from)?;

    let mut all = Vec::new();
    for (offset, line) in &lines {
        let evs = adapter.map(line, &file_key, *offset);
        all.extend(evs);
    }
    let inserted = store.upsert_events(&all)?;

    // 파일이 실제로 자란 경우에만 subagents 스캔·offset 기록을 수행한다 — 무변경 파일에
    // 매 수집 주기마다 read_dir + DB 쓰기가 반복되는 것 방지(Gemini medium).
    if new_offset > from {
        // 서브에이전트 하위 트랜스크립트 수 — <세션id>/subagents/*.jsonl 존재 카운트만 (전문 파싱은 후속, 코칭 v3 §4.1-3)
        // 이 카운트는 subagents/ 아래 모든 *.jsonl을 포함한다 — 스펙 §4.1-3의 agent-*.jsonl보다 넓은 상위집합(의도).
        let sub_dir = file.with_extension("").join("subagents");
        if let Ok(entries) = std::fs::read_dir(&sub_dir) {
            let n = entries
                .flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
                .count() as i64;
            if let Some(sid) = file.file_stem().and_then(|s| s.to_str()) {
                store.conn.execute(
                    "UPDATE sessions SET subagent_files=?2 WHERE session_id=?1",
                    rusqlite::params![sid, n],
                )?;
            }
        }
        store.set_offset(&file_key, new_offset)?;
    }
    Ok(inserted)
}

type FlatRow = (
    String, Option<String>, Option<String>, Option<String>, i64, i64, i64, i64, i64, i64, i64,
    Option<String>, Option<String>, Option<String>, Option<String>, Option<String>,
);

fn flatten(e: &NormalizedEvent) -> FlatRow {
    match &e.kind {
        EventKind::AssistantTurn { model, usage, web_search, web_fetch } => (
            "assistant_turn".into(),
            Some(format!("{:?}", model.family).to_lowercase()),
            Some(format!("{:?}", model.tier).to_lowercase()),
            Some(model.raw_id.clone()),
            usage.input as i64, usage.output as i64, usage.cache_read as i64,
            usage.cache_creation as i64, usage.eph_1h as i64,
            *web_search as i64, *web_fetch as i64,
            None, None, None, None, None,
        ),
        EventKind::ToolCall { kind, raw_name, target, .. } => {
            let (tkind, tsrv, ttool) = match kind {
                ToolKind::McpCall { server, tool } => (
                    "mcp_call".to_string(),
                    Some(server.clone()),
                    Some(tool.clone()),
                ),
                other => (tool_kind_str(other).to_string(), None, None),
            };
            (
                "tool_call".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
                Some(tkind), tsrv, ttool, target.clone(), Some(raw_name.clone()),
            )
        }
        EventKind::ToolResult { .. } => (
            "tool_result".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, None, None,
        ),
        EventKind::Compaction => (
            "compaction".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, None, None,
        ),
        EventKind::UserPrompt { .. } => (
            "user_prompt".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, None, None,
        ),
        EventKind::PermissionMode { mode } => (
            "permission_mode".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, Some(mode.clone()), None,
        ),
        EventKind::SecretFlag { pattern_id } => (
            "secret_flag".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, Some(pattern_id.clone()), None,
        ),
        EventKind::SessionMeta { .. } => (
            "session_meta".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, None, None,
        ),
    }
}

fn tool_kind_str(k: &ToolKind) -> &'static str {
    match k {
        ToolKind::FileRead => "file_read",
        ToolKind::FileEdit => "file_edit",
        ToolKind::FileWrite => "file_write",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::McpCall { .. } => "mcp_call",
        ToolKind::WebSearch => "web_search",
        ToolKind::WebFetch => "web_fetch",
        ToolKind::SubAgent => "sub_agent",
        ToolKind::Skill { .. } => "skill",
        ToolKind::Other(_) => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    /// 테스트용 방문 1건 — 필요한 필드만 바꿔 쓴다.
    fn visit(life_id: &str, visited_at: &str, kind: &str) -> LifeVisit {
        LifeVisit {
            life_id: life_id.into(),
            visited_at: visited_at.into(),
            kind: kind.into(),
            owner_name: Some("코난".into()),
            design_json: Some(r#"{"wallpaper":"cream","floor":"wood","objects":[]}"#.into()),
            diary_excerpt_json: "[]".into(),
            signed: true,
        }
    }

    /// `events(session_id)` 인덱스가 사라지면 **성능 절벽**이 돌아온다 — 상관 서브쿼리로
    /// events를 훑는 `struggle_sessions`가 실측 30MB DB(62k 행·292 세션)에서 188ms → 56,583ms로
    /// 300배 느려졌고, 그 쿼리는 스캔 후처리가 **스토어 락을 쥔 채** 돌아 앱 전체가 멎었다.
    /// 기능 테스트로는 절대 안 잡힌다(결과는 같고 시간만 다르다) — 그래서 존재를 직접 단언한다.
    #[test]
    fn events_session_id_index_exists() {
        let store = SqliteStore::open_in_memory().unwrap();
        let found: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_events_session'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "events(session_id) 인덱스가 없다 — 후처리 쿼리가 전체 스캔이 된다");
    }

    #[test]
    fn life_visit_round_trips_and_buckets_by_local_date() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 로컬 시각으로 기록 → 로컬 날짜로 조회 (타임존 무관하게 성립)
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        store.record_life_visit(&visit("life-a", &now.to_rfc3339(), "auto")).unwrap();

        let rows = store.life_visits_for_date(&today).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].life_id, "life-a");
        assert_eq!(rows[0].kind, "auto");
        assert_eq!(rows[0].owner_name.as_deref(), Some("코난"));
        assert!(rows[0].signed);
        // 다른 날짜 버킷엔 안 잡힌다
        let yesterday =
            (now.date_naive() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        assert!(store.life_visits_for_date(&yesterday).unwrap().is_empty());
    }

    #[test]
    fn prev_life_visit_returns_nearest_earlier_row_of_same_room() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.record_life_visit(&visit("life-a", "2026-07-01T10:00:00Z", "manual")).unwrap();
        store.record_life_visit(&visit("life-a", "2026-07-10T10:00:00Z", "manual")).unwrap();
        store.record_life_visit(&visit("life-b", "2026-07-09T10:00:00Z", "manual")).unwrap();

        let prev = store.prev_life_visit("life-a", "2026-07-20T00:00:00Z").unwrap().unwrap();
        assert_eq!(prev.visited_at, "2026-07-10T10:00:00Z"); // 가장 가까운 과거
        // 첫 방문(그보다 이전 행 없음)
        assert!(store.prev_life_visit("life-a", "2026-07-01T10:00:00Z").unwrap().is_none());
        // 다른 방 행은 섞이지 않는다
        assert_eq!(
            store.prev_life_visit("life-b", "2026-07-20T00:00:00Z").unwrap().unwrap().visited_at,
            "2026-07-09T10:00:00Z"
        );
    }

    #[test]
    fn last_visit_times_returns_latest_per_room() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.record_life_visit(&visit("life-a", "2026-07-01T10:00:00Z", "auto")).unwrap();
        store.record_life_visit(&visit("life-a", "2026-07-12T10:00:00Z", "auto")).unwrap();
        store.record_life_visit(&visit("life-b", "2026-07-05T10:00:00Z", "manual")).unwrap();

        let mut times = store.last_visit_times().unwrap();
        times.sort();
        assert_eq!(
            times,
            vec![
                ("life-a".to_string(), "2026-07-12T10:00:00Z".to_string()),
                ("life-b".to_string(), "2026-07-05T10:00:00Z".to_string()),
            ]
        );
    }

    #[test]
    fn prune_life_visits_deletes_only_older_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.record_life_visit(&visit("life-a", "2026-06-01T10:00:00Z", "auto")).unwrap();
        store.record_life_visit(&visit("life-a", "2026-07-20T10:00:00Z", "auto")).unwrap();

        assert_eq!(store.prune_life_visits("2026-07-01T00:00:00Z").unwrap(), 1);
        assert_eq!(
            store.last_visit_times().unwrap(),
            vec![("life-a".to_string(), "2026-07-20T10:00:00Z".to_string())]
        );
    }

    fn turn(session: &str, uuid: &str, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "test".into(),
            host: "Windows".into(),
            project_id: "c--users-jibin".into(),
            session_id: session.into(),
            uuid: Some(uuid.into()),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(),
            source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { cache_creation: cache_create, ..Default::default() },
                web_search: 0,
                web_fetch: 0,
            },
        }
    }

    #[test]
    fn upsert_is_idempotent_by_dedup_key() {
        let store = SqliteStore::open_in_memory().unwrap();
        let evs = vec![turn("s1", "u1", 55000), turn("s1", "u2", 0)];
        let n1 = store.upsert_events(&evs).unwrap();
        assert_eq!(n1, 2);
        // 같은 이벤트 재삽입 → 0개 신규
        let n2 = store.upsert_events(&evs).unwrap();
        assert_eq!(n2, 0);
        assert_eq!(store.count_events().unwrap(), 2);
    }

    #[test]
    fn offset_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 0);
        store.set_offset("f.jsonl", 4096).unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 4096);
        store.set_offset("f.jsonl", 8192).unwrap();
        assert_eq!(store.get_offset("f.jsonl").unwrap(), 8192);
    }

    #[test]
    fn upsert_keeps_multiple_events_sharing_one_line_uuid() {
        use crate::model::*;
        let mk = |off: u64, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "t".into(),
            host: "Windows".into(),
            project_id: "c--users-jibin".into(),
            session_id: "s1".into(),
            uuid: Some("u1".into()),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(),
            source_offset: off,
            msg_id: None,
            kind,
        };
        let evs = vec![
            mk(0, EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(),
                web_search: 0,
                web_fetch: 0,
            }),
            mk(1, EventKind::ToolCall {
                kind: ToolKind::FileRead,
                raw_name: "Read".into(),
                target: Some("a.txt".into()),
                tool_use_id: None,
            }),
            mk(2, EventKind::ToolCall {
                kind: ToolKind::FileRead,
                raw_name: "Read".into(),
                target: Some("b.txt".into()),
                tool_use_id: None,
            }),
        ];
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(
            store.upsert_events(&evs).unwrap(),
            3,
            "AssistantTurn + 2 ToolCalls sharing one line uuid must all persist"
        );
        assert_eq!(store.count_events().unwrap(), 3);
        // idempotent re-insert
        assert_eq!(store.upsert_events(&evs).unwrap(), 0);
    }

    #[test]
    fn bash_secret_never_persists_to_any_text_column() {
        // 계약: Bash 명령 내 시크릿은 pattern_id만 저장되고 원문은 어떤 텍스트 컬럼에도 남지 않는다.
        use crate::adapter::SourceAdapter;
        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = crate::adapter::ClaudeCodeAdapter {
            root: std::path::PathBuf::from("."),
            host: "Windows".into(),
        };
        let secret = "sk-ant-api03-AbCdEfGh123456";
        let line = format!(
            r#"{{"type":"assistant","sessionId":"s1","uuid":"u1","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":1}},"content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"export ANTHROPIC_API_KEY={secret}"}}}}]}}}}"#
        );
        let evs = adapter.map(&line, "s1.jsonl", 0);
        store.upsert_events(&evs).unwrap();

        for col in [
            "tool_target", "raw_name", "model_raw", "tool_kind", "tool_server",
            "tool_tool", "session_id", "project_id", "source_file", "dedup_key",
        ] {
            let hits: i64 = store
                .conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM events WHERE {col} LIKE ?1"),
                    params![format!("%{secret}%")],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(hits, 0, "raw secret leaked into events.{col}");
        }
        // pattern_id 플래그는 정상 방출
        let flags: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM events WHERE kind='secret_flag'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(flags, 1);
    }

    #[test]
    fn earliest_session_ts_returns_min_first_ts() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sid: &str, uuid: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sid.into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
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

    #[test]
    fn replace_host_inventory_drops_stale_keeps_other_hosts() {
        use crate::inventory::McpServer;
        let mut store = SqliteStore::open_in_memory().unwrap();
        // 시드: host "H" 에 (P,"A"),(P,"stale"); host "H2" 에 (Q,"keep")
        store.upsert_inventory("H", "P", &[
            McpServer { name: "A".into(), source: "project".into() },
            McpServer { name: "stale".into(), source: "project".into() },
        ]).unwrap();
        store.upsert_inventory("H2", "Q", &[
            McpServer { name: "keep".into(), source: "project".into() },
        ]).unwrap();

        // 현재셋 = (P,[A]) 로 교체 → stale 제거
        store.replace_host_inventory("H", &[
            ("P".to_string(), vec![McpServer { name: "A".into(), source: "project".into() }]),
        ]).unwrap();

        let mut h = store.active_servers("H", "P").unwrap();
        h.sort();
        assert_eq!(h, vec!["A"], "stale 서버는 제거, 현재 서버는 유지");
        assert_eq!(store.active_servers("H2", "Q").unwrap(), vec!["keep"], "다른 호스트 불변");
    }

    #[test]
    fn replace_host_inventory_empty_clears_host() {
        use crate::inventory::McpServer;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.upsert_inventory("H", "P", &[
            McpServer { name: "A".into(), source: "project".into() },
        ]).unwrap();
        store.replace_host_inventory("H", &[]).unwrap();
        assert!(store.active_servers("H", "P").unwrap().is_empty(), "빈 셋이면 호스트 행 전부 제거");
    }

    #[test]
    fn ingest_file_then_rollup_aggregates_tokens() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        // 트랜스크립트 디렉터리명이 project_id의 원천이므로 하위 디렉터리에 배치
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("s1.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        let line1 = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","timestamp":"2026-07-01T10:00:00Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":55000}}}"#;
        let line2 = r#"{"type":"assistant","sessionId":"s1","uuid":"u2","timestamp":"2026-07-01T10:05:00Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":5,"output_tokens":7}}}"#;
        writeln!(f, "{line1}").unwrap();
        writeln!(f, "{line2}").unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let n = ingest_file(&store, &adapter, &file).unwrap();
        assert_eq!(n, 2);

        // 재수집(offset 저장됨) → 신규 0
        assert_eq!(ingest_file(&store, &adapter, &file).unwrap(), 0);

        store.rebuild_rollup().unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2026-07-01T10:00:00Z").unwrap()
            .with_timezone(&chrono::Local).format("%Y-%m-%d").to_string();
        let r = store.rollup_for("Windows", "c--users-jibin", &expected).unwrap().unwrap();
        assert_eq!(r.tok_input, 15);
        assert_eq!(r.tok_output, 27);
        assert_eq!(r.tok_cache_create, 55000);
        assert_eq!(r.session_count, 1);
    }

    fn sess_turn(host: &str, project: &str, session: &str, uuid: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: project.into(), session_id: session.into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }
    }

    #[test]
    fn rollup_and_model_mix_bucket_by_local_date() {
        // UTC 자정 직전 이벤트 — 로컬 타임존(KST 등 동쪽)에선 다음날로 버킷돼야 한다.
        // 기대값을 chrono::Local로 계산하므로 머신 타임존과 무관하게 결정론적.
        let store = SqliteStore::open_in_memory().unwrap();
        let ts = "2026-07-01T23:30:00Z";
        // model_mix는 0토큰 그룹을 제외하므로 토큰을 채운다
        let mut ev = sess_turn("Windows", "p1", "s1", "u1", ts);
        if let EventKind::AssistantTurn { usage, .. } = &mut ev.kind { usage.input = 10; }
        store.upsert_events(&[ev]).unwrap();
        store.rebuild_rollup().unwrap();

        let expected = chrono::DateTime::parse_from_rfc3339(ts).unwrap()
            .with_timezone(&chrono::Local).format("%Y-%m-%d").to_string();
        assert_eq!(store.summary_for_date(&expected).unwrap().session_count, 1);
        assert!(!store.model_mix_for_date(&expected).unwrap().is_empty());
    }

    #[test]
    fn findings_for_date_scopes_r5_by_session_date_not_last_seen() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        // s1은 07-01, s2는 07-02 세션(upsert_events가 sessions.first_ts 채움)
        store.upsert_events(&[
            sess_turn("Windows", "p", "s1", "u1", "2026-07-01T10:00:00Z"),
            sess_turn("Windows", "p", "s2", "u2", "2026-07-02T10:00:00Z"),
        ]).unwrap();
        // R5 finding은 s1(07-01 행동)에 대한 것이지만 last_seen은 07-02(나중에 rules 실행).
        store.upsert_finding(&Finding {
            rule_id: "R5".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"a.txt","count":5}),
            est_tokens_saved: 4800, prescription: None, dedup_key: "R5|s1|a.txt".into(),
        }, "2026-07-02T09:00:00Z").unwrap();

        // 행동 날짜(07-01)로 조회 → 잡힘(last_seen=07-02인데도)
        let d1 = store.findings_for_date("Windows", "2026-07-01").unwrap();
        assert_eq!(d1.len(), 1);
        assert_eq!(d1[0].scope_ref, "s1");
        // last_seen 날짜(07-02)로 조회 → 없음(s1 세션은 07-02가 아니므로) — last_seen이 아니라 행동 날짜 기준
        assert!(store.findings_for_date("Windows", "2026-07-02").unwrap().is_empty());
    }

    #[test]
    fn findings_for_date_all_hides_non_new_status_but_shows_project_card() {
        // 스펙 §4 B ⓓ: 세션별 후보 카드는 다이어리에 안 띄운다(숨김 상태: pending/confirmed/rejected).
        // R7 프로젝트 카드(status='new')는 그대로 노출돼야 한다.
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            sess_turn("Windows", "p", "s1", "u1", "2026-07-01T10:00:00Z"),
        ]).unwrap();

        // R7 세션 후보 — upsert_finding이 init 'pending'(숨김)으로 넣는다.
        store.upsert_finding(&Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"session_id":"s1"}), est_tokens_saved: 0,
            prescription: None, dedup_key: "R7|sess|Windows|s1".into(),
        }, "2026-07-01T10:00:00Z").unwrap();
        assert!(store.findings_for_date_all("2026-07-01").unwrap()
            .iter().all(|f| f.dedup_key != "R7|sess|Windows|s1"), "pending 세션 후보는 숨겨야");

        // 판정 후 confirmed로 전환돼도 여전히 숨김(세션별 후보는 항상 비노출).
        store.set_judgment("R7|sess|Windows|s1", Some("confirmed"), &serde_json::json!({"over_modeled": true})).unwrap();
        assert!(store.findings_for_date_all("2026-07-01").unwrap()
            .iter().all(|f| f.dedup_key != "R7|sess|Windows|s1"), "confirmed 세션 후보도 숨겨야");

        // R7 프로젝트 카드 — scope_kind='project'라 init 'new'(노출).
        store.upsert_finding(&Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "project".into(), scope_ref: "p".into(),
            evidence: serde_json::json!({
                "total_sessions": 3, "session_ids": ["s1"],
                "note": "LLM 판정: 이 프로젝트의 Opus 세션 상당수가 Sonnet으로 충분",
            }),
            est_tokens_saved: 0, prescription: None, dedup_key: "R7|Windows|p".into(),
        }, "2026-07-01T10:00:00Z").unwrap();
        let all = store.findings_for_date_all("2026-07-01").unwrap();
        assert!(all.iter().any(|f| f.dedup_key == "R7|Windows|p"), "프로젝트 카드는 노출돼야");
    }

    // ── E: session_work_kinds + plugin 설치/사용 감지 ──

    fn wk_prompt_event(sid: &str, ts: &str, preview: &str) -> crate::model::NormalizedEvent {
        crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(format!("{sid}-p-{ts}")), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: format!("{sid}.jsonl"), source_offset: 0, msg_id: None,
            kind: crate::model::EventKind::UserPrompt { preview: preview.into(), is_command: false },
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
        // 창 밖 judged는 제외 (세션 last_ts보다 늦게 시작하는 창)
        let after = chrono::Utc::now().to_rfc3339();
        assert!(store.judged_work_kind_sessions(&after).unwrap().is_empty());
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
    fn installed_plugins_snapshot_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.installed_plugins_map("Windows").unwrap().is_none(), "미스캔 = None");
        store.set_installed_plugins("Windows",
            &serde_json::json!({"superpowers@mp": true, "off@mp": false})).unwrap();
        let map = store.installed_plugins_map("Windows").unwrap().unwrap();
        assert_eq!(map["superpowers@mp"], serde_json::json!(true));
        assert_eq!(map["off@mp"], serde_json::json!(false), "disabled도 스냅숏에 보존");
        assert!(store.installed_plugins_map("WSL:u").unwrap().is_none(), "호스트 분리");
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
            // 하네스 plugin_ 접두 MCP 호출 (context7 플러그인 경유 — standalone 없이도 감지)
            tool("s3", "u3", crate::model::ToolKind::from_raw_name("mcp__plugin_context7_context7__query-docs"),
                "mcp__plugin_context7_context7__query-docs", None, &recent),
            // 창 밖 스킬 호출 (superpowers) — 사용으로 안 침
            tool("s4", "u4", crate::model::ToolKind::Skill { name: "superpowers:brainstorming".into() },
                "Skill", Some("superpowers:brainstorming"), &old),
        ]).unwrap();
        assert!(store.plugin_used_recently("Windows", "frontend-design", None, &window).unwrap());
        assert!(store.plugin_used_recently("Windows", "context7", Some("context7"), &window).unwrap(),
            "plugin_ 접두 하네스 서버명으로 감지");
        assert!(!store.plugin_used_recently("Windows", "superpowers", None, &window).unwrap(),
            "창 밖 사용은 최근 사용 아님");
        assert!(!store.plugin_used_recently("WSL:u", "frontend-design", None, &window).unwrap(),
            "호스트 분리");
        // standalone 동명 MCP 서버 호출도 사용으로 친다
        store.upsert_events(&[
            tool("s2", "u2", crate::model::ToolKind::from_raw_name("mcp__context7__query-docs"),
                "mcp__context7__query-docs", None, &recent),
        ]).unwrap();
        assert!(store.plugin_used_recently("Windows", "context7", Some("context7"), &window).unwrap());
        // 서브에이전트(sidechain) 안에서만 쓴 plugin도 "사용 중"이다 — 억제(침묵) 방향이므로
        // §2 계약(발화 근거 = main-chain)과 무관. R2 감지기와 동일 (Codex 리뷰).
        let mut side = tool("s5", "u5",
            crate::model::ToolKind::Skill { name: "codex:rescue".into() },
            "Skill", Some("codex:rescue"), &recent);
        side.is_sidechain = true;
        store.upsert_events(&[side]).unwrap();
        assert!(store.plugin_used_recently("Windows", "codex", None, &window).unwrap(),
            "sidechain 전용 사용도 침묵 대상");
    }

    #[test]
    fn replace_plugin_inventory_atomic_swap() {
        use crate::inventory::PluginRecord;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            PluginRecord {
                plugin_key: "superpowers@mp".into(), namespace: "superpowers".into(),
                skill_count: 3, resident_tokens: 900,
                skills: vec!["brainstorming".into(), "writing-plans".into(), "tdd".into()],
                mcp_servers: vec![],
            },
        ]).unwrap();
        // 재교체: 다른 셋 → stale 제거 확인
        store.replace_plugin_inventory("Windows", &[
            PluginRecord {
                plugin_key: "vercel@mp".into(), namespace: "vercel".into(),
                skill_count: 1, resident_tokens: 400,
                skills: vec!["deploy".into()],
                mcp_servers: vec!["vercel".into()],
            },
        ]).unwrap();

        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM plugin_inventory WHERE host='Windows'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "원자 교체 — superpowers 행은 제거됨");
        let (ns, skills_json, mcp_json): (String, String, String) = store.conn.query_row(
            "SELECT namespace, skills_json, mcp_servers_json FROM plugin_inventory WHERE plugin_key='vercel@mp'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(ns, "vercel");
        assert_eq!(serde_json::from_str::<Vec<String>>(&skills_json).unwrap(), vec!["deploy"]);
        assert_eq!(serde_json::from_str::<Vec<String>>(&mcp_json).unwrap(), vec!["vercel"]);
    }

    #[test]
    fn findings_for_date_includes_r1_when_scope_active_that_day() {
        use crate::finding::{Finding, Prescription, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        // Windows: 07-01 세션(프로젝트 p), 07-03 세션(프로젝트 q)
        store.upsert_events(&[
            sess_turn("Windows", "p", "s1", "u1", "2026-07-01T10:00:00Z"),
            sess_turn("Windows", "q", "s2", "u2", "2026-07-03T10:00:00Z"),
        ]).unwrap();
        // R1 host-global("*"→scope_kind=host), last_seen 07-05
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "Windows".into(),
            evidence: serde_json::json!({"server":"context7"}),
            est_tokens_saved: 2500,
            prescription: Some(Prescription { kind: "remove_mcp".into(), payload: serde_json::json!({"server":"context7"}) }),
            dedup_key: "R1|Windows|Windows|context7".into(),
        }, "2026-07-05T09:00:00Z").unwrap();
        // R1 project(p) scope
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "project".into(), scope_ref: "p".into(),
            evidence: serde_json::json!({"server":"playwright"}),
            est_tokens_saved: 2500, prescription: None, dedup_key: "R1|Windows|p|playwright".into(),
        }, "2026-07-05T09:00:00Z").unwrap();

        // 07-01: host 활성(s1) + project p 활성(s1) → 둘 다 포함
        let d1 = store.findings_for_date("Windows", "2026-07-01").unwrap();
        assert_eq!(d1.len(), 2);
        assert!(d1.iter().any(|f| f.scope_kind == "host"));
        assert!(d1.iter().any(|f| f.scope_kind == "project"));
        // 07-03: host 활성(s2)이나 프로젝트는 q만 → host-global만, project p는 제외
        let d3 = store.findings_for_date("Windows", "2026-07-03").unwrap();
        assert_eq!(d3.len(), 1);
        assert_eq!(d3[0].scope_kind, "host");
        // 07-02: 세션 없음 → 아무 finding도 없음
        assert!(store.findings_for_date("Windows", "2026-07-02").unwrap().is_empty());
    }

    #[test]
    fn summary_and_settings_queries() {
        use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p1".into(),
            session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-02T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: 100, output: 50, cache_read: 10, cache_creation: 5, eph_1h: 0, eph_5m: 0 },
                web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        store.rebuild_rollup().unwrap();

        let day = store.summary_for_date("2026-07-02").unwrap();
        assert_eq!(day.session_count, 1);
        assert_eq!(day.tok_input, 100);
        assert_eq!(store.summary_for_date("2099-01-01").unwrap().session_count, 0);
        assert_eq!(store.total_sessions().unwrap(), 1);

        // settings 라운드트립
        assert_eq!(store.get_setting("k").unwrap(), None);
        store.set_setting("k", "v1").unwrap();
        store.set_setting("k", "v2").unwrap(); // upsert
        assert_eq!(store.get_setting("k").unwrap(), Some("v2".into()));
        assert_eq!(store.all_settings().unwrap(), vec![("k".to_string(), "v2".to_string())]);
    }

    #[test]
    fn findings_and_diary_queries() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let base = Finding {
            rule_id: "R5".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({"path":"a.txt","count":5}),
            est_tokens_saved: 7200, prescription: None, dedup_key: "R5|s1|a.txt".into(),
        };
        store.upsert_finding(&base, "2026-07-01T10:00:00Z").unwrap();
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            est_tokens_saved: 99000, dedup_key: "R1|global|ctx".into(),
            ..base
        }, "2026-07-01T11:00:00Z").unwrap();

        let rows = store.list_findings_current(false).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rule_id, "R1", "est_tokens_saved DESC 정렬");
        assert_eq!(rows[0].occurrences, 1);
        assert_eq!(store.sum_est_tokens_saved().unwrap(), 99000 + 7200);

        let sevs = store.finding_severities().unwrap();
        assert!(sevs.contains(&("R1|global|ctx".to_string(), "warn".to_string())));

        store.upsert_diary_index("2026-07-01", "Windows", "/tmp/d1.md", 100, "mock").unwrap();
        store.upsert_diary_index("2026-07-02", "Windows", "/tmp/d2.md", 100, "mock").unwrap();
        assert_eq!(store.diary_dates().unwrap(), vec!["2026-07-01", "2026-07-02"]);
        assert_eq!(store.diary_path_for("2026-07-02").unwrap(), Some("/tmp/d2.md".into()));
        assert_eq!(store.diary_path_for("2099-01-01").unwrap(), None);
    }

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
        assert!(store.set_finding_status("k1", "dismissed", "2026-08-03T00:00:00Z").unwrap());
        assert_eq!(store.list_findings_current(false).unwrap().len(), 1);
        assert_eq!(store.list_findings_current(true).unwrap().len(), 2);
        // 없는 키는 false
        assert!(!store.set_finding_status("nope", "resolved", "2026-08-03T00:00:00Z").unwrap());

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
    fn upsert_finding_refreshes_severity_and_prescription_but_keeps_status() {
        // R10 강등(Warn→Info, prescription 제거)이 기존 DB에도 upsert로 반영돼야 한다 (스펙 §3.2).
        // status(사용자 처분)는 upsert가 건드리지 않아야 한다.
        let store = SqliteStore::open_in_memory().unwrap();
        let f = Finding {
            rule_id: "R10".into(),
            severity: Severity::Warn,
            scope_host: Some("Windows".into()),
            scope_project: Some("p".into()),
            scope_kind: "project".into(),
            scope_ref: "p".into(),
            evidence: serde_json::json!({"n": 1}),
            est_tokens_saved: 500,
            prescription: Some(Prescription {
                kind: "automation_model_config".into(),
                payload: serde_json::json!({}),
            }),
            dedup_key: "R10|W|p".into(),
        };
        store.upsert_finding(&f, "2026-07-01T10:00:00Z").unwrap();
        assert!(store.set_finding_status("R10|W|p", "dismissed", "2026-08-03T00:00:00Z").unwrap());

        let f2 = Finding { severity: Severity::Info, prescription: None, ..f };
        store.upsert_finding(&f2, "2026-07-02T10:00:00Z").unwrap();

        let (severity, prescription_json, status): (String, Option<String>, String) = store
            .conn
            .query_row(
                "SELECT severity, prescription_json, status FROM findings WHERE dedup_key='R10|W|p'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(severity, "info");
        assert!(prescription_json.is_none());
        assert_eq!(status, "dismissed");
    }

    // ── 처분·수명 모델 (스펙 §5) ────────────────────────────────────────────

    /// 근거 수치가 n인 R6 패턴 finding — 재발 감지 테스트의 공용 재료.
    fn r6_with_sessions(key: &str, n: u64) -> Finding {
        Finding {
            rule_id: "R6".into(),
            severity: Severity::Suggest,
            scope_host: Some("Windows".into()),
            scope_project: None,
            scope_kind: "pattern".into(),
            scope_ref: "x".into(),
            evidence: serde_json::json!({"repeated_prompt": "rep", "session_count": n}),
            est_tokens_saved: 0,
            prescription: None,
            dedup_key: key.into(),
        }
    }

    fn disposition_of(store: &SqliteStore, key: &str) -> (String, Option<i64>, Option<String>) {
        store
            .conn
            .query_row(
                "SELECT status, status_evidence_n, status_ts FROM findings WHERE dedup_key=?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }

    /// 스펙 §5.1 — 「해결함」은 그 순간의 룰별 근거 수치를 스냅숏한다. 이 값이 재발 판정의
    /// 기준선이다. last_seen·occurrences는 스캔 지표라 기준선이 될 수 없다(§1.2 D5).
    #[test]
    fn resolving_a_finding_snapshots_rule_evidence_and_dispose_time() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();

        assert!(store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap());

        let (status, n, ts) = disposition_of(&store, "R6|W|a");
        assert_eq!(status, "resolved");
        assert_eq!(n, Some(3), "R6은 evidence.session_count를 스냅숏해야 함");
        assert_eq!(ts.as_deref(), Some("2026-08-03T09:00:00Z"));
    }

    /// 「무시」는 같은 묶음까지 영구 침묵이라 재발 기준선이 필요 없다 — 처분 시각만 남긴다.
    #[test]
    fn dismissing_a_finding_records_time_without_an_evidence_snapshot() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();

        store.set_finding_status("R6|W|a", "dismissed", "2026-08-03T09:00:00Z").unwrap();

        let (status, n, ts) = disposition_of(&store, "R6|W|a");
        assert_eq!(status, "dismissed");
        assert_eq!(n, None, "무시는 재발 기준선을 두지 않는다");
        assert_eq!(ts.as_deref(), Some("2026-08-03T09:00:00Z"));
    }

    /// 「실행취소」(new 복귀)는 기준선과 처분 시각을 지운다 — 남겨두면 다음 처분 전까지
    /// 옛 기준선이 살아 있어 엉뚱한 시점의 수치와 비교하게 된다.
    #[test]
    fn undoing_a_disposition_clears_the_snapshot_and_dispose_time() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        store.set_finding_status("R6|W|a", "new", "2026-08-03T09:30:00Z").unwrap();

        let (status, n, ts) = disposition_of(&store, "R6|W|a");
        assert_eq!(status, "new");
        assert_eq!(n, None);
        assert_eq!(ts, None);
    }

    /// 스펙 §5.1 — 수치가 그대로면 침묵. 스캔은 60초 디바운스라 여기서 부활시키면
    /// 「해결함」을 누른 1분 뒤 카드가 되돌아온다(§1.2 D5).
    #[test]
    fn resolved_finding_stays_silent_while_evidence_is_unchanged() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-03T09:01:00Z").unwrap();

        assert_eq!(disposition_of(&store, "R6|W|a").0, "resolved");
    }

    /// 근거 수치가 **초과**하면 재발이다 — 같은 지시를 또 반복해 세션이 붙었다는 뜻.
    /// 활성 복귀와 함께 기준선을 지워 다음 처분이 새 기준선을 잡게 한다.
    #[test]
    fn resolved_finding_revives_when_evidence_exceeds_the_snapshot() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        store.upsert_finding(&r6_with_sessions("R6|W|a", 4), "2026-08-04T09:00:00Z").unwrap();

        let (status, n, ts) = disposition_of(&store, "R6|W|a");
        assert_eq!(status, "new", "근거 수치 초과 = 재발 → 활성 복귀");
        assert_eq!(n, None);
        assert_eq!(ts, None);
    }

    /// 관찰창이 밀려 수치가 줄었다 원래대로 돌아온 건 재발이 아니다 — **초과**해야 한다.
    /// 보수적이지만 계속 반복하면 결국 초과하므로 영구 누락은 아니다(§12).
    #[test]
    fn resolved_finding_stays_silent_when_evidence_dips_and_returns() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        store.upsert_finding(&r6_with_sessions("R6|W|a", 2), "2026-08-04T09:00:00Z").unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-05T09:00:00Z").unwrap();

        assert_eq!(disposition_of(&store, "R6|W|a").0, "resolved");
    }

    /// 업데이트 전에 처분한 행은 기준선이 NULL이다 — 판정하지 않는다. 판정하면 업데이트
    /// 직후 묵은 카드가 한꺼번에 되살아난다.
    #[test]
    fn resolved_finding_with_null_snapshot_is_never_judged_as_recurrence() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        // 마이그레이션 이전에 처분된 행 재현 — status만 있고 기준선이 없다
        store
            .conn
            .execute(
                "UPDATE findings SET status='resolved', status_evidence_n=NULL, status_ts=NULL
                 WHERE dedup_key='R6|W|a'",
                [],
            )
            .unwrap();

        store.upsert_finding(&r6_with_sessions("R6|W|a", 99), "2026-08-04T09:00:00Z").unwrap();

        assert_eq!(disposition_of(&store, "R6|W|a").0, "resolved");
    }

    /// 「무시」는 영구 침묵이다 — 수치가 늘어도 부활하지 않는다(§5: 같은 묶음까지 침묵).
    /// 기준선을 **일부러 심어두고** 검증한다: 실제로는 무시가 기준선을 남기지 않아 NULL 가드에
    /// 걸리지만, 그러면 이 테스트가 "dismissed는 부활 대상이 아니다"를 증명하지 못한다
    /// (뮤테이션 확인: 부활 조건을 `status IN ('resolved','dismissed')`로 바꿔도 안 잡혔다).
    #[test]
    fn dismissed_finding_never_revives_even_with_a_baseline_planted() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "dismissed", "2026-08-03T09:00:00Z").unwrap();
        store
            .conn
            .execute("UPDATE findings SET status_evidence_n=3 WHERE dedup_key='R6|W|a'", [])
            .unwrap();

        store.upsert_finding(&r6_with_sessions("R6|W|a", 40), "2026-08-04T09:00:00Z").unwrap();

        assert_eq!(disposition_of(&store, "R6|W|a").0, "dismissed");
    }

    /// 해결함 7일 창 — 7일이 지나면 **화면에서만** 사라진다. 행은 남아 재발 감지를 계속하고,
    /// 다음 스캔이 같은 카드를 새 `new`로 되살리지 않아야 한다(지우면 룰이 새로 만든다).
    #[test]
    fn resolved_row_survives_rescans_and_is_not_recreated_as_new() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        // 7일 경과 후에도 룰은 같은 묶음을 계속 방출한다(수치는 그대로)
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-11T09:00:00Z").unwrap();
        store.prune_stale_r6_patterns(&["R6|W|a".to_string()]).unwrap();

        let (status, _, ts) = disposition_of(&store, "R6|W|a");
        assert_eq!(status, "resolved", "행이 보존돼야 재발 감지가 이어진다");
        assert_eq!(ts.as_deref(), Some("2026-08-03T09:00:00Z"), "처분 시각은 스캔이 밀지 않는다");
        assert!(store.list_findings_current(false).unwrap().is_empty(), "활성 목록엔 안 나온다");
        assert_eq!(store.list_findings_current(true).unwrap().len(), 1);
    }

    /// 재발 복귀는 **키도 severity도 그대로**라 전체 스냅숏 비교로는 보이지 않는다.
    /// 파이프라인이 활성 스냅숏을 봐야 `coach:finding`이 나가고 알림·말풍선이 뜬다 —
    /// 안 그러면 "재발하면 다시 떠야 한다"(§5)가 조용히 묻힌다.
    #[test]
    fn revival_is_invisible_to_the_full_snapshot_but_fresh_in_the_active_one() {
        use std::collections::HashMap;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();

        let full_before: HashMap<String, String> =
            store.finding_severities().unwrap().into_iter().collect();
        let active_before: HashMap<String, String> =
            store.active_finding_severities().unwrap().into_iter().collect();
        assert!(active_before.is_empty(), "처분된 카드는 활성 스냅숏에 없다");

        store.upsert_finding(&r6_with_sessions("R6|W|a", 4), "2026-08-04T09:00:00Z").unwrap();

        assert!(
            crate::pipeline::diff_findings(&full_before, &store.finding_severities().unwrap())
                .is_empty(),
            "전체 스냅숏은 재발을 놓친다 — 이게 활성 스냅숏을 쓰는 이유다"
        );
        assert_eq!(
            crate::pipeline::diff_findings(&active_before, &store.active_finding_severities().unwrap()),
            vec!["R6|W|a".to_string()],
        );
    }

    /// 7일 창은 프론트의 순수 함수가 판정한다(컴포넌트 테스트 라이브러리가 없어 세운 규약).
    /// 그러려면 처분 시각이 목록 행에 실려야 한다 — 두 테이블 모두.
    #[test]
    fn disposition_time_is_carried_on_finding_and_content_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_finding(&r6_with_sessions("R6|W|a", 3), "2026-08-01T00:00:00Z").unwrap();
        store.set_finding_status("R6|W|a", "resolved", "2026-08-03T09:00:00Z").unwrap();
        let row = &store.list_findings_current(true).unwrap()[0];
        assert_eq!(row.status_ts.as_deref(), Some("2026-08-03T09:00:00Z"));

        let item = crate::content::ContentItem {
            id: "lesson-model".into(),
            kind: crate::content::ItemKind::Tip,
            title: "t".into(),
            body: "b".into(),
            source_url: None,
            dimension: None,
            trigger_tags: vec!["personal".into()],
            base_priority: 0,
        };
        store.replace_content_items(&[(item, 550)], "2026-08-01T00:00:00Z", &[]).unwrap();
        store.set_content_status("lesson-model", "resolved", "2026-08-03T09:00:00Z").unwrap();
        let rows = store.list_content("2026-08-03T10:00:00Z", 3.0, true).unwrap();
        assert_eq!(rows[0].status_ts.as_deref(), Some("2026-08-03T09:00:00Z"));
    }

    #[test]
    fn upsert_seeds_r6_as_pending_others_as_new() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |rule: &str, key: &str| Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "x".into(),
            evidence: serde_json::json!({"repeated_prompt": "rep"}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&mk("R6", "R6|W|a"), "2026-07-21T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R11", "R11|W|b"), "2026-07-21T00:00:00Z").unwrap();
        let status = |k: &str| -> String {
            store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1",
                rusqlite::params![k], |r| r.get(0)).unwrap()
        };
        assert_eq!(status("R6|W|a"), "pending", "신규 R6은 판정 전 pending");
        assert_eq!(status("R11|W|b"), "new", "다른 룰은 기존대로 new");

        // 이미 판정돼 rejected가 된 R6은 재관측(upsert)돼도 status 불변 — 판정 캐시 유지
        store.set_finding_status("R6|W|a", "rejected", "2026-08-03T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R6", "R6|W|a"), "2026-07-21T01:00:00Z").unwrap();
        assert_eq!(status("R6|W|a"), "rejected", "ON CONFLICT는 status를 덮지 않아야 함");
    }

    #[test]
    fn r7_session_finding_starts_pending_project_starts_new() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |kind: &str, key: &str| crate::finding::Finding {
            rule_id: "R7".into(), severity: crate::finding::Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: kind.into(), scope_ref: "r".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&mk("session", "R7|sess|Windows|s1"), "2026-07-06T10:00:00Z").unwrap();
        store.upsert_finding(&mk("project", "R7|Windows|p"), "2026-07-06T10:00:00Z").unwrap();
        let status = |key: &str| -> String {
            store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1", [key], |r| r.get(0)).unwrap()
        };
        assert_eq!(status("R7|sess|Windows|s1"), "pending", "세션 후보는 판정 전 비노출");
        assert_eq!(status("R7|Windows|p"), "new", "프로젝트 롤업 카드는 즉시 노출");
    }

    #[test]
    fn judgment_json_column_roundtrips_through_finding_row() {
        let store = SqliteStore::open_in_memory().unwrap();
        let f = Finding {
            rule_id: "R6".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "pattern:abcd1234".into(),
            evidence: serde_json::json!({"repeated_prompt": "판매 리포트 뽑아줘"}),
            est_tokens_saved: 0, prescription: None, dedup_key: "R6|Windows|abcd1234".into(),
        };
        store.upsert_finding(&f, "2026-07-21T00:00:00Z").unwrap();
        // 신규 컬럼에 직접 판정 결과를 써 넣고, 조회가 이를 실어오는지 검증
        store.conn.execute(
            "UPDATE findings SET judgment_json=?2 WHERE dedup_key=?1",
            rusqlite::params!["R6|Windows|abcd1234", r#"{"worthy":true,"reason":"매일 반복되는 절차"}"#],
        ).unwrap();
        let rows = store.list_findings_current(true).unwrap();
        let row = rows.iter().find(|r| r.dedup_key == "R6|Windows|abcd1234").unwrap();
        let j = row.judgment.as_ref().expect("judgment_json이 FindingRow로 실려야 함");
        assert_eq!(j["worthy"], serde_json::json!(true));
        assert_eq!(j["reason"], serde_json::json!("매일 반복되는 절차"));
    }

    #[test]
    fn pending_r6_batch_filters_attempts_and_limits() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |key: &str, rep: &str| Finding {
            rule_id: "R6".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "x".into(),
            evidence: serde_json::json!({"repeated_prompt": rep}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        // 14개 pending 시드 (유효 매칭 12개 > limit 10 → LIMIT truncation을 실제로 검증)
        for i in 0..14 {
            let ts = format!("2026-07-21T00:{:02}:00Z", i);
            store.upsert_finding(&mk(&format!("R6|W|{i}"), &format!("반복 지시 {i}번")), &ts).unwrap();
        }
        // 하나는 attempts=3 도달 → 제외
        store.set_judgment("R6|W|0", None, &serde_json::json!({"attempts": 3, "error": "malformed"})).unwrap();
        // 하나는 이미 판정돼 new → pending 아님 → 제외
        store.set_judgment("R6|W|1", Some("new"), &serde_json::json!({"worthy": true, "attempts": 1})).unwrap();

        // 남은 유효 pending = 2..=13 (12개) > limit 10
        let batch = store.pending_r6_for_judgment(10).unwrap();
        assert_eq!(batch.len(), 10, "배치 상한 10 (유효 12개 중 최신 10개로 truncate)");
        assert!(batch.iter().all(|t| t.dedup_key != "R6|W|0"), "attempts 3 도달분 제외");
        assert!(batch.iter().all(|t| t.dedup_key != "R6|W|1"), "판정 완료(new) 제외");
        // last_seen DESC → 최신(13)이 먼저, 가장 오래된 유효 2개(2,3)는 상한에 밀려 제외
        assert_eq!(batch[0].dedup_key, "R6|W|13");
        assert_eq!(batch[0].representative, "반복 지시 13번");
        assert_eq!(batch[0].host, "Windows", "host 필드가 채워져야 함 (Task 7이 소비)");
        assert_eq!(batch[0].prev_attempts, 0, "미시도는 attempts 0");
        assert!(batch.iter().all(|t| t.dedup_key != "R6|W|2"), "상한에 밀린 오래된 유효 카드 제외");
        assert!(batch.iter().all(|t| t.dedup_key != "R6|W|3"), "상한에 밀린 오래된 유효 카드 제외");
    }

    #[test]
    fn pending_for_judgment_returns_generic_candidates() {
        let store = SqliteStore::open_in_memory().unwrap();
        // pending R7 세션 후보 2개 시드 (evidence에 session_id)
        let mk = |key: &str, sid: &str| crate::finding::Finding {
            rule_id: "R7".into(), severity: crate::finding::Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: sid.into(),
            evidence: serde_json::json!({"session_id": sid}),
            est_tokens_saved: 0, prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&mk("R7|sess|Windows|s1", "s1"), "2026-07-06T10:00:00Z").unwrap();
        store.upsert_finding(&mk("R7|sess|Windows|s2", "s2"), "2026-07-06T11:00:00Z").unwrap();
        let batch = store.pending_for_judgment("R7", 10).unwrap();
        assert_eq!(batch.len(), 2);
        assert!(batch.iter().all(|c| c.prev_attempts == 0));
        assert!(batch.iter().any(|c| c.evidence["session_id"] == "s2"));
        // R6 후보는 안 섞임
        assert!(store.pending_for_judgment("R6", 10).unwrap().is_empty());
    }

    #[test]
    fn set_judgment_transitions_status_or_keeps_pending() {
        let store = SqliteStore::open_in_memory().unwrap();
        let f = Finding {
            rule_id: "R6".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "x".into(),
            evidence: serde_json::json!({"repeated_prompt": "r"}), est_tokens_saved: 0,
            prescription: None, dedup_key: "R6|W|k".into(),
        };
        store.upsert_finding(&f, "2026-07-21T00:00:00Z").unwrap();
        // status 유지(파싱 실패) — judgment_json만 갱신
        store.set_judgment("R6|W|k", None, &serde_json::json!({"attempts": 1, "error": "bad"})).unwrap();
        let (st, j): (String, String) = store.conn.query_row(
            "SELECT status, judgment_json FROM findings WHERE dedup_key='R6|W|k'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(st, "pending");
        assert!(j.contains("\"attempts\":1"));
        // status 전환(판정 완료)
        store.set_judgment("R6|W|k", Some("rejected"), &serde_json::json!({"worthy": false, "attempts": 1})).unwrap();
        let st2: String = store.conn.query_row(
            "SELECT status FROM findings WHERE dedup_key='R6|W|k'", [], |r| r.get(0)).unwrap();
        assert_eq!(st2, "rejected");
    }

    #[test]
    fn model_mix_for_date_groups_by_family() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = |uuid: &str, model: &str, inp: u64, out: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-05T10:00:00Z".into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            msg_id: None,
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
        assert_eq!(mix[0], ("claude-opus-4-8".to_string(), 165)); // 100+50+10+5, 내림차순 첫 항목
        assert_eq!(mix[1], ("claude-haiku-4-5-20251001".to_string(), 30));
        assert!(store.model_mix_for_date("2099-01-01").unwrap().is_empty());

        // 세션 컨텍스트 — upsert_events가 만든 sessions 행에서 프로젝트·시작시각
        let ctx = store.session_ctx("s1").unwrap().unwrap();
        assert_eq!(ctx.0, "p");
        assert_eq!(ctx.1.as_deref(), Some("2026-07-05T10:00:00Z"));
        assert!(store.session_ctx("nope").unwrap().is_none());
    }

    #[test]
    fn model_mix_for_range_bounds_inclusive_and_open_start() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = |uuid: &str, ts: &str, inp: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: inp, output: 0, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        };
        store.upsert_events(&[
            ev("u1", "2026-07-01T10:00:00Z", 1),
            ev("u2", "2026-07-03T10:00:00Z", 10),
            ev("u3", "2026-07-05T10:00:00Z", 100),
        ]).unwrap();

        // 양끝 포함: 03~05 → 10+100 (여러 날짜가 한 모델로 합산)
        assert_eq!(store.model_mix_for_range(Some("2026-07-03"), "2026-07-05").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 110)]);
        // 단일일(from=to) — model_mix_for_date와 동치
        assert_eq!(store.model_mix_for_range(Some("2026-07-03"), "2026-07-03").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 10)]);
        // from=None → 하한 없음(전체)
        assert_eq!(store.model_mix_for_range(None, "2026-07-05").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 111)]);
        // 범위 밖 → 빈 벡터
        assert!(store.model_mix_for_range(Some("2026-08-01"), "2026-08-31").unwrap().is_empty());
    }

    #[test]
    fn model_mix_excludes_zero_token_models() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = |uuid: &str, model: &str, inp: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-03T10:00:00Z".into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { input: inp, output: 0, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        };
        store.upsert_events(&[
            ev("u1", "claude-opus-4-8", 100),
            ev("u2", "<synthetic>", 0), // Claude Code가 로컬 합성하는 에러 안내 메시지 — usage 전부 0
        ]).unwrap();
        assert_eq!(store.model_mix_for_range(None, "2026-07-03").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 100)]);
    }

    #[test]
    fn session_ctx_returns_cwd_and_first_prompt() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
                uuid: Some("m1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s1.jsonl".into(), source_offset: 0,
                msg_id: None,
                kind: EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: None },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
                uuid: Some("p1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s1.jsonl".into(), source_offset: 10,
                msg_id: None,
                kind: EventKind::UserPrompt { preview: "Run this exact Bash command".into(), is_command: false },
            },
        ]).unwrap();
        let (proj, _ts, cwd, prompt) = store.session_ctx("s1").unwrap().unwrap();
        assert_eq!(proj, "p");
        assert_eq!(cwd.as_deref(), Some("D:\\Project\\cowork"));
        assert_eq!(prompt.as_deref(), Some("Run this exact Bash command"));
        assert!(store.session_ctx("nope").unwrap().is_none());
    }

    #[test]
    fn tool_usage_for_sessions_chunks_beyond_sqlite_var_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 실제 도구 호출 2세션 (Bash x2, Read x1)
        let ev = |sess: &str, off: u64, raw: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-{off}")), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-19T10:00:00Z".into()), source_file: "s.jsonl".into(), source_offset: off,
            msg_id: None,
            kind: EventKind::ToolCall {
                kind: ToolKind::from_raw_name(raw), raw_name: raw.into(), target: None,
                tool_use_id: Some(format!("{sess}-{off}-t")),
            },
        };
        store.upsert_events(&[ev("s0",1,"Bash"), ev("s0",2,"Bash"), ev("s1",1,"Read")]).unwrap();
        // 999 초과 id (대부분 없는 것) — 청크 안 되면 "too many SQL variables"로 크래시
        let mut ids: Vec<String> = (0..1500).map(|i| format!("z{i}")).collect();
        ids.push("s0".into());
        ids.push("s1".into());
        let out = store.tool_usage_for_sessions(&ids).unwrap(); // 크래시하지 않아야
        let map: std::collections::HashMap<_, _> = out.into_iter().collect();
        assert_eq!(map.get("Bash"), Some(&2)); // 청크 경계 넘어 합산 정확
        assert_eq!(map.get("Read"), Some(&1));
    }

    #[test]
    fn delete_findings_by_rule_and_scope_removes_only_matching() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |rule: &str, kind: &str, key: &str| crate::finding::Finding {
            rule_id: rule.into(), severity: crate::finding::Severity::Suggest,
            scope_host: None, scope_project: None,
            scope_kind: kind.into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&mk("R7", "session", "R7|s1"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R7", "project", "R7|W|p"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "session", "R5|s1|a"), "2026-07-06T00:00:00Z").unwrap();

        let n = store.delete_findings_by_rule_and_scope("R7", "session").unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.count_findings().unwrap(), 2);
    }

    #[test]
    fn tool_result_row_stores_status_and_pointer() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some("u1".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-07T10:00:00Z".into()),
            source_file: "C:\\proj\\s1.jsonl".into(), source_offset: 42,
            msg_id: None,
            kind: EventKind::ToolResult { tool_use_id: "toolu_1".into(), status: ResultStatus::Denied, result_len: 4200 },
        };
        assert_eq!(store.upsert_events(&[ev]).unwrap(), 1);
        let (kind, status, tuid, sfile): (String, Option<String>, Option<String>, Option<String>) =
            store.conn.query_row(
                "SELECT kind, result_status, tool_use_id, source_file FROM events WHERE session_id='s1'",
                [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!(kind, "tool_result");
        assert_eq!(status.as_deref(), Some("denied"));
        assert_eq!(tuid.as_deref(), Some("toolu_1"));
        assert_eq!(sfile.as_deref(), Some("C:\\proj\\s1.jsonl"));
    }

    #[test]
    fn migrate_adds_model_raw_and_forces_recollect() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("m.db");
        // 구 스키마(모든 테이블은 신형, events만 model_raw 없는 구형)로 DB 선생성
        {
            let conn = Connection::open(&db).unwrap();
            let old_schema = SCHEMA.replace(" model_raw TEXT,", "");
            assert!(old_schema.len() < SCHEMA.len(), "구 스키마 치환 실패");
            conn.execute_batch(&old_schema).unwrap();
            conn.execute_batch(
                "INSERT INTO events (dedup_key, session_id, host, project_id, source_offset, kind)
                   VALUES ('old:0','s1','Windows','p',0,'assistant_turn');
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 123);
                 INSERT INTO daily_rollup (host, project_id, date, session_count)
                   VALUES ('Windows','p','2026-07-05',1);",
            ).unwrap();
        }
        // 마이그레이션 전 findings 심어서 보존 검증
        {
            let store = {
                let conn = Connection::open(&db).unwrap();
                SqliteStore { conn }
            };
            store.upsert_finding(&Finding {
                rule_id: "R1".into(), severity: Severity::Warn,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "host".into(), scope_ref: "srv".into(),
                evidence: serde_json::json!({}), est_tokens_saved: 10,
                prescription: None, dedup_key: "keep".into(),
            }, "2026-07-05T00:00:00Z").unwrap();
            store.set_finding_status("keep", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행
        // 컬럼 생김 + 재수집 유도(events/ingest_state/rollup 비움)
        let n: i64 = store.conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let n: i64 = store.conn.query_row("SELECT COUNT(*) FROM ingest_state", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert_eq!(store.summary_for_date("2026-07-05").unwrap().session_count, 0);
        // findings·status 보존
        let rows = store.list_findings_current(true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "dismissed");
        // 재실행(이미 마이그레이션됨) 시 무파괴
        store.set_setting("k", "v").unwrap();
        let store2 = SqliteStore::open(&db).unwrap();
        assert_eq!(store2.get_setting("k").unwrap().as_deref(), Some("v"));
    }

    #[test]
    fn migrate_v2_1_adds_source_file_cols_and_forces_recollect() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("m21.db");
        // v2 시대 스키마(model_raw는 있으나 v2.1 컬럼은 없는 상태)로 DB 선생성
        // — has_model_raw는 참이라 v2 분기는 건너뛰고, has_source_file만 거짓이라 v2.1 분기만 단독 발화한다.
        let step1 = SCHEMA.replace(
            ",\n  source_file TEXT, tool_use_id TEXT, result_status TEXT",
            "",
        );
        assert!(step1.len() < SCHEMA.len(), "events v2.1 컬럼 치환 실패");
        let v2_schema = step1.replace(
            ",\n  cwd TEXT, first_prompt_preview TEXT, first_prompt_source_file TEXT, first_prompt_offset INTEGER",
            "",
        );
        assert!(v2_schema.len() < step1.len(), "sessions v2.1 컬럼 치환 실패");

        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(&v2_schema).unwrap();
            conn.execute_batch(
                "INSERT INTO events (dedup_key, session_id, host, project_id, source_offset, kind)
                   VALUES ('old:0','s1','Windows','p',0,'assistant_turn');
                 INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch)
                   VALUES ('s1','Windows','p','claude-code','2026-07-05T00:00:00Z','2026-07-05T00:00:00Z',NULL);
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 123);
                 INSERT INTO daily_rollup (host, project_id, date, session_count)
                   VALUES ('Windows','p','2026-07-05',1);",
            ).unwrap();
        }
        // 마이그레이션 전 findings 심어서 보존 검증
        {
            let conn = Connection::open(&db).unwrap();
            let store = SqliteStore { conn };
            store.upsert_finding(&Finding {
                rule_id: "R1".into(), severity: Severity::Warn,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "host".into(), scope_ref: "srv".into(),
                evidence: serde_json::json!({}), est_tokens_saved: 10,
                prescription: None, dedup_key: "keep21".into(),
            }, "2026-07-05T00:00:00Z").unwrap();
            store.set_finding_status("keep21", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v2.1 분기 발화

        // v2.1 컬럼이 실제로 생겼는지 확인
        let has_source_file = store.conn
            .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name='source_file'").unwrap()
            .exists([]).unwrap();
        assert!(has_source_file, "events.source_file 컬럼이 추가돼야 함");
        let has_cwd = store.conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name='cwd'").unwrap()
            .exists([]).unwrap();
        assert!(has_cwd, "sessions.cwd 컬럼이 추가돼야 함");

        // 전체 재수집 유도 — events/sessions/ingest_state/daily_rollup 모두 비워짐
        for table in ["events", "sessions", "ingest_state", "daily_rollup"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} 은(는) v2.1 마이그레이션 후 비워져야 함");
        }

        // findings·status는 보존
        let rows = store.list_findings_current(true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].dedup_key, "keep21");
        assert_eq!(rows[0].status, "dismissed");
    }

    /// 스펙 §5.3 — 처분·수명 컬럼은 **컬럼 전용** 마이그레이션이다. judgment_json 선례를 따라
    /// events/sessions/ingest_state/daily_rollup을 절대 비우지 않는다. 마이그레이션이 전량
    /// 재수집을 유발해 릴리스마다 콜드 스캔이 되돌아오던 사고(#146)의 재발 방지 가드.
    #[test]
    fn migrate_adds_status_lifecycle_columns_without_recollecting() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mstatus.db");
        let old = SCHEMA
            .replace(",\n  status_evidence_n INTEGER, status_ts TEXT", "")
            .replace(",\n  status_ts TEXT\n);", "\n);");
        assert!(old.len() < SCHEMA.len(), "처분·수명 컬럼 치환 실패 — SCHEMA 문자열 확인");

        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(&old).unwrap();
            // user_version=8 = 현행 최신. 이전 파괴적 분기는 건너뛰고 새 컬럼 분기만 단독 발화한다.
            conn.execute_batch(
                "PRAGMA user_version = 8;
                 INSERT INTO events (dedup_key, session_id, host, project_id, source_offset, kind)
                   VALUES ('e:0','s1','Windows','p',0,'assistant_turn');
                 INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts)
                   VALUES ('s1','Windows','p','claude-code','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z');
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);
                 INSERT INTO daily_rollup (host, project_id, date, session_count)
                   VALUES ('Windows','p','2026-08-01',1);
                 INSERT INTO content_items (id, kind, title, body, status)
                   VALUES ('lesson-cache','tip','t','b','dismissed');",
            )
            .unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행

        for (table, col) in [
            ("findings", "status_evidence_n"),
            ("findings", "status_ts"),
            ("content_items", "status_ts"),
        ] {
            let exists = store
                .conn
                .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name='{col}'"))
                .unwrap()
                .exists([])
                .unwrap();
            assert!(exists, "{table}.{col} 컬럼이 추가돼야 함");
        }
        for table in ["events", "sessions", "ingest_state", "daily_rollup"] {
            let n: i64 = store
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "{table} 행은 컬럼 전용 마이그레이션에서 보존돼야 함");
        }
        // 사용자 처분 기록도 보존
        let status: String = store
            .conn
            .query_row("SELECT status FROM content_items WHERE id='lesson-cache'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "dismissed");
    }

    #[test]
    fn prompt_events_accumulate_all_prompts_with_norm_and_dedup() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let base = |uuid: &str, off: u64, preview: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-20T10:00:00Z".into()),
            source_file: "s1.jsonl".into(), source_offset: off,
            msg_id: None,
            kind: EventKind::UserPrompt { preview: preview.into(), is_command: false },
        };
        store.upsert_events(&[
            base("p1", 10, "매일 아침 판매 리포트 뽑아줘"),
            base("p2", 20, "PR 리뷰 코멘트 종합해서 조치해줘"), // 세션 중간 프롬프트도 축적
            base("p3", 30, "ㅇㅋ"),                              // 8자 미만 → 제외
        ]).unwrap();
        store.upsert_events(&[base("p1", 10, "매일 아침 판매 리포트 뽑아줘")]).unwrap(); // 멱등

        let rows: Vec<(String, String, String)> = {
            let mut stmt = store.conn.prepare(
                "SELECT session_id, norm60, preview FROM prompt_events ORDER BY source_offset").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap()
                .collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(rows.len(), 2, "짧은 프롬프트 제외 + 멱등이어야 함");
        assert_eq!(rows[0].1, "매일 아침 판매 리포트 뽑아줘");
        assert_eq!(rows[1].2, "PR 리뷰 코멘트 종합해서 조치해줘");
        // sessions.first_prompt_preview는 기존대로 최초 1건 유지
        let first: Option<String> = store.conn.query_row(
            "SELECT first_prompt_preview FROM sessions WHERE session_id='s1'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(first.as_deref(), Some("매일 아침 판매 리포트 뽑아줘"));
    }

    #[test]
    fn session_user_prompts_returns_full_text_main_chain() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        // 실질 프롬프트 2개 (deref는 source_file+offset 필요 없이 preview로 폴백 확인)
        store.upsert_events(&[
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(), host: "Windows".into(),
                project_id: "p".into(), session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 0, msg_id: None,
                kind: EventKind::UserPrompt { preview: "이 파일 이름만 바꿔줘".into(), is_command: false },
            },
        ]).unwrap();
        let ps = store.session_user_prompts("s1", 5).unwrap();
        assert_eq!(ps.len(), 1);
        assert!(ps[0].contains("이름만 바꿔줘"));
    }

    #[test]
    fn session_user_prompts_returns_chronological_order() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(), host: "Windows".into(),
                project_id: "p".into(), session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-06T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 0, msg_id: None,
                kind: EventKind::UserPrompt { preview: "첫번째로 파일을 읽어줘".into(), is_command: false },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(), host: "Windows".into(),
                project_id: "p".into(), session_id: "s1".into(), uuid: Some("u2".into()), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-06T10:01:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 1, msg_id: None,
                kind: EventKind::UserPrompt { preview: "두번째로 코드를 수정해줘".into(), is_command: false },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(), host: "Windows".into(),
                project_id: "p".into(), session_id: "s1".into(), uuid: Some("u3".into()), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-06T10:02:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: 2, msg_id: None,
                kind: EventKind::UserPrompt { preview: "세번째로 테스트를 실행해줘".into(), is_command: false },
            },
        ]).unwrap();
        let ps = store.session_user_prompts("s1", 5).unwrap();
        assert_eq!(ps, vec!["첫번째로 파일을 읽어줘", "두번째로 코드를 수정해줘", "세번째로 테스트를 실행해줘"],
            "심사관은 시간순(오래된 것부터)으로 봐야 함");
    }

    #[test]
    fn prompt_occurrence_rows_aggregates_per_norm_session() {
        use crate::model::{EventKind, NormalizedEvent};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sess: &str, off: u64, text: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-{off}")), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: off,
            msg_id: None, kind: EventKind::UserPrompt { preview: text.into(), is_command: false },
        };
        // s1: 같은 지시 2회(다른 ts) + s2: 같은 지시 1회. (모두 8자↑ = norm60 대상)
        store.upsert_events(&[
            mk("s1", 0, "이 함수 리팩토링 진행해줘", "2026-07-01T10:00:00Z"),
            mk("s1", 1, "이 함수 리팩토링 진행해줘", "2026-07-01T10:05:00Z"),
            mk("s2", 0, "이 함수 리팩토링 진행해줘", "2026-07-01T11:00:00Z"),
        ]).unwrap();

        let rows = store.prompt_occurrence_rows("2026-06-01T00:00:00Z").unwrap();
        // (host,norm,session) 그룹: (s1)=2, (s2)=1
        assert_eq!(rows.len(), 2);
        let s1 = rows.iter().find(|r| r.session_id == "s1").unwrap();
        assert_eq!(s1.occurrences, 2);
        assert!(s1.norm60.contains("리팩토링"));
        let s2 = rows.iter().find(|r| r.session_id == "s2").unwrap();
        assert_eq!(s2.occurrences, 1);
    }

    #[test]
    fn prompt_occurrence_rows_excludes_outside_window() {
        use crate::model::{EventKind, NormalizedEvent};
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "old".into(),
            uuid: Some("old-0".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-01-01T00:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0, msg_id: None,
            kind: EventKind::UserPrompt { preview: "관찰창 밖 오래된 지시".into(), is_command: false },
        }]).unwrap();
        let rows = store.prompt_occurrence_rows("2026-06-01T00:00:00Z").unwrap();
        assert!(rows.is_empty(), "cutoff 이전 프롬프트는 제외");
    }

    #[test]
    fn prompt_sessions_for_norms_unions_multiple_norms() {
        use crate::model::{EventKind, NormalizedEvent};
        use crate::rules::r6_repeated_prompts::normalize;
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |sess: &str, text: &str, ts: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(format!("{sess}-0")), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None, kind: EventKind::UserPrompt { preview: text.into(), is_command: false },
        };
        store.upsert_events(&[
            mk("s1", "리뷰 코멘트 종합 검토해줘", "2026-07-01T10:00:00Z"),
            mk("s2", "리뷰 코멘트 종합 반영 부탁", "2026-07-01T11:00:00Z"),
        ]).unwrap();
        let n1 = normalize("리뷰 코멘트 종합 검토해줘").unwrap();
        let n2 = normalize("리뷰 코멘트 종합 반영 부탁").unwrap();

        let rows = store.prompt_sessions_for_norms("Windows", &[n1, n2]).unwrap();
        let sessions: std::collections::HashSet<&str> =
            rows.iter().map(|(s, _)| s.as_str()).collect();
        assert!(sessions.contains("s1") && sessions.contains("s2"), "두 norm의 세션 합집합");
    }

    #[test]
    fn prompt_sessions_for_norms_empty_returns_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.prompt_sessions_for_norms("Windows", &[]).unwrap().is_empty());
    }

    #[test]
    fn prompt_events_skip_sidechain_prompts() {
        // 사이드체인(서브에이전트) user 라인은 사용자 지시가 아니다 — R6 재료 제외 (Codex P2)
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some("sc1".into()), parent_uuid: None, is_sidechain: true,
            ts: Some("2026-07-20T10:00:00Z".into()),
            source_file: "s1.jsonl".into(), source_offset: 10,
            msg_id: None,
            kind: EventKind::UserPrompt { preview: "서브에이전트 내부의 반복 프롬프트입니다".into(), is_command: false },
        }]).unwrap();
        let n: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM prompt_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "사이드체인 프롬프트는 prompt_events에 쌓이면 안 됨");
    }

    #[test]
    fn prompt_events_skip_command_invocations_but_keep_first_prompt() {
        // 스킬/커맨드 호출(is_command)은 R6 반복 마이닝(prompt_events)에서 제외하되,
        // 세션 대표(first_prompt_preview)로는 보존한다 (Codex 리뷰 finding 2).
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some("c1".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-20T10:00:00Z".into()),
            source_file: "s1.jsonl".into(), source_offset: 10,
            msg_id: None,
            kind: EventKind::UserPrompt {
                preview: "…플랜을 superpowers:executing-plans 로 실행".into(), is_command: true },
        }]).unwrap();
        let n: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM prompt_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "커맨드 호출 프롬프트는 prompt_events에 쌓이면 안 됨");
        let fp: Option<String> = store.conn
            .query_row("SELECT first_prompt_preview FROM sessions WHERE session_id='s1'",
                [], |r| r.get(0)).unwrap();
        assert!(fp.unwrap().contains("executing-plans"), "first_prompt는 세션 대표로 보존");
    }

    #[test]
    fn migrate_v4_recollects_to_rebuild_prompt_events() {
        // v3 시대 DB에는 사이드체인 프롬프트가 prompt_events에 섞여 있을 수 있다 → 재수집으로 재구축
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv4.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 3;
                 INSERT INTO prompt_events (dedup_key, session_id, host, project_id,
                   source_file, source_offset, norm60, preview)
                   VALUES ('sc:0','s1','Windows','p','f.jsonl',0,'서브에이전트 프롬프트','서브에이전트 프롬프트');
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 99);",
            ).unwrap();
        }
        let store = SqliteStore::open(&db).unwrap();
        for table in ["prompt_events", "ingest_state"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} 은(는) v4에서 비워져 재수집돼야 함");
        }
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 4, "v4 분기를 지나야 함 (후속 분기로 더 올라갈 수 있음)");
    }

    #[test]
    fn migrate_v5_purges_flooded_r23_findings_without_recollect() {
        // 가족 dedup·상한 도입 전에 쌓인 R23 카드 홍수(200+)를 일괄 정리 — 재수집은 불필요
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv5.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 4;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);",
            ).unwrap();
            let store = SqliteStore { conn };
            store.upsert_finding(&Finding {
                rule_id: "R23".into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:flood".into(),
                evidence: serde_json::json!({"sequence": ["bash:gh", "file-ops", "other"]}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R23|Windows|flood".into(),
            }, "2026-07-20T12:00:00Z").unwrap();
            store.upsert_finding(&Finding {
                rule_id: "R6".into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:ok".into(),
                evidence: serde_json::json!({"repeated_prompt": "매일 아침 판매 리포트 뽑아줘"}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R6|Windows|ok".into(),
            }, "2026-07-20T12:00:00Z").unwrap();
            // PR2: upsert가 R6을 pending으로 넣으므로, v6(오염된 'new' 카드 정화) 검증을 위해 new로 복원
            store.set_finding_status("R6|Windows|ok", "new", "2026-08-03T00:00:00Z").unwrap();
            // v6까지는 dismissed R23이 나깅 방지용으로 보존됐으나, v7에서 R23 룰 자체가
            // 폐기되며 dismissed 포함 전량 삭제 대상이 된다 (PR2 스펙 §4.6) — 아래 assert 참고.
            store.upsert_finding(&Finding {
                rule_id: "R23".into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:muted".into(),
                evidence: serde_json::json!({"sequence": ["skill:x", "mcp:m", "bash:gh"]}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R23|Windows|muted".into(),
            }, "2026-07-20T12:00:00Z").unwrap();
            store.set_finding_status("R23|Windows|muted", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }
        let store = SqliteStore::open(&db).unwrap();
        let keys: Vec<String> = {
            let mut stmt = store.conn.prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap()
        };
        // v5(R23 new 삭제) → v6(R6/R23 new 삭제·재수집) → v7(R23 전량 삭제, dismissed 포함)까지
        // 연쇄 실행된 결과 — R23|Windows|flood(new)는 v5/v6에서, R6|Windows|ok(new)는 v6에서,
        // R23|Windows|muted(dismissed)는 v7에서 삭제되어 findings가 전부 빈다 (PR2 스펙 §4.6:
        // R23 룰 폐기로 dismissed 쿨다운 기록도 무의미해짐).
        assert!(keys.is_empty(), "v7 purges ALL R23 incl. dismissed (spec §4.6); R6 new purged by v6");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 6, "밀린 분기 전부 통과 (mv2 테스트 전례)");
    }

    #[test]
    fn migrate_v6_recollects_and_purges_repeat_junk() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv6.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 5;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);
                 INSERT INTO prompt_events (dedup_key, session_id, host, project_id,
                   source_file, source_offset, norm60, preview)
                 VALUES ('old-key', 's1', 'Windows', 'p', 'f.jsonl', 0, 'x', 'x');",
            ).unwrap();
            let store = SqliteStore { conn };
            let f = |rule: &str, key: &str| Finding {
                rule_id: rule.into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: format!("pattern:{key}"),
                evidence: serde_json::json!({}), est_tokens_saved: 0,
                prescription: None, dedup_key: key.into(),
            };
            store.upsert_finding(&f("R6", "R6|Windows|junk"), "2026-07-21T00:00:00Z").unwrap();
            // PR2: upsert가 R6을 pending으로 넣으므로, v6(오염된 'new' 카드 정화) 검증을 위해 new로 복원
            store.set_finding_status("R6|Windows|junk", "new", "2026-08-03T00:00:00Z").unwrap();
            store.upsert_finding(&f("R23", "R23|Windows|junk"), "2026-07-21T00:00:00Z").unwrap();
            store.upsert_finding(&f("R6", "R6|Windows|muted"), "2026-07-21T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|muted", "dismissed", "2026-08-03T00:00:00Z").unwrap();
            store.upsert_finding(&f("R1", "R1|Windows|keep"), "2026-07-21T00:00:00Z").unwrap();
        }
        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v6 분기 발화
        for table in ["ingest_state", "prompt_events", "events", "daily_rollup", "sessions"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table}는 재수집을 위해 비워져야 함");
        }
        let keys: Vec<String> = {
            let mut stmt = store.conn
                .prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap()
                .collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(keys, vec!["R1|Windows|keep".to_string(), "R6|Windows|muted".to_string()],
            "R6/R23 junk('new')만 삭제 — dismissed·타 룰은 보존");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 6, "v6 분기 통과 (v7 연쇄로 최종 7)");
        // 멱등: 다시 열어도 변화 없음
        drop(store);
        let store = SqliteStore::open(&db).unwrap();
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 6, "v6 분기 통과 (v7 연쇄로 최종 7)");
    }

    #[test]
    fn migrate_v7_purges_r23_and_repends_r6() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m7.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch("PRAGMA user_version = 6;").unwrap();
            let store = SqliteStore { conn };
            let f = |rule: &str, key: &str| Finding {
                rule_id: rule.into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "x".into(),
                evidence: serde_json::json!({}), est_tokens_saved: 0,
                prescription: None, dedup_key: key.into(),
            };
            store.upsert_finding(&f("R23", "R23|Windows|a"), "2026-07-21T00:00:00Z").unwrap();
            store.upsert_finding(&f("R23", "R23|Windows|muted"), "2026-07-21T00:00:00Z").unwrap();
            store.set_finding_status("R23|Windows|muted", "dismissed", "2026-08-03T00:00:00Z").unwrap();
            store.upsert_finding(&f("R6", "R6|Windows|active"), "2026-07-21T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|active", "new", "2026-08-03T00:00:00Z").unwrap(); // PR1 시대 노출 카드
            store.upsert_finding(&f("R6", "R6|Windows|kept"), "2026-07-21T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|kept", "dismissed", "2026-08-03T00:00:00Z").unwrap();
            store.upsert_finding(&f("R11", "R11|Windows|keep"), "2026-07-21T00:00:00Z").unwrap();
        }
        // 재오픈 → migrate 실행
        let store = SqliteStore::open(&path).unwrap();
        let count = |sql: &str| -> i64 { store.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM findings WHERE rule_id='R23'"), 0, "R23 전량 삭제(dismissed 포함)");
        let status = |k: &str| -> String {
            store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1",
                rusqlite::params![k], |r| r.get(0)).unwrap()
        };
        assert_eq!(status("R6|Windows|active"), "pending", "노출 R6은 판정 대상으로 되돌림");
        assert_eq!(status("R6|Windows|kept"), "dismissed", "R6 dismissed는 보존");
        assert_eq!(status("R11|Windows|keep"), "new", "무관 룰은 불변");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 7, "v7 분기 통과 (후속 분기 연쇄로 더 올라갈 수 있음)");
        // 멱등 — 재오픈해도 안전
        drop(store);
        let store2 = SqliteStore::open(&path).unwrap();
        let uv2: i64 = store2.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv2 >= 7, "v7 분기 통과 (후속 분기 연쇄로 더 올라갈 수 있음)");
    }

    #[test]
    fn migrate_v8_recollects_and_purges_command_invocation_r6() {
        // 스킬/커맨드 호출 프롬프트 제외(normalize) → prompt_events 재구축 + 오염 R6 'new' 정화.
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv8.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 7;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);
                 INSERT INTO prompt_events (dedup_key, session_id, host, project_id,
                   source_file, source_offset, norm60, preview)
                 VALUES ('old-key', 's1', 'Windows', 'p', 'f.jsonl', 0, 'x', 'x');",
            ).unwrap();
            let store = SqliteStore { conn };
            let f = |rule: &str, key: &str| Finding {
                rule_id: rule.into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: format!("pattern:{key}"),
                evidence: serde_json::json!({}), est_tokens_saved: 0,
                prescription: None, dedup_key: key.into(),
            };
            store.upsert_finding(&f("R6", "R6|Windows|cmd"), "2026-07-23T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|cmd", "new", "2026-08-03T00:00:00Z").unwrap(); // 스킬 호출로 뜬 오탐 카드
            store.upsert_finding(&f("R6", "R6|Windows|muted"), "2026-07-23T00:00:00Z").unwrap();
            store.set_finding_status("R6|Windows|muted", "dismissed", "2026-08-03T00:00:00Z").unwrap();
            store.upsert_finding(&f("R1", "R1|Windows|keep"), "2026-07-23T00:00:00Z").unwrap();
        }
        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v8 분기 발화
        for table in ["ingest_state", "prompt_events", "events", "daily_rollup", "sessions"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table}는 재수집을 위해 비워져야 함");
        }
        let keys: Vec<String> = {
            let mut stmt = store.conn
                .prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap()
                .collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(keys, vec!["R1|Windows|keep".to_string(), "R6|Windows|muted".to_string()],
            "R6 'new'만 삭제 — dismissed·타 룰은 보존");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 8, "v8 분기 통과");
    }

    #[test]
    fn translation_is_generated_once_per_item_and_survives_recuration() {
        // 스펙 §6.3 "아이템당 1회 + DB 캐시" — 같은 id를 다시 큐레이션해도 Engine을 부르지
        // 않는다. 호출 카운트 대신 **대상 목록이 비는지**로 검증한다(호출부가 이 목록만 돈다).
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |id: &str, tags: &[&str], title: &str| crate::content::ContentItem {
            id: id.into(),
            kind: crate::content::ItemKind::News,
            title: title.into(),
            body: "You can use up to 50% of your weekly usage limit.".into(),
            source_url: None,
            dimension: None,
            trigger_tags: tags.iter().map(|s| s.to_string()).collect(),
            base_priority: 0,
        };
        let ranked = vec![
            (mk("cc-announce-a", &["announcement"], "Fable 5 promo"), 320),
            // 한국어 소스는 번역 대상이 아니다 (§6.3 표: 내장 팁·팀 지식은 "그대로")
            (mk("T-L0-1", &[], "잔심부름엔 굳이 상위 모델 아니어도 돼요"), 500),
        ];
        store.replace_content_items(&ranked, "2026-08-03T00:00:00Z", &[]).unwrap();

        let pending = store.content_needing_translation(10).unwrap();
        assert_eq!(pending.len(), 1, "영어 소식만 번역 대상: {pending:?}");
        assert_eq!(pending[0].id, "cc-announce-a");

        store
            .set_content_translation(
                "cc-announce-a",
                Some("페이블 5 프로모"),
                "주간 사용 한도의 50%까지 쓸 수 있어요.",
                Some("2026-08-31"),
            )
            .unwrap();
        assert!(store.content_needing_translation(10).unwrap().is_empty(), "번역된 아이템 재호출 금지");

        // 재큐레이션(다음 스캔)이 캐시를 덮어쓰면 매 스캔 LLM을 다시 부르게 된다
        store.replace_content_items(&ranked, "2026-08-04T00:00:00Z", &[]).unwrap();
        assert!(
            store.content_needing_translation(10).unwrap().is_empty(),
            "재큐레이션이 번역 캐시를 지우면 안 된다"
        );

        let rows = store.list_content("2026-08-04T00:00:00Z", 14.0, false).unwrap();
        let row = rows.iter().find(|r| r.id == "cc-announce-a").unwrap();
        assert_eq!(row.summary_ko.as_deref(), Some("주간 사용 한도의 50%까지 쓸 수 있어요."));
        assert_eq!(row.title_ko.as_deref(), Some("페이블 5 프로모"));
        assert_eq!(row.deadline.as_deref(), Some("2026-08-31"));
        // 원문은 남는다 — 엔진 없는 사용자는 이 원문을 본다 (§6.3 "엔진 없으면 원문 그대로")
        assert_eq!(row.title, "Fable 5 promo");
    }

    #[test]
    fn announcements_are_notified_once_per_id() {
        // §6.5 "새 공지 id가 처음 감지되면 … 한 번만 알린다"
        let store = SqliteStore::open_in_memory().unwrap();
        let first = store
            .take_unnotified_announcements(&["a".to_string(), "b".to_string()])
            .unwrap();
        assert_eq!(first, vec!["a".to_string(), "b".to_string()]);
        assert!(
            store.take_unnotified_announcements(&["a".into(), "b".into()]).unwrap().is_empty(),
            "같은 id는 다시 알리지 않는다"
        );
        let second = store
            .take_unnotified_announcements(&["b".into(), "c".into()])
            .unwrap();
        assert_eq!(second, vec!["c".to_string()], "새 id만");
    }

    #[test]
    fn migrate_adds_content_translation_columns_without_recollect() {
        // 소식 파이프라인(⑥ 스펙 §6.3) — summary_ko·title_ko·deadline은 **컬럼만** 추가한다.
        // 릴리스마다 콜드 스캔이 되돌아온 사고(#146)를 반복하지 않도록, 수집 테이블의
        // 행 수가 보존되는지 테이블별로 명시 검증한다 (judgment_json 전례).
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mko.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            // 번역 컬럼이 없던 시절의 content_items로 되돌린다 (구 스키마 재현)
            conn.execute_batch(
                "DROP TABLE content_items;
                 CREATE TABLE content_items (
                   id TEXT PRIMARY KEY,
                   kind TEXT NOT NULL, dimension TEXT, title TEXT NOT NULL, body TEXT NOT NULL,
                   source_url TEXT, trigger_tags TEXT NOT NULL DEFAULT '[]',
                   score INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL DEFAULT 'new',
                   first_seen TEXT, last_seen TEXT
                 );
                 PRAGMA user_version = 8;
                 INSERT INTO content_items (id, kind, title, body) VALUES ('t1','tip','제목','본문');
                 INSERT INTO events (dedup_key, session_id, host, project_id, source_offset, kind)
                   VALUES ('e1','s1','Windows','p',0,'assistant_turn');
                 INSERT INTO sessions (session_id, host, project_id) VALUES ('s1','Windows','p');
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 42);
                 INSERT INTO daily_rollup (host, project_id, date) VALUES ('Windows','p','2026-08-02');",
            ).unwrap();
        }
        let store = SqliteStore::open(&db).unwrap(); // migrate 실행
        for table in ["events", "sessions", "ingest_state", "daily_rollup", "content_items"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 1, "{table} 행이 보존돼야 함 — 재수집 유발 금지(#146)");
        }
        for col in ["summary_ko", "title_ko", "deadline"] {
            let exists = store.conn
                .prepare(&format!(
                    "SELECT 1 FROM pragma_table_info('content_items') WHERE name='{col}'"
                )).unwrap()
                .exists([]).unwrap();
            assert!(exists, "content_items.{col} 이 추가돼야 함");
        }
    }

    #[test]
    fn migrate_v2_creates_prompt_events_and_recollects() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv2.db");
        // v1 시대 DB 시뮬레이션 — prompt_events 없음, user_version=1
        let v1_schema = SCHEMA.replace("CREATE TABLE IF NOT EXISTS prompt_events", "CREATE TABLE IF NOT EXISTS prompt_events_absent");
        assert!(v1_schema.contains("prompt_events_absent"), "prompt_events 치환 실패");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(&v1_schema).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 1;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 123);",
            ).unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v2 분기 발화

        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 2, "v2 분기를 지나야 함 (후속 정리 분기로 더 올라갈 수 있음)");
        let n: i64 = store.conn.query_row("SELECT COUNT(*) FROM ingest_state", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "prompt_events 백필을 위해 전체 재수집을 유도해야 함");
        let has_pe = store.conn
            .prepare("SELECT 1 FROM pragma_table_info('prompt_events') WHERE name='norm60'").unwrap()
            .exists([]).unwrap();
        assert!(has_pe, "prompt_events 테이블이 생성돼야 함");
    }

    #[test]
    fn migrate_v3_purges_r23_findings_without_recollect() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mv3.db");
        // v2 시대 DB — R23 특이 토큰 가드 도입 전에 쌓인 일반 루프 finding이 남아 있는 상태
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "PRAGMA user_version = 2;
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 123);",
            ).unwrap();
            let store = SqliteStore { conn };
            store.upsert_finding(&Finding {
                rule_id: "R23".into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:x".into(),
                evidence: serde_json::json!({"sequence": ["file-ops", "bash:npx", "bash:git"], "session_count": 3}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R23|Windows|x".into(),
            }, "2026-07-20T00:00:00Z").unwrap();
            store.upsert_finding(&Finding {
                rule_id: "R6".into(), severity: Severity::Suggest,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:ok".into(),
                evidence: serde_json::json!({"repeated_prompt": "매일 아침 판매 리포트 뽑아줘"}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R6|Windows|ok".into(),
            }, "2026-07-20T00:00:00Z").unwrap();
            // v6까지 연쇄 실행되면 status='new'인 R6/R23은 전량 정화 대상이라, 이 finding이
            // "R23만 삭제" 관찰을 견디려면 dismissed로 사용자 기록화해야 한다.
            store.set_finding_status("R6|Windows|ok", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v3 분기 발화 (이후 v6까지 연쇄)

        let keys: Vec<String> = {
            let mut stmt = store.conn.prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(keys, vec!["R6|Windows|ok".to_string()],
            "R23는 v3에서 즉시 삭제 — dismissed R6는 v6 연쇄까지 통과해도 보존");
        let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(uv >= 3, "v3 분기를 지나야 함 (후속 분기로 더 올라갈 수 있음)");
    }

    #[test]
    fn migrate_synthetic_prompt_purges_and_forces_recollect_once() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("msyn.db");
        // 마커 없는 기존 DB 시뮬레이션 — 스키마는 최신, settings에 마커만 부재
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(
                "INSERT INTO sessions (session_id, host, project_id, agent, first_prompt_preview)
                   VALUES ('s1','WSL:U','p','claude-code','<ide_opened_file>The user opened the file /home/j/a.md');
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 99);",
            ).unwrap();
            let store = SqliteStore { conn };
            // 오염 R6 finding → 삭제 대상 / 정상 R6 finding·타 룰 finding → 보존
            store.upsert_finding(&Finding {
                rule_id: "R6".into(), severity: Severity::Suggest,
                scope_host: Some("WSL:U".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:bad".into(),
                evidence: serde_json::json!({"repeated_prompt": "<ide_opened_file>The user opened", "session_count": 10}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R6|WSL:U|bad".into(),
            }, "2026-07-19T00:00:00Z").unwrap();
            store.upsert_finding(&Finding {
                rule_id: "R6".into(), severity: Severity::Suggest,
                scope_host: Some("WSL:U".into()), scope_project: None,
                scope_kind: "pattern".into(), scope_ref: "pattern:ok".into(),
                evidence: serde_json::json!({"repeated_prompt": "매일 아침 판매 리포트 뽑아줘", "session_count": 3}),
                est_tokens_saved: 0, prescription: None, dedup_key: "R6|WSL:U|ok".into(),
            }, "2026-07-19T00:00:00Z").unwrap();
            // v6까지 연쇄 실행되면 status='new'인 R6는 (합성 오염 여부와 무관히) 전량 정화
            // 대상이라, "정상 R6는 보존" 관찰을 견디려면 dismissed로 사용자 기록화해야 한다.
            store.set_finding_status("R6|WSL:U|ok", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — 마커 부재로 발화 (이후 v6까지 연쇄)

        // 전체 재수집 유도 + 오염 R6만 삭제
        for table in ["events", "sessions", "ingest_state", "daily_rollup"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} 은(는) 합성 프롬프트 마이그레이션 후 비워져야 함");
        }
        let keys: Vec<String> = {
            let mut stmt = store.conn.prepare("SELECT dedup_key FROM findings ORDER BY dedup_key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap()
        };
        assert_eq!(keys, vec!["R6|WSL:U|ok".to_string()],
            "합성 마커 오염 R6는 v1에서 삭제 — dismissed R6는 v6 연쇄까지 통과해도 보존");

        // 마커가 설정돼 두 번째 open은 재수집을 다시 유도하지 않는다
        store.conn.execute(
            "INSERT INTO ingest_state (source_file, last_offset) VALUES ('g.jsonl', 7)", [],
        ).unwrap();
        drop(store);
        let store2 = SqliteStore::open(&db).unwrap();
        let n: i64 = store2.conn
            .query_row("SELECT COUNT(*) FROM ingest_state", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "마커 설정 후에는 재수집이 반복되면 안 됨");
    }

    #[test]
    fn session_meta_and_user_prompt_route_to_sessions_not_events() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let base = |uuid: &str, off: u64, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-07T10:00:00Z".into()),
            source_file: "C:\\proj\\s1.jsonl".into(), source_offset: off, msg_id: None, kind,
        };
        store.upsert_events(&[
            base("m1", 0, EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: Some("main".into()) }),
            base("p1", 10, EventKind::UserPrompt { preview: "Run this exact Bash command".into(), is_command: false }),
            // 나중 프롬프트는 COALESCE로 무시돼야 함
            base("p2", 20, EventKind::UserPrompt { preview: "두 번째 프롬프트".into(), is_command: false }),
            base("a1", 30, EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0 }),
        ]).unwrap();

        // events엔 AssistantTurn 1건만
        assert_eq!(store.count_events().unwrap(), 1);
        let (cwd, gb, prev, pfile, poff): (Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>) =
            store.conn.query_row(
                "SELECT cwd, git_branch, first_prompt_preview, first_prompt_source_file, first_prompt_offset
                 FROM sessions WHERE session_id='s1'",
                [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
        assert_eq!(cwd.as_deref(), Some("D:\\Project\\cowork"));
        assert_eq!(gb.as_deref(), Some("main"));
        assert_eq!(prev.as_deref(), Some("Run this exact Bash command")); // 최초값 유지
        assert_eq!(pfile.as_deref(), Some("C:\\proj\\s1.jsonl"));
        assert_eq!(poff, Some(10));
    }

    #[test]
    fn ts_less_compaction_does_not_wipe_session_first_last_ts() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let base = |uuid: &str, off: u64, ts: Option<&str>, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: ts.map(|s| s.to_string()),
            source_file: "s1.jsonl".into(), source_offset: off, msg_id: None, kind,
        };
        // 1) ts 있는 이벤트로 first_ts/last_ts 설정
        store.upsert_events(&[
            base("a1", 0, Some("2026-07-01T10:00:00Z"), EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0 }),
        ]).unwrap();
        // 2) 같은 세션에 ts 없는 Compaction 이벤트 upsert → first_ts/last_ts는 훼손되면 안 됨
        store.upsert_events(&[
            base("c1", 1, None, EventKind::Compaction),
        ]).unwrap();

        let (first_ts, last_ts): (Option<String>, Option<String>) = store.conn.query_row(
            "SELECT first_ts, last_ts FROM sessions WHERE session_id='s1'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(first_ts.as_deref(), Some("2026-07-01T10:00:00Z"), "ts 없는 이벤트가 first_ts를 NULL로 훼손하면 안 됨");
        assert_eq!(last_ts.as_deref(), Some("2026-07-01T10:00:00Z"), "ts 없는 이벤트가 last_ts를 NULL로 훼손하면 안 됨");
    }

    #[test]
    fn daily_line_roundtrip_and_upsert_overwrites() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 없으면 None
        assert_eq!(store.get_daily_line("2026-07-08").unwrap(), None);
        // upsert 후 (text, fp) 라운드트립
        store.upsert_daily_line("2026-07-08", "오늘 좀 굴렀다.", "3|100|200|1").unwrap();
        assert_eq!(
            store.get_daily_line("2026-07-08").unwrap(),
            Some(("오늘 좀 굴렀다.".to_string(), "3|100|200|1".to_string()))
        );
        // 같은 날짜 재upsert → text·fp 덮어씀
        store.upsert_daily_line("2026-07-08", "생각보다 바빴네.", "5|300|400|2").unwrap();
        assert_eq!(
            store.get_daily_line("2026-07-08").unwrap(),
            Some(("생각보다 바빴네.".to_string(), "5|300|400|2".to_string()))
        );
        // 다른 날짜는 독립적으로 None
        assert_eq!(store.get_daily_line("2099-01-01").unwrap(), None);
    }

    #[test]
    fn chatter_pool_roundtrip_and_upsert_overwrites() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 없으면 None
        assert_eq!(store.get_chatter_pool("2026-07-10").unwrap(), None);
        // upsert 후 (lines, fp) 라운드트립 — JSON 직렬화 왕복
        let lines = vec!["오늘 좀 바빴네".to_string(), "커밋은 자주".to_string()];
        store.upsert_chatter_pool("2026-07-10", &lines, "3|100|200|1").unwrap();
        assert_eq!(
            store.get_chatter_pool("2026-07-10").unwrap(),
            Some((lines, "3|100|200|1".to_string()))
        );
        // 같은 날짜 재upsert → 덮어씀. 빈 풀(idle 캐시)도 왕복 가능
        store.upsert_chatter_pool("2026-07-10", &[], "0|0|0|2").unwrap();
        assert_eq!(
            store.get_chatter_pool("2026-07-10").unwrap(),
            Some((Vec::new(), "0|0|0|2".to_string()))
        );
        // 다른 날짜는 독립적으로 None
        assert_eq!(store.get_chatter_pool("2099-01-01").unwrap(), None);
    }

    #[test]
    fn chatter_pool_corrupt_json_falls_back_to_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO chatter_pool (date, lines, fingerprint) VALUES ('2026-07-10', 'not-json', '3|1|2|0')",
                [],
            )
            .unwrap();
        // 손상 JSON → 에러 아닌 빈 풀. fp는 보존 — 사실 변경 시 다음 스캔이 덮어씀
        assert_eq!(
            store.get_chatter_pool("2026-07-10").unwrap(),
            Some((Vec::new(), "3|1|2|0".to_string()))
        );
    }

    #[test]
    fn permission_mode_and_secret_flag_events_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |kind: EventKind, off: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: None, parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-19T10:00:00Z".into()),
            source_file: "s1.jsonl".into(), source_offset: off, msg_id: None, kind,
        };
        store.upsert_events(&[
            mk(EventKind::PermissionMode { mode: "plan".into() }, 0),
            mk(EventKind::SecretFlag { pattern_id: "github_token".into() }, 800),
        ]).unwrap();
        let (k1, t1): (String, String) = store.conn.query_row(
            "SELECT kind, tool_target FROM events WHERE kind='permission_mode'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((k1.as_str(), t1.as_str()), ("permission_mode", "plan"));
        let t2: String = store.conn.query_row(
            "SELECT tool_target FROM events WHERE kind='secret_flag'", [], |r| r.get(0)).unwrap();
        assert_eq!(t2, "github_token");
        // 멱등: 같은 이벤트 재삽입 시 dedup (uuid None → source_file:offset 키)
        let n = store.upsert_events(&[mk(EventKind::PermissionMode { mode: "plan".into() }, 0)]).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn migrate_v3_adds_subagent_files_and_forces_recollect() {
        use crate::finding::{Finding, Severity};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("m3.db");
        // v3 마이그레이션 전 스키마(subagent_files 없음)로 DB 선생성
        let v3_old_schema = SCHEMA.replace(
            ", subagent_files INTEGER NOT NULL DEFAULT 0",
            "",
        );
        assert!(v3_old_schema.len() < SCHEMA.len(), "v3 subagent_files 컬럼 치환 실패");

        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(&v3_old_schema).unwrap();
            conn.execute_batch(
                "INSERT INTO events (dedup_key, session_id, host, project_id, source_offset, kind)
                   VALUES ('old:0','s1','Windows','p',0,'assistant_turn');
                 INSERT INTO sessions (session_id, host, project_id, agent, first_ts, last_ts, git_branch)
                   VALUES ('s1','Windows','p','claude-code','2026-07-05T00:00:00Z','2026-07-05T00:00:00Z',NULL);
                 INSERT INTO ingest_state (source_file, last_offset) VALUES ('f.jsonl', 123);
                 INSERT INTO daily_rollup (host, project_id, date, session_count)
                   VALUES ('Windows','p','2026-07-05',1);
                 INSERT INTO diary_index (date, scope, path, tokens_used, engine)
                   VALUES ('2026-07-05','Windows','/diary.md',100,'claude-code');",
            ).unwrap();
        }
        // 마이그레이션 전 findings 심어서 보존 검증
        {
            let conn = Connection::open(&db).unwrap();
            let store = SqliteStore { conn };
            store.upsert_finding(&Finding {
                rule_id: "R1".into(), severity: Severity::Warn,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "host".into(), scope_ref: "srv".into(),
                evidence: serde_json::json!({}), est_tokens_saved: 10,
                prescription: None, dedup_key: "keepv3".into(),
            }, "2026-07-05T00:00:00Z").unwrap();
            store.set_finding_status("keepv3", "dismissed", "2026-08-03T00:00:00Z").unwrap();
        }

        let store = SqliteStore::open(&db).unwrap(); // migrate 실행 — v3 분기 발화

        // v3 컬럼이 실제로 생겼는지 확인
        let has_subagent_files = store.conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name='subagent_files'").unwrap()
            .exists([]).unwrap();
        assert!(has_subagent_files, "sessions.subagent_files 컬럼이 추가돼야 함");

        // 전체 재수집 유도 — events/sessions/ingest_state/daily_rollup 모두 비워짐
        for table in ["events", "sessions", "ingest_state", "daily_rollup"] {
            let n: i64 = store.conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{table} 은(는) v3 마이그레이션 후 비워져야 함");
        }

        // diary_index는 보존
        let diary_rows: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM diary_index", [], |r| r.get(0)).unwrap();
        assert_eq!(diary_rows, 1, "diary_index 는 v3 마이그레이션 후에도 보존돼야 함");

        // findings·status는 보존
        let rows = store.list_findings_current(true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].dedup_key, "keepv3");
        assert_eq!(rows[0].status, "dismissed");
    }

    #[test]
    fn replace_personal_skills_and_host_settings_roundtrip() {
        use crate::inventory::PersonalSkill;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_personal_skills("Windows", &[
            PersonalSkill { name: "gh-commit".into(), path: "C:\\u\\.claude\\skills\\gh-commit\\SKILL.md".into(),
                            body_chars: 300, scope: "user".into() },
        ]).unwrap();
        // replace: 다시 부르면 이전 행 대체
        store.replace_personal_skills("Windows", &[
            PersonalSkill { name: "deploy".into(), path: "D:\\proj\\.claude\\skills\\deploy\\SKILL.md".into(),
                            body_chars: 2400, scope: "project".into() },
        ]).unwrap();
        let (name, scope, chars): (String, String, i64) = store.conn.query_row(
            "SELECT name, scope, body_chars FROM personal_skill_inventory WHERE host='Windows'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((name.as_str(), scope.as_str(), chars), ("deploy", "project", 2400));

        store.replace_host_settings("Windows", Some("claude-fable-5[1m]"), Some("xhigh"), "2026-07-19T10:00:00Z").unwrap();
        store.replace_host_settings("Windows", Some("claude-sonnet-5"), None, "2026-07-19T11:00:00Z").unwrap(); // upsert
        let (m, e): (String, Option<String>) = store.conn.query_row(
            "SELECT default_model, effort_level FROM host_settings WHERE host='Windows'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(m, "claude-sonnet-5");
        assert_eq!(e, None);
    }

    #[test]
    fn session_cwds_returns_distinct_local_host_cwds() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.conn.execute_batch(
            "INSERT INTO sessions (session_id, host, project_id, cwd) VALUES
             ('s1','Windows','p','D:\\proj'), ('s2','Windows','p','D:\\proj'),
             ('s3','wsl:U','p','/home/x/proj'), ('s4','Windows','p',NULL);").unwrap();
        let cwds = store.session_cwds("Windows").unwrap();
        assert_eq!(cwds, vec!["D:\\proj".to_string()]); // distinct + host 필터 + NULL 제외
    }

    #[test]
    fn ingest_file_counts_subagent_transcripts() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(proj.join("abc").join("subagents")).unwrap();
        // 세션 jsonl + 서브에이전트 파일 2개 (+ jsonl 아닌 파일 1개는 미집계)
        let file = proj.join("abc.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type":"assistant","sessionId":"abc","uuid":"u1","timestamp":"2026-07-19T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#).unwrap();
        for name in ["agent-a.jsonl", "agent-b.jsonl"] {
            std::fs::File::create(proj.join("abc").join("subagents").join(name)).unwrap();
        }
        std::fs::File::create(proj.join("abc").join("subagents").join("note.txt")).unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        ingest_file(&store, &adapter, &file).unwrap();

        let n: i64 = store.conn.query_row(
            "SELECT subagent_files FROM sessions WHERE session_id='abc'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn ingest_file_without_subagent_dir_keeps_zero() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("solo.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type":"assistant","sessionId":"solo","uuid":"u1","timestamp":"2026-07-19T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#).unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        ingest_file(&store, &adapter, &file).unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT subagent_files FROM sessions WHERE session_id='solo'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    /// 논리 dedup 키 테스트용 이벤트 — 포크 복제본은 uuid·session·file이 다르고
    /// ts·message.id·tool_use_id가 보존된다 (2026-07-21 실데이터 검증).
    fn hygiene_ev(
        sess: &str, file: &str, uuid: &str, off: u64,
        msg_id: Option<&str>, kind: crate::model::EventKind,
    ) -> crate::model::NormalizedEvent {
        crate::model::NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-08T08:53:54.410Z".into()),
            source_file: file.into(), source_offset: off,
            msg_id: msg_id.map(Into::into), kind,
        }
    }

    #[test]
    fn resume_fork_copies_collapse_by_logical_identity() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage { input: 10, output: 1556, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        for (i, (sess, file)) in
            [("orig", "a.jsonl"), ("fork1", "b.jsonl"), ("fork2", "c.jsonl")].iter().enumerate()
        {
            store.upsert_events(&[
                hygiene_ev(sess, file, &format!("u{i}a"), 0, Some("msg_A"), turn()),
                hygiene_ev(sess, file, &format!("u{i}b"), 1, None, EventKind::ToolCall {
                    kind: ToolKind::from_raw_name("Bash"), raw_name: "Bash".into(),
                    target: Some("gh pr view".into()), tool_use_id: Some("toolu_X".into()),
                }),
                hygiene_ev(sess, file, &format!("u{i}c"), 2, None, EventKind::ToolResult {
                    tool_use_id: "toolu_X".into(), status: ResultStatus::Ok, result_len: 10,
                }),
            ]).unwrap();
        }
        let count = |k: &str| -> i64 {
            store.conn.query_row("SELECT COUNT(*) FROM events WHERE kind=?1",
                rusqlite::params![k], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("assistant_turn"), 1, "포크 복제 AssistantTurn은 1행");
        assert_eq!(count("tool_call"), 1, "포크 복제 ToolCall은 1행");
        assert_eq!(count("tool_result"), 1, "포크 복제 ToolResult는 1행");
        let out: i64 = store.conn.query_row(
            "SELECT COALESCE(SUM(tok_output),0) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(out, 1556, "토큰 이중 계산 금지");
    }

    #[test]
    fn multiline_assistant_message_counts_usage_once() {
        use crate::model::*;
        // 한 API 응답이 여러 assistant 라인으로 쪼개질 때 usage가 라인마다 반복된다
        // (실측: assistant 601줄 = 메시지 233개). AssistantTurn은 msg.id당 1행,
        // ToolCall은 tool_use_id가 블록마다 달라 전부 보존 (스펙 §3.1).
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage { input: 10, output: 500, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        let call = |tid: &str| EventKind::ToolCall {
            kind: ToolKind::from_raw_name("Read"), raw_name: "Read".into(),
            target: Some("a.rs".into()), tool_use_id: Some(tid.into()),
        };
        store.upsert_events(&[
            hygiene_ev("s1", "s1.jsonl", "u1", 0, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u2", 10, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u2t", 11, None, call("toolu_1")),
            hygiene_ev("s1", "s1.jsonl", "u3", 20, Some("msg_B"), turn()),
            hygiene_ev("s1", "s1.jsonl", "u3t", 21, None, call("toolu_2")),
        ]).unwrap();
        let turns: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='assistant_turn'", [], |r| r.get(0)).unwrap();
        let calls: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='tool_call'", [], |r| r.get(0)).unwrap();
        let out: i64 = store.conn.query_row(
            "SELECT COALESCE(SUM(tok_output),0) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(turns, 1, "같은 msg.id의 AssistantTurn은 1행");
        assert_eq!(calls, 2, "블록별 ToolCall은 전부 보존");
        assert_eq!(out, 500, "usage 반복은 1회만 합산");
    }

    #[test]
    fn assistant_without_message_id_falls_back_to_uuid_offset() {
        use crate::model::*;
        // <synthetic> 등 message.id 없는 라인은 기존 uuid:offset 규칙 유지
        let store = SqliteStore::open_in_memory().unwrap();
        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("<synthetic>"),
            usage: TokenUsage { input: 0, output: 0, cache_read: 0,
                                cache_creation: 0, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        };
        store.upsert_events(&[
            hygiene_ev("s1", "s1.jsonl", "u1", 0, None, turn()),
            hygiene_ev("s1", "s1.jsonl", "u2", 10, None, turn()),
        ]).unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE kind='assistant_turn'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "식별자 없으면 병합하지 않는다 (폴백)");
    }

    #[test]
    fn forked_prompt_copies_collapse_to_one_row() {
        use crate::model::*;
        // resume 포크: 같은 ts·내용의 프롬프트가 3개 세션 파일에 복제 → prompt_events 1행
        // (R6 "3개 세션" 부풀림의 근본 원인 — 스펙 §1.1-1)
        let store = SqliteStore::open_in_memory().unwrap();
        for (i, (sess, file)) in
            [("orig", "a.jsonl"), ("fork1", "b.jsonl"), ("fork2", "c.jsonl")].iter().enumerate()
        {
            store.upsert_events(&[hygiene_ev(sess, file, &format!("u{i}"), 0, None,
                EventKind::UserPrompt { preview: "그 배포 버전 사내망에 올린 것 맞는지 확인해줘".into(), is_command: false },
            )]).unwrap();
        }
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM prompt_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "포크 복제 프롬프트는 1행");
    }

    #[test]
    fn memory_crud_roundtrip() {
        use crate::memory::Memory;
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(store.count_memories().unwrap(), 0);

        let id1 = store.add_memory("주인은 비간을 먹지 않음", "chat").unwrap();
        let id2 = store.add_memory("목요일 오후는 회의로 바쁨", "manual").unwrap();
        assert!(id2 > id1);
        assert_eq!(store.count_memories().unwrap(), 2);

        // created_at ASC, id ASC 정렬
        let all = store.list_memories().unwrap();
        assert_eq!(
            all.iter().map(|m: &Memory| m.text.as_str()).collect::<Vec<_>>(),
            vec!["주인은 비간을 먹지 않음", "목요일 오후는 회의로 바쁨"]
        );
        assert_eq!(all[0].source, "chat");
        assert!(all[0].updated_at.is_none());

        store.update_memory(id1, "주인은 완전 채식(비건)임").unwrap();
        let updated = store.list_memories().unwrap();
        assert_eq!(updated[0].text, "주인은 완전 채식(비건)임");
        assert!(updated[0].updated_at.is_some());

        store.delete_memory(id2).unwrap();
        assert_eq!(store.count_memories().unwrap(), 1);
    }

    #[test]
    fn add_memory_rejects_blank() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.add_memory("   ", "chat").is_err());
        assert_eq!(store.count_memories().unwrap(), 0);
    }

    #[test]
    fn sessions_for_date_matches_rollup_membership_and_names_by_cwd_basename() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let ts = now.to_rfc3339();

        let ev = |sid: &str, off: u64, ts: &str, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "t".into(),
            host: "Windows".into(),
            project_id: "win:d:\\project\\space-a".into(),
            session_id: sid.into(),
            uuid: Some(format!("{sid}-{off}")),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some(ts.into()),
            source_file: "C:\\proj\\s.jsonl".into(),
            source_offset: off,
            msg_id: None,
            kind,
        };

        let turn = || EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage::default(),
            web_search: 0,
            web_fetch: 0,
        };
        // 어제 시작 → 오늘도 활동한 세션(자정 넘김). 오늘 버킷에 **잡혀야** 한다.
        let yesterday = (now - chrono::Duration::days(1)).to_rfc3339();

        store
            .upsert_events(&[
                // cwd 있는 세션 — 표시명은 basename
                ev(
                    "s1",
                    0,
                    &ts,
                    EventKind::SessionMeta { cwd: "D:\\Project\\space-a".into(), git_branch: None },
                ),
                ev(
                    "s1",
                    10,
                    &ts,
                    EventKind::UserPrompt {
                        preview: "PR138까지 머지했다".into(),
                        is_command: false,
                    },
                ),
                ev("s1", 20, &ts, turn()),
                // cwd 없는 세션 — project_id로 폴백. 프롬프트도 없음
                ev("s2", 0, &ts, turn()),
                // 다른 날짜 세션 — 오늘 버킷에 안 잡혀야 한다
                ev("s3", 0, "2026-01-02T03:04:05Z", turn()),
                // 이벤트가 없는(메타·프롬프트만) 세션 — daily_rollup에 안 잡히므로 목록에도 없어야 한다
                ev(
                    "s4",
                    0,
                    &ts,
                    EventKind::SessionMeta { cwd: "D:\\Project\\meta-only".into(), git_branch: None },
                ),
                // 자정 넘긴 세션: 시작은 어제, 오늘도 활동
                ev("s5", 0, &yesterday, turn()),
                ev("s5", 10, &ts, turn()),
            ])
            .unwrap();

        let rows = store.sessions_for_date(&today).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"s1") && ids.contains(&"s2"), "오늘 세션: {ids:?}");
        assert!(!ids.contains(&"s3"), "다른 날짜 세션이 섞였다: {ids:?}");
        assert!(!ids.contains(&"s4"), "이벤트 없는 세션은 요약에도 없으니 제외: {ids:?}");
        assert!(ids.contains(&"s5"), "자정 넘긴 세션이 빠졌다: {ids:?}");

        // 요약(daily_rollup)의 세션 수와 목록 길이가 일치해야 한다 — 같은 정의를 쓰므로.
        store.rebuild_rollup().unwrap();
        let summary = store.summary_for_date(&today).unwrap();
        assert_eq!(
            summary.session_count as usize, rows.len(),
            "요약 세션 수와 목록 길이 불일치: {} vs {}", summary.session_count, rows.len()
        );

        let s1 = rows.iter().find(|r| r.session_id == "s1").unwrap();
        assert_eq!(s1.project, "space-a", "cwd basename을 표시명으로");
        assert_eq!(s1.first_prompt.as_deref(), Some("PR138까지 머지했다"));

        let s2 = rows.iter().find(|r| r.session_id == "s2").unwrap();
        assert_eq!(s2.project, "win:d:\\project\\space-a", "cwd 없으면 project_id 폴백");
        assert!(s2.first_prompt.is_none());

        // 자정 넘긴 세션의 표시 시각은 **그날 첫 활동**이어야 한다(어제 23시가 아니라).
        let s5 = rows.iter().find(|r| r.session_id == "s5").unwrap();
        assert_eq!(
            &s5.first_ts[..10], &today,
            "그날 첫 활동 시각이어야 한다: {}", s5.first_ts
        );
    }
}
