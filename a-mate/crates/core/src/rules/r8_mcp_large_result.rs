//! R8 — MCP 대형 결과 (유예 승격, 2026-07-19). tool_result 상태에 이어 **결과 크기**(result_len)를
//! 수집하기 시작하면서 승격 가능. "이 MCP 서버는 매 호출마다 큰 결과를 가져와 컨텍스트를 크게 소모한다"는
//! 실제 토큰 낭비 신호 — 곧 압축돼 사라질 때가 많다.
//!
//! 정밀도의 선: 판별은 결정론(서버별 큰 결과 횟수·총량). est_tokens_saved는 근거 없는 값 금지 →
//! 0(가치 제안형). 대신 evidence에 **측정된** 총 문자수/평균/최대와 근사 토큰을 담아 서사가 인용한다.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;

pub struct R8McpLargeResult {
    /// "큰 결과"로 볼 문자 수 문턱 (기본 8000자 ≈ ~2000토큰)
    pub char_threshold: u64,
    /// 발화에 필요한 큰-결과 최소 횟수
    pub min_calls: u64,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R8McpLargeResult {
    fn default() -> Self {
        R8McpLargeResult { char_threshold: 8000, min_calls: 3, days: 14 }
    }
}

/// 문자 수 → 근사 토큰 (영문 ~4자/토큰의 보수적 근사).
fn approx_tokens(chars: u64) -> u64 {
    chars / 4
}

impl Rule for R8McpLargeResult {
    fn id(&self) -> &'static str {
        "R8"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let rows = store.mcp_large_results(self.char_threshold, self.min_calls, &since)?;

        let mut out = Vec::new();
        for (server, n, total_chars, max_chars) in rows {
            let avg_chars = total_chars / n.max(1);
            out.push(Finding {
                rule_id: "R8".into(),
                severity: Severity::Suggest,
                scope_host: None,
                scope_project: None,
                scope_kind: "mcp".into(),
                scope_ref: format!("mcp:{server}"),
                evidence: serde_json::json!({
                    "server": server,
                    "large_result_count": n,
                    "avg_chars": avg_chars,
                    "max_chars": max_chars,
                    "approx_tokens_total": approx_tokens(total_chars),
                    "approx_tokens_avg": approx_tokens(avg_chars),
                    "char_threshold": self.char_threshold,
                    "window_days": self.days,
                }),
                est_tokens_saved: 0, // 실제 절약은 사용자가 얼마나 좁히냐에 달림 — 수치 강변 금지
                prescription: Some(Prescription {
                    kind: "narrow_mcp_result".into(),
                    payload: serde_json::json!({ "server": server }),
                }),
                dedup_key: format!("R8|{server}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, NormalizedEvent, ResultStatus, ToolKind};

    fn mcp_call_result(sess: &str, i: u64, server: &str, result_len: u64, ts: &str) -> Vec<NormalizedEvent> {
        let raw = format!("mcp__{server}__query");
        let tuid = format!("{sess}-tu{i}");
        let base = |uuid: String, off: u64, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: sess.into(),
            uuid: Some(uuid), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()), source_file: "s.jsonl".into(), source_offset: off, kind,
        };
        vec![
            base(
                format!("{sess}-c{i}"), i * 2,
                EventKind::ToolCall {
                    kind: ToolKind::from_raw_name(&raw), raw_name: raw.clone(),
                    target: None, tool_use_id: Some(tuid.clone()),
                },
            ),
            base(
                format!("{sess}-r{i}"), i * 2 + 1,
                EventKind::ToolResult { tool_use_id: tuid, status: ResultStatus::Ok, result_len },
            ),
        ]
    }

    #[test]
    fn r8_fires_for_server_with_repeated_large_results() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let mut evs = Vec::new();
        // context7 서버: 큰 결과 3회 (12000자)
        for i in 0..3 {
            evs.extend(mcp_call_result("s1", i, "context7", 12000, &now));
        }
        // 작은 결과 서버: 무시돼야
        evs.extend(mcp_call_result("s1", 9, "tinysrv", 200, &now));
        store.upsert_events(&evs).unwrap();

        let findings = R8McpLargeResult::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R8");
        assert_eq!(f.evidence["server"], "context7");
        assert_eq!(f.evidence["large_result_count"], 3);
        assert_eq!(f.evidence["avg_chars"], 12000);
        assert_eq!(f.evidence["approx_tokens_avg"], 3000);
        assert_eq!(f.est_tokens_saved, 0);
        assert_eq!(f.dedup_key, "R8|context7");
    }

    #[test]
    fn r8_silent_below_min_calls() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let mut evs = Vec::new();
        for i in 0..2 {
            // 2회뿐 — 문턱 미달
            evs.extend(mcp_call_result("s1", i, "context7", 12000, &now));
        }
        store.upsert_events(&evs).unwrap();
        assert!(R8McpLargeResult::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r8_ignores_non_mcp_large_results() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        // 일반 Bash 큰 결과 3회 — tool_server 없음 → 무시
        let mut evs = Vec::new();
        for i in 0..3 {
            let tuid = format!("b-tu{i}");
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
                uuid: Some(format!("b-c{i}")), parent_uuid: None, is_sidechain: false,
                ts: Some(now.clone()), source_file: "s.jsonl".into(), source_offset: i * 2,
                kind: EventKind::ToolCall {
                    kind: ToolKind::Execute, raw_name: "Bash".into(),
                    target: None, tool_use_id: Some(tuid.clone()),
                },
            });
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
                uuid: Some(format!("b-r{i}")), parent_uuid: None, is_sidechain: false,
                ts: Some(now.clone()), source_file: "s.jsonl".into(), source_offset: i * 2 + 1,
                kind: EventKind::ToolResult { tool_use_id: tuid, status: ResultStatus::Ok, result_len: 20000 },
            });
        }
        store.upsert_events(&evs).unwrap();
        assert!(R8McpLargeResult::default().evaluate(&store).unwrap().is_empty());
    }
}
