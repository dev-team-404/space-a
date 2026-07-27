# 방명록 스킨 토큰 + 원글/답글 시인성 설계 (G7)

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate (프론트엔드 단독 소품)
- **출처**: [로드맵 4차 배치 G7](../plans/2026-07-26-life-social-diary-followups-roadmap.md) — G4(PR #112) 검증 중 사용자 피드백
- **선행**: G4 머지 완료(PR #112, 2026-07-27) — 같은 파일(`GuestbookTab.svelte`)을 건드리므로 그 뒤 착수

## 배경

`GuestbookTab.svelte`의 `<style>`이 스킨 무관 고정 파스텔 토큰을 쓴다:

- 입력창 `border: var(--pastel-lav)`, 리스트 카드 `background: var(--pastel-cream)` — 스킨(sky/mint/peach/lavender)을 바꿔도 방명록만 고정 색.
- 답글은 투명 배경 + `border: 1px var(--pastel-lav)`, `.replies`는 `border-left: 2px var(--pastel-lav)`뿐이라 **원글/답글 경계가 약하다**.

theme.css 설계 규칙("색·radius·그림자는 반드시 토큰에서만", 레거시 별칭은 v1 아웃-스코프 전용)에 맞춰 스킨 의미 토큰으로 마이그레이션한다.

## 확정 결정

| 결정 | 선택 | 근거 |
|---|---|---|
| 토큰 전략 | **기존 의미 토큰 재사용** (신설 없음, theme.css 변경 0) | 이웃 탭과 동일 어휘로 스킨 4종×모드 2종 자동 대응. 소품에 전용 토큰은 과함 |
| 원글/답글 구분 | **인셋 답글** — 원글 `--frame-bg` 카드 + 답글 `--panel2` 채움 + 들여쓰기 10→14px | 이웃 탭(CoachTab 인셋, NameChip 팝오버)과 같은 어휘. 배경 층위로 위계가 명확 |
| 범위 | **`<style>` 전체 일관 정리** — 색 토큰 + 입력창 배경/글자 명시 + 헤더 소버튼 + radius 토큰화 | theme.css 규칙 정합. 파일이 작아 diff 부담 낮음 |
| 헤더 소버튼(답글/삭제) | **`--accent-tint` 채움 + `--ink` 글자, 단일 규칙** | `--panel2` 채움은 답글 카드(`--panel2` 배경) 위에서 묻힘. accent-tint는 양쪽 표면에서 보이고 행동 어포던스에 스킨 색감 |

## 토큰 매핑표

변경 대상은 `a-mate/src/lib/ui/GuestbookTab.svelte`의 `<style>` 한정. **마크업·스크립트·theme.css 불변** (G4가 방금 넣은 NameChip 배선 유지).

| 셀렉터 | 현재 | 변경 후 | 비고 |
|---|---|---|---|
| `input` | `border: 1px var(--pastel-lav)`, radius `8px`, 배경/글자 미지정 | `border: 1px var(--line)` + `background: var(--frame-bg)` + `color: var(--ink)` + radius `var(--radius-s)` | ChatTab 입력창 패턴. 배경 미지정은 다크 모드에서 브라우저 기본값 의존 → 명시 |
| `button` (제출) | `background: var(--accent)`, `color: var(--accent-ink)`, radius `8px` | 색 유지, radius만 `var(--radius-s)` | 이미 시맨틱 |
| `.list article` (원글 카드) | `background: var(--pastel-cream)`, radius `10px` | `background: var(--frame-bg)` + radius `var(--radius-m)` + `box-shadow: var(--shadow-soft)` | HomeTab/CoachTab/ModelMix 카드와 동일 어휘. radius 10→12px |
| `.list .ava` | `background-color: var(--pastel-lav)` | `background-color: var(--line)` | 별칭 해소 — 현재도 실질값이 `--line`, 시각 동일 |
| `.list time` | `color: var(--ink-soft)` | 유지 | 이미 시맨틱 |
| `.list header button` (답글/삭제) | `background: var(--pastel-lav)`, `color: var(--ink)` | `background: var(--accent-tint)`, `color: var(--ink)` | 원글·답글 양쪽 표면에서 시인. 전경 `--accent` 금지 규칙과 무관(배경 + tint) |
| `.replies` | `border-left: 2px var(--pastel-lav)`, `padding-left: 10px` | `border-left: 2px var(--line)`, `padding-left: 14px` | 들여쓰기 확대 |
| `.list .reply` (답글) | `background: transparent`, `border: 1px var(--pastel-lav)`, radius `8px` | `background: var(--panel2)`, `border: 0`, radius `var(--radius-s)`, `box-shadow: none` | 인셋 답글. `box-shadow: none` 필수 — 답글도 `<article>`이라 `.list article` 그림자가 상속 매칭됨 |

## 배경 층위 (위계)

```
탭 뒷배경 --frame-2  →  원글 카드 --frame-bg (가장 밝음 + shadow-soft)  →  답글 --panel2 (카드 안 인셋)
```

스킨 4종 × 라이트/다크 8조합 모두 토큰 정의상 층위 유지 (예: sky 다크 `#131b31 → #1b2544 → #243050`, sky 라이트 `#e4edf8 → #fcfdff → #eaf1fb`).

## 검증선

시각 변경이라 자동 테스트로 결과를 못 잡는다. 검증은 두 층:

1. **자동 (구현 세션)**: `npm test` — `no-hardcoded-colors` 통과(hex 0건, 전경 `color: var(--accent)` 0건) + 기존 166건 회귀 없음. `npm run build` 컴파일 검증. 네이티브 Windows PowerShell에서 실행(WSL 금지).
2. **실화면 (사용자, hub 연결 환경)**: 사외망 개발 PC는 hub 도달 불가 → **PR 체크리스트**로 스크린샷 확인 항목을 남긴다:
   - [ ] 원글/답글 층위 대비 — 라이트·다크 각 1스킨 이상
   - [ ] 목록에서 카드 그림자(`--shadow-soft`) 과함 여부 — 과하면 그림자 제거 + `border: 1px solid var(--line)` 폴백
   - [ ] 입력창·헤더 소버튼(accent-tint) 시인성
   - [ ] 답글 폼(입력창)이 `--panel2` 인셋 옆에서 어색하지 않은지

## 범위 밖

- theme.css 토큰 추가/변경 (재사용만)
- 마크업/스크립트 변경, NameChip·아바타 로직
- 다른 탭의 `--pastel-*` 잔존 사용처 (예: App.svelte `.profile` border, ChatTab 입력창 border) — 별도 정리 대상이지 이번 diff에 안 담음
