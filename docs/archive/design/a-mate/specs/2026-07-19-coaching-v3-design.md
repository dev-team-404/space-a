---
status: done
archived: 2026-08-03
---

# 코칭 v3 설계 스펙 — 사용자 레버 카탈로그 재편 (추천 · 가이드 준수 · 실시간 넛지)

- 작성: 2026-07-19 (브레인스토밍 산출물, 킥오프: 사용자 E2E 피드백 2026-07-18)
- 선행: 코칭 v2(`2026-07-06-coaching-v2-design.md`), v2.1(`2026-07-07-coaching-v2.1-design.md`)
- 다음 단계: writing-plans → `docs/design/a-mate/plans/` → SDD → PR(§15 분할) → 사용자 E2E

## 1. 배경과 원칙

v2.1 완성 후 실사용에서 코치 탭 내용이 사실상 3종(R1 "MCP 끄세요" 반복, R2 "스킬 낭비",
R10 "자동화 모델 바꾸세요")으로 수렴했고, 사용자가 같은 잣대의 신뢰 문제를 제기했다 (2026-07-18):

1. **R1**: 탐지는 적절하나 "안 쓰니 끄세요"의 반복은 무익 — 지금 하는 작업 기준으로
   유용한 스킬/MCP를 **추천**하는 코칭이 돼야 한다.
2. **R2**: 스킬 상주분은 name+description뿐(플러그인당 수백 토큰, `inventory.rs` chars/4 실측)이라
   "낭비" 프레임의 근거가 부족하다.
3. **R10**: 문제 자동화는 대부분 **사용자가 설정하지 않은 도구**가 만든다 — 그 수정을
   사용자에게 지시하는 것은 실질 코칭이 아니다.
4. **총칙**: 모든 코칭은 **사용자 기인 행동**의 교정만 가이드한다. 작업 중 에이전트가 한
   행동에 대한 지적은 무의미하다.

**v3 원칙** (v2 원칙 1~4 계승 + 추가, 합의됨)

5. **사용자 레버만**: 조치 지점이 사용자 소유(설정 파일 · 습관/다음 세션의 선택 · 환경 구성 ·
   프로젝트 문서)인 것만 코칭한다. 에이전트 행동 관찰은 레버가 사용자 것일 때만 서사에 쓴다.
6. **잔소리 → 추천**: 제거/정리 권고는 단독 카드가 아니라 작업 패턴 기반 추천의 맥락 안에서
   한 번만 말한다. 같은 내용의 재노출은 항목 집합이 변할 때만.
7. **가이드 인용**: 공식 가이드(Anthropic Claude Code Best Practices 등)에 근거가 있는 코칭은
   `guide_ref`로 출처를 카드에 표기한다 — "제 생각"이 아니라 "가이드 기준" 안내.
8. **실시간 넛지는 별도 표면**: 진행 중 세션에만 유효한 안내(핸드오프 등)는 회고형 finding과
   분리된 휘발성 채널로 전달하고, 세션 종료 후 카드로 남기지 않는다(v2 원칙 2 유지).
9. v3 신규 룰의 `est_tokens_saved`는 전부 **0**(가치 제안/위생형 — 근거 없는 절약 수치 금지).
   비용-등가 추정은 유지되는 R7만 계속 사용한다.

## 2. 범위

| 구분 | 내용 |
|---|---|
| §3 처분 | 제거 R1·R2·R9·R12 / Info 강등 R10 / 유지 R7(확장)·R11 / 보류 유지 R5 |
| §4 수집 | 어댑터 갭 5건(실측 검증) + 신규 스캔 4건 + 마이그레이션 |
| §5 R13 | 작업 패턴 기반 스킬/MCP 추천 — 환경 큐레이션 카드 (R1·R12·R9 신호 흡수) |
| §6 R14 | CLAUDE.md 가족: a 부재 / b 비대 / c 서브디렉토리 분리 |
| §7 R15 | 반복 보일러플레이트 첫 프롬프트 → 스킬/CLAUDE.md 저장 |
| §8 R16 | 대형 구현 전 계획 부재 → plan 모드/플랜 스킬 |
| §9 R17 | 실시간 핸드오프 넛지 (신규 표면) |
| §10 R18 | superpowers 과잉 프로세스 (풀 프로세스 소규모 결과 · SDD-후-linear) |
| §11 | 가이드 준수 계열: R19 권한 우회 / R20 시크릿 / R21 검증 부재 / R22 반복 실패 메모 |
| §12 | 표면 변경 (guide_ref 렌더, CoachTab, coach.rs, finding_advice) |

