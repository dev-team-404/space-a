# 맥락 인지 마스코트 — 다이어리 도구·근무맥락 반영 + 잡담 코믹 위로 설계 스펙

- 작성: 2026-07-12 (브레인스토밍 산출물)
- 배경: PR #22(다이어리 다양성) 재생성 E2E 사용자 피드백 —
  ① 이모지가 일기당 1개뿐, 더 쓰면 좋겠다,
  ② recent_diaries를 넣었는데도 **매일 context7 얘기로 시작하고 내용이 거의 같음**,
  ③ 주말·공휴일·장시간·연속세션 근무 시 주인을 위로·응원해주면 좋겠다(다이어리 + 잡담),
  ④ 도구 사용 현황(오늘 스킬 10개 썼다 등)을 일기에 반영하면 좋겠다.
- 선행: `docs/specs/2026-07-10-diary-variety-ui-fixes-design.md`(PR #22), `voice_guidance()`(diary/mod.rs) 3표면 공유
- 다음 단계: writing-plans → 표면별 2묶음 구현 → PR

## 1. 진단 (이 세션 재생성으로 확인)

Windows 다이어리는 `findings_for_date("Windows", date)`만 소재로 쓰는데, 상시(host 스코프) finding 중
매일 나오는 건 **R1 context7(est 2500) 하나뿐**(나머지 R1/R2는 `wsl:*` 호스트라 Windows 일기엔 안 들어옴).
즉 매일 브리프의 유일한 코칭이 context7 → LLM이 매일 그것으로 시작·같은 내용. recent_diaries의
"반복하지 마라" 소프트 지시만으론 역부족 — **모델이 쓸 다른 소재가 브리프에 없다.**
재생성된 07-05·07-07·07-08·07-10 일기 4편 전부 context7으로 시작함을 육안 확인.

→ 해결의 핵심: **그날그날 달라지는 소재(도구 사용량·근무 맥락)를 브리프에 넣고, 이미 다룬 상시 finding은 누른다.**

## 2. 브레인스토밍 결정 (2026-07-12)

| 논점 | 결정 |
|---|---|
| 범위 | **표면별 2묶음** — A. 다이어리(PR #22 브랜치 이어서), B. 잡담(신규 PR). 도구/모델 대시보드 탭은 **유예**(별도 브레인스토밍) |
| 반복 처리 | **접근 1** — 상시 finding 감면·그날 얘기로 전환. `recently_covered` 구조 신호 + day-centric 프롬프트. 새 코칭 없으면 억지 지적 금지 |
| 도구 사용 반영 | 브리프에 `tool_usage` 신호 추가(그날 도구 집계) — 반복 해소의 핵심 재료 겸 사용자 요청 ④ |
| 위로 문턱 | 다이어리 위로: 하루 세션 지속시간 **합 ≥ 5시간**. 잡담 쉬어라: 하루 **세션 ≥ 5번** (둘 다 조정 가능 상수) |
| 공휴일 위로 | **LLM 판단** — occasions 그대로 쓰고 "기념일인데 일했으면 위로" 지시, 쉬는 명절 vs 재미 기념일 구분은 LLM 상식에 맡김(신규 신호 없음) |
| 이모지 | 다이어리 프롬프트만 "문단마다 1~2개"로 상향. `voice_guidance` ⑥(남발 금지) 불변 |

## 3. 데이터 근거 (조사 완료)

- **도구 사용량**: `events.tool_kind`(file_read/file_edit/file_write/search/execute/mcp_call/sub_agent/skill/web_*) + `tool_server`(MCP) + `tool_target`(스킬명). `date(ts,'localtime')` 버킷 집계 가능.
- **근무시간**: `sessions.first_ts`/`last_ts`(세션별). 하루 지속시간 합 = Σ(last_ts−first_ts).
- **세션 수**: `daily_rollup.session_count`(이미 `Brief.totals.session_count`).
- **주말**: `date.weekday()`. **공휴일**: `compute_occasions`가 이미 계산(category="holiday").

## 4. 변경 상세

### 묶음 A — 다이어리 (`crates/core/src/diary/mod.rs` + `store.rs`, PR #22 브랜치 이어서)

**A1. 브리프 신호 3종**

1. `BriefFinding.recently_covered: bool` — `assemble_brief`에서 recent_diaries 날짜들의
   `findings_for_date(host, d)` dedup_key 집합을 만들고, 오늘 finding의 dedup_key가 그 안에 있으면 true.
   (요 며칠 일기에서 이미 다룬 상시 이슈 표시)
2. `Brief.tool_usage: ToolUsage` — `store::tool_usage_for(host, date)`:
   ```
   struct ToolUsage {
     total_calls: u64,
     by_kind: Vec<(String, u64)>,   // 0 아닌 kind만, count 내림차순
     skills: Vec<String>,           // distinct 스킬명, 최대 8
     mcp_servers: Vec<String>,      // distinct MCP 서버명, 최대 8
   }
   ```
3. `Brief.work_context: WorkContext` — `store::work_stats_for(host, date)` + 날짜 계산:
   ```
   struct WorkContext {
     is_weekend: bool,      // 토·일
     active_hours: f64,     // Σ 세션 지속시간(시간, 소수 1자리)
     long_work: bool,       // active_hours >= LONG_WORK_HOURS (5.0)
   }
   ```
   `const LONG_WORK_HOURS: f64 = 5.0;`

**A2. store 쿼리 2종**

- `tool_usage_for(host, date)`: `SELECT tool_kind, COUNT(*) ... WHERE host=?1 AND date(ts,'localtime')=?2 AND tool_kind IS NOT NULL GROUP BY tool_kind` + 스킬/서버 distinct는 `tool_target`/`tool_server`로.
- `work_stats_for(host, date)`: `SELECT COALESCE(SUM((julianday(last_ts)-julianday(first_ts))*24),0) FROM sessions WHERE host=?1 AND date(first_ts,'localtime')=?2` → active_hours.

**A3. `build_system_prompt` 재조정** (형식/소재 지시 문단)

- 반복: "`recently_covered: true`인 finding은 요 며칠 일기에서 이미 다룬 상시 이슈다 — 오늘은 그걸로 시작하지 말고, 정 필요하면 맨 뒤 한 줄로만. `false`(새것)를 우선 소재로. 새 코칭이 없으면 억지로 지적을 만들지 말 것."
- 도구 텍스처: "`tool_usage`로 그날의 리듬을 살려라 (예: '오늘 스킬을 열 번 넘게 불러서 정신없었네', '파일만 계속 뒤졌다')."
- 위로/응원: "`work_context.is_weekend`이거나 occasions에 명절·공휴일이 있는데도 일했다면, 쉬는 날 함께해줘 고맙다는 위로·응원을 한마디 (단 재미 기념일인 발렌타인·파이데이 등은 위로 대상 아님 — 상식으로 구분). `long_work`면 '오래 붙어 있었네, 무리하지 마' 식으로 챙겨라."
- 이모지: 기존 "문단마다 1개 정도" → **"문단마다 1~2개, 겹치지 않게"**.

### 묶음 B — 잡담 (Mascot, 신규 PR)

`compute_chatter_pool`이 쓰는 컨텍스트에 근무 맥락(오늘 주말 여부·오늘 세션 수) 추가 →
`build_chatter_prompt`에 코믹 지시: 주말이면 "주말에 나와서 뭐하는 짓이야ㅋㅋ", 오늘 세션이
`CHATTER_REST_SESSIONS(=5)` 이상이면 "그만 좀 하고 쉬고 와라" 류. 사실주장 금지·기존 정적 폴백 유지.
(fp/캐시/트레이 레벨 등 PR #18 구조 재사용, 콘텐츠만 확장)

### 재생성 절차 (코드 아님 — 묶음 A 머지 후)

앱 종료 → `diary_index` 창(오늘 −7~−1) 삭제(**findings 유지**) → 실엔진 앱 실행 → backfill 재생성 →
육안: 날마다 다른 이야기·도구 언급·주말/장시간 위로·이모지↑.

## 5. 검증 / 테스트 기준

- **core (묶음 A)**:
  - `tool_usage_for` — kind별 카운트·distinct 스킬/서버.
  - `work_stats_for` — 세션 지속시간 합, long_work 경계(5h).
  - `recently_covered` — 최근 일기 finding에 있으면 true / 없으면 false.
  - `assemble_brief` — tool_usage·work_context·recently_covered 채워짐.
  - 프롬프트 마커 — recently_covered·tool_usage·위로·이모지("문단마다 1~2개").
  - 기존 `system_prompt_*`·`assemble_brief_*`·`recent_diaries_*` 회귀(수동 Brief 생성부 보정).
- **core (묶음 B)**: chatter 컨텍스트/프롬프트에 주말·연속세션 코믹 신호 마커.
- **최종 판정**: 재생성 육안(§4 재생성 절차).

## 6. 스코프 밖

- **도구/모델 사용 대시보드 탭**(아이디어 ⑤) — 별도 브레인스토밍/스펙/PR(미니홈피 컨셉, 다이어리 탭 하단 vs 별도 탭 결정 포함).
- `voice_guidance` 수정, 톤 프리셋, 달력·홈 UI, 한마디(daily-line) 프롬프트, 다크 테마, 신규 색 토큰.
- recent_diaries 메커니즘 자체(PR #22에서 도입 완료 — 여기서는 recently_covered로 보강만).

## 7. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- 묶음 A는 `feat/diary-variety-ui-fixes`(PR #22) 브랜치에 이어서. 묶음 B는 신규 브랜치 → 별도 PR(base=main).
- 커밋 태스크 단위, main 직접 커밋 금지.
