---
status: done
archived: 2026-07-25
---

# 설정 표면 통합 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 별도 `settings` 창(LLM 엔진·캐릭터 이미지·Life Server)을 미니홈피 창의 설정 탭으로 흡수해 설정 진입점을 하나로 만든다.

**Architecture:** 설정 탭을 상단 pill 서브탭 4그룹(연결/나/공개/모양)으로 재구성한다. 기존 `LifeSettingsTab.svelte`(116줄, 5개 관심사)를 그룹별 컴포넌트로 쪼개고, 별도 창의 3섹션을 `ConnectionGroup`으로 옮긴 뒤 창·엔트리·죽은 배선을 제거한다. 트레이와 마스코트는 `chat:goto-tab{tab:"settings", target:"conn"}` 딥링크로 설정 탭을 연다.

**Tech Stack:** Tauri v2 (Rust) + Svelte 5 runes + Vite + TypeScript, Vitest(프론트) / `cargo test`(Rust)

**설계 스펙:** [`docs/archive/design/a-mate/specs/2026-07-25-settings-consolidation-design.md`](../specs/2026-07-25-settings-consolidation-design.md)

## Global Constraints

- **플랫폼은 Windows 전용.** macOS/Linux 분기를 만들지 않는다.
- **WSL에서 빌드·실행 금지.** `cargo test`, `npm run tauri dev/build`는 네이티브 Windows PowerShell에서만 돌린다. WSL에서 작업 중이라면 Rust 검증 스텝은 **미검증으로 표시하고 Task 6의 수동 체크리스트로 넘긴다** — 통과했다고 적지 않는다.
- **Tauri v1 API 금지** (`SystemTray`, `tauri::updater`, `WindowBuilder` 등).
- **Svelte 5 runes만 사용** — `$state`, `$derived`, `$effect`, `$props()`. 컴포넌트 props는 `let { x }: {x: T} = $props();` 형태(예: `src/lib/ui/GuestbookTab.svelte:3`).
- **snippet(`{@render}`) 도입 금지** — 코드베이스에 사용례가 없다. 섹션 껍데기는 컴포넌트가 아니라 `:global` 스타일로 제공한다.
- **`<style>` 블록에 hex 색상 금지.** `src/lib/no-hardcoded-colors.test.ts`가 실패한다. CSS 변수만 쓴다. 사용 가능한 변수: `--text --text-soft --line --surface-inset --accent --accent-ink --accent-strong --danger --cream --cream-ink --lav-surface --lav-ink --frame --frame-2 --panel2 --radius-s --radius-m --radius-l --shadow-soft`.
- **전경 텍스트에 `color: var(--accent)` 금지** — 라이트 테마에서 저대비다. `--accent-strong`을 쓴다 (`no-hardcoded-colors.test.ts`가 검사).
- **CSS 변수 어휘는 `--line`/`--surface-inset`/`--text` 계열로 통일.** 옮겨오는 `Settings.svelte`의 `--pastel-lav`/`--frame-bg`/`--ink` 계열은 쓰지 않는다.
- **커밋 메시지는 Conventional Commits + 영어**, `Co-Authored-By` 트레일러 금지.
- **저장 키·커맨드·계약 변경 없음.** 데이터 마이그레이션이 필요한 작업이 나오면 계획을 벗어난 것이다.
- **문서는 한국어**, UI 문구도 한국어.

---

## File Structure

| 파일 | 책임 |
|---|---|
| `src/lib/ui/settings/groups.ts` | **생성** — 그룹 id·라벨 목록, 기본 그룹, 딥링크 target 정규화 (순수) |
| `src/lib/ui/settings/status.ts` | **생성** — `Status` 타입 + idle/busy/ok/err 생성자 (순수) |
| `src/lib/ui/tab-routing.ts` | **생성** — `Tab` 타입, `isTab`, 방 변경 시 탭 결정 (순수) |
| `src/lib/ui/settings/StatusLine.svelte` | **생성** — 상태 한 줄 표시 (프레젠테이션 전용) |
| `src/lib/ui/settings/SettingsTab.svelte` | **생성** — 서브탭 렌더 + 그룹 선택 + 공용 섹션 `:global` 스타일 |
| `src/lib/ui/settings/ConnectionGroup.svelte` | **생성** — Life Server + LLM 엔진 + 캐릭터 이미지 |
| `src/lib/ui/settings/MeGroup.svelte` | **생성** — 개인정보 + 주인 메모리 |
| `src/lib/ui/settings/PrivacyGroup.svelte` | **생성** — 일촌 관리 + 다이어리 공개 범위 |
| `src/lib/ui/settings/LookGroup.svelte` | **생성** — 테마 |
| `src/lib/ui/LifeSettingsTab.svelte` | **삭제** — 내용이 위 4그룹으로 분산 |
| `src/Settings.svelte`, `src/settings.html`, `src/settings.ts` | **삭제** |
| `src/lib/api.ts` | **수정** — `EngineSettings` 래퍼 추가, `openSettingsWindow` 제거 |
| `src/App.svelte` | **수정** — `SettingsTab` 사용, `Tab` 타입 import, `pendingTab` 처리, `onGotoTab` 가드 |
| `src/Mascot.svelte` | **수정** — 미연결 버튼 목적지 변경 |
| `vite.config.ts` | **수정** — `settings` 엔트리 제거 |
| `src-tauri/tauri.conf.json` | **수정** — `settings` 윈도우 정의 제거 |
| `src-tauri/src/commands.rs` | **수정** — `valid_tab`에 `"settings"`, `open_settings_window` 제거 |
| `src-tauri/src/tray.rs` | **수정** — `show_settings` → 설정 탭 딥링크 |
| `src-tauri/src/lib.rs` | **수정** — `"settings"` 라벨 참조 2곳 + invoke_handler 등록 제거 |

---

## Task 1: 설정 탭 순수 모듈

그룹 목록·상태 헬퍼·탭 라우팅을 `.svelte` 밖 순수 함수로 만든다. 소비자는 아직 없다 — 이후 태스크가 쓴다.

**Files:**
- Create: `a-mate/src/lib/ui/settings/groups.ts`
- Create: `a-mate/src/lib/ui/settings/status.ts`
- Create: `a-mate/src/lib/ui/tab-routing.ts`
- Test: `a-mate/src/lib/ui/settings/groups.test.ts`
- Test: `a-mate/src/lib/ui/settings/status.test.ts`
- Test: `a-mate/src/lib/ui/tab-routing.test.ts`

**Interfaces:**
- Consumes: 없음 (첫 태스크)
- Produces:
  - `groups.ts`: `type SettingsGroup = 'conn'|'me'|'privacy'|'look'`, `SETTINGS_GROUPS: {id: SettingsGroup; label: string}[]`, `DEFAULT_GROUP: SettingsGroup`, `normalizeGroup(raw: string | undefined | null): SettingsGroup`
  - `status.ts`: `type StatusKind = 'idle'|'ok'|'err'|'busy'`, `interface Status {kind: StatusKind; text: string}`, `IDLE: Status`, `busy(text: string): Status`, `ok(text: string): Status`, `err(e: unknown): Status`
  - `tab-routing.ts`: `type Tab = 'home'|'diary'|'coach'|'chat'|'guestbook'|'settings'`, `isTab(raw: string): raw is Tab`, `resolveTabAfterLifeChange(lifeChanged: boolean, pendingTab: Tab|null, currentTab: Tab): {tab: Tab; pendingTab: Tab|null}`

- [ ] **Step 1: 실패하는 테스트 3개 작성**