**제외 (검토 후 기각/보류, 결정 로그 §14)**

- `/clear` 미사용·한 세션 멀티태스크: 이질 작업 판별이 의미론 — R17이 안전한 부분집합 커버.
- 캐시 TTL 리듬(자리비움 후 재개 비용): 판정 가능하나 과잉 개입 — 기각.
- 커밋 체크포인트 부재: 프로젝트별 편차 커 오탐 — R21에 부분 흡수.
- 서브에이전트 미활용(v2 킥오프 3): "직렬이 적절한 경우 많음"(사용자 지적) — Agent 매핑
  수정(§4.1) 후 데이터 보고 재평가.
- R5 재승격: 2026-07-10 판정 존중. cross-session 취지는 R14가 에이전트 비난 없이 대체.

## 3. 기존 룰 처분

### 3.1 제거 — R1, R2, R9, R12

- `ops.rs` 등록 해제 + 스캔 시 `delete_findings_by_rule_and_scope` 일괄 삭제(v2 §3 이행 전례):
  R1(host·project), R2(host), R9(session), R12(project).
- 룰 코드·테스트는 보존(R5 전례) — R1·R12·R9의 탐지 신호는 R13 분류기가 재사용한다(§5).
- 삭제 근거: R2 §1-2. R9는 웹 호출 횟수가 에이전트 행동이라 v3 원칙 5 위반 —
  신호(웹 조회 과다)는 R13의 `WebDocsHeavy` 패턴으로 전환. R1·R12는 R13에 흡수.

### 3.2 Info 강등 — R10 (관찰 카드)

- 탐지(`detect_bursts` + 과반 Opus)는 무변경. severity `Warn → Info`,
  **prescription 제거(None)**, `est_tokens_saved = 0`, fix_command None(현행).
- advice 교체(관찰만, 지시 없음): "이 프로젝트에서 자동화로 보이는 초단기 세션 N건이 Opus로
  돌았어요 (경로 `{rep_cwd}`). 직접 만든 자동화라면 모델 설정을 낮출 수 있고, 아니라면
  참고만 하세요." — 수정 지시가 아니라 소유 여부를 사용자가 판단하는 정보 제공.
- 기존 R10 finding은 dedup_key 불변이므로 upsert로 severity·evidence가 자연 갱신된다.

### 3.3 유지 — R7(확장), R11

- **R11 무변경** (결정론 거부→승인 + allowlist 레버 — v3 기준 모범 사례).
  이번 mac 실측으로 거부 마커("The user doesn't want to proceed with this tool use.") 동작 재확인.
- **R7 확장**: 판정·집계 무변경. `host_settings`(§4.2)의 기본 모델·effort를 evidence에 추가하고
  advice에 반영: 기본값이 최상위(예: `claude-fable-5[1m]` + effort xhigh)인데 잔심부름 비율이
  높으면 "다음엔 `claude --model sonnet`으로 시작하거나 기본 모델·effort를 낮춰보세요".
  guide_ref: 모델/사고 예산을 작업 난이도에 맞추라는 공식 가이드.

### 3.4 보류 유지 — R5

현행 그대로(등록 해제 + 카드 삭제 유지). 재승격 없음.

## 4. 수집 확장 (선행 과제)

### 4.1 어댑터 갭 수정 (이 mac 실데이터로 검증됨)

