//! 콘텐츠 큐레이션 파이프라인의 현실성 검증용 실행 데모 (해커톤 스파이크).
//!
//! 실제로 하는 일:
//!   1. in-memory SQLite에 한 페르소나의 사용 로그를 시드
//!   2. detect_profile()로 역량 사다리 위치를 감지 (결정론)
//!   3. Claude Code CHANGELOG를 **실제 네트워크로 fetch** (T1 피드, 실패해도 계속)
//!   4. 내장 팁(T3) + 피드를 프로필로 스코어링·정렬
//!   5. 결과를 stdout에 출력하고 127.0.0.1:8787에서 JSON으로 서빙
//!
//! 실행:  cargo run -p agent-mentor --example curation_server -- [junior|mid]
//! 확인:  curl -s localhost:8787 | jq

use agent_mentor::content::{ClaudeChangelogSource, ContentSource};
use agent_mentor::model::*;
use agent_mentor::profile::detect_profile;
use agent_mentor::store::SqliteStore;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

fn turn(session: &str, uuid: &str, model: &str) -> NormalizedEvent {
    NormalizedEvent {
        source_agent: "claude-code".into(), schema_version: "demo".into(),
        host: "Windows".into(), project_id: "c--users-jibin-proj".into(),
        session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
        is_sidechain: false, ts: Some("2026-07-14T10:00:00Z".into()),
        source_file: "s.jsonl".into(), source_offset: 0,
        msg_id: None,
        kind: EventKind::AssistantTurn {
            model: NormModel::from_raw_id(model),
            usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
        },
    }
}

/// 페르소나별 시드 이벤트 + (선택) 활성 finding.
fn seed(store: &SqliteStore, persona: &str) {
    match persona {
        // 이제 막 시작한 개발자: 전부 상위 모델, 스킬 0회 → 프론티어=모델 리터러시
        "junior" => {
            store.upsert_events(&[
                turn("s1", "u1", "claude-opus-4-8"),
                turn("s1", "u2", "claude-opus-4-8"),
                turn("s2", "u3", "claude-opus-4-8"),
            ]).unwrap();
        }
        // 중급: 모델 혼용(Lv0 통과)했지만 안 쓰는 MCP가 상주 → 프론티어=컨텍스트 위생
        _ => {
            store.upsert_events(&[
                turn("s1", "u1", "claude-opus-4-8"),
                turn("s1", "u2", "claude-haiku-4-5"),
                turn("s2", "u3", "claude-sonnet-4-6"),
            ]).unwrap();
            let f = agent_mentor::finding::Finding {
                rule_id: "R1".into(), severity: agent_mentor::finding::Severity::Warn,
                scope_host: Some("Windows".into()), scope_project: None,
                scope_kind: "host".into(), scope_ref: "context7".into(),
                evidence: serde_json::json!({"server":"context7","calls":0}),
                est_tokens_saved: 4200, prescription: None,
                dedup_key: "R1|Windows|context7".into(),
            };
            store.upsert_finding(&f, "2026-07-14T10:00:00Z").unwrap();
        }
    }
}

fn main() {
    let persona = std::env::args().nth(1).unwrap_or_else(|| "junior".into());

    // 1) 시드 + 2) 프로필 감지
    let store = SqliteStore::open_in_memory().unwrap();
    seed(&store, &persona);
    let profile = detect_profile(&store).unwrap();

    // 3) 피드 모으기: AGENT_MENTOR_CHANGELOG_FILE면 파일 파싱(오프라인), 아니면 실제 fetch.
    let src = ClaudeChangelogSource::default();
    let feed = match std::env::var("AGENT_MENTOR_CHANGELOG_FILE") {
        Ok(path) => std::fs::read_to_string(&path)
            .map(|md| { let n = src.parse_markdown(&md); eprintln!("[feed] changelog 파일 파싱 OK({path}) — {}건", n.len()); n })
            .unwrap_or_else(|e| { eprintln!("[feed] 파일 읽기 실패(계속): {e}"); vec![] }),
        Err(_) => src.fetch()
            .map(|n| { eprintln!("[feed] changelog fetch OK — {}건", n.len()); n })
            .unwrap_or_else(|e| { eprintln!("[feed] changelog fetch 실패(계속): {e}"); vec![] }),
    };

    // 4) **실제 프로덕션 경로**: ops::run_curation — 랭킹 + SQLite content_items에 persist +
    //    쿨다운 적용한 노출 목록 반환 (Tauri 파이프라인이 부르는 바로 그 함수).
    let now = "2026-07-14T10:00:00Z";
    let visible =
        agent_mentor::ops::run_curation(&store, feed, &[], now, &agent_mentor::ops::FeedPlan::all())
            .unwrap();
    let all = store.list_content(now, agent_mentor::content::CONTENT_COOLDOWN_DAYS, true).unwrap();

    let payload = serde_json::json!({
        "persona": persona,
        "frontier": profile.frontier().map(|d| d.key()),
        "profile": profile.dims.iter().map(|d| serde_json::json!({
            "dimension": d.dimension.key(),
            "mastery": format!("{:?}", d.mastery),
            "evidence": d.evidence,
        })).collect::<Vec<_>>(),
        "active_tags": profile.active_tags,
        "persisted_total": all.len(),   // content_items에 저장된 전체(숨김 포함)
        "feed": visible.iter().map(|r| serde_json::json!({
            "score": r.score,
            "kind": r.kind,
            "dimension": r.dimension,
            "title": r.title,
            "body": r.body,
            "url": r.source_url,
        })).collect::<Vec<_>>(),
    });
    let body = serde_json::to_string_pretty(&payload).unwrap();

    // stdout 요약 — 노출 목록(쿨다운·억제 반영된 최종 산출)
    println!("\n═══ 페르소나: {persona} · 프론티어: {:?} · persist {}건 → 노출 {}건 ═══",
        profile.frontier().map(|d| d.key()), all.len(), visible.len());
    for r in &visible {
        println!("  [{:>4}] {} · {}", r.score, r.dimension.as_deref().unwrap_or(&r.kind), r.title);
    }

    // 5) HTTP 서빙
    let listener = TcpListener::bind("127.0.0.1:8787").expect("8787 바인드 실패");
    eprintln!("\n[serve] http://127.0.0.1:8787 (curl로 확인, Ctrl+C 종료)");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        // 요청 라인만 읽고 버림 (데모)
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n\
             Access-Control-Allow-Origin: *\r\nConnection: close\r\n\
             Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(resp.as_bytes());
    }
}
