---
title: 미니홈피 테마 시스템 (테마 모드 × 색상 세트) 설계
status: draft
date: 2026-07-21
component: a-mate
---

# 미니홈피 테마 시스템 설계

> 시각 미리보기(확정 근거): 라이트/다크 · 스킨 4종 · 설정 화면 목업 · 코칭 흰-배경 버그 before/after
> — Artifact: https://claude.ai/code/artifact/9e9fcf67-b177-443d-be46-32e23e0a3a5a
>
> 기준 커밋: `main ea23b38` — 이후 머지분(설정 탭 `LifeSettingsTab`, 방명록 `GuestbookTab`, 방 소셜 기능 #83) 포함. 테마 컨트롤은 별도 Settings 창이 아니라 **미니홈피 설정 탭(`LifeSettingsTab`)** 에 둔다.

## 1. 배경 · 문제

현재 a-mate 미니홈피 프론트(`src/`)의 색 체계는 다음 문제를 가진다.

| 문제 | 실제 상태 |
|------|-----------|
| 라이트 모드 부재 | `src/lib/theme.css`에 **다크 `:root` 하나뿐**. 전체 소스에 `prefers-color-scheme` 0건 → "시스템 설정으로 바뀐다"는 관찰과 달리 **테마 전환 로직 자체가 없음** |
| 토큰 의미 혼재 | `--pastel-lav`가 보더(`#2a3354`)이자 버튼/트랙 표면으로 이중 사용. `--frame-bg`/`--panel2`/`--ink` 등 이름이 역할과 불일치 |
| 하드코딩 색 산재 | 컴포넌트에 밝은 색이 흩어져 있음: `#f4f1fa`(CoachTab ×3), `#ffd9ca`(LifeView), `#fdf9ef44`·`#eef7f344`(SessionModal), `#fff`(DiaryTab·Mascot·Settings), `#0b3327`·`#3a1512`(App·TipCard) |
| **가독성 버그** | 다크 화면 위 하드코딩 밝은 배경(`#f4f1fa`)에 거의-흰 글자(`--ink` `#e8ecf8`)가 얹혀 **코칭 탭의 코드/드래프트 박스가 "흰 배경 + 흰 글자"로 판독 불가** |

→ 색 정의가 흩어져 있고 라이트 팔레트가 아예 없으므로, **토큰 체계를 다시 세우는 것**부터 시작한다.

## 2. 목표 · 비목표

**목표**
- 사용자가 **테마 모드(라이트 / 다크 / 시스템)** 를 설정에서 직접 고른다.
- 사용자가 **색상 세트(스킨)** 를 여러 파스텔 세트 중에서 고른다. 기본은 **하늘**.
- 파스텔 기반으로 배경·표면·글자 대비를 재정비해 **"흰 배경 + 흰 글자" 류 저대비를 구조적으로 제거**한다.
- 설정은 영속되고 **세 창(`chat`/`settings`/`mascot`)에 즉시 동기화**된다.

**비목표**
- a-lens(Pillar 3) 등 다른 컴포넌트의 색은 건드리지 않는다. 범위는 a-mate 프론트로 한정.
- 로봇/가구 스프라이트 색(`src/lib/robot/parts.ts`, `interior/` 에셋 색)은 **UI 테마와 무관**하므로 제외.
- **홈 탭 방 뷰(`LifeView` 격자방 / `MiniLife` 장식방)** 의 웜톤 방 비주얼은 v1 범위 밖. 테마/스킨은 **UI 크롬**(탭·카드·코칭·설정·홈 위젯)에만 적용하고, 방 뷰는 레거시 별칭(§6)으로 렌더를 유지하며 **명백한 저대비 버그만 최소 수정**한다. 방 팔레트의 스킨 대응은 후속 과제.
- `ModelMix`의 카테고리 차트 색(`FALLBACK`/`OTHER_COLOR`)은 데이터 시각화 색이라 별도 취급(후속). v1에서는 유지.
- 사용자 커스텀 색(임의 hex 지정)은 하지 않는다. 사전 정의된 스킨 중 선택만.

## 3. 개념 모델 — 2축 독립 조합

테마는 **모드**와 **스킨** 두 축의 독립 조합이다.

| 축 | 값 | 의미 |
|----|----|------|
| **테마 모드** | `light` / `dark` / `system` | 밝기. `system`은 OS 설정(`prefers-color-scheme`)을 따른다 |
| **색상 세트(스킨)** | `sky`(기본) / `mint` / `peach` / `lavender` | 색 계열. 각 스킨은 accent 계열 + 표면 톤을 함께 정의 |

- 적용은 루트 요소 속성으로: `<html data-theme="light|dark" data-skin="sky|mint|peach|lavender">`
- `data-theme`에는 **항상 확정된 light 또는 dark만** 들어간다. `system`은 프론트에서 `prefers-color-scheme`로 해석해 둘 중 하나로 확정한다(§6).

## 4. 토큰 계약 (시맨틱 레이어)

컴포넌트는 아래 시맨틱 토큰만 참조한다. **모든 배경 토큰은 대응하는 글자 토큰과 짝**을 이루며, 컴포넌트가 배경을 지정하면 반드시 짝 글자색을 함께 지정한다(저대비 재발 차단).

| 토큰 | 역할 | 짝/비고 |
|------|------|---------|
| `--bg` | 앱 배경 (창 바탕) | 위에 `--text` |
| `--frame` | 카드·위젯 표면 | 위에 `--text` |
| `--frame-2` | 미니홈피 외곽 프레임(카드보다 깊게) | 위에 `--text` |
| `--surface-inset` | 칩·바 트랙·**코드/드래프트 박스** | 위에 `--text` — **구 `#f4f1fa` 대체** |
| `--line` | 보더·구분선 **전용** | — |
| `--text` | 본문 글자 | — |
| `--text-soft` | 보조/설명 글자 | — |
| `--accent` | 스킨 대표색 **fill**(버튼·활성 탭·칩 배경) + 좌측 강조선 | 위에 `--accent-ink` |
| `--accent-ink` | `--accent` fill **위 글자** | 어두운 중립색 |
| `--accent-strong` | **밝은 표면 위** 스킨색 글자·링크(제목 강조, "저장" 등) | 라이트에서 진한 톤, 다크에서 `--accent`와 동일 |
| `--accent-tint` / `--accent-tint-b` | 📊 근거 박스 바탕 / 보더 | 위에 `--text` |
| `--lav-surface` / `--lav-ink` | 보조 버튼(비강조 액션) 표면/글자 | 상태·기능색(스킨 공유, §5) |
| `--coral` / `--coral-ink` | 경고·배지 표면/글자 | 상태·기능색 |
| `--cream` / `--cream-ink` | 정보(.env 등) 표면/글자 | 상태·기능색 |
| `--danger` | 오류 텍스트(폼 검증 등) | 상태·기능색 (구 하드코딩 `#b74444` 대체) |
| `--radius-s/m/l`, `--shadow-soft` | 반경·그림자 | 스킨/모드 무관 상수 |

### 4.1 구 토큰 → 신 토큰 매핑 (마이그레이션 기준)

| 구 토큰 | 신 토큰 |
|---------|---------|
| `--bg-grad` | `--bg` |
| `--frame-bg` | `--frame` |
| `--frame-2` | `--frame-2` (유지) |
| `--panel2` | `--surface-inset` |
| `--line` | `--line` (보더 역할만 유지) |
| `--ink` | `--text` |
| `--ink-soft` | `--text-soft` |
| `--accent` | `--accent` (유지) |
| `--pastel-mint` (솔리드 민트 배지) | `--accent` |
| `--pastel-lav` (보더 용도) | `--line` |
| `--pastel-lav` (버튼/트랙 표면 용도) | `--lav-surface` |
| `--lav` | `--lav-ink` |
| `--pastel-coral` | `--coral` |
| `--pastel-cream` | `--cream` |
| `--mint-tint` / `--mint-tint-b` | `--accent-tint` / `--accent-tint-b` |
| 하드코딩 `#f4f1fa` | `--surface-inset` (+ 짝 `--text`) |
| 하드코딩 `#0b3327` (민트 위 글자) | `--accent-ink` |
| 하드코딩 `#fff` (accent 위 글자) | `--accent-ink` |
| 하드코딩 `#3a1512` (코랄 배지 글자) | `--coral-ink` |
| 하드코딩 `#ffd9ca` (제목 배지) | `--coral` (또는 `--accent-tint`) |

## 5. 팔레트 값 (확정 시작값)

아래 값은 시각 검토를 거쳐 확정한 **시작값**이다. 인앱 실측 시 미세 조정 가능(대비 규칙은 유지).

### 5.1 스킨별 · 모드별 (accent 계열 + 표면·글자)

**하늘 (sky) — 기본**

| 토큰 | light | dark |
|------|-------|------|
| `--bg` | `#eaf2fc` | `#0f1626` |
| `--frame` | `#fcfdff` | `#1b2544` |
| `--frame-2` | `#e4edf8` | `#131b31` |
| `--surface-inset` | `#eaf1fb` | `#243050` |
| `--line` | `#d6e0f0` | `#313d61` |
| `--text` | `#232f3e` | `#e6ecf8` |
| `--text-soft` | `#55607a` | `#98a4c6` |
| `--accent` | `#84c9ef` | `#7fc9f0` |
| `--accent-ink` | `#0e2230` | `#0c1826` |
| `--accent-strong` | `#1670b8` | `#7fc9f0` |
| `--accent-tint` | `#e2f1fc` | `rgba(127,201,240,.12)` |
| `--accent-tint-b` | `#bfe2f6` | `rgba(127,201,240,.30)` |

**민트 (mint)**

| 토큰 | light | dark |
|------|-------|------|
| `--bg` | `#ecf8f3` | `#0e1c19` |
| `--frame` | `#ffffff` | `#16241f` |
| `--frame-2` | `#e4f0ea` | `#0f1a17` |
| `--surface-inset` | `#ecf5f1` | `#1f302a` |
| `--line` | `#d3e7de` | `#2c3f38` |
| `--text` | `#26332e` | `#e6f0ec` |
| `--text-soft` | `#647469` | `#94a89f` |
| `--accent` | `#5cd0b0` | `#7ee8c8` |
| `--accent-ink` | `#0e2620` | `#0a1613` |
| `--accent-strong` | `#0b8368` | `#7ee8c8` |
| `--accent-tint` | `#e2f7ef` | `rgba(126,232,200,.12)` |
| `--accent-tint-b` | `#bfe9da` | `rgba(126,232,200,.30)` |

**살구 (peach)**

| 토큰 | light | dark |
|------|-------|------|
| `--bg` | `#fdf3ec` | `#291c12` |
| `--frame` | `#ffffff` | `#392819` |
| `--frame-2` | `#f7e9de` | `#211710` |
| `--surface-inset` | `#fbeee4` | `#453321` |
| `--line` | `#f0dccb` | `#57422c` |
| `--text` | `#3a2a20` | `#f4ede7` |
| `--text-soft` | `#8a6f5c` | `#c6ad97` |
| `--accent` | `#f4b183` | `#f0b58c` |
| `--accent-ink` | `#3a1e0e` | `#241206` |
| `--accent-strong` | `#b5501f` | `#f0b58c` |
| `--accent-tint` | `#fdeadd` | `rgba(240,181,140,.12)` |
| `--accent-tint-b` | `#f6d3b8` | `rgba(240,181,140,.30)` |

**라벤더 (lavender)**

| 토큰 | light | dark |
|------|-------|------|
| `--bg` | `#f3f0fc` | `#1e1a38` |
| `--frame` | `#ffffff` | `#2a2450` |
| `--frame-2` | `#eae6f8` | `#1d183a` |
| `--surface-inset` | `#f0ecfb` | `#332c62` |
| `--line` | `#e0d9f2` | `#443c70` |
| `--text` | `#2c2740` | `#ece9f7` |
| `--text-soft` | `#6f6885` | `#a8a2ca` |
| `--accent` | `#b9a6f0` | `#b8a7f7` |
| `--accent-ink` | `#1e163a` | `#140f28` |
| `--accent-strong` | `#6c4fd0` | `#b8a7f7` |
| `--accent-tint` | `#efe9fb` | `rgba(184,167,247,.12)` |
| `--accent-tint-b` | `#ddd0f5` | `rgba(184,167,247,.30)` |

### 5.2 상태·기능 파스텔 (스킨 공유, 모드별)

경고/정보/보조액션은 의미 기반 색이므로 스킨과 무관하게 공유하고 모드만 뒤집는다.

| 토큰 | light | dark | 용도 |
|------|-------|------|------|
| `--lav-surface` | `#e7e9fb` | `#2c2c54` | 비강조 버튼 표면 |
| `--lav-ink` | `#37356b` | `#c9c2f7` | 위 글자 |
| `--coral` | `#ffb3ad` | `#7a3a38` | 경고 카드·코칭 배지 |
| `--coral-ink` | `#6b241d` | `#ffd9d5` | 위 글자 |
| `--cream` | `#ffe6bf` | `#4a3f2a` | 정보(.env 등) |
| `--cream-ink` | `#6b5218` | `#ffe4b0` | 위 글자 |
| `--danger` | `#b74444` | `#f2a5a0` | 오류 텍스트 |

> **엣지**: 라벤더 스킨에서는 `--accent`(라벤더)와 `--lav-surface`(보조 라벤더)가 톤이 근접한다. v1에서는 허용(강조/비강조는 채도·글자로 구분). 필요 시 후속에서 보조색 이름을 중립화(`--surface-2`)한다.

### 5.3 가독성 근거 (요약)
- `--accent` fill 위 글자는 **진초록/동일계열이 아닌 어두운 중립**(`--accent-ink`)으로 → 명도·색상 대비 모두 확보(같은-hue 탁함 제거).
- 밝은 표면 위 스킨색 **글자**는 밝은 `--accent`가 아니라 진한 `--accent-strong`으로 → 흰 배경 대비 ≥ 4.5:1.
- "밝은 accent + 흰 글자" 조합은 대비 부족이라 **채택하지 않음**.

## 6. 적용 메커니즘 (프론트)

새 모듈 `src/lib/theme.ts` — 세 창 엔트리(`App.svelte`/`Settings.svelte`/`Mascot.svelte`)에서 초기화.

**순수 해석 함수 (단위 테스트 대상)**
```ts
type Mode = 'light' | 'dark' | 'system';
type Skin = 'sky' | 'mint' | 'peach' | 'lavender';
function resolveTheme(mode: Mode, systemPrefersDark: boolean): 'light' | 'dark' {
  return mode === 'system' ? (systemPrefersDark ? 'dark' : 'light') : mode;
}
```

**적용 (부수효과)**
```ts
function apply(mode: Mode, skin: Skin) {
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(mode, dark);
  root.dataset.skin = skin;
}
```

**초기화 흐름**
1. 모듈 로드 즉시 **기본값(`system`+`sky`)으로 `data-theme`/`data-skin`을 먼저 세팅** → FOUC(잘못된 테마 번쩍임) 방지.
2. `invoke('theme_get')`로 저장값을 받아 재적용.
3. 구독:
   - `matchMedia('(prefers-color-scheme: dark)')` change → 현재 mode가 `system`이면 재적용.
   - Tauri 이벤트 `theme:changed`(payload `{mode, skin}`) → 상태 갱신 후 재적용(창 간 동기화).
4. `setTheme({mode, skin})` → 낙관적 즉시 `apply()` + `invoke('theme_set', {mode, skin})`.

CSS(`theme.css`)는 `:root[data-skin="sky"][data-theme="light"] { … }` … 8개 블록으로 §5 값을 정의. `data-theme`은 항상 light/dark로 확정되므로 CSS에 `@media` 중복 불필요(단일 소스). theme.ts는 세 창 엔트리(`App`/`Settings`/`Mascot`)에서 초기화되어 어느 창이든 선택된 스킨·모드로 렌더된다(컨트롤 UI는 §8의 `LifeSettingsTab` 한 곳).

**레거시 별칭 레이어.** 방 뷰(§2 비목표) 등 v1에서 이관하지 않는 컴포넌트가 계속 렌더되도록, `theme.css`의 각 스킨×모드 블록에 구 토큰명을 신 토큰에 매핑하는 별칭을 함께 정의한다 — `--ink: var(--text)`, `--ink-soft: var(--text-soft)`, `--frame-bg: var(--frame)`, `--panel2: var(--surface-inset)`, `--bg-grad: var(--bg)`, `--pastel-mint: var(--accent)`, `--pastel-coral: var(--coral)`, `--pastel-cream: var(--cream)`, `--mint-tint(-b): var(--accent-tint(-b))`, `--lav: var(--lav-ink)`. 단 `--pastel-lav`는 보더·표면 이중 사용이므로 별칭은 흔한 역할(보더=`--line`)로 두고, **인스코프 컴포넌트의 표면 용도는 마이그레이션에서 `--lav-surface`로 정정**한다. 이관 완료된 컴포넌트가 늘면 별칭은 후속 PR에서 제거한다.

## 7. 저장 · 동기화 (Rust)

기존 `SqliteStore` KV(`get_setting`/`set_setting`)와 `app.emit` 선례(`settings:changed`)를 그대로 사용.

- **저장 키**: `theme_mode`(기본 `"system"`), `theme_skin`(기본 `"sky"`).
- **커맨드**(`src-tauri/src/commands.rs`, `lib.rs` invoke_handler 등록):
  - `theme_get() -> ThemeSettings { mode, skin }` — 미설정 시 기본값.
  - `theme_set(mode: String, skin: String)` — 값 검증(허용 enum) → `set_setting` ×2 → `app.emit("theme:changed", ThemeSettings)`로 전 창 브로드캐스트.
- **입력 검증**: mode/skin이 허용 집합 밖이면 에러 반환(프론트는 UI로만 호출하므로 방어적).
- (선택, v1 범위 밖) 네이티브 창 크롬 일치: Tauri v2 `set_theme`로 타이틀바 밝기 맞춤 — 후속 고려.

## 8. 설정 UI — 미니홈피 설정 탭 (`LifeSettingsTab.svelte`)

미니홈피 관련 설정은 미니홈피 안의 **설정 탭**(`App.svelte`의 `{ id: 'settings' }` → `LifeSettingsTab`)에서 한다. 여기 **테마** 섹션을 추가한다(별도 `Settings.svelte` 창이 아님 — 그 창은 엔진/허브/이미지 설정 전용).

- **테마 모드**: 세그먼트 컨트롤 `[라이트] [다크] [시스템]`.
- **색상 세트**: 스킨 스와치 `하늘 · 민트 · 살구 · 라벤더`(각 accent 색 원형, 선택 표시).
- 변경 즉시 `setTheme({mode, skin})` 호출 → 저장 + `theme:changed`로 세 창 반영. 별도 "저장" 버튼·토스트 불필요(기존 공개범위 토글과 동일한 즉시 반영 UX).
- 카피: 섹션 제목 "테마", 라벨 "밝기"(라이트/다크/시스템), "색상 세트".
- 배치: 기존 "일촌 관리"·"다이어리 공개 범위" 위 또는 아래 한 섹션. 이 컴포넌트 자체의 하드코딩(`color:white`, `#b74444`)·구 토큰도 함께 정정(§9).

> `Settings.svelte`(엔진/허브/이미지 창)와 `Mascot.svelte`는 컨트롤을 갖지 않지만, 창이 선택된 테마로 렌더되도록 theme.ts를 초기화한다(§6).

## 9. 컴포넌트 마이그레이션 목록

`theme.css` 재작성 후, 아래 파일의 구 토큰·하드코딩 색을 §4.1 매핑대로 치환한다.

| 파일 | 처리 | 범위 |
|------|------|------|
| `src/lib/theme.css` | 토큰 계약 + 8개 스킨×모드 블록 + 상태색 + **레거시 별칭 레이어**(§6)로 **전면 재작성** | 인스코프 |
| `src/lib/theme.ts` | **신규** — resolver·적용·이벤트 구독·`setTheme` | 인스코프 |
| `src/App.svelte` | `--bg-grad`/`--ink*`/`--frame*`/`--panel2`/`--pastel-lav` 치환, `#0b3327`→`--accent-ink`, `#3a1512`→`--coral-ink`, theme.ts 초기화 | 인스코프 |
| `src/lib/ui/LifeSettingsTab.svelte` | **테마 UI 호스트**(§8) + `color:white`→`--accent-ink`, `#b74444`→`--danger`, `--pastel-cream/lav` 치환 | 인스코프 |
| `src/lib/ui/GuestbookTab.svelte` | `color:white`→`--accent-ink`, 구 토큰 치환 | 인스코프 |
| `src/Settings.svelte` | 구 토큰 치환, `#fff`(primary)→`--accent-ink`, theme.ts 초기화 (**컨트롤은 없음**) | 인스코프 |
| `src/Mascot.svelte` | 라이트 컨텍스트 메뉴 팔레트(`#f3f3f3·#1f1f1f·#666·#c7c7c7·#aaa·#d9d9d9·#fff·#fafafa·#a9a9a9`) → 토큰(`--frame`/`--text`/`--text-soft`/`--surface-inset`/`--line`), theme.ts 초기화 | 인스코프 |
| `src/lib/ui/CoachTab.svelte` | **`#f4f1fa` ×3 → `--surface-inset`(+ `--text`)** [원 버그 수정], `--pastel-*` 치환 | 인스코프 |
| `src/lib/ui/SessionModal.svelte` | `#fdf9ef44`→`--cream`(알파 정리), `#eef7f344`→`--accent-tint` | 인스코프 |
| `src/lib/ui/DiaryTab.svelte` | `.sel color:#fff`·`.dot #fff`→`--accent-ink` | 인스코프 |
| `src/lib/ui/home/TipCard.svelte` | `#0b3327`→`--accent-ink` | 인스코프 |
| 기타 `src/lib/ui/**`·`home/**` | `var(--ink*/--frame*/--panel2/--pastel*/--mint-tint*)` 잔여 치환(전수 grep) | 인스코프 |
| `src/lib/ui/LifeView.svelte` | **v1 제외** — 별칭으로 렌더 유지. 명백한 저대비만 최소 수정(예: `color:white` on `--accent`) | 아웃 |
| `src/lib/ui/MiniLife.svelte` | **v1 제외** — 별칭으로 렌더 유지(`#e9e3f8` 등 방 그라디언트 보존) | 아웃 |
| `src/lib/ui/home/ModelMix.svelte` | **v1 제외** — 차트 카테고리 색 별도 취급(후속) | 아웃 |

## 10. 에러 처리

- `theme_get` 실패(스토어 미초기화 등) → 프론트는 이미 세팅한 **기본값(system+sky)** 유지. 앱은 항상 스타일된 상태.
- `theme_set` 실패 → 낙관적 적용은 유지하되 콘솔 경고. 다음 로드 시 저장값과 재동기화.
- 잘못된 저장값(수동 편집 등) → 프론트 `apply()`에서 허용 집합 밖이면 기본값으로 폴백.

## 11. 테스트 전략

| 층 | 테스트 |
|----|--------|
| 프론트 단위(Vitest) | `resolveTheme(mode, systemPrefersDark)` 진리표(6조합), 잘못된 값 폴백 |
| Rust | `theme_get`/`theme_set` 스토어 왕복, 기본값, 잘못된 enum 거부 |
| 회귀 가드 | **인스코프** `src/**/*.svelte` 스타일에 하드코딩 hex 색 없음을 검사하는 테스트/스크립트 → 저대비 버그 재발 방지. 예외(허용 목록): `theme.css`, 스프라이트(`robot/*`), v1 아웃-스코프(`LifeView`·`MiniLife`·`ModelMix`). 예외 파일이 줄면 목록도 축소 |
| 수동 QA 체크리스트 | 스킨 4 × 모드 3 × 창 3 조합 스팟체크, 특히 **코칭 드래프트/코드 박스 가독성**(원 버그), 창 간 즉시 동기화, `system`에서 OS 토글 반영 |

## 12. 구현 순서 (PR 구성, 개요)

상세 태스크는 별도 구현 계획(writing-plans)에서. 대략:

1. **토큰 기반** — `theme.css` 재작성(계약 + 8블록 + 상태색 + 레거시 별칭), `theme.ts` 추가, 회귀 가드 테스트. (별칭 덕에 방 뷰 등은 이 단계에서 이미 렌더 유지)
2. **백엔드** — `theme_get`/`theme_set` 커맨드 + `theme:changed` emit + Rust 테스트.
3. **컴포넌트 마이그레이션** — §9 인스코프 파일 전수 치환(코칭 버그 수정 포함), 하드코딩 hex 제거.
4. **설정 UI + 창 연동** — `LifeSettingsTab` 테마 섹션(밝기·색상 세트), 세 창(`App`/`Settings`/`Mascot`) theme.ts 초기화·구독.
5. **QA·마무리** — 수동 체크리스트, DoD 아카이브(ADR 0013).

단일 기능 브랜치(`feat/minihompy-theme-system`)에서 진행하며, 완료 시 이 스펙과 구현 계획을 `docs-archive`로 이관한다.

## 13. 미해결 / 추후

- 라벤더 스킨의 accent↔보조 라벤더 근접(§5.2 엣지) — 필요 시 보조색 중립화.
- 네이티브 창 크롬(타이틀바) 밝기 일치(`set_theme`) — 후속.
- 스킨 추가(로즈 등)는 §5에 블록만 더하면 되도록 확장 가능하게 유지.