| # | 갭 | 수정 | 실측 근거 |
|---|---|---|---|
| 1 | `Agent` 툴명 미매핑 | `model.rs::from_raw_name`에 `"Agent" => SubAgent` 추가 | mac 23세션에서 Agent 84회 전부 `Other` 분류 → `heavy_tools` 왜곡(R7 오판 위험) |
| 2 | 첫 프롬프트 오염 | `adapter.rs::extract_prompt_preview`가 `<command`만 스킵 → `<local-command-stdout>`도 스킵 | mac 다수 세션 first_prompt가 "Set model to …" |
| 3 | 서브에이전트 파일 미인지 | `<세션id>/subagents/agent-*.jsonl` 존재 수 → `sessions.subagent_files` (전문 파싱은 후속) | isSidechain은 이제 0, 별도 파일로 분리됨 (87개 관측) |
| 4 | permission-mode 라인 미수집 | `type=="permission-mode"` 라인 → 세션 집계 컬럼 `sessions.plan_mode_hits`, `sessions.bypass_mode_hits` | mac에서 `plan` 5회·`auto`·`default` 관측 |
| 5 | compact 경계 마커 미수집 | auto/manual compact 경계 라인 → `Compaction` 이벤트에 trigger 구분(가능하면) | **mac에선 0건 — 마커는 Windows 실데이터로 핀** (v2.1 거부 마커 전례). 미인식 → 미수집 = 관련 룰 침묵(fail-safe) |

### 4.2 신규 스캔·플래그

- **개인 스킬 인벤토리**: `~/.claude/skills/*/SKILL.md` + 프로젝트 `.claude/skills/*/SKILL.md` →
  신규 테이블 `personal_skill_inventory(host, name, path, body_chars, scope: user|project)`.
  기존 플러그인 스킬 스캔(`scan_skills_dir`) 재사용.
- **호스트 설정 스냅숏**: `settings.json`의 `model`, `effortLevel` →
  신규 테이블 `host_settings(host, default_model, effort_level, scanned_at)` (R7 확장·R13 `OutdatedModel`용).
- **시크릿 플래그 (R20용)**: ingest 중 사용자 프롬프트 텍스트·Bash `command` 인자에 큐레이션
  정규식(§11.2) 매칭 → `events` kind `secret_flag`에 **pattern_id + 포인터만** 저장.
  **매칭된 본문은 절대 DB에 저장하지 않는다**(v2.1 §3 프로즈 미저장 원칙의 강화 적용).
- **CLAUDE.md 검사(R14)는 수집이 아니라 룰 평가 시 로컬 파일시스템 조회**:
  `sessions.cwd`가 로컬에 존재하는 프로젝트만 평가(다른 host의 경로는 자연 스킵 — 오탐 없음).

### 4.3 마켓플레이스 카탈로그 (R13용)

v1은 **앱 동봉 오프라인 스냅숏 JSON**(`crates/core/assets/catalog.json`) — 결정론·프라이버시 유지.
원격 인덱스 갱신(등록된 마켓플레이스의 marketplace.json fetch)은 후속 확장으로,
기존 `SkillRecommendationSource` trait 뒤에 어댑터로 붙인다(v2 §4.5 시드의 실현).

```json
{
  "version": "2026-07-19",
  "sources": ["claude-plugins-official", "anthropics/skills"],
  "items": [
    { "id": "context7", "kind": "plugin", "tags": ["web-docs"],
      "display": "context7", "purpose_ko": "라이브러리 최신 문서 조회",
      "install": "claude plugin install context7@claude-plugins-official" }
  ]
}
```

카탈로그 등록 소스: Anthropic 공식 마켓플레이스(`claude-plugins-official`) + 공식
`anthropics/skills` 리포. 커뮤니티 허브는 사용자가 후속 등록 가능(원격 어댑터와 함께).

### 4.4 마이그레이션

v2.1 전례(§4.4) 확장: 신규 컬럼(`sessions.subagent_files` 등) 부재 트리거로
events/sessions/ingest_state/daily_rollup 재수집. findings·status·diary_index 보존.

