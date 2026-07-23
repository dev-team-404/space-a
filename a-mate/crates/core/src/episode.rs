//! 에피소드 세그먼터 (결정론) — 코칭 v3 재설계 §3①.
//! 한 세션의 이벤트를 [실질 UserPrompt → 다음 실질 프롬프트 전까지의 작업] = 에피소드로 자른다.
//! clear/compact 습관에 불변인 작업 단위. C·A·F 공유 척추이며 F가 첫 소비자.
//!
//! 경계(실질 프롬프트)는 `prompt_events`에서 온다 — ingest가 이미 sidechain·meta·도구결과·
//! 주입을 제외하고 8자↑ 실질 발화만 담는다(store.rs). 따라서 접착(<8자) 프롬프트는 애초에
//! 경계가 되지 않아 별도 병합 로직이 필요 없다. 에피소드 내 작업 사실(물려받은 컨텍스트·
//! compaction)은 `events`의 main-chain AssistantTurn·Compaction에서 온다.
//!
//! 범위 경계(YAGNI): 접착 프롬프트 원문 리스트(`glue_followups`)는 싣지 않는다 —
//! F는 안 쓰고 ingest가 <8자 원문을 버리기 때문. 유일한 소비 예정처였던 C가 드롭돼
//! (스펙 §4 C, 2026-07-22) 현재 소비자가 없다 — 필요한 소비자가 생기면 ingest 확장과 함께 추가.

use crate::store::SqliteStore;
use anyhow::Result;
use std::collections::BTreeMap;

/// 한 실질 프롬프트가 여는 작업 단위.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    /// 에피소드를 연 실질 프롬프트 미리보기 (근거 인용용).
    pub lead_preview: String,
    /// 실질 프롬프트의 source_offset (deref 포인터).
    pub lead_offset: u64,
    /// 에피소드 시작 ts (= lead 프롬프트 ts). 없으면 None.
    pub first_ts: Option<String>,
    /// 이 에피소드의 첫 main-chain AssistantTurn이 물려받은 컨텍스트
    /// = tok_input + tok_cache_read + tok_cache_create (세 input 범주 전부).
    /// cache_create를 포함하는 이유: 캐시 만료·재생성(작업 경계의 시간 간격에서 흔함) 시
    /// 물려받은 컨텍스트가 cache_read가 아닌 cache_create로 청구된다 — 빼면 캐시 미스 턴의
    /// 대용량 상속이 작게 보여 F 발화가 억제된다. 그런 턴이 없으면 0.
    pub inherited_ctx: u64,
    /// 에피소드 범위 안에 (main-chain) Compaction 이벤트가 있었는지 (보조 신호).
    pub had_compaction: bool,
}

// 내부 정렬 키: (ts 문자열, source_offset). ts 우선, offset 타이브레이크.
// ts 없는 행은 빈 문자열로 맨 앞 정렬 (스펙 §3① "ts 없는 프롬프트는 offset 정렬").
type OrderKey = (String, u64);

struct Lead {
    host: String,
    project_id: String,
    ts: Option<String>,
    offset: u64,
    preview: String,
}

enum Ev {
    Turn { inherited: u64 },
    Compaction,
}

struct EvRow {
    key: OrderKey,
    ev: Ev,
}

