---
status: done
archived: 2026-07-26
---

# 봇 탭 + 마스코트 아이덴티티 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a-mate 미니홈피 "나" 탭을 "봇"으로 재구성하고, 마스코트 이름·호칭·MBTI 외관/문체·미리보기 기반 마스코트 생성을 도입한다.

**Architecture:** 코어(`crates/core`)에 호칭(`owner_title`)·MBTI 값을 프롬프트로 흘리는 배관과 변주 시드 기반 마스코트 묘사를 넣고, Tauri 셸(`src-tauri`)이 설정에서 값을 읽어 주입하며 preview/commit 커맨드로 미리보기·저장을 분리한다. 프론트(`src`)는 "봇" 탭 UI와 마스코트 생성 섹션을 그린다.

**Tech Stack:** Rust (Cargo workspace, `cargo test`), Tauri v2, Svelte 5 + Vite + TypeScript (Vitest). Windows 전용.

## Global Constraints

- 커밋 메시지는 **영어 Conventional Commits** (`feat`/`fix`/`refactor`/`test`/`docs`, scope `agent`). 본문 필요시. 커밋 푸터에 `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **Windows PowerShell**에서 빌드/테스트 — WSL 금지. Rust: `cargo test`(워크스페이스), 프론트: `npm test`. `a-mate/`에서 실행.
- Tauri **v2 API만** (v1 금지). 무거운 처리는 Rust, 프론트는 렌더만.
- `RobotSpec` 계약(6슬롯: antenna·head·eyes·body·arms·palette)은 **불변** — 프론트 폴백 렌더러가 소비. 외관 확장은 전부 `sprite.rs` 묘사문 쪽.
- 호칭 미설정 기본값은 문자열 `"주인"`. MBTI는 `mascot::normalize_mbti`로 정규화(유효 4글자 대문자 또는 `None`).
- 작업 디렉터리는 워크트리 `D:\Project\space-a\.claude\worktrees\bot-tab-mascot` (브랜치 `feat/bot-tab-mascot-identity`). 모든 경로는 이 루트 기준의 `a-mate/...`.

---

## File Structure

**코어(`a-mate/crates/core/src/`)**
- `mascot.rs` — `mbti_voice_hint`·`DEFAULT_OWNER_TITLE` 신설, 한마디·잡담 프롬프트에 호칭+MBTI 주입.
- `sprite.rs` — `character_description` 시그니처 변경(변주 시드+MBTI), `mbti_traits` 신설.
- `chat.rs` — `ChatContext`·`CoachingBrief`에 `honorific`·`mbti` 추가, 채팅·코칭 프롬프트 치환, `assemble_coaching_brief` 설정 주입.
- `diary/mod.rs` — `DiaryConfig`에 `mbti` 추가, 일기·idle 프롬프트에 MBTI 주입(호칭은 기존).

**Tauri(`a-mate/src-tauri/src/`)**
- `commands.rs` — `Profile`/`profile_get`/`profile_set`에 `owner_title`, `chat_context_inner` 주입, `mascot_preview`/`mascot_commit` 신설, `regenerate_sprite` 제거, `owner_title` 헬퍼, §F 등록 payload.
- `pipeline.rs` — `maybe_generate_sprite` 새 시그니처(uuid 시드), 일기 `DiaryConfig` 설정 주입.
- `lib.rs` — invoke_handler 목록에서 `regenerate_sprite` 제거, `mascot_preview`·`mascot_commit` 추가.
- `life_client.rs`(core지만 Life 전용) — `register_profile`에 `owner_os_user` payload(§F).

**프론트(`a-mate/src/`)**
- `lib/api.ts` — `Profile.owner_title`, `mascotPreview`/`mascotCommit`, `regenerateSprite` 제거.
- `lib/ui/settings/groups.ts` — `me` 라벨 `봇`.
- `lib/ui/settings/MeGroup.svelte` — 섹션 제목·필드·MBTI 안내·재생성 제거·마스코트 생성 섹션.
- `lib/ui/settings/ConnectionGroup.svelte` — "캐릭터 재생성" 버튼/함수 제거.

---

## Task 1: core `mbti_voice_hint` + `DEFAULT_OWNER_TITLE` (mascot.rs)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (뒤쪽, `normalize_mbti` 근처)
- Test: 같은 파일 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub const DEFAULT_OWNER_TITLE: &str = "주인";` · `pub fn mbti_voice_hint(mbti: Option<&str>) -> String` (MBTI 4축 톤 지침 문자열; 미설정 시 `String::new()`).

- [ ] **Step 1: Write the failing test** — `mascot.rs`의 `mod tests` 안에 추가:

```rust
#[test]
fn mbti_voice_hint_covers_axes_and_empty() {
    assert_eq!(mbti_voice_hint(None), "");
    assert_eq!(mbti_voice_hint(Some("bad")), ""); // 무효 → 빈 문자열
    let t = mbti_voice_hint(Some("INTJ"));
    assert!(t.contains("사실") && t.contains("냉정")); // T: 팩트 기반 냉정
    assert!(t.contains("차분"));                       // I
    assert!(t.contains("비유") || t.contains("큰 그림")); // N
    assert!(t.contains("결론"));                       // J
    let f = mbti_voice_hint(Some("ENFP"));
    assert!(f.contains("공감") || f.contains("따뜻"));  // F
    assert!(f.contains("활기") || f.contains("감탄"));  // E
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent_mentor mbti_voice_hint_covers_axes_and_empty`
Expected: FAIL — `cannot find function mbti_voice_hint`

- [ ] **Step 3: Write minimal implementation** — `normalize_mbti` 함수 바로 아래에 추가:

```rust
/// 호칭 기본값 — owner_title 미설정 시 이 문자열을 쓴다(전 채널 공용).
pub const DEFAULT_OWNER_TITLE: &str = "주인";

/// MBTI 4축 → 마스코트 발화 톤 지침. 유효 MBTI가 아니면 빈 문자열(기존 페르소나 유지).
/// 기본 페르소나(1인칭·능청) 위에 성향 색을 얹는 용도 — 일기·한마디·잡담·채팅·코칭 공용.
pub fn mbti_voice_hint(mbti: Option<&str>) -> String {
    let Some(m) = mbti.and_then(normalize_mbti) else {
        return String::new();
    };
    let b = m.as_bytes();
    let ei = if b[0] == b'E' { "말은 활기차게, 감탄사·리액션을 곁들여" } else { "말은 차분하고 사색적으로, 담백하게 절제해" };
    let sn = if b[1] == b'S' { "구체적인 사실과 디테일 위주로" } else { "비유와 큰 그림, 아이디어를 곁들여" };
    let tf = if b[2] == b'T' { "감정 완충은 최소로 사실·수치에 근거해 냉정하고 직설적으로" } else { "공감과 따뜻함을 담아 관계 중심으로" };
    let jp = if b[3] == b'J' { "정돈된 결론 중심으로" } else { "유연하고 개방적으로 여지를 남기며" };
    format!(
        " 성향({m}) 반영: {tf} 말하되, {ei}, {sn}, {jp} 표현하세요. \
         (단 마스코트 특유의 능청스러운 1인칭 톤은 유지합니다.)"
    )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p agent_mentor mbti_voice_hint_covers_axes_and_empty`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): add mbti_voice_hint and DEFAULT_OWNER_TITLE helpers"