## 5. R13 — 작업 패턴 기반 환경 큐레이션 (신규, R1·R12·R9 흡수) ★핵심

"요즘 이런 작업을 주로 하시니 이건 어때요?" — 추천·활용·정리 3섹션의 host 단위 카드 1장.

**작업 패턴 분류기 (전부 결정론, 최근 30일 events 집계)**

| 패턴 | 판정 | 산출 |
|---|---|---|
| `WebDocsHeavy` | web_search+web_fetch 합 ≥ 30 (구 R9 신호) | 문서 조회 계열 추천 (예: context7) |
| `FrontendWork` | `.tsx/.css/.svelte/.html` 편집 ≥ 20 | UI 계열 추천 (예: frontend-design) |
| `GitHubHeavy` | Bash `command`가 `git `로 시작 ≥ 20 및 `gh `로 시작 = 0 | gh CLI 안내 (Best Practices 명시 항목) |
| `OutdatedModel` | `host_settings.default_model`이 큐레이션 최신 테이블 밖 | 최신 권장 모델 안내 |
| `UnusedMcp` (구 R1) | 인벤토리 서버 호출 0 + 상주 임계(R1 로직 재사용) | "빼도 좋아요" 정리 항목 |
| `InstalledSkillFit` (구 R12) | 대형 구현 세션 ≥2 + 설치 스킬 미사용(R12 로직 재사용) | "이럴 때 쓰세요" 활용 항목 |
| `PersonalSkillHygiene` | 개인 스킬 30일 호출 0 **그리고** body_chars < 500 | "지워도 잃을 게 없어요" 정리 항목 (판단은 사용자에게) |

- 추천 항목은 카탈로그(§4.3)에서 tags 매칭, **이미 설치된 것은 추천에서 제외**(활용 섹션으로).
- evidence: `{ patterns: {...근거 수치}, recommend: [...], activate: [...], cleanup: [...] }`
- severity `Info` · est 0 · prescription `{ kind: "curate_environment", payload: {recommend, activate, cleanup} }`
- fix_command: None (설치 명령은 항목별 `install` 문자열을 카드에서 복사 제공)
- **재노출 억제(원칙 6)**: `dedup_key = R13|{host}|{rev}` — rev는 정렬된 항목 id 집합의 짧은 해시.
  항목 집합이 변할 때만 새 카드가 생기고, 스캔 시 이전 rev의 R13 finding은 삭제한다.
  dismissed 상태는 dedup_key 단위로 이미 영구 보존되므로 같은 내용의 재알림이 없다.
- advice 방향: "요즘 {패턴 요약} 작업이 많네요. {추천}을 써보세요. {활용}은 설치돼 있으니 이럴 때
  유용해요. 반면 {정리}는 호출 0회라 빼도 좋아요" — 제거 권고가 추천 맥락 속 한 문장으로만.

## 6. R14 — CLAUDE.md 가족 (신규, subtype 3종)

프로젝트 메모리 위생. `sessions.cwd`가 로컬에 존재하는 프로젝트만 평가(§4.2).
공통: severity `Suggest` · est 0 · fix_command None · `dedup_key = R14|{host}|{project}|{subtype}`.

| subtype | 판별 (결정론) | 처방 (prescription kind) |
|---|---|---|
| `missing` | 프로젝트 세션 ≥ 5 그리고 cwd에 CLAUDE.md 없음 | `init_claude_md` — "/init 한 번이면 매 세션 구조 재탐색이 사라져요" |
| `oversized` | CLAUDE.md 존재, chars ≥ 8000 | `trim_claude_md` — 다듬기. claude-md-management 스킬 설치 시 연계 안내(R13 활용 섹션과 교차) |
| `split` | 루트 CLAUDE.md 존재 + 편집이 몰리는 상위 디렉터리 ≥ 2곳(각 file_edits ≥ 20) + 그 디렉터리들에 CLAUDE.md 없음 | `split_claude_md` — "영역 지침은 해당 폴더 CLAUDE.md로, 루트는 슬림하게" |

