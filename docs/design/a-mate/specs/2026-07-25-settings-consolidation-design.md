# 설정 표면 통합 설계 — 별도 `settings` 창을 미니홈피 설정 탭으로

- **날짜**: 2026-07-25
- **컴포넌트**: a-mate (Pillar 1)
- **범위**: 프론트엔드(Svelte) + Tauri 셸 배선. `crates/core` 도메인 로직 변경 없음. 계약(`contracts/`)·스키마·저장 키 변경 없음.

## 배경 — 설정이 세 곳에 흩어져 있다

| 표면 | 여는 법 | 내용 |
|---|---|---|
| 트레이 우클릭 메뉴 (`src-tauri/src/tray.rs`) | 트레이 아이콘 우클릭 | 열기, 마스코트 표시·위치 초기화, 실시간 조언, 화면 캡처 보호, 잡담(자주/가끔/안 함), 지금 스캔, **설정**(→ 별도 창), 업데이트 확인, 시작 시 실행, 종료 |
| 별도 `settings` 창 (`src/Settings.svelte`, 520×720) | 트레이 → "설정" | LLM 엔진, 캐릭터 이미지 모델, **Life Server**(URL·API 키·연결/종료) |
| 미니홈피 창 설정 탭 (`src/lib/ui/LifeSettingsTab.svelte`) | chat 창 → "설정" 탭 | 개인정보, 테마, 일촌 관리, 다이어리 공개 범위, 주인 메모리 |

Life Server 연결은 트레이에서만 닿는 별도 창에 있고, 미니홈피 창에는 또 다른 설정 탭이 있다.
두 표면을 나누는 기준이 사용자 입장에서 존재하지 않는다.

## 원칙

**설정(저장되는 값)은 한 곳, 액션(지금 하는 일)은 가장 가까운 표면.**

| 표면 | 역할 |
|---|---|
| 미니홈피 창 **설정 탭** | 모든 환경설정 |
| **마스코트** 우클릭 | 방 이동 · 말풍선 설정 (상태 변경 액션) |
| **트레이** 우클릭 | 열기 · 지금 스캔 · 업데이트 확인 · 종료 (앱 수명 제어) |

### 방 이동·말풍선을 마스코트에 남기는 이유

1. **액션이지 설정이 아니다.** 저장해두는 환경설정이 아니라 지금 어디에 있을지 고르는 동작이다.
   설정 탭에 넣으면 "설정을 바꿨더니 화면이 남의 방으로 바뀌는" 인과가 생긴다.
2. **마스코트가 리모컨, 미니홈피 창이 화면**이라는 역할 분담이 이미 코드에 있다.
   `App.svelte`의 폴링이 2초마다 `lifeView()`를 조회해 방이 바뀌면 방문 모드로 전환한다.
   조작은 마스코트, 결과는 창.
3. **접근성이 반대 방향이다.** 마스코트는 항상 떠 있어 창을 열지 않고 방을 옮길 수 있다.
   설정 탭으로 옮기면 "창 열기 → 설정 탭 → 방 이동 → 홈 탭 복귀" 4단계가 된다.

### 의도적 예외 — 트레이 토글류는 이번 범위 밖

실시간 조언, 화면 캡처 보호, 잡담 빈도, 시작 시 실행, 마스코트 표시는 원칙상 설정이지만
트레이에 그대로 남긴다. 창을 열지 않고 즉시 끄는 것이 본래 가치이고,
Rust `CheckMenuItem` ↔ 프론트 양방향 상태 동기화가 별도 작업 분량이다.
→ 아래 **"범위 밖 — 후속 과제"**에 기록.

## 설정 탭 정보 구조 — 상단 pill 서브탭 4그룹

합치면 설정 탭에 8개 섹션이 모인다. `<hr/>`로 이어 붙이면 한 스크롤에 8덩어리가 쌓여
Life Server 연결 란을 찾기 어렵다. 미니홈피 창은 이미 왼쪽 프로필 칸과 오른쪽 세로 탭이 있어
왼쪽 서브 목록을 더하면 세로 띠가 4개가 된다. 따라서 **상단 pill 행**을 쓴다
(`LifeSettingsTab`의 기존 `.seg` 스타일 재사용).

