//! R23 — 반복 도구 시퀀스 → 스킬/커맨드화 제안 (R6 v2 스펙 §3).
//! 프롬프트 원문 없이, 여러 세션이 공유하는 도구 호출 n-gram으로 반복 워크플로를 감지한다.
//! 예: `bash:gh → skill:codex → file-ops` (PR 생성 → 리뷰 → 코멘트 조치).
//! R13~R22는 코칭 v3 예약 번호라 R23을 쓴다.

use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub struct R23ToolSequences {
    /// 같은 시퀀스가 등장한 세션 수 문턱
    pub min_sessions: usize,
    /// 관찰 기간 (일)
    pub days: i64,
}

impl Default for R23ToolSequences {
    fn default() -> Self {
        R23ToolSequences { min_sessions: 3, days: 14 }
    }
}

const NGRAM_MIN: usize = 3;
const NGRAM_MAX: usize = 6;

/// tool_call 행 → 시퀀스 토큰 (스펙 §3.1). skill_draft가 같은 기준을 쓰도록 crate 공개.
pub(crate) fn tokenize(
    tool_kind: &str,
    tool_server: Option<&str>,
    tool_target: Option<&str>,
) -> String {
    match tool_kind {
        "execute" => match tool_target.and_then(|t| t.split_whitespace().next()) {
            Some(cmd) => format!("bash:{}", cmd.to_lowercase()),
            None => "bash".into(),
        },
        "mcp_call" => format!("mcp:{}", tool_server.unwrap_or("?")),
        "skill" => format!("skill:{}", tool_target.unwrap_or("?")),
        "sub_agent" => "agent".into(),
        "file_read" | "file_edit" | "file_write" | "search" => "file-ops".into(),
        other => other.to_string(),
    }
}

/// 에이전트가 자율적으로 흔히 쓰는 일반 명령 — 이것만으로 이뤄진 시퀀스는
/// 사용자가 지시한 워크플로가 아니라 에이전트의 기본 루프다(빌드·테스트·VCS·셸 유틸).
const GENERIC_BASH: &[&str] = &[
    "git", "npm", "npx", "pnpm", "yarn", "node", "python", "python3", "pip", "pip3",
    "cargo", "rustc", "go", "pytest", "ls", "cd", "cat", "echo", "mkdir", "rm", "cp",
    "mv", "grep", "rg", "find", "sed", "awk", "head", "tail", "touch", "chmod",
    "curl", "wget", "powershell", "pwsh", "cmd", "dir", "type", "sh", "bash", "test",
];

/// 사용자 의도가 실린 특이 토큰인가 — skill/MCP/서브에이전트 호출, 또는 일반 명령이 아닌
/// bash(예: gh, codex, 배포 스크립트). 특이 토큰이 하나도 없는 시퀀스는 코칭 가치가 없다.
fn is_distinctive(token: &str) -> bool {
    if token.starts_with("skill:") || token.starts_with("mcp:") || token == "agent" {
        return true;
    }
    token
        .strip_prefix("bash:")
        .is_some_and(|cmd| !GENERIC_BASH.contains(&cmd))
}

/// 연속 동일 토큰 압축(RLE) — file-ops 연쇄가 시퀀스를 잠식하지 않게 한다.
fn rle(tokens: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tokens {
        if out.last() != Some(&t) {
            out.push(t);
        }
    }
    out
}

fn hash8(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}

/// a가 b의 연속 부분열인가 (동일 길이 포함).
fn is_contiguous_subseq(a: &[String], b: &[String]) -> bool {
    a.len() <= b.len() && b.windows(a.len()).any(|w| w == a)
}

/// (host, session) → RLE 압축 토큰 열 (cutoff 이후 tool_call, 메인 체인 한정 —
/// 사이드체인의 도구 호출은 사용자의 수동 워크플로가 아니다).
/// R23 판정과 스킬 초안 재료 수집(skill_draft)이 같은 기준을 공유한다.
pub(crate) fn collect_streams(
    store: &SqliteStore,
    cutoff: &str,
) -> Result<BTreeMap<(String, String), Vec<String>>> {
    let mut stmt = store.conn.prepare(
        "SELECT host, session_id, tool_kind, tool_server, tool_target
         FROM events
         WHERE kind='tool_call' AND is_sidechain=0 AND ts >= ?1
         ORDER BY host, session_id, source_offset, id",
    )?;
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map(rusqlite::params![cutoff], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut streams: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (host, sess, kind, server, target) in rows {
        let tok = tokenize(kind.as_deref().unwrap_or("other"), server.as_deref(), target.as_deref());
        streams.entry((host, sess)).or_default().push(tok);
    }
    Ok(streams.into_iter().map(|(k, v)| (k, rle(v))).collect())
}

/// 주어진 시퀀스를 연속 부분열로 포함하는 host의 세션 목록 — 스킬 초안 재료 수집용.
pub(crate) fn sessions_containing(
    store: &SqliteStore,
    host: &str,
    sequence: &[String],
    days: i64,
) -> Result<Vec<String>> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
    Ok(collect_streams(store, &cutoff)?
        .into_iter()
        .filter(|((h, _), toks)| h == host && is_contiguous_subseq(sequence, toks))
        .map(|((_, s), _)| s)
        .collect())
}

