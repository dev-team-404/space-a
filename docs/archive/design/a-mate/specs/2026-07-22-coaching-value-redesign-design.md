---
status: done
archived: 2026-08-03
---

# 코칭 룰 실질가치 재설계 — 설계 스펙

- **날짜**: 2026-07-22
- **상태**: 설계 확정 (브레인스토밍 → 구현 계획 전 단계)
- **컴포넌트**: a-mate (Agent Mentor)
- **관련**: [코칭 v3 설계](./2026-07-19-coaching-v3-design.md), [R6 v2 세션 반복 채굴](./2026-07-20-r6-v2-session-repeat-mining.md), PR #82 (R6 판정 레이어)

---

## 1. 배경 & 문제 정의

R6 판정 레이어(결정론 채굴 + LLM 판정 게이트, PR #82) 이후, 나머지 코칭 룰의 **실질 가치에 근본 의문**이 제기되었다. 현행 활성 룰 5개 중 **R6만 유일하게 "행동을 바꾸는" 코칭**(반복 지시 → 스킬화)이고, 나머지(R7 통계·R8·R10·R11)는 대부분 "비용 잔소리"라 실질 가치가 낮다.

**방향**: R6의 철학(결정론 채굴 + LLM 판정)을 **공유 인프라로 승격**해, 사용자에게 실질 가치 있는 코칭 아이템을 늘린다. 통계 룰을 **LLM 사후평가 코칭**으로 전환한다.

### 재설계 대상 아이템

| 코드 | 아이템 | 처분 |
|------|--------|------|
| **B** (R7 진화) | 모델 적정화 — LLM이 작업 무게를 판정 | 재구성 (통계 → LLM 판정) |
| **F** (신규) | 컨텍스트 위생 — clear/compact 없이 대용량 상주 코칭 | 신규 |
| ~~**C** (신규)~~ | 프롬프트 습관 — 재설명 루프 코칭 | **드롭** (2026-07-22, §4 C 참조) |
| **A** (R6 확장) | 워크플로 레버리지 — 반복 지시 스킬화 강화 | R6 채굴 강화 |
| **E** (신규) | 공식 마켓플레이스 plugin 추천 | 신규 (큐레이션 확장) |
| R8 | MCP 대형 결과 | 유지 (무변경) |
| ~~R10~~ | 자동화 버스트 | **제거** |
| ~~R11~~ | 권한 마찰 | **제거** |

---

## 2. 불변 계약 (모든 아이템이 상속)

1. **근거 = 사용자가 직접 친 프롬프트 전문 + 객관적 main-chain 사실**(턴 수·도구 종류·출력 토큰·모델·effort). `prompt_events`는 이미 sidechain·meta(슬래시커맨드)·도구결과·주입 리마인더를 제외한 "사람 발화"만 담는다.
2. **금지 = 에이전트/서브에이전트의 산출물 품질·모델 선택 채점.** 판정·집계는 `is_sidechain=0`만 본다. *"LLM이 한 작업을 근거로 삼는 코칭은 무의미하다"* (사용자 대전제).
3. **반복·occurrence를 세는 아이템(R6·A 등)은 세션 카운트를 발화 문턱으로 쓰지 않는다** — never-clear(한 세션 유지 + auto-compact) 사용자 실명 방지, 반복은 에피소드/occurrence로 센다. (세션이 본질 단위인 B는 예외이며, never-clear는 F가 담당한다 — §4 B·F 시너지.)
4. **Fail-safe 침묵**: 엔진 미설정이면 LLM 판정 아이템은 no-op(pending 잔류). 근거 없는 수치 금지(`est_tokens_saved=0` 관행).
5. **저빈도·쿨다운**: 같은 코칭 반복 금지. content 큐레이션의 쿨다운·dismissal 인프라 재사용.

---

## 3. 아키텍처 — 공유 인프라 2종

### ① 에피소드 세그먼터 (결정론) — `crates/core/src/episode.rs` (신규)

한 세션의 이벤트를 **에피소드** = `[실질 UserPrompt → 다음 실질 프롬프트 전까지의 작업]`로 자른다. clear/compact 습관에 불변인 작업 단위. **C·A·F가 소비, B는 안 씀.** LLM 불필요.

**알고리즘 (순수 결정론)**:
1. 세션 이벤트를 `(ts, source_offset)`로 정렬 (ts 우선, offset 타이브레이크 = 파일 내 바이트 순서).
2. **경계**: 각 *실질* UserPrompt(`r6::normalize(p) != None`, 즉 8자 이상)가 새 에피소드를 연다.
3. **접착 병합**: 명백한 접착 프롬프트는 새 에피소드를 열지 않고 앞 에피소드에 `glue_followups`로 부착. 접착 판정 = 8자 미만 **또는 작은 마커 집합**("ㅇㅋ", "네", "다시", "계속", "ㄱㄱ" 등).

**Episode 구조 (스케치)**:
```rust
struct Episode {
    session_id, host, project_id,
    lead_prompt: PromptRef,          // 실질 프롬프트 (deref 포인터 source_offset + preview)
    glue_followups: Vec<PromptRef>,  // 부착된 접착 프롬프트
    first_ts, last_ts,
    had_compaction: bool,            // 범위 내 Compaction 이벤트 존재 (F 재료)
}
// 작업 사실 집계(main-chain 필터 포함)는 helper: episode_facts(store, &ep)
```

**책임 경계**: 세그먼터는 **명백한 접착만** 결정론으로 병합한다. 장황한 교정("아니 그거 말고 X로")은 8자 이상이라 별도 에피소드가 되며, **이를 "교정"으로 식별하는 건 C(LLM)의 몫**이다. 세그먼터가 교정 검출을 흉내내지 않는다 → 단순·결정론 유지. LLM은 판정 계층(C)에 이미 있으므로 세그먼터에 넣는 것은 오버엔지니어링·중복.

**엣지 케이스**: ts 없는 프롬프트는 offset 정렬 / fork·resume 복제본은 PR1 논리 dedup이 이미 접음 / Compaction 이벤트는 에피소드를 열지 않고 `had_compaction` 마커로만 기록 / 실질 프롬프트 0개 세션은 건너뜀.

### ② 코칭 판정 패스 (결정론 채굴 → LLM 판정 → verdict 캐시 → 노출)

`src-tauri/pipeline.rs::maybe_judge_repeats`(R6 전용)를 **아이템 무관 드라이버 + 작은 트레이트**로 승격. R6을 첫 기존 소비자로 편입해 추상화를 검증한다. **B·C·A가 소비**(R6 포함).

**트레이트 (4 슬롯 — 아이템별)**:
```rust
trait CoachingJudge {
    fn id(&self) -> &'static str;                              // "R6", "R7" ...
    fn pending(&self, s: &SqliteStore, cap: usize)             // ⓐ 채굴: 판정 대기 후보
        -> Result<Vec<Candidate>>;
    fn context(&self, s: &SqliteStore, c: &Candidate)          // ⓑ 근거: 프롬프트 전문 deref + main-chain 사실
        -> Result<JudgeContext>;
    fn prompt(&self, ctx: &JudgeContext) -> (String, String);  // ⓒ 판정 프롬프트 (system, user)
    fn classify(&self, verdict_json: &Value) -> Actionability; // ⓒ verdict → {Confirmed, Rejected}
    fn rollup(&self, s: &SqliteStore) -> Result<Vec<String>>;  // ⓓ verdict → 노출 finding (신규 dedup_key들)
}
```

**공유 드라이버 (아이템 무관 — `maybe_judge_repeats` 락 규율 그대로)**:
```
run_coaching_judgments(app, store, judges):
  ① 엔진 해석 (짧은 락) — 없으면 no-op (fail-safe 침묵)
  ② 각 judge: pending(cap) + context() 수집 (짧은 락, SQL만)
       └ context 실패 = 판정 시도로 계산(attempts++, 3회면 rejected) — HoL 큐 막힘 방지
  ③ 락 밖: engine.generate(prompt) → verdict 파싱  (LLM 네트워크 I/O)
  ④ verdict 저장 (짧은 락): classify → 상태 전이 + attempts + tokens 캐시
  ⑤ 각 judge.rollup() (짧은 락) → 노출 finding 생성/갱신
  ⑥ 신규 노출 dedup_key 있으면 coach:finding + scan:done 재발행
```

**Verdict 생애주기 (R6에서 검증된 공유 규칙)**:

| 판정 결과 | 상태 전이 | 이유 |
|-----------|-----------|------|
| Confirmed (actionable) | `pending → confirmed` | 롤업 대상 |
| Rejected (not actionable) | `pending → rejected` (영구 캐시) | 재판정 안 함 |
| Malformed (응답 왔으나 파싱 실패) | `pending` 유지, attempts++ (3회면 rejected) | 인프라 실패 아님 |
| Transport fail (엔진 다운) | `pending` 유지, attempts **미증가** | 다음 스캔 재시도 |

같은 dedup_key는 `upsert ON CONFLICT`으로 status 보존, **pending만 재판정** (R6 계약 그대로).

**R6 편입 (추상화 검증)**: `Judgment{worthy,...}` → R6의 `classify`: `worthy→Confirmed / !worthy→Rejected`. R6은 **rollup = 항등**(confirmed 패턴 finding이 곧 노출). B는 rollup에서 세션 verdict를 프로젝트 카드로 집계 — 같은 트레이트, 다른 rollup.

**스토어 일반화**: `pending_r6_for_judgment(n)` → `pending_for_judgment(rule_id, n)` (R6 호출부는 `"R6"` 전달로 무변경). `set_judgment(dedup_key, status, judgment)` 이미 범용. verdict JSON은 아이템별 자유 스키마.

**등록**: `run_coaching_judgments(app, store, &[R6, R7])`가 파이프라인에서 `maybe_judge_repeats` 자리를 대체. C·A는 나중에 벡터에 judge만 추가.

---

## 4. 아이템 상세

### B — 모델 적정화 (R7 v3)

**정체성**: 신규 룰이 아니라 **R7 진화** (dedup 연속성 + 기존 `start_with_lighter_model` 처방 재사용). R7의 `evaluate()`가 *통계 발화*에서 **후보 채굴**로 바뀌고, 판정 패스가 정밀도를 담당.

**단위 = 세션** (에피소드 아님). 모델은 **실행 시점(세션 launch) 속성**이고 세션 중 상·하위 전환은 사실상 불가(컨텍스트 크기 차이로 더더욱). clear 쓰는 정상 사용자는 세션 ≈ 작업. never-clear 사용자는 "모델 문제"가 아니라 "컨텍스트 위생 문제(F)"로 리다이렉트 — B를 비틀지 않음(관심사 분리).

- **ⓐ 채굴 (결정론, 값싼 필터)**: 관찰창(14일) 내 **main-chain에서 Opus를 실제로 쓴 세션**(`opus_turns(is_sidechain=0) ≥ 1`), 배치 캡. 의도적으로 통계 "light" 프록시로 사전 필터하지 않음(그게 교체 대상). 제외: `sub_agent` 도구 쓴 세션(실질 작업, 그 모델은 사용자 것 아님) · 버스트 세션(`detect_bursts`) · main-chain 턴 0.
- **ⓑ 컨텍스트**: 세션의 실질 UserPrompt(들) 전문 deref + `is_sidechain=0`만 집계한 assistant_turns·tok_output·tool_calls+종류·모델·소요시간 + host effort_level(서사). subagent 활동·에이전트 산출물 절대 미포함. ⚠ **`collect_session_stats`에 `WHERE is_sidechain=0` 추가** (현행 subagent 혼입 결함 수정).
- **ⓒ 판정**: `{ over_modeled: bool, reason: string(한 문장), suggested_model: "sonnet" }`. system: "작업의 **무게**만 보라 — 에이전트가 잘했는지·subagent 모델은 판단 대상 아님. 확신 없으면 over_modeled=false" (정밀도 우선).
- **ⓓ 롤업 & 처방**: 세션별 verdict는 캐시(재판정 방지), 카드는 세션별로 안 띄움. **한 `(host, project)`에서 over_modeled ≥3 세션 & 판정된 것 중 과반이면 프로젝트 카드 1장** — 실제 세션 요청 한 줄들을 근거로 인용. 처방 `start_with_lighter_model {to: "sonnet"}`, dedup `R7|{host}|{project}`. 닫으면 그 프로젝트 카드 억제.
- **스코프 경계**: v1은 모델(Opus→Sonnet) 판정 집중. effort는 host 기본값이라 서사 맥락으로만(per-세션 effort 데이터 부재 → 독립 판정은 후속). 타깃은 sonnet 고정(Haiku 세분화 후속).

### F — 컨텍스트 위생 (신규 결정론 룰)

**성격**: 순수 결정론 룰(R8형) — LLM 판정 패스 안 씀. 에피소드 세그먼터 소비. `crates/core/src/rules/` 신규.

**대상 행동**: clear/compact 없이 대용량 컨텍스트를 계속 상주시켜 작업 전환 후에도 이전 컨텍스트를 이고 감 → 매 턴 큰 입력 비용 재지불 + 응답 품질 저하(주의 희석).

**핵심 신호 (절대 임계보다 강건)**: 절대 토큰 임계는 컨텍스트 창(200k vs 1m)마다 애매 → 대신 **"작업 경계에서 컨텍스트가 리셋되는가"**를 직접 본다.
> 각 에피소드의 **첫 main-chain 턴이 물려받은 컨텍스트** = `tok_input + tok_cache_read`.
> `/clear` 사용자 → 새 작업이 거의 0에서 시작 / 안 끊는 사용자 → 큰 컨텍스트 물려받고 시작.

**F 발화 신호 = 한 세션에서 "큰 컨텍스트를 물려받고 시작하는 에피소드"의 비율이 높음.** 보조 신호: 세션당 에피소드 다수, `had_compaction`.

| 파라미터 | 기본값(잠정 — 실데이터 핀) | 의미 |
|----------|--------------------------|------|
| `min_episodes` | ≥5 | 습관 성립에 필요한 작업 수 |
| `inherited_ctx_threshold` | ~50k 토큰 | "큰 컨텍스트 물려받음" 기준 |
| `carry_ratio` | ≥60% | 그런 에피소드 비율 |

→ `(host, project)` 롤업 카드 1장, 가장 심한 세션 근거 인용. 임계값은 R7·R8처럼 **Windows 실데이터로 핀**.

**처방**: kind `context_hygiene`. **품질 우선 프레이밍**: *"작업 전환 시 `/clear`로 컨텍스트를 끊으면 토큰뿐 아니라 응답 품질에 이득."* auto-compact를 비난하지 않음(제안형). `est_tokens_saved=0`. severity Suggest. 쿨다운.

**B와의 시너지**: never-clear 세션은 B 후보에서 제외(무겁게 보임)되지만 F가 잡음 → "작업 경계에서 끊어라"로 유도 → 끊으면 B의 세션 단위가 자연히 정상 작동.

### C — 프롬프트 습관 코칭 (신규) — ⛔ 드롭 (2026-07-22)

> **처분: 구현하지 않음.** B(#92)·F(#93) 착지 후 C 착수 직전 재검토에서 드롭 결정.
> **사유 ①(증거 경계 오염)**: 재설명 루프의 실제 원인은 대개 사용자 발주가 아니라 **에이전트 오해**인데,
> 허용된 근거(사용자 프롬프트 전문 + main-chain 사실)만으로 "발주 부실" vs "에이전트 오해"를 구분할 수 없다.
> 즉 §2 불변 계약(*에이전트 산출물 품질을 근거로 삼는 코칭 금지*)을 사실상 위반하고 사용자에게 책임을 전가한다.
> **사유 ②(처방이 잔소리)**: "발주 3줄 스펙"은 §1이 잘라내려던 일반론적 잔소리에 가장 근접 — F(끊어라)·A(스킬화) 같은
> 구체적·사용자 통제 레버가 아니다.
> **의존성**: C에 의존하는 항목 없음(A·E는 F·B에만 의존, 코드 참조 0건) → 드롭이 후속 아이템을 막지 않음.
> **아래 원안은 이력 보존용.**

**재사용**: 에피소드 세그먼터 + LLM 판정 패스.

- **채굴(결정론)**: "재설명 루프" 후보 — 한 작업(에피소드 + glue_followups + 인접·짧은간격 후속 에피소드)에 프롬프트가 여러 번 몰린 경우.
- **판정(LLM)**: `{ is_reexplain_loop: bool, reason, habit_tip }`. **핵심 정밀도**: 반복 정제(iteration)는 정상이고 좋은 것 — "그만 고쳐라"는 잘못된 조언. 코칭은 좁게 *"초기 발주가 자주 부실해 피할 수 있는 왕복이 생긴다"*로 한정. 확신 없으면 finding 아님(통계로 못 하고 LLM이어야 하는 이유).
- **롤업**: 세션 간 집계 → *"최근 N개 작업이 3+회 교정 필요 → 초반 3줄 스펙"*. 근거 = 사용자 프롬프트 전문.
- **스코프**: v1 = 재설명 루프 집중. "한 줄 발주 습관"·"세션 간 같은 질문 반복"은 후속 확장(YAGNI).

### A — R6 확장 (워크플로 레버리지)

**재사용**: 에피소드 세그먼터 + LLM 판정 패스(R6은 이미 소비자). 신규 룰 아님 — R6 **채굴 강화**.

| 한계 | 현행 | A 확장 |
|------|------|--------|
| **never-clear 사각** | `COUNT(DISTINCT session_id)≥3` → 한 세션에 5번 반복해도 세션=1 → 침묵 | **occurrence/에피소드 카운트로 발화** (R6이 이미 추적하는 `occurrences_total` 승격) |
| **완전일치만** | 정규화 문자열 동치 | **느슨한 클러스터링**(토큰 중첩/유사도)로 채굴 확대 → 의미 유사 반복도 후보. 정밀도는 **기존 LLM 판정이 게이트** |

판정 verdict·rollup은 R6 기존(worthy→스킬 초안) 재사용. 확장은 미더 채굴자뿐.

### E — 공식 마켓플레이스 plugin 추천 (신규)

**설계 위치**: 판정 패스(finding+rollup)가 아니라 **기존 콘텐츠 큐레이션 인프라에 얹음** — 큐레이션이 이미 "관련 팁을 근거와 함께, 쿨다운·dismissal로 안 나대게" 노출을 해결. 새로운 건 LLM 매칭뿐.

**은퇴한 R2(미사용 plugin)·R12(미사용 skill) 의도를 되살리되, 실패 원인(작업 관련성 없는 잔소리)을 LLM work-kind 매칭으로 해결.**

**두 케이스 = 인벤토리 상태 매핑**: LLM이 세션 작업 성격 판정("프론트엔드 작업") 후,

| 인벤토리 상태 | 추천 |
|--------------|------|
| 설치+enabled인데 그 작업에 미사용 | "다음에 이런 작업엔 `frontend-design` 써봐라" (①) |
| 미설치 | "이런 작업엔 유용하니 깔아서 써봐라" (②) |
| 이미 사용 중 | 침묵 |

- **카탈로그(마켓플레이스)는 주로 ②(미설치)용** — 로컬에 없는 plugin의 존재·용도를 알아야 하므로. ①은 로컬 인벤토리 + `plugin_purpose`로 충분.
- **재사용/부활**: `curation.rs`의 `SkillRecommendationSource`·`WorkPattern`·`plugin_purpose`(이미 `frontend-design` 등 등록) 스캐폴드를 일반화(결정론 1개 패턴 `LargeImplNoSkill` → LLM 파생 work-kind) + 인벤토리(설치/enabled/사용) + 마켓플레이스 카탈로그 소스(②용, `ClaudeChangelogSource` 형제로 `fetch_feed_items`에 편입).
- **프라이버시·네트워크**: 카탈로그 = 공개 read-only(사용자 데이터 미전송, changelog 동급, 엣지 fetch, 실패 관대). 매칭 = 온프레 Engine. 근거 = 프롬프트, 이미 쓰는 건 침묵.

**정직한 미확정**: 공식 마켓플레이스 카탈로그의 정확한 소스 URL·스키마는 구현 착수 시 실제 마켓플레이스를 확인해 핀(파싱 실패=관대 폴백). 사외망에서 공개 URL 도달 가정. 캐시 TTL은 changelog 관행 참조.

---

## 5. R10 / R11 제거

**은퇴 관행(기존 R1/R2/R5/R9/R12와 동일)**: RuleEngine 등록 해제 + `delete_findings_by_rule_and_scope`로 findings purge + **룰 코드·테스트 보존**.

- **R10 (자동화 버스트)**: 관찰 전용 카드(Info, 처방 없음), 실질 가치 없음. 등록 해제 + `delete_findings_by_rule_and_scope("R10", "project")`. ⚠ `detect_bursts`는 `session_stats.rs`에 있고 B가 버스트 제외에 계속 쓰므로 **유지**.
- **R11 (권한 마찰)**: 거부→승인 마찰 감지(근거는 사용자 행동이라 제약은 통과하나) 가치가 행동으로 이어지기 애매하고 재설계 취지(잔소리 컷)와 맞음. 등록 해제 + `delete_findings_by_rule_and_scope("R11", "project")` + 코드·테스트 보존.

**재설계 후 `run_rules` 등록 상태**: R6(판정 채굴자), R7=B(판정 채굴자), R8(결정론 무변경), F(결정론 신규). C·A(판정, 후속 PR), E(큐레이션+LLM, 후속 PR). ~~R10~~·~~R11~~ 은퇴(코드 보존).

---

## 6. 에러 처리 & 테스트 전략

### Fail-safe (기존 패턴 계승)
- 엔진 없음 → LLM 아이템(B·C·A·E매칭) no-op, pending 잔류. 결정론(F·R8)은 계속.
- 전송 실패 → skip, attempts 미증가, 다음 스캔 재시도. Malformed → attempts++, 3회면 rejected.
- verdict 캐시 → 같은 dedup_key 재판정 안 함. 카탈로그 fetch 실패(E) → 추천 없음(에러 아님).
- 락 규율 → resolve/gather/persist 짧은 락, LLM·네트워크 I/O는 락 밖.
- 프라이버시 → 사용자 프롬프트는 설정 Engine(온프레)에만, 카탈로그는 공개 read-only. `est_tokens_saved=0` 관행.

### 테스트 (cargo test, in-memory store — 네이티브 Windows)
- **세그먼터**: 경계·접착병합(마커집합 포함)·정렬·compaction 마커·sidechain 제외.
- **공유 판정 패스**: MockEngine — verdict 4상태 생애주기·attempts·캐시·rollup. **R6 기존 테스트 회귀 통과**(편입 검증).
- **B**: 채굴(Opus main-chain, sub_agent·버스트·무턴 제외)·**main-chain 필터 신규 테스트**·rollup 문턱(≥3&과반)→프로젝트 카드+예시·문턱 미달 침묵·엔진 없음 no-op.
- **F**: carry-ratio 발화·정상 clear 침묵·main-chain 토큰 필터·임계.
- **C/A**: C 정밀도(재설명 루프 vs 정상 정제)·**A never-clear 케이스(한 세션 반복도 발화)**·느슨한 클러스터+판정 게이트.
- **E**: 카탈로그 fetch(mock·관대 실패)·설치/enabled/사용 상태→①②·이미 사용 침묵·work-kind 매칭.
- **R10/R11 제거**: purge 테스트·`detect_bursts` 생존.
- **정밀도**: 확신 없으면 finding 아님(judge.rs 테스트 미러).

---

## 7. 구현 순서 (PR 분할)

에피소드 세그먼터가 핵심 공유 척추 (C·A·F 소비 / B·E·R8은 안 씀).

1. **B** — 공유 LLM 판정 패스 v1 추출 + R6 편입(검증) + `collect_session_stats` main-chain 필터 + R7 채굴/판정/롤업. **R10·R11 제거 곁다리.** 증거 경계 계약 확정.
2. **F** — 에피소드 세그먼터 구현 + 컨텍스트 위생 결정론 룰. (세그먼터가 여기서 처음 필요.)
3. ~~**C** — 재설명 루프 채굴 + 판정~~ → **드롭** (2026-07-22, §4 C 참조). 이후 순서는 F→**A**→E.
4. **A** — R6 채굴 강화 (occurrence 카운트 + 느슨한 클러스터). ← **다음 착수** (F·B 선행 완료, C 무의존).
5. **E** — 마켓플레이스 카탈로그 소스 + LLM work-kind 매칭 + 큐레이션 스캐폴드 일반화.

각 아이템 구현 PR은 별도 writing-plans에서 상세 계획.

---

## 8. 미확정 / 후속

- **임계값 핀**: F의 `inherited_ctx_threshold`·`carry_ratio`·`min_episodes`, B의 롤업 문턱 → Windows 실데이터로 캘리브레이션.
- **E 카탈로그 소스**: 공식 마켓플레이스 URL·스키마 구현 시 확인.
- **신규 룰 ID 배정**: F의 룰 ID는 예약된 R13(환경 큐레이션)·R16·R19·R20과 충돌 없이 배정 (구현 시 확정).
- **B effort 독립 판정**·**B Haiku 세분화**·**C 추가 습관 신호(한 줄 발주·세션 간 반복)** → 후속.