```

---

## Task 2: core `character_description` 변주 시드 + MBTI 외관 (sprite.rs)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (`extended_traits`·`character_description`·`sprite_for_seed`)
- Test: 같은 파일 `mod tests`

**Interfaces:**
- Consumes: `crate::mascot::RobotSpec`, `crate::mascot::normalize_mbti`.
- Produces: `pub fn character_description(spec: &RobotSpec, mbti: Option<&str>, seed: &str) -> String` (시그니처 변경 — 기존 `(spec, identity)` 대체). `mbti_traits`는 내부(private).
- **Breaking**: 기존 호출부(`commands.rs`·`pipeline.rs`·`sprite_for_seed`)는 Task 6/7에서 새 시그니처로 갱신. 이 태스크에서는 `sprite_for_seed`만 함께 고친다.

- [ ] **Step 1: Write the failing test** — `sprite.rs`의 `mod tests`에 추가(기존 `description_is_deterministic_and_covers_slots`는 Step 3에서 새 시그니처로 수정):

```rust
#[test]
fn description_varies_by_seed_and_reflects_mbti() {
    let spec = robot_spec_for("seed-A");
    // 같은 (spec, mbti, seed) → 결정론
    let a1 = character_description(&spec, Some("INTJ"), "v1");
    let a2 = character_description(&spec, Some("INTJ"), "v1");
    assert_eq!(a1, a2);
    // 다른 변주 시드 → 달라질 수 있다(표본에서 최소 2종)
    use std::collections::HashSet;
    let set: HashSet<_> = (0..12)
        .map(|i| character_description(&spec, Some("INTJ"), &format!("v{i}")))
        .collect();
    assert!(set.len() > 1, "변주 시드로 묘사가 달라져야 함 (distinct={})", set.len());
    // 로봇 어휘 유지
    assert!(a1.contains("ROBOT") && a1.contains("leg units"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent_mentor description_varies_by_seed_and_reflects_mbti`
Expected: FAIL — 컴파일 에러(인자 개수 불일치) 또는 함수 시그니처 불일치

- [ ] **Step 3: Write implementation** — `extended_traits`를 MBTI 인지 `mbti_traits`로 대체하고 `character_description` 시그니처를 바꾼다.

`extended_traits` 함수를 아래로 **교체**:

```rust
/// MBTI 성향 + 변주 시드 → (체형, 마감, 악세사리, 스타일 무드) 묘사 조각.
/// 각 축의 *부분집합*을 MBTI 성향으로 정의하고 변주 시드 해시가 그 안에서 고른다
/// (같은 타입=일관 무드, 시드마다 다른 조합). MBTI 미설정이면 편향 없이 전체에서 고른다.
fn mbti_traits(mbti: Option<&str>, seed: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(seed.as_bytes());
    let pick = |group: &[&'static str], byte: u8| group[(byte as usize) % group.len()];
    let m = mbti.and_then(crate::mascot::normalize_mbti);
    let mb = m.as_ref().map(|s| s.as_bytes());

    // 체형 (S/N): S=단단·컴팩트 / N=슬렌더·경량
    const BUILD_S: [&str; 2] = ["a compact sturdy body", "a solid grounded build"];
    const BUILD_N: [&str; 2] = ["a slender lightweight body", "a tall willowy build"];
    const BUILD_ANY: [&str; 3] = ["a compact body", "a slender body", "a sturdy build"];
    let build = match mb { Some(b) if b[1] == b'S' => pick(&BUILD_S, d[6]),
                           Some(b) if b[1] == b'N' => pick(&BUILD_N, d[6]),
                           _ => pick(&BUILD_ANY, d[6]) };

    // 마감·색온도 (T/F): T=차가운 금속 / F=따뜻·부드러움
    const FINISH_T: [&str; 2] = ["a cool brushed-steel finish", "a charcoal matte finish"];
    const FINISH_F: [&str; 2] = ["a warm cream plastic finish", "a soft matte-white finish"];
    const FINISH_ANY: [&str; 4] = ["a matte white finish", "a brushed steel finish", "a cream plastic finish", "a charcoal matte finish"];
    let finish = match mb { Some(b) if b[2] == b'T' => pick(&FINISH_T, d[7]),
                            Some(b) if b[2] == b'F' => pick(&FINISH_F, d[7]),
                            _ => pick(&FINISH_ANY, d[7]) };

    // 악세사리: 성향 그룹(N_T 분석가 / N_F 외교관 / S_J 관리자 / S_P 탐험가) 테마
    const ACC_NT: [&str; 3] = ["with a slim backpack module", "with a utility tool belt", "with a tiny status light on its chest"];
    const ACC_NF: [&str; 3] = ["with headphone-style side units", "with a small shoulder lamp", "with soft glowing trim"];
    const ACC_SJ: [&str; 3] = ["with a utility tool belt", "with a tiny status light on its chest", ""];
    const ACC_SP: [&str; 3] = ["with a small shoulder lamp", "", "with a light travel pack"];
    const ACC_ANY: [&str; 6] = ["", "with a small shoulder lamp", "with a slim backpack module", "with headphone-style side units", "with a tiny status light on its chest", "with a utility tool belt"];
    let accessory = match mb {
        Some(b) if b[1] == b'N' && b[2] == b'T' => pick(&ACC_NT, d[8]),
        Some(b) if b[1] == b'N' && b[2] == b'F' => pick(&ACC_NF, d[8]),
        Some(b) if b[1] == b'S' && b[3] == b'J' => pick(&ACC_SJ, d[8]),
        Some(b) if b[1] == b'S' && b[3] == b'P' => pick(&ACC_SP, d[8]),
        _ => pick(&ACC_ANY, d[8]),
    };

    // 스타일 무드 (S/N, 재미 요소): S=깔끔·단정 / N=개성·예술·몽환
    let styling = match mb {
        Some(b) if b[1] == b'S' => ", with a clean, tidy, conventional look",
        Some(b) if b[1] == b'N' => ", with a quirky, artistic mix-and-match look and a dreamy vibe",
        _ => "",
    };
    (build, finish, accessory, styling)
}
```

그리고 `character_description`를 아래로 **교체**(HEAD/EYES/COLORS/POSE/chassis 블록은 기존 그대로 두고, 시그니처와 traits 호출·format!만 변경):

```rust
/// 시드 스펙 + MBTI + 변주 시드 → **로봇** 묘사. 화풍은 레퍼런스(치비 픽셀) 유지.
/// `seed`가 체형/마감/악세/무드 변주를 좌우한다(재생성마다 다른 후보).
pub fn character_description(spec: &crate::mascot::RobotSpec, mbti: Option<&str>, seed: &str) -> String {
    const HEAD: [&str; 6] = [
        "a rounded helmet-shaped head with a short antenna",
        "a boxy head with rounded corners and two small side vents",
        "a dome head with a wide visor band",
        "a rounded head with small ear-discs on both sides",
        "a tall head unit with a blinking status light on top",
        "a compact head with a flat top panel and no antenna",
    ];
    const EYES: [&str; 6] = [
        "two glowing oval eyes behind a dark visor",
        "big round glowing eyes with bright highlights",
        "calm narrow glowing eye slits",
        "sparkling square eyes with bright highlights",
        "gentle droopy glowing eyes",
        "cheerful curved glowing eyes like a smile",
    ];
    const COLORS: [(&str, &str, &str); 8] = [
        ("navy", "grey", "cyan"),
        ("blue", "steel blue", "sky blue"),
        ("green", "grey", "lime"),
        ("pink", "mauve", "magenta"),
        ("purple", "dark grey", "violet"),
        ("mustard yellow", "slate", "amber"),
        ("teal", "grey", "mint"),
        ("slate grey", "charcoal", "white"),
    ];
    const POSE: [&str; 6] = [
        "arms relaxed at its sides",
        "hands resting on its hips",
        "one hand raised in a small wave",
        "both hands together in front",
        "holding a small toolbox",
        "hands behind its back",
    ];
    let (cc, lc, ac) = COLORS[(spec.palette as usize) % 8];
    let (build, finish, accessory, styling) = mbti_traits(mbti, seed);
    let chassis = match (spec.body as usize) % 6 {
        0 => format!("a {cc} rounded chest plate with a small lit panel"),
        1 => format!("a {cc} armored torso with shoulder pauldrons"),
        2 => format!("a plain {cc} torso with a single seam line"),
        3 => format!("a {cc} padded torso with soft rounded edges"),
        4 => format!("a white torso with {cc} trim and {cc} buttons"),
        _ => format!("a {cc} torso with an exposed cable harness"),
    };
    let acc_part = if accessory.is_empty() { String::new() } else { format!(", {accessory}") };
    format!(
        "a chibi pixel-art ROBOT (not a human) with {build} and {finish}: {head}, {eyes}, \
         {chassis}, {lc} leg units with flat feet, {ac} glowing accents{acc_part}, {pose}{styling}",
        head = HEAD[(spec.antenna as usize) % 6],
        eyes = EYES[(spec.eyes as usize) % 6],
        pose = POSE[(spec.arms as usize) % 6],
    )
}
```

`sprite_for_seed`를 새 시그니처로 갱신(방문자 스프라이트는 MBTI 없음·시드 결정론 유지):

```rust
pub fn sprite_for_seed(cfg: &SpriteConfig, seed: &str) -> Result<Vec<u8>> {
    let spec = crate::mascot::robot_spec_for(seed);
    let desc = character_description(&spec, None, seed);
    generate(cfg, &desc)
}
```

기존 테스트 2개를 새 시그니처로 수정:

```rust
#[test]
fn description_is_deterministic_and_covers_slots() {
    let id = "DESKTOP-X|user";
    let spec = robot_spec_for(id);
    let d1 = character_description(&spec, None, id);
    let d2 = character_description(&spec, None, id);
    assert_eq!(d1, d2);
    assert!(d1.contains("chibi pixel-art"));
    assert!(d1.contains("ROBOT"), "로봇으로 묘사되어야 함: {d1}");
    assert!(d1.contains("leg units"), "다리 유닛 슬롯 포함: {d1}");
    for human in ["skin", "hair", "pants", "sneakers", "hoodie"] {
        assert!(!d1.contains(human), "인물 어휘 '{human}'가 남아있음: {d1}");
    }
}

#[test]
fn different_seeds_can_differ() {
    let a = character_description(&robot_spec_for("A|a"), None, "A|a");
    let b = character_description(&robot_spec_for("B|bbbb"), None, "B|bbbb");
    assert_ne!(a, b);
}
```

기존 `extended_traits_add_build_finish_accessory_axes` 테스트는 `extended_traits` 제거로 무효 → 삭제.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent_mentor --lib sprite::`
Expected: PASS (신규 + 수정 테스트 모두)

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/sprite.rs
git commit -m "feat(agent): make character_description vary by seed and reflect MBTI traits"
```

---

## Task 3: core chat — 호칭·MBTI를 ChatContext·CoachingBrief·프롬프트에 (chat.rs)

**Files:**
- Modify: `a-mate/crates/core/src/chat.rs` (`ChatContext`·`CoachingBrief` 구조체, `build_chat_system_prompt`, `build_coaching_system_prompt`, `assemble_coaching_brief`, `memory_section`, 테스트 헬퍼)
- Test: 같은 파일 `mod tests`

**Interfaces:**
- Consumes: `crate::mascot::{mbti_voice_hint, normalize_mbti, DEFAULT_OWNER_TITLE}`.
- Produces: `ChatContext { …, honorific: String, mbti: Option<String> }`, `CoachingBrief { …, honorific: String, mbti: Option<String> }`. (한마디·잡담(Task 4)이 `ChatContext.honorific`/`.mbti`를 소비.)

- [ ] **Step 1: Write the failing test** — 기존 `mod tests`에 추가:

```rust
#[test]
fn chat_prompt_uses_custom_honorific_and_mbti_voice() {
    let mut ctx = /* 기존 테스트의 ctx 헬퍼 사용 */ sample_ctx();
    ctx.honorific = "대장".into();
    ctx.mbti = Some("INTJ".into());
    let p = build_chat_system_prompt(&ctx);
    assert!(p.contains("대장"));            // 커스텀 호칭
    assert!(!p.contains("'주인'"));         // 기본 호칭 리터럴 부재
    assert!(p.contains("냉정"));            // T 성향 톤
}
```

> 참고: `chat.rs` 테스트에 `ctx`를 만드는 기존 헬퍼가 없으면, 아래 Step 3에서 `sample_ctx()`를 추가한다. 이미 있으면 그 헬퍼에 `honorific`/`mbti` 필드를 더한다.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent_mentor chat_prompt_uses_custom_honorific_and_mbti_voice`
Expected: FAIL — `ChatContext`에 `honorific`/`mbti` 필드 없음(컴파일 에러)

- [ ] **Step 3: Write implementation**

`ChatContext`에 필드 추가(구조체 끝):

```rust
    /// 마스코트가 주인을 부르는 호칭(owner_title, 기본 "주인").
    pub honorific: String,
    /// 정규화된 MBTI(없으면 None) — 발화 톤에 반영.
    pub mbti: Option<String>,
```

`CoachingBrief`에도 동일 두 필드 추가.

`memory_section`을 호칭 파라미터화:

```rust
fn memory_section(memories: &[String], honorific: &str) -> String {
    let block = crate::memory::memory_block(memories);
    if block.is_empty() {
        String::new()
    } else {
        format!("\n\n[{honorific}에 대해 기억한 것 — 관련될 때만 자연스럽게 언급, 없는 사실 지어내지 말 것]\n{block}")
    }
}
```

`build_chat_system_prompt`의 첫 문단·`mem` 인자·user 치환:

```rust
    let honorific = &ctx.honorific;
    let voice = crate::mascot::mbti_voice_hint(ctx.mbti.as_deref());
    format!(
        "당신은 {honorific}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 대화하며 \
         사용자를 '{honorific}'이라고 부릅니다. 답은 짧고 대화체로.{voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 컨텍스트의 사실과 수치에만 근거해 답하고, \
         컨텍스트에 없는 구체적 수치를 지어내지 마세요. 모르면 모른다고 말하세요. \
         코칭 지적에 대해 물으면 그 지적의 근거(무엇이)와 개선 방향(어떻게)을 쉽게 풀어 설명하세요.\n\n\
         [오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}{mem}",
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
        mem = memory_section(&ctx.memories, honorific),
    )
```

`build_coaching_system_prompt`도 동일 방식: `let honorific = &brief.honorific; let voice = crate::mascot::mbti_voice_hint(brief.mbti.as_deref());` 후 첫 문단 `당신은 {honorific}의 AX(에이전트 활용) 튜터입니다. … 1인칭·대화체로 사용자를 '{honorific}'이라 부르되 … 간결하게.{voice}`, `[이번 주 브리프 — {honorific}]`, 그리고 `mem = memory_section(&brief.memories, honorific)`로 치환(기존 `{user}`·`'주인'` 전부 제거).

`assemble_coaching_brief`에서 설정 읽어 주입 — 함수 끝의 `Ok(CoachingBrief { … })`에 추가:

```rust
    let honorific = store
        .get_setting("owner_title").ok().flatten()
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::mascot::DEFAULT_OWNER_TITLE.to_string());
    let mbti = store
        .get_setting("user_mbti").ok().flatten()
        .and_then(|m| crate::mascot::normalize_mbti(&m));
```

그리고 `Ok(CoachingBrief { user_name, …, memories, honorific, mbti })`.

`sample_ctx()` 테스트 헬퍼가 없으면 `mod tests`에 추가(있으면 필드만 보강):

```rust
#[cfg(test)]
fn sample_ctx() -> ChatContext {
    ChatContext {
        user_name: "jibin".into(), date: "2026-07-26".into(),
        session_count: 3, tok_input: 100, tok_output: 200, est_tokens_saved_total: 4200,
        findings: vec![("detail".into(), "action".into())],
        memories: vec![], honorific: "주인".into(), mbti: None,
    }
}
```

> 기존 chat.rs 테스트가 `ChatContext { … }`를 직접 리터럴로 만드는 곳이 있으면 전부 `honorific: "주인".into(), mbti: None,`를 추가한다. `assemble_coaching_brief`를 검증하는 기존 테스트도 통과해야 한다.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent_mentor --lib chat::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/chat.rs
git commit -m "feat(agent): thread owner_title honorific and MBTI voice into chat/coaching prompts"
```

---

## Task 4: core mascot — 호칭·MBTI를 한마디·잡담 프롬프트에 (mascot.rs)

**Files:**
- Modify: `a-mate/crates/core/src/mascot.rs` (`build_daily_line_prompt`, `build_chatter_prompt`, `comic_directives`, 테스트 헬퍼)
- Test: 같은 파일 `mod tests`·`daily_line_tests`·`chatter_tests`

**Interfaces:**
- Consumes: `ChatContext.honorific`·`ChatContext.mbti`(Task 3), `mbti_voice_hint`(Task 1).

- [ ] **Step 1: Write the failing test** — `daily_line_tests`에 추가:

```rust
#[test]
fn daily_line_prompt_uses_custom_honorific_and_mbti() {
    let mut c = ctx(3, 100, 200, 1);
    c.honorific = "대장".into();
    c.mbti = Some("INTJ".into());
    let p = build_daily_line_prompt(&c);
    assert!(p.contains("대장"));
    assert!(!p.contains("'주인'"));
    assert!(p.contains("냉정")); // T 성향
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent_mentor daily_line_prompt_uses_custom_honorific_and_mbti`
Expected: FAIL — `ctx` 헬퍼에 `honorific`/`mbti` 없음(컴파일 에러) 또는 문구 불일치

- [ ] **Step 3: Write implementation**

`build_daily_line_prompt`:

```rust
pub fn build_daily_line_prompt(ctx: &crate::chat::ChatContext) -> String {
    format!(
        "당신은 {honorific}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '{honorific}'이라고 부릅니다.{voice} \
         \
         {voice_guidance} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}\n\n\
         오늘 하루의 기분이나 재치를 담아 짧은 한 문장(40자 이내)으로 표현하세요. \
         대화가 아니라 오늘을 한마디로 요약하는 혼잣말입니다. 딱 한 문장만 출력하세요.",
        honorific = ctx.honorific,
        voice = mbti_voice_hint(ctx.mbti.as_deref()),
        voice_guidance = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
    )
}
```

`build_chatter_prompt`도 동일 패턴: 첫 문단 `{honorific}`·`'{honorific}'`·`{voice}` 삽입, `user = ctx.user_name` 제거.

`comic_directives`의 예시 문구에서 "주인"을 호칭으로(함수는 `ctx` 접근 가능):

```rust
    let h = &ctx.honorific;
    if work.is_weekend {
        items.push(format!(
            "- 오늘은 주말인데 {h}이 또 나와서 일하고 있다 — \"주말에 또 나왔어? 일중독이야ㅋㅋ\" 같은 능청."
        ));
    }
    // long_work 케이스 등은 그대로. 마지막 안내문 "다마고치가 주인을 놀리는 톤." → "다마고치가 {h}을 놀리는 톤."
```

테스트 헬퍼 `ctx(...)` 2곳(`daily_line_tests`·`chatter_tests`)에 필드 추가:

```rust
        memories: vec![],
        honorific: "주인".into(),
        mbti: None,
```

기존 `prompt_carries_persona_facts_voice_and_one_line_directive` 등은 `p.contains("주인")`을 그대로 통과(기본 호칭이 "주인"이므로). `assert!(p.contains("jibin"))`처럼 `user_name`을 검사하던 부분이 있으면 삭제(더 이상 프롬프트에 없음). `chatter_prompt_carries_persona_facts_voice_and_directives`의 `assert!(p.contains("jibin"))` 제거.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent_mentor --lib mascot::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/mascot.rs
git commit -m "feat(agent): thread honorific and MBTI voice into daily line and chatter prompts"
```

---

## Task 5: core diary — MBTI 문체 주입 (diary/mod.rs)

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`DiaryConfig`, `Default`, `build_system_prompt`, `build_idle_prompt`, 테스트)
- Test: 같은 파일 `mod tests`

**Interfaces:**
- Consumes: `crate::mascot::mbti_voice_hint`.
- Produces: `DiaryConfig { …, mbti: Option<String> }`.

- [ ] **Step 1: Write the failing test** — `mod tests`에 추가:

```rust
#[test]
fn diary_prompt_injects_mbti_voice() {
    let mut cfg = DiaryConfig::default();
    cfg.mbti = Some("INTJ".into());
    let p = build_idle_prompt(&cfg, &sample_idle(), &[]);
    assert!(p.contains("냉정")); // T 성향 톤이 idle 프롬프트에도
}
```

> `sample_idle()`가 없으면 기존 idle 테스트가 만드는 `IdleContext` 리터럴을 재사용하거나 헬퍼로 추출한다(기존 `build_idle_prompt` 테스트 참고, 라인 ~2410).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent_mentor diary_prompt_injects_mbti_voice`
Expected: FAIL — `DiaryConfig`에 `mbti` 없음

- [ ] **Step 3: Write implementation**

`DiaryConfig`에 필드 추가:

```rust
    pub mbti: Option<String>,
```

`impl Default`에 `mbti: None,` 추가.

`build_system_prompt`(일기)와 `build_idle_prompt`(idle) 각각에서 `voice_guidance()` 주입부 근처에 MBTI 톤을 덧붙인다. 두 함수 모두 `format!` 안에서 `{voice}`(= `voice_guidance()`)를 쓰므로, 그 뒤에 이어 붙일 인자 `mbti_voice`를 추가:

- 함수 상단에 `let mbti_voice = crate::mascot::mbti_voice_hint(cfg.mbti.as_deref());`
- `format!`의 문자열에서 `{voice}` 다음에 `{mbti_voice}`를 넣고, 인자 목록에 `mbti_voice = mbti_voice,` 추가.

(예: `… {voice}{mbti_voice} …` 형태. `voice_guidance`가 문장으로 끝나므로 공백 포함된 `mbti_voice`가 자연스럽게 이어진다.)

라인 1217 등 `DiaryConfig { … honorific: "주인".into(), … }`를 리터럴로 만드는 테스트가 있으면 `mbti: None,` 추가(또는 `..DiaryConfig::default()`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent_mentor --lib diary::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/diary/mod.rs
git commit -m "feat(agent): inject MBTI voice hint into diary and idle prompts"
```

---

## Task 6: tauri — profile owner_title + 설정 주입 (commands.rs, pipeline.rs)

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`Profile`, `profile_get`, `profile_set`, `chat_context_inner`, `owner_title` 헬퍼)
- Modify: `a-mate/src-tauri/src/pipeline.rs` (일기 `DiaryConfig` 구성 — 라인 ~380)
- Test: `commands.rs` `mod tests`

**Interfaces:**
- Consumes: Task 3의 `ChatContext.honorific/.mbti`, Task 5의 `DiaryConfig.mbti`.
- Produces: `pub(crate) fn owner_title(store: &SqliteStore) -> String`; `Profile { …, owner_title: String }`; `profile_set(state, name, org, mbti, owner_title)`.

- [ ] **Step 1: Write the failing test** — `commands.rs` `mod tests`에 추가(기존 테스트가 쓰는 in-memory store 헬퍼 패턴을 따른다):

```rust
#[test]
fn owner_title_defaults_and_roundtrips() {
    let store = SqliteStore::open_in_memory().unwrap(); // 기존 테스트의 생성 방식에 맞춰 조정
    assert_eq!(owner_title(&store), "주인");
    store.set_setting("owner_title", "대장").unwrap();
    assert_eq!(owner_title(&store), "대장");
    store.set_setting("owner_title", "   ").unwrap(); // 공백 → 기본값
    assert_eq!(owner_title(&store), "주인");
}
```

> `SqliteStore` 생성은 이 파일의 기존 테스트(예: `chat_context_inner`를 테스트하는 1384 근처)와 동일한 방식으로 맞춘다.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-mentor-app owner_title_defaults_and_roundtrips`
Expected: FAIL — `cannot find function owner_title`

- [ ] **Step 3: Write implementation**

`owner_title` 헬퍼(파일 내 `sprite_identity` 근처):

```rust
/// 호칭 설정(owner_title) — 없거나 공백이면 기본 "주인".
pub(crate) fn owner_title(store: &SqliteStore) -> String {
    store.get_setting("owner_title").ok().flatten()
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| agent_mentor::mascot::DEFAULT_OWNER_TITLE.to_string())
}
```

`Profile` 구조체에 `pub owner_title: String,` 추가.

`profile_get`의 `Ok(Profile { … })`에 `owner_title: owner_title(&guard),` 추가.

`profile_set` 시그니처에 `owner_title: String` 추가하고 저장:

```rust
pub fn profile_set(
    state: State<AppState>,
    name: String,
    org: String,
    mbti: String,
    owner_title: String,
) -> Result<Profile, String> {
    // … 기존 name/mbti/org 검증 …
    let title = owner_title.trim();
    let title = if title.is_empty() { agent_mentor::mascot::DEFAULT_OWNER_TITLE } else { title };
    {
        let guard = lock(&state)?;
        // … 기존 set_setting 들 …
        guard.set_setting("owner_title", title).map_err(|e| e.to_string())?;
    }
    profile_get(state)
}
```

`chat_context_inner`의 `Ok(ChatContext { … })`에 주입:

```rust
        memories,
        honorific: owner_title(store),
        mbti: store.get_setting("user_mbti").ok().flatten()
            .and_then(|m| agent_mentor::mascot::normalize_mbti(&m)),
```

`pipeline.rs` 라인 ~380 일기 cfg 구성부를 설정 주입으로 교체:

```rust
                    let honorific = crate::commands::owner_title(&store); // store 접근 가능한 스코프에서
                    let mbti = store.get_setting("user_mbti").ok().flatten()
                        .and_then(|m| agent_mentor::mascot::normalize_mbti(&m));
                    let cfg = DiaryConfig { vault_dir: vault.clone(), honorific, mbti, ..DiaryConfig::default() };
```

> 주의: `pipeline.rs` 380 라인의 store 락 스코프를 확인해, `store`가 살아있는 구간에서 `honorific`/`mbti`를 읽어 `cfg`에 담는다. idle 일기(`render_idle_diary`)를 별도 cfg로 만드는 곳이 있으면 동일하게 주입한다.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-mentor-app` (또는 워크스페이스 전체 `cargo test`)
Expected: PASS. 컴파일 에러 없이 `profile_set`/`profile_get`/`chat_context_inner` 갱신 반영.

- [ ] **Step 5: Commit**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(agent): persist owner_title and inject honorific/MBTI into contexts"
```

---

## Task 7: tauri — 마스코트 preview/commit + regenerate 제거 + 시드 배선 (commands.rs, pipeline.rs, lib.rs)

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`regenerate_sprite` 제거, `mascot_preview`/`mascot_commit` 신설)
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`maybe_generate_sprite` 새 시그니처 — Task 2)
- Modify: `a-mate/src-tauri/src/lib.rs` (invoke_handler 목록)
- Test: 수동(네트워크/FS) — 아래 Step에 명시

**Interfaces:**
- Consumes: Task 2 `character_description(spec, mbti, seed)`, `robot_spec_from_profile`, `resolve_sprite_cfg`, `sprite_identity`, `upload_cached_mascot`.
- Produces: `mascot_preview() -> Result<String, String>`(candidate base64), `mascot_commit() -> Result<(), String>`.

- [ ] **Step 1: Replace `regenerate_sprite` with `mascot_preview` + `mascot_commit`**

`commands.rs`의 `regenerate_sprite` 함수를 아래 두 함수로 **교체**:

```rust
/// 마스코트 미리보기 — 새 변주 시드로 후보를 생성해 sprite.candidate.png에 저장하고 base64 반환.
/// sprite.png(실사용본)는 건드리지 않는다. 네트워크는 락 밖.
#[tauri::command(async)]
pub fn mascot_preview(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    use base64::Engine as _;
    use tauri::Manager as _;
    let (cfg, mbti) = {
        let guard = lock(&state)?;
        let cfg = crate::resolve_sprite_cfg(&guard);
        let (_uuid, mbti) = sprite_identity(&guard)?;
        (cfg, mbti)
    };
    let Some(cfg) = cfg else {
        return Err("이미지 모델이 설정되지 않았어요 — 설정 → 연결 → 캐릭터 이미지에서 URL·키를 넣어주세요".into());
    };
    // 변주 시드 = 새 UUID(재생성마다 다른 후보). spec·묘사 모두 이 시드로 뽑는다.
    let seed = uuid::Uuid::new_v4().to_string();
    let spec = agent_mentor::mascot::robot_spec_from_profile(&seed, mbti.as_deref());
    let desc = agent_mentor::sprite::character_description(&spec, mbti.as_deref(), &seed);
    let png = agent_mentor::sprite::generate(&cfg, &desc).map_err(|e| e.to_string())?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("sprite.candidate.png"), &png).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&png))
}

