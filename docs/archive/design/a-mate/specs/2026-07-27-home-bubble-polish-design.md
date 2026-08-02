---
status: done
archived: 2026-07-27
---

# 홈·말풍선 소품 일괄 (묶음 ①: P5 + H1 + H3) 설계

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate (프론트 + `crates/core`)
- **관계**: [로드맵](../plans/2026-07-26-life-social-diary-followups-roadmap.md) "묶음 실행 계획" ① — P5(말풍선 한국어 줄바꿈) + H1(오늘의 일기 박스 vs mood 정리) + H3(CHATTER 자유 생성). P5·H3가 `bubble.ts`를 공유해 한 브랜치·한 PR로 묶고, **항목당 커밋을 분리**한다.
- **베이스**: main 54b0982 (V1 스폰 분산 PR #114 머지 이후)

## 확정된 결정 (브레인스토밍 결과)

| 항목 | 결정 |
|---|---|
| P5 적용 범위 | 말풍선 3곳(LifeView·MiniLife·Mascot) + H1 박스 `.daily-line`. **max-width·line-clamp는 불변**(실화면 확인 후 필요 시 후속) |
| H1 방식 | 박스를 **클릭 불가 "오늘의 한마디" 카드**로 정정 + mood 제거. "더 보기" 링크 불필요(다이어리는 탭으로 이동) |
| H3 전략 | **LLM 우선 + 정적 폴백**: 풀이 있으면 풀에서만 pick, 풀 크기 5→12, 프롬프트에 주제 다양화 지시. MBTI·작업 강도 등 기존 특성 배선은 유지·명시 참조 |

## P5 — 말풍선 한국어 줄바꿈 (커밋 1)

### 증상·원인

사용자 말풍선(`lifeSetBubble`)·잡담 말풍선이 어절 중간에서 깨진다(예: "허락하/는"). CSS 기본 CJK 줄바꿈은 음절 단위 개행을 허용하고, `LifeView`는 `overflow-wrap: anywhere`가 이를 증폭한다. `word-break: keep-all`(어절 단위)이 어디에도 없다.

### 렌더 지점별 변경 (CSS만, 마크업 불변)

| 파일 | 셀렉터 | 내용물 | 현재 | 변경 |
|---|---|---|---|---|
| `src/lib/ui/LifeView.svelte` | `.agent-bubble` | **lifeSetBubble**(사용자 입력, 개행 가능) | `white-space:pre-wrap; overflow-wrap:anywhere` | `word-break:keep-all` 추가 |
| `src/lib/ui/MiniLife.svelte` | `.bubble` | 로컬 정적 잡담(hub 미연결 홈) | 줄바꿈 속성 없음 | `word-break:keep-all; overflow-wrap:anywhere` 추가 |
| `src/Mascot.svelte` | `.bubble .text` | 잡담/알림(데스크톱 마스코트) | 줄바꿈 속성 없음 | `word-break:keep-all; overflow-wrap:anywhere` 추가 |

- `keep-all` + `overflow-wrap:anywhere` 병용 시: 평소엔 공백(어절) 경계에서만 줄바꿈, 컨테이너 폭을 넘는 초장 토큰(URL 등)만 비상 개행 — 넘침 방지 유지.
- ⚠ LifeView/MiniLife는 `no-hardcoded-colors` ALLOWLIST(v1 아웃-스코프) — **색상은 건드리지 않는다**. 줄바꿈 속성만.
- 사전 조사 보정: MiniLife 말풍선은 lifeSetBubble이 아니라 자체 정적 잡담이다(`MiniLife.svelte:23`). lifeSetBubble 렌더는 LifeView 한 곳. 다만 줄바꿈 증상은 세 곳 공통이라 적용 범위엔 영향 없다.

## H1 — 홈 aside "오늘의 일기" 박스 vs mood 정리 (커밋 2)

### 현재 상태

- `App.svelte:174-178` "📔 오늘의 일기" **버튼** = `dailyLine`(LLM 오늘의 **한마디**) 렌더 — 라벨만 "일기", 클릭 시 diary 탭.
- `App.svelte:151-153, 182` `.mood` = `est_tokens_saved_total` 기반 하드코딩 2-state 문자열 — 사용자가 "오늘의 한마디"로 오인.
- 둘이 같은 자리를 경쟁 → **하나만 남긴다**.

### 변경 (`App.svelte`만)

1. 버튼 박스 → **클릭 불가 카드**로 교체: `<button class="diary" onclick=…>` → `<div class="daily">`. 라벨 `📔 오늘의 일기` → `💬 오늘의 한마디`. `더 보기 →`(`.more`)·`title`·hover 스타일 제거. 카드 시각 골격(`--panel2` 배경 + accent 왼선)은 유지.
2. `.mood` 파생값·마크업·CSS **제거**.
3. `.daily-line`의 `word-break: break-word` → `keep-all` (P5 결정 반영, `overflow-wrap: break-word`는 유지).
4. `{#if dailyLine && !visiting}` 게이트 유지 — `dailyLine`이 null이면(엔진 미설정 등) aside엔 초상만 남는다(허용).

## H3 — CHATTER 자유 생성 (커밋 3)

### 현재 구조

잡담 소스 둘 — (1) 서버 `compute_chatter_pool`(LLM, 오늘 사실 기반 5개, 스캔당 fp-게이트 캐시), (2) `bubble.ts` 정적 `CHATTER` 15개. `pickChatter`가 둘을 합쳐 균등 랜덤 → 정적이 75% 지배, "고정 목록" 체감의 원인.

### 변경

**프론트 `src/lib/robot/bubble.ts`:**

```
chatterCandidates(pool, summary, honorific)
  변경 전: [...pool, ...CHATTER.map(…)]   // 항상 혼합
  변경 후: pool.length ? [...pool] : CHATTER.map(…)   // LLM 우선, 정적은 폴백 전용
```

- 정적 `CHATTER` 15개는 **축소 없이 유지** — 오프라인/엔진 미설정/오늘 활동 0건(`session_count==0` → 빈 풀)의 폴백.
- `pickChatter`의 recent-3 제외·기아 방지 로직 불변.

**서버 `crates/core/src/mascot.rs`:**

- `CHATTER_POOL_SIZE` 5 → **12** (스캔당 LLM 1회 배치 생성은 그대로 — 비용 증가는 출력 토큰 소폭).
- `build_chatter_prompt`에 **주제 다양화 지시** 추가: 잡담끼리 주제·결이 겹치지 않게 — 관찰, 엉뚱한 궁금증, 마스코트 셀프 개그, 응원, 오늘 작업 강도에 대한 능청 등 여러 각도. "정밀도의 선"(사실·수치 날조 금지)은 유지.
- 기존 특성 배선은 그대로 살린다(사용자 요구): MBTI(`mbti_voice_hint`) · 작업 강도(`comic_directives`: 주말/연속세션/장시간) · 오늘 사실(`facts_block`). 다양화 지시가 이 축들을 명시적으로 참조하게 작성.

### 테스트

| 대상 | 내용 |
|---|---|
| `bubble.test.ts` | 기존 9건 통과 유지 확인(사전 검토상 로직 변경과 호환). 신규 1건: **풀이 있으면 정적 문구가 후보에서 빠진다** |
| `cargo test` | `CHATTER_POOL_SIZE` 변경 영향 확인 + 프롬프트에 다양화 지시 존재 단언 추가(`chatter_tests` 선례) |

## 검증선

- **P5·H1**: 시각 변경은 자동 테스트로 못 잡는다 — `npm run build` + `npm test`(no-hardcoded-colors 포함)가 회귀선. **실화면 스크린샷 확인은 PR 체크리스트로 사용자 진행**(hub 연결 필요): ① 방 화면 lifeSetBubble 줄바꿈 ② 홈 미니룸·데스크톱 마스코트 말풍선 ③ 홈 aside 한마디 카드.
- **H3**: `npm test`(bubble.test.ts) + `cargo test`(mascot.rs).

## 커밋·PR 구성

| 커밋 | scope | 파일 |
|---|---|---|
| 1. P5 | `fix(agent)` | LifeView.svelte · MiniLife.svelte · Mascot.svelte (CSS만) |
| 2. H1 | `feat(agent)` | App.svelte |
| 3. H3 | `feat(agent)` | bubble.ts · bubble.test.ts · mascot.rs |

- 브랜치 `feat/home-bubble-polish` (main 54b0982 기준 격리 워크트리), PR 1개.
- DoD: 머지 시 같은 PR에서 `docs-archive`(ADR 0013). 로드맵 완료 기록은 작업 마지막에 main 최신 반영 후 커밋(병렬 세션 충돌 회피).
- 병렬 주의: P3 세션이 `mascot.rs`를 건드릴 수 있음(P3=신규 함수 vs H3=`compute_chatter_pool`·프롬프트 수정) — 머지 가능 수준이나 P3 세션에 본 스펙 공유.

## 스코프 밖 (후속 메모)

- **MiniLife 로컬 정적 CHATTER 5개**(`MiniLife.svelte:23`): hub 미연결 홈의 자체 잡담 — H3 로드맵 스코프(bubble.ts) 밖. LLM 풀 혼합은 후속 판단.
- **말풍선 max-width·line-clamp 조정**: keep-all 적용 후 실화면에서 어색하면 후속으로.
