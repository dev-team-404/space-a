---
status: done
archived: 2026-07-21
---

# 반복 코칭 재설계 — 데이터 위생과 의미 판정

- **날짜**: 2026-07-21
- **상태**: 승인 (구현 대기 — PR1 → PR2 순차)
- **관련 문서**: [R6 v2 세션 반복 마이닝](2026-07-20-r6-v2-session-repeat-mining.md),
  [코칭 v3 설계](2026-07-19-coaching-v3-design.md)
- **대체**: R6 v2 스펙의 §3(R23)을 폐기하고, R6 판정 방식을 확장한다.

## 1. 문제 — 2026-07-21 실사용 판정

PR #77(R6 v2)·#78(R23 카드 홍수 수정) 머지 후에도 코치 탭이 junk 카드로 채워졌다.
조사 결과 **이전 PR들이 고친 것(카드 수량 억제)과 별개의 원인**이 확인됐다.

### 1.1 근본 원인 (실데이터 검증 완료)

| # | 원인 | 증거 | 영향 |
|---|------|------|------|
| 1 | **resume 히스토리 복제** — Claude Code는 대화를 resume/포크하면 새 세션 파일에 전체 히스토리를 복사한다. sessionId·uuid는 새로 발급, **timestamp·tool_use_id·message.id는 보존** | 같은 프롬프트가 WSL 세션 파일 3개에 동일 ts(`2026-07-08T08:53:54.410Z`), 서로 다른 uuid/sessionId로 존재. `toolu_01NVwx…`·`msg_011Ccp9b…`가 3파일에 동일 | 대화 1개를 2번 resume → 모든 프롬프트·도구 시퀀스가 "3개 세션"이 되어 `min_sessions=3` 자동 통과. R6 카드 15장 중 12장이 이것 |
| 2 | **다중 라인 usage 반복** — 한 API 응답이 여러 assistant 라인으로 쪼개질 때 usage가 라인마다 반복 기록됨 | 실측 파일: assistant 601줄 = 실제 메시지 233개, `output_tokens` 1556이 3줄에 반복 | AssistantTurn을 라인마다 방출 → **토큰 통계 ~2.6배 과대 계상** (포크와 무관하게 상시) |
| 3 | **인터럽트 합성 마커 미필터** — `is_synthetic_marker`는 `<` 접두만 거름 | `[Request interrupted by user]`가 43개 파일에 존재 → 27세션 카드 | R6 junk 카드 |
| 4 | **tokenizer 구멍** — env 할당 접두가 명령 토큰이 됨 | `bash:test_database_url=postgresql+asyncpg://postgres:postgres@…` 카드 — **접속 문자열(자격증명)이 evidence에 노출** | R23 junk 카드 + 프라이버시 원칙(본문 미저장) 위반 |

### 1.2 제품 판단

