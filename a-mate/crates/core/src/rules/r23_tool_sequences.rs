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
/// host당 카드 상한 — 겹침 dedup 후에도 무관한 패턴이 코치 탭을 채우지 않게 한다.
const MAX_CARDS_PER_HOST: usize = 5;

/// tool_call 행 → 시퀀스 토큰 (스펙 §3.1). skill_draft가 같은 기준을 쓰도록 crate 공개.
pub(crate) fn tokenize(
    tool_kind: &str,
    tool_server: Option<&str>,
    tool_target: Option<&str>,
    raw_name: Option<&str>,
) -> String {
    match tool_kind {
        // 시크릿 리댁션된 명령(<redacted: …>)은 내용을 알 수 없으니 일반 bash로 취급 —
        // 특이 토큰으로 오인해 무관한 시크릿 명령들이 한 패턴으로 뭉치는 것 방지 (Codex P2).
        "execute" => match tool_target
            .filter(|t| !t.starts_with('<'))
            .and_then(|t| t.split_whitespace().next())
        {
            Some(cmd) => format!("bash:{}", cmd.to_lowercase()),
            None => "bash".into(),
        },
        "mcp_call" => format!("mcp:{}", tool_server.unwrap_or("?")),
        "skill" => format!("skill:{}", tool_target.unwrap_or("?")),
        "sub_agent" => "agent".into(),
        "file_read" | "file_edit" | "file_write" | "search" => "file-ops".into(),
        // 미분류 내장 도구는 raw_name으로 구분 — 전부 'other'로 뭉개면 서로 다른
        // 워크플로가 같은 n-gram을 공유한다 (Codex P2).
        "other" => raw_name.map(|r| r.to_lowercase()).unwrap_or_else(|| "other".into()),
        k => k.to_string(),
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

/// a가 b의 연속 부분열인가 (동일 길이 포함). 빈 a는 항상 참 — windows(0) 패닉 방지.
fn is_contiguous_subseq(a: &[String], b: &[String]) -> bool {
    if a.is_empty() {
        return true;
    }
    a.len() <= b.len() && b.windows(a.len()).any(|w| w == a)
}

/// 두 시퀀스가 **특이 토큰을 포함한** 연속 2-토큰(bigram)을 공유하는가 — 같은
/// 워크플로에서 파생된 포함·시프트 변형을 한 가족으로 판정한다.
/// 공통(비특이) 단계(file-ops → bash:cargo 등)만 겹치는 서로 다른 워크플로를
/// 병합해 지우지 않도록 특이 토큰 조건을 건다 (PR#78 리뷰).
fn shares_distinctive_bigram(a: &[String], b: &[String]) -> bool {
    a.windows(2).any(|wa| {
        (is_distinctive(&wa[0]) || is_distinctive(&wa[1]))
            && b.windows(2).any(|wb| wa == wb)
    })
}

/// (host, session) → RLE 압축 토큰 열 (cutoff 이후 tool_call, 메인 체인 한정 —
/// 사이드체인의 도구 호출은 사용자의 수동 워크플로가 아니다).
/// R23 판정과 스킬 초안 재료 수집(skill_draft)이 같은 기준을 공유한다.
pub(crate) fn collect_streams(
    store: &SqliteStore,
    cutoff: &str,
) -> Result<BTreeMap<(String, String), Vec<String>>> {
    let mut stmt = store.conn.prepare(
        "SELECT host, session_id, tool_kind, tool_server, tool_target, raw_name
         FROM events
         WHERE kind='tool_call' AND is_sidechain=0 AND ts >= ?1
         ORDER BY host, session_id, source_offset, id",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map(rusqlite::params![cutoff], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut streams: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (host, sess, kind, server, target, raw) in rows {
        let tok = tokenize(
            kind.as_deref().unwrap_or("other"),
            server.as_deref(),
            target.as_deref(),
            raw.as_deref(),
        );
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
                    // raw_name 없는 무명 도구('other'/빈 토큰)가 낀 시퀀스는 카드 정보가 없다.
                    if w.iter().all(|x| x == &w[0])
                        || !w.iter().any(|t| is_distinctive(t))
                        || w.iter().any(|t| t == "other" || t.is_empty())
                    {
                        continue;
                    }
                    seen.insert(w.to_vec());
                }
            }
            for g in seen {
                *counts.entry((host.clone(), g)).or_insert(0) += 1;
            }
        }

        // 문턱 통과 후보 → host별 겹침 가족(연결 요소)당 대표 1개 + host당 상한.
        // score = 세션 수 × 길이: 짧고 강한 패턴(예: 3-gram × 8세션)과 길고 풍부한
        // 패턴(예: 6-gram × 3세션)의 균형을 하나의 순위로 정한다. 실데이터에서 부분
        // 시퀀스의 세션 수는 항상 상위 시퀀스 이상이라, "빈도 우위면 유지" 방식은
        // dedup을 무력화해 카드 홍수를 만들었다 (2026-07-20 실사용 판정).
        // 가족은 연결 요소로 만든다 — 그리디 비교는 다리(bridge) 후보가 탈락하면
        // 가족이 갈라져 대표가 중복될 수 있다 (PR#78 리뷰).
        let mut by_host: BTreeMap<String, Vec<(Vec<String>, u64)>> = BTreeMap::new();
        for ((host, g), n) in counts {
            if (n as usize) >= self.min_sessions {
                by_host.entry(host).or_default().push((g, n));
            }
        }
        let score = |g: &[String], n: u64| n * g.len() as u64;
        let mut kept: Vec<(String, Vec<String>, u64)> = Vec::new();
        for (host, cands) in by_host {
            // 연결 요소 라벨링 (BFS)
            let mut comp = vec![usize::MAX; cands.len()];
            let mut n_comp = 0usize;
            for i in 0..cands.len() {
                if comp[i] != usize::MAX {
                    continue;
                }
                comp[i] = n_comp;
                let mut queue = vec![i];
                while let Some(u) = queue.pop() {
                    for v in 0..cands.len() {
                        if comp[v] == usize::MAX
                            && shares_distinctive_bigram(&cands[u].0, &cands[v].0)
                        {
                            comp[v] = n_comp;
                            queue.push(v);
                        }
                    }
                }
                n_comp += 1;
            }
            // 가족당 대표 = score 최고 (동점이면 긴 것 → 사전순 작은 것)
            let mut reps: Vec<&(Vec<String>, u64)> = (0..n_comp)
                .map(|c| {
                    cands
                        .iter()
                        .zip(&comp)
                        .filter(|(_, cc)| **cc == c)
                        .map(|(x, _)| x)
                        .max_by(|a, b| {
                            score(&a.0, a.1)
                                .cmp(&score(&b.0, b.1))
                                .then_with(|| a.0.len().cmp(&b.0.len()))
                                .then_with(|| b.0.cmp(&a.0))
                        })
                        .expect("연결 요소는 비어 있지 않다")
                })
                .collect();
            reps.sort_by(|a, b| {
                score(&b.0, b.1)
                    .cmp(&score(&a.0, a.1))
                    .then_with(|| b.0.len().cmp(&a.0.len()))
                    .then_with(|| a.0.cmp(&b.0))
            });
            reps.truncate(MAX_CARDS_PER_HOST);
            kept.extend(reps.into_iter().map(|(g, n)| (host.clone(), g.clone(), *n)));
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
        assert_eq!(tokenize("execute", None, Some("gh pr create"), Some("Bash")), "bash:gh");
        assert_eq!(tokenize("execute", None, None, Some("Bash")), "bash");
        assert_eq!(tokenize("mcp_call", Some("context7"), None, None), "mcp:context7");
        assert_eq!(tokenize("skill", None, Some("codex:rescue"), Some("Skill")), "skill:codex:rescue");
        assert_eq!(tokenize("sub_agent", None, None, Some("Task")), "agent");
        assert_eq!(tokenize("file_read", None, Some("a.rs"), Some("Read")), "file-ops");
        assert_eq!(tokenize("search", None, None, Some("Grep")), "file-ops");
        assert_eq!(tokenize("web_fetch", None, None, Some("WebFetch")), "web_fetch");
    }

    #[test]
    fn tokenize_redacted_bash_is_generic() {
        // 시크릿 리댁션 target은 특이 명령이 아니다 (Codex P2)
        let tok = tokenize("execute", None, Some(crate::adapter::SECRET_REDACTED), Some("Bash"));
        assert_eq!(tok, "bash");
    }

    #[test]
    fn tokenize_unclassified_tool_uses_raw_name() {
        // ToolKind::Other는 raw_name으로 구분 — 'other'로 뭉개면 안 됨 (Codex P2)
        assert_eq!(tokenize("other", None, None, Some("TodoWrite")), "todowrite");
        assert_eq!(tokenize("other", None, None, Some("NotebookEdit")), "notebookedit");
        assert_eq!(tokenize("other", None, None, None), "other");
    }

    #[test]
    fn r23_distinguishes_unclassified_tools_by_raw_name() {
        // 쿼리가 raw_name을 실제로 전달하는지 — evaluate 경로로 검증
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for sess in ["s1", "s2", "s3"] {
            seed_tool(&store, sess, 0, &now, "Skill", Some("codex:rescue"));
            seed_tool(&store, sess, 10, &now, "TodoWrite", None);
            seed_tool(&store, sess, 20, &now, "Bash", Some("gh pr view"));
        }
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let seq: Vec<&str> = findings[0].evidence["sequence"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(seq, vec!["skill:codex:rescue", "todowrite", "bash:gh"]);
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
    fn contiguous_subseq_empty_needle_is_safe() {
        // slice.windows(0)은 패닉 — 빈 시퀀스 입력에도 안전해야 함 (Gemini medium)
        assert!(is_contiguous_subseq(&[], &["a".to_string()]));
    }

    #[test]
    fn r23_collapses_workflow_family_to_single_best_card() {
        // 한 워크플로에서 파생된 겹침 변형(포함·시프트)은 대표 1장만 —
        // score(세션수×길이)가 높은 쪽. 짧고 강한 패턴(8세션×3=24)이
        // 길고 희소한 변형(3세션×4=12)을 이긴다. (카드 홍수 수정, 2026-07-20)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let seed_core = |sess: &str| {
            seed_tool(&store, sess, 0, &now, "Bash", Some("gh pr create"));
            seed_tool(&store, sess, 10, &now, "Skill", Some("codex:rescue"));
            seed_tool(&store, sess, 20, &now, "mcp__m__query", None);
        };
        for sess in ["a1", "a2", "a3", "a4", "a5"] {
            seed_core(sess);
        }
        for sess in ["b1", "b2", "b3"] {
            seed_core(sess);
            seed_tool(&store, sess, 30, &now, "Bash", Some("docker build"));
        }
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "겹침 가족은 대표 1장만: {findings:?}");
        assert_eq!(findings[0].evidence["session_count"], 8);
        assert_eq!(findings[0].evidence["sequence"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn r23_caps_cards_per_host() {
        // 서로 무관한 패턴이 아무리 많아도 host당 상위 5장까지만 (카드 홍수 방지)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for i in 0..6 {
            for s in 0..3 {
                let sess = format!("p{i}s{s}");
                seed_tool(&store, &sess, 0, &now, "Skill", Some(&format!("team:flow{i}")));
                seed_tool(&store, &sess, 10, &now, &format!("mcp__srv{i}__q"), None);
                seed_tool(&store, &sess, 20, &now, "Bash", Some(&format!("deploy{i} run")));
            }
        }
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 5, "host당 5장 상한: {}건", findings.len());
    }

    #[test]
    fn r23_ignores_sequences_with_unnamed_tools() {
        // raw_name이 없는(NULL) 도구는 'other' 토큰이 되는데, 이런 시퀀스는 카드로서
        // 정보가 없다 — 제외 (실사용 노이즈: bash:gh → file-ops → other)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for sess in ["u1", "u2", "u3"] {
            seed_tool(&store, sess, 0, &now, "Bash", Some("gh pr view"));
            seed_tool(&store, sess, 10, &now, "Read", Some("a.rs"));
            store.conn.execute(
                "INSERT INTO events (dedup_key, session_id, host, project_id, ts, source_offset,
                   kind, tool_kind, raw_name, is_sidechain, source_file)
                 VALUES (?1, ?2, 'Windows', 'p', ?3, 20, 'tool_call', 'other', NULL, 0, 's.jsonl')",
                rusqlite::params![format!("{sess}-null"), sess, now],
            ).unwrap();
        }
        assert!(R23ToolSequences::default().evaluate(&store).unwrap().is_empty(),
            "무명 도구가 낀 시퀀스는 침묵해야 함");
    }

    #[test]
    fn r23_keeps_workflows_sharing_only_generic_bigram() {
        // 공통(비특이) 꼬리 단계(file-ops → bash:cargo)만 공유하는 서로 다른
        // 워크플로는 별개 카드로 남아야 한다 (PR#78 Gemini)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for (skill, group) in [("team:deploy", "d"), ("team:test", "t")] {
            for s in 0..3 {
                let sess = format!("{group}{s}");
                seed_tool(&store, &sess, 0, &now, "Skill", Some(skill));
                seed_tool(&store, &sess, 10, &now, "Read", Some("a.rs"));
                seed_tool(&store, &sess, 20, &now, "Bash", Some("cargo test"));
            }
        }
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 2, "특이 토큰 없는 bigram 공유로 병합되면 안 됨: {findings:?}");
    }

    #[test]
    fn r23_collapses_transitive_family_via_bridge() {
        // X~Y, Y~Z만 직접 겹칠 때(X~Z 직접 공유 없음) 셋은 한 가족 — 대표 1장 (PR#78 Codex P2)
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let seed_stream = |group: &str, tools: [(&str, Option<&str>); 3]| {
            for s in 0..3 {
                let sess = format!("{group}{s}");
                for (i, (raw, target)) in tools.iter().enumerate() {
                    seed_tool(&store, &sess, (i as u64) * 10, &now, raw, *target);
                }
            }
        };
        seed_stream("x", [("mcp__a__q", None), ("Skill", Some("b")), ("mcp__c__q", None)]);
        seed_stream("y", [("Skill", Some("b")), ("mcp__c__q", None), ("Skill", Some("d"))]);
        seed_stream("z", [("mcp__c__q", None), ("Skill", Some("d")), ("mcp__e__q", None)]);
        let findings = R23ToolSequences::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "브리지로 이어진 가족은 대표 1장만: {findings:?}");
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