- evidence: `{ subtype, cwd, session_count | claude_md_chars | dirs: [{path, edits}] }`
- guide_ref: CLAUDE.md/메모리 공식 문서.
- 실측: 이 mac 7개 프로젝트 중 3곳 CLAUDE.md 부재(그중 1곳은 718턴 최대 세션 프로젝트).

## 7. R15 — 반복 보일러플레이트 첫 프롬프트 (신규)

- **판별**: `sessions.first_prompt_preview` 정규화(공백 축약) 접두 80자가 동일한 세션이
  host 전체에서 ≥ 3건 (프로젝트 경계 무관 — 실측: 같은 장문 프롬프트가 두 프로젝트에서 반복).
  §4.1-2(`<local-command-stdout>` 스킵)가 선행돼야 오탐이 없다.
- evidence: `{ prompt_preview, total_sessions, session_ids, projects }`
- severity `Suggest` · est 0 · prescription `{ kind: "save_as_skill_or_claude_md" }` · fix_command None
- `dedup_key = R15|{host}|{prompt_hash}` (prompt_hash = 정규화 접두 80자 해시)
- advice: "같은 지시문으로 시작한 세션이 N건이에요. CLAUDE.md나 커스텀 스킬(슬래시 커맨드)로
  저장하면 타이핑도 줄고 매번 같은 품질의 지시가 돼요."

## 8. R16 — 대형 구현 전 계획 부재 (신규, 가치 제안형)

- **판별**: 프로젝트에서 대형 구현 세션(file_edits ≥ 10) 중 `plan_mode_hits == 0` **그리고**
  플랜계 Skill(brainstorming/writing-plans 계열 큐레이션 목록) 호출 0인 세션 ≥ 3건.
- evidence: `{ session_ids, total_sessions, planless_ratio_pct }`
- severity `Info` · est 0 · prescription `{ kind: "use_plan_mode" }` · fix_command None
- `dedup_key = R16|{host}|{project}`
- advice: "큰 구현을 계획 없이 바로 시작한 세션이 N건이에요. plan 모드(Shift+Tab)로 계획을 먼저
  승인하면 재작업이 줄어요. {플랜 스킬 미설치 시: R13 추천과 교차 안내}"
- guide_ref: Best Practices의 plan 활용 항목. R18(과잉 프로세스)과 반대 방향의 짝 — 둘 다
  "작업 크기에 맞는 프로세스"라는 같은 원칙의 양끝이다.

## 9. R17 — 실시간 핸드오프 넛지 (신규 표면)

auto-compact에만 의존하는 사용자에게, **진행 중 세션**이 컨텍스트 한계에 접근하면
"핸드오프 프롬프트를 받아 새 세션에서 이어가는 건 어때요?"를 그 순간에 전달한다.

**finding이 아니다** — findings 테이블에 넣지 않고 카드로 남기지 않는다(원칙 8).

- **활성 세션 판정**: jsonl mtime ≤ 5분 (파이프라인 30초 폴링 실측 — 지연 ≤ ~30초).
- **점유율**: 마지막 assistant 턴의 `input + cache_read + cache_creation` 합.
- **윈도우 추론(보수)**: 세션 관측 최대 점유율 > 200k면 1M 윈도우, 아니면 200k로 간주.
  (`message.model`에 `[1m]` 표기가 없음을 실측 확인 — 관측 기반 추론이 유일한 결정론.)
- **발화**: 점유율 ≥ 임계(200k 윈도우 150k / 1M 윈도우 700k) 도달 시 **세션당 1회**.
  신규 테이블 `nudge_log(session_id, kind, ts)`로 중복 방지. 세션 종료(비활성) 시 소멸.
- **채널**: 기존 notices + 마스코트 말풍선. 문구: "지금 세션 컨텍스트가 약 {pct}% 찼어요 —
  핸드오프 프롬프트를 요청하고 새 세션에서 이어가면 흐름을 잃지 않아요."
