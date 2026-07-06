# 코칭 v2 설계 스펙 — "실제 조치 가능한 것만 코칭한다"

- 작성: 2026-07-06 (브레인스토밍 산출물, 킥오프: `docs/brainstroming/2026-07-06-coaching-v2-kickoff.md`)
- 기준 커밋: main `194db90`+ (PR #10 포함 — 코칭 카드/세션 상세 UI, model_raw, transcript 파서 존재)
- 다음 단계: writing-plans → `docs/plans/` → SDD → PR → 사용자 E2E

## 1. 배경과 원칙

PR① E2E에서 R7("가벼운 작업에 Opus는 과해요" + `/model haiku`)이 실전에서 헛돎:
자동화 도구(cowork probe)가 만든 동형 초단기 세션마다 카드 1장 → 같은 지적 81회 노출,
그리고 처방(`/model haiku`)이 사용자의 실제 레버가 아님(세션은 도구가 생성).

**v2 원칙 (합의됨)**

1. 코칭 대상은 조치 지점이 **① 설정 파일 한 곳** 또는 **② 다음 세션의 선택(습관·작업 방식)** 인 것만.
2. 지나간 세션 안에서의 행동 교정 권고는 폐기.
3. 정밀도의 선 유지: **결정론적 사실만 서사에 인용**. 해석·가설은 사실과 구분해 표기.
4. 세션 단위 카드 스팸 폐지 → **프로젝트 집계 카드**가 기본.

## 2. 범위

| 포함 | 내용 |
|---|---|
| R10 신규 | 자동화 버스트 탐지 (프로젝트 집계) |
| R7 v2 재설계 | 세션 finding 폐기 → 프로젝트 잔심부름 비율 집계 + 시작 시점 권고 |
| R11 신규 | 권한 재시도 마찰 (프로젝트 집계) |
| R12 신규 | 설치 스킬 미활용 — 가치 제안형 (킥오프 6①) |
| 서사 강화 | R1/R2 detail에 큐레이션 용도 사전 반영 (킥오프 7) |
| UI | 집계 카드 "포함 세션 N건" 펼치기 → 기존 SessionModal 재사용 |

**제외 (후속)**

- 킥오프 3(서브에이전트 미활용)·5(컨텍스트 눈덩이): 휴리스틱 튜닝 필요, 구조 검증 후 후속 PR.
- 킥오프 6②(미설치 스킬 추천): **사내 스킬허브 검색 API 어댑터**로 확장 예정(사외망에서 접근 불가).
  이번엔 `SkillRecommendationSource` trait 시드만 설계(§5.4). 외부 전송은 프라이버시 원칙상 사내 on-prem 선택지로만.
- R3/R4/R8(캐시 히트율·MCP 대형 결과 폐기·에러 정밀 탐지): ToolResult/UserPrompt 수집 선행 필요, 유예 유지.

## 3. 데이터 모델

**스키마 변경 없음.** 기존 `findings` 테이블과 `scope_kind` 필드를 그대로 사용한다.

- 집계 finding: `scope_kind="project"`, `scope_ref=project_id`.
- dedup_key 규약: `"{rule_id}|{host}|{project_id}"` — 매 스캔마다 upsert(occurrences 증가, evidence·est_tokens_saved 갱신).
- 포함 세션 목록은 **evidence JSON에 내장**:

```json
{
  "session_ids": ["...최신순, 상한 100..."],
  "total_sessions": 81,
  "...": "룰별 통계 필드 (§4 참조)"
}
```

`session_ids`가 상한(100)을 넘으면 최신 100건만 담고 `total_sessions`로 전체 규모를 표시한다.

**이행(마이그레이션)**: 스캔 시작 시 `rule_id='R7' AND scope_kind='session'`인 기존 finding을 일괄 삭제한다.
v2에서 R7 세션 finding은 재생성되지 않으므로 1회성 정리로 충분하다. 집계가 events에서 재계산하므로 정보 손실 없음.

## 4. 룰 상세

### 4.1 공유 헬퍼 — `rules/session_stats.rs` (신규)

단일 SQL로 세션별 요약을 수집한다. `Rule` trait·`RuleEngine`은 무변경, 기존 룰(R1/R2/R5/R9)도 무변경.

```rust
pub struct SessionStat {
    pub session_id: String,
    pub host: String,
    pub project_id: String,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub assistant_turns: u64,
    pub opus_turns: u64,
    pub non_opus_turns: u64,
    pub tok_output: u64,
    pub billable: u64,        // input+output+cache_read+cache_create
    pub tool_calls: u64,
    pub heavy_tools: u64,     // sub_agent, mcp_call, web_search, web_fetch
    pub web_reqs: u64,        // 서버측 web_search+web_fetch
    pub file_edits: u64,
    pub skill_calls: u64,     // Skill 툴콜 수
    pub temp_target_hits: u64 // tool_target의 temp 경로 패턴 매칭 수
}

pub fn collect_session_stats(store: &SqliteStore) -> Result<Vec<SessionStat>>;
pub fn detect_bursts(stats: &[SessionStat]) -> Vec<BurstGroup>;
```

`detect_bursts`가 버스트 판별의 **단일 진실 공급원** — R10은 이 결과로 finding을 만들고,
R7 v2는 이 결과에 포함된 세션을 집계에서 **제외**한다(이중 지적 방지).

### 4.2 R10 — 자동화 버스트 (신규)

**판별 (보수적, 합의됨)** — 필수 조건 전부 충족 시 버스트 그룹:

- 같은 `(host, project_id)` 내에서
- 어시스턴트 턴 ≤ **3**인 세션이
- **5건 이상**, 시간순 정렬 시 **이웃 세션 간격 ≤ 10분**으로 이어진 체인
  (간격 = 이전 세션 `last_ts` → 다음 세션 `first_ts`. 체인 = 이 조건을 연속으로 충족하는 최대 그룹.
  ts 없는 세션은 판별에서 제외.)

**발화 조건**: 버스트 그룹 세션의 **과반이 Opus 전용**(opus_turns ≥1, non_opus == 0)일 때만.
(이미 haiku로 도는 자동화는 코칭 불필요 → 침묵.)

**가산 신호 (판별에 미사용, 서사에만 인용)**: temp 경로 패턴 비율(`temp_target_hits`), 동형 모델(model_raw 단일).

- evidence: `{ session_ids, total_sessions, opus_session_count, span: {from, to}, median_gap_secs, median_turns, temp_hit_ratio, dominant_model_raw }`
- est_tokens_saved: 버스트 내 Opus 전용 세션들의 billable 합 × 80% (R7과 동일한 비용-등가 가정)
- prescription: `{ kind: "automation_model_config", payload: { to: "haiku" } }`
- advice 방향: "자동화 파이프라인이 Opus로 N건 돌았어요. **그 도구의 모델 설정 한 곳**을 haiku로 바꾸면 이후 전부에 적용돼요."
- fix_command: **None** (도구 설정 위치는 사용자 환경마다 다름 — 서사로 안내)
- dedup_key: `R10|{host}|{project_id}`

**오탐 시 비용 인지**: 사람이 직접 연 짧은 세션들을 자동화로 오인하면 엉뚱한 처방이 되어 신뢰를 깎는다.
그래서 임계값을 보수적으로 두고, temp 경로 같은 정황은 판별이 아닌 서사 보강에만 쓴다.

### 4.3 R7 v2 — 프로젝트 잔심부름 비율 (재설계)

세션 판정 기준은 현행 유지(Opus 전용 + `tok_output < 700` + 도구 1~5회 + 무거운 도구 0 + 웹 0),
단 **세션 finding을 만들지 않고** 프로젝트별로 집계한다.

- 집계 대상: `detect_bursts` 결과에 포함된 세션 **제외** 후의 "가벼운 Opus 세션"
- 발화 조건: 해당 세션 ≥ **3건** 그리고 비율 ≥ **50%**
  (분모 = **버스트 세션 제외 후**의 프로젝트 세션 수 — 버스트가 많은 프로젝트에서 비율이 희석되지 않도록)
- evidence: `{ session_ids, total_sessions(=가벼운 Opus 세션 수), project_session_count, ratio_pct, sum_billable }`
- est_tokens_saved: 해당 세션들의 billable 합 × 80%
- prescription: `{ kind: "start_with_lighter_model", payload: { to: "sonnet" } }`
- advice 방향: "이 프로젝트 세션 X%가 잔심부름이었어요 — 다음엔 `claude --model sonnet`으로 시작하거나 settings.json에서 기본 모델을 낮춰보세요."
- fix_command: `claude --model sonnet` (다음 세션 시작 명령 — "다음 세션의 선택" 레버)
- dedup_key: `R7|{host}|{project_id}` (세션 스코프 키 `R7|{session}`은 폐기, §3 이행 참조)

### 4.4 R11 — 권한 재시도 마찰 (신규)

- 세션 내 동일 `(raw_name, tool_target)` **연속 ≥ 3회** 호출 run을 "마찰 이벤트" 1건으로 카운트 (실증: Write 3연발).
  "연속" = 세션 내 이벤트를 ts·source_offset 순으로 정렬했을 때 사이에 다른 도구 호출이 끼지 않은 run.
- 발화 조건: 프로젝트에 마찰 이벤트 ≥ **2건**
- evidence: `{ session_ids, total_sessions, friction_events: [{tool, target, run_length, session_id}], by_tool: {Write: n, ...} }`
- est_tokens_saved: **0** — tool_call 이벤트에는 토큰 컬럼이 없어 결정론적 추정이 불가하다.
  근거 없는 절약 수치를 만들지 않는다(정밀도의 선). 카드 가치는 마찰 제거 서사로 전달.
- prescription: `{ kind: "permission_allowlist", payload: { tools: [...] } }`
- advice 방향: **사실과 가설 구분 표기** — "같은 대상에 `Write`가 연속 3회 호출된 패턴이 N건 있었어요(사실).
  권한 거부 후 재시도일 수 있어요(가설) — settings.json 허용목록에 추가하면 거부→재시도 낭비가 사라져요."
  (ToolResult 미수집 상태라 거부 여부를 단정하지 않는다.)
- fix_command: **None** (허용목록 편집은 서사로 안내)
- dedup_key: `R11|{host}|{project_id}`

### 4.5 R12 — 설치 스킬 미활용 (신규, 킥오프 6①)

가치 제안형 코칭 — 절약이 아닌 "이 스킬을 쓰면 더 잘 됩니다".

**추천 소스 추상화 (6② 확장 시드)**:

```rust
pub trait SkillRecommendationSource {
    /// 세션 패턴에 매칭되는 추천 (설치 여부 판단은 룰 쪽 책임)
    fn recommend(&self, pattern: &WorkPattern) -> Vec<SkillRecommendation>;
}
```

v2 구현은 **내장 큐레이션 테이블 소스 1개**(작업 패턴 → 추천 스킬).
사내 스킬허브 검색 API 어댑터는 후속(사내망 작업)으로, 이 trait 뒤에 붙는다.

**v1 큐레이션 테이블 (1개 패턴으로 시작)**:

| 패턴 | 조건 | 추천 |
|---|---|---|
| 대형 구현 세션인데 스킬 미사용 | `file_edits ≥ 10` AND `skill_calls == 0` | superpowers 플랜/브레인스토밍/SDD 계열 |

- 발화 조건: 패턴 매칭 세션 ≥ **2건**(프로젝트 집계) AND 추천 스킬이 `plugin_inventory`에 **설치되어 있음**.
  미설치면 침묵(6② 전까지 미설치 추천 없음).
- evidence: `{ session_ids, total_sessions, pattern: "large_impl_no_skill", recommended_skills: [...] }`
- est_tokens_saved: 0 (가치 제안형 — 토큰 절약 주장 안 함)
- prescription: `{ kind: "use_skill", payload: { skills: [...] } }`
- advice 방향: "대형 구현 세션 N건에서 플랜/브레인스토밍 스킬을 한 번도 쓰지 않았어요.
  확정 플랜 구현은 subagent-driven-development로 — 컨트롤러만 Opus, 구현은 sonnet."
- fix_command: **None**
- dedup_key: `R12|{host}|{project_id}`

### 4.6 서사 강화 — R1/R2 용도 사전 (킥오프 7, 룰 아님)

정적 큐레이션 사전(코드 내 상수 테이블): 알려진 플러그인/MCP 서버 → 한 줄 용도 설명.

- `finding_advice`(R1/R2 분기)가 사전에 항목이 있으면 detail에 용도 문장을 삽입:
  "frontend-design 플러그인은 UI 디자인 작업에 유용해요. 하지만 호출 0회 — 상주 토큰만 소비 중이에요."
- 사전에 없으면 현행 문구 그대로 (결정론 유지 — "최근 작업과 무관" 같은 의미 대조는 하지 않는다).
- 사전 위치: 신규 모듈 `crates/core/src/curation.rs` — 용도 사전(§4.6)과 R12 큐레이션 테이블(§4.5)을 함께 둔다.
  스킬 description 활용은 후속 검토.

## 5. 표면 변경

### 5.1 백엔드

- `diary/mod.rs::finding_advice`: R10/R11/R12 분기 추가, R7 문구 교체(프로젝트 집계 서사), R1/R2 용도 사전 참조.
- `coach.rs::fix_command`: R7 → `claude --model sonnet`으로 교체(기존 `/model haiku` 폐기). R10/R11/R12 → None.
- `commands.rs`: 신규 커맨드 `sessions_ctx(ids: Vec<String>) -> Vec<SessionCtxItem>` —
  집계 카드 펼치기용 배치 조회. `SessionCtxItem = { session_id, project_id, first_ts }` (기존 `SessionCtx` 확장 재사용).

### 5.2 프론트 (CoachTab)

- `scope_kind == "project"` 카드에 **"포함 세션 N건" 펼치기** 추가:
  evidence의 `session_ids`로 `sessions_ctx` 호출 → 목록 렌더 → 항목 클릭 시 기존 `SessionModal` 재사용.
- `total_sessions > session_ids.length`이면 "최신 100건 표시 중" 안내.
- 기존 세션 스코프 카드 UI는 무변경(R5/R9가 계속 사용).

## 6. 테스트 / 검증 기준

기존 룰 테스트 스타일(인메모리 store + 이벤트 주입) 유지.

- **R10**: 양성(동형 초단기 5건, 간격 짧음, Opus) / 음성(4건뿐, 간격 큼, 턴 많음, 과반 비Opus) / 가산 신호가 판별에 영향 없음
- **R7 v2**: 버스트 세션 제외 검증(버스트에 흡수된 세션은 비율 계산에서 빠짐) / 비율·최소 건수 경계 / 세션 finding 미생성
- **R11**: 연속 3회 경계(2회는 미발화) / 비연속 3회는 미발화 / 마찰 이벤트 1건뿐이면 미발화
- **R12**: 매칭+설치 시 발화 / 미설치 시 침묵 / 스킬 사용 세션은 미매칭
- **이행**: 기존 R7 세션 finding이 스캔 후 삭제됨
- **E2E 최종 판정**: 사용자 실데이터에서 기존 R7 카드 81장 → R10 집계 카드 1장으로 흡수 확인

## 7. 결정 로그 (브레인스토밍, 2026-07-06)

| 논점 | 결정 |
|---|---|
| 범위 | 1(R10)·2(R7v2)·4(R11)·6①(R12)·7(서사 강화). 3·5·6②는 후속 |
| 6② | 사내 스킬허브 검색 API 어댑터로 확장(사외망 접근 불가) → trait 시드만 이번에 |
| 집계 세션 목록 저장 | evidence JSON 내장(상한 100 + total) — 스키마 변경 없음 |
| 기존 R7 세션 finding | 폐기(삭제). 집계가 events에서 재계산하므로 손실 없음 |
| 버스트 오탐선 | 보수적: 턴 ≤3 · N ≥5 · 간격 ≤10분 필수, temp 경로·동형 모델은 서사 전용 가산 신호 |
| 룰 구조 | 공유 세션요약 헬퍼(session_stats) + detect_bursts 단일 공급원. trait·엔진·기존 룰 무변경 |

## 8. 구현 시 참고

- 빌드 환경(Windows): `docs/plans/2026-07-05-minihompy-restyle-pr1-handoff.md`의 mingw 레시피 필수.
- 진행 관례: 이 스펙 승인 → writing-plans(`docs/plans/`) → SDD(태스크별 서브에이전트+리뷰, 최종 whole-branch 리뷰) → PR → 사용자 E2E.
