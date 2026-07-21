//! 세션별 요약 통계와 자동화 버스트 판별의 단일 공급원 (코칭 v2 스펙 §4.1).
//! R10(버스트)·R7v2(잔심부름 비율)·R12(스킬 미활용)가 공유한다.

use crate::store::SqliteStore;
use anyhow::Result;
use chrono::DateTime;

/// 버스트 판별 상수 (스펙 §4.2 — 보수적 오탐선, 합의됨)
pub const BURST_MAX_TURNS: u64 = 3;
pub const BURST_MIN_SESSIONS: usize = 5;
pub const BURST_MAX_GAP_SECS: i64 = 600;

#[derive(Debug, Clone)]
pub struct SessionStat {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub assistant_turns: u64,
    pub opus_turns: u64,
    pub non_opus_turns: u64,
    pub tok_output: u64,
    pub billable: u64,
    pub tool_calls: u64,
    pub heavy_tools: u64,
    pub web_reqs: u64,
    pub file_edits: u64,
    pub skill_calls: u64,
    pub temp_target_hits: u64,
    pub model_raws: Vec<String>,
}

impl SessionStat {
    /// R7 세션 판정 기준 (스펙 §4.3 — 현행 유지): Opus 전용 + 가벼운 출력 + 사소한 도구만.
    pub fn is_light_opus(&self, max_output_tokens: u64, max_tool_calls: u64) -> bool {
        self.opus_turns >= 1
            && self.non_opus_turns == 0
            && self.tok_output < max_output_tokens
            && self.tool_calls >= 1
            && self.tool_calls <= max_tool_calls
            && self.heavy_tools == 0
            && self.web_reqs == 0
    }
}

