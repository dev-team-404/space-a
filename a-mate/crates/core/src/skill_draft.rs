//! R6 후속 — 반복 지시 Finding을 **실제 SKILL.md 초안**으로 전환.
//! 킥오프 3대 차별점 중 하나("반복 작업은 Skill로 전환된다")의 마지막 반 걸음:
//! v1 R6는 "묶으세요"라고 제안만 했다. 여기서는 그 반복이 실제로 어떤 도구 시퀀스였는지
//! 세션에서 모아, 붙여넣기만 하면 되는 `.claude/skills/<name>/SKILL.md` 초안을 만든다.
//!
//! 정밀도의 선: **재료 수집은 결정론(SQL)**, **서사(설명·단계 문장)만 LLM**.
//! 엔진이 없으면 결정론 골격(skeleton)으로도 바로 쓸 수 있는 초안을 낸다.

use crate::diary::engine::Engine;
use crate::rules::r6_repeated_prompts::normalize;
use crate::store::SqliteStore;
use anyhow::{anyhow, Result};

/// 초안을 만들 재료 — 전부 로컬 파생 신호.
#[derive(Debug, Clone)]
pub struct DraftContext {
    /// 대표 프롬프트(원문 미리보기)
    pub representative: String,
    /// 같은 지시로 열린 세션 수
    pub session_count: u64,
    /// 그 세션들의 프롬프트 변형 표본(최대 5개, 중복 제거)
    pub sample_prompts: Vec<String>,
    /// 그 세션들에서 실제로 쓴 도구 상위 (raw_name, 호출수)
    pub top_tools: Vec<(String, u64)>,
}

/// R6 finding의 evidence(대표 프롬프트)로 세션을 되짚어 재료를 모은다(단일 norm).
/// 3개 호출부(judge·CLI·make-draft 커맨드) 호환용 — 내부적으로 gather_context_multi에 위임.
pub fn gather_context(
    store: &SqliteStore,
    host: &str,
    representative: &str,
) -> Result<DraftContext> {
    let Some(target) = normalize(representative) else {
        return Err(anyhow!("대표 프롬프트가 너무 짧아 초안 대상이 아닙니다"));
    };
    gather_context_multi(store, host, representative, &[target])
}

/// A — 느슨한 묶음의 **여러 변형(norm60)** 세션을 되짚어 재료를 모은다.
/// 세션·원문 중복 제거 후 표본 5개, 도구 상위 집계. representative는 표시용 대표(앵커).
pub fn gather_context_multi(
    store: &SqliteStore,
    host: &str,
    representative: &str,
    norms: &[String],
) -> Result<DraftContext> {
    let mut matched_ids: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for (sid, preview) in store.prompt_sessions_for_norms(host, norms)? {
        if !matched_ids.iter().any(|s| s == &sid) {
            matched_ids.push(sid);
        }
        let trimmed = preview.trim().to_string();
        if !trimmed.is_empty() && !samples.iter().any(|s| s == &trimmed) {
            samples.push(trimmed);
        }
    }
    samples.truncate(5);
    let top_tools = store.tool_usage_for_sessions(&matched_ids)?;
    Ok(DraftContext {
        representative: representative.trim().to_string(),
        session_count: matched_ids.len() as u64,
        sample_prompts: samples,
        top_tools,
    })
}

/// R6 finding evidence로 초안 재료를 모은다. `member_norms`(A 느슨한 묶음)가 있으면 묶음 전체,
/// 없으면(구버전 finding) 대표 하나로 폴백. judge·CLI 공통 진입점 — 카드가 센 세션·도구를
/// 초안도 그대로 보게 해 카드/초안 불일치를 막는다.
pub fn gather_context_for_finding(
    store: &SqliteStore,
    host: &str,
    representative: &str,
    evidence: &serde_json::Value,
) -> Result<DraftContext> {
    let norms: Vec<String> = evidence
        .get("member_norms")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if norms.is_empty() {
        gather_context(store, host, representative)
    } else {
        gather_context_multi(store, host, representative, &norms)
    }
}

/// 스킬 디렉터리/커맨드 이름용 슬러그 — 영숫자+하이픈, 소문자, 40자 컷.
/// 한글 등 비ASCII만 남으면 안정적 해시 접미로 폴백(빈 이름 금지).
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let slug: String = out.trim_matches('-').chars().take(40).collect();
    if slug.is_empty() {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(name.as_bytes());
        format!("workflow-{:02x}{:02x}", d[0], d[1])
    } else {
        slug
    }
}

