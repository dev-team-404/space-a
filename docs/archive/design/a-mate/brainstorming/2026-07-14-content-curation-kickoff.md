# 콘텐츠 큐레이션 킥오프 — "공식 커리큘럼을 사다리로, 개인 로그를 GPS로"

> **이 문서의 목적**: 외부 지식(공식 가이드·팁·사내 행사)을 시의적절하게 코칭에 녹이는
> 새 축의 시드 문서. 새 세션에서 브레인스토밍 → 스펙(`docs/specs/`) → 플랜(`docs/plans/`) → SDD로 진행한다.
> **아직 코드 짜지 말 것.**
> 검증된 커리큘럼 데이터: [`2026-07-14-curriculum-catalog.json`](./2026-07-14-curriculum-catalog.json)
> (2026-07-14 공식 페이지 직접 fetch — 서드파티 요약 배제).

---

## 시작 프롬프트 (새 세션에 그대로 붙여넣기)

```
콘텐츠 큐레이션 시작. docs/brainstroming/2026-07-14-content-curation-kickoff.md 읽고 브레인스토밍부터.
관례: 스펙은 docs/specs/, 플랜은 docs/plans/, 브레인스토밍 중엔 코드 금지.
사다리 레벨 감지 신호의 임계값, 노출 예산, 사내 API 연동 범위는 질문으로 구체화할 것.
동봉 데이터: docs/brainstroming/2026-07-14-curriculum-catalog.json (검증된 공식 강좌 syllabus).
```

---

## Why — 전사 AX 뿌리내리기: 모두를 AI 코딩 마스터로

### 배경 문제 3겹

1. **개인 차원 — "뻔한 조언" 한계** (main 저장소 `docs/highlevel/level-1-ai-mentor.md`의 명시된 열린 질문).
   현행 코칭(R1~R12)은 낭비 *탐지*는 잘하지만, 탐지 기반 코칭만으로는
   "MCP 정리하세요" 수준을 넘기 어렵다. 사용자가 **다음에 뭘 배워야 하는지**의 방향 제시가 없다.
2. **조직 차원 — AX 역량의 개인차가 방치됨.** 에이전트를 잘 쓰는 사람과 못 쓰는 사람의 격차가
   벌어지는데, 이를 좁히는 체계(커리큘럼·코칭)가 없다. 각자 시행착오를 반복한다.
3. **정보 차원 — 공식 가이드·신기능·행사 정보가 흘러가 버림.** Anthropic/OpenAI 공식 학습 자료,
   릴리스 소식, 사내 AX 행사가 존재하지만 개발자의 작업 맥락과 **연결되는 순간**이 없어 소비되지 않는다.

### 목표 (궁극)

> **회사 전체에 AX가 자연스럽게 뿌리내리고, 모든 구성원이 AI 코딩 마스터가 되는 것.**

- "자연스럽게" — 별도 교육 이벤트나 강제 수강이 아니라, **매일 쓰는 도구가 각자의 작업 맥락에서
  다음 배울 것을 짚어주는** 방식으로 역량이 스며든다. 코칭이 곧 학습이다.
- "모든 구성원" — 잘 쓰는 사람은 더 깊이(오케스트레이션까지), 이제 시작한 사람은 기초부터,
  비개발 직군은 AX 상식(4D)까지. 한 사다리 위에서 각자 다음 칸으로.

기존 코칭이 "낭비를 줄이는" 수비형이라면, 이 축은 "역량을 올리는" **공격형 코칭**이다.
멘토 정체성(제품명!)의 본령 — 텔레마코스를 *가르친* 멘토르.

### 왜 우리가 만들지 않고 Anthropic·OpenAI 커리큘럼을 채용하나

- **공신력.** "AI 잘 쓰는 법"을 우리가 정의하면 근거가 약하고 사내 정치에 휘둘린다.
  모델을 만든 당사자(Anthropic)와 업계 표준(OpenAI)의 **공식 커리큘럼**을 준거로 삼으면,
  코칭의 모든 조언이 "우리 생각"이 아니라 "공식 권장"이 되어 신뢰가 선다.
- **검증된 승급 구조.** 두 회사 커리큘럼이 **동일한 사다리**를 그린다
  (기본기 → 컨텍스트/워크플로 → 재사용(스킬) → 오케스트레이션). 우리가 임의로 짠 순서가 아니라
  업계가 합의한 역량 사다리 — 실제로 Claude Code 101의 모듈 순서가 이 사다리와 일치한다(§What 검증).
- **유지보수 위임.** 커리큘럼이 갱신되면(신규 강좌·개정) 우리는 링크만 따라가면 된다.
  콘텐츠 원본을 우리가 소유·갱신하지 않는다 (번들엔 뼈대·URL만, §How 참고).