/// 마스코트 저장 — candidate를 실사용본(sprite.png)으로 승격하고 반영(emit + 허브 업로드).
#[tauri::command(async)]
pub fn mascot_commit(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    use tauri::{Emitter as _, Manager as _};
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let candidate = dir.join("sprite.candidate.png");
    let png = std::fs::read(&candidate)
        .map_err(|_| "저장할 미리보기가 없어요 — 먼저 '재생성'을 눌러주세요".to_string())?;
    std::fs::write(dir.join("sprite.png"), &png).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&candidate);
    if let Some(client) = hub_client(&state)? {
        let _ = upload_cached_mascot(&app, &client);
    }
    log::info!("마스코트 저장(commit) 완료");
    let _ = app.emit("sprite:ready", ());
    Ok(())
}
```

- [ ] **Step 2: `pipeline.rs` `maybe_generate_sprite` 새 시그니처 반영**

라인 ~767-768을 교체(첫 자동생성은 uuid 시드·결정론 유지):

```rust
        let spec = agent_mentor::mascot::robot_spec_from_profile(&uuid, mbti.as_deref());
        let desc = sprite::character_description(&spec, mbti.as_deref(), &uuid);
```

- [ ] **Step 3: `lib.rs` invoke_handler 목록 갱신**

`commands::regenerate_sprite,` 줄을 제거하고 그 자리에 추가:

```rust
                commands::mascot_preview,
                commands::mascot_commit,
