pub mod r1_unused_mcp;
pub mod r2_unused_plugins;
pub mod r5_repeated_read;
pub mod r6_repeated_prompts;
pub mod r7_judge;
pub mod r7_opus_trivial;
pub mod r8_mcp_large_result;
pub mod r9_web_overuse;
pub mod r10_automation_burst;
pub mod r11_permission_friction;
pub mod r12_unused_skills;
pub mod r24_context_hygiene;
pub mod session_stats;

use crate::finding::Finding;
use crate::store::SqliteStore;
use anyhow::Result;

pub trait Rule {
    fn id(&self) -> &'static str;
    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>>;
}

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleEngine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> RuleEngine {
        RuleEngine { rules }
    }

    pub fn run(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut out = Vec::new();
        for rule in &self.rules {
            out.extend(rule.evaluate(store)?);
        }
        Ok(out)
    }
}

/// events(project_id)와 인벤토리(claude.json 경로)를 잇는 v0 정규화 키.
pub fn normalize_project_key(s: &str) -> String {
    crate::adapter::decode_project_id(s)
}