pub fn collect_session_stats(store: &SqliteStore) -> Result<Vec<SessionStat>> {
    let mut stmt = store.conn.prepare(
        "SELECT session_id,
                COALESCE(MAX(host),'') AS host,
                COALESCE(MAX(project_id),'') AS project,
                MIN(ts) AS first_ts, MAX(ts) AS last_ts,
                SUM(CASE WHEN kind='assistant_turn' THEN 1 ELSE 0 END) AS turns,
                SUM(CASE WHEN kind='assistant_turn' AND model_family='opus' THEN 1 ELSE 0 END) AS opus_turns,
                SUM(CASE WHEN kind='assistant_turn' AND model_family IS NOT NULL AND model_family<>'opus' THEN 1 ELSE 0 END) AS non_opus,
                COALESCE(SUM(tok_output),0) AS out_tok,
                COALESCE(SUM(tok_input),0)+COALESCE(SUM(tok_output),0)+COALESCE(SUM(tok_cache_read),0)+COALESCE(SUM(tok_cache_create),0) AS billable,
                SUM(CASE WHEN kind='tool_call' THEN 1 ELSE 0 END) AS tool_calls,
                SUM(CASE WHEN kind='tool_call' AND tool_kind IN ('sub_agent','mcp_call','web_search','web_fetch') THEN 1 ELSE 0 END) AS heavy,
                COALESCE(SUM(web_search),0)+COALESCE(SUM(web_fetch),0) AS web_reqs,
                SUM(CASE WHEN kind='tool_call' AND tool_kind IN ('file_edit','file_write') THEN 1 ELSE 0 END) AS file_edits,
                SUM(CASE WHEN kind='tool_call' AND tool_kind='skill' THEN 1 ELSE 0 END) AS skill_calls,
                SUM(CASE WHEN kind='tool_call' AND (
                    tool_target LIKE '%/tmp/%' OR tool_target LIKE '%/temp/%'
                    OR tool_target LIKE '%\\tmp\\%' OR tool_target LIKE '%\\temp\\%'
                ) THEN 1 ELSE 0 END) AS temp_hits,
                GROUP_CONCAT(DISTINCT model_raw) AS model_raws
         FROM events
         GROUP BY session_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SessionStat {
            session_id: r.get(0)?,
            host: r.get(1)?,
            project_id: r.get(2)?,
            first_ts: r.get(3)?,
            last_ts: r.get(4)?,
            assistant_turns: r.get::<_, i64>(5)? as u64,
            opus_turns: r.get::<_, i64>(6)? as u64,
            non_opus_turns: r.get::<_, i64>(7)? as u64,
            tok_output: r.get::<_, i64>(8)? as u64,
            billable: r.get::<_, i64>(9)? as u64,
            tool_calls: r.get::<_, i64>(10)? as u64,
            heavy_tools: r.get::<_, i64>(11)? as u64,
            web_reqs: r.get::<_, i64>(12)? as u64,
            file_edits: r.get::<_, i64>(13)? as u64,
            skill_calls: r.get::<_, i64>(14)? as u64,
            temp_target_hits: r.get::<_, i64>(15)? as u64,
            model_raws: r
                .get::<_, Option<String>>(16)?
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

#[derive(Debug, Clone)]
pub struct BurstGroup {
    pub host: String,
    pub project_id: String,
    /// 시간순(first_ts 오름차순)
    pub sessions: Vec<SessionStat>,
}

fn parse_ts(s: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// 스펙 §4.2 보수 판별: 같은 (host, project)에서 턴 ≤3 세션이 5건+,
/// 이웃 간격(이전 last_ts → 다음 first_ts) ≤10분 체인. ts 없는 세션 제외.
pub fn detect_bursts(stats: &[SessionStat]) -> Vec<BurstGroup> {
    use std::collections::BTreeMap;
    let mut by_proj: BTreeMap<(String, String), Vec<&SessionStat>> = BTreeMap::new();
    for s in stats {
        if s.assistant_turns > BURST_MAX_TURNS {
            continue;
        }
        let (Some(f), Some(l)) = (s.first_ts.as_deref(), s.last_ts.as_deref()) else { continue };
        if parse_ts(f).is_none() || parse_ts(l).is_none() {
            continue;
        }
        by_proj.entry((s.host.clone(), s.project_id.clone())).or_default().push(s);
    }

    let mut out = Vec::new();
    for ((host, project_id), mut sessions) in by_proj {
        sessions.sort_by(|a, b| a.first_ts.cmp(&b.first_ts));
        let mut chain: Vec<&SessionStat> = Vec::new();
        let flush = |chain: &mut Vec<&SessionStat>, out: &mut Vec<BurstGroup>| {
            if chain.len() >= BURST_MIN_SESSIONS {
                out.push(BurstGroup {
                    host: host.clone(),
                    project_id: project_id.clone(),
                    sessions: chain.iter().map(|s| (*s).clone()).collect(),
                });
            }
            chain.clear();
        };
        for s in sessions {
            if let Some(prev) = chain.last() {
                let gap = match (
                    prev.last_ts.as_deref().and_then(parse_ts),
                    s.first_ts.as_deref().and_then(parse_ts),
                ) {
                    (Some(a), Some(b)) => (b - a).num_seconds(),
                    _ => i64::MAX,
                };
                if gap > BURST_MAX_GAP_SECS {
                    flush(&mut chain, &mut out);
                }
            }
            chain.push(s);
        }
        flush(&mut chain, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn turn_at(session: &str, uuid: &str, model: &str, output: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { output, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn tool_at(session: &str, uuid: &str, kind: ToolKind, raw: &str, target: Option<&str>, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::ToolCall { kind, raw_name: raw.into(), target: target.map(String::from), tool_use_id: None },
        }
    }

    /// 버스트 양성: 초단기(턴1) 세션 5건, 간격 5분
    fn burst_events() -> Vec<NormalizedEvent> {
        let mut evs = Vec::new();
        for i in 0..5 {
            let sid = format!("b{i}");
            let ts = format!("2026-07-06T10:{:02}:00Z", i * 5); // 10:00, 10:05, ...
            evs.push(turn_at(&sid, &format!("u{i}"), "claude-opus-4-8", 100, &ts));
            evs.push(tool_at(&sid, &format!("t{i}"), ToolKind::FileRead, "Read",
                Some("C:\\Users\\x\\AppData\\Local\\Temp\\probe.txt"), &ts));
        }
        evs
    }

    #[test]
    fn collect_stats_aggregates_per_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn_at("s1", "u1", "claude-opus-4-8", 300, "2026-07-06T09:00:00Z"),
            turn_at("s1", "u2", "claude-sonnet-4-6", 100, "2026-07-06T09:05:00Z"),
            tool_at("s1", "u3", ToolKind::FileEdit, "Edit", Some("a.rs"), "2026-07-06T09:06:00Z"),
            tool_at("s1", "u4", ToolKind::Skill { name: "superpowers:brainstorming".into() },
                "Skill", Some("superpowers:brainstorming"), "2026-07-06T09:07:00Z"),
        ]).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert_eq!(stats.len(), 1);
        let s = &stats[0];
        assert_eq!(s.session_id, "s1");
        assert_eq!(s.assistant_turns, 2);
        assert_eq!(s.opus_turns, 1);
        assert_eq!(s.non_opus_turns, 1);
        assert_eq!(s.tok_output, 400);
        assert_eq!(s.tool_calls, 2);
        assert_eq!(s.file_edits, 1);
        assert_eq!(s.skill_calls, 1);
        assert_eq!(s.first_ts.as_deref(), Some("2026-07-06T09:00:00Z"));
        assert_eq!(s.model_raws.len(), 2); // opus + sonnet
    }

    #[test]
    fn is_light_opus_replicates_r7_session_criteria() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn_at("s1", "u1", "claude-opus-4-8", 400, "2026-07-06T09:00:00Z"),
            tool_at("s1", "u2", ToolKind::FileRead, "Read", Some("a.md"), "2026-07-06T09:01:00Z"),
        ]).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert!(stats[0].is_light_opus(700, 5));
        assert!(!stats[0].is_light_opus(300, 5)); // 출력 임계 미달
    }

    #[test]
    fn detect_bursts_positive_five_short_sessions_close_gaps() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&burst_events()).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        let bursts = detect_bursts(&stats);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].sessions.len(), 5);
        assert_eq!(bursts[0].project_id, "d--proj");
        assert!(bursts[0].sessions[0].temp_target_hits >= 1); // temp 가산 신호 수집됨
    }

    #[test]
    fn detect_bursts_negative_only_four_sessions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let evs: Vec<_> = burst_events().into_iter()
            .filter(|e| e.session_id != "b4").collect();
        store.upsert_events(&evs).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert!(detect_bursts(&stats).is_empty());
    }

    #[test]
    fn detect_bursts_negative_wide_gaps() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        for i in 0..5 {
            let sid = format!("g{i}");
            let ts = format!("2026-07-06T1{}:00:00Z", i); // 1시간 간격
            evs.push(turn_at(&sid, &format!("u{i}"), "claude-opus-4-8", 100, &ts));
        }
        store.upsert_events(&evs).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert!(detect_bursts(&stats).is_empty());
    }

    #[test]
    fn detect_bursts_negative_many_turns() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        for i in 0..5 {
            let sid = format!("m{i}");
            let base = format!("2026-07-06T10:{:02}", i * 5);
            for t in 0..4 { // 턴 4개 > BURST_MAX_TURNS(3)
                evs.push(turn_at(&sid, &format!("u{i}-{t}"), "claude-opus-4-8", 100,
                    &format!("{base}:{:02}Z", t * 10)));
            }
        }
        store.upsert_events(&evs).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert!(detect_bursts(&stats).is_empty());
    }

    #[test]
    fn detect_bursts_excludes_sessions_without_ts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = burst_events();
        for e in evs.iter_mut().filter(|e| e.session_id == "b0") { e.ts = None; }
        store.upsert_events(&evs).unwrap();
        let stats = collect_session_stats(&store).unwrap();
        assert!(detect_bursts(&stats).is_empty()); // ts 없는 b0 제외 → 4건뿐
    }
}