```

- [ ] **Step 4: Build + 수동 검증**

Run: `cargo build -p agent-mentor-app`
Expected: 컴파일 성공(모든 `character_description` 호출부가 새 시그니처와 일치, `regenerate_sprite` 잔존 참조 없음).

수동(이미지 모델 설정 후 `npm run tauri dev`): 봇 탭 → 마스코트 생성 → "재생성" 시 미리보기 갱신, "저장" 시 마스코트 창 교체(`sprite:ready`). 저장 전에는 `sprite.png` 미변경.

- [ ] **Step 5: Commit**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/pipeline.rs a-mate/src-tauri/src/lib.rs
git commit -m "feat(agent): add mascot preview/commit flow and remove regenerate_sprite"
```

---

## Task 8: tauri — §F 주인 식별 데이터(owner_os_user) 전송 (life_client.rs, commands.rs)

**Files:**
- Modify: `a-mate/crates/core/src/life_client.rs` (`register`, `register_profile`)
- Modify: `a-mate/src-tauri/src/commands.rs` (`hub_connect` 등록 호출)
- Test: `life_client.rs` `mod tests`(있으면) 또는 신규 순수 테스트

**Interfaces:**
- Produces: `register_profile(base_url, api_key, name, mascot_seed, org, agent_uuid, owner_os_user)` — payload에 비어있지 않으면 `owner_os_user` 포함.