## What — 공식 커리큘럼 트랙 기반 점진적 역량 개선

### 핵심 설계: 커리큘럼을 진도표가 아니라 지도로 쓴다

달력 기반 진도(Day 1 모델, Day 7 스킬…)는 "조치 가능한 것만 코칭" 원칙과 충돌하고
결국 뻔한 조언으로 회귀한다. 대신:

> **공식 커리큘럼 = 순서와 콘텐츠 (지도). 사용자 로그 = 현재 위치 (GPS).**
> 멘토는 로그에서 현재 레벨을 감지하고, **바로 다음 단계(frontier)의 팁만** 서빙한다.

- 이미 마스터한 단계 → 침묵. 너무 먼 단계 → 보류. **손 뻗으면 닿는 것만** 코칭 (근접발달영역).
- 승급은 시간이 아니라 **행동 변화로 감지**하고, 감지되면 잔소리가 아닌 **칭찬**으로 전환
  (기존 `following` 전이·"새 무기 감지" 패턴 재사용).

### 역량 사다리 5레벨 (감지 신호는 전부 기존 수집 데이터)

| Lv | 주제 | 현재 위치 감지 (store에 이미 있음) | 승급 감지 | 공식 콘텐츠 (검증 ✅) |
|---|---|---|---|---|
| 0 | 모델 리터러시 | 모델 믹스 100% 상위티어, 잔심부름 비율(R7 재료) | 티어 혼용 시작 | Claude Code 101 M1 (에이전틱 루프·권한) |
| 1 | 컨텍스트 위생 | R1·R2, CLAUDE.md 부재, 캐시 히트율 | MCP 정리 실행 or CLAUDE.md 생성 | 101 M3 (Explore→Plan→Code→Commit, 컨텍스트 관리) |
| 2 | 반복 제거→스킬 | R12, 반복 워크플로(R6 재료) | **첫 스킬 생성** (인벤토리 diff) | Intro to Agent Skills ("Stop repeating yourself, teach Claude once") |
| 3 | 워크플로 자동화 | R11, R10, hooks 부재 | 권한 사전 허용 or 첫 hook | Claude Code in Action M3–M4 (커스텀 커맨드·hooks·GitHub·SDK) |
| 4 | 오케스트레이션 | `is_sidechain` 사용률, 동시 세션 | 서브에이전트 정기 활용 | Intro to Subagents (위임 판단·컨텍스트 격리) |

- 검증 시 발견: **Claude Code 101의 모듈 순서 자체가 이 사다리와 일치**
  (루프→워크플로→CLAUDE.md→skills→MCP→hooks) — 순서가 우리 임의가 아니라 공식 커리큘럼 준거.
- **AX 상식 트랙** (레벨 무관, 비개발 확산용): Anthropic **AI Fluency 4D 프레임워크**
  (Delegation·Description·Discernment·Diligence, 공식) + Cowork + OpenAI Academy 3코스
  (검증 결과 OpenAI 코스는 전부 직장인 일반용 — 코딩 사다리가 아니라 이 트랙에 배치).
- MCP 강좌 2종은 레벨 배치 대신 **태그 게이트** (MCP 관련 finding 있는 사용자에게만).

### 정보 소스 3티어

| 티어 | 소스 | 성격 | 전략 |
|---|---|---|---|
| T1 공식 피드 | Claude Code changelog·Anthropic 뉴스·강좌 신설 | 변하는 사실 | pull-only fetch, 일 1회 |
| T2 사내 | 사내 스킬허브·AX 행사/교육 캘린더 (API 확보됨) | 시의성 높음 | pull-only fetch |
| T3 내장 | 사다리×팁 카탈로그 (커리큘럼 뼈대 + 우리말 증류 팁 30~50개) | 거의 불변 | **앱에 번들** — 네트워크 0에서도 동작 |

**번들/피드 분리 규칙**: 팁 본문에 모델명·가격·강좌 개수 등 **변하는 사실 하드코딩 금지**
(실증: 서드파티 블로그 "13개 강좌"가 몇 달 만에 실제 20개로 낡음). 원리는 번들, 사실은 피드.
저작권: 강좌 원문 미수록 — 메타데이터·URL·우리말 증류 팁만 (Academy 강좌 링크로 유도).
번들 선례: `occasions.rs` 음력 정적 테이블, `curation.rs` BuiltinCurationSource.

## How — 기존 아키텍처에 얹기 (새 개념 최소화)

