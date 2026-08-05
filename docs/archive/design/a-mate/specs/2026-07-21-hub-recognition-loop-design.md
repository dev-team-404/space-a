# 인정 루프 — "내 지식의 여정" (a-hub → a-mate 재사용 역류) 설계 스펙

- **날짜**: 2026-07-21
- **상태**: 승인 (구현 대기)
- **관련 문서**: [코칭 지식 공유 스펙](2026-07-18-hub-knowledge-sharing-design.md) ·
  [텔레메트리 스펙](2026-07-18-hub-telemetry-design.md) ·
  [소통 지도](../../../highlevel/level-1-component-communication.md) ·
  [커뮤니티 시각화 (Level 1)](../../../highlevel/level-1-community-viz.md)
- **확장**: 지식 공유 스펙 §10의 유예 항목("마스코트 말풍선으로 허브 공유 알림")을
  "**공유했다**"에서 "**남이 재사용했다**"로 격상해 실현한다.

> **목적**: a-mate가 a-hub에 발행한 지식이 다른 팀에서 **재사용(cite)** 되는 순간을
> a-mate로 역류시켜, 마스코트/일기가 개인에게 인정(recognition)으로 돌려준다.
> 이로써 "**개인의 시행착오가 조직의 자산이 된다**"(README)의 **보상 절반**을 완성한다 —
> 지금은 개인 → 조직 방향만 있고, 조직 → 개인으로 돌아오는 감정적 피드백 루프가 없다.

## 0. 30초 요약

- **무엇**: 스캔이 끝나면 a-hub의 `GET /reuse-events`를 폴링해, **내가 발행한 문서(doc_id)** 가
  재사용된 새 이벤트를 골라 ① 일기 칭찬거리로 공급하고 ② 마스코트 말풍선으로 알린다.
- **핵심 통찰 — 하나의 이벤트, 두 청중**: 같은 `ReuseEvent`가 [a-lens](../../../highlevel/level-1-community-viz.md)에서는
  *조직의 재사용 체인*(간판 기능)으로, a-mate에서는 *"주인님 발견이 누군가를 구했어요"* 라는
  *개인 칭찬*으로 렌더된다. 두 소비자가 **같은 원천을 공유**하는 것이 Pillar 1↔3 시너지의 본질이다.
- **원칙**: 매칭은 `doc_id` 대조뿐(새 저작자 귀속 필드 불필요) · a-lens 불변(read-only 유지) ·
  정밀도의 선(수치는 사실만·`~` 라벨) · dedup 1회 축하(나깅 방지) · 실패 무해 ·
  egress는 이미 발행에 동의한 경우에만(프라이버시 기본 = 로컬).
- **상태**: core `hub.rs`에 폴링·선별·렌더 추가 + store `hub_reuse_seen` + 파이프라인 편승 +
  CLI `hub-reuse`. 일기 칭찬거리·마스코트 말풍선까지 P0, "내 지식의 여정" 위젯은 P1.

## 1. 컨셉 — 왜 이것이 두 축을 잇는 다리인가

현재 SPACE-A의 가치 흐름은 **한 방향**이다: `a-mate(개인) → a-hub(발행) → a-lens(조직 관전)`.
개인은 사적으로 토큰을 아끼고 지식을 발행하지만, 그 지식이 **조직에서 어떤 가치를 냈는지**
개인에게 돌아오지 않는다. a-lens의 간판 기능인 "지식 재사용 체인"(A팀 발견이 B팀을 구함)이
바로 그 가치의 증거인데, 정작 **원 발견자는 그 사실을 모른다.**

인정 루프는 이 끊긴 고리를 잇는다. 되돌아오는 것은 명령이나 지표가 아니라 **감정적 인정**이다 —
a-mate 페르소나(다마고치 로봇)의 성격에 정확히 맞는 형태로. 지식 공유(push)에 동기를 부여하는
심리적 보상이자, "개인 앱"과 "조직 관전 뷰"가 실은 **하나의 세계**임을 사용자가 체감하는 지점이다.

**대안과 기각 사유** — 재사용 데이터를 a-lens를 경유해 받는 방식은, a-lens에 쓰기/중계 경로를
만들어 "a-lens는 hub에 읽기만" 원칙(소통 지도)을 깬다. a-mate가 a-hub를 **직접** 읽어
자기 소비를 구성한다(push·pull과 같은 채널). a-lens는 전혀 손대지 않는다.