- [ ] **Step 1: Write the failing test** — `life_client.rs`에 payload 조립을 검증할 순수 헬퍼가 없다면, payload 구성만 별도 순수 함수로 뽑아 테스트한다. 최소 변경으로는 아래처럼 `register_profile`의 body 조립 로직을 그대로 두고, 인자만 추가하는 컴파일-레벨 변경이므로 **빌드 성공**을 게이트로 삼는다. (life_client는 네트워크 함수라 단위 테스트가 없다 — 기존에도 network 주석으로 제외.)

- [ ] **Step 2: Implement — `register_profile`에 `owner_os_user` 인자·payload 추가**

```rust
pub fn register(base_url: &str, api_key: Option<&str>, name: &str, mascot_seed: &str) -> Result<Value> {
    register_profile(base_url, api_key, name, mascot_seed, "", "", "")
}

pub fn register_profile(
    base_url: &str,
    api_key: Option<&str>,
    name: &str,
    mascot_seed: &str,
    org: &str,
    agent_uuid: &str,
    owner_os_user: &str,
) -> Result<Value> {
    let mut body = json!({ "name": name, "mascot_seed": mascot_seed });
    if !org.trim().is_empty() { body["org"] = json!(org); }
    if !agent_uuid.trim().is_empty() { body["agent_uuid"] = json!(agent_uuid); }
    if !owner_os_user.trim().is_empty() { body["owner_os_user"] = json!(owner_os_user); }
    // … 이하 기존 그대로 …
```