```
┌─ 준녕님의 미니홈피 ───────────────────┐
│ ┌─────┐  [연결] 나  공개  모양        │┐홈
│ │ 🤖  │  ──────────────────────  │┤다
│ └─────┘  Life Server               │┤코
│ 📔 오늘의  연결됨 — 내 방: life-a1    │┤침
│   일기     서버 URL [http://…:8001 ] │┤채
│           API 키   [•••••••••••• ]  │┤팅
│           (연결) (연결 종료)          │┤방
│           ──────────────────────  │┤명
│           LLM 엔진                  │┤──
│           엔드포인트 […/v1        ]  ││▶설
│           모델명    [gpt-4o-mini ]  ││ 정
│           (저장) (연결 테스트)        │┘
│           ──────────────────────  │
│           캐릭터 이미지 ▼             │
└──────────────────────────────────┘
```

| 그룹 id | 라벨 | 섹션 | 출처 |
|---|---|---|---|
| `conn` | 연결 | Life Server · LLM 엔진 · 캐릭터 이미지 | `Settings.svelte`에서 이동 |
| `me` | 나 | 개인정보 · 주인 메모리 | `LifeSettingsTab` |
| `privacy` | 공개 | 일촌 관리 · 다이어리 공개 범위 | `LifeSettingsTab` |
| `look` | 모양 | 테마 | `LifeSettingsTab` |

**기본 그룹은 `conn`** — 처음 쓸 때 가장 먼저 필요한 설정이다.
**마지막 그룹을 기억하지 않는다.** `App.svelte`가 탭을 `{#if}`로 렌더하므로
설정 탭을 떠나면 `SettingsTab`이 언마운트되고 다시 들어올 때 `conn`으로 돌아온다 — 별도 처리 없이 성립한다.

딥링크는 기존 `chat:goto-tab` 이벤트의 `target`을 그룹 id로 쓴다:
`open_chat_tab("settings", "conn")`. `target`은 탭 문맥으로 해석하는 자유 문자열이고
백엔드는 내용을 해석하지 않으므로(`commands.rs` `GotoTabPayload` 주석) 기존 규약에 맞는다.

## 파일 구조

`LifeSettingsTab.svelte`는 116줄에 이미 5개 관심사(프로필·테마·일촌·공개범위·메모리)가 섞여 있고
3개를 더 받으면 손대기 어려워진다. 그룹 단위로 나눈다.

```
src/lib/ui/settings/
  SettingsTab.svelte       # 서브탭 렌더 + 그룹 선택만 담당 (LifeSettingsTab 대체)
  ConnectionGroup.svelte   # Life Server + LLM 엔진 + 캐릭터 이미지
  MeGroup.svelte           # 개인정보 + 주인 메모리
  PrivacyGroup.svelte      # 일촌 관리 + 다이어리 공개 범위
  LookGroup.svelte         # 테마
  StatusLine.svelte        # idle/ok/err/busy 상태 배지 (프레젠테이션)
  groups.ts                # 그룹 목록 + target 정규화 (순수 함수)
  status.ts                # Status 타입 + idle/busy/ok/err 생성자 (순수 함수)

src/lib/ui/tab-routing.ts  # Tab 타입 + 방 변경 시 탭 결정 (순수 함수, "방문 중 처리" 참고)
```

`Tab` 타입은 지금 `App.svelte` 안에 있다. `resolveTabAfterLifeChange`를 테스트하려면
타입과 로직이 `.svelte` 밖에 있어야 하므로 `tab-routing.ts`로 옮기고 `App.svelte`가 import한다.
설정 그룹과는 관심사가 다르므로(탭 라우팅 vs 설정 그룹) `groups.ts`에 섞지 않는다.

- `LifeSettingsTab.svelte`는 삭제하고 `App.svelte`의 import를 `SettingsTab`으로 교체한다.
- **각 그룹은 자기 데이터만 로드·저장하고 서로 상태를 공유하지 않는다.**
  `SettingsTab`은 어떤 그룹을 보여줄지만 결정한다 — 데이터를 알지 않는다.
- `StatusLine` + `status.ts`가 필요한 이유: 지금 `Settings.svelte`의 `.status`/`.source`와
  `LifeSettingsTab`의 `.pstatus`가 같은 `{kind:'idle'|'ok'|'err'|'busy'; text}` 패턴을
  세 번 다르게 구현하고 있다.