pub fn segment_all(store: &SqliteStore) -> Result<Vec<Episode>> {
    // 1) 경계: prompt_events (실질·main-chain 프롬프트) — 세션별로 모은다.
    let mut leads: BTreeMap<String, Vec<Lead>> = BTreeMap::new();
    {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id, ts, source_offset, preview FROM prompt_events",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Lead {
                    host: r.get(1)?,
                    project_id: r.get(2)?,
                    ts: r.get(3)?,
                    offset: r.get::<_, i64>(4)? as u64,
                    preview: r.get(5)?,
                },
            ))
        })?;
        for row in rows {
            let (sid, lead) = row?;
            leads.entry(sid).or_default().push(lead);
        }
    }

    // 2) 작업 사실: events의 main-chain assistant_turn·compaction — 세션별로 모은다.
    let mut evs: BTreeMap<String, Vec<EvRow>> = BTreeMap::new();
    {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, kind, ts, source_offset, is_sidechain, tok_input, tok_cache_read, tok_cache_create
             FROM events WHERE kind IN ('assistant_turn','compaction')",
        )?;
        let rows = stmt.query_map([], |r| {
            let sid: String = r.get(0)?;
            let kind: String = r.get(1)?;
            let ts: Option<String> = r.get(2)?;
            let offset = r.get::<_, i64>(3)? as u64;
            let is_side = r.get::<_, i64>(4)? != 0;
            let ti = r.get::<_, i64>(5)? as u64;
            let tcr = r.get::<_, i64>(6)? as u64;
            let tcc = r.get::<_, i64>(7)? as u64;
            // main-chain만: sidechain(서브에이전트) 활동은 사용자 컨텍스트가 아니다.
            // inherited = 세 input 범주 전부(cache_create 포함) — 캐시 미스 턴 누락 방지.
            let ev = match (kind.as_str(), is_side) {
                ("compaction", false) => Some(Ev::Compaction),
                ("assistant_turn", false) => Some(Ev::Turn { inherited: ti + tcr + tcc }),
                _ => None,
            };
            Ok((sid, ev.map(|ev| EvRow { key: (ts.unwrap_or_default(), offset), ev })))
        })?;
        for row in rows {
            let (sid, maybe) = row?;
            if let Some(evrow) = maybe {
                evs.entry(sid).or_default().push(evrow);
            }
        }
    }

    // 3) 세션별 병합.
    let mut out = Vec::new();
    for (session_id, mut sess_leads) in leads {
        if sess_leads.is_empty() {
            continue;
        }
        sess_leads.sort_by(|a, b| {
            (a.ts.clone().unwrap_or_default(), a.offset)
                .cmp(&(b.ts.clone().unwrap_or_default(), b.offset))
        });
        let lead_keys: Vec<OrderKey> = sess_leads
            .iter()
            .map(|l| (l.ts.clone().unwrap_or_default(), l.offset))
            .collect();
        let mut episodes: Vec<Episode> = sess_leads
            .iter()
            .map(|l| Episode {
                session_id: session_id.clone(),
                host: l.host.clone(),
                project_id: l.project_id.clone(),
                lead_preview: l.preview.clone(),
                lead_offset: l.offset,
                first_ts: l.ts.clone(),
                inherited_ctx: 0,
                had_compaction: false,
            })
            .collect();
        let mut turn_set = vec![false; episodes.len()];

        if let Some(mut sess_evs) = evs.remove(&session_id) {
            sess_evs.sort_by(|a, b| a.key.cmp(&b.key));
            for evrow in sess_evs {
                // ev가 속한 에피소드 = key <= ev.key 인 마지막 lead.
                let idx = match lead_keys.iter().rposition(|k| *k <= evrow.key) {
                    Some(i) => i,
                    None => continue, // 첫 실질 프롬프트 이전 이벤트 → 무시.
                };
                match evrow.ev {
                    Ev::Turn { inherited } => {
                        if !turn_set[idx] {
                            episodes[idx].inherited_ctx = inherited;
                            turn_set[idx] = true;
                        }
                    }
                    Ev::Compaction => episodes[idx].had_compaction = true,
                }
            }
        }
        out.extend(episodes);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    // 관찰 무관 — 세그먼터는 창 필터를 하지 않는다. 고정 ts로 순서만 검증.
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

    fn turn(session: &str, offset: u64, input: u64, cache_read: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-a{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input, cache_read, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn compaction(session: &str, offset: u64, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(format!("{session}-c{offset}")),
            parent_uuid: None, is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: offset, msg_id: None,
            kind: EventKind::Compaction,
        }
    }

    #[test]
    fn inherited_context_includes_cache_creation() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 캐시 미스 턴: 물려받은 컨텍스트가 cache_create로 청구됨(input·cache_read는 작음).
        let mut t = turn("s1", 1, 5_000, 0, "2026-07-01T10:00:01Z");
        if let EventKind::AssistantTurn { usage, .. } = &mut t.kind {
            usage.cache_creation = 55_000;
        }
        store
            .upsert_events(&[prompt("s1", 0, "캐시 미스로 시작하는 실질 작업 지시", "2026-07-01T10:00:00Z"), t])
            .unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(
            eps[0].inherited_ctx, 60_000,
            "inherited = tok_input + tok_cache_read + tok_cache_create"
        );
    }

    #[test]
    fn segments_by_substantive_prompt_and_attaches_first_turn_context() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 에피소드 A: 실질 프롬프트 → 첫 턴(input 40k + cache 20k = 60k) → 둘째 턴(무시)
        // 에피소드 B: 실질 프롬프트 → 첫 턴(input 5k)
        store.upsert_events(&[
            prompt("s1", 0, "첫 작업을 시작해줘 자세히", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 40_000, 20_000, "2026-07-01T10:00:01Z"),
            turn("s1", 2, 99_000, 0, "2026-07-01T10:00:02Z"),
            prompt("s1", 3, "다른 작업으로 넘어가자 상세히", "2026-07-01T11:00:00Z"),
            turn("s1", 4, 5_000, 0, "2026-07-01T11:00:01Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 2, "실질 프롬프트 2개 → 에피소드 2개");
        assert_eq!(eps[0].lead_offset, 0);
        assert_eq!(eps[0].inherited_ctx, 60_000, "첫 main-chain 턴의 input+cache_read");
        assert_eq!(eps[0].had_compaction, false);
        assert_eq!(eps[1].lead_offset, 3);
        assert_eq!(eps[1].inherited_ctx, 5_000);
        assert_eq!(eps[0].lead_preview, "첫 작업을 시작해줘 자세히");
    }

    #[test]
    fn glue_prompt_does_not_open_new_episode() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            prompt("s1", 0, "긴 실질 작업 지시입니다", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 50_000, 0, "2026-07-01T10:00:01Z"),
            // 접착 프롬프트(<8자) — normalize None → prompt_events 미삽입 → 경계 아님
            prompt("s1", 2, "계속", "2026-07-01T10:05:00Z"),
            turn("s1", 3, 55_000, 0, "2026-07-01T10:05:01Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 1, "접착 프롬프트는 새 에피소드를 열지 않는다");
        assert_eq!(eps[0].inherited_ctx, 50_000, "첫 턴만 상속으로 잡힌다");
    }

    #[test]
    fn sidechain_turn_is_not_counted_as_inherited_context() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut side = turn("s1", 1, 90_000, 0, "2026-07-01T10:00:01Z");
        side.is_sidechain = true;
        store.upsert_events(&[
            prompt("s1", 0, "실질 작업 지시 문장입니다", "2026-07-01T10:00:00Z"),
            side, // sidechain 턴 먼저 와도 무시
            turn("s1", 2, 3_000, 0, "2026-07-01T10:00:02Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].inherited_ctx, 3_000, "sidechain 턴 제외, 첫 main-chain 턴만");
    }

    #[test]
    fn session_without_substantive_prompt_is_skipped() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", 0, 70_000, 0, "2026-07-01T10:00:00Z"), // 프롬프트 없이 턴만
        ]).unwrap();
        assert!(segment_all(&store).unwrap().is_empty());
    }

    #[test]
    fn compaction_within_episode_sets_flag() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            prompt("s1", 0, "첫 에피소드 실질 지시야", "2026-07-01T10:00:00Z"),
            turn("s1", 1, 10_000, 0, "2026-07-01T10:00:01Z"),
            prompt("s1", 2, "둘째 에피소드 실질 지시야", "2026-07-01T11:00:00Z"),
            compaction("s1", 3, "2026-07-01T11:00:30Z"),
            turn("s1", 4, 20_000, 0, "2026-07-01T11:00:40Z"),
        ]).unwrap();

        let eps = segment_all(&store).unwrap();
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].had_compaction, false);
        assert_eq!(eps[1].had_compaction, true, "compaction이 든 에피소드만 true");
    }
}