```
외부 소스(T1/T2) ──┐
                    ├─ ContentSource trait ─→ ContentItem 정규화 ─→ SQLite(content_items)
내장 카탈로그(T3) ──┘                                                      │
                                                                          ▼
   CompetencyProfile ←─ detect_level(store)        score(item, profile) — frontier 부스트
   (순수 함수, session_stats.rs 패턴)                (결정론 태그 매칭 — 정밀도의 선)
                                                                          │
                                              노출 예산 게이트 (나깅 방지 재사용)
                                                                          ▼
                                    말풍선(일 최대 1) · 홈탭 팁(상시 1) · 알림(일 ≤3) · 다이어리 다이제스트(주 1)
```

- **파이프라인 편승**: 새 스레드 없음. 스캔 완료 후 락 밖 단계에 `maybe_fetch_content()`
  (daily_line처럼 날짜 fingerprint 게이트, 일 1회).
- **정밀도의 선 준수**: 레벨 감지·스코어링·승급 판정 = 결정론. LLM은 영문 소식의
  한국어 마스코트 보이스 1줄 요약만 (`Engine` + `voice_guidance()` 재사용, 피드 사실만 인용).
- **나깅 방지**: `content_items`에 `new→shown→dismissed` 상태. dismiss된 태그는 쿨다운
  (예: MCP 팁 2회 닫으면 해당 태그 2주 침묵).
- **승급 = 서사 이벤트**: 승급 감지 → 칭찬 말풍선 + 다이어리 문단
  ("주인이 드디어 첫 스킬을 만들었다!"). 다마고치 성장 서사와 합치.
- **프라이버시**: pull-only (로컬 데이터 egress 0). 트레이에 "소식 받아오기 ✓" 토글.

### 예상 터치포인트

```
crates/core/src/content.rs      [신규] ContentSource trait + ContentItem + BuiltinTipsSource(T3)
                                       + FeedSource(T1/T2, ureq) + score() 순수 함수
crates/core/src/profile.rs      [신규] detect_level(store) → CompetencyProfile (순수 함수)
crates/core/src/store.rs        [수정] content_items 테이블 + 상태 upsert
src-tauri/src/pipeline.rs       [수정] maybe_fetch_content() 편승 (날짜 게이트)
src-tauri/src/commands.rs       [수정] list_content / set_content_status (+ *_inner 테스트)
src/lib/api.ts                  [수정] invoke 래퍼 + content:tip 이벤트
src/lib/ui/home/TipCard.svelte  [신규] 홈탭 "오늘의 팁" 위젯
```

### MVP 단계 (해커톤)

1. **Day 1**: T3 내장 카탈로그(팁 20개+) + `detect_level` + `score()` — **네트워크 0에서 데모 성립**
2. **Day 2**: T1 changelog fetch + 파이프라인 편승 + T2 사내 API 연동
3. **Day 3**: TipCard UI + 말풍선 트리거 + 승급 칭찬 서사 + (여유 시) LLM 한국어 요약

## 브레인스토밍에서 결정할 것

- **레벨 감지 임계값** — "Lv1 졸업"의 정확한 판정선 (캐시율 몇 %? CLAUDE.md 존재만으로 충분?).
  다차원(각 dimension 독립 레벨) vs 단일 레벨 스칼라.
- **노출 예산 확정** — 말풍선 일 1건이 기존 실시간 조언(FIFO 5)과 경합할 때 우선순위.
- **T2 사내 API 연동 범위** — 스킬허브 검색/행사 캘린더 중 해커톤 범위, 인증 방식.
- **content_items vs findings 테이블 재사용** — 팁도 Finding으로 취급해 상태머신을 공짜로 얻을지,
  별도 테이블로 갈지 (est_tokens_saved 정렬과 충돌 여부).
- **승급 칭찬의 발화 채널** — occasion 재사용 vs 신규 이벤트.
- **팁 저작 규모** — 레벨당 몇 개면 "매일 새로움"이 유지되나 (dismiss 소진 속도 추정).

## 참고

- 검증 데이터: [`2026-07-14-curriculum-catalog.json`](./2026-07-14-curriculum-catalog.json) —
  Anthropic Academy 20강좌 중 사다리 핵심 7강좌 syllabus + OpenAI 3코스 (공식 페이지 직접 fetch).
- 기존 확장 시드: `curation.rs::SkillRecommendationSource` (이 축이 그 일반화).
- 재사용 패턴: fingerprint 게이트(`mascot.rs` daily_line), 상태머신(`findings.status`),
  `following` 전이·새 스킬 감지(다이어리 칭찬거리), 정적 번들(`occasions.rs`).
- 관례: 브레인스토밍 → 스펙 `docs/specs/` → 플랜 `docs/plans/` → SDD → PR → 사용자 E2E.