- **섹션 껍데기 컴포넌트는 두지 않는다.** 코드베이스에 snippet(`{@render}`) 사용례가 없어
  래퍼 컴포넌트를 만들면 새 idiom을 들여오게 된다. 각 그룹이 `<section><h2>…</h2>`를 직접 쓰고
  공용 섹션 스타일(제목·설명·구분선)은 `SettingsTab.svelte`의 `:global` 블록이 제공한다 —
  기존 컴포넌트들이 섹션을 인라인으로 쓰는 방식과 같다.

`groups.ts` 공개 인터페이스:

```ts
export type SettingsGroup = 'conn' | 'me' | 'privacy' | 'look';
export const SETTINGS_GROUPS: { id: SettingsGroup; label: string }[];
export const DEFAULT_GROUP: SettingsGroup;              // 'conn'
export function normalizeGroup(raw: string | undefined): SettingsGroup;  // 모르는 값 → 'conn'
```

## 제거·배선 변경

| 위치 | 변경 |
|---|---|
| `src/Settings.svelte`, `src/settings.html`, `src/settings.ts` | 삭제 |
| `src/lib/ui/LifeSettingsTab.svelte` | 삭제 (내용은 `settings/` 그룹들로 분산) |
| `vite.config.ts` | rollup input에서 `settings` 엔트리 제거 |
| `src-tauri/tauri.conf.json` | `settings` 윈도우 정의 제거 |
| `src-tauri/src/lib.rs` `apply_content_protection` | 대상 라벨에서 `"settings"` 제거 |
| `src-tauri/src/lib.rs` `on_window_event` | CloseRequested hide 대상에서 `"settings"` 제거 (`"chat"`만 남음) |
| `src-tauri/src/commands.rs` `valid_tab` | `"settings"` 추가 |
| `src-tauri/src/commands.rs` `open_settings_window` | 커맨드 제거 + `lib.rs` invoke_handler 등록 해제 |
| `src-tauri/src/tray.rs` `show_settings` | 제거 → chat 창 show/unminimize/focus + `chat:goto-tab{tab:"settings", target:"conn"}` emit |
| `src/lib/api.ts` `openSettingsWindow` | 제거 |
| `src/Mascot.svelte` "서버 미연결 — 설정 열기" 버튼 | `openSettingsWindow()` → `openChatTab('settings','conn')` |
| `src/App.svelte` | `LifeSettingsTab` → `SettingsTab`, `onGotoTab` 가드에 `'settings'` 추가, `target`을 그룹으로 전달, `Tab` 타입과 방 변경 리셋을 `tab-routing.ts`로 추출 |

**데이터 마이그레이션 없음.** 옮기는 세 섹션이 쓰는 커맨드
(`engine_settings_get/set`, `engine_test`, `image_settings_get/set`, `regenerate_sprite`,
`hub_settings_get`, `hub_connect`, `hub_disconnect`)와 저장 위치는 그대로다. UI 위치만 바뀐다.

기존 마스코트 미연결 안내는 **이미 존재한다**(`Mascot.svelte`의 `{#if !hubOn}` 분기).
새로 추가하는 것이 아니라 목적지만 바꾼다.

## 방문 중 처리 — 유일한 함정

`App.svelte`의 `visibleTabs`는 남의 방 방문 중에 설정 탭을 숨긴다.
사적 탭이 방 주인 것으로 오독되는 것을 막는 기존 규칙이다
(설정 탭에는 내 이름·조직·MBTI가 있어 "○○님의 미니홈피" 헤더 아래 노출되면 오독 위험이 실재한다).

**동작**: 방문 중 트레이 "설정" → `lifeGoto(myLifeId)`로 내 방 복귀 → 설정 탭 `conn` 그룹.
오독 방지 규칙을 지키면서 사용자는 한 번 눌러 닿는다. 방 이동은 이미 구현된 동작이라 추가 복잡도가 작다.

**충돌**: `App.svelte`의 폴링에 이미 다음 코드가 있다.

```ts
const lifeChanged = currentLifeId !== '' && currentLifeId !== v.life.life_id;
if (lifeChanged) tab = 'home';
```

