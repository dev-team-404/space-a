//! 주인 메모리(Owner Memory) — 주인이 명시적으로 기억을 요청한 자유 텍스트 사실.
//! 채팅 `save_memory` 툴콜 또는 수동 UI로 저장되고, 채팅·코칭·일기 프롬프트에 주입된다.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub text: String,
    pub created_at: String, // 로컬 날짜 또는 RFC3339
    pub updated_at: Option<String>,
    pub source: String, // "chat" | "manual"
}

/// 프롬프트에 주입할 최대 메모리 개수. 사용자 큐레이션이라 도달 가능성 낮음.
pub const MAX_MEMORIES: usize = 40;
/// 프롬프트에 주입할 메모리 총 문자수 상한(대략).
pub const MAX_MEMORY_CHARS: usize = 2000;

/// 메모리 텍스트 목록을 프롬프트 블록으로 직렬화한다(캡 적용).
/// 입력은 `list_memories()` 순서(created_at ASC = 오래된→최신)를 가정한다.
/// 캡을 넘으면 **최신(뒤쪽)**을 유지하고 초과분은 버리며, 버린 수를 로그로 남긴다(무음 절단 금지).
/// 빈 목록이면 빈 문자열을 반환한다(호출부가 섹션을 생략).
pub fn memory_block(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    // 최신 우선으로 뒤에서부터 캡 적용, 출력은 다시 오래된→최신 순으로.
    let mut selected: Vec<&String> = Vec::new();
    let mut chars = 0usize;
    for line in lines.iter().rev() {
        if selected.len() >= MAX_MEMORIES {
            break;
        }
        let next = chars + line.chars().count() + 2; // "- " 여유
        if !selected.is_empty() && next > MAX_MEMORY_CHARS {
            break;
        }
        chars = next;
        selected.push(line);
    }
    let dropped = lines.len() - selected.len();
    if dropped > 0 {
        log::info!(
            "memory_block: {dropped}개 메모리가 캡({MAX_MEMORIES}개/{MAX_MEMORY_CHARS}자)으로 제외됨"
        );
    }
    selected.reverse();
    selected.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_empty_when_no_memories() {
        assert_eq!(memory_block(&[]), "");
    }

    #[test]
    fn block_formats_bullets() {
        let lines = vec!["주인은 비건임".to_string(), "목요일 오후 회의".to_string()];
        let b = memory_block(&lines);
        assert!(b.contains("- 주인은 비건임"));
        assert!(b.contains("- 목요일 오후 회의"));
    }

    #[test]
    fn block_caps_by_count_keeps_most_recent() {
        // list_memories는 created_at ASC(오래된→최신). 캡 초과 시 최신(뒤쪽) 유지.
        let lines: Vec<String> = (0..(MAX_MEMORIES + 5)).map(|i| format!("mem{i}")).collect();
        let b = memory_block(&lines);
        let count = b.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(count, MAX_MEMORIES);
        assert!(b.contains(&format!("mem{}", MAX_MEMORIES + 4))); // 최신 포함
        assert!(!b.lines().any(|l| l == "- mem0")); // 가장 오래된 것 제외
    }

    #[test]
    fn block_caps_by_chars() {
        let big = "가".repeat(600);
        let lines = vec![big.clone(), big.clone(), big.clone(), big.clone(), big];
        let b = memory_block(&lines);
        // 각 ~600자, 캡 2000자 → 최신 3~4개만
        assert!(b.chars().count() <= MAX_MEMORY_CHARS + 200); // 헤더/불릿 여유
    }
}
