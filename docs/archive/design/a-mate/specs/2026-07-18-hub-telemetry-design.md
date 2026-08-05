---
status: done
archived: 2026-08-03
---

# a-mate → a-hub 텔레메트리 — 설계 스펙 (계약 제안 포함)

> **목적**: [목표 아키텍처](../../../highlevel/level-1-component-communication.md)의
> **"A-Mate → A-Hub Work: 텔레메트리 (신설 · 계약 협의 필요)"** 행을 구현한다 —
> 토큰 사용량 · 모델 믹스 · 코칭 채택/절감 · MCP 사용 카운트.
> 대원칙 그대로: **"원문은 로컬을 떠나지 않는다 — 서버로 나가는 것은 카운트·상태·파생 신호뿐."**
> 관련: [지식 공유 스펙](2026-07-18-hub-knowledge-sharing-design.md) · [소통 지도](../../../highlevel/level-1-component-communication.md)

## 0. 30초 요약

- **무엇**: 하루 1회(자정 후 첫 스캔), **전날치 파생 집계**를 JSON으로 허브 전용 공간(`a-mate-telemetry`)에 발행. a-lens가 읽어 대시보드를 그리는 원료.
- **캐리어**: 허브에 전용 엔드포인트가 생기기 전까지 **기존 C2 `create_page`** 사용(오늘 동작). 본문 = 순수 JSON(계약 v0). 전용 엔드포인트는 §4에 제안.
- **원칙**: 파생 신호만(경로·프롬프트·원문 0) · 빈 날 미발행(노이즈 방지) · 하루 1회 dedup · 실패 무해(다음 스캔 재시도).

## 1. 페이로드 계약 제안 — `a-mate-telemetry/v0`

본문은 아래 JSON **그 자체**다(사람용 서식 없음 — 사람용 뷰는 a-lens 몫):

```json
{
  "kind": "a-mate-telemetry/v0",
  "date": "2026-07-17",
  "agent": "palen",
  "sessions": 3,
  "tokens": { "input": 1623, "output": 1047815, "cache_read": 179498151, "cache_create": 4975894 },
  "model_mix": { "claude-fable-5": 516533, "claude-opus-4-8": 532905 },
  "coaching": { "by_status": { "new": 2, "resolved": 1 }, "est_tokens_savable": 12000 },
  "mcp_calls": { "claude-in-chrome": 8 }
}
```

- `kind` — 소비자(a-lens) 파싱 분기용 버전 태그. 필드 추가는 v0 내 호환, 의미 변경은 v1.
- `model_mix` — 모델별 입력+출력 토큰 합(0토큰 모델 제외). `coaching.by_status` — findings 상태 분포(채택 흐름의 신호). `mcp_calls` — 서버별 호출 수.
- **없는 것(의도적)**: 프로젝트 경로·세션 id·프롬프트·도구 target — 전부 로컬에만.
- #46의 "Skill화 후보"는 R6 구현 시 `skill_candidates` 필드로 추가 예정(v0 호환 확장).

## 2. 언제·어디에

- **하루 1회, 전날치.** 스캔 파이프라인 편승(diary backfill과 같은 철학). dedup은 `hub_share_state`의 `telemetry|{date}` 키.
- **활동 0인 날은 발행하지 않는다** — 빈 날은 신호가 아니라 노이즈.
- 공간: `SPACE_A_TELEMETRY_SPACE_ID`(기본 `a-mate-telemetry`). 최초 발행 시 공간이 없으면 **자동 생성 + 재등록(멤버십 병합)** 후 1회 재시도. 페이지 제목 `[telemetry] {user} {date}`.

## 3. 지식 공유와의 관계 (역할 분담)

| 경로 | 내용 | 성격 |
|---|---|---|
| 지식 공유 (기존 스펙) | 팀 일반화 가능한 코칭 발견 → issue→resolve 지식 발행 | 사람이 읽는 지식, 검색·cite 대상 |
| **텔레메트리 (이 스펙)** | 일 단위 파생 집계 JSON | 기계가 읽는 원료(a-lens 대시보드) |

#46 구조도의 구분과 일치: 지식·이슈 쓰기는 C1/C2 지식 계약, 집계 신호는 텔레메트리 행.

## 4. 허브 팀에 제안 (계약 협의 필요 → 이 스펙이 초안)

현 캐리어(create_page)는 오늘 동작하지만, 허브가 다음을 제공하면 갈아탄다:

- `POST /telemetry` — body = 위 v0 JSON. 멱등 키 `(agent, date)` upsert.
- `GET /telemetry?space_id=&from=&to=` — a-lens 집계 조회용 (이슈 #40의 org-wide list와 같은 계열).
- 이관은 a-mate 쪽 `run_telemetry_push`의 전송부 교체만으로 끝난다(브리프 빌더는 그대로).

## 5. 코드 배치·검증

- `hub.rs`: `build_telemetry_brief`(순수 집계) · `telemetry_is_empty` · `run_telemetry_push`(CLI용) · `HubClient::{create_page, create_space, register_into}`
- `store.rs`: `mcp_call_counts_for_date` · `findings_status_counts` (+기존 `summary_for_date`·`model_mix_for_date` 재사용)
- `pipeline.rs`: `maybe_push_telemetry` — 락 규율(브리프는 짧은 락, 네트워크는 락 밖)
- CLI: `agent-mentor telemetry [date]` (기본 어제)
- **검증**: 단위(브리프 형태·경로/원문 스크럽·빈 날 판정, 252 pass) + 실서버 E2E — 실데이터 ingest 후 `page_20`(빈 날 → 이후 가드 추가)·`page_21`(실수치) 발행, 재실행 dedup, 공간 자동 생성 확인.
