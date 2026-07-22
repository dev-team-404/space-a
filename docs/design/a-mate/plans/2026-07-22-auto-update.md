# a-mate 인앱 자동 업데이트 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a-mate(Tauri v2)에 인앱 자동 업데이트를 추가해, 사용자가 앱 안에서 새 버전을 감지·다운로드·설치·재시작할 수 있게 한다.

**Architecture:** `tauri-plugin-updater`(업데이트) + `tauri-plugin-process`(재시작)를 도입한다. 업데이트 흐름은 **프론트엔드 주도**: chat 창이 시작 시 `check()`를 호출하고, 결과를 상단 배너로 표시한다. 트레이 "업데이트 확인"은 chat 창을 띄우고 `update:check` 이벤트를 emit해 같은 흐름을 수동 트리거한다. 피드는 공개 릴리스 저장소(`dev-team-404/a-mate-releases`)의 정적 `latest.json`.

**Tech Stack:** Tauri v2, Rust, Svelte 5(runes), TypeScript, Vitest, GitHub Releases, `gh` CLI, bash + `powershell.exe` interop.

## Global Constraints

- 플랫폼: **Windows 전용**. macOS/Linux 분기 불필요. Rust 전체 빌드는 네이티브 Windows에서만 (WSL 빌드 금지).
- 프론트 테스트/빌드(vitest, vite)는 WSL(Node v22)에서 실행 가능. Rust 컴파일·NSIS 번들·E2E는 Windows 빌드 복사본(`C:\Users\salt.jeong\amate-build\a-mate`)에서.
- Tauri 플러그인 버전 표기는 major만: `"2"` (기존 Cargo.toml 컨벤션).
- 스타일은 **전역 CSS 토큰(`var(--…)`)만** 사용. 하드코딩 색 금지 — `src/lib/no-hardcoded-colors.test.ts`가 강제.
- 모든 백엔드 호출은 `src/lib/api.ts`에 래핑하고 컴포넌트는 거기서 import (기존 컨벤션).
- 커밋 메시지: Conventional Commits, 영어. `Co-Authored-By` 트레일러 금지(사용자 전역 설정).
- updater **private key는 절대 커밋하지 않는다**. public key만 `tauri.conf.json`에 커밋.
- 사내망 TLS 우회: Windows 빌드 시 `CARGO_HTTP_CHECK_REVOKE=false` env 필요.

---

### Task 1: 서명 키페어 생성 + 공개 릴리스 저장소 준비 (인프라)

**Files:** (코드 변경 없음 — 산출물은 키파일 + 원격 저장소)

**Interfaces:**
- Produces: updater **public key** 문자열(Task 5의 `tauri.conf.json` `plugins.updater.pubkey`에 사용), 비공개 **private key 파일**(`~/.tauri/a-mate-updater.key`, Task 7 릴리스에 사용), 원격 저장소 `dev-team-404/a-mate-releases`.

> ⚠️ **외부 영향 작업**: 공개 GitHub 저장소 생성. 실행 전 사용자 승인 확인(이미 설계에서 승인됨).

- [ ] **Step 1: updater 키페어 생성**

WSL에서 (Node/tauri CLI 사용):
```bash
mkdir -p ~/.tauri
cd /home/msaltnet/code/space-a/a-mate
npm install    # tauri CLI 사용 위해 (node_modules 없으면)
npm run tauri signer generate -- -w ~/.tauri/a-mate-updater.key --password ""
```
Expected: `~/.tauri/a-mate-updater.key`(private) + `~/.tauri/a-mate-updater.key.pub`(public) 생성. stdout에 public key(base64) 출력.

- [ ] **Step 2: public key 확인·보관**

```bash
cat ~/.tauri/a-mate-updater.key.pub
```
Expected: `untrusted comment: ...` 헤더 없이 base64 한 줄(또는 `dW50cnVzdGVk...` 형태). 이 값을 Task 5에서 사용. private key 파일은 절대 커밋하지 않음(홈 밖으로 유출 금지).

- [ ] **Step 3: 공개 릴리스 저장소 생성**