- [ ] **Step 3: `hub_connect` 호출부 갱신** — 라인 ~766의 `register_profile(...)` 호출에 OS 계정명 추가:

```rust
    let os_user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let v = life_client::register_profile(&url, key_opt.as_deref(), &user, &uuid, &org, &uuid, &os_user)
        .map_err(|e| e.to_string())?;
```

또한 라인 ~726의 안내 문구 "미니홈피 설정의 개인정보에서 이름을 먼저 입력하세요" → "봇 탭의 마스코트 정보에서 마스코트 이름을 먼저 입력하세요"로 갱신(라벨 변경 정합).

- [ ] **Step 4: Build to verify**

Run: `cargo build`
Expected: 성공. `register`/`register_profile` 다른 호출부가 있으면 새 인자에 맞춰 갱신(그 외 호출부 없음 확인: `register(`는 `register_profile` 래퍼로만 사용).

- [ ] **Step 5: Commit**

```bash
git add a-mate/crates/core/src/life_client.rs a-mate/src-tauri/src/commands.rs
git commit -m "feat(agent): send owner OS account in Life register payload (hidden owner identity)"
```

---

## Task 9: frontend api.ts — 프로필 owner_title + preview/commit (api.ts)

**Files:**
- Modify: `a-mate/src/lib/api.ts`
- Test: 타입체크(`npx tsc --noEmit`) + 이후 태스크의 컴포넌트가 소비

