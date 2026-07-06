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
    }

    #[test]
    fn r7_v2_suggests_next_session_start_command() {
        assert_eq!(fix_command("R7", &json!({})).as_deref(), Some("claude --model sonnet"));
    }

    #[test]
    fn v2_aggregate_rules_have_no_fix_command() {
        assert_eq!(fix_command("R10", &json!({})), None);
        assert_eq!(fix_command("R11", &json!({})), None);
        assert_eq!(fix_command("R12", &json!({})), None);
    }

    #[test]
    fn advice_only_and_missing_evidence_are_none() {
        assert_eq!(fix_command("R5", &json!({"path": "a.md"})), None);
        assert_eq!(fix_command("R9", &json!({})), None);
        assert_eq!(fix_command("R1", &json!({})), None); // server 키 없으면 None
        assert_eq!(fix_command("RX", &json!({})), None);
    }
}