```bash
gh repo create dev-team-404/a-mate-releases --public \
  --description "Agent Mentor (a-mate) release channel — installers + update manifest"
```
Expected: 저장소 생성 성공 메시지 + URL.

- [ ] **Step 4: 생성 확인**

```bash
gh repo view dev-team-404/a-mate-releases --json name,visibility -q '.name + " / " + .visibility'
```
Expected: `a-mate-releases / PUBLIC`

- [ ] **Step 5: (커밋 없음)** — 이 태스크는 인프라 준비만. 코드 변경이 없어 커밋 대상 없음.

---

### Task 2: 프론트엔드 의존성 추가

**Files:**
- Modify: `a-mate/package.json` (dependencies)

**Interfaces:**
- Produces: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` npm 패키지(Task 4에서 import).

- [ ] **Step 1: 플러그인 npm 패키지 설치**

```bash
cd /home/msaltnet/code/space-a/a-mate
npm install @tauri-apps/plugin-updater@^2 @tauri-apps/plugin-process@^2
```
Expected: `package.json` dependencies에 두 패키지 추가, `package-lock.json` 갱신.

- [ ] **Step 2: 기존 테스트가 여전히 통과하는지 확인**

Run: `npm test`
Expected: 기존 13개 테스트 파일 PASS (회귀 없음).

- [ ] **Step 3: 커밋**

```bash
git add a-mate/package.json a-mate/package-lock.json
git commit -m "chore(agent): add updater and process plugin npm deps"
```

---

### Task 3: 업데이트 배너 순수 헬퍼 (TDD)

배너 표시 여부·문구·진행률 계산을 순수 함수로 분리한다(기존 `notices.ts`/`model-mix-helpers.ts` 컨벤션). 부수효과 없는 이 로직만 유닛 테스트한다.

**Files:**
- Create: `a-mate/src/lib/ui/update-banner-helpers.ts`
- Test: `a-mate/src/lib/ui/update-banner-helpers.test.ts`

**Interfaces:**
- Produces:
  - `type UpdatePhase` (아래 정의)
  - `shouldShow(p: UpdatePhase): boolean`
  - `bannerText(p: UpdatePhase): string`
  - `progressPercent(downloaded: number, total: number | null): number | null`

- [ ] **Step 1: 실패하는 테스트 작성**

`a-mate/src/lib/ui/update-banner-helpers.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import {
  shouldShow,
  bannerText,
  progressPercent,
  type UpdatePhase,
} from './update-banner-helpers';

describe('shouldShow', () => {
  it('idle는 항상 숨김', () => {
    expect(shouldShow({ kind: 'idle' })).toBe(false);
  });
  it('available/downloading는 항상 표시', () => {
    expect(shouldShow({ kind: 'available', version: '0.2.0', notes: null })).toBe(true);
    expect(shouldShow({ kind: 'downloading', downloaded: 1, total: 2 })).toBe(true);
  });
  it('checking/uptodate/error는 수동일 때만 표시', () => {
    expect(shouldShow({ kind: 'checking', manual: false })).toBe(false);
    expect(shouldShow({ kind: 'checking', manual: true })).toBe(true);
    expect(shouldShow({ kind: 'uptodate', manual: false })).toBe(false);
    expect(shouldShow({ kind: 'uptodate', manual: true })).toBe(true);
    expect(shouldShow({ kind: 'error', manual: false, message: 'x' })).toBe(false);
    expect(shouldShow({ kind: 'error', manual: true, message: 'x' })).toBe(true);
  });
});

describe('bannerText', () => {
  it('available은 버전 포함', () => {
    expect(bannerText({ kind: 'available', version: '0.2.0', notes: null })).toBe('새 버전 0.2.0 있음');
  });
  it('downloading은 진행률(%) 표시, total 없으면 % 생략', () => {
    expect(bannerText({ kind: 'downloading', downloaded: 50, total: 200 })).toBe('업데이트 다운로드 중… 25%');
    expect(bannerText({ kind: 'downloading', downloaded: 50, total: null })).toBe('업데이트 다운로드 중…');
  });
  it('uptodate/error 문구', () => {
    expect(bannerText({ kind: 'uptodate', manual: true })).toBe('최신 버전입니다');
    expect(bannerText({ kind: 'error', manual: true, message: '네트워크' })).toBe('업데이트 확인 실패: 네트워크');
  });
});

