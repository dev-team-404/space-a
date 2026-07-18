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
  source_file TEXT, tool_use_id TEXT, result_status TEXT
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
  first_seen TEXT, last_seen TEXT, occurrences INTEGER DEFAULT 1
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
  first_seen TEXT, last_seen TEXT
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
"#;

pub struct SqliteStore {
    pub conn: Connection,
}

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
    Ok(())
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
        let mut inserted = 0usize;
        for e in evs {
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
                EventKind::UserPrompt { preview } => {
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
                    continue;
                }
                _ => {}
            }

            let dedup_key = match &e.uuid {
                Some(u) => format!("{}:{}", u, e.source_offset),
                None => format!("{}:{}", e.source_file, e.source_offset),
            };

            let (tool_use_id, result_status) = match &e.kind {
                EventKind::ToolCall { tool_use_id, .. } => (tool_use_id.clone(), None),
                EventKind::ToolResult { tool_use_id, status } =>
                    (Some(tool_use_id.clone()), Some(status.as_str().to_string())),
                _ => (None, None),
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
                  source_file, tool_use_id, result_status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)",
                params![
                    dedup_key, e.session_id, e.host, e.project_id, e.ts, e.source_offset as i64, kind_str,
                    mfam, mtier, mraw, ti, to, tcr, tcc, e1h, ws, wf,
                    tkind, tsrv, ttool, ttarget, raw, e.is_sidechain as i64,
                    e.source_file, tool_use_id, result_status
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
        self.conn.execute(
            "INSERT INTO findings
                (dedup_key, rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est_tokens_saved, prescription_json, status,
                 first_seen, last_seen, occurrences)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'new',?11,?11,1)
             ON CONFLICT(dedup_key) DO UPDATE SET
                last_seen = ?11,
                occurrences = occurrences + 1,
                est_tokens_saved = ?9,
                evidence_json = ?8,
                severity = ?3,
                prescription_json = ?10",
            params![
                f.dedup_key, f.rule_id, f.severity.as_str(), f.scope_host, f.scope_project,
                f.scope_kind, f.scope_ref, evidence, f.est_tokens_saved as i64, presc, now_ts
            ],
        )?;
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
    pub fn findings_for_date_all(&self, date: &str) -> Result<Vec<Finding>> {
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence_json, est_tokens_saved, prescription_json, dedup_key
             FROM findings f
             WHERE EXISTS (
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

    /// v2 이행: 특정 룰의 특정 스코프 finding 일괄 삭제 (예: R7 세션 스코프 폐기 — 스펙 §3).
    pub fn delete_findings_by_rule_and_scope(&self, rule_id: &str, scope_kind: &str) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM findings WHERE rule_id=?1 AND scope_kind=?2",
            params![rule_id, scope_kind],
        )?;
        Ok(n)
    }

    /// 큐레이션 콘텐츠를 현재 랭킹으로 upsert. findings 선례처럼 **사용자 status는 보존**
    /// (dismissed는 재스캔에도 유지 — 나깅 방지). 점수·본문·last_seen만 갱신.
    pub fn replace_content_items(
        &self,
        ranked: &[(crate::content::ContentItem, i64)],
        now_ts: &str,
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
        tx.commit()?;
        Ok(())
    }

    fn content_row_from(r: &rusqlite::Row) -> rusqlite::Result<ContentRow> {
        let tags_json: String = r.get(6)?;
        Ok(ContentRow {
            id: r.get(0)?, kind: r.get(1)?, dimension: r.get(2)?,
            title: r.get(3)?, body: r.get(4)?, source_url: r.get(5)?,
            trigger_tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            score: r.get(7)?, status: r.get(8)?,
            personal: None, // list_content에서 enrich_personal로 채움
        })
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
            let mut stmt = self.conn.prepare(
                "SELECT id,kind,dimension,title,body,source_url,trigger_tags,score,status
                 FROM content_items ORDER BY score DESC, id",
            )?;
            let rows = stmt.query_map([], Self::content_row_from)?;
            let out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            return Ok(self.enrich_personal(out));
        }
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,dimension,title,body,source_url,trigger_tags,score,status
             FROM content_items c
             WHERE status='new' AND score >= 0
               AND NOT EXISTS (
                 SELECT 1 FROM content_items d
                 WHERE d.status='dismissed' AND d.dimension IS NOT NULL
                   AND d.dimension IS c.dimension
                   AND julianday(?1) - julianday(d.last_seen) < ?2
               )
             ORDER BY score DESC, id",
        )?;
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

    /// status: 'new' | 'shown' | 'dismissed' (검증은 커맨드 층). dismiss 시 last_seen 갱신해
    /// 쿨다운 기준 시각으로 삼는다. 반환 = 해당 행 존재 여부.
    pub fn set_content_status(&self, id: &str, status: &str, now_ts: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE content_items SET status=?2, last_seen=?3 WHERE id=?1",
            params![id, status, now_ts],
        )?;
        Ok(n > 0)
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

    /// dedup_key로 단건 조회 (hub 재개 경로용).
    pub fn find_finding(&self, dedup_key: &str) -> Result<Option<FindingRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                        evidence_json, est_tokens_saved, prescription_json, dedup_key,
                        last_seen, occurrences, status
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
                    ))
                },
            )
            .optional()?;
        Ok(row.map(
            |(rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
              evidence_json, est, prescription_json, dedup_key, last_seen, occ, status)| {
                FindingRow {
                    rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                    evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
                    est_tokens_saved: est as u64,
                    prescription: prescription_json.and_then(|s| serde_json::from_str(&s).ok()),
                    dedup_key, last_seen,
                    occurrences: occ as u64,
                    status,
                }
            },
        ))
    }

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
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
    /// "당신 로그: …" — 사용자 실측 데이터로 접지한 근거 줄. list_content read 시점 계산.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personal: Option<String>,
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
        assert!(store.set_finding_status("R10|W|p", "dismissed").unwrap());

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
                kind: EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: None },
            },
            NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
                uuid: Some("p1".into()), parent_uuid: None, is_sidechain: false,
                ts: Some("2026-07-07T10:00:00Z".into()),
                source_file: "s1.jsonl".into(), source_offset: 10,
                kind: EventKind::UserPrompt { preview: "Run this exact Bash command".into() },
            },
        ]).unwrap();
        let (proj, _ts, cwd, prompt) = store.session_ctx("s1").unwrap().unwrap();
        assert_eq!(proj, "p");
        assert_eq!(cwd.as_deref(), Some("D:\\Project\\cowork"));
        assert_eq!(prompt.as_deref(), Some("Run this exact Bash command"));
        assert!(store.session_ctx("nope").unwrap().is_none());
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
            kind: EventKind::ToolResult { tool_use_id: "toolu_1".into(), status: ResultStatus::Denied },
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
            store.set_finding_status("keep", "dismissed").unwrap();
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
            store.set_finding_status("keep21", "dismissed").unwrap();
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

    #[test]
    fn session_meta_and_user_prompt_route_to_sessions_not_events() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let base = |uuid: &str, off: u64, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-07T10:00:00Z".into()),
            source_file: "C:\\proj\\s1.jsonl".into(), source_offset: off, kind,
        };
        store.upsert_events(&[
            base("m1", 0, EventKind::SessionMeta { cwd: "D:\\Project\\cowork".into(), git_branch: Some("main".into()) }),
            base("p1", 10, EventKind::UserPrompt { preview: "Run this exact Bash command".into() }),
            // 나중 프롬프트는 COALESCE로 무시돼야 함
            base("p2", 20, EventKind::UserPrompt { preview: "두 번째 프롬프트".into() }),
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
            source_file: "s1.jsonl".into(), source_offset: off, kind,
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
            source_file: "s1.jsonl".into(), source_offset: off, kind,
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
            store.set_finding_status("keepv3", "dismissed").unwrap();
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
}