/// 엔진 없이도 바로 쓸 수 있는 결정론 골격 — LLM 실패/미설정 시 폴백이자 회귀 테스트 기준.
pub fn skeleton_draft(ctx: &DraftContext) -> String {
    let slug = slugify(&ctx.representative);
    let tools = if ctx.top_tools.is_empty() {
        "- (이 반복에서 도구 호출이 기록되지 않았습니다)".to_string()
    } else {
        ctx.top_tools
            .iter()
            .map(|(t, n)| format!("- `{t}` — {n}회"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let samples = ctx
        .sample_prompts
        .iter()
        .map(|p| format!("- \"{}\"", p.replace('"', "'")))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\n\
name: {slug}\n\
description: {desc}\n\
---\n\n\
# {slug}\n\n\
최근 {count}개 세션에서 **같은 지시**를 반복했습니다. 매번 다시 설명하는 대신 이 스킬로 묶으세요.\n\n\
## 언제 쓰나\n\n\
{samples}\n\n\
같은 의도의 요청이 들어오면 이 스킬을 호출합니다.\n\n\
## 반복에서 실제로 쓴 도구\n\n\
{tools}\n\n\
## 절차\n\n\
1. 위 도구들을 정해진 순서로 실행합니다.\n\
2. 매번 바뀌는 값(파일·경로·날짜 등)만 인자로 받습니다.\n\
3. 결과를 사용자가 기대하는 형식으로 요약합니다.\n\n\
> 이 초안은 a-mate가 반복 패턴을 감지해 자동 생성했습니다. 절차 문장을 실제 워크플로에 맞게 다듬어 저장하세요.\n",
        slug = slug,
        desc = ctx.representative.chars().take(80).collect::<String>(),
        count = ctx.session_count,
        samples = if samples.is_empty() { "- (표본 없음)".into() } else { samples },
        tools = tools,
    )
}

/// LLM에게 SKILL.md를 쓰게 하는 프롬프트. 결정론 골격을 초안으로 주고 **다듬게** 한다 —
/// 빈 응답으로도 최소한 골격만큼은 보장하고, 모델은 절차 문장·인자화만 개선하면 된다.
fn draft_prompt(ctx: &DraftContext) -> (String, String) {
    let system = "당신은 Claude Code 스킬 작성 전문가입니다. \
주어진 SKILL.md 초안을 **더 구체적이고 실행 가능하게 다듬으세요**. \
규칙: (1) YAML 프런트매터(name, description)를 유지하되 name은 소문자+하이픈 슬러그, description은 한 문장. \
(2) '## 언제 쓰나', '## 절차'(번호 목록) 섹션을 반드시 포함. \
(3) 절차는 주어진 도구 목록을 실제 순서로 반영하고, 매번 바뀌는 값은 인자로 빼세요. \
(4) 출력은 순수 마크다운 SKILL.md 본문만 — 코드펜스로 전체를 감싸거나 사족을 붙이지 마세요. \
한국어로 작성하세요."
        .to_string();

    let skeleton = skeleton_draft(ctx);
    let user = format!(
        "다음은 반복 지시에서 자동 생성한 SKILL.md 초안입니다. 이 골격을 유지·보강해 완성하세요.\n\n{skeleton}",
    );
    (system, user)
}

/// 엔진 출력이 코드펜스로 감싸여 오면 벗겨 순수 SKILL.md만 남긴다.
fn strip_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // 첫 줄(언어 태그) 제거 후 마지막 ``` 이전까지
        let body = rest.splitn(2, '\n').nth(1).unwrap_or("");
        if let Some(end) = body.rfind("```") {
            return body[..end].trim().to_string();
        }
        return body.trim().to_string();
    }
    t.to_string()
}

/// 초안 생성 결과.
pub struct SkillDraft {
    pub markdown: String,
    /// 저장 시 쓸 디렉터리 슬러그
    pub slug: String,
    /// LLM이 생성했는가(아니면 결정론 골격 폴백)
    pub llm_generated: bool,
}

/// 재료(ctx)로 초안을 만든다. 엔진이 있으면 LLM, 없거나 실패하면 골격.
pub fn build_draft(ctx: &DraftContext, engine: Option<&dyn Engine>) -> SkillDraft {
    if let Some(eng) = engine {
        let (system, user) = draft_prompt(ctx);
        if let Ok(out) = eng.generate(&system, &user) {
            let md = strip_fence(&out.text);
            // 골격보다 부실하면(프런트매터·절차 누락 또는 너무 짧음) 결정론 골격을 택한다.
            let has_frontmatter = md.contains("name:") && md.contains("description:");
            let has_procedure = md.contains("## 절차") || md.contains("절차");
            if has_frontmatter && has_procedure && md.len() >= 180 {
                let slug = extract_name(&md).unwrap_or_else(|| slugify(&ctx.representative));
                return SkillDraft { markdown: md, slug, llm_generated: true };
            }
        }
    }
    let md = skeleton_draft(ctx);
    let slug = extract_name(&md).unwrap_or_else(|| slugify(&ctx.representative));
    SkillDraft { markdown: md, slug, llm_generated: false }
}

