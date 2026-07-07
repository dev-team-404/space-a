use crate::finding::{Finding, Prescription, Severity};
use crate::model::{EventKind, NormalizedEvent, ToolKind};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY, host TEXT, project_id TEXT, agent TEXT,
  first_ts TEXT, last_ts TEXT, git_branch TEXT,
  cwd TEXT, first_prompt_preview TEXT, first_prompt_source_file TEXT, first_prompt_offset INTEGER
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
                   last_ts = MAX(COALESCE(last_ts, ?4), ?4),
                   first_ts = MIN(COALESCE(first_ts, ?4), ?4)",
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
                evidence_json = ?8",
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

    /// 오늘 모델별(raw id 기준, 구 데이터는 family 폴백) 토큰(입력+출력) 합. 내림차순.
    pub fn model_mix_for_date(&self, date: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model_raw, model_family) AS m,
                    COALESCE(SUM(tok_input),0) + COALESCE(SUM(tok_output),0) AS toks
             FROM events
             WHERE date(ts, 'localtime')=?1 AND COALESCE(model_raw, model_family) IS NOT NULL
             GROUP BY m ORDER BY toks DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 세션 컨텍스트(프로젝트, 시작 시각) — 세션 스코프 finding의 "어떤 작업인지" 표시용.
    pub fn session_ctx(&self, session_id: &str) -> Result<Option<(String, Option<String>)>> {
        self.conn
            .query_row(
                "SELECT project_id, first_ts FROM sessions WHERE session_id=?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
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

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
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
    store.set_offset(&file_key, new_offset)?;
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
        store.upsert_events(&[sess_turn("Windows", "p1", "s1", "u1", ts)]).unwrap();
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
}
