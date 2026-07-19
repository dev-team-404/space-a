---
status: done
archived: 2026-07-19
---

# 다이어리 가독성 개선 설계 스펙

- 작성: 2026-07-10 (브레인스토밍 산출물)
- 배경: 마스코트 보이스 #2(PR #15, voice_guidance) 머지 후 사용자 육안 피드백 —
  "문체는 자연스러운데 너무 길고 장황해서 읽기 어려움. 줄간격·파스텔 배경·이모지 적당히 필요."
- 선행: `voice_guidance()`(diary/mod.rs:283) 3표면 공유 중(다이어리·오늘의 한마디·잡담), DiaryTab.svelte(달력+본문), theme.css 파스텔 토큰
- 다음 단계: writing-plans → **인라인 실행**(PR #15 선례 — 소규모 2파일) → push+PR → 앱DB 재생성 육안

## 1. 브레인스토밍 결정 (2026-07-10)

| 논점 | 결정 |
|---|---|
| 길이 | **짧게 2~3문단, 전체 350자 이내** — 현재 프롬프트에 길이 지시가 아예 없음(장황함의 직접 원인). 상한과 함께 "핵심 한두 가지만 골라 쓰기"를 명시(브리프 사실 전부 쓰려는 경향 차단) |
| 이모지 | **다이어리만 완화** — `voice_guidance` ⑥(남발 금지)은 불변(3표면 공유·한마디/잡담은 40자 한 줄이라 남발 위험). `build_system_prompt`에만 "문단당 0~1개, 감정 포인트에만" 추가. 남발 금지+적당히 사용은 양립 |
| UI 수준 | **카드 + 타이포그래피** — article을 frame-bg 카드(radius-m·shadow-soft·패딩)로, line-height 1.75·문단 간격·가독 폭. 기존 theme.css 토큰만, 신규 색 금지 |
| 재생성 | 구현·머지 후 앱DB `diary_index` 삭제 → startup backfill 7일 창(오늘 제외)이 새 프롬프트로 재생성. #17 한마디·#18 잡담 E2E와 한 세션에 묶음 |

## 2. 변경 상세

### 2a. 프롬프트 — `crates/core/src/diary/mod.rs` `build_system_prompt`만

occasions 문단 뒤(포맷 문자열 끝)에 형식 지시 문단 추가:

```
형식: 일기는 짧게 — 2~3문단, 전체 350자 이내로 쓰세요. \
그날의 핵심 한두 가지만 골라 쓰고 나머지 사실은 과감히 버리세요. \
이모지는 적당히 — 문단당 0~1개, 감정이 실리는 자리에만 쓰세요.
```

- `voice_guidance()` 무변경 (⑥ "이모지 남발 금지"와 위 지시는 양립 — 남발은 금지, 기본 사용량만 상향).
- 오늘의 한마디·잡담 프롬프트(mascot.rs)는 무영향.

### 2b. UI — `src/lib/ui/DiaryTab.svelte` `<style>`만

```css
article {
  background: var(--frame-bg); border-radius: var(--radius-m);
  box-shadow: var(--shadow-soft); padding: 18px 22px;
  max-width: 62ch; line-height: 1.75;
}
article :global(p + p) { margin-top: 0.9em; }
```

- `.body`(스크롤 컨테이너)·`.empty`·달력(.cal) 무변경. 기존 `article :global(h1,h2,h3)` 규칙 유지.
- 신규 색·토큰 추가 없음 ("최소한의 파스텔톤 꾸미기" = 기존 카드 문법 재사용).

### 2c. 재생성 절차 (코드 아님 — 구현·머지 후 실행)

1. `npm run build` + `cargo build -p agent-mentor-app`
2. 앱 종료 상태에서 `%APPDATA%\dev.agentmentor.app\agent-mentor.db`의 `diary_index` 행 삭제
3. `.env` 실엔진(localhost:4444) 상태로 앱 실행 → startup backfill이 7일 창 재생성(vault `.md` 덮어씀)
4. 육안: 새 길이(2~3문단)·이모지·카드/줄간격 확인 — #17 한마디·#18 잡담 E2E 동시 수행

## 3. 검증 / 테스트 기준

- **core**: 기존 `system_prompt_*` 마커 테스트 회귀 + 신규 마커 테스트 1개
  (프롬프트가 "2~3문단"·"350자"·"이모지" 지시 포함 검증).
- **프론트**: `npm run build` + 기존 `npx vitest run` 회귀 (신규 vitest 없음 — style만).
- **최종 판정**: 재생성 육안 (§2c).

## 4. 스코프 밖

voice_guidance 수정, 톤 프리셋(A/B/C) 변경, 브리프 축소, 달력 UI 변경,
한마디·잡담 프롬프트, 다크 테마, 신규 색 토큰.

## 5. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- 브랜치 `feat/diary-readability`(생성됨) → 인라인 실행 → push+PR(base=main).
