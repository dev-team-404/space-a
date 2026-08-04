---
status: done
archived: 2026-08-03
---

# R24(F 컨텍스트 위생) 은퇴 — 설계

- **날짜**: 2026-07-23
- **상태**: 승인 (구현 대기)
- **컴포넌트**: a-mate (`crates/core`, `src/lib/ui`)
- **결정 요지**: R24(F) 룰과 **그 전용 인프라(에피소드 세그먼터)를 완전 제거**한다. 이미 쌓인 카드는 purge.
- **관련**: [코칭 가치 재설계](./2026-07-22-coaching-value-redesign-design.md) §4 F, [F 구현 계획(아카이브)](../plans/2026-07-22-coaching-f-context-hygiene.md)

## 1. 결정

**R24(F, 컨텍스트 위생) 룰을 은퇴하고, F가 유일 소비자였던 에피소드 세그먼터(`episode.rs`)까지
함께 제거한다.** 이미 사용자 DB에 쌓인 R24 카드는 다음 스캔에서 purge한다.

보존이 아니라 **제거**를 택한 이유는 §5에서 확정한 대로 세그먼터의 **소비자·흡수처가 전무**하기 때문이다
(§3.2). R10·R11이 "보존"된 것은 그 helper(`detect_bursts`)를 R7이 계속 쓰기 때문이었고,
R1/R2/R9/R12는 탐지 신호가 R13으로 흡수됐다 — 둘 다 정당한 잔존 근거가 있었다. F 세그먼터에는 없다.

## 2. 배경 — 관측된 문제

사용자가 코칭 카드(제목 "작업을 바꿀 때 컨텍스트를 끊으면 더 좋아요")에 대해 두 가지를 제기했다.

1. **반복적** — 같은 카드가 프로젝트마다 1장씩, 5개 프로젝트에서 5장 떴다.
2. **부정확 / 오탐** — "왜 `/clear`를 쓰라는지 모르겠다. 나는 컨텍스트를 무리하게 쓰지 않았다."

카드가 근거로 인용한 `sample_prompts`가 실제로는 **한 작업 안의 후속·마무리 질문**이었다
(예: "AF쪽에도 동일한 키를 추가하도록 되어 있지?", "이 세션 종료 전에 한 가지 확인 사항"). 작업 전환이 아니다.

## 3. 근본 원인

### 3.1 오탐 (불만 2) — 8자 경계가 작업 경계가 아님

R24는 **에피소드 세그먼터**(`episode.rs`) 위에서 동작한다. 세그먼터는 **8자 이상인 모든 `UserPrompt`를
새 에피소드(=새 작업)의 경계로 삼는다**([redesign §3① line 49](./2026-07-22-coaching-value-redesign-design.md)).
따라서 같은 작업의 후속 질문도 별개의 "작업"으로 세어진다.

R24 발화 신호는 "큰 컨텍스트(≥50k)를 물려받은 채 시작하는 에피소드 비율(carry_ratio)이 높음"이다.
그런데 한 세션에서 **깊게 이어지는 단일 작업**은 후속 질문마다 자연히 큰 컨텍스트를 물려받으므로
carry_ratio가 사실상 항상 ~100%가 되어, **정상적인 몰입 작업과 "never-clear 나쁜 습관"을 구분하지 못한다.**

핵심은 "상주 컨텍스트가 현재 작업과 **관련 있는가**"인데, 결정론 신호로는 이를 판정할 수 없다.

| 신호 | 관련성 판정 가능? |
|------|------------------|
| `inherited_ctx` 크기·비율 | ❌ 크기만 잼 (현재 방식, 오탐 원인) |
| `had_compaction` | ❌ "컸다"만 앎, "낡았다"는 모름 |
| 유휴 시간 간격 | △ 노이즈 프록시 — 다음날 같은 작업 이어감엔 오탐, 5분 만의 작업 전환엔 미탐 |

