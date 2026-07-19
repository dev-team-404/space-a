---
status: done
archived: 2026-07-19
---

# 다이어리 다양성 + UI 픽스 4건 설계 스펙

- 작성: 2026-07-10 (브레인스토밍 산출물)
- 배경: 재생성 E2E(PR #19 가독성·#21 R5 보류 머지 후) 사용자 육안 피드백 4건 —
  ① 일기가 매일 같은 내용(전부 context7 빼라는 얘기), 이전 며칠 일기를 참조하면 좋겠다,
  ② 이모지가 없다(넣기로 했는데), ③ 배경(파스텔 꾸미기)이 안 보인다,
  ④ 홈 "절약 실천 top3"가 길어지면 가로 스크롤 + 위 주간 사용량이 늘어진다.
- 선행: 킥오프·진단 문서 `docs/brainstorming/2026-07-10-diary-variety-ui-fixes-kickoff.md`
- 다음 단계: writing-plans → **인라인 실행**(소규모 backend+front) → push+PR(base=main) → 앱DB 재생성 육안

## 1. 진단 (조사 완료 — 재조사 불필요)

1. **일기 반복**: R5 발화 보류(PR #21) 후 `findings`가 R1/R2(host 스코프, context7 등)뿐 —
   host 스코프 finding은 활성인 동안 `findings_for_date`에 매일 포함 → 매일 같은 코칭 서사.
   350자 단축(PR #19)으로 체감 심화. **브리프에 어제와의 차이를 만들 컨텍스트가 없음.**
2. **이모지 0개**: 프롬프트 "문단당 0~1개"가 0개를 허용하는 소극 지시 + `voice_guidance` ⑥
   "이모지 남발 금지"가 억제로 이김 → 실제 산출물은 0개.
3. **배경 안 보임**: 카드 CSS는 정상. 문제는 `DiaryTab` 본문 카드(`article`)가 `--frame-bg`(#fffdfa)인데
   그 프레임 배경도 `--frame-bg` — 흰 배경 위 흰 카드라 시각 구분 불가.
4. **홈 그리드**: `HomeTab` `.grid { 1fr 1fr }`인데 grid 아이템 기본 `min-width:auto` 때문에
   `SaveTop3`의 긴 `suggested_action`(`.action`은 `white-space:nowrap`)이 컬럼을 밀어냄
   → `.action`의 기존 `text-overflow:ellipsis`(SaveTop3.svelte:39)가 무력화, 같은 컬럼 `WeekTrend`
   늘어짐, 전체 가로 스크롤.

## 2. 변경 상세

### 2a. `Brief.recent_diaries` — `crates/core/src/diary/mod.rs`

브리프에 직전 며칠간의 내 일기 발췌를 실어, LLM이 "어제와 다른 이야기"를 쓰도록 컨텍스트를 준다.

- 신규 타입 `RecentDiary { date: String, excerpt: String }`, `Brief`에 `recent_diaries: Vec<RecentDiary>` 추가.
- `assemble_brief`에서 직전 3일(`RECENT_DIARY_LOOKBACK=3`)을 **오래된 것부터** 순회하며
  `store.diary_path_for(date)`로 vault 경로를 얻어 파일을 읽고, 발췌를 500자(`char` 경계 안전)로 캡.
- 발췌는 토큰 푸터(`render_diary`가 붙이는 `*— 이 일기 ~N 토큰 …*`)를 제외 —
  이 라인이 예시로 들어가면 LLM이 흉내내 이중 푸터가 생길 수 있어 제거(작은 정제).
- 파일 없음·읽기 실패는 **조용히 스킵**(브리프 조립을 막지 않음). 해당 날짜 일기가 없으면 항목 없음.
- 순서 정합: startup backfill이 `missing_diary_dates`로 **오래된 날짜부터** 재생성하므로,
  오늘 일기를 만들 때 직전 날짜 일기는 이미 존재(재생성 세션 내). vault 로컬 파일→온프렘 엔진이라 **프라이버시 경계 불변**.

### 2b. 프롬프트 지시 — `build_system_prompt`만 (같은 파일)

occasions 문단과 형식 문단 사이에 `recent_diaries` 지시 문단 추가:

```
브리프의 `recent_diaries`는 직전 며칠간 내가 쓴 일기입니다. \
거기서 이미 다룬 지적·화제는 되풀이하지 말고(꼭 필요하면 한 줄로만 스치듯), \
오늘 브리프의 오늘만의 사실과 기분에 집중해 어제와는 다른 이야기로 쓰세요. \
비어있으면 신경 쓰지 마세요.
```

이모지 지시(형식 문단 마지막 줄) 상향 — "문단당 0~1개"(0 허용) → 문단마다 1개 정도로:

```
이모지는 문단마다 1개 정도, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.
```

- `voice_guidance()` **무변경** — ⑥ "남발 금지(문장마다 붙이지 마세요)"와 "문단마다 1개 정도"는 양립
  (문단 단위 1개 ≠ 문장마다). 3표면 공유 조각이라 다이어리 프롬프트에서만 사용량을 올린다.
- 오늘의 한마디·잡담 프롬프트(mascot.rs)는 무영향.

### 2c. UI — `src/lib/ui/DiaryTab.svelte` `<style>`만 (fix 3)

`article` 카드 배경 `var(--frame-bg)` → `var(--pastel-cream)` 한 곳만.
(달력의 일기 있는 날짜 셀 `.cells button.has`와 동일 톤 — 기존 토큰만, 신규 색 없음.)

### 2d. UI — `src/lib/ui/HomeTab.svelte` `<style>`만 (fix 4)

`.grid`에 한 줄 추가:

```css
.grid > :global(*) { min-width: 0; }
```

- grid 아이템 기본 `min-width:auto`를 0으로 낮춰 1fr 컬럼을 고정 → `SaveTop3 .action`의 기존
  ellipsis 복구, `WeekTrend` 늘어짐·가로 스크롤 해소. 자식이 하위 컴포넌트 루트라 `:global` 필요.
- 마크업·다른 규칙·자식 컴포넌트 무변경.

### 2e. 재생성 절차 (코드 아님 — 머지 후 실행)

1. `npm run build` + `cargo build -p agent-mentor-app`
2. 앱 종료 상태에서 `%APPDATA%\dev.agentmentor.app\agent-mentor.db`의 `diary_index` 창(오늘 기준 −7~−1) 삭제.
   **`findings`는 유지** (일기 반복은 프롬프트/브리프로 해결 — findings 삭제 불필요).
3. 실엔진(localhost:4444) 상태로 앱 실행 → startup backfill이 7일 창을 새 프롬프트·브리프로 재생성.
4. 육안: 날마다 다른 일기·이모지·크림 카드·홈 그리드 고정 확인.

## 3. 검증 / 테스트 기준

- **core**:
  - `recent_diaries` — 존재(오래된 것부터·발췌·푸터 제외)/부재(빈 벡터)/캡(500자, 멀티바이트 안전) 테스트.
  - 프롬프트 마커 — `recent_diaries`·"되풀이하지"·"다른 이야기"(신규), 이모지 "문단마다 1개"(기존 테스트의 "0~1개" 교체).
  - 기존 `system_prompt_*`·`assemble_brief_*`·`generate_diary_*` 회귀(수동 `Brief{}` 생성부 `recent_diaries: vec![]` 보정).
- **front**: `npx vitest run` 회귀 + `npm run build` (style만 — 신규 vitest 없음).
- **최종 판정**: 재생성 육안 (§2e).

## 4. 스코프 밖

`voice_guidance` 수정, 톤 프리셋(A/B/C), 브리프 다른 필드 축소, 달력 UI, 한마디·잡담 프롬프트,
다크 테마, 신규 색 토큰, `App.svelte` 프레임 배경 변경(카드 톤만으로 대비 확보).

## 5. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- 브랜치 `feat/diary-variety-ui-fixes`(생성됨) → 인라인 실행 → push+PR(base=main). 커밋 태스크 단위, main 직접 커밋 금지.
- 킥오프 문서도 이 브랜치에 함께 커밋.
