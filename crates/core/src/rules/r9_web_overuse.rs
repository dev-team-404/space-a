use crate::finding::{Finding, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

/// R9 — 웹 도구 남용.
/// 세션당 서버측 웹 도구 호출(web_search+web_fetch) 과다를 지목.
/// advice-only(처방 없음): "캐싱/로컬"은 자동 적용 가능한 결정론적 액션이 아니므로
/// 정밀도의 선(코칭 설계 §1.4)상 처방 카드로 승격 부적합. R5처럼 evidence+finding_advice만.
pub struct R9WebOveruse {
    pub threshold: u64,
    pub heuristic_tokens_per_request: u64,
}

impl Default for R9WebOveruse {
    fn default() -> Self {
        R9WebOveruse { threshold: 15, heuristic_tokens_per_request: 2000 }
    }
}

impl Rule for R9WebOveruse {
    fn id(&self) -> &'static str {
        "R9"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id,
                    COALESCE(SUM(web_search),0) AS ws,
                    COALESCE(SUM(web_fetch),0) AS wf
             FROM events
             WHERE kind='assistant_turn'
             GROUP BY session_id
             HAVING ws + wf >= ?1
             ORDER BY ws + wf DESC",
        )?;
        let rows = stmt.query_map(params![self.threshold as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,                              // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(), // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(), // project
                r.get::<_, i64>(3)? as u64,                         // web_search
                r.get::<_, i64>(4)? as u64,                         // web_fetch
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, ws, wf) = row?;
            let total = ws + wf;
            out.push(Finding {
                rule_id: "R9".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({
                    "web_search": ws, "web_fetch": wf, "total_requests": total,
                    "note": "세션당 서버 웹 도구 호출 과다"
                }),
                est_tokens_saved: total * self.heuristic_tokens_per_request,
                prescription: None,
                dedup_key: format!("R9|{session}"),
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

    fn web_turn(session: &str, uuid: &str, ws: u32, wf: u32) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: ws, web_fetch: wf,
            },
        }
    }

    #[test]
    fn r9_flags_session_over_threshold() {
        let store = SqliteStore::open_in_memory().unwrap();
        // s1: web_search 10 + web_fetch 8 = 18 (>=15)
        store.upsert_events(&[
            web_turn("s1", "u1", 10, 3),
            web_turn("s1", "u2", 0, 5),
        ]).unwrap();
        let findings = R9WebOveruse::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R9");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.scope_kind, "session");
        assert_eq!(f.evidence["total_requests"], 18);
        assert_eq!(f.evidence["web_search"], 10);
        assert_eq!(f.evidence["web_fetch"], 8);
        assert_eq!(f.est_tokens_saved, 18 * 2000);
        assert!(f.prescription.is_none(), "R9는 advice-only");
    }

    #[test]
    fn r9_silent_under_threshold() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[web_turn("s1", "u1", 5, 5)]).unwrap(); // 10 < 15
        assert!(R9WebOveruse::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r9_scoped_per_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // s1: 8(<15), s2: 18(>=15) → s2만
        store.upsert_events(&[
            web_turn("s1", "u1", 8, 0),
            web_turn("s2", "u3", 18, 0),
        ]).unwrap();
        let findings = R9WebOveruse::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope_ref, "s2");
    }
}