redesign §3① line 64가 이미 **"새 작업 vs 후속/교정 구분은 C(LLM)의 몫"**이라 못 박았으나, F는
"순수 결정론 룰(R8형)"로 설계되어 C를 쓰지 않는다 — 결정론으로는 원래부터 불가능한 판정을 시도했다.
1M 창 + 프롬프트 캐시 환경에서 비용·주의 희석 논리도 약하다(worst 세션 `max_inherited` 395k = 1M의 40% 미만).
**R24는 R10·R11과 같은 "비용 잔소리" 계열**로, 이번 재설계가 은퇴시킨 부류다(redesign §2).

### 3.2 세그먼터의 소비자·흡수처 부재 (제거 근거)

redesign §7은 에피소드 세그먼터를 "C·A·F 공유 척추"로 설계했으나, **머지된 현재 코드는 그와 다르다:**

| 항목 | redesign §7 계획 | 실제 현재 (검증: git log + 코드 grep) |
|------|-----------------|--------------------------------------|
| **A** (R6 채굴 확장) | 후속 PR, 세그먼터 소비 | **머지 완료(#94).** R6 클러스터링(`r6_cluster`, bigram Jaccard)으로 구현 — **세그먼터 미사용** |
| **C** (재설명 루프) | 후속 PR, 세그먼터 주 소비자 | **드롭됨** (#94 브랜치 `docs/coaching-c-drop`, 커밋 `drop coaching item C`) |
| 세그먼터 비테스트 소비자 | C·A·F | **`r24_context_hygiene.rs` 단 하나** (`rg` 검증) |

→ F를 은퇴시키면 세그먼터는 **소비자 0, 예정 소비자 0**의 고아 코드가 된다. Rust 특성상 F 룰 파일을
dead code로 남기려면 세그먼터도 남겨야 하므로(F가 `use crate::episode::…`), 선택은 "둘 다 보존" 아니면
"둘 다 제거"로 좁혀지고, 정당한 잔존 근거가 없어 **제거**를 택한다(§1).

## 4. 기각한 대안

| 대안 | 기각 사유 |
|------|-----------|
| **유휴 간격 경계 패치** (F 유지 + `first_ts` 차이로 경계 판정) | 위 사용자 오탐은 줄지만 "정확"해지지 않음(노이즈 프록시). "정확한 코칭 아니면 삭제" 기준 미달 |
| **auto-compact 재구성** ("반복 auto-compaction" 신호) | 정확·측정가능하나 1M 창에선 드물어 실질 가치 작음. F 수정이 아니라 별 영역의 신규 룰 |
| **C(LLM)에 연동** | C가 드롭됨. 설령 되살려도 F의 결정론 성격 상실 + 비용·지연, 정확해져도 코칭 가치 낮음(§3.1) |
| **세그먼터 보존(둘 다 보존)** | 소비자·예정 소비자 0 (§3.2). 정당한 잔존 근거 없음 → 단순성 원칙상 제거 |

## 5. 구현 — 완전 제거 목록

`rg`(binary-safe, no-glob)로 확정한 전 참조점. **Windows에서 `rg --glob`·Grep 도구 `glob`은 백슬래시
중첩 경로 일부를 조용히 누락하므로 glob 없이 검증했다.**

### 5.1 파일 삭제 (2)

- `crates/core/src/episode.rs` — 에피소드 세그먼터 + 단위 테스트
- `crates/core/src/rules/r24_context_hygiene.rs` — R24 룰 + 단위 테스트

### 5.2 파일 편집 (Rust)

| 파일 | 조치 |
|------|------|
| `crates/core/src/lib.rs` | `pub mod episode;` (line 7) 제거 |
| `crates/core/src/rules/mod.rs` | `pub mod r24_context_hygiene;` (line 13) 제거 |
| `crates/core/src/ops.rs` | ① `RuleEngine::new(vec![…])`에서 `Box::new(…R24ContextHygiene…)` (line 182) 제거 ② `run_rules` 앞부분에 purge 라인 추가 (아래) ③ 테스트 `run_rules_purges_retired_rule_findings`에 `R24|W|proj` 시드+단언 추가 ④ 테스트 `run_rules_registers_r24_and_surfaces_project_card_as_new`을 **비발화 검증으로 전환**(`run_rules_does_not_surface_r24_after_retirement`: 발화 조건 데이터여도 R24 카드 0 — 재등록 회귀 가드) |
| `crates/core/src/coach.rs` | `assert_eq!(fix_command("R24", …), None);` (line 60) 제거 |
| `crates/core/src/diary/mod.rs` | `finding_advice`의 `"R24" => { … }` arm (line 254~, **카드 문구 생성부**) 제거 + 테스트 `finding_advice_r24_context_hygiene_cites_worst_session` (line 2242~) 제거. **기본(catch-all) arm 존재 확인 후 제거** |

purge 라인 (R10·R11 블록 옆):

```rust
// 코칭 가치 재설계 후속(2026-07-23): F(R24 컨텍스트 위생) 은퇴 — 룰·세그먼터 완전 제거.
// 8자↑ 프롬프트=새 작업 경계가 후속질문을 작업전환으로 오인 → 결정론으로 정확도 확보 불가(스펙 참조).
store.delete_findings_by_rule_and_scope("R24", "project")?;
```

R24는 `scope_kind:"project"`로만 발화했으므로 이 한 줄로 기존 5장 전부 정리된다.
**불만1(반복)도 여기서 해소** — 카드가 발화되지 않아 프로젝트별 5장 문제가 소멸(별도 집계 로직 불필요).

### 5.3 파일 편집 (프론트엔드)

| 파일 | 조치 |
|------|------|
| `src/lib/ui/coach-helpers.ts` | `COACH_TITLE`의 `R24: '…'` (line 39) 제거 |
| `src/lib/ui/coach-helpers.test.ts` | `it('R24 컨텍스트 위생 카드 제목', …)` (line 51~) 제거 |

### 5.4 절대 건드리지 않을 경계 (오제거 방지)

**`Dimension::ContextHygiene`는 R24와 무관한 별개 개념**이다 — 코칭 프로필의 Lv1 차원("안 쓰는
MCP·플러그인 정리, CLAUDE.md")으로, 콘텐츠 팁(T-L1-\*) 큐레이션에 쓰인다.

- **유지**: `profile.rs`의 `Dimension::ContextHygiene` 전부, `content.rs`의 관련 팁, `TipCard.svelte`의
  `context_hygiene: '컨텍스트 정리'` 라벨. 문자열 `"context_hygiene"`이 R24 처방 kind와 우연히 겹치지만
  렌더 경로가 다르다(팁 차원 라벨 ↔ 룰 처방). R24 제거로 깨지지 않는다.

## 6. 후속 / 파급

- redesign §7의 **"F 임계값 캘리브레이션"** 후속 항목 **폐기**.
- redesign §4/§7의 **B↔F 시너지**(never-clear 세션을 F가 B로 리다이렉트) 소멸 — never-clear 사용자는
  **틀린 코칭 대신 침묵**을 받으며, 재설계의 fail-safe-침묵 원칙과 일치.
- **에피소드 세그먼터 개념 폐기** — 재설계 스펙이 상정한 "C·A·F 공유 척추"는 C 드롭·A 미사용으로
  현실에서 성립하지 않았다. 훗날 유사 기능이 필요하면 그때의 소비자에 맞춰 신설한다(git 이력으로 복구 가능).

## 7. 검증

- `cargo test`(워크스페이스) — 삭제된 R24/세그먼터 테스트가 사라지고, §5.2 갱신 테스트 통과, 컴파일·회귀 0.
- `npm test`(Vitest) — coach-helpers 테스트에서 R24 케이스 제거 후 통과.
- 다음 스캔에서 기존 R24 카드 5장 소멸(§5.2 purge).
- **완전 제거 확인**: `rg -a 'R24|segment_all|crate::episode' crates/ src/` (glob 미사용) 결과가
  문서/스펙 참조를 제외하고 0.
