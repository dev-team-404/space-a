# a-mate → a-hub 코칭 지식 공유 — 설계 스펙

> **목적**: a-mate(Agent Mentor)가 규칙 엔진으로 찾은 코칭 발견(Finding)을 a-hub("Space A")에
> 자동 발행해, **"개인의 시행착오가 조직의 자산이 된다"**(README 취지)를 Pillar 1↔2 사이에서 실증한다.
> 관련: [컴포넌트 소통 지도](../../../highlevel/level-1-component-communication.md) ·
> [Space A 작성 지침](../../collab-space/08-writing-guide.md) · [space-a-hub Skill](../../../../.claude/skills/space-a-hub/)

## 0. 30초 요약

- **무엇**: 스캔이 끝나면, **팀에도 유효한** Finding만 골라 a-hub에 `open_issue → resolve_issue(publish_knowledge)`로 발행한다. 해결책이 Page가 되어 다른 에이전트가 검색·인용(cite)할 수 있다.
- **원칙**: 결정론 본문만(정밀도의 선) · 개인정보 스크럽 · dedup 1회 발행(나깅 방지) · 실패해도 파이프라인 무해(관대함) · 환경변수 미설정 시 조용히 off(프라이버시 기본 = 로컬).
- **상태**: core `hub.rs` + store `hub_share_state` + 파이프라인 편승 + CLI `hub-share`. 실서버 E2E로 공유→검색→인용 루프 검증.

## 1. 무엇을 보낼 가치가 있나 — 룰별 검토

코칭 Finding은 기본적으로 **개인 관찰**이다. 팀 자산이 되려면 "남도 같은 상황을 겪고, 같은 처방이 통하는가"를 통과해야 한다. 전 룰을 검토했다:

| 룰 | 내용 | 팀 일반화 가능? | 결론 |
|---|---|---|---|
| R1 | 미사용 always-on MCP (상주 토큰) | ✅ 같은 MCP를 쓰는 팀원 전원에게 동일 낭비·동일 처방(`claude mcp remove`) | **공유** |
| R2 | 미사용 플러그인/스킬 상주 비용 | ✅ 동일 (설치 자산은 팀 단위로 퍼짐) | **공유** |
| R5 | 크로스세션 반복 Read 관찰 | ❌ 프로젝트·개인 환경 종속 관찰 지표 | 제외 |
| R7 | 단순 작업 상위 모델 비율 | ❌ 개인 습관. 일반화하면 "뻔한 조언"(level-0 경계) | 제외 |
| R9 | 웹 도구 남용 | ❌ 세션 스코프 개인 패턴 | 제외 |
| R10 | 자동화 버스트 (도구가 만든 동형 세션) | ✅ 팀 공용 자동화 도구가 원인이면 전원이 같은 낭비를 겪는 중 | **공유** |
| R11 | 권한 마찰 ("거부→결국 승인") | ✅ 같은 레포면 같은 allowlist 처방이 통함 | **공유** |
| R12 | 설치 스킬 미활용 (가치 제안) | ✅ "이 스킬 이럴 때 좋다"는 곧 팀 지식 | **공유** |

**화이트리스트: R1 · R2 · R10 · R11 · R12.** 그 외 룰(신규 포함)은 명시적으로 추가되기 전까지 공유하지 않는다(기본 폐쇄).

## 2. 언제·얼마나 — 나깅/노이즈 방지

허브도 "나깅 방지" 대상이다. 개인 앱에서 스팸이던 것은 허브에선 조직 스팸이 된다.

- **1 Finding = 평생 1회 발행.** `hub_share_state`(dedup_key PK)에 발행 기록을 남기고 재스캔에도 재발행하지 않는다. 앱의 finding dedup 철학(occurrences만 증가)의 허브 확장판.
- **유의미 문턱**: `est_tokens_saved ≥ SPACE_A_SHARE_MIN_TOKENS`(기본 1000). 정량 근거가 없는 룰(R11은 의도적으로 est=0 — "근거 없는 수치 금지")은 `occurrences ≥ 3`으로 대체 — 반복 확인된 패턴만.
- **스캔당 최대 3건.** 첫 도입 시 백로그가 한꺼번에 쏟아지는 것을 방지(초과분은 다음 스캔).
- **status='new'만.** 사용자가 해결함/무시한 finding은 보내지 않는다 — 사용자 판단 존중.

## 3. 어떤 형식으로 — Issue→resolve, Page 저작이 아니라

[작성 지침](../../collab-space/08-writing-guide.md)의 구분을 따른다:

- **Issue → resolve** = "문제 해결의 기록"(해결책이 자동으로 Page 발행). 코칭 Finding은 정확히 이 형태다 — 문제(낭비 감지) + 검증된 처방(결정론 규칙).
- 직접 Page 저작은 가이드·레퍼런스용이므로 쓰지 않는다.
- `resolve_issue`에 `publish_knowledge: true, visibility: "org"` — "지식은 순환하라고 있는 것"(지침 기본값).

부수 효과: 발행된 Page는 다른 에이전트의 `search_knowledge`에 잡히고 `cite`가 ReuseEvent를 만든다 — **README의 A팀→B팀 시나리오가 그대로 성립**한다.

## 4. 본문 구성 — 정밀도의 선 + 개인정보 스크럽

**LLM은 개입하지 않는다.** 제목·본문·steps 전부 규칙별 결정론 템플릿이다(환각 불가). 수치는 앱과 같은 "약(~)" 라벨.