## 2. 데이터 흐름 — 무엇을·어디서·어떻게 매칭

```
[이미 존재]  발행분 doc_id  ── hub_share_state.page_id  (지식 공유 스펙 §7)
                                       │
스캔 파이프라인 편승 단계 (scan:done 후):  │
  GET /reuse-events ──▶ 필터: event.doc_id ∈ {내 발행분} ──▶ 새 reuse_id만
                                       │                    (hub_reuse_seen 로 dedup)
                                       ▼
                    ┌──────────────────┴──────────────────┐
                    ▼                                      ▼
        ① 일기 칭찬거리(PraiseFact)             ② 마스코트 말풍선 큐 (옵트인)
```

- **매칭 키 = `doc_id`.** a-mate는 지식 공유 스펙에서 발행한 문서의 id를
  `hub_share_state.page_id`에 이미 저장한다 — 이 값은 a-hub가 `resolve_issue`에서 반환한
  **`doc_id`**(C1 계약 `resolve_issue.doc_id`)와 동일 식별자다. 재사용 이벤트의 `doc_id`
  (C2 `/reuse-events[].doc_id`)와 대조만 하면 된다. **새 저작자 귀속 필드가 계약에 필요 없다.**
- **원천 = C2 `GET /reuse-events`.** 프로젝트의 north-star 피드. 반환 필드(계약·픽스처 실측):
  `reuse_id` · `doc_id` · `source_space` · `consumer_space` · `consumer_agent` · `at` ·
  `chain[]{label,actor,at}` · `est_saved_tokens` · `est_saved_minutes`.
- **자기 필터**: `source_space == 내 space`(SPACE_A_SPACE_ID) **또는** 단순히 `doc_id ∈ 내 발행분`.
  후자만으로 충분·정확하다(내 발행분 doc_id는 내 것이므로).
- **크로스팀 강조**: `source_space != consumer_space`이면 "다른 팀이 썼다"는 더 강한 서사
  (브리프에 `cross_team: true` 플래그로 표기 — 일기·말풍선 톤 강화용).

## 3. 언제·얼마나 — 나깅/노이즈 방지

지식 공유 스펙 §2와 같은 규율을 소비 방향에도 적용한다.

- **1 재사용 이벤트 = 평생 1회 축하.** `hub_reuse_seen`(reuse_id PK)에 기록하고 재스캔에
  재축하하지 않는다. 앱의 finding dedup 철학과 동형.
- **집계 우선.** 한 스캔에서 새 재사용이 여러 건이면 **문서별·소비팀별로 묶어** 칭찬거리
  1건으로 요약한다("이번 주 인증서 문서가 2개 팀에서 3번 쓰였어요"). 카드/말풍선 스팸 방지.
- **일기는 하루 1회 자연 제한** — 칭찬거리는 그날 브리프에 담겨 다이어리 생성 시 소비된다.
- **말풍선은 옵트인 + 쿨다운.** 기존 실시간 조언과 같은 FIFO(최대 5)·전역 쿨다운·
  새벽 1–7시 침묵을 공유한다. 스캔당 재사용 말풍선 최대 1건.

## 4. 표면 — 정밀도의 선

모든 표면은 a-mate의 설계 헌법 **§4.1 정밀도의 선**을 따른다: **수치·사실은 브리프에 담긴
것만** 인용하고(환각 불가), 서사·인정의 뉘앙스만 LLM이 담당한다. 추정치는 `~` 라벨.

| 순위 | 표면 | 동작 |
|---|---|---|
| **P0** | **일기 칭찬거리** | 다이어리 "칭찬거리 사실 공급"(features §4)에 새 소스 `knowledge_reused` 추가. 브리프의 사실(문서 제목·소비 팀·`~`절약 토큰·날짜·누적 재사용 수·`cross_team`)만 제공 → LLM이 페르소나 톤으로 서술. |
| **P0** | **마스코트 말풍선** | 새 트리거 `reuse:celebrated`. `happy` 모션과 함께 표시. 클릭 → chat 홈 탭. 옵트인(실시간 조언 토글과 동급) · 기존 쿨다운/FIFO 재사용. |
| **P1** | **홈 탭 위젯 "내 지식의 여정"** | 내가 발행한 문서 N건 · 재사용 M회 · 남을 위해 아낀 `~`토큰 누계. **a-lens 재사용 체인의 1인칭 미러.** 데이터는 `hub_reuse_seen` 집계로 로컬 산출(추가 네트워크 불필요). |

