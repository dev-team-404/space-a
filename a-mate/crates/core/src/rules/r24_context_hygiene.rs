//! R24 — 컨텍스트 위생 (신규 결정론 룰, 코칭 v3 재설계 §4 F).
//! clear/compact 없이 대용량 컨텍스트를 상주시켜, 작업을 바꿔도 이전 컨텍스트를 이고 가는 습관.
//! 절대 토큰 임계 대신 "작업 경계에서 컨텍스트가 리셋되는가"를 본다 — 각 에피소드의 첫
//! main-chain 턴이 물려받은 컨텍스트(tok_input+tok_cache_read)로 판별. LLM 판정 미사용(R8형).

use crate::episode::{segment_all, Episode};
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct R24ContextHygiene {
    /// 습관 성립에 필요한 세션당 최소 에피소드(작업) 수.
    pub min_episodes: usize,
    /// "큰 컨텍스트를 물려받음"으로 볼 토큰 임계.
    pub inherited_ctx_threshold: u64,
    /// 큰 컨텍스트를 물려받은 에피소드 비율 문턱 (0.0~1.0).
    pub carry_ratio: f64,
    /// (host,project) 카드 발화에 필요한 최소 "위생 나쁜 세션" 수.
    pub min_bad_sessions: usize,
    /// 관찰 기간(일).
    pub days: i64,
}

impl Default for R24ContextHygiene {
    // ⚠ CALIBRATE — 잠정값. Windows 실데이터로 캘리브레이션 (스펙 §8). R7·R8 관행.
    fn default() -> Self {
        R24ContextHygiene {
            min_episodes: 5,
            inherited_ctx_threshold: 50_000,
            carry_ratio: 0.60,
            min_bad_sessions: 1,
            days: 14,
        }
    }
}

struct BadSession {
    session_id: String,
    host: String,
    project_id: String,
    episodes: usize,
    carry_count: usize,
    carry_ratio_pct: u64,
    max_inherited: u64,
    sample_prompts: Vec<String>,
    first_ts: Option<String>,
}