`a-mate/src/lib/ui/settings/groups.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { SETTINGS_GROUPS, DEFAULT_GROUP, normalizeGroup } from './groups';

describe('SETTINGS_GROUPS', () => {
  it('연결·나·공개·모양 4그룹이 이 순서로', () => {
    expect(SETTINGS_GROUPS.map((g) => g.id)).toEqual(['conn', 'me', 'privacy', 'look']);
    expect(SETTINGS_GROUPS.map((g) => g.label)).toEqual(['연결', '나', '공개', '모양']);
  });
  it('기본 그룹은 연결 — 처음 쓸 때 가장 먼저 필요한 설정', () => {
    expect(DEFAULT_GROUP).toBe('conn');
  });
});

describe('normalizeGroup', () => {
  it('유효한 그룹 id는 그대로', () => {
    expect(normalizeGroup('conn')).toBe('conn');
    expect(normalizeGroup('me')).toBe('me');
    expect(normalizeGroup('privacy')).toBe('privacy');
    expect(normalizeGroup('look')).toBe('look');
  });
  it('모르는 값·빈값·undefined·null은 기본 그룹으로', () => {
    expect(normalizeGroup('bogus')).toBe('conn');
    expect(normalizeGroup('')).toBe('conn');
    expect(normalizeGroup(undefined)).toBe('conn');
    expect(normalizeGroup(null)).toBe('conn');
  });
});
```

`a-mate/src/lib/ui/settings/status.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { IDLE, busy, ok, err } from './status';

describe('Status 생성자', () => {
  it('IDLE은 빈 텍스트 — StatusLine이 아무것도 렌더하지 않는 상태', () => {
    expect(IDLE).toEqual({ kind: 'idle', text: '' });
  });
  it('busy/ok는 kind와 문구를 그대로 담는다', () => {
    expect(busy('저장 중…')).toEqual({ kind: 'busy', text: '저장 중…' });
    expect(ok('저장했어요.')).toEqual({ kind: 'ok', text: '저장했어요.' });
  });
  it('err는 무엇이든 문자열로 — catch(e)의 e 타입이 unknown이라서', () => {
    expect(err(new Error('boom'))).toEqual({ kind: 'err', text: 'Error: boom' });
    expect(err('네트워크 오류')).toEqual({ kind: 'err', text: '네트워크 오류' });
    expect(err(404)).toEqual({ kind: 'err', text: '404' });
  });
});
```

`a-mate/src/lib/ui/tab-routing.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { isTab, resolveTabAfterLifeChange } from './tab-routing';

describe('isTab', () => {
  it('아는 탭은 통과', () => {
    for (const t of ['home', 'diary', 'coach', 'chat', 'guestbook', 'settings']) {
      expect(isTab(t)).toBe(true);
    }
  });
  it('모르는 탭은 거부 — 백엔드가 보낸 낯선 값을 무시해야 한다', () => {
    expect(isTab('bogus')).toBe(false);
    expect(isTab('')).toBe(false);
  });
});

describe('resolveTabAfterLifeChange', () => {
  it('방이 안 바뀌면 현재 탭 유지 (pendingTab도 유지)', () => {
    expect(resolveTabAfterLifeChange(false, null, 'coach')).toEqual({ tab: 'coach', pendingTab: null });
    expect(resolveTabAfterLifeChange(false, 'settings', 'home')).toEqual({ tab: 'home', pendingTab: 'settings' });
  });
  it('방이 바뀌면 홈으로 리셋 — 방문 컨텍스트 전환 기존 동작', () => {
    expect(resolveTabAfterLifeChange(true, null, 'coach')).toEqual({ tab: 'home', pendingTab: null });
  });
  it('pendingTab이 있으면 홈 리셋을 이긴다 — 트레이 설정이 방문 중에 눌린 경우', () => {
    expect(resolveTabAfterLifeChange(true, 'settings', 'home')).toEqual({ tab: 'settings', pendingTab: null });
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run (`a-mate/`에서):
```bash
npm test -- src/lib/ui/settings src/lib/ui/tab-routing.test.ts
```
Expected: FAIL — `Failed to resolve import "./groups"` / `"./status"` / `"./tab-routing"`

- [ ] **Step 3: `groups.ts` 작성**

```ts
/** 설정 탭 서브 그룹. id는 chat:goto-tab의 target으로도 쓰인다(트레이·마스코트 딥링크). */
export type SettingsGroup = 'conn' | 'me' | 'privacy' | 'look';

export const SETTINGS_GROUPS: { id: SettingsGroup; label: string }[] = [
  { id: 'conn', label: '연결' },
  { id: 'me', label: '나' },
  { id: 'privacy', label: '공개' },
  { id: 'look', label: '모양' },
];

/** 처음 쓸 때 가장 먼저 필요한 설정이 연결(Life Server·엔진)이다. */
export const DEFAULT_GROUP: SettingsGroup = 'conn';

/** 딥링크 target 정규화 — 백엔드는 target 내용을 해석하지 않으므로 여기서 방어한다. */
export function normalizeGroup(raw: string | undefined | null): SettingsGroup {
  return SETTINGS_GROUPS.some((g) => g.id === raw) ? (raw as SettingsGroup) : DEFAULT_GROUP;
}
```

- [ ] **Step 4: `status.ts` 작성**

```ts
/** 저장·연결 등의 진행/결과 상태. Settings/LifeSettings에 세 번 중복됐던 패턴을 한 곳으로. */
export type StatusKind = 'idle' | 'ok' | 'err' | 'busy';
export interface Status {
  kind: StatusKind;
  text: string;
}

/** text가 빈 문자열이면 StatusLine이 아무것도 렌더하지 않는다. */
export const IDLE: Status = { kind: 'idle', text: '' };

export const busy = (text: string): Status => ({ kind: 'busy', text });
export const ok = (text: string): Status => ({ kind: 'ok', text });
/** catch(e)의 e는 unknown — 무엇이 와도 문자열로 만든다. */
export const err = (e: unknown): Status => ({ kind: 'err', text: `${e}` });
```

- [ ] **Step 5: `tab-routing.ts` 작성**

```ts
/** 미니홈피 창 탭. App.svelte와 트레이 딥링크(chat:goto-tab)가 공유한다. */
export type Tab = 'home' | 'diary' | 'coach' | 'chat' | 'guestbook' | 'settings';

const TAB_IDS: readonly string[] = ['home', 'diary', 'coach', 'chat', 'guestbook', 'settings'];

/** chat:goto-tab의 tab 문자열이 아는 탭인지. 모르면 무시한다. */
export function isTab(raw: string): raw is Tab {
  return TAB_IDS.includes(raw);
}

/**
 * 방이 바뀌면 홈으로 리셋한다(방문 컨텍스트 전환 — 기존 동작).
 * 단 pendingTab이 있으면 그 의도가 이긴다: 트레이 "설정"이 남의 방 방문 중에 눌려
 * 내 방으로 돌아오는 중인 경우, 리셋이 설정 탭 의도를 덮어써선 안 된다.
 */
export function resolveTabAfterLifeChange(
  lifeChanged: boolean,
  pendingTab: Tab | null,
  currentTab: Tab,
): { tab: Tab; pendingTab: Tab | null } {
  if (!lifeChanged) return { tab: currentTab, pendingTab };
  if (pendingTab) return { tab: pendingTab, pendingTab: null };
  return { tab: 'home', pendingTab: null };
}
```

- [ ] **Step 6: 테스트 통과 확인**

Run:
```bash
npm test -- src/lib/ui/settings src/lib/ui/tab-routing.test.ts
```
Expected: PASS — 3개 파일, 10개 테스트

- [ ] **Step 7: 전체 프론트 테스트 통과 확인 (회귀 없음)**

Run:
```bash
npm test
```
Expected: PASS — 기존 테스트 전부 + 새 10개

- [ ] **Step 8: 커밋**

```bash
git add src/lib/ui/settings/groups.ts src/lib/ui/settings/status.ts src/lib/ui/tab-routing.ts \
        src/lib/ui/settings/groups.test.ts src/lib/ui/settings/status.test.ts src/lib/ui/tab-routing.test.ts
git commit -m "feat(a-mate): add pure modules for settings groups and tab routing"
```

---

## Task 2: Rust — `valid_tab`이 settings를 허용

딥링크가 설정 탭을 가리킬 수 있게 백엔드 화이트리스트를 넓힌다. 아직 호출자가 없어 동작 변화는 없다.

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` — `valid_tab` (약 1011행) + `mod tests` 내 테스트 추가

**Interfaces:**
- Consumes: 없음
- Produces: `valid_tab("settings") == true` — Task 5의 트레이 딥링크가 이 화이트리스트를 통과해야 한다

- [ ] **Step 1: 실패하는 테스트 작성**