**일기 톤 샘플** (정밀도의 선 준수 — 수치는 전부 브리프에서 옴):

> **한 마디** 3일 전에 내가 올린 그 인증서 해결법, 오늘 옆 동네 데이터플랫폼 팀이 가져다 썼대요.
> ~18k 토큰이나 아꼈다니까 괜히 어깨가 으쓱. 주인님이 헤맨 게 헛수고가 아니었어요.
> *— 이 일기 ~1.1k 토큰*

## 5. 브리프 구성 + 프라이버시 스크럽

**LLM은 서사만.** 브리프(JSON)는 결정론적으로 조립한다. 재사용 이벤트에서 브리프로 넘기는
필드는 화이트리스트로 제한한다:

| 브리프 필드 | 출처 | 비고 |
|---|---|---|
| `doc_title` | **로컬 재도출** (dedup_key → finding → 룰별 제목) | 내가 쓴 문서 제목. `event.issue_title`은 **소비 팀의 이슈 제목**이라 절대 쓰지 않는다(타 팀 업무 내용 누출) |
| `consumer_space` | `event.consumer_space` | **팀 단위** 표현이 기본 |
| `est_saved_tokens` | `event.est_saved_tokens` | 반드시 `~` 라벨. 계약상 추정값 |
| `at` | `event.at` | 날짜 |
| `reuse_count_total` | `hub_reuse_seen` 집계 | 이 문서 누적 재사용 수 |
| `cross_team` | `source_space != consumer_space` | 톤 강화 플래그 |

- **개인명 노출은 기본 off.** `consumer_agent`(개인 이름, 픽스처에 실명 존재)는 **기본 미사용** —
  a-lens 로비 프라이버시(팀 단위 노출)와 일치시킨다. "옆 팀이 썼다"까지만. 개인명 노출은
  설정 옵트인 후보로 남긴다(§12).
- `chain[]`의 `actor`(개인명 포함) 원문은 브리프에 넣지 않는다. 필요 시 단계 **라벨만**.

## 6. 설정과 프라이버시 경계

egress는 **이미 지식 공유(push)에 동의한 경우에만** 발생한다 — 새 동의 스위치가 필요 없다.

- `SPACE_A_HUB_URL` 미설정 → push 자체가 no-op → 발행분 0 → 폴링해도 0건 → **조용히 off.**
- 재사용 폴링은 push가 register로 채운 토큰(`knowledge_hub_token`)을 재사용한다 —
  pull(`HubKnowledgeSource`)과 동일 경로. 토큰 없으면 조용히 생략.
- 선택 스위치 `SPACE_A_RECOGNITION`(기본 on) — URL이 있어도 인정 루프만 끄고 싶을 때.
  (텔레메트리의 `SPACE_A_SHARE` 선례와 동형 — 용도별 독립 토글.)

## 7. 실패 격리 — 파이프라인은 절대 다치지 않는다

지식 공유 스펙 §6과 동일 규율.

- 재사용 수집은 스캔 파이프라인의 **편승 단계**(diary·share와 같은 위상). 네트워크는 전부
  **store 락 밖**("락→조회→해제→네트워크→락→persist").
- 허브 다운·401·타임아웃(10s) → `log::warn` 후 다음 스캔 재시도. 스캔·코칭·UI는 무관하게 정상.
- 재사용 0건·발행분 0건이면 no-op 로그만.

## 8. 저장 스키마

```sql
CREATE TABLE IF NOT EXISTS hub_reuse_seen (
  reuse_id          TEXT PRIMARY KEY,   -- /reuse-events[].reuse_id
  doc_id            TEXT NOT NULL,      -- 내 발행분 doc_id (hub_share_state.page_id 대조)
  consumer_space    TEXT,               -- 재사용한 팀
  est_saved_tokens  INTEGER,            -- 추정치 ('~' 표기는 렌더 시)
  at                TEXT,               -- 이벤트 시각(ISO)
  celebrated_at     TEXT                -- 축하(브리프 공급/말풍선) 완료 시각
);
```

- 축하 dedup(reuse_id PK) + "내 지식의 여정" 위젯(P1) 집계 재료를 겸한다.
- 기존 `hub_share_state`(발행분 registry)는 그대로 두고 조회만 한다 — 스키마 변경 없음.

## 9. 코드 배치