impl Rule for R24ContextHygiene {
    fn id(&self) -> &'static str {
        "R24"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(self.days)).to_rfc3339();
        let episodes = segment_all(store)?;

        // 세션별 그룹화 (결정론 위해 BTreeMap).
        let mut by_session: BTreeMap<String, Vec<Episode>> = BTreeMap::new();
        for ep in episodes {
            by_session.entry(ep.session_id.clone()).or_default().push(ep);
        }

        // 세션별 carry-ratio → 위생 나쁜 세션 추림.
        let mut bad: Vec<BadSession> = Vec::new();
        for (session_id, eps) in by_session {
            // 관찰창: 세션 최초 ts ≥ cutoff. ts 없는 세션 제외 (R7 관행).
            let first_ts = eps.iter().filter_map(|e| e.first_ts.clone()).min();
            match first_ts.as_deref() {
                Some(ts) if ts >= cutoff.as_str() => {}
                _ => continue,
            }
            let n = eps.len();
            if n < self.min_episodes {
                continue;
            }
            let carry_count =
                eps.iter().filter(|e| e.inherited_ctx >= self.inherited_ctx_threshold).count();
            let ratio = carry_count as f64 / n as f64;
            if ratio < self.carry_ratio {
                continue;
            }
            let max_inherited = eps.iter().map(|e| e.inherited_ctx).max().unwrap_or(0);
            let host = eps[0].host.clone();
            let project_id = eps[0].project_id.clone();
            // 근거 인용: inherited_ctx 큰 순 상위 3개 에피소드의 lead preview.
            let mut sorted = eps.clone();
            sorted.sort_by(|a, b| b.inherited_ctx.cmp(&a.inherited_ctx));
            let sample_prompts =
                sorted.iter().take(3).map(|e| e.lead_preview.clone()).collect::<Vec<_>>();
            bad.push(BadSession {
                session_id,
                host,
                project_id,
                episodes: n,
                carry_count,
                carry_ratio_pct: (ratio * 100.0).round() as u64,
                max_inherited,
                sample_prompts,
                first_ts,
            });
        }

        // (host, project) 롤업.
        let mut by_proj: BTreeMap<(String, String), Vec<BadSession>> = BTreeMap::new();
        for b in bad {
            by_proj.entry((b.host.clone(), b.project_id.clone())).or_default().push(b);
        }

        let mut out = Vec::new();
        for ((host, project_id), mut sessions) in by_proj {
            if sessions.len() < self.min_bad_sessions {
                continue;
            }
            // 가장 심한 세션 = carry_ratio_pct desc, max_inherited desc.
            sessions.sort_by(|a, b| {
                b.carry_ratio_pct
                    .cmp(&a.carry_ratio_pct)
                    .then(b.max_inherited.cmp(&a.max_inherited))
            });
            let worst = &sessions[0];
            out.push(Finding {
                rule_id: "R24".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: Some(project_id.clone()),
                scope_kind: "project".into(),
                scope_ref: project_id.clone(),
                evidence: serde_json::json!({
                    "host": host,
                    "project": project_id,
                    "bad_session_count": sessions.len(),
                    "worst_session_id": worst.session_id,
                    "worst_carry_ratio_pct": worst.carry_ratio_pct,
                    "worst_episodes": worst.episodes,
                    "worst_carry_count": worst.carry_count,
                    "worst_max_inherited_tokens": worst.max_inherited,
                    "inherited_ctx_threshold": self.inherited_ctx_threshold,
                    "carry_ratio_threshold_pct": (self.carry_ratio * 100.0).round() as u64,
                    "min_episodes": self.min_episodes,
                    "window_days": self.days,
                    "sample_prompts": worst.sample_prompts,
                    "first_ts": worst.first_ts,
                }),
                est_tokens_saved: 0,
                prescription: Some(Prescription {
                    kind: "context_hygiene".into(),
                    payload: serde_json::json!({}),
                }),
                dedup_key: format!("R24|{host}|{project_id}"),
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

    fn recent(h: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339()
    }

    fn prompt(session: &str, offset: u64, preview: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-p{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::UserPrompt { preview: preview.into() },
        }
    }

    fn turn(session: &str, offset: u64, input: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-a{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    /// 한 세션에 에피소드 `n`개를 만든다. 각 에피소드 첫 턴 input = `inherited`.
    /// offset은 세션 내 단조 증가. ts는 recent(base_h - i) 로 관찰창 안쪽.
    fn session_with_episodes(sid: &str, n: usize, inherited: u64, base_h: i64) -> Vec<NormalizedEvent> {
        let mut evs = Vec::new();
        for i in 0..n {
            let off = (i as u64) * 2;
            let ts = recent(base_h - i as i64); // 시간 진행
            evs.push(prompt(sid, off, &format!("에피소드 {i} 실질 작업 지시 문장"), &ts));
            evs.push(turn(sid, off + 1, inherited, &ts));
        }
        evs
    }

    #[test]
    fn fires_when_carry_ratio_high() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 6개 에피소드 전부 60k 상속 → carry_ratio 100% ≥ 60%, n=6 ≥ 5.
        store.upsert_events(&session_with_episodes("s1", 6, 60_000, 20)).unwrap();

        let f = R24ContextHygiene::default().evaluate(&store).unwrap();
        assert_eq!(f.len(), 1, "위생 나쁜 프로젝트 카드 1장");
        let card = &f[0];
        assert_eq!(card.rule_id, "R24");
        assert_eq!(card.scope_kind, "project");
        assert_eq!(card.dedup_key, "R24|Windows|d--proj");
        assert_eq!(card.est_tokens_saved, 0);
        assert_eq!(card.prescription.as_ref().unwrap().kind, "context_hygiene");
        assert_eq!(card.evidence["worst_carry_ratio_pct"], 100);
        assert_eq!(card.evidence["worst_episodes"], 6);
        assert_eq!(card.evidence["worst_session_id"], "s1");
        assert!(card.evidence["sample_prompts"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn silent_for_healthy_clear_habit() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 6개 에피소드지만 전부 2k만 상속 (/clear로 끊음) → carry_ratio 0%.
        store.upsert_events(&session_with_episodes("s1", 6, 2_000, 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn silent_below_min_episodes() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 4개 에피소드(큰 상속이어도) < min_episodes(5) → 침묵.
        store.upsert_events(&session_with_episodes("s1", 4, 80_000, 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn silent_outside_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        // base_h = 24*20 시간 전(=20일 전) → 관찰창(14일) 밖.
        store.upsert_events(&session_with_episodes("s1", 6, 60_000, 24 * 20)).unwrap();
        assert!(R24ContextHygiene::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn rollup_cites_worst_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 같은 프로젝트 두 세션 모두 위생 나쁨 — 상속량 다르게.
        store.upsert_events(&session_with_episodes("s1", 6, 55_000, 30)).unwrap();
        store.upsert_events(&session_with_episodes("s2", 6, 90_000, 20)).unwrap();

        let f = R24ContextHygiene::default().evaluate(&store).unwrap();
        assert_eq!(f.len(), 1, "프로젝트당 카드 1장");
        assert_eq!(f[0].evidence["bad_session_count"], 2);
        assert_eq!(f[0].evidence["worst_session_id"], "s2", "max_inherited 큰 세션 인용");
        assert_eq!(f[0].evidence["worst_max_inherited_tokens"], 90_000);
    }
}
