# 다이어리 다양성 + UI 픽스 4건 킥오프 (2026-07-10)

재생성 E2E(PR #19·#21 머지 후) 사용자 피드백 4건. 진단 완료 — 설계안까지 사용자에게
제시된 상태(승인 대기). 새 세션은 여기서 시작: 승인 확인 → 스펙(docs/specs/) →
플랜 → 구현(규모상 인라인 실행 후보) → PR(base=main).

## 사용자 피드백 원문 요지

1. 일기가 매일 같은 내용(전부 context7 빼라는 얘기). 이전 2~3일 일기를 참조해서 쓰면 좋겠다.
2. 이모지가 없다(넣기로 했는데).
3. 배경(파스텔 꾸미기)이 안 보인다.
4. 홈 탭 "절약 실천 top3"가 길어지면 가로 스크롤이 생기고 그 위 주간 사용량이 늘어진다 — 고정 사이즈로.

## 진단 (조사 완료 — 재조사 불필요)

1. **일기 반복**: R5 발화 보류(PR #21) 후 findings가 R1/R2(host 스코프, context7 등)뿐
   — host 스코프는 활성인 동안 `findings_for_date`에 매일 포함 → 매일 같은 코칭 서사.
   350자 단축(PR #19)으로 체감 심화.
2. **이모지 0개**: 프롬프트 "문단당 0~1개"가 0개 허용 소극 지시 + voice_guidance ⑥
   "남발 금지"가 억제로 이김.
3. **배경 안 보임**: 카드 CSS는 dist·바이너리에 정상 포함(확인됨). 문제는 App.svelte
   `.homepy` 프레임 전체가 `frame-bg`(#fffdfa)인데 DiaryTab 카드도 `frame-bg` —
   흰 배경 위 흰 카드라 시각 구분 불가.
4. **홈 그리드**: `HomeTab.svelte` `.grid { 1fr 1fr }`인데 grid 아이템 기본
   `min-width:auto` 때문에 SaveTop3의 긴 `suggested_action`(nowrap)이 컬럼을 밀어냄
   → `.action`의 기존 ellipsis 무력화, 같은 컬럼 WeekTrend 늘어짐, 가로 스크롤.

## 설계안 (사용자에게 제시됨 — 승인만 남음)

1. **`Brief.recent_diaries` 추가** (crates/core/src/diary/mod.rs):
   직전 3일 중 존재하는 일기(각 본문 500자 캡, char 경계 안전)를 vault에서 읽어
   브리프에 포함(`store.diary_path_for(date)` 재사용, 읽기 실패는 조용히 스킵).
   `build_system_prompt`에 지시 추가: "recent_diaries에서 이미 다룬 지적은 반복하지
   말고(필요하면 한 줄로 스치기), 오늘만의 사실·기분에 집중해 어제와 다른 이야기로."
   backfill이 오래된 날짜부터 생성하므로(missing_diary_dates) 재생성 순서 정합.
   로컬 파일→온프렘 엔진이라 프라이버시 경계 불변.
2. **이모지 지시 상향** (build_system_prompt): "문단당 0~1개" → "문단마다 1개 정도,
   자연스러운 자리에(같은 이모지 반복 금지)".
3. **DiaryTab 카드 배경** `frame-bg` → `var(--pastel-cream)` (달력의 일기 있는 날짜
   셀과 동일 톤, 기존 토큰만).
4. **HomeTab** `.grid > :global(*) { min-width: 0; }` 한 줄 — 컬럼 1fr 고정,
   기존 ellipsis 작동.

## 검증

- core: brief recent_diaries(존재/부재/캡) + 프롬프트 마커(반복 금지·이모지) 테스트,
  기존 system_prompt_*·brief 회귀.
- front: vitest 회귀 + npm run build (style 위주라 신규 vitest 없음).
- 최종: 머지 후 재생성 육안 — **findings는 유지**하고 diary_index 창(오늘 기준 -7~-1)만
  삭제 → 앱 실행. 날마다 다른 일기·이모지·크림 카드·홈 그리드 고정 확인.

## 참고

- 브랜치명 후보: `feat/diary-variety-ui-fixes`. 커밋은 태스크 단위, main 직접 커밋 금지.
- 재생성 절차·DB 경로: `%APPDATA%\dev.agentmentor.app\agent-mentor.db`, 앱 종료 후
  py sqlite3로 삭제(이 세션에서 반복 검증된 절차 — [[roadmap]] #12~#14).
- 빌드 레시피: 메모리 [[build-env]] (매 cargo 전 필수).
- 직전 이력: PR #19(가독성)·#20(.claude 노이즈)·#21(R5 보류) — [[roadmap]] #12~#14.
