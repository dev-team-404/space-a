use crate::model::{EventKind, NormalizedEvent, ToolKind};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY, host TEXT, project_id TEXT, agent TEXT,
  first_ts TEXT, last_ts TEXT, git_branch TEXT
);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,
  session_id TEXT NOT NULL, host TEXT NOT NULL, project_id TEXT NOT NULL,
  ts TEXT, source_offset INTEGER NOT NULL, kind TEXT NOT NULL,
  model_family TEXT, model_tier TEXT,
  tok_input INTEGER DEFAULT 0, tok_output INTEGER DEFAULT 0,
  tok_cache_read INTEGER DEFAULT 0, tok_cache_create INTEGER DEFAULT 0,
  tok_eph_1h INTEGER DEFAULT 0, web_search INTEGER DEFAULT 0, web_fetch INTEGER DEFAULT 0,
  tool_kind TEXT, tool_server TEXT, tool_tool TEXT, tool_target TEXT, raw_name TEXT,
  is_sidechain INTEGER DEFAULT 0
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
CREATE TABLE IF NOT EXISTS diary_index (
  date TEXT NOT NULL, scope TEXT NOT NULL, path TEXT NOT NULL,
  tokens_used INTEGER DEFAULT 0, engine TEXT,
  PRIMARY KEY (date, scope)
);
CREATE TABLE IF NOT EXISTS ingest_state (
  source_file TEXT PRIMARY KEY, last_offset INTEGER NOT NULL DEFAULT 0, last_mtime INTEGER
);
"#;

pub struct SqliteStore {
    pub conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<SqliteStore> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn })
    }

    pub fn open_in_memory() -> Result<SqliteStore> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn })
    }

    pub fn upsert_events(&self, evs: &[NormalizedEvent]) -> Result<usize> {
        let mut inserted = 0usize;
        for e in evs {
            let dedup_key = e
                .uuid
                .clone()
                .unwrap_or_else(|| format!("{}:{}", e.source_file, e.source_offset));

            // 봉투 공통 + kind별 컬럼 추출
            let (kind_str, mfam, mtier, ti, to, tcr, tcc, e1h, ws, wf,
                 tkind, tsrv, ttool, ttarget, raw) = flatten(e);

            let n = self.conn.execute(
                "INSERT OR IGNORE INTO events
                 (dedup_key, session_id, host, project_id, ts, source_offset, kind,
                  model_family, model_tier, tok_input, tok_output, tok_cache_read,
                  tok_cache_create, tok_eph_1h, web_search, web_fetch,
                  tool_kind, tool_server, tool_tool, tool_target, raw_name, is_sidechain)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
                params![
                    dedup_key, e.session_id, e.host, e.project_id, e.ts, e.source_offset as i64, kind_str,
                    mfam, mtier, ti, to, tcr, tcc, e1h, ws, wf,
                    tkind, tsrv, ttool, ttarget, raw, e.is_sidechain as i64
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
}

type FlatRow = (
    String, Option<String>, Option<String>, i64, i64, i64, i64, i64, i64, i64,
    Option<String>, Option<String>, Option<String>, Option<String>, Option<String>,
);

fn flatten(e: &NormalizedEvent) -> FlatRow {
    match &e.kind {
        EventKind::AssistantTurn { model, usage, web_search, web_fetch } => (
            "assistant_turn".into(),
            Some(format!("{:?}", model.family).to_lowercase()),
            Some(format!("{:?}", model.tier).to_lowercase()),
            usage.input as i64, usage.output as i64, usage.cache_read as i64,
            usage.cache_creation as i64, usage.eph_1h as i64,
            *web_search as i64, *web_fetch as i64,
            None, None, None, None, None,
        ),
        EventKind::ToolCall { kind, raw_name, target } => {
            let (tkind, tsrv, ttool) = match kind {
                ToolKind::McpCall { server, tool } => (
                    "mcp_call".to_string(),
                    Some(server.clone()),
                    Some(tool.clone()),
                ),
                other => (tool_kind_str(other).to_string(), None, None),
            };
            (
                "tool_call".into(), None, None, 0, 0, 0, 0, 0, 0, 0,
                Some(tkind), tsrv, ttool, target.clone(), Some(raw_name.clone()),
            )
        }
        EventKind::SessionMeta { .. } => (
            "session_meta".into(), None, None, 0, 0, 0, 0, 0, 0, 0,
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
}
