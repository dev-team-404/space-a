//! E — 공식 마켓플레이스 plugin 추천 (스펙 2026-07-22 §4 E).
//! 판정 패스(finding)가 아니라 콘텐츠 큐레이션에 얹는다: LLM은 세션의 작업 성격(work-kind)만
//! 판정해 캐시하고(src-tauri 파이프라인 별도 스텝), 추천 카드 생성은 순수 결정론 —
//! 캐시된 work-kind + 큐레이션 테이블(curation.rs) + 인벤토리 상태 + 카탈로그(② 존재 검증).
//! 불변 계약: 근거 = 사용자 프롬프트(prompt_events)뿐 / 이미 쓰는 plugin 침묵 / 실패 관대.

use crate::content::{CatalogEntry, ContentItem, ItemKind};
use crate::curation::{plugin_recos_for, work_kind_label, WORK_KINDS};
use crate::store::SqliteStore;
use anyhow::Result;

/// 관찰창(일) — R7 롤업과 동일.
pub const WORK_KIND_WINDOW_DAYS: i64 = 14;
/// 스캔당 판정 배치 상한 — 판정 패스 관행(10)과 동일.
pub const WORK_KIND_BATCH_CAP: usize = 10;
/// 같은 work-kind 세션이 이만큼 모여야 발화(저빈도 원칙, R12 min_sessions 선례). ⚠ CALIBRATE
pub const MIN_MATCHED_SESSIONS: usize = 2;

/// work-kind 판정 프롬프트 (system, user). R7Judge와 같은 증거 경계 — 사용자 요청만 본다.
pub fn work_kind_prompt(prompts: &[String]) -> (String, String) {
    let vocab = WORK_KINDS
        .iter()
        .map(|(k, l)| format!("- {k}: {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!(
        "당신은 Claude Code 세션의 작업 성격을 분류하는 심사관입니다.\n\
         사용자가 이 세션에서 시킨 작업이 아래 종류 중 어디에 해당하는지 고르세요(복수 가능).\n\
         {vocab}\n\
         사용자 요청 자체의 성격만 보세요 — 에이전트가 무엇을 어떻게 했는지는 판단 대상이 아닙니다.\n\
         명확히 해당하는 것만 고르고, **확신이 없으면 빈 배열**을 반환하세요.\n\
         아래 JSON 객체 하나만 출력(코드펜스·사족 금지):\n\
         {{\"work_kinds\": [\"kind_key\", ...]}}"
    );
    let joined = prompts
        .iter()
        .map(|p| format!("- \"{}\"", p.replace('"', "'")))
        .collect::<Vec<_>>()
        .join("\n");
    (system, format!("사용자 요청:\n{joined}"))
}

/// verdict JSON → vocabulary 검증된 work-kind 목록(정렬·dedup). `work_kinds` 필드가 없거나
/// 배열이 아니면 None = 형식 불량(attempts 재시도 계약). 미지 kind는 조용히 버린다.
pub fn parse_work_kinds(verdict: &serde_json::Value) -> Option<Vec<String>> {
    let arr = verdict.get("work_kinds")?.as_array()?;
    let mut kinds: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|k| WORK_KINDS.iter().any(|(key, _)| key == k))
        .map(String::from)
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    Some(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_kind_prompt_lists_vocabulary_and_guards_evidence_boundary() {
        let (system, user) = work_kind_prompt(&["로그인 화면 버튼 스타일 다듬어줘".to_string()]);
        for (kind, _) in WORK_KINDS {
            assert!(system.contains(kind), "vocabulary {kind} 누락");
        }
        assert!(system.contains("확신이 없으면 빈 배열"), "정밀도 우선 지시");
        assert!(system.contains("에이전트가"), "증거 경계(에이전트 산출물 채점 금지) 명시");
        assert!(system.contains("work_kinds"), "JSON 스키마 명시");
        assert!(user.contains("버튼 스타일"), "사용자 프롬프트 전문 포함");
    }

    #[test]
    fn parse_work_kinds_validates_against_vocabulary() {
        // 유효 kind만 통과 + 미지 kind 조용히 드랍 + dedup
        assert_eq!(
            parse_work_kinds(&serde_json::json!({"work_kinds": ["frontend_ui", "nonsense", "frontend_ui"]})),
            Some(vec!["frontend_ui".to_string()])
        );
        // 빈 배열 = 유효한 "해당 없음" 판정 (재시도 아님)
        assert_eq!(parse_work_kinds(&serde_json::json!({"work_kinds": []})), Some(vec![]));
        // 필드 없음·배열 아님 = 형식 불량(None → attempts 재시도 계약)
        assert_eq!(parse_work_kinds(&serde_json::json!({})), None);
        assert_eq!(parse_work_kinds(&serde_json::json!({"work_kinds": "frontend_ui"})), None);
    }
}