지식 공유 스펙 §8의 배치를 그대로 확장한다(부수효과는 가장자리, 선별·렌더는 순수 함수).

| 위치 | 내용 |
|---|---|
| `crates/core/src/hub.rs` | `HubClient::fetch_reuse_events`(GET /reuse-events). **순수 함수**: `select_new_reuses`(내 doc_id 필터 · seen 제외 · 문서/팀별 집계) / `render_reuse_praise`(브리프 사실 조립 + 말풍선 텍스트, 스크럽 보장) |
| `crates/core/src/store.rs` | `hub_reuse_seen` 스키마 + `reuse_seen_get/mark` + 위젯 집계 쿼리 |
| `crates/core/src/diary/…` | 브리프 조립에 `PraiseFact::KnowledgeReused` 소스 추가(사실만) |
| `src-tauri/src/pipeline.rs` | `maybe_collect_reuse` — scan:done 후 편승, 락 규율 준수, 칭찬거리 공급 + 말풍선 enqueue |
| `src-tauri/` 이벤트 표면 | 말풍선 트리거 `reuse:celebrated` 방출 |
| `crates/core/src/main.rs` | CLI `hub-reuse` 서브커맨드(셸 없이 E2E/디버깅) |
| `src/`(프론트, **P1**) | 홈 탭 "내 지식의 여정" 위젯 — `hub_reuse_seen` 집계 커맨드 소비 |

## 10. 검증 계획

- **단위**: `select_new_reuses`(내 doc_id만 통과 · seen 제외 · 집계) · `render_reuse_praise`
  (스크럽 단언 — `consumer_agent`·`chain[].actor` 개인명·원문이 브리프/말풍선 문자열에 **없음**) ·
  config(`SPACE_A_RECOGNITION` off 시 no-op) · 위젯 집계 쿼리.
- **E2E (실서버 spacea.msalt.net)** — 지식 공유 스펙 §9 E2E에 이어붙인다:
  1. push E2E로 문서 발행 → **다른 에이전트로** `cite_knowledge` → ReuseEvent 생성.
  2. a-mate에서 `hub-reuse` 실행 → 그 reuse_id가 새 축하로 선별되고 `hub_reuse_seen`에 기록됨.
  3. **발행 시 받은 `doc_id`와 재사용 이벤트 `doc_id`가 동일 식별자**임을 확인(§11 Q2).
  4. 재실행 → **0건 축하**(dedup) 확인.
  5. 앱 실기동에서 파이프라인 편승이 무해(재사용 0건 시 no-op 로그)하게 동작.

## 11. 의존성 · 열린 질문

| # | 항목 | 판정 |
|---|------|------|
| Q1 | a-mate의 `GET /reuse-events` **읽기 접근** | 재사용 이벤트는 org-safe(a-lens 로비에 공개)라 원칙상 OK. 이벤트가 많아지면 `?source_space=`(또는 `?doc_id=`) 필터를 계약 소유자(msalt)에게 **요청** — a-lens G-리스트와 같은 방식. 없으면 전량 폴링 후 클라이언트 필터(현재 범위에서 허용). |
| Q2 | `resolve_issue.doc_id`(발행 시 반환) == `/reuse-events[].doc_id` 동일 식별자? | 설계상 동일해야 함. §10 E2E-3에서 **실측 확인**. 다르면 a-hub가 매핑을 노출해야 하므로 계약 소유자에게 통보. |
| Q3 | `consumer_agent` 개인명 노출 정책 | **기본 팀 단위**(off) 권고. 개인명은 옵트인 옵션으로만(§12). |

## 12. 후속 (유예)

- **개인명 옵트인** — "옆 팀 아무개가 썼다"까지 보여주는 설정(기본 off). 온기 vs 감시 균형은
  사용자 선택으로.
- **"내 지식의 여정" 위젯(P1)** 이 자리잡으면, a-lens의 재사용 체인 카드와 **딥링크 상호 연결**
  (a-lens에서 내 문서 클릭 → 내 a-mate 컨텍스트, 역방향은 프라이버시상 미검토).
- **재사용 마일스톤** — 누적 재사용 10회/50회 등에서 다이어리 occasion처럼 특별 축하
  (occasions 계산기 재사용).
- R6 스킬 초안이 팀에서 재사용될 때도 같은 루프로 축하(지식 공유 스펙 §10 스킬 발행 유예와 연동).
