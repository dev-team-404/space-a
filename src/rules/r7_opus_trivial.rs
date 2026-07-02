use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;

/// R7 — 단순 작업에 Opus (세션 단위).
///
/// v0 이탈(스펙 §5는 턴 단위): `events`에 uuid 컬럼이 없어 턴↔도구 조인이 불가하므로
/// 세션 단위로 근사한다. 조건 3개 모두 충족 시 지목:
///   1) Opus 전용(opus turn ≥1, non-opus turn 0)
///   2) 가벼운 출력(SUM(tok_output) < max_output_tokens)
///   3) 사소한 도구만(tool_call 1..=max, 무거운 도구 0, 서버 웹 0)
/// est_tokens_saved는 비용등가(Opus↔Haiku 5:1 균일 가격비 → 20% 비용 → 80% 절감).
pub struct R7OpusTrivial {
    pub max_output_tokens: u64,
    pub max_tool_calls: u64,
    pub savings_fraction_pct: u64,
}

impl Default for R7OpusTrivial {
    fn default() -> Self {
        R7OpusTrivial { max_output_tokens: 700, max_tool_calls: 5, savings_fraction_pct: 80 }
    }
}

impl Rule for R7OpusTrivial {
    fn id(&self) -> &'static str {
        "R7"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id,
                    SUM(CASE WHEN kind='assistant_turn' AND model_family='opus' THEN 1 ELSE 0 END) AS opus_turns,
                    SUM(CASE WHEN kind='assistant_turn' AND model_family IS NOT NULL AND model_family<>'opus' THEN 1 ELSE 0 END) AS non_opus,
                    COALESCE(SUM(tok_output),0) AS out_tok,
                    COALESCE(SUM(tok_input),0)+COALESCE(SUM(tok_output),0)+COALESCE(SUM(tok_cache_read),0)+COALESCE(SUM(tok_cache_create),0) AS billable,
                    COALESCE(SUM(web_search),0)+COALESCE(SUM(web_fetch),0) AS web_reqs,
                    SUM(CASE WHEN kind='tool_call' THEN 1 ELSE 0 END) AS tool_calls,
                    SUM(CASE WHEN kind='tool_call' AND tool_kind IN ('sub_agent','mcp_call','web_search','web_fetch') THEN 1 ELSE 0 END) AS heavy,
                    GROUP_CONCAT(DISTINCT CASE WHEN kind='tool_call' THEN tool_kind END) AS tool_kinds
             FROM events
             GROUP BY session_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,                               // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),  // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),  // project
                r.get::<_, i64>(3)? as u64,                          // opus_turns
                r.get::<_, i64>(4)? as u64,                          // non_opus
                r.get::<_, i64>(5)? as u64,                          // out_tok
                r.get::<_, i64>(6)? as u64,                          // billable
                r.get::<_, i64>(7)? as u64,                          // web_reqs
                r.get::<_, i64>(8)? as u64,                          // tool_calls
                r.get::<_, i64>(9)? as u64,                          // heavy
                r.get::<_, Option<String>>(10)?.unwrap_or_default(), // tool_kinds csv
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, opus_turns, non_opus, out_tok,
                 billable, web_reqs, tool_calls, heavy, tool_kinds_csv) = row?;

            // 1) Opus 전용
            if opus_turns == 0 || non_opus > 0 {
                continue;
            }
            // 2) 가벼운 출력
            if out_tok >= self.max_output_tokens {
                continue;
            }
            // 3) 사소한 도구만
            if tool_calls < 1 || tool_calls > self.max_tool_calls {
                continue;
            }
            if heavy > 0 || web_reqs > 0 {
                continue;
            }

            let mut tools: Vec<String> = tool_kinds_csv
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            tools.sort();

            let est = (billable * self.savings_fraction_pct + 50) / 100;

            out.push(Finding {
                rule_id: "R7".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({
                    "model": "opus",
                    "turns": opus_turns,
                    "tok_output": out_tok,
                    "tool_calls": tool_calls,
                    "tools": tools,
                    "billable_tokens": billable,
                    "note": "비용-등가 추정(Opus↔Haiku 5:1 가격비)"
                }),
                est_tokens_saved: est,
                prescription: Some(Prescription {
                    kind: "switch_model".into(),
                    payload: serde_json::json!({ "from": "opus", "to": "haiku" }),
                }),
                dedup_key: format!("R7|{session}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn turn(session: &str, uuid: &str, model: &str, output: u64, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { output, cache_creation: cache_create, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn tool(session: &str, uuid: &str, kind: ToolKind, raw: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::ToolCall { kind, raw_name: raw.into(), target: None },
        }
    }

    #[test]
    fn r7_flags_all_opus_trivial_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // Opus 전용, 출력 400(<700), 상주 60000, 도구 2개(file_read/file_edit)
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 300, 60000),
            turn("s1", "u2", "claude-opus-4-8", 100, 0),
            tool("s1", "u3", ToolKind::FileRead, "Read"),
            tool("s1", "u4", ToolKind::FileEdit, "Edit"),
        ]).unwrap();

        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R7");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.scope_kind, "session");
        assert_eq!(f.prescription.as_ref().unwrap().kind, "switch_model");
        assert_eq!(f.evidence["model"], "opus");
        assert_eq!(f.evidence["tok_output"], 400);
        assert_eq!(f.evidence["tool_calls"], 2);
        assert_eq!(f.evidence["billable_tokens"], 60400);
        // billable 60400 × 0.8 = 48320
        assert_eq!(f.est_tokens_saved, 48320);
        let tools = f.evidence["tools"].as_array().unwrap();
        assert_eq!(tools[0], "file_edit"); // 정렬됨
        assert_eq!(tools[1], "file_read");
    }

    #[test]
    fn r7_silent_when_mixed_model() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 100, 0),
            turn("s1", "u2", "claude-sonnet-4-6", 100, 0),
            tool("s1", "u3", ToolKind::FileRead, "Read"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_heavy_tool() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 100, 0),
            tool("s1", "u2", ToolKind::McpCall { server: "ctx".into(), tool: "x".into() }, "mcp__ctx__x"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_server_web_used() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut t = turn("s1", "u1", "claude-opus-4-8", 100, 0);
        if let EventKind::AssistantTurn { web_search, .. } = &mut t.kind {
            *web_search = 2;
        }
        store.upsert_events(&[t, tool("s1", "u2", ToolKind::FileRead, "Read")]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_no_tool() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 순수 대화형 Opus 턴(도구 0) → 짧지만 깊은 답변일 수 있어 제외
        store.upsert_events(&[turn("s1", "u1", "claude-opus-4-8", 100, 60000)]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_output_too_large() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 800, 0), // 800 >= 700
            tool("s1", "u2", ToolKind::FileRead, "Read"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }
}
