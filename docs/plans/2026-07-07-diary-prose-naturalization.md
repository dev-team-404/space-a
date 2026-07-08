# 다이어리 문체 자연화 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 다이어리 서사의 한국어 "AI티"(번역투·이중피동·과잉영어·기계적 나열·상투구·이모지 남발·리듬 획일·빈 헤지)를 생성 시점에 예방해, 마스코트 1인칭 일기를 진짜 사람이 쓴 것처럼 자연스럽게 만든다.

**Architecture:** im-not-ai(사후 탐지→수술적 수정 파이프라인)를 포팅하는 대신, 그 범주를 *생성 지침*으로 번역해 시스템 프롬프트에 흡수한다(예방). 재사용 가능한 `voice_guidance() -> &'static str` 조각을 새로 만들고, 기존 `build_system_prompt`가 `{voice}` 자리로 조합만 한다. §8.1a 1인칭 관점·정밀도의 선·occasions·`finding_advice`는 전부 무변경. 팩트 안전은 기존 정밀도의 선이 담당(im-not-ai의 30%/50% 안전장치는 예방 모드에 대응물 없음).

**Tech Stack:** Rust(크레이트 `agent-mentor` = core) · serde_json · 단위 테스트는 in-source `#[cfg(test)] mod tests`.

## Global Constraints

- **빌드(Windows, Git Bash) — 모든 cargo 호출 전 매번:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- 코어 테스트: `cargo test -p agent-mentor <filter>`.
- **정밀도의 선(불변):** 서사는 브리프 사실·수치만 인용, 지어내기 금지. 추정 토큰은 "추정/잠재"로 명시. 이 인식적 헤지(추정·잠재·"~로 보여요")는 문체 규칙이 제거하는 "빈 헤지"와 **혼동 금지 — carve-out으로 유지**.
- **관점 불변(§8.1a):** 주인 3인칭 지칭·1인칭 자기 일기·회고/다짐·유머·occasions. 이번 변경은 문체 조각 *추가*뿐, 기존 프롬프트 텍스트 삭제/수정 없음.
- **에이전트 추상화(CLAUDE.md):** 문체 조각은 에이전트별 하드코딩이 아니라 다이어리 생성 공통 지침. #1/#3이 재활용하도록 `&'static str` 반환.
- **이모지:** 완전 금지가 아니라 **남용 금지**(다마고치 캐릭터상 아주 가끔은 허용).

## File Structure

| 파일 | 책임 | 태스크 |
|---|---|---|
| `crates/core/src/diary/mod.rs` (신규 `voice_guidance`) | 문체 자연화 지침 조각(재사용 `&'static str`) | 1 |
| `crates/core/src/diary/mod.rs` (`build_system_prompt`) | `{voice}` 자리로 조각 조합 | 1 |
| `crates/core/src/diary/mod.rs` (`#[cfg(test)] mod tests`) | 조각 계약 + 조합 계약 테스트 | 1 |

## Deferred (이번 밖 — 후속 백로그)

- **#1 프로필 '오늘의 한마디'·#3 상주봇 말풍선**: 표면(DB 필드·날짜 캐시·말풍선 케이던스·카테고리·뮤트)과
  표면별 태스크 프롬프트. 이번엔 공유되는 *보이스 겹*만 `voice_guidance()`로 분리해 훅을 남긴다.
- **다이어리 재생성/E2E**: 코드 태스크 아님 — 아래 "Post-merge" 참조.

---

### Task 1: 다이어리 문체 자연화 — `voice_guidance()` 신설 + `build_system_prompt` 조합

**Files:**
- Modify: `crates/core/src/diary/mod.rs` (신규 `voice_guidance` 함수 — 기존 `build_system_prompt` 근처)
- Modify: `crates/core/src/diary/mod.rs:252-274` (`build_system_prompt`의 `format!`에 `{voice}` 자리·인자 추가)
- Test: `crates/core/src/diary/mod.rs` (`#[cfg(test)] mod tests`에 2개 테스트 추가)