- **회고 서브타입(옵션, compact 마커 핀 후)**: auto-compact 경계 ≥ N회 + 수동 `/compact` 0회인
  host에 "auto-compact에만 의존하고 계세요" Info 카드 1장. 마커 미핀 시 전체 침묵(fail-safe).

## 10. R18 — superpowers 과잉 프로세스 (신규, subtype 2종)

"작업 크기에 맞지 않는 풀 프로세스"를 사용자 레버(CLAUDE.md 규칙·프롬프트 습관)로 교정.
공통: severity `Info` · est 0 · fix_command None · `dedup_key = R18|{host}|{project}|{subtype}`.

| subtype | 판별 (결정론) | 비고 |
|---|---|---|
| `full_process_small_change` | 한 세션에서 brainstorming·writing-plans 계열 Skill 모두 호출 **그리고** 소스 편집 파일 수 1~3 (docs/superpowers/** · docs/design/** 제외) — 이런 세션 ≥ 3건/프로젝트 | **편집 0건 세션은 제외** — "만들지 않기로 결정"은 프로세스가 가치를 낸 경우 |
| `sdd_then_linear` | subagent-driven-development Skill 호출 후 같은 세션 `Agent` 호출 0 — ≥ 2건/프로젝트 | §4.1-1(Agent 매핑) 선행 필수 |

- prescription `{ kind: "process_rightsizing" }` — advice에 CLAUDE.md 규칙 예문 제공:
  "작은 작업까지 풀 프로세스를 거친 세션이 N건이에요. CLAUDE.md에 '단일 파일 수정은
  브레인스토밍/플랜 생략' 같은 규칙을 넣거나, 시작할 때 '프로세스 생략하고 바로'라고 말해보세요."

## 11. 가이드 준수 계열 — R19 / R20 / R21 / R22

공통: guide_ref 필수(§12.1), fix_command None.

### 11.1 R19 — 권한 우회 모드 상시 사용 (Warn)

- **판별**: host에서 `bypass_mode_hits ≥ 1`인 세션 ≥ 5건 그리고 그 비율 ≥ 50%.
- evidence: `{ bypass_sessions, total_sessions, ratio_pct }` · est 0
- prescription `{ kind: "use_allowlist" }` · `dedup_key = R19|{host}`
- advice: "세션 대부분이 권한 우회 모드로 돌고 있어요. 가이드는 우회 대신 allowlist를 권해요 —
  자주 승인하는 도구부터 등록하면 안전과 편의를 다 챙겨요." (R11과 교차 안내)

### 11.2 R20 — 프롬프트 시크릿 붙여넣기 (Warn) ★1순위

- **판별**: `secret_flag` 이벤트(§4.2) — 큐레이션 정규식 테이블(curation.rs 상수):
  `sk-ant-…`, `ghp_/gho_/github_pat_…`, `AKIA[0-9A-Z]{16}`, `xox[bap]-…`,
  `-----BEGIN … PRIVATE KEY-----`, `AIza…` (버전업 가능한 상수 목록).
  최근 30일 host 합계 ≥ 1건이면 발화.
- evidence: `{ count, by_pattern: {pattern_id: n}, session_ids }` — **매칭 본문 없음, 포인터만**.
  전문 확인은 세션 상세의 deref(v2.1 §8)로 사용자 본인만.
- est 0 · prescription `{ kind: "use_env_reference" }` · `dedup_key = R20|{host}`
- advice: "프롬프트나 명령에 API 키로 보이는 문자열이 N건 있었어요. 가이드는 키를 대화에 직접
  붙여넣지 말고 환경변수·설정 파일 참조로 넘기라고 권해요. 이미 노출된 키는 회전을 검토하세요."

### 11.3 R21 — 검증 명령 부재 (Suggest)

- **판별(이중 조건)**: 프로젝트의 대형 구현 세션(file_edits ≥ 10)이 ≥ 3건이고 **그 세션들 전부**에서
  검증 명령(Bash `command`가 큐레이션 패턴: `cargo test|npm test|pytest|go test|vitest|jest|make test` 등)
  실행 0회 **그리고** cwd의 CLAUDE.md(있는 경우)에도 해당 패턴 없음. 한 세션이라도 검증했으면 침묵.
- evidence: `{ session_ids, total_sessions, claude_md_has_verify: false }` · est 0
- prescription `{ kind: "add_verify_commands" }` · `dedup_key = R21|{host}|{project}`
- advice: "구현 세션들이 검증 없이 끝났어요. 가이드는 Claude에게 스스로 확인할 수단을 주라고
  권해요 — CLAUDE.md에 이 프로젝트의 빌드/테스트 명령 한 줄을 적어두세요." (R14와 연계)

### 11.4 R22 — 반복 실패 명령 → 환경 메모 (Suggest) ★1순위

- **판별**: 같은 (host, project)에서 동일 Bash `command`(v0: 완전 일치)가
  `result_status='error'`로 ≥ 3회 그리고 ≥ 2개 세션에 걸침. tool_call↔tool_result 조인은
  R11 방식 재사용. 노이즈 억제: 이후 같은 명령이 `ok`로 끝났으면(해결됨) 침묵.
- evidence: `{ command_preview(≤80자), fail_count, session_ids, total_sessions }` · est 0
- prescription `{ kind: "add_env_note_claude_md" }` · `dedup_key = R22|{host}|{project}|{cmd_hash}`
- advice: "여러 세션에서 같은 명령이 반복 실패했어요. 환경 정보가 없어 매번 같은 벽에 부딪히는
  거예요 — CLAUDE.md에 '이 레포에선 X 대신 Y' 한 줄을 넣어두세요."
  (에이전트 비난이 아니라 **문서화 부재**가 대상 — 레버는 사용자 소유)

## 12. 표면 변경

### 12.1 guide_ref 인프라

- `curation.rs`에 `guide_ref(rule_id) -> Option<(title, url)>` 상수 테이블
  (Best Practices, 보안, CLAUDE.md/메모리, 모델 선택 문서 — URL은 큐레이션 상수라 갱신 용이).
- `finding_advice` 반환에 guide_ref 동봉 → CoachTab 카드 하단 "📖 Anthropic 가이드" 링크 배지.

### 12.2 백엔드

- `coach.rs::fix_command`: 신규 룰 전부 None 확인 테스트만 추가(R13 설치 명령은 payload 복사 UI).
- `diary/mod.rs::finding_advice`: R10 문구 교체(§3.2), R7 확장 문구, R13~R22 분기 추가.
- `commands.rs`: R13 정리/추천 항목 렌더용 payload 통과 확인. R17용 넛지 상태는 pipeline에서 처리.

### 12.3 프론트

- CoachTab: R13 3섹션(추천/활용/정리) 카드, R14~R22 프로젝트/호스트 카드(기존 집계 카드 UI 재사용),
  guide_ref 배지.
- 마스코트/notices: R17 넛지 수신·표시(기존 findingNotice 경로와 별개 kind `nudge`).

## 13. 테스트 / 검증 기준

기존 스타일(인메모리 store + 이벤트 주입) 유지.

- **처분**: R1·R2·R9·R12 카드가 스캔 후 삭제 / R10 severity=Info·prescription 없음·est 0.
- **수집**: Agent→SubAgent 매핑 / `<local-command-stdout>` 스킵 / subagent_files 카운트 /
  plan·bypass hits / secret_flag가 본문 없이 pattern_id+포인터만 저장.
- **R13**: 패턴별 양성·음성 / 설치된 항목은 추천 제외 / rev 변경 시에만 새 dedup_key·구 카드 삭제 /
  개인 스킬 위생은 30일·500자 이중 조건.
- **R14**: 세 subtype 경계(세션 5 / 8000자 / 디렉터리 2·20편집) / cwd 로컬 부재 시 스킵.
- **R15**: 접두 80자 정규화 매칭 / 3건 경계 / command-stdout 오염 방지 회귀.
- **R16**: plan_mode_hits 또는 플랜 스킬 있으면 음성.
- **R17**: 임계·세션당 1회(nudge_log) / 윈도우 추론(>200k→1M) / 비활성 세션 미발화 /
  회고 서브타입은 마커 미핀 시 침묵.
- **R18**: 편집 0건 세션 제외 / docs 경로 제외 / SDD 후 Agent 있으면 음성.
- **R19~R22**: 각 판별 경계 + R22 "이후 ok면 침묵" + R20 본문 미저장 검증(중요).
- **E2E 최종 판정**: 실데이터에서 코치 탭이 "잔소리 3종"이 아니라 추천·위생·가이드 카드로
  구성되고, 같은 내용의 반복 노출이 없음을 사용자가 확인.

## 14. 결정 로그 (브레인스토밍, 2026-07-18 ~ 07-19)

| 논점 | 결정 |
|---|---|
| R1 | 단독 카드 폐지 → R13 정리 섹션으로 흡수 (탐지 로직 재사용) |
| R2 | 제거 — 상주분(name+desc chars/4)이 미미해 "낭비" 프레임 근거 부족 |
| R9 | 제거 — 웹 호출은 에이전트 행동. 신호는 R13 WebDocsHeavy로 재활용 |
| R10 | Info 강등 — 관찰 정보만, 처방 제거 (사용자 미소유 자동화에 수정 지시 금지) |
| R12 | R13 활용 섹션으로 흡수, 미설치 추천으로 확장 |
| R5 | 보류 유지 — R14가 취지를 에이전트 비난 없이 대체 |
| 추천 소스 | v1 오프라인 스냅숏(공식 마켓플레이스+anthropics/skills), 원격 갱신은 후속 어댑터 |
| 재노출 억제 | R13 rev 해시 dedup — 항목 집합 변화 시에만 새 카드 |
| 실시간 넛지 | finding이 아닌 별도 휘발성 표면(nudge_log, notices/말풍선), 세션당 1회 |
| 과잉 프로세스 | 결정론 프록시 2종(풀프로세스+소규모 결과, SDD-후-linear), 편집 0건 제외 |
| 개인 스킬 | 삭제 "권유"는 미사용+소형 이중 조건만, 프레임은 토큰이 아니라 오작동 방지 |
| 가이드 준수 | guide_ref 상수 테이블로 공식 문서 인용 — 권위 부여, 잔소리 감쇠 |
| 시크릿 | 본문 미저장(pattern_id+포인터), 30일 윈도우, Warn |
| est 원칙 | v3 신규 룰 전부 est 0 — 절약 수치는 R7만 유지 |
| 우선순위 | R20·R22·R13·R14 1순위, R18·R19 2순위 (PR 분할에 반영) |

## 15. 구현 시 참고 — PR 분할

| PR | 내용 | 비고 |
|---|---|---|
| ① 정리·수집 기반 | §3 처분 전부 + §4 수집(어댑터 갭·신규 스캔·마이그레이션) | 룰 추가 없음 — 기존 테스트 조정 포함. 이후 PR의 공통 선행 |
| ② 위생·가이드 룰 | R14 · R20 · R22 · R21 · R19 + guide_ref 인프라 | 1순위(R14·R20·R22) 먼저, R21·R19는 같은 PR 내 후순 태스크 |
| ③ 큐레이션·프로세스 룰 | R13(카탈로그 동봉) · R15 · R16 · R18 | R13 카탈로그 JSON 작성 포함 |
| ④ 실시간 표면 | R17 (nudge_log·활성 세션 감지·notices/말풍선) | compact 회고 서브타입은 Windows 마커 핀 후 |

- 빌드 환경(Windows): `docs/archive/design/a-mate/plans/2026-07-05-minihompy-restyle-pr1-handoff.md`의 mingw 레시피.
- 진행 관례: 이 스펙 승인 → writing-plans → SDD(태스크별 서브에이전트+리뷰) → PR → 사용자 E2E.
- compact 마커·(선택) bypassPermissions 라인 표기는 Windows 실데이터로 핀 — v2.1 §10 전례.