`commands.rs`의 `mod tests` 안, `goto_tab_payload_serializes_target_optionally` 테스트 바로 위에 추가:

```rust
    #[test]
    fn valid_tab_allows_deeplinkable_tabs_including_settings() {
        for tab in ["home", "diary", "coach", "chat", "settings"] {
            assert!(valid_tab(tab), "{tab}은 딥링크 대상이어야 한다");
        }
        // 방명록은 방 문맥이 필요해 딥링크 대상이 아니다 (프론트 onGotoTab 가드와 일치)
        assert!(!valid_tab("guestbook"));
        assert!(!valid_tab("bogus"));
        assert!(!valid_tab(""));
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run (**Windows PowerShell**, `a-mate/`에서):
```powershell
cargo test -p agent-mentor-app valid_tab_allows_deeplinkable_tabs_including_settings
```
Expected: FAIL — `assertion failed: valid_tab(tab)` / `settings은 딥링크 대상이어야 한다`

> WSL에서 작업 중이면 이 스텝을 **미검증**으로 남기고 Task 6 수동 체크리스트에 기록한다. 통과했다고 적지 않는다.

- [ ] **Step 3: `valid_tab`에 `"settings"` 추가**

`commands.rs`의 `valid_tab`을 다음으로 교체:

```rust
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn valid_tab(tab: &str) -> bool {
    // settings는 트레이·마스코트가 설정 탭으로 딥링크할 때 쓴다(target=그룹 id).
    matches!(tab, "home" | "diary" | "coach" | "chat" | "settings")
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run (**Windows PowerShell**):
```powershell
cargo test -p agent-mentor-app valid_tab_allows_deeplinkable_tabs_including_settings
```
Expected: PASS

- [ ] **Step 5: 워크스페이스 전체 테스트 (회귀 없음)**

Run (**Windows PowerShell**):
```powershell
cargo test
```
Expected: PASS

- [ ] **Step 6: 커밋**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(a-mate): allow settings as a deep-linkable chat tab"
```

---

## Task 3: 설정 탭을 4그룹으로 재구성하고 별도 창 3섹션 흡수

이번 작업의 본체. `LifeSettingsTab.svelte`를 그룹별로 쪼개고, `Settings.svelte`의 LLM 엔진·캐릭터 이미지·Life Server를 `ConnectionGroup`으로 옮긴다. **이 태스크가 끝나면 설정 탭만으로 모든 설정에 닿을 수 있다.** 별도 창은 아직 존재하지만(Task 5에서 제거) 같은 커맨드를 쓰므로 동작이 어긋나지 않는다.

**Files:**
- Modify: `a-mate/src/lib/api.ts` — `EngineSettings` 인터페이스 + 래퍼 3개 추가
- Create: `a-mate/src/lib/ui/settings/StatusLine.svelte`
- Create: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte`
- Create: `a-mate/src/lib/ui/settings/MeGroup.svelte`
- Create: `a-mate/src/lib/ui/settings/PrivacyGroup.svelte`
- Create: `a-mate/src/lib/ui/settings/LookGroup.svelte`
- Create: `a-mate/src/lib/ui/settings/SettingsTab.svelte`
- Delete: `a-mate/src/lib/ui/LifeSettingsTab.svelte`
- Modify: `a-mate/src/App.svelte` — import 교체 (8행), 렌더 교체 (181행)

**Interfaces:**
- Consumes: Task 1의 `SETTINGS_GROUPS`, `DEFAULT_GROUP`, `normalizeGroup`, `Status`, `IDLE`, `busy`, `ok`, `err`
- Produces:
  - `SettingsTab.svelte` props: `let { group = DEFAULT_GROUP }: { group?: SettingsGroup } = $props()`
  - `StatusLine.svelte` props: `let { status }: { status: Status } = $props()`
  - `api.ts`: `interface EngineSettings {url: string; key: string; model: string; source: 'store'|'env'|'none'}`, `engineSettingsGet(): Promise<EngineSettings>`, `engineSettingsSet(url, key, model): Promise<void>`, `engineTest(url, key, model): Promise<string>`

- [ ] **Step 1: `api.ts`에 엔진 설정 래퍼 추가**

`src/lib/api.ts`의 `ImageSettings` 블록(약 280행, `/** 캐릭터 이미지 모델 설정 …`) **바로 위**에 추가:

```ts
/** 텍스트 LLM 엔진 설정 — 일기·한마디·잡담·채팅이 쓰는 OpenAI 호환 엔드포인트. */
export interface EngineSettings { url: string; key: string; model: string; source: 'store' | 'env' | 'none' }

export const engineSettingsGet = () => invoke<EngineSettings>('engine_settings_get');
export const engineSettingsSet = (url: string, key: string, model: string) =>
  invoke<void>('engine_settings_set', { url, key, model });
/** 연결 확인 — 성공 시 사람이 읽는 메시지를 돌려준다. */
export const engineTest = (url: string, key: string, model: string) =>
  invoke<string>('engine_test', { url, key, model });
```

- [ ] **Step 2: `StatusLine.svelte` 작성**

`src/lib/ui/settings/StatusLine.svelte`:

```svelte
<script lang="ts">
  import type { Status } from './status';
  let { status }: { status: Status } = $props();
</script>

{#if status.text}<p class="status" data-kind={status.kind}>{status.text}</p>{/if}

<style>
  .status{margin:10px 0 0;font-size:12px;padding:7px 11px;border-radius:8px;background:var(--surface-inset);color:var(--text-soft);word-break:break-all}
  .status[data-kind=ok]{background:var(--lav-surface);color:var(--lav-ink)}
  .status[data-kind=err]{color:var(--danger)}
</style>
```

- [ ] **Step 3: `ConnectionGroup.svelte` 작성 — Life Server + LLM 엔진 + 캐릭터 이미지**

`src/lib/ui/settings/ConnectionGroup.svelte`. `Settings.svelte`의 세 섹션을 옮기되 raw `invoke` 대신 `api.ts` 래퍼를, 상태는 `status.ts`를, 변수는 `--line`/`--surface-inset`/`--text` 계열을 쓴다. Life Server를 **맨 위**에 둔다 (딥링크가 노리는 섹션).

```svelte
<script lang="ts">
  import {
    engineSettingsGet, engineSettingsSet, engineTest,
    hubConnect, hubDisconnect, hubSettingsGet,
    imageSettingsGet, imageSettingsSet, regenerateSprite,
    type EngineSettings, type HubSettings, type ImageSettings,
  } from '../../api';
  import { IDLE, busy, err, ok, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  // --- Life Server (방 방문·에이전트 위치) ---
  let hub = $state<HubSettings | null>(null);
  let hubUrl = $state(''), hubApiKey = $state('');
  let hubStatus = $state<Status>(IDLE);
  async function loadHub(){ try { hub = await hubSettingsGet(); hubUrl = hub.url; hubApiKey = hub.api_key; } catch { /* 미설정 */ } }
  loadHub();
  async function connectHub(){
    hubStatus = busy('연결 중…');
    try { hub = await hubConnect(hubUrl, hubApiKey); hubStatus = ok(`연결 완료 — 개인 방이 만들어졌어요 (${hub.life_id})`); }
    catch(e){ hubStatus = err(e); }
  }
  async function disconnectHub(){
    hubStatus = busy('연결 종료 중…');
    try { hub = await hubDisconnect(); hubStatus = ok('Life 서버 연결을 종료했어요. 서버의 방과 공개 데이터는 유지됩니다.'); }
    catch(e){ hubStatus = err(e); }
  }

  // --- 텍스트 LLM 엔진 ---
  let eng = $state<EngineSettings>({ url:'', key:'', model:'', source:'none' });
  let engStatus = $state<Status>(IDLE);
  async function loadEngine(){
    try { eng = await engineSettingsGet(); } catch(e){ engStatus = err(`설정을 불러오지 못했어요: ${e}`); }
  }
  loadEngine();
  async function saveEngine(){
    engStatus = busy('저장 중…');
    try { await engineSettingsSet(eng.url, eng.key, eng.model); await loadEngine(); engStatus = ok('저장했어요. 다음 호출부터 적용됩니다.'); }
    catch(e){ engStatus = err(e); }
  }
  async function testEngine(){
    engStatus = busy('연결 확인 중…');
    try { engStatus = ok(await engineTest(eng.url, eng.key, eng.model)); }
    catch(e){ engStatus = err(`연결 실패: ${e}`); }
  }
  const engSourceLabel = $derived(
    eng.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : eng.source === 'env' ? '.env 파일 값 사용 중 (아래에 저장하면 이 값을 덮어씁니다)'
    : '엔진 미설정 — 일기·채팅이 비활성 상태예요',
  );

  // --- 캐릭터 이미지 모델 (사내 LLM은 그림을 못 그려 텍스트 엔진과 분리) ---
  let img = $state<ImageSettings>({ url:'', key:'', model:'', source:'none' });
  let imgStatus = $state<Status>(IDLE);
  async function loadImage(){
    try { img = await imageSettingsGet(); } catch(e){ imgStatus = err(`이미지 설정을 불러오지 못했어요: ${e}`); }
  }
  loadImage();
  async function saveImage(){
    imgStatus = busy('저장 중…');
    try { await imageSettingsSet(img.url, img.key, img.model); await loadImage(); imgStatus = ok('저장했어요.'); }
    catch(e){ imgStatus = err(e); }
  }
  async function regenerate(){
    imgStatus = busy('캐릭터 그리는 중… 수십 초 걸릴 수 있어요');
    try { await regenerateSprite(); imgStatus = ok('새 캐릭터로 바뀌었어요 — 마스코트를 확인해보세요!'); }
    catch(e){ imgStatus = err(`생성 실패: ${e}`); }
  }
  const imgSourceLabel = $derived(
    img.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : img.source === 'env' ? '.env 값 사용 중'
    : '미설정 — 캐릭터가 기본 그림으로 표시됩니다',
  );
</script>

<section>
  <h2>Life Server</h2>
  <p class="hint">방 방문·에이전트 위치를 관장하는 life-server에 연결합니다 (hub와 별개 프로세스).</p>
  {#if hub}
    <p class="source" data-kind={hub.connected ? 'store' : 'none'}>
      {hub.connected ? `연결됨 — 내 방: ${hub.life_id}` : '미연결'}
    </p>
  {/if}
  <div class="fields">
    <label class="field"><span>서버 URL</span>
      <input type="text" bind:value={hubUrl} placeholder="http://192.168.0.10:8001" spellcheck="false"/></label>
    <label class="field"><span>API 키 <em>(선택)</em></span>
      <input type="password" bind:value={hubApiKey} placeholder="비워두면 인증 없이 (관문 켜진 서버만 필요)" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={connectHub} disabled={hubStatus.kind==='busy'}>연결</button>
    {#if hub?.connected}<button onclick={disconnectHub} disabled={hubStatus.kind==='busy'}>연결 종료</button>{/if}
  </div>
  <StatusLine status={hubStatus}/>
</section>

<section>
  <h2>LLM 엔진</h2>
  <p class="hint">일기·한마디·잡담·채팅이 사용할 OpenAI 호환 엔드포인트를 지정합니다.</p>
  <p class="source" data-kind={eng.source}>{engSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>엔드포인트 URL</span>
      <input type="text" bind:value={eng.url} placeholder="http://localhost:4444/v1" spellcheck="false"/></label>
    <label class="field"><span>API 키 <em>(선택)</em></span>
      <input type="password" bind:value={eng.key} placeholder="비워두면 인증 없이 호출" spellcheck="false"/></label>
    <label class="field"><span>모델명</span>
      <input type="text" bind:value={eng.model} placeholder="gpt-4o-mini" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveEngine} disabled={engStatus.kind==='busy'}>저장</button>
    <button onclick={testEngine} disabled={engStatus.kind==='busy'}>연결 테스트</button>
  </div>
  <StatusLine status={engStatus}/>
  <p class="hint2">URL을 비우고 저장하면 .env(AGENT_MENTOR_ENGINE_*) 값으로 되돌아갑니다.</p>
</section>

<section>
  <h2>캐릭터 이미지</h2>
  <p class="hint">마스코트 캐릭터를 그릴 이미지 생성 모델입니다. 사내 LLM은 그림을 못 그리므로 위 텍스트 엔진과 따로 지정합니다. 사람마다 한 번 생성해 캐시하므로 이후에는 호출하지 않습니다.</p>
  <p class="source" data-kind={img.source}>{imgSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>엔드포인트 URL</span>
      <input type="text" bind:value={img.url} placeholder="https://openrouter.ai/api/v1" spellcheck="false"/></label>
    <label class="field"><span>API 키</span>
      <input type="password" bind:value={img.key} placeholder="sk-or-..." spellcheck="false"/></label>
    <label class="field"><span>이미지 모델</span>
      <input type="text" bind:value={img.model} placeholder="google/gemini-2.5-flash-image" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind==='busy'}>저장</button>
    <button onclick={regenerate} disabled={imgStatus.kind==='busy'}>캐릭터 재생성</button>
  </div>
  <StatusLine status={imgStatus}/>
</section>

<style>
  .source{margin:8px 0 0;font-size:12px;padding:7px 11px;border-radius:8px;background:var(--lav-surface);color:var(--lav-ink)}
  .source[data-kind=none]{background:var(--surface-inset);color:var(--danger)}
  .source[data-kind=env]{background:var(--cream);color:var(--cream-ink)}
  .fields{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
  .field{display:flex;flex-direction:column;gap:4px}
  .field>span{font-size:12px;color:var(--text-soft)}
  .field em{font-style:normal;opacity:.7}
  .field input{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .actions{display:flex;gap:8px;margin-top:12px}
  .actions button{border:0;border-radius:99px;padding:7px 14px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font:inherit}
  .actions button.primary{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .actions button:disabled{opacity:.55;cursor:default}
  .hint2{color:var(--text-soft);font-size:11px;margin:8px 0 0}
</style>
```

- [ ] **Step 4: `MeGroup.svelte` 작성 — 개인정보 + 주인 메모리**

`LifeSettingsTab.svelte`의 개인정보(스크립트 5–26행, 마크업 60–71행)와 주인 메모리(스크립트 38–58행, 마크업 73–96행)를 옮긴다. 상태는 `status.ts`/`StatusLine`으로 바꾼다.

```svelte
<script lang="ts">
  import {
    memoryAdd, memoryDelete, memoryList, memoryUpdate,
    profileGet, profileSet, regenerateSprite,
    type Memory, type Profile,
  } from '../../api';
  import { IDLE, busy, err, ok, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  const MBTI_OPTIONS = ['', 'ISTJ','ISFJ','INFJ','INTJ','ISTP','ISFP','INFP','INTP','ESTP','ESFP','ENFP','ENTP','ESTJ','ESFJ','ENFJ','ENTJ'];
  let prof = $state<Profile>({ name: '', org: '', uuid: '', mbti: '' });
  let profMbti = $state('');
  let profStatus = $state<Status>(IDLE);
  async function loadProfile(){ try { prof = await profileGet(); profMbti = prof.mbti; } catch(e){ profStatus = err(e); } }
  loadProfile();
  async function saveProfile(){
    profStatus = busy('저장 중…');
    const mbtiChanged = profMbti !== prof.mbti;
    try {
      prof = await profileSet(prof.name, prof.org, profMbti);
      profMbti = prof.mbti;
      if (mbtiChanged) {
        profStatus = busy('MBTI가 바뀌어 캐릭터를 다시 그리는 중… (수십 초)');
        try { await regenerateSprite(); profStatus = ok('저장 + 새 캐릭터 반영 완료'); }
        catch(e){ profStatus = ok(`저장됨. 캐릭터 재생성은 실패(연결 그룹의 이미지 모델 설정 확인): ${e}`); }
      } else {
        profStatus = ok('저장했어요.');
      }
    } catch(e){ profStatus = err(e); }
  }

  let memories = $state<Memory[]>([]);
  let memInput = $state(''), memEditId = $state<number | null>(null), memEditText = $state('');
  let memStatus = $state<Status>(IDLE);
  async function loadMemories(){ try { memories = await memoryList(); } catch(e){ memStatus = err(e); } }
  loadMemories();
  async function addMemory(){
    const t = memInput.trim(); if(!t) return;
    try { await memoryAdd(t); memInput=''; memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
  function startEdit(m: Memory){ memEditId = m.id; memEditText = m.text; }
  async function saveEdit(){
    if(memEditId==null) return;
    const t = memEditText.trim(); if(!t) return;
    try { await memoryUpdate(memEditId, t); memEditId=null; memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
  async function removeMemory(id: number){
    try { await memoryDelete(id); memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
</script>

<section>
  <h2>개인정보</h2>
  <p class="hint">a-hub 연결과 마스코트 캐릭터에 쓰입니다. 아이디는 자동 부여되며 바뀌지 않아요.</p>
  <div class="fields">
    <label class="field"><span>이름</span><input type="text" bind:value={prof.name} placeholder="예: 준녕" spellcheck="false"/></label>
    <label class="field"><span>조직</span><input type="text" bind:value={prof.org} placeholder="S/W 혁신팀" spellcheck="false"/></label>
    <label class="field"><span>아이디</span><input type="text" value={prof.uuid} readonly title="자동 부여된 고유 ID"/></label>
    <label class="field"><span>MBTI <em>(선택)</em></span>
      <select bind:value={profMbti}>{#each MBTI_OPTIONS as m}<option value={m}>{m === '' ? '미설정' : m}</option>{/each}</select></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveProfile} disabled={profStatus.kind==='busy'}>저장</button>
  </div>
  <StatusLine status={profStatus}/>
  <p class="hint2">MBTI를 바꾸고 저장하면 그 성향에 맞춰 캐릭터를 다시 그립니다. (연결 그룹의 이미지 모델 설정 필요)</p>
</section>

<section>
  <h2>주인 메모리</h2>
  <p class="hint">마스코트가 기억할 나에 대한 사실이에요. 채팅에서 "기억해둬"라고 하거나 여기서 직접 추가할 수 있어요. (엔진으로 전송됩니다)</p>
  <div class="memadd">
    <input type="text" bind:value={memInput} placeholder="예: 나는 비건이야" onkeydown={(e)=>{if(e.key==='Enter')addMemory();}}/>
    <button class="primary" onclick={addMemory}>추가</button>
  </div>
  <ul class="memlist">
    {#each memories as m (m.id)}
      <li>
        {#if memEditId===m.id}
          <input type="text" bind:value={memEditText} onkeydown={(e)=>{if(e.key==='Enter')saveEdit();}}/>
          <button onclick={saveEdit}>저장</button>
          <button onclick={()=>memEditId=null}>취소</button>
        {:else}
          <span class="memtext">{m.text}</span>
          <span class="memdate">{m.created_at}</span>
          <button onclick={()=>startEdit(m)}>편집</button>
          <button onclick={()=>removeMemory(m.id)}>삭제</button>
        {/if}
      </li>
    {:else}
      <li class="memempty">아직 기억한 게 없어요.</li>
    {/each}
  </ul>
  <StatusLine status={memStatus}/>
</section>

<style>
  .fields{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
  .field{display:flex;flex-direction:column;gap:4px}
  .field>span{font-size:12px;color:var(--text-soft)}
  .field em{font-style:normal;opacity:.7}
  .field input,.field select{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .field input[readonly]{opacity:.7;cursor:default}
  .actions{display:flex;gap:8px;margin-top:12px}
  .actions button{border:0;border-radius:99px;padding:7px 14px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font:inherit}
  .actions button.primary{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .actions button:disabled{opacity:.55;cursor:default}
  .hint2{color:var(--text-soft);font-size:11px;margin:8px 0 0}
  .memadd{display:flex;gap:8px;margin-top:12px}
  .memadd input{flex:1;border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .memadd button{border:0;border-radius:99px;padding:7px 14px;background:var(--accent);color:var(--accent-ink);font-weight:700;cursor:pointer;font-size:inherit}
  .memlist{list-style:none;padding:0;margin:12px 0 0;display:flex;flex-direction:column;gap:6px}
  .memlist li{display:flex;align-items:center;gap:8px;padding:8px 10px;background:var(--cream);color:var(--cream-ink);border-radius:8px}
  .memlist .memtext{flex:1}
  .memlist .memdate{font-size:11px;color:var(--text-soft)}
  .memlist input{flex:1;border:1px solid var(--line);border-radius:6px;padding:6px 8px;background:var(--surface-inset);color:var(--text);font:inherit}
  .memlist button{border:0;border-radius:99px;padding:5px 10px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font-size:12px}
  .memempty{color:var(--text-soft);justify-content:center}
</style>
```

- [ ] **Step 5: `PrivacyGroup.svelte` 작성 — 일촌 관리 + 다이어리 공개 범위**

`LifeSettingsTab.svelte`의 스크립트 27–31행 + 마크업(72행의 "일촌 관리"·"다이어리 공개 범위" 부분)을 옮긴다.

```svelte
<script lang="ts">
  import {
    getDiary, lifeContentAccess, lifePeople, lifeSetContentVisibility,
    lifeSetDiaryVisibility, lifeSetFriend, lifeView, listDiaryDates,
    type ContentVisibility, type LifePerson,
  } from '../../api';
  import { IDLE, err, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  // 공개 범위 타입은 api.ts의 ContentVisibility가 원본 — 로컬에 다시 정의하지 않는다.
  const VISIBILITIES: [ContentVisibility, string][] = [['private','비공개'],['friends','일촌공개'],['public','전체공개']];

  let people = $state<LifePerson[]>([]);
  let status = $state<Status>(IDLE);
  let visibility = $state<ContentVisibility>((localStorage.getItem('life-diary-visibility') as ContentVisibility) || 'private');
  let sharing = $state(false);

  async function load(){
    try {
      people = (await lifePeople()).people;
      const view = await lifeView();
      visibility = (await lifeContentAccess(view.me.my_life_id)).features.diary.visibility;
    } catch(e){ status = err(e); }
  }
  load();

  async function toggle(person: LifePerson){ await lifeSetFriend(person.agent_id, !person.is_friend); await load(); }

  async function setVisibility(next: ContentVisibility){
    sharing = true; status = IDLE;
    try {
      await lifeSetContentVisibility('diary', next);
      if (next !== 'private') {
        const dates = await listDiaryDates();
        await Promise.all(dates.map(async (date) => lifeSetDiaryVisibility(date, (await getDiary(date)) || '', next)));
      }
      visibility = next;
      localStorage.setItem('life-diary-visibility', next);
    } catch(e){ status = err(e); }
    finally { sharing = false; }
  }
</script>

<section>
  <h2>일촌 관리</h2>
  <p class="hint">내가 일촌공개로 설정한 콘텐츠를 볼 수 있는 사람을 관리합니다.</p>
  <div class="people">
    {#each people as person (person.agent_id)}
      <label><span>{person.name}</span><input type="checkbox" checked={person.is_friend} onchange={()=>toggle(person)}/></label>
    {:else}
      <p class="hint">등록된 다른 사용자가 없어요.</p>
    {/each}
  </div>
</section>

<section>
  <h2>다이어리 공개 범위</h2>
  <p class="hint">비공개가 기본이며, 공개를 선택한 경우에만 다이어리가 Life 서버로 전송됩니다.</p>
  <nav class="seg">
    {#each VISIBILITIES as option (option[0])}
      <button class:active={visibility===option[0]} disabled={sharing} onclick={()=>setVisibility(option[0])}>{option[1]}</button>
    {/each}
  </nav>
  <StatusLine {status}/>
</section>

<style>
  .people{display:grid;grid-template-columns:repeat(2,1fr);gap:8px;margin-top:14px}
  .people label{display:flex;justify-content:space-between;padding:10px 12px;background:var(--cream);color:var(--cream-ink);border-radius:9px}
  .seg{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden;margin-top:10px}
  .seg button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 16px;cursor:pointer;font:inherit}
  .seg button:last-child{border-right:0}
  .seg button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .seg button:disabled{opacity:.55;cursor:default}
</style>
```

- [ ] **Step 6: `LookGroup.svelte` 작성 — 테마**

`LifeSettingsTab.svelte`의 스크립트 32–36행 + 마크업(72행의 "테마" 부분)을 옮긴다. 스킨 점 색상 hex는 **스크립트에 있으므로** `no-hardcoded-colors` 검사 대상이 아니다 (기존과 동일).

```svelte
<script lang="ts">
  import { getTheme, setTheme, type ThemeMode, type ThemeSkin } from '../../theme';

  let themeMode = $state<ThemeMode>(getTheme().mode);
  let themeSkin = $state<ThemeSkin>(getTheme().skin);
  const MODES: [ThemeMode, string][] = [['light','라이트'],['dark','다크'],['system','시스템']];
  const SKINS: { id: ThemeSkin; label: string; dot: string }[] = [
    { id:'sky', label:'하늘', dot:'#84c9ef' },
    { id:'mint', label:'민트', dot:'#5cd0b0' },
    { id:'peach', label:'살구', dot:'#f4b183' },
    { id:'lavender', label:'라벤더', dot:'#b9a6f0' },
  ];
  function pickMode(m: ThemeMode){ themeMode = m; setTheme({ mode: m, skin: themeSkin }); }
  function pickSkin(s: ThemeSkin){ themeSkin = s; setTheme({ mode: themeMode, skin: s }); }
</script>

<section>
  <h2>테마</h2>
  <p class="hint">밝기</p>
  <nav class="seg">
    {#each MODES as opt (opt[0])}
      <button class:active={themeMode===opt[0]} onclick={()=>pickMode(opt[0])}>{opt[1]}</button>
    {/each}
  </nav>
  <p class="hint">색상 세트</p>
  <div class="skins">
    {#each SKINS as s (s.id)}
      <button class="swatch" class:active={themeSkin===s.id} onclick={()=>pickSkin(s.id)} title={s.label}>
        <span class="dot" style="background:{s.dot}"></span>{s.label}
      </button>
    {/each}
  </div>
</section>

<style>
  .seg{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden;margin-top:6px}
  .seg button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 16px;cursor:pointer;font:inherit}
  .seg button:last-child{border-right:0}
  .seg button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .skins{display:flex;gap:10px;flex-wrap:wrap;margin-top:6px}
  .swatch{display:flex;align-items:center;gap:6px;border:1px solid var(--line);background:var(--surface-inset);color:var(--text);border-radius:99px;padding:5px 12px 5px 6px;cursor:pointer;font:inherit}
  .swatch.active{border-color:var(--accent-strong);font-weight:700}
  .swatch .dot{width:16px;height:16px;border-radius:50%;border:1px solid var(--line)}
</style>
```

- [ ] **Step 7: `SettingsTab.svelte` 작성 — 서브탭 + 공용 섹션 스타일**

`src/lib/ui/settings/SettingsTab.svelte`. `group` prop이 바뀌면 선택도 따라가도록 `$effect`로 동기화한다 (딥링크가 이미 열린 설정 탭의 그룹을 바꿀 수 있어야 한다).

```svelte
<script lang="ts">
  import { DEFAULT_GROUP, SETTINGS_GROUPS, type SettingsGroup } from './groups';
  import ConnectionGroup from './ConnectionGroup.svelte';
  import LookGroup from './LookGroup.svelte';
  import MeGroup from './MeGroup.svelte';
  import PrivacyGroup from './PrivacyGroup.svelte';

  let { group = DEFAULT_GROUP }: { group?: SettingsGroup } = $props();

  let active = $state<SettingsGroup>(group);
  // 딥링크(트레이·마스코트)가 이미 열린 설정 탭의 그룹을 바꿀 수 있어야 한다.
  $effect(() => { active = group; });
</script>

<div class="tab">
  <nav class="seg groups">
    {#each SETTINGS_GROUPS as g (g.id)}
      <button class:active={active===g.id} onclick={()=>active=g.id}>{g.label}</button>
    {/each}
  </nav>
  <div class="group">
    {#if active === 'conn'}<ConnectionGroup/>
    {:else if active === 'me'}<MeGroup/>
    {:else if active === 'privacy'}<PrivacyGroup/>
    {:else}<LookGroup/>{/if}
  </div>
</div>

<style>
  .tab{padding:18px}
  .seg.groups{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden}
  .seg.groups button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 18px;cursor:pointer;font:inherit}
  .seg.groups button:last-child{border-right:0}
  .seg.groups button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .group{margin-top:16px}
  /* 공용 섹션 스타일 — 각 그룹이 <section><h2>…를 직접 쓰고 여기서 모양을 준다 */
  .group :global(h2){margin:0 0 5px;font-size:16px}
  .group :global(.hint){margin:0;color:var(--text-soft);font-size:12px}
  .group :global(section + section){border-top:1px solid var(--line);margin-top:20px;padding-top:20px}
</style>
```

- [ ] **Step 8: `App.svelte` 배선 교체**

`src/App.svelte` 8행의 import를 교체:

```svelte
  import SettingsTab from './lib/ui/settings/SettingsTab.svelte';
```

181행 `{:else}` 분기의 `<LifeSettingsTab />`를 교체:

```svelte
        {:else}
          <SettingsTab />
        {/if}
```

- [ ] **Step 9: 구 컴포넌트 삭제**

```bash
git rm src/lib/ui/LifeSettingsTab.svelte
```

- [ ] **Step 10: 테스트 통과 확인 (하드코딩 색·회귀)**

Run:
```bash
npm test
```
Expected: PASS — `no-hardcoded-colors`가 새 `settings/*.svelte` 5개를 자동 검사하고 통과. `LifeSettingsTab.svelte` 케이스는 사라짐.

- [ ] **Step 11: 타입·빌드 확인**

Run:
```bash
npm run build
```
Expected: 성공. 실패 시 대개 import 경로(`../../api`, `../../theme`)나 `HubSettings`/`Memory` 타입명 불일치다.

- [ ] **Step 12: 커밋**

```bash
git add src/lib/api.ts src/lib/ui/settings/ src/App.svelte
git commit -m "feat(a-mate): regroup the settings tab and absorb engine, image and life server sections"
```

---

## Task 4: 방문 중 트레이 설정 → 내 방 복귀 후 설정 탭

`App.svelte`가 `tab-routing.ts`를 쓰도록 바꾸고 `pendingTab`을 붙인다. 아직 `settings` 딥링크를 보내는 쪽이 없으므로(Task 5) **이 태스크의 검증은 단위 테스트까지다** — end-to-end 확인은 Task 6 수동 체크리스트.

**Files:**
- Modify: `a-mate/src/App.svelte` — `Tab` 타입 import, 폴링 리셋 로직, `onGotoTab` 가드, `SettingsTab`에 그룹 전달

**Interfaces:**
- Consumes: Task 1의 `Tab`, `isTab`, `resolveTabAfterLifeChange`, `normalizeGroup`, `SettingsGroup`; Task 3의 `SettingsTab` `group` prop
- Produces: `chat:goto-tab{tab:"settings", target}` 수신 시 방문 중이면 내 방 복귀 후 설정 탭으로 착지하는 동작 — Task 5의 트레이·마스코트 딥링크가 여기에 의존

- [ ] **Step 1: `App.svelte`에서 `Tab` 타입을 tab-routing으로 교체**

22행의 로컬 `type Tab = …` 선언을 삭제하고, import 블록에 추가:

```svelte
  import { isTab, resolveTabAfterLifeChange, type Tab } from './lib/ui/tab-routing';
  import { normalizeGroup, type SettingsGroup } from './lib/ui/settings/groups';
```

`const TABS: { id: Tab; label: string }[]`는 그대로 둔다 (라벨은 App의 관심사).

- [ ] **Step 2: `pendingTab`·`settingsGroup` 상태 추가**

`let tab = $state<Tab>('home');` 아래에 추가:

```svelte
  // 방문 중에 설정 딥링크를 받으면 내 방으로 돌아간 뒤 착지해야 한다.
  // 방 변경 시 홈으로 리셋하는 폴링 로직이 그 의도를 덮어쓰지 않도록 예약해 둔다.
  let pendingTab = $state<Tab | null>(null);
  // 그룹은 지연하지 않는다 — 설정 탭이 안 보이는 동안 바뀌어도 관측되는 효과가 없다.
  let settingsGroup = $state<SettingsGroup>('conn');
```

- [ ] **Step 3: 폴링의 홈 리셋을 순수 함수로 교체**

`$effect` 안 `tick()`의 `if (lifeChanged) tab = 'home';`를 교체:

```svelte
        const next = resolveTabAfterLifeChange(lifeChanged, pendingTab, tab);
        tab = next.tab;
        pendingTab = next.pendingTab;
```

같은 `tick()`의 방문 중 강제 이동 줄에 설정 예약 중 예외를 더한다. 기존:

```svelte
        if (visiting && (!['home','diary','guestbook'].includes(tab) || (tab === 'diary' && !canViewDiary))) tab = 'home';
```

교체:

```svelte
        // 설정 착지를 예약한 상태(내 방으로 돌아오는 중)면 홈으로 밀지 않는다 — 다음 tick이 착지시킨다.
        if (!pendingTab && visiting && (!['home','diary','guestbook'].includes(tab) || (tab === 'diary' && !canViewDiary))) tab = 'home';
```

- [ ] **Step 4: `onGotoTab` 가드에 settings 추가 + 방문 중 분기**

기존 `onGotoTab` 블록(97–102행)을 교체:

```svelte
  onGotoTab(({ tab: t, target }) => {
    if (!isTab(t) || t === 'guestbook') return; // 방명록은 방 문맥이 필요해 딥링크 대상이 아니다
    if (t === 'settings') {
      settingsGroup = normalizeGroup(target);
      // 남의 방 방문 중이면 내 방으로 돌아간 뒤 착지한다 (내 설정이 방 주인 것으로 오독되지 않게)
      if (visiting) { pendingTab = 'settings'; lifeGoto(myLifeId).catch(() => { pendingTab = null; }); }
      else tab = 'settings';
      return;
    }
    if (target && t === 'coach') gotoCoach(target);
    else if (target && t === 'diary') gotoDiary(target);
    else tab = t;
  });
```

`lifeGoto`를 `./lib/api` import 목록에 추가한다 (12–16행 블록).

- [ ] **Step 5: `SettingsTab`에 그룹 전달**

Task 3 Step 8에서 넣은 `<SettingsTab />`를 교체:

```svelte
        {:else}
          <SettingsTab group={settingsGroup} />
        {/if}
```

- [ ] **Step 6: 테스트·빌드 확인**

Run:
```bash
npm test && npm run build
```
Expected: PASS + 빌드 성공. `resolveTabAfterLifeChange`/`isTab`는 Task 1에서 이미 커버됨.

- [ ] **Step 7: 커밋**

```bash
git add src/App.svelte
git commit -m "feat(a-mate): land on the settings tab after returning home from a visit"
```

---

## Task 5: 별도 settings 창과 죽은 배선 제거

트레이·마스코트를 딥링크로 재배선하고 창·엔트리·커맨드를 지운다. **삭제 전에 Task 3·4가 검증된 상태여야 한다.**

**Files:**
- Modify: `a-mate/src-tauri/src/tray.rs` — `show_settings` 제거, 메뉴 핸들러 교체
- Modify: `a-mate/src-tauri/src/commands.rs` — `open_settings_window` 제거
- Modify: `a-mate/src-tauri/src/lib.rs` — invoke_handler 등록 해제, `"settings"` 라벨 참조 2곳
- Modify: `a-mate/src-tauri/tauri.conf.json` — `settings` 윈도우 정의 제거
- Modify: `a-mate/vite.config.ts` — `settings` 엔트리 제거
- Modify: `a-mate/src/lib/api.ts` — `openSettingsWindow` 제거
- Modify: `a-mate/src/Mascot.svelte` — 미연결 버튼 목적지 변경
- Delete: `a-mate/src/Settings.svelte`, `a-mate/src/settings.html`, `a-mate/src/settings.ts`

**Interfaces:**
- Consumes: Task 2의 `valid_tab("settings")`, Task 4의 settings 딥링크 처리
- Produces: 트레이 "설정"과 마스코트 "서버 미연결" 버튼이 미니홈피 창 설정 탭 `conn` 그룹을 연다

- [ ] **Step 1: `tray.rs` — `show_settings`를 딥링크로 교체**

`fn show_settings`(208–214행)를 교체:

```rust
/// 설정 = 미니홈피 창의 설정 탭(연결 그룹). 별도 설정 창은 없다.
fn show_settings(app: &AppHandle) {
    use tauri::Emitter;
    show_chat(app);
    if let Err(error) = app.emit(
        "chat:goto-tab",
        crate::commands::GotoTabPayload { tab: "settings".into(), target: Some("conn".into()) },
    ) {
        log::error!("설정 탭 이동 이벤트 전송 실패: {error}");
    }
}
```

`"settings" => show_settings(app),` 핸들러(161행)는 그대로 둔다.

- [ ] **Step 2: `commands.rs` — `open_settings_window` 제거**

999–1008행의 doc comment + `open_settings_window` 함수 전체를 삭제한다.

- [ ] **Step 3: `lib.rs` — 등록·라벨 참조 제거**

- 427행 `commands::open_settings_window,`를 invoke_handler 목록에서 삭제
- 182행 `for label in ["chat", "mascot", "settings"] {` → `for label in ["chat", "mascot"] {`
- 226행 `if matches!(window.label(), "chat" | "settings") {` → `if window.label() == "chat" {`

- [ ] **Step 4: 창 정의·빌드 엔트리 제거**

`src-tauri/tauri.conf.json`의 `app.windows` 배열에서 `"label": "settings"` 객체를 삭제한다 (`chat`, `mascot`만 남는다).

`vite.config.ts`의 rollup input에서 한 줄 삭제:

```
        settings: fileURLToPath(new URL('./src/settings.html', import.meta.url)),
```

`vite.config.ts` 14행 주석("2단계에서 mascot.html 엔트리 추가")은 그대로 둔다 — 이 작업과 무관한 이력 메모다.

- [ ] **Step 5: 프론트 배선 제거·교체**

`src/lib/api.ts` 231행 삭제:

```
export const openSettingsWindow = () => invoke<void>('open_settings_window');
```

`src/Mascot.svelte` 7행 import에서 `openSettingsWindow`를 빼고 (`openChatTab`은 이미 있다), 291–294행의 미연결 분기를 교체:

```svelte
      {#if !hubOn}
        <button class="item" onclick={() => { openChatTab('settings', 'conn'); closeLifeMenu(); }}>
          서버 미연결 — 설정 열기
        </button>
```

- [ ] **Step 6: 죽은 파일 삭제**

```bash
git rm src/Settings.svelte src/settings.html src/settings.ts
```

- [ ] **Step 7: 남은 참조가 없는지 확인**

Run:
```bash
grep -rn "openSettingsWindow\|open_settings_window\|settings.html\|Settings.svelte" src src-tauri/src vite.config.ts src-tauri/tauri.conf.json
```
Expected: 출력 없음. 나오면 그 참조를 지운다.

- [ ] **Step 8: 프론트 테스트·빌드**

Run:
```bash
npm test && npm run build
```
Expected: PASS + 빌드 성공. `dist/`에 `settings.html`이 더 이상 생성되지 않는다.

- [ ] **Step 9: Rust 빌드·테스트**

Run (**Windows PowerShell**):
```powershell
cargo test
```
Expected: PASS. `unused import` / `dead_code` 경고가 나오면 해당 import를 정리한다.

> WSL이면 **미검증**으로 남기고 Task 6 체크리스트에 기록한다.

- [ ] **Step 10: 커밋**

```bash
git add -A src src-tauri vite.config.ts
git commit -m "refactor(a-mate): remove the standalone settings window"
```

---

## Task 6: Windows 수동 검증 + 아카이브 (DoD)

**Files:**
- Modify: `a-mate/docs`가 아닌 저장소 루트 `docs/` — `docs-archive` 스킬이 이동시킨다
- Move: `docs/design/a-mate/specs/2026-07-25-settings-consolidation-design.md` → `docs/archive/…`
- Move: `docs/design/a-mate/plans/2026-07-25-settings-consolidation.md` → `docs/archive/…`

**Interfaces:**
- Consumes: Task 1–5 완료
- Produces: 없음 (마무리)

- [ ] **Step 1: Windows PowerShell에서 앱 실행**

Run (`a-mate/`에서):
```powershell
npm run tauri dev
```

- [ ] **Step 2: 수동 체크리스트 실행 — 각 항목의 실제 결과를 기록한다**

| # | 시나리오 | 기대 |
|---|---|---|
| 1 | 트레이 우클릭 → "설정" | 미니홈피 창이 열리고 설정 탭 **연결** 그룹 (Life Server가 맨 위) |
| 2 | 설정 탭에서 연결/나/공개/모양 전환 | 4그룹 모두 렌더, 콘솔 에러 없음 |
| 3 | 연결 그룹에서 LLM 엔진 "저장" + "연결 테스트" | 상태 줄에 저장/결과 메시지 |
| 4 | 연결 그룹에서 Life Server "연결" | 연결됨 + 내 방 id 표시 |
| 5 | 나 그룹에서 이름 변경 후 저장 | 저장 메시지, 미니홈피 헤더 이름 반영 |
| 6 | 모양 그룹에서 테마 밝기·색상 변경 | 창과 마스코트에 즉시 반영 |
| 7 | 다른 사람 방으로 이동 후 트레이 → "설정" | 내 방으로 돌아간 뒤 설정 탭 연결 그룹 (**홈으로 튕기지 않음**) |
| 8 | Life Server 연결 종료 후 마스코트 우클릭 | "서버 미연결 — 설정 열기" → 설정 탭 연결 그룹 |
| 9 | 마스코트 우클릭 → 방 이동 / 말풍선 설정 | 기존과 동일하게 동작 (건드리지 않은 기능) |
| 10 | 설정 탭 → 홈 탭 → 설정 탭 | 연결 그룹으로 초기화 (마지막 그룹 기억 안 함 — 의도된 동작) |
| 11 | 트레이 "화면 캡처 보호" 토글 | chat·mascot 창에 적용 (settings 창 제거 후 회귀 없음) |
| 12 | `npm run tauri build` | 빌드 성공, 설치본에서 1·7번 재확인 |

- [ ] **Step 3: 실패한 항목이 있으면 고치고 커밋**

실패 항목마다 원인을 적고 수정한다. 수정이 스펙과 어긋나면 스펙을 먼저 고친다 (문서 우선).

- [ ] **Step 4: 아카이브 (DoD)**

`docs-archive` 스킬을 실행해 이 계획과 스펙을 `docs/archive/` 미러로 옮긴다.

```
Skill: docs-archive
대상: docs/design/a-mate/specs/2026-07-25-settings-consolidation-design.md
      docs/design/a-mate/plans/2026-07-25-settings-consolidation.md
```

- [ ] **Step 5: 커밋**

```bash
git add -A docs
git commit -m "docs(a-mate): archive settings consolidation spec and plan"
```

- [ ] **Step 6: PR 생성**

```bash
git push -u origin docs/amate-settings-consolidation
gh pr create --title "refactor(a-mate): consolidate settings into the minihompy settings tab" --body "$(cat <<'EOF'
## 요약
별도 `settings` 창(LLM 엔진·캐릭터 이미지·Life Server)을 미니홈피 창 설정 탭으로 흡수했습니다. 설정 진입점이 세 곳에서 한 곳으로 줄었습니다.

## 변경
- 설정 탭을 상단 서브탭 4그룹(연결/나/공개/모양)으로 재구성
- `LifeSettingsTab.svelte`(116줄, 5개 관심사)를 `src/lib/ui/settings/` 아래 그룹별 컴포넌트로 분해
- 별도 창·엔트리(`settings.html`/`settings.ts`/`Settings.svelte`)·`open_settings_window` 커맨드 제거
- 트레이 "설정"·마스코트 "서버 미연결" → `chat:goto-tab{settings, conn}` 딥링크
- 남의 방 방문 중 설정 딥링크는 내 방 복귀 후 착지 (`resolveTabAfterLifeChange`)

## 범위 밖
트레이 토글류(실시간 조언·화면 캡처 보호·잡담 빈도·시작 시 실행·마스코트 표시)는 트레이에 그대로 남겼습니다. 방 이동·말풍선도 마스코트 유지 — 설정이 아니라 액션이라서입니다.

## 검증
- `npm test` / `cargo test` 통과
- Windows 수동 체크리스트 12항목 (계획 Task 6)

스펙: `docs/archive/design/a-mate/specs/2026-07-25-settings-consolidation-design.md`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**스펙 커버리지**

| 스펙 섹션 | 담당 태스크 |
|---|---|
| 원칙 (설정 vs 액션) | Task 5 (트레이·마스코트 재배선), Task 3 (설정 한 곳) |
| 의도적 예외 — 트레이 토글류 | 손대지 않음. PR 본문·Task 6 #11로 회귀만 확인 |
| 설정 탭 IA 4그룹 | Task 1 (`groups.ts`), Task 3 (`SettingsTab`) |
| 기본 그룹 `conn`·마지막 그룹 기억 안 함 | Task 1 (`DEFAULT_GROUP`), Task 6 #10 |
| 딥링크 `target`=그룹 id | Task 1 (`normalizeGroup`), Task 2 (`valid_tab`), Task 4, Task 5 |
| 파일 구조 (`settings/` 7파일 + `tab-routing.ts`) | Task 1, Task 3 |
| 섹션 껍데기 컴포넌트 두지 않음 | Task 3 Step 7 (`:global` 스타일) |
| 제거·배선 변경 표 (전 항목) | Task 5 Step 1–6 |
| 데이터 마이그레이션 없음 | 저장 키·커맨드 미변경 — Global Constraints에 명시 |
| 방문 중 처리 + `pendingTab` | Task 1 (`resolveTabAfterLifeChange`), Task 4 |
| 그룹 target은 지연하지 않음 | Task 4 Step 2·4 (`settingsGroup` 즉시 반영) |
| 스타일 정합 (`--line` 계열) | Global Constraints + Task 3 각 컴포넌트 `<style>` |
| 테스트 표 (Rust 1 + Vitest 3 + 수동) | Task 1, Task 2, Task 6 |

빠진 스펙 요구사항 없음.

**플레이스홀더 스캔**: 없음. 모든 코드 스텝이 실제 코드 블록을 담고 있고, 모든 실행 스텝이 실제 명령과 기대 결과를 담고 있다.

**타입 일관성 확인**

- `SettingsGroup`/`normalizeGroup`/`DEFAULT_GROUP` — Task 1 정의, Task 3 `SettingsTab`·Task 4 `App.svelte`에서 동일 이름으로 사용 ✅
- `Status`/`IDLE`/`busy`/`ok`/`err` — Task 1 정의, Task 3 세 그룹에서 동일 시그니처로 사용 ✅
- `Tab`/`isTab`/`resolveTabAfterLifeChange` — Task 1 정의, Task 4에서 동일 사용 ✅
- `EngineSettings`/`engineSettingsGet`/`engineSettingsSet`/`engineTest` — Task 3 Step 1 정의, 같은 태스크 Step 3에서 사용 ✅
- `HubSettings`(`api.ts:137`)·`ContentVisibility`(`:216`)·`LifePerson`(`:209` 부근)·`ImageSettings`(`:281`)·`Profile`(`:297`)·`Memory`(`:303`) — 기존 `api.ts` 타입 재사용, 로컬 재정의 없음 ✅
- `GotoTabPayload` — `commands.rs` 기존 struct(`pub struct` + `pub` 필드). `lib.rs`의 `mod commands;`는 비공개지만 `tray`도 크레이트 루트의 자식이므로 `crate::commands::GotoTabPayload` 접근이 성립한다 (비공개 항목은 정의 모듈과 그 하위에서 보인다) ✅
- `StatusLine` prop 이름 `status` — Task 3 Step 2 정의, Step 3–5에서 `status={hubStatus}` / `{status}` 형태로 일관 사용 ✅