**Interfaces:**
- Consumes: 기존 `build_system_prompt(cfg: &DiaryConfig) -> String`, `DiaryConfig::default()`.
- Produces: `pub fn voice_guidance() -> &'static str`. `build_system_prompt`가 반환 문자열에 `voice_guidance()`
  내용을 **verbatim 포함**(`{voice}` 인터폴레이션이라 `\` 연속행 접힘의 영향 없음).

- [ ] **Step 1: 실패하는 계약 테스트 2개를 tests 모듈에 추가**

`crates/core/src/diary/mod.rs`의 `#[cfg(test)] mod tests { ... }` 안(기존 `system_prompt_*` 테스트 근처)에 아래 두 테스트를 추가:

```rust
    #[test]
    fn voice_guidance_covers_key_anti_ai_directives() {
        let v = super::voice_guidance();
        // 긍정 보이스
        assert!(v.contains("구어체"));
        // 회피목록 핵심
        assert!(v.contains("번역투"));
        assert!(v.contains("피동"));           // 이중·과잉 피동
        assert!(v.contains("고유명"));         // 기술 고유명 보존(과잉 영어 예외)
        assert!(v.contains("이모지"));         // 이모지 남용 회피
        // 헤지 carve-out — 정밀도의 선이 요구하는 인식적 헤지는 유지
        assert!(v.contains("추정") && v.contains("잠재"));
        // form-only 예시(어색함 → 자연스러움)
        assert!(v.contains("→"));
    }

    #[test]
    fn build_system_prompt_embeds_voice_guidance() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains(super::voice_guidance())); // 조각이 그대로 배선됨
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent-mentor diary::tests::voice_guidance_covers_key_anti_ai_directives diary::tests::build_system_prompt_embeds_voice_guidance`
Expected: 컴파일 FAIL — `cannot find function `voice_guidance` in this scope` (아직 함수 없음).

- [ ] **Step 3: `voice_guidance()` 구현**

`crates/core/src/diary/mod.rs`의 `build_system_prompt` 함수 **바로 위**에 추가:

```rust
/// 다이어리 서사의 한국어 "AI티"를 줄이는 문체 가이드(생성 시 예방).
/// im-not-ai(사후 탐지→수정)의 범주를 생성 지침으로 번역한 것. 팩트 안전은 정밀도의 선이 담당.
/// #1 프로필 한마디·#3 상주봇 말풍선이 다른 태스크 프롬프트에서 재사용하도록 `&'static str` 반환.
pub fn voice_guidance() -> &'static str {
    "문체는 진짜 사람이 그날 하루를 캐주얼하게 적는 일기처럼 자연스럽게. \
     문장 길이와 종결어미를 다양하게 섞고(짧은 감탄·구어 종결을 간간이), 담백한 구어체로 쓰세요. \
     다음 'AI티'는 피하세요: \
     ① 번역투('~을 통해', '~에 대해', '작업을 진행/수행하였다' 같은 do/have류 직역), \
     ② 이중·과잉 피동('읽혀지다', '보여지다', '되어지다' → 능동이나 단일 피동으로), \
     ③ 굳이 안 써도 될 과잉 영어(단 기술 고유명 Read·Opus·MCP·플러그인/스킬 이름 등은 그대로 보존), \
     ④ 사실을 번호목록·불릿으로 기계적으로 나열하기(→ 하나의 이야기 흐름으로 녹이세요), \
     ⑤ AI 상투구('결론적으로', '종합하면', '시사하는 바가 크다', '~라고 할 수 있다'), \
     ⑥ 이모지 남발(아주 가끔이면 캐릭터상 괜찮지만 문장마다 붙이지 마세요), \
     ⑦ 리듬 획일('~했다. ~했다. ~했다.'처럼 같은 길이·같은 종결의 반복), \
     ⑧ 습관적으로 얼버무리는 빈 헤지('~인 것 같기도', '어느 정도', '다소'). \
     단, 브리프 수치·가설의 불확실성을 표시하는 헤지(추정·잠재·'~로 보여요')는 정밀도의 선이므로 \
     반드시 유지하세요 — 이 인식적 헤지와 ⑧의 빈 헤지를 혼동하지 마세요. \
     예시(형태만 참고, 내용은 브리프 사실만 쓸 것): \
     어색함 '오늘은 총 세 개의 세션을 통해 작업이 진행되었고, 같은 파일이 여러 번 읽혀지는 상황이 발생하였다' → \
     자연스러움 '오늘 세션 세 번. 같은 파일을 자꾸 다시 열었다 — 좀 헤맸네'."
}
```

- [ ] **Step 4: 계약 테스트 통과 확인**

Run: `cargo test -p agent-mentor diary::tests::voice_guidance_covers_key_anti_ai_directives`
Expected: PASS. (`build_system_prompt_embeds_voice_guidance`는 아직 FAIL — 다음 스텝에서 조합.)

- [ ] **Step 5: `build_system_prompt`에 `{voice}` 자리 조합**

`crates/core/src/diary/mod.rs`의 `build_system_prompt` `format!`에서, 톤·유머 문장 다음(정밀도의 선 문장 앞)에 `{voice}` 자리를 끼우고 named 인자를 추가한다. 아래 두 군데를 편집:

기존 (톤·유머 문장 끝 → 정밀도의 선 시작 사이):
```rust
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
```
교체:
```rust
         가볍고 유머러스하게, 다마고치풍의 능청과 장난기를 살려 쓰세요(단 과하지 않게). \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 JSON 브리프의 사실과 수치에만 근거해 서술하고, \
```

기존 (`format!` 인자 목록):
```rust
        honorific = cfg.honorific,
        tone = cfg.tone,
    )
```
교체:
```rust
        honorific = cfg.honorific,
        tone = cfg.tone,
        voice = voice_guidance(),
    )
```

- [ ] **Step 6: 조합 테스트 + 기존 계약 회귀 확인**

Run: `cargo test -p agent-mentor diary`
Expected: PASS — 신규 2개 + 기존 `system_prompt_uses_self_diary_perspective`·`system_prompt_has_humor_evidence_and_occasions_instructions`·`system_prompt_injects_tone_and_honorific` 모두 green(관점·호칭·유머·detail·suggested_action·occasions·정밀도의 선 문구 잔존).

- [ ] **Step 7: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 문체 자연화 지침 흡수 — voice_guidance() 신설 + build_system_prompt 조합 (스펙 §4·§5)"
```

---

## Post-merge (코드 태스크 아님 — 사용자 E2E)

스펙 §7. 이 브랜치 머지 후 유예해둔 다이어리 재생성으로 새 문체 육안 확인:

1. 앱 종료 → 앱 DB `%APPDATA%\dev.agentmentor.app\agent-mentor.db`의 `diary_index` 행 삭제.
2. 앱 재시작 → startup 스캔이 `missing_diary_dates`(7일 창·오늘 제외·엔진 필수)로 backfill 재생성.
3. 판정: 문체가 자연스러운지, 수치·고유명(정밀도의 선)이 보존됐는지, 헤지 carve-out(추정·잠재·"보여요")이 지켜졌는지.

(CLI DB = repo-root `./agent-mentor.db`는 앱 DB와 별개. LLM 엔진 `.env.ref`의 `LLM_BASE_URL`은 네이티브 Windows에선 `localhost:4444`로 대체 — 메모리 `build-env`.)