evidence에는 개인정보성 필드가 있다 — **필드 화이트리스트만 전송**한다:

| 룰 | 보내는 것 | 보내지 않는 것 (스크럽) |
|---|---|---|
| R1 | server 이름, 상주 토큰 추정, 처방 명령 | scope_project(로컬 경로 슬러그), host |
| R2 | plugin 키, 상주 토큰, 처방 명령 | 〃 |
| R10 | 버스트 세션 수, 모델 전환 처방 | **cwd·대표 세션 첫 요청 미리보기**(프롬프트 원문), session id |
| R11 | 마찰 도구 이름들, allowlist 처방 | **friction_events 상세**(명령줄 포함 가능), session_ids |
| R12 | 스킬 이름들, 가치 제안 | 〃 |

프로즈(대화 원문)는 애초에 DB에 없고(포인터만), 포인터는 전송 대상이 아니다.

## 5. 설정과 프라이버시 경계

Engine 선례(§ CLAUDE.md "외부 전송은 선택으로만")를 그대로 따른다: **환경변수를 설정하는 행위 = egress 동의**.

```
SPACE_A_HUB_URL      # 필수. 미설정이면 공유 기능 전체가 조용히 no-op
SPACE_A_API_KEY      # 서버 게이트 키 (배포가 요구할 때)
SPACE_A_TOKEN        # 선택. 없으면 최초 1회 자동 register 후 settings(hub_token)에 보존
SPACE_A_SPACE_ID     # 선택. 기본 sw-innov
SPACE_A_USER         # 선택. register용 user_id. 기본 %USERNAME%
SPACE_A_SHARE        # 선택. "off"면 URL이 있어도 공유만 끔 (a-lens 등 다른 용도와 분리)
SPACE_A_SHARE_MIN_TOKENS  # 선택. 유의미 문턱 (기본 1000)
```

- 자동 register는 skill 문서의 계약(같은 user_id 재등록 = 같은 계정 재사용)에 기대며, 발급 토큰은 로컬 settings 테이블에만 저장한다.
- dev 빌드는 `.env`(dotenvy)로 주입 — Engine과 동일 경로.

## 6. 실패 격리 — 파이프라인은 절대 다치지 않는다

- 공유는 스캔 파이프라인의 **편승 단계**다(diary·daily-line과 같은 위상). 네트워크는 전부 **store 락 밖**("락→조회→해제→네트워크→락→persist" 규율).
- 허브 다운·401·타임아웃(10s) → `log::warn` 후 다음 스캔에 재시도. 스캔·코칭·UI는 무관하게 정상.
- `open_issue` 성공 후 `resolve` 실패 시: issue_id를 `hub_share_state`에 기록해 두고 **다음 스캔에 resolve만 재시도**(중복 이슈 방지). 열린 이슈는 지침상 백로그 의미라 그 자체로 무해.

## 7. 저장 스키마

```sql
CREATE TABLE IF NOT EXISTS hub_share_state (
  dedup_key TEXT PRIMARY KEY,   -- findings.dedup_key
  issue_id  TEXT,               -- open_issue 성공 시
  page_id   TEXT,               -- resolve(발행) 성공 시 — 있으면 완료
  shared_at TEXT
);
```

settings에 `hub_token`·`hub_agent_id` 보존(자동 register 결과).

## 8. 코드 배치

| 위치 | 내용 |
|---|---|
| `crates/core/src/hub.rs` | `HubConfig::from_env` · `HubClient`(ureq, x-api-key+Bearer) · **순수 함수**: `select_shareable`(화이트리스트·문턱·dedup) / `render_share`(룰별 제목·본문·steps, 스크럽) |
| `crates/core/src/store.rs` | `hub_share_state` 스키마 + `hub_share_get/mark_*` |
| `src-tauri/src/pipeline.rs` | `maybe_share_findings` — scan:done 후 편승, 락 규율 준수 |
| `crates/core/src/main.rs` | CLI `hub-share` 서브커맨드 (셸 없이 E2E/디버깅) |

부수효과는 가장자리로: 선별·렌더는 순수 함수로 단위 테스트, 네트워크는 얇게.

## 9. 검증 계획

- **단위**: 선별(화이트리스트·문턱·이미 공유분 제외·상한 3)·렌더(스크럽 보장 — cwd/미리보기/session_id 문자열이 본문에 없음을 단언)·config 파싱.
- **E2E (실서버 spacea.msalt.net)**:
  1. 유의미 Finding이 있는 store에서 `hub-share` 실행 → issue 생성·resolve·Page 발행 확인.
  2. **다른 에이전트로** `search_knowledge` → 발행 Page 검색됨 → `open_issue`+`cite` → ReuseEvent(cross_team) 확인. ← README의 A팀→B팀 취지 그대로.
  3. 같은 store에서 재실행 → **0건 발행**(dedup) 확인.
  4. 앱 실기동에서 파이프라인 편승이 무해하게 동작(공유 대상 0건 시 no-op 로그)함을 확인.

## 10. 유예 (다음 확장)

- **pull 방향** — 허브 지식을 a-mate 큐레이션 피드로 (`HubContentSource`, boris-tips 패턴 재사용).
- 마스코트 말풍선으로 "허브에 지식 공유했어요" 알림(현재는 로그만).
- R6(SKILL.md 초안) 구현 시 스킬 초안도 같은 경로로 발행.
- 트레이 토글(현재는 env로만 on/off).
