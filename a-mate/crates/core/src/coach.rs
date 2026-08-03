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
        // v2: 세션 중 전환은 실제 레버가 아님 — "다음 세션의 선택"으로 처방 (스펙 §4.3)
        "R7" => Some("claude --model sonnet".to_string()),
        _ => None,
    }
}

/// 「해결함」 시점에 스냅숏할 **룰별 근거 수치** (스펙 §5.1). 재발은 이 값을 *초과*했을 때만
/// 인정한다 — 실제 행동이 또 있어야 늘어나는 값이라야 한다.
///
/// 프론트의 근거 칩(`coach-helpers.ts` `evidenceChip`)과 같은 키를 읽는다: 사용자가 카드에서
/// 본 수치가 곧 기준선이 되도록. `occurrences`·`last_seen`은 스캔마다 갱신되는 스캔 지표라
/// 여기 쓸 수 없다(§1.2 D5) — 썼다면 처분 60초 뒤 카드가 부활한다.
///
/// 재료가 없으면 `None`. 0으로 대체하지 않는다 — "0 초과"는 다음 스캔에 곧바로 참이 된다.
pub fn recurrence_evidence_n(rule_id: &str, evidence: &Value) -> Option<i64> {
    let key = match rule_id {
        "R6" => "session_count",
        "R7" => "total_sessions",
        "R8" => "large_result_count",
        _ => return None,
    };
    evidence.get(key)?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::{fix_command, recurrence_evidence_n};
    use serde_json::json;

    /// 스펙 §5.1 — 재발 기준선은 근거 칩과 **같은** 룰별 수치다. 실제 행동이 있어야 늘어난다.
    #[test]
    fn recurrence_baseline_reads_the_rule_specific_evidence_count() {
        assert_eq!(recurrence_evidence_n("R6", &json!({"session_count": 4})), Some(4));
        assert_eq!(recurrence_evidence_n("R7", &json!({"total_sessions": 5})), Some(5));
        assert_eq!(recurrence_evidence_n("R8", &json!({"large_result_count": 12})), Some(12));
    }

    /// 재료가 없으면 None — 0으로 채우면 "0 초과"가 즉시 참이 되어 처분 직후 카드가 부활한다.
    #[test]
    fn recurrence_baseline_is_none_when_the_rule_or_key_is_unknown() {
        assert_eq!(recurrence_evidence_n("R6", &json!({})), None);
        assert_eq!(recurrence_evidence_n("R6", &json!({"session_count": "네개"})), None);
        assert_eq!(recurrence_evidence_n("R99", &json!({"session_count": 4})), None);
    }

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
    }

    #[test]
    fn r7_v2_suggests_next_session_start_command() {
        assert_eq!(fix_command("R7", &json!({})).as_deref(), Some("claude --model sonnet"));
    }

    #[test]
    fn v2_aggregate_rules_have_no_fix_command() {
        assert_eq!(fix_command("R10", &json!({})), None);
        assert_eq!(fix_command("R11", &json!({"friction_events": [], "by_tool": {}})), None);
        assert_eq!(fix_command("R12", &json!({})), None);
        // v2.1 신규 처방 kind는 서사로만 안내 — fix_command 없음
        assert_eq!(
            fix_command("R5", &json!({"subtype": "within_session_context_drift", "total_sessions": 3})),
            None
        );
        assert_eq!(
            fix_command("R5", &json!({"subtype": "cross_session_claude_md", "files": []})),
            None
        );
    }

    #[test]
    fn advice_only_and_missing_evidence_are_none() {
        assert_eq!(fix_command("R5", &json!({"path": "a.md"})), None);
        assert_eq!(fix_command("R9", &json!({})), None);
        assert_eq!(fix_command("R1", &json!({})), None); // server 키 없으면 None
        assert_eq!(fix_command("RX", &json!({})), None);
    }
}