describe('progressPercent', () => {
  it('total이 null/0이면 null', () => {
    expect(progressPercent(10, null)).toBe(null);
    expect(progressPercent(10, 0)).toBe(null);
  });
  it('정상 비율 계산 + 0~100 clamp', () => {
    expect(progressPercent(50, 200)).toBe(25);
    expect(progressPercent(300, 200)).toBe(100);
    expect(progressPercent(-5, 200)).toBe(0);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `npm test -- update-banner-helpers`
Expected: FAIL — `Cannot find module './update-banner-helpers'`

- [ ] **Step 3: 최소 구현**

`a-mate/src/lib/ui/update-banner-helpers.ts`:
```ts
export type UpdatePhase =
  | { kind: 'idle' }
  | { kind: 'checking'; manual: boolean }
  | { kind: 'available'; version: string; notes: string | null }
  | { kind: 'downloading'; downloaded: number; total: number | null }
  | { kind: 'uptodate'; manual: boolean }
  | { kind: 'error'; manual: boolean; message: string };

export function shouldShow(p: UpdatePhase): boolean {
  switch (p.kind) {
    case 'available':
    case 'downloading':
      return true;
    case 'checking':
    case 'uptodate':
    case 'error':
      return p.manual;
    case 'idle':
    default:
      return false;
  }
}

export function progressPercent(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, Math.floor((downloaded / total) * 100)));
}

export function bannerText(p: UpdatePhase): string {
  switch (p.kind) {
    case 'checking':
      return '업데이트 확인 중…';
    case 'available':
      return `새 버전 ${p.version} 있음`;
    case 'downloading': {
      const pct = progressPercent(p.downloaded, p.total);
      return pct === null ? '업데이트 다운로드 중…' : `업데이트 다운로드 중… ${pct}%`;
    }
    case 'uptodate':
      return '최신 버전입니다';
    case 'error':
      return `업데이트 확인 실패: ${p.message}`;
    case 'idle':
    default:
      return '';
  }
}
```

- [ ] **Step 4: 통과 확인**

Run: `npm test -- update-banner-helpers`
Expected: PASS (모든 it 통과)

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/update-banner-helpers.ts a-mate/src/lib/ui/update-banner-helpers.test.ts
git commit -m "feat(agent): add update banner pure helpers"
```

---

### Task 4: 업데이트 스토어 + 배너 컴포넌트 + chat 창 연결

부수효과(플러그인 호출)를 담는 스토어와 프레젠테이션 컴포넌트를 추가하고 chat 창에 붙인다.

**Files:**
- Modify: `a-mate/src/lib/api.ts` (이벤트 래퍼 추가)
- Create: `a-mate/src/lib/ui/update-store.svelte.ts`
- Create: `a-mate/src/lib/ui/UpdateBanner.svelte`
- Modify: `a-mate/src/App.svelte` (import + 배치 + 시작 시 체크/이벤트 구독)

**Interfaces:**
- Consumes: `UpdatePhase`, `shouldShow`, `bannerText`, `progressPercent` (Task 3); `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` (Task 2).
- Produces:
  - `api.ts`: `onUpdateCheckRequested(cb: () => void): Promise<UnlistenFn>`
  - `update-store.svelte.ts`: `updateStore` (`{ phase: UpdatePhase }`), `runCheck(manual: boolean): Promise<void>`, `installUpdate(): Promise<void>`, `dismiss(): void`

- [ ] **Step 1: api.ts에 트레이 이벤트 래퍼 추가**

`a-mate/src/lib/api.ts` — 기존 `listen` import(파일 상단, line 2)를 재사용해 파일 끝(이벤트 래퍼들이 모인 구역, 약 line 247-265 부근)에 추가:
```ts
// 트레이 "업데이트 확인" → chat 창에서 수동 체크 트리거
export const onUpdateCheckRequested = (cb: () => void): Promise<UnlistenFn> =>
  listen('update:check', () => cb());
```

- [ ] **Step 2: 업데이트 스토어 작성**

`a-mate/src/lib/ui/update-store.svelte.ts`:
```ts
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import type { UpdatePhase } from './update-banner-helpers';

export const updateStore = $state<{ phase: UpdatePhase }>({ phase: { kind: 'idle' } });

let pending: Update | null = null;

export async function runCheck(manual: boolean): Promise<void> {
  updateStore.phase = { kind: 'checking', manual };
  try {
    const update = await check();
    pending = update;
    if (update) {
      updateStore.phase = { kind: 'available', version: update.version, notes: update.body ?? null };
    } else {
      updateStore.phase = { kind: 'uptodate', manual };
    }
  } catch (e) {
    updateStore.phase = { kind: 'error', manual, message: String(e) };
  }
}

export async function installUpdate(): Promise<void> {
  if (!pending) return;
  let downloaded = 0;
  let total: number | null = null;
  updateStore.phase = { kind: 'downloading', downloaded: 0, total: null };
  await pending.downloadAndInstall((ev) => {
    switch (ev.event) {
      case 'Started':
        total = ev.data.contentLength ?? null;
        updateStore.phase = { kind: 'downloading', downloaded: 0, total };
        break;
      case 'Progress':
        downloaded += ev.data.chunkLength;
        updateStore.phase = { kind: 'downloading', downloaded, total };
        break;
      case 'Finished':
        break;
    }
  });
  await relaunch();
}

export function dismiss(): void {
  updateStore.phase = { kind: 'idle' };
}
```

- [ ] **Step 3: 배너 컴포넌트 작성**

`a-mate/src/lib/ui/UpdateBanner.svelte` (스타일은 `var(--…)` 토큰만; 기존 TipCard 알림 박스 스타일 참고):
```svelte
<script lang="ts">
  import { updateStore, installUpdate, dismiss } from './update-store.svelte';
  import { shouldShow, bannerText } from './update-banner-helpers';

  const phase = $derived(updateStore.phase);
  const visible = $derived(shouldShow(phase));
  const busy = $derived(phase.kind === 'downloading');
  const canInstall = $derived(phase.kind === 'available');
</script>

{#if visible}
  <div class="update-banner" role="status">
    <span class="msg">{bannerText(phase)}</span>
    {#if canInstall}
      <button class="primary" onclick={installUpdate}>지금 업데이트</button>
      <button class="ghost" onclick={dismiss}>나중</button>
    {:else if !busy}
      <button class="ghost" onclick={dismiss}>닫기</button>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-left: 3px solid var(--accent);
    background: var(--surface, var(--bg));
    color: var(--ink);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    font-size: 13px;
  }
  .msg { flex: 1; }
  button {
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    padding: 4px 10px;
    cursor: pointer;
    font-size: 12px;
    background: transparent;
    color: var(--ink);
  }
  button.primary {
    background: var(--accent);
    color: var(--on-accent, var(--bg));
    border-color: var(--accent);
  }
</style>
```

> 실행 시: 사용하는 토큰(`--surface`, `--on-accent` 등)이 `src/lib/theme.css`에 없으면 존재하는 토큰으로 교체하거나 fallback(`var(--x, var(--y))`)을 유지한다. `no-hardcoded-colors.test.ts`를 반드시 통과시킬 것.

- [ ] **Step 4: App.svelte에 배너 배치 + 시작 시 체크/이벤트 구독**

`a-mate/src/App.svelte` import 블록(line 3-9 부근)에 추가:
```ts
  import { onMount } from 'svelte';
  import UpdateBanner from './lib/ui/UpdateBanner.svelte';
  import { runCheck } from './lib/ui/update-store.svelte';
  import { onUpdateCheckRequested } from './lib/api';
```
(이미 `onMount`가 import돼 있으면 중복 추가하지 말 것.)

`<script>` 안에 onMount 추가:
```ts
  onMount(() => {
    runCheck(false); // 시작 시 자동 체크(조용히)
    const un = onUpdateCheckRequested(() => runCheck(true)); // 트레이 수동 체크
    return () => { un.then((f) => f()); };
  });
```

`.homepy` 최상단, `<header class="titlebar">` 바로 앞(line 130-141 부근)에 배치:
```svelte
    <div class="homepy">
      <UpdateBanner />
      <header class="titlebar">
```

- [ ] **Step 5: 타입/빌드/테스트 검증**

Run:
```bash
cd /home/msaltnet/code/space-a/a-mate
npm run build          # vite 빌드 (타입·컴파일 오류 없어야)
npm test               # 기존 + Task 3 테스트 PASS, no-hardcoded-colors PASS
```
Expected: vite build 성공, 모든 테스트 PASS.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/update-store.svelte.ts a-mate/src/lib/ui/UpdateBanner.svelte a-mate/src/App.svelte
git commit -m "feat(agent): add in-app update banner and startup check to chat window"
```

---

### Task 5: Rust 플러그인 등록 + updater 설정 + 권한

**Files:**
- Modify: `a-mate/src-tauri/Cargo.toml` (dependencies)
- Modify: `a-mate/src-tauri/src/lib.rs` (플러그인 등록)
- Modify: `a-mate/src-tauri/capabilities/default.json` (권한)
- Modify: `a-mate/src-tauri/tauri.conf.json` (`bundle.createUpdaterArtifacts`, `plugins.updater`)

**Interfaces:**
- Consumes: Task 1의 public key.
- Produces: Rust 런타임에 updater/process 플러그인 활성화, 빌드 시 `.sig` 산출.

- [ ] **Step 1: Cargo.toml에 플러그인 의존성 추가**

`a-mate/src-tauri/Cargo.toml`의 `[dependencies]`(line 15-29)에서 `tauri-plugin-opener = "2"` 아래에 추가:
```toml
tauri-plugin-updater = "2"
tauri-plugin-process = "2"
```

- [ ] **Step 2: lib.rs에 플러그인 등록**

`a-mate/src-tauri/src/lib.rs` line 220 `.plugin(tauri_plugin_opener::init())` 뒤, `.on_window_event(` (line 221) 앞에 추가:
```rust
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
```

- [ ] **Step 3: capabilities에 권한 추가**

`a-mate/src-tauri/capabilities/default.json`의 `permissions` 배열(line 6-14)에 추가:
```json
    "updater:default",
    "process:allow-restart"
```
(기존 마지막 항목 `"opener:allow-default-urls"` 뒤에 콤마 처리해서 추가.)

- [ ] **Step 4: tauri.conf.json에 updater 설정 + createUpdaterArtifacts**

`a-mate/src-tauri/tauri.conf.json`의 `bundle` 블록에 `"createUpdaterArtifacts": true`를 추가하고, 최상위에 `plugins.updater` 블록 신설. `<PUBKEY>`는 Task 1 Step 2의 public key로 치환:
```json
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "createUpdaterArtifacts": true,
    "icon": ["icons/icon.ico", "icons/128x128.png"]
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/dev-team-404/a-mate-releases/releases/latest/download/latest.json"
      ],
      "pubkey": "<PUBKEY>"
    }
  }
```
(`plugins`는 최상위 키. `app`/`bundle`과 형제. JSON 문법 유효성 확인: `python3 -c "import json;json.load(open('a-mate/src-tauri/tauri.conf.json'))"`.)

- [ ] **Step 5: JSON 유효성 + (가능 시) 프론트 재빌드 확인**

Run:
```bash
python3 -c "import json;json.load(open('/home/msaltnet/code/space-a/a-mate/src-tauri/tauri.conf.json'));print('ok')"
```
Expected: `ok`
> Rust 컴파일 검증은 Windows 빌드(Task 8)에서 수행 — WSL에는 MSVC/webview 툴체인이 없어 `cargo check` 불가.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src-tauri/Cargo.toml a-mate/src-tauri/src/lib.rs a-mate/src-tauri/capabilities/default.json a-mate/src-tauri/tauri.conf.json
git commit -m "feat(agent): register updater/process plugins and configure update feed"
```

---

### Task 6: 트레이 "업데이트 확인" 메뉴 항목

**Files:**
- Modify: `a-mate/src-tauri/src/tray.rs` (메뉴 아이템 + 이벤트 arm)

**Interfaces:**
- Consumes: Task 4의 프론트 이벤트 리스너(`update:check`), 기존 `show_chat`(tray.rs 내 사용 중).
- Produces: 트레이 메뉴에 "업데이트 확인" 항목, 클릭 시 chat 창 표시 + `update:check` emit.

- [ ] **Step 1: 메뉴 아이템 생성**

`a-mate/src-tauri/src/tray.rs` line 61(다른 `MenuItem::with_id` 근처)에 추가:
```rust
    let check_update = MenuItem::with_id(app, "check_update", "업데이트 확인", true, None::<&str>)?;
```

- [ ] **Step 2: 메뉴 배열에 삽입**

`tray.rs:62` `Menu::with_items` 배열에서 `&settings` 뒤에 `&check_update`를 추가:
```rust
    let menu = Menu::with_items(app, &[&open, &mascot, &reset_mascot, &realtime, &protect, &chatter_menu, &scan, &settings, &check_update, &autostart, &quit])?;
```

- [ ] **Step 3: 이벤트 arm 추가**

`tray.rs`의 `on_menu_event` match(line 80-168)에서 `"settings" => show_settings(app),` 뒤에 추가 (`tauri::Emitter`는 이미 사용 중):
```rust
            "check_update" => {
                show_chat(app);
                let _ = app.emit("update:check", ());
            }
```

- [ ] **Step 4: 문법 확인(패턴 일치)**

Run:
```bash
grep -n "check_update" /home/msaltnet/code/space-a/a-mate/src-tauri/src/tray.rs
```
Expected: 3곳(아이템 생성, 메뉴 배열, 이벤트 arm) 출력.
> 컴파일 검증은 Windows 빌드(Task 8)에서.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src-tauri/src/tray.rs
git commit -m "feat(agent): add tray menu item to check for updates"
```

---

### Task 7: 릴리스 스크립트 (수동 발행)

**Files:**
- Create: `a-mate/scripts/release-amate.sh`

**Interfaces:**
- Consumes: Task 1의 private key 파일, Task 5의 `createUpdaterArtifacts`/pubkey, `gh` CLI.
- Produces: 버전 태그, GitHub Release(`a-mate-releases`)에 `*-setup.exe` + `latest.json` 업로드.

- [ ] **Step 1: 릴리스 스크립트 작성**

`a-mate/scripts/release-amate.sh`:
```bash
#!/usr/bin/env bash
# a-mate 릴리스: 버전 갱신 → 서명 빌드 → latest.json 생성 → GitHub Release 발행
# 사용법: bash a-mate/scripts/release-amate.sh 0.2.0
set -euo pipefail

VERSION="${1:?사용법: release-amate.sh <version> (예: 0.2.0)}"
REPO=/home/msaltnet/code/space-a
WIN_BUILD_WSL=/mnt/c/Users/salt.jeong/amate-build/a-mate
WIN_BUILD_WIN='C:\Users\salt.jeong\amate-build\a-mate'
RELEASE_REPO="dev-team-404/a-mate-releases"
KEY_FILE="$HOME/.tauri/a-mate-updater.key"
KEY_PW=""   # 키 생성 시 빈 암호를 썼다면 그대로. 아니면 여기(또는 env)로 주입

CONF="$REPO/a-mate/src-tauri/tauri.conf.json"
CARGO="$REPO/a-mate/src-tauri/Cargo.toml"

echo "[1/6] 버전 $VERSION 반영 (tauri.conf.json + Cargo.toml)"
node -e "const f='$CONF';const j=require(f);j.version='$VERSION';require('fs').writeFileSync(f, JSON.stringify(j,null,2)+'\n')"
sed -i -E "0,/^version = \"[^\"]*\"/s//version = \"$VERSION\"/" "$CARGO"
git -C "$REPO" add "$CONF" "$CARGO"
git -C "$REPO" commit -m "chore(agent): release v$VERSION"
git -C "$REPO" tag "v$VERSION"

echo "[2/6] Windows 빌드 폴더 동기화"
mkdir -p "$WIN_BUILD_WSL"
rsync -a --delete --exclude 'node_modules/' --exclude 'target/' --exclude 'dist/' \
  --exclude '.git/' --exclude '.env' "$REPO/a-mate/" "$WIN_BUILD_WSL/"

echo "[3/6] 서명 빌드 (Windows)"
KEY_CONTENT="$(cat "$KEY_FILE")"
powershell.exe -NoProfile -Command "\
  [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
  \$env:CARGO_HTTP_CHECK_REVOKE='false'; \
  \$env:TAURI_SIGNING_PRIVATE_KEY='$KEY_CONTENT'; \
  \$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD='$KEY_PW'; \
  \$env:Path=\"\$env:USERPROFILE\.cargo\bin;\$env:Path\"; \
  Set-Location '$WIN_BUILD_WIN'; \
  npm install; \
  npm run tauri build"

echo "[4/6] latest.json 생성"
NSIS_DIR="$WIN_BUILD_WSL/target/release/bundle/nsis"
SETUP="$(ls -t "$NSIS_DIR"/*setup*.exe | head -1)"
SIG="$(cat "$SETUP.sig")"
BASENAME="$(basename "$SETUP")"
# GitHub 다운로드 URL은 공백을 점(.)으로 치환
URLNAME="${BASENAME// /.}"
cat > "$NSIS_DIR/latest.json" <<JSON
{
  "version": "$VERSION",
  "notes": "v$VERSION",
  "platforms": {
    "windows-x86_64": {
      "signature": "$SIG",
      "url": "https://github.com/$RELEASE_REPO/releases/download/v$VERSION/$URLNAME"
    }
  }
}
JSON

echo "[5/6] GitHub Release 발행 ($RELEASE_REPO)"
git -C "$REPO" push origin "v$VERSION"
gh release create "v$VERSION" --repo "$RELEASE_REPO" \
  --title "v$VERSION" --notes "Agent Mentor v$VERSION" \
  "$SETUP" "$NSIS_DIR/latest.json"

echo "[6/6] 완료: v$VERSION 발행됨 → $RELEASE_REPO"
```

- [ ] **Step 2: 실행 권한 + 문법 검사**

Run:
```bash
chmod +x /home/msaltnet/code/space-a/a-mate/scripts/release-amate.sh
bash -n /home/msaltnet/code/space-a/a-mate/scripts/release-amate.sh && echo "문법 OK"
```
Expected: `문법 OK`

- [ ] **Step 3: 커밋**

```bash
git add a-mate/scripts/release-amate.sh
git commit -m "chore(agent): add manual release script for signed updates"
```

---

### Task 8: Windows 빌드 + E2E 검증 (수동)

플러그인·서명·업데이트 흐름은 실제 Windows 빌드와 실릴리스로만 검증된다.

**Files:** (검증 전용 — 코드 변경 없음)

- [ ] **Step 1: 서명 빌드 성공 확인 (v0.1.0 기준 첫 빌드)**

`release-amate.sh`를 쓰거나 수동으로, Windows에서 `createUpdaterArtifacts: true` + `TAURI_SIGNING_PRIVATE_KEY`로 빌드:
```bash
bash /home/msaltnet/code/space-a/a-mate/scripts/release-amate.sh 0.1.0
```
Expected: `target/release/bundle/nsis/`에 `*-setup.exe` **및 `*-setup.exe.sig`** 생성. Rust 컴파일 성공(플러그인 등록 코드 포함).

- [ ] **Step 2: 릴리스 발행 확인**

Run:
```bash
gh release view v0.1.0 --repo dev-team-404/a-mate-releases --json assets -q '.assets[].name'
```
Expected: `Agent Mentor_0.1.0_x64-setup.exe`, `latest.json` 자산 존재.

- [ ] **Step 3: 이전 버전 설치본 준비**

v0.1.0 setup.exe를 Windows에 설치(직접). 앱 실행 확인.

- [ ] **Step 4: 새 버전 발행 후 자동 업데이트 E2E**

버전 올려 재발행:
```bash
bash /home/msaltnet/code/space-a/a-mate/scripts/release-amate.sh 0.1.1
```
그다음 v0.1.0 설치본 앱을 **재시작** → chat 창 상단 배너에 "새 버전 0.1.1 있음"이 뜨는지 확인 → [지금 업데이트] 클릭 → 다운로드·설치·재시작 후 버전이 0.1.1인지 확인. 트레이 "업데이트 확인"도 동작하는지 확인.
Expected: 배너 표시 → 업데이트 적용 → 재시작 후 최신 버전.
> 서명 불일치/네트워크 실패 시 배너 에러 문구 확인(에러 처리 검증).

- [ ] **Step 5: (커밋 없음)** — 검증 전용.

---

### Task 9: 문서화 & DoD (ADR + 로드맵 갱신 + 아카이브)

**Files:**
- Create: `docs/adr/0018-a-mate-auto-update-channel.md`
- Modify: `docs/design/a-mate/02-features.md`, `docs/design/a-mate/04-history-and-roadmap.md`
- Archive: 이 spec + plan을 `docs/archive/`로 (docs-archive 스킬)

- [ ] **Step 1: ADR 작성**

`docs/adr/0018-a-mate-auto-update-channel.md` — 결정: "a-mate 자동 업데이트는 `tauri-plugin-updater` + 공개 릴리스 전용 저장소(`a-mate-releases`) + updater 서명키(코드 서명 인증서 제외) + 수동 릴리스 스크립트로 한다." 배경·대안(사내/OCI 호스팅, 토큰 임베드)·결과를 기존 ADR 형식(0017 등 참고)에 맞춰 작성.

- [ ] **Step 2: 로드맵 문서 갱신**

`docs/design/a-mate/04-history-and-roadmap.md`의 "updater 활성화 | 뼈대만 | 보류" 행과 `02-features.md:125`의 "updater는 뼈대만(…보류)"를 이 구현 완료에 맞춰 갱신(예: "구현됨 — [ADR 0018]").

- [ ] **Step 3: 커밋**

```bash
git add docs/adr/0018-a-mate-auto-update-channel.md docs/design/a-mate/02-features.md docs/design/a-mate/04-history-and-roadmap.md
git commit -m "docs(adr): record a-mate auto-update channel decision"
```

- [ ] **Step 4: 아카이브 (docs-archive 스킬)**

이 spec(`2026-07-22-auto-update-design.md`)과 plan(`2026-07-22-auto-update.md`)을 `docs-archive` 스킬로 `docs/archive/` 미러로 이동(ADR 0013 DoD 규칙). 같은 PR에서 수행.

- [ ] **Step 5: PR 생성 & 머지**

`feat/a-mate-auto-update` → `main` PR 생성, 리뷰 후 머지.

---

## Self-Review

**Spec coverage:**
- UX(인앱 자동 업데이트) → Task 3/4 (배너, 시작 시 체크, 다운로드·설치·재시작). ✅
- `tauri-plugin-updater` + `tauri-plugin-process` → Task 2(JS), Task 5(Rust). ✅
- GitHub Releases 공개 저장소 피드 → Task 1(저장소), Task 5(endpoints), Task 7(발행). ✅
- updater 서명키만(코드 서명 제외) → Task 1(키 생성), Task 5(pubkey), Task 7(서명 빌드). ✅
- 수동 릴리스 스크립트 → Task 7. ✅
- 확인 시점(시작 시+트레이 수동), 배너 위치(chat 상단), 조용한 실패 → Task 4(onMount, onUpdateCheckRequested, shouldShow), Task 6(트레이). ✅
- 에러 처리 → Task 3(error phase), Task 8 Step 4(E2E 에러 검증). ✅
- 테스트(Vitest 순수 헬퍼 + 수동 E2E) → Task 3, Task 8. ✅
- ADR + 로드맵 갱신 + 아카이브 → Task 9. ✅

**Placeholder scan:** `<PUBKEY>`(Task 5)는 Task 1에서 생성되는 실제 값으로 치환하는 지시가 명확 — 미해결 placeholder 아님. 그 외 TBD/TODO 없음.

**Type consistency:** `UpdatePhase`의 6개 variant가 Task 3 정의와 Task 4 스토어 사용에서 일치(`idle/checking/available/downloading/uptodate/error`). `runCheck`/`installUpdate`/`dismiss` 시그니처가 Task 4 Interfaces와 컴포넌트 사용에서 일치. 이벤트명 `update:check`가 Task 4(리스너)·Task 6(emit)에서 일치. ✅