방이 바뀌면 홈으로 강제 이동하므로 **내 방으로 돌아가는 순간 설정 탭 의도가 홈으로 덮어써진다.**
`pendingTab` 플래그를 두고 방 변경 리셋을 한 번 건너뛴다. 결정 로직은 순수 함수로 뽑는다:

```ts
// src/lib/ui/tab-routing.ts
export type Tab = 'home' | 'diary' | 'coach' | 'chat' | 'guestbook' | 'settings';

export function resolveTabAfterLifeChange(
  lifeChanged: boolean,
  pendingTab: Tab | null,
  currentTab: Tab,
): { tab: Tab; pendingTab: Tab | null };
```

| lifeChanged | pendingTab | 결과 tab | 결과 pendingTab |
|---|---|---|---|
| false | null | currentTab (변화 없음) | null |
| false | 'settings' | currentTab (변화 없음) | 'settings' (아직 방이 안 바뀜 — 유지) |
| true | null | 'home' (기존 동작) | null |
| true | 'settings' | 'settings' (홈 리셋 건너뜀) | null (소비됨) |

`pendingTab`은 `chat:goto-tab`으로 `settings`를 받았고 그 시점에 `visiting === true`일 때만 설정한다.
방문 중이 아니면 곧바로 탭을 바꾸므로 `pendingTab`은 쓰지 않는다.

**그룹 target은 지연하지 않는다.** `pendingTab`은 탭만 나르고, 그룹은 이벤트를 받는 즉시
`settingsGroup` 상태에 반영한다 — 설정 탭이 아직 보이지 않는 동안 그룹 값이 바뀌어도
관측되는 효과가 없으므로 두 값을 함께 지연시킬 이유가 없다.

## 스타일 정합

`Settings.svelte`는 `--pastel-lav`/`--frame-bg`/`--ink` 계열 변수를 쓰고
`LifeSettingsTab`은 `--line`/`--surface-inset`/`--text` 계열을 쓴다.
옮길 때 **후자로 통일**한다 — 설정 탭이 미니홈피 창 안에 사는 이상 그 창의 어휘를 따라야 한다.

`src/lib/no-hardcoded-colors.test.ts`가 `<style>` 블록 안의 hex 색을 막는다.
`settings/` 아래 새 `.svelte` 파일은 자동으로 검사 대상에 포함되므로 CSS 변수만 쓴다.
(테마 스킨 점 색상처럼 스크립트에 있는 hex는 검사 대상이 아니다 — 기존과 동일.)

## 테스트

| 종류 | 내용 |
|---|---|
| Rust | `valid_tab("settings") == true` — 기존 `valid_tab` 테스트 확장 |
| Vitest | `normalizeGroup` — 4개 유효 값 + `undefined`/모르는 값 → `'conn'` |
| Vitest | `resolveTabAfterLifeChange` — 위 표 4개 조합 |
| Vitest | `no-hardcoded-colors` — 새 파일 자동 커버 |
| 수동(Windows) | 트레이 "설정" → 미니홈피 창 설정 탭 `conn` 그룹 |
| 수동(Windows) | 남의 방 방문 중 트레이 "설정" → 내 방 복귀 후 설정 탭 (홈으로 튕기지 않음) |
| 수동(Windows) | Life 미연결 상태에서 마스코트 우클릭 → "서버 미연결 — 설정 열기" → `conn` 그룹 |
| 수동(Windows) | 4그룹 전환 + 각 섹션 저장·연결 테스트 동작 확인 |

**Rust 빌드·테스트는 WSL에서 돌릴 수 없다.** `cargo test`와 `npm run tauri dev`는
네이티브 Windows PowerShell에서 확인한다 (a-mate/CLAUDE.md 제약).

## 범위 밖 — 후속 과제

1. **트레이 토글류를 설정 탭에도 노출** — 실시간 조언, 화면 캡처 보호, 잡담 빈도,
   시작 시 실행, 마스코트 표시. Rust `CheckMenuItem` ↔ 프론트 양방향 동기화가 필요하다
   (`settings:changed` 이벤트가 이미 있으나 역방향 갱신 경로가 없다).
2. **방 이동을 미니홈피 홈 탭에도 추가** — 마스코트 유지가 원칙이므로 하지 않는다.
   창을 주로 쓰는 사용자 요청이 실제로 나오면 재검토한다.