**Interfaces:**
- Produces: `Profile { …, owner_title: string }`, `profileSet(name, org, mbti, ownerTitle)`, `mascotPreview(): Promise<string>`, `mascotCommit(): Promise<void>`. `regenerateSprite` 제거.

- [ ] **Step 1: Implement** — `api.ts`에서:

`Profile` 인터페이스·`profileSet` 교체:

```ts
export interface Profile { name: string; org: string; uuid: string; mbti: string; owner_title: string }
export const profileGet = () => invoke<Profile>('profile_get');
export const profileSet = (name: string, org: string, mbti: string, ownerTitle: string) =>
  invoke<Profile>('profile_set', { name, org, mbti, ownerTitle });
```

`regenerateSprite`를 제거하고 preview/commit 추가:

```ts
/** 마스코트 후보를 새로 그려 미리보기 base64를 돌려준다(수십 초). 저장 전까지 실사용본 미변경. */
export async function mascotPreview(): Promise<string> {
  return await invoke<string>('mascot_preview');
}
/** 미리보기 후보를 실제 마스코트로 저장(전체 반영). 완료 시 sprite:ready. */
export async function mascotCommit(): Promise<void> {
  await invoke('mascot_commit');
}
```

- [ ] **Step 2: Verify types compile**

Run(`a-mate/`): `npx tsc --noEmit`
Expected: `regenerateSprite` 참조가 남아 에러가 나면(ConnectionGroup/MeGroup) 이후 태스크에서 제거되므로, 이 태스크 커밋 전 Task 10·11과 함께 타입체크가 통과해야 한다. 순서상 이 태스크는 커밋만 하고, 최종 타입체크는 Task 11 뒤에 수행한다.

- [ ] **Step 3: Commit**

```bash
git add a-mate/src/lib/api.ts
git commit -m "feat(agent): add mascotPreview/mascotCommit and owner_title to profile API"
```

---

## Task 10: frontend — "봇" 탭 UI (groups.ts, MeGroup.svelte)

**Files:**
- Modify: `a-mate/src/lib/ui/settings/groups.ts` (`me` 라벨)
- Modify: `a-mate/src/lib/ui/settings/MeGroup.svelte`
- Test: `a-mate/src/lib/ui/settings/groups.test.ts`

**Interfaces:**
- Consumes: Task 9의 `profileSet`, `mascotPreview`, `mascotCommit`, `getSprite`.

- [ ] **Step 1: Write the failing test** — `groups.test.ts`에 추가(기존 테스트 스타일에 맞춤):

```ts
import { SETTINGS_GROUPS } from './groups';
it('me 그룹 라벨은 "봇"', () => {
  expect(SETTINGS_GROUPS.find((g) => g.id === 'me')?.label).toBe('봇');
});
```

- [ ] **Step 2: Run test to verify it fails**

Run(`a-mate/`): `npm test -- groups`
Expected: FAIL — 라벨이 아직 `'나'`

- [ ] **Step 3: Implement**

`groups.ts`: `{ id: 'me', label: '나' }` → `{ id: 'me', label: '봇' }`.

`MeGroup.svelte` `<script>` 변경:
- import에 `regenerateSprite` 제거, `mascotPreview, mascotCommit, getSprite` 추가. `profileSet` 인자 4개로.
- `Profile` 초기값에 `owner_title: '주인'` 추가: `let prof = $state<Profile>({ name:'', org:'', uuid:'', mbti:'', owner_title:'주인' });`
- `saveProfile`에서 MBTI 재생성 분기 제거:

```ts
  async function saveProfile(){
    profStatus = busy('저장 중…');
    try {
      prof = await profileSet(prof.name, prof.org, profMbti, prof.owner_title);
      profMbti = prof.mbti;
      profStatus = ok('저장했어요.');
    } catch(e){ profStatus = err(e); }
  }
```

- 마스코트 생성 상태·함수 추가:

```ts
  let sprite = $state<string | null>(null);
  let hasCandidate = $state(false);
  let genStatus = $state<Status>(IDLE);
  async function loadSprite(){ sprite = await getSprite(); }
  loadSprite();
  async function regenMascot(){
    genStatus = busy('마스코트 그리는 중… 수십 초 걸릴 수 있어요');
    try { sprite = await mascotPreview(); hasCandidate = true; genStatus = ok('미리보기 완성 — 마음에 들면 저장하세요.'); }
    catch(e){ genStatus = err(`생성 실패: ${e}`); }
  }
  async function saveMascot(){
    genStatus = busy('저장 중…');
    try { await mascotCommit(); hasCandidate = false; genStatus = ok('마스코트를 저장했어요!'); }
    catch(e){ genStatus = err(e); }
  }
```