/// 초안을 `<base>/<slug>/SKILL.md`에 쓴다. 같은 이름이 있으면 `-2`,`-3`… 접미로
/// 기존 스킬을 보호(덮어쓰지 않음). 저장 경로를 반환. base는 보통 `~/.claude/skills`.
pub fn write_draft(base: &std::path::Path, slug: &str, markdown: &str) -> Result<std::path::PathBuf> {
    let clean = slugify(slug);
    let mut dir = base.join(&clean);
    let mut n = 2;
    while dir.exists() {
        dir = base.join(format!("{clean}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, markdown)?;
    Ok(path)
}

/// 프런트매터의 name: 값을 슬러그화해 회수.
fn extract_name(md: &str) -> Option<String> {
    for line in md.lines().take(10) {
        if let Some(v) = line.trim().strip_prefix("name:") {
            let v = v.trim();
            if !v.is_empty() {
                return Some(slugify(v));
            }
        }
    }
    None
}

/// 프런트매터(맨 위 `---`…`---`)의 `name:` 값을 주어진 슬러그로 교체한다.
/// 판정이 제안한 이름으로 저장 슬러그를 바꿀 때 SKILL.md 안의 정체성(name)도 함께 맞춰
/// 디렉터리 이름과 프런트매터가 어긋나지 않게 한다. name: 줄이 없으면 원본 그대로 반환.
pub fn set_frontmatter_name(markdown: &str, slug: &str) -> String {
    let mut lines: Vec<&str> = markdown.lines().collect();
    let replaced = format!("name: {slug}");
    let mut in_fm = false;
    let mut target = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if i == 0 {
            if t == "---" { in_fm = true; continue; }
            break; // 프런트매터 없음
        }
        if in_fm && t == "---" { break; } // 프런트매터 끝
        if in_fm && t.starts_with("name:") { target = Some(i); break; }
    }
    match target {
        Some(i) => {
            lines[i] = &replaced;
            let joined = lines.join("\n");
            if markdown.ends_with('\n') { format!("{joined}\n") } else { joined }
        }
        None => markdown.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diary::engine::MockEngine;
    use crate::model::{EventKind, NormalizedEvent, ToolKind};

    fn seed(store: &SqliteStore, sess: &str, prompt: &str, tools: &[&str]) {
        let now = chrono::Utc::now().to_rfc3339();
        let mut evs = vec![NormalizedEvent {
            source_agent: "claude-code".into(),
            schema_version: "t".into(),
            host: "Windows".into(),
            project_id: "p".into(),
            session_id: sess.into(),
            uuid: Some(format!("{sess}-u0")),
            parent_uuid: None,
            is_sidechain: false,
            ts: Some(now.clone()),
            source_file: "s.jsonl".into(),
            source_offset: 0,
            msg_id: None,
            kind: EventKind::UserPrompt { preview: prompt.into(), is_command: false },
        }];
        for (i, t) in tools.iter().enumerate() {
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(),
                schema_version: "t".into(),
                host: "Windows".into(),
                project_id: "p".into(),
                session_id: sess.into(),
                uuid: Some(format!("{sess}-t{i}")),
                parent_uuid: None,
                is_sidechain: false,
                ts: Some(now.clone()),
                source_file: "s.jsonl".into(),
                source_offset: (i + 1) as u64,
                msg_id: None,
                kind: EventKind::ToolCall {
                    kind: ToolKind::from_raw_name(t),
                    raw_name: (*t).into(),
                    target: None,
                    tool_use_id: Some(format!("{sess}-tu{i}")),
                },
            });
        }
        store.upsert_events(&evs).unwrap();
    }

    #[test]
    fn gather_collects_sessions_and_tools() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed(&store, "s1", "매일 아침 판매 리포트 뽑아줘", &["Bash", "Read"]);
        seed(&store, "s2", "매일  아침 판매 리포트 뽑아줘 ", &["Bash", "Write"]);
        seed(&store, "s3", "완전 다른 요청", &["Grep"]);
        let ctx = gather_context(&store, "Windows", "매일 아침 판매 리포트 뽑아줘").unwrap();
        assert_eq!(ctx.session_count, 2); // s1,s2 정규화 동치, s3 제외
        let tool_names: Vec<&str> = ctx.top_tools.iter().map(|(t, _)| t.as_str()).collect();
        assert!(tool_names.contains(&"Bash"));
        assert!(!tool_names.contains(&"Grep")); // 다른 지시의 도구는 안 섞임
    }

    #[test]
    fn skeleton_is_valid_skill_md() {
        let ctx = DraftContext {
            representative: "매일 아침 판매 리포트 뽑아줘".into(),
            session_count: 3,
            sample_prompts: vec!["매일 아침 판매 리포트 뽑아줘".into()],
            top_tools: vec![("Bash".into(), 5), ("Read".into(), 2)],
        };
        let md = skeleton_draft(&ctx);
        assert!(md.starts_with("---\nname:"));
        assert!(md.contains("description:"));
        assert!(md.contains("## 절차"));
        assert!(md.contains("`Bash` — 5회"));
    }

    #[test]
    fn build_uses_engine_when_valid() {
        let ctx = DraftContext {
            representative: "리포트 뽑아줘".into(),
            session_count: 3,
            sample_prompts: vec!["리포트 뽑아줘".into()],
            top_tools: vec![("Bash".into(), 5)],
        };
        let good = MockEngine {
            canned: "---\nname: daily-report\ndescription: 매일 판매 리포트를 생성한다\n---\n\n\
# daily-report\n\n## 언제 쓰나\n\n매일 아침 판매 집계가 필요할 때 이 스킬을 호출합니다.\n\n\
## 절차\n\n1. Bash로 원장을 집계합니다.\n2. 날짜 범위를 인자로 받습니다.\n\
3. 결과를 마크다운 표로 요약합니다.\n".into(),
        };
        let d = build_draft(&ctx, Some(&good));
        assert!(d.llm_generated);
        assert_eq!(d.slug, "daily-report");
    }

    #[test]
    fn build_falls_back_when_engine_output_invalid() {
        let ctx = DraftContext {
            representative: "리포트 뽑아줘".into(),
            session_count: 3,
            sample_prompts: vec![],
            top_tools: vec![],
        };
        let bad = MockEngine { canned: "죄송하지만 못 만들겠습니다".into() };
        let d = build_draft(&ctx, Some(&bad));
        assert!(!d.llm_generated); // 프런트매터 없음 → 골격 폴백
        assert!(d.markdown.contains("## 절차"));
    }

    #[test]
    fn strip_fence_unwraps_markdown_block() {
        let s = "```markdown\n---\nname: x\n---\nbody\n```";
        assert_eq!(strip_fence(s), "---\nname: x\n---\nbody");
    }

    #[test]
    fn slugify_handles_korean_only() {
        assert!(slugify("판매 리포트").starts_with("workflow-"));
        assert_eq!(slugify("Daily Sales Report!!"), "daily-sales-report");
    }

    #[test]
    fn set_frontmatter_name_syncs_identity_with_slug() {
        let md = "---\nname: old-name\ndescription: 무언가\n---\n\n# old-name\n\n본문\n";
        let out = set_frontmatter_name(md, "new-name");
        assert!(out.contains("name: new-name"));
        assert!(!out.contains("name: old-name"));
        // 프런트매터 밖 본문의 텍스트는 건드리지 않는다
        assert!(out.contains("# old-name"));
        assert!(out.contains("description: 무언가"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn set_frontmatter_name_noop_without_frontmatter() {
        let md = "name: not-frontmatter\n본문뿐";
        assert_eq!(set_frontmatter_name(md, "x"), md);
    }

    #[test]
    fn write_draft_creates_file_and_avoids_clobber() {
        let tmp = std::env::temp_dir().join(format!("amate-skilltest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        // 1회차: skills/daily-report/SKILL.md
        let p1 = write_draft(&tmp, "daily-report", "one").unwrap();
        assert!(p1.ends_with("daily-report/SKILL.md") || p1.ends_with("daily-report\\SKILL.md"));
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), "one");
        // 2회차 같은 슬러그: 기존 보호 → daily-report-2
        let p2 = write_draft(&tmp, "daily-report", "two").unwrap();
        assert_ne!(p1, p2);
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), "one"); // 원본 불변
        assert_eq!(std::fs::read_to_string(&p2).unwrap(), "two");
        assert!(p2.to_string_lossy().contains("daily-report-2"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