impl Rule for R23ToolSequences {
    fn id(&self) -> &'static str {
        "R23"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let streams = collect_streams(store, &cutoff)?;

        // (host, n-gram) → 등장 세션 수 (세션 내 중복은 1회)
        let mut counts: BTreeMap<(String, Vec<String>), u64> = BTreeMap::new();
        for ((host, _sess), tokens) in streams {
            let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
            for n in NGRAM_MIN..=NGRAM_MAX.min(tokens.len()) {
                for w in tokens.windows(n) {
                    // 무의미 패턴 가드 — 토큰 다양성 ≥2 그리고 특이 토큰(사용자 의도) ≥1.
                    // 일반 명령·file-ops만으로 된 에이전트 자율 루프는 제외.
                    let distinct: BTreeSet<&String> = w.iter().collect();
                    if distinct.len() < 2 || !w.iter().any(|t| is_distinctive(t)) {
                        continue;
                    }
                    seen.insert(w.to_vec());
                }
            }
            for g in seen {
                *counts.entry((host.clone(), g)).or_insert(0) += 1;
            }
        }

        // 문턱 통과 후보 → 최장 시퀀스만 (포함되는 짧은 후보 제거)
        let mut candidates: Vec<(String, Vec<String>, u64)> = counts
            .into_iter()
            .filter(|(_, n)| (*n as usize) >= self.min_sessions)
            .map(|((host, g), n)| (host, g, n))
            .collect();
        candidates.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.1.cmp(&b.1)));
        let mut kept: Vec<(String, Vec<String>, u64)> = Vec::new();
        for (host, g, n) in candidates {
            if kept.iter().any(|(kh, kg, _)| kh == &host && is_contiguous_subseq(&g, kg)) {
                continue;
            }
            kept.push((host, g, n));
        }

        let mut out = Vec::new();
        for (host, seq, n) in kept {
            let joined = seq.join(" → ");
            let h = hash8(&joined);
            out.push(Finding {
                rule_id: "R23".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: None,
                scope_kind: "pattern".into(),
                scope_ref: format!("pattern:{h}"),
                evidence: serde_json::json!({
                    "sequence": seq,
                    "session_count": n,
                    "window_days": self.days,
                }),
                est_tokens_saved: 0, // 근거 없는 수치 금지 — 가치 제안형
                prescription: Some(Prescription {
                    kind: "skillify".into(),
                    payload: serde_json::json!({ "sequence": seq }),
                }),
                dedup_key: format!("R23|{host}|{h}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, NormalizedEvent, ToolKind};

    fn seed_tool(store: &SqliteStore, sess: &str, off: u64, ts: &str, raw: &str, target: Option<&str>) {
        // 어댑터와 동일하게 Skill은 특수 구성 (name = target)
        let kind = if raw == "Skill" {
            ToolKind::Skill { name: target.unwrap_or("").into() }
        } else {
            ToolKind::from_raw_name(raw)
        };
        store
            .upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(),
                schema_version: "t".into(),
                host: "Windows".into(),
                project_id: "p".into(),
                session_id: sess.into(),
                uuid: Some(format!("{sess}-u{off}")),
                parent_uuid: None,
                is_sidechain: false,
                ts: Some(ts.into()),
                source_file: "s.jsonl".into(),
                source_offset: off,
                kind: EventKind::ToolCall {
                    kind,
                    raw_name: raw.into(),
                    target: target.map(Into::into),
                    tool_use_id: Some(format!("t-{sess}-{off}")),
                },
            }])
            .unwrap();
    }

    /// PR 워크플로 세션: gh pr create → 파일 정리 → codex 리뷰 스킬 → 코멘트 조회
    fn seed_pr_workflow(store: &SqliteStore, sess: &str, ts: &str) {
        seed_tool(store, sess, 0, ts, "Bash", Some("gh pr create --fill"));
        seed_tool(store, sess, 10, ts, "Read", Some("a.rs"));
        seed_tool(store, sess, 20, ts, "Edit", Some("a.rs"));
        seed_tool(store, sess, 30, ts, "Skill", Some("codex:rescue"));
        seed_tool(store, sess, 40, ts, "Bash", Some("gh pr view --comments"));
    }

    #[test]
    fn tokenize_maps_kinds_per_spec() {
        assert_eq!(tokenize("execute", None, Some("gh pr create")), "bash:gh");
        assert_eq!(tokenize("execute", None, None), "bash");
        assert_eq!(tokenize("mcp_call", Some("context7"), None), "mcp:context7");
        assert_eq!(tokenize("skill", None, Some("codex:rescue")), "skill:codex:rescue");
        assert_eq!(tokenize("sub_agent", None, None), "agent");
        assert_eq!(tokenize("file_read", None, Some("a.rs")), "file-ops");
        assert_eq!(tokenize("search", None, None), "file-ops");
        assert_eq!(tokenize("web_fetch", None, None), "web_fetch");
    }

    #[test]
    fn r23_fires_on_three_sessions_sharing_sequence() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for sess in ["s1", "s2", "s3"] {
            seed_pr_workflow(&store, sess, &now);
        }
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "최장 시퀀스 하나만 남아야 함: {findings:?}");
        let f = &findings[0];
        assert_eq!(f.rule_id, "R23");
        assert_eq!(f.evidence["session_count"], 3);
        let seq: Vec<&str> = f.evidence["sequence"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        // Read→Edit 연쇄는 file-ops 하나로 압축(RLE)
        assert_eq!(seq, vec!["bash:gh", "file-ops", "skill:codex:rescue", "bash:gh"]);
        assert_eq!(f.est_tokens_saved, 0);
        assert_eq!(f.prescription.as_ref().unwrap().kind, "skillify");
    }

    #[test]
    fn sessions_containing_matches_by_contiguous_subsequence() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_pr_workflow(&store, "s1", &now);
        seed_pr_workflow(&store, "s2", &now);
        seed_tool(&store, "s3", 0, &now, "Bash", Some("cargo test")); // 무관 세션
        let seq: Vec<String> =
            ["bash:gh", "file-ops", "skill:codex:rescue"].iter().map(|s| s.to_string()).collect();
        let mut got = sessions_containing(&store, "Windows", &seq, 14).unwrap();
        got.sort();
        assert_eq!(got, vec!["s1".to_string(), "s2".to_string()]);
        let none: Vec<String> = ["mcp:x", "bash:y", "agent"].iter().map(|s| s.to_string()).collect();
        assert!(sessions_containing(&store, "Windows", &none, 14).unwrap().is_empty());
    }

    #[test]
    fn r23_silent_below_session_threshold() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        seed_pr_workflow(&store, "s1", &now);
        seed_pr_workflow(&store, "s2", &now);
        assert!(R23ToolSequences::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r23_ignores_generic_agent_loops() {
        // 에이전트가 자율 반복하는 일반 루프(파일 수정 → 테스트 → 커밋)는 사용자 워크플로가 아니다
        // — 실사용 노이즈 재현: file-ops → bash:npx → bash:git (2026-07-20 사용자 판정)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for sess in ["g1", "g2", "g3"] {
            seed_tool(&store, sess, 0, &now, "Read", Some("a.ts"));
            seed_tool(&store, sess, 10, &now, "Edit", Some("a.ts"));
            seed_tool(&store, sess, 20, &now, "Bash", Some("npx vitest run"));
            seed_tool(&store, sess, 30, &now, "Bash", Some("git commit -m x"));
        }
        assert!(R23ToolSequences::default().evaluate(&store).unwrap().is_empty(),
            "특이 토큰 없는 일반 루프는 침묵해야 함");
    }

    #[test]
    fn r23_ignores_file_ops_only_and_sidechain() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        // file-ops만 잔뜩 반복돼도 (RLE로 뭉개져) 패턴이 아니다
        for sess in ["a1", "a2", "a3"] {
            for off in [0u64, 10, 20, 30, 40, 50] {
                seed_tool(&store, sess, off, &now, "Read", Some("x.rs"));
            }
        }
        assert!(R23ToolSequences::default().evaluate(&store).unwrap().is_empty());
    }
}