`MeGroup.svelte` 템플릿 변경:
- 첫 `<section>`의 `<h2>개인정보</h2>` → `<h2>마스코트 정보</h2>`, hint 문구를 마스코트 맥락으로: `<p class="hint">봇(마스코트)의 이름·성향입니다. 아이디는 자동 부여되며 바뀌지 않아요.</p>`
- "이름" 필드 라벨 → "마스코트 이름", placeholder 예시 유지:
  `<label class="field"><span>마스코트 이름</span><input type="text" bind:value={prof.name} placeholder="예: 둘쇠" spellcheck="false"/></label>`
- 조직 뒤(또는 이름 뒤)에 호칭 필드 추가:
  `<label class="field"><span>호칭 <em>(주인을 부르는 말)</em></span><input type="text" bind:value={prof.owner_title} placeholder="주인" spellcheck="false"/></label>`
- MBTI 필드에 안내를 인접 배치(필드 아래):
  `<p class="hint2">MBTI는 마스코트 외관 성향과 말투에 반영돼요. 외관은 아래 '마스코트 생성'에서 재생성·저장해야 실제로 바뀝니다.</p>`
- 기존 하단 `<p class="hint2">MBTI를 바꾸고 저장하면 …</p>` 제거.
- 첫 섹션 닫은 뒤, 주인 메모리 섹션 **앞에** 마스코트 생성 섹션 삽입:

```svelte
<section>
  <h2>마스코트 생성</h2>
  <p class="hint">MBTI·성향에 맞춰 마스코트를 그립니다. '재생성'으로 미리보고 '저장'을 눌러야 실제로 반영돼요. (연결 탭의 캐릭터 이미지 모델 설정 필요)</p>
  <div class="preview">
    {#if sprite}
      <img src={`data:image/png;base64,${sprite}`} alt="마스코트 미리보기"/>
    {:else}
      <span class="noimg">아직 생성된 마스코트가 없어요.</span>
    {/if}
  </div>
  <div class="actions">
    <button onclick={regenMascot} disabled={genStatus.kind==='busy'}>재생성</button>
    <button class="primary" onclick={saveMascot} disabled={!hasCandidate || genStatus.kind==='busy'}>저장</button>
  </div>
  <StatusLine status={genStatus}/>
</section>
```

- `<style>`에 미리보기 최소 스타일 추가:

```css
  .preview{margin-top:12px;display:flex;align-items:center;justify-content:center;min-height:140px;background:var(--surface-inset);border:1px solid var(--line);border-radius:8px}
  .preview img{max-height:180px;image-rendering:pixelated}
  .preview .noimg{color:var(--text-soft);font-size:12px}
```

- [ ] **Step 4: Run test to verify it passes**

Run(`a-mate/`): `npm test -- groups`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add a-mate/src/lib/ui/settings/groups.ts a-mate/src/lib/ui/settings/groups.test.ts a-mate/src/lib/ui/settings/MeGroup.svelte
git commit -m "feat(agent): rework Me tab into 봇 tab with mascot info and generation section"
```

---

## Task 11: frontend — 연결 탭 캐릭터 재생성 버튼 제거 (ConnectionGroup.svelte)

**Files:**
- Modify: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte`
- Test: 타입체크 + 전체 프론트 테스트

**Interfaces:**
- Consumes: (없음 — 제거만)

- [ ] **Step 1: Implement**

`ConnectionGroup.svelte`에서:
- import에서 `regenerateSprite` 제거(라인 6의 `imageSettingsGet, imageSettingsSet, imageTest, regenerateSprite,` → `imageSettingsGet, imageSettingsSet, imageTest,`).
- `regenerate` 함수(63-67) 삭제.
- "캐릭터 이미지" 섹션 actions의 `<button onclick={regenerate} …>캐릭터 재생성</button>`(라인 137) 삭제. 저장·연결 테스트 버튼은 유지.
- 섹션 hint의 "사람마다 한 번 생성해 캐시하므로 이후에는 호출하지 않습니다." 문구는 유지하되, 재생성 위치 안내를 추가: "캐릭터 생성·재생성은 봇 탭의 '마스코트 생성'에서 합니다."

- [ ] **Step 2: Verify full typecheck + tests**

Run(`a-mate/`): `npx tsc --noEmit` 그리고 `npm test`
Expected: `regenerateSprite` 참조 0개, 타입 통과, 기존 프론트 테스트 GREEN.

- [ ] **Step 3: Full Rust check**

Run(`a-mate/`): `cargo test`
Expected: 워크스페이스 전체 GREEN.

- [ ] **Step 4: Commit**

```bash
git add a-mate/src/lib/ui/settings/ConnectionGroup.svelte
git commit -m "feat(agent): remove character regenerate button from connection tab"
```

---

## Task 12: 문서 아카이브 (DoD)

**Files:**
- 이 plan + 스펙을 완료 처리.

- [ ] **Step 1:** `docs-archive` 스킬로 이 plan(`2026-07-26-bot-tab-mascot-identity.md`)과 스펙(`2026-07-26-bot-tab-mascot-identity-design.md`)을 `docs/archive/` 미러로 이동(ADR 0013). 후속 로드맵(`2026-07-26-life-social-diary-followups-roadmap.md`)은 **아카이브하지 않는다**(미완 후속).
- [ ] **Step 2:** Commit: `docs(agent): archive bot-tab mascot identity spec and plan`

---

## Self-Review

- **Spec coverage**: §A(Task 10·8 안내문구·groups) · §B(Task 7·10) · §C(Task 2·7) · §D(Task 1·3·4·6) · §E(Task 1·3·4·5) · §F(Task 8) · 커맨드 요약(Task 6·7) · 테스트(각 Task Step 1) — 전 항목 매핑됨.
- **Placeholder scan**: 모든 코드 스텝에 실제 코드 포함. "기존 헬퍼가 없으면 추가"류는 대안 코드까지 제시(sample_ctx/sample_idle). 네트워크·FS 경로(preview/commit, register)는 빌드+수동 검증으로 명시.
- **Type consistency**: `character_description(spec, mbti: Option<&str>, seed: &str)`가 Task 2·6·7·(sprite_for_seed)에서 일관. `ChatContext.honorific/.mbti`·`CoachingBrief.honorific/.mbti`·`DiaryConfig.mbti`가 정의(Task 3·5)와 소비(Task 4·6)에서 일치. `profileSet(name, org, mbti, ownerTitle)`가 api(Task 9)·컴포넌트(Task 10)·커맨드(Task 6)에서 일치. `mascot_preview`/`mascot_commit` 이름이 커맨드(Task 7)·핸들러(Task 7)·api(Task 9)에서 일치.