- **R23(도구 시퀀스 카드)은 전제가 약하다.** 도구 선택은 사용자가 아니라 에이전트가 한다.
  도구 종류 n-gram에 잡히는 반복은 대부분 에이전트의 자율 루프이거나 이미 스킬로 자동화된
  워크플로다(`skill:superpowers:writing-plans` 포함 시퀀스에 "스킬로 만드세요" 제안 = 순환).
  두 차례의 필터 수리(PR #77/#78)가 계속 필요했다는 사실 자체가 신호다.
- **R6(반복 지시)은 "반복됐는가"만 보고 "스킬로 만들 가치가 있는가"를 보지 않는다.**
  어휘 동치 상위권은 필연적으로 대화 접착제("진행해줘")다.
- **코칭 카드는 틀렸을 때 비용이 비대칭적으로 크다.** junk 90%면 코치 탭 전체가 불신받는다.
  이 룰들에는 recall이 아니라 **precision이 전부**다.

## 2. 결정 요약 (사용자 확정, 2026-07-21)

| # | 결정 | 선택 | 근거 |
|---|------|------|------|
| 1 | 스코프 | **한 스펙, 2 PR 분리** — PR1 데이터 위생 → PR2 판정 레이어 | PR1은 기계적 수정, PR1 후 정화된 카드를 관찰해 PR2 세부 재보정 |
| 2 | R23 처분 | **카드 폐기** | 도구 토큰에는 사용자 의도가 없어 LLM 판정을 붙여도 재료 부족은 그대로 |
| 3 | R6 판정 시점 | **스캔 후 비동기 배치** | 스캔이 네트워크에 안 묶이고, 엔진 미설정 시 fail-safe 침묵 |
| 4 | 신호원 범위 | **판정 게이트만** (기존 어휘 채굴 유지) | YAGNI — 교정 발화·의미 군집은 검증 후 후속 |
| 5 | dedup 위치 | **쓰기 시점 논리 키** | 중복이 DB에 안 들어와 모든 하류(롤업·룰·세션 수)가 자동 교정 |

## 3. PR1 — 데이터 위생

### 3.1 쓰기 시점 논리 dedup 키

`store::upsert_events`의 dedup_key를 kind별 논리 식별자로 교체한다.

| 이벤트 | 새 dedup_key | 폴백 (식별자 부재 시) |
|--------|-------------|----------------------|
| ToolCall | `tc:{host}:{tool_use_id}` | `uuid:offset` (기존 규칙) |
| ToolResult | `tr:{host}:{tool_use_id}` | 〃 |
| AssistantTurn | `at:{host}:{message.id}` | 〃 (`<synthetic>` 등 id 없는 라인) |
| UserPrompt → prompt_events | `up:{host}:{project}:{ts}:{sha8(preview)}` | 〃 |
| Compaction·PermissionMode·SessionMeta | 기존 유지 | — |

- adapter가 assistant 라인의 `message.id`를 추출해 `NormalizedEvent`에 신규 필드로 전달한다.
- host를 키에 포함 — 호스트 간 우연 충돌 방지(대화는 호스트를 넘지 않지만 안전 마진).
- 효과 세 가지가 한 수정으로 해소:
  1. resume 포크 복제본이 DB에 안 들어옴 → 세션 수 부풀림·토큰 이중 계산 소멸
  2. 같은 `message.id`의 AssistantTurn은 1개만 저장 → usage 반복 과대 계상 정상화
     (ToolCall은 tool_use_id가 블록마다 달라 전부 보존)
  3. 포크 파일에는 새 tail 이벤트만 남아 세션 카운트가 "대화" 단위에 수렴
- **첫 스캔 파일이 이긴다**: 어느 복사본이 남는지는 스캔 순서에 달렸지만 내용이 동일해 무해.

> ⚠️ **배포 후 대시보드 토큰 수치가 크게 줄어든다.** 버그가 아니라 과대 계상의 정상화다.

### 3.2 인터럽트 마커 필터

`adapter::is_synthetic_marker`에 `[Request interrupted` 접두 추가.
`[Request interrupted by user]`·`[Request interrupted by user for tool use]` 카드 소멸.

### 3.3 tokenizer 수정

R23 카드는 PR2에서 폐기되지만, PR1 시점에는 룰이 살아 있고 접속 문자열 노출은 즉시 막아야 한다.

- `NAME=value` 형태의 env 할당 접두를 건너뛰고 실제 명령을 토큰으로
  (`TEST_DATABASE_URL=… pytest` → `bash:pytest`; 할당만 있으면 `bash`).
- 토큰에 시크릿 패턴(`curation::find_secret_patterns` 재사용) 감지 시 `bash`로 강등.
- `export`·`set`·`env`를 GENERIC_BASH에 추가.

### 3.4 마이그레이션 `user_version=6`

- 전체 재수집: events / sessions / ingest_state / daily_rollup / prompt_events 비움.
- R6·R23 `status='new'` finding 삭제(junk 정화). **dismissed/resolved는 보존** —
  dedup_key(`R6|host|hash(norm)`)가 안정적이라 나깅 방지 쿨다운이 유지된다.
- PR1 배포 후 정화된 어휘 카드가 잠시 노출된다 — PR2 세부(판정 프롬프트·문턱)를
  재보정하는 관찰 체크포인트로 삼는다.

## 4. PR2 — R6 의미 판정 레이어 + R23 폐기

### 4.1 Finding 라이프사이클

```
채굴(SQL, 결정론) → status='pending' (비노출)
                      │  판정 패스 (Engine)
                      ├─ worthy   → status='new' (카드 노출)
                      └─ unworthy → status='rejected' (비노출, 영구 캐시)
```

- status 값 `pending`/`rejected` 추가. UI·집계는 이미 `status='new'`만 노출 → 변경 최소.
- `upsert_finding`의 INSERT 초기 status를 룰별 지정(R6→`pending`, 나머지→`new`).
  ON CONFLICT는 기존대로 status 불변 → **판정 캐시가 재스캔에도 유지**.
- 판정 결과는 evidence가 아닌 **신규 컬럼 `judgment_json`**에 저장 — 룰 재평가의
  evidence 갱신이 판정을 덮지 않고, 병합 로직이 불필요.

### 4.2 판정 패스 (비동기 배치)

- 위치: `src-tauri/pipeline.rs` 스캔 완료 후 비동기 실행.
- 엔진: **다이어리와 같은 Engine 설정 재사용**(`diary::engine`, on-prem 기본).
  미설정이면 패스 스킵 — pending은 영원히 비노출(fail-safe 침묵).
- 배치: `rule_id='R6' AND status='pending'` 중 시도 횟수 3 미만
  (`json_extract(judgment_json,'$.attempts')`, NULL=0)을 `last_seen DESC LIMIT 10`으로 선별.
- 후보 입력: 기존 `skill_draft::gather_context` 재사용
  (대표 프롬프트·변형 표본 ≤5·세션 수·주요 도구).
- 응답: 엄격 JSON `{"worthy": bool, "reason": "한국어 한 문장", "suggested_name": "kebab-slug"}`.
- 에러 구분:
  - **전송 실패**(엔진 다운·타임아웃) → attempts 미증가, 다음 스캔 재시도.
  - **형식 불량**(JSON 파싱 실패) → attempts+1(`judgment_json`에 기록), 3회 도달 시
    pending 잔류·재시도 중단. 인프라 실패를 `rejected`로 오캐시하지 않는다.
- 사용 토큰은 `judgment_json.tokens`에 기록.

### 4.3 판정 프롬프트 (정밀도 우선)

- system: 코치 페르소나 + 기준.
  - worthy = 재사용 가능한 **절차·규칙·체크리스트**를 담은 지시.
  - unworthy = 대화 접착제("진행해줘"), 일회성 문맥 의존 지시, 질문·피드백, 인사.
  - **"확신이 없으면 worthy=false"를 명시** — precision > recall.
  - JSON 외 출력 금지.
- user: 대표 프롬프트 + 변형 표본 + 세션 수 + 주요 도구 (한국어).

### 4.4 R23 폐기

- `RuleEngine` 등록에서 R23 제거 — finding 생산 중단.
- `r23_tool_sequences.rs`: 다른 소비자가 없음을 plan 단계에서 확인 후
  `tokenize`/`collect_streams`/`sessions_containing`/`gather_context_for_sequence` 포함 삭제.
  (R6 스킬 초안의 도구 통계는 `tool_usage_for_sessions`(raw_name 기반)라 무관.)
- UI: `coach-helpers`/`CoachTab`의 R23 카드 매핑, `api.ts` 타입 제거.

### 4.5 UI (소폭)

- R6 카드에 판정 사유 1줄 표시("코치 판정: …").
- `suggested_name`을 '스킬 초안 만들기'의 기본 이름으로 전달. 초안 생성은 기존 클릭 시 흐름 유지.

### 4.6 마이그레이션 `user_version=7`

- R23 finding **전량 삭제**(dismissed 포함 — 룰이 사라지므로 쿨다운 기록도 무의미).
- 기존 R6 `new` → `pending` 전환(정화된 카드도 판정을 거치도록). dismissed/resolved 보존.

## 5. 테스트 계획

| PR | 대상 | 검증 |
|----|------|------|
| 1 | 포크 시뮬레이션 | 같은 msg.id/toolu/ts+내용을 다른 session_id·source_file로 upsert → 각 1행만 |
| 1 | 다중 라인 | 같은 msg.id 3줄 → AssistantTurn 1개·토큰 1회 합산, ToolCall은 각각 보존 |
| 1 | R6 | 포크 3파일 = 세션 1 → 문턱 미달 침묵 |
| 1 | 마커 | `[Request interrupted…]` prompt_events 미적재 |
| 1 | tokenize | env 접두 스킵, export 일반 판정, 시크릿 토큰 강등 |
| 1 | 마이그레이션 v6 | 재수집 트리거, dismissed 보존, 멱등 |
| 2 | 라이프사이클 | pending 비노출, worthy→new, unworthy→rejected 영구 캐시(재스캔 시 재판정 없음) |
| 2 | 판정 패스 | MockEngine으로 worthy/reject/malformed/전송실패 4시나리오, attempts 상한, 배치 상한 10 |
| 2 | 엔진 부재 | 패스 스킵, R6 카드 0장 |
| 2 | 마이그레이션 v7 | R23 전량 삭제, R6 new→pending, 멱등 |
| 2 | UI 헬퍼 | R23 매핑 제거, 판정 사유 렌더 |

## 6. 비범위 (후속 후보)

- **반복 교정 발화 룰** — "아니, 커밋 메시지는 영어로" 류를 CLAUDE.md 규칙 제안으로 연결.
  별도 브레인스토밍으로 진행.
- **의미 군집화** — 어휘 동치가 아닌 의미 유사 프롬프트 묶기(임베딩/LLM).
- **세션 리스트의 대화 단위 표시** — sessions 테이블은 포크 파일마다 행이 남는다(무해).
  대화 계보 UI가 필요해지면 별도 검토.
- **SecretFlag·PermissionMode·Compaction의 포크 복제 dedup** — 현재 소비 룰이 없어
  영향 0이지만, R16/R19류 룰이 생기기 전에 논리 키 부여를 검토 (PR1 최종 리뷰 지적).
