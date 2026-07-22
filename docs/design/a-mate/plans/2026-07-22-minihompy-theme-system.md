# 미니홈피 테마 시스템 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 미니홈피에 테마 모드(라이트/다크/시스템) × 색상 세트(스킨 4종)를 도입하고, 흩어진 색을 시맨틱 토큰으로 재구성해 코칭 탭의 "흰 배경+흰 글자" 저대비 버그를 없앤다.

**Architecture:** `theme.css`를 시맨틱 토큰 계약 + 8개(스킨×모드) 블록 + 상태색 + **레거시 별칭**으로 재작성한다. `theme.ts`가 `system`을 `prefers-color-scheme`로 확정해 `<html data-theme data-skin>`을 세팅하고, Rust `theme_get`/`theme_set`(SqliteStore KV)로 영속·`theme:changed` 이벤트로 세 창을 동기화한다. 컨트롤 UI는 미니홈피 설정 탭(`LifeSettingsTab`)에 둔다.

**Tech Stack:** Svelte 5 (runes) + Vite + TypeScript (Vitest), Tauri v2 + Rust (cargo test), CSS custom properties. Windows 전용 — 빌드/테스트는 네이티브 PowerShell.

**설계 스펙:** `docs/design/a-mate/specs/2026-07-21-minihompy-theme-system-design.md` (팔레트 값·토큰 계약·범위의 근거)

## Global Constraints

- 스택: Tauri v2 + Svelte 5 + TS. **v1 API 금지**(SystemTray/WindowBuilder 등).
- 색·radius·그림자는 **`theme.css`에서만** 정의. 컴포넌트는 시맨틱 토큰만 참조하고, 배경 토큰을 쓰면 짝 글자 토큰을 함께 지정.
- 빌드/실행/테스트는 **네이티브 Windows PowerShell**에서, `a-mate/`에서. WSL 금지.
- 커밋: Conventional Commits, 영어 subject.
- **v1 범위 밖(수정 금지, 별칭으로 렌더 유지):** `LifeView.svelte`, `MiniLife.svelte`, `ModelMix.svelte`, 스프라이트(`robot/*`), `interior/` 에셋.
- 기본값: 밝기 `system`, 색상 세트 `sky`. 스킨 id: `sky|mint|peach|lavender`. 모드 id: `light|dark|system`.

---

### Task 1: 토큰 재작성 + 테마 resolver (`theme.css`, `theme.ts`)

**Files:**
- Modify(rewrite): `a-mate/src/lib/theme.css`
- Create: `a-mate/src/lib/theme.ts`
- Test: `a-mate/src/lib/theme.test.ts`

**Interfaces:**
- Produces: `type ThemeMode='light'|'dark'|'system'`, `type ThemeSkin='sky'|'mint'|'peach'|'lavender'`, `interface ThemeSettings{mode:ThemeMode;skin:ThemeSkin}`, `DEFAULT_THEME`, `resolveTheme(mode,systemPrefersDark):'light'|'dark'`, `normalizeTheme(raw):ThemeSettings`, `initTheme():Promise<void>`, `setTheme(next:ThemeSettings):Promise<void>`, `getTheme():ThemeSettings`.
- Consumes(런타임): Tauri `invoke('theme_get')`/`invoke('theme_set',{mode,skin})`, event `theme:changed`(Task 2).

- [ ] **Step 1: `theme.test.ts` 작성 (실패 예정)**

```ts
import { describe, it, expect } from 'vitest';
import { resolveTheme, normalizeTheme, DEFAULT_THEME } from './theme';

describe('resolveTheme', () => {
  it('light/dark는 시스템과 무관하게 확정', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
  it('system은 OS 선호를 따름', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });
});

describe('normalizeTheme', () => {
  it('유효 값은 유지', () => {
    expect(normalizeTheme({ mode: 'dark', skin: 'peach' })).toEqual({ mode: 'dark', skin: 'peach' });
  });
  it('null/깨진 값은 기본값 폴백', () => {
    expect(normalizeTheme(null)).toEqual(DEFAULT_THEME);
    expect(normalizeTheme({ mode: 'x' as any, skin: 'y' as any })).toEqual(DEFAULT_THEME);
  });
});
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `npm test -- theme.test` (a-mate/)
Expected: FAIL — `theme.ts` 모듈/함수 미존재.

- [ ] **Step 3: `theme.ts` 작성**

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type ThemeMode = 'light' | 'dark' | 'system';
export type ThemeSkin = 'sky' | 'mint' | 'peach' | 'lavender';
export interface ThemeSettings { mode: ThemeMode; skin: ThemeSkin }

const MODES: ThemeMode[] = ['light', 'dark', 'system'];
const SKINS: ThemeSkin[] = ['sky', 'mint', 'peach', 'lavender'];
export const DEFAULT_THEME: ThemeSettings = { mode: 'system', skin: 'sky' };

/** system이면 OS 선호로, 아니면 그대로. 순수 함수. */
export function resolveTheme(mode: ThemeMode, systemPrefersDark: boolean): 'light' | 'dark' {
  if (mode === 'dark') return 'dark';
  if (mode === 'light') return 'light';
  return systemPrefersDark ? 'dark' : 'light';
}

/** 저장값이 깨졌을 때 안전한 기본값으로 정규화. 순수 함수. */
export function normalizeTheme(raw: Partial<ThemeSettings> | null | undefined): ThemeSettings {
  const mode = raw && MODES.includes(raw.mode as ThemeMode) ? (raw.mode as ThemeMode) : DEFAULT_THEME.mode;
  const skin = raw && SKINS.includes(raw.skin as ThemeSkin) ? (raw.skin as ThemeSkin) : DEFAULT_THEME.skin;
  return { mode, skin };
}

let current: ThemeSettings = { ...DEFAULT_THEME };
const darkMql = () => window.matchMedia('(prefers-color-scheme: dark)');

function paint(): void {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(current.mode, darkMql().matches);
  root.dataset.skin = current.skin;
}

/** 각 창 엔트리(chat/mascot/settings)에서 1회 호출. */
export async function initTheme(): Promise<void> {
  paint(); // 기본값으로 즉시 적용 (FOUC 방지)
  try {
    current = normalizeTheme(await invoke<ThemeSettings>('theme_get'));
    paint();
  } catch (e) {
    console.warn('theme_get 실패, 기본값 유지:', e);
  }
  darkMql().addEventListener('change', () => { if (current.mode === 'system') paint(); });
  await listen<ThemeSettings>('theme:changed', (ev) => { current = normalizeTheme(ev.payload); paint(); });
}

/** 설정 UI에서 호출. 낙관적 적용 + 저장(서버가 theme:changed 브로드캐스트). */
export async function setTheme(next: ThemeSettings): Promise<void> {
  current = normalizeTheme(next);
  paint();
  try { await invoke('theme_set', { mode: current.mode, skin: current.skin }); }
  catch (e) { console.warn('theme_set 실패:', e); }
}

export function getTheme(): ThemeSettings { return { ...current }; }
```

- [ ] **Step 4: `theme.css` 전면 재작성**

```css
/* SPACE A 미니홈피 테마 — 테마 모드(light/dark) × 색상 세트(스킨).
   색·radius·그림자는 반드시 여기서만. 컴포넌트는 시맨틱 토큰만 참조하고,
   배경 토큰을 쓰면 짝 글자 토큰을 함께 지정한다. (설계: specs/2026-07-21-minihompy-theme-system-design.md) */

/* 모드/스킨 무관 상수 */
:root { --radius-s: 8px; --radius-m: 12px; --radius-l: 16px; }

/* ── 상태·기능 파스텔 (스킨 공유, 모드별) + color-scheme + shadow ── */
:root[data-theme="light"] {
  --lav-surface: #e7e9fb; --lav-ink: #37356b;
  --coral: #ffb3ad; --coral-ink: #6b241d;
  --cream: #ffe6bf; --cream-ink: #6b5218;
  --danger: #b74444;
  --shadow-soft: 0 8px 24px rgba(20, 26, 45, 0.10);
  color-scheme: light;
}
:root[data-theme="dark"] {
  --lav-surface: #2c2c54; --lav-ink: #c9c2f7;
  --coral: #7a3a38; --coral-ink: #ffd9d5;
  --cream: #4a3f2a; --cream-ink: #ffe4b0;
  --danger: #f2a5a0;
  --shadow-soft: 0 8px 24px rgba(0, 0, 0, 0.40);
  color-scheme: dark;
}

/* ── 스킨 × 모드: accent 계열 + 표면·글자 + 레거시 별칭 ──
   레거시 별칭: v1 아웃-스코프(LifeView/MiniLife 등)가 계속 렌더되도록 구 토큰명을 신 토큰에 매핑.
   --pastel-lav는 보더·표면 이중사용이었으나 별칭은 보더(--line)로 두고, 인스코프 표면 용도는 마이그레이션에서 --lav-surface로 정정. */
:root[data-skin="sky"][data-theme="light"] {
  --bg: #eef5fc; --frame: #ffffff; --frame-2: #e6edf7; --surface-inset: #eef3fb;
  --line: #d9e2f1; --text: #26303f; --text-soft: #66738a;
  --accent: #84c9ef; --accent-ink: #0e2230; --accent-strong: #1670b8;
  --accent-tint: #e3f2fc; --accent-tint-b: #bfe2f6;
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="sky"][data-theme="dark"] {
  --bg: #0f1626; --frame: #1b2544; --frame-2: #131b31; --surface-inset: #243050;
  --line: #313d61; --text: #e6ecf8; --text-soft: #98a4c6;
  --accent: #7fc9f0; --accent-ink: #0c1826; --accent-strong: #7fc9f0;
  --accent-tint: rgba(127,201,240,.12); --accent-tint-b: rgba(127,201,240,.30);
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="mint"][data-theme="light"] {
  --bg: #ecf8f3; --frame: #ffffff; --frame-2: #e4f0ea; --surface-inset: #ecf5f1;
  --line: #d3e7de; --text: #26332e; --text-soft: #647469;
  --accent: #5cd0b0; --accent-ink: #0e2620; --accent-strong: #0b8368;
  --accent-tint: #e2f7ef; --accent-tint-b: #bfe9da;
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="mint"][data-theme="dark"] {
  --bg: #0e1c19; --frame: #16241f; --frame-2: #0f1a17; --surface-inset: #1f302a;
  --line: #2c3f38; --text: #e6f0ec; --text-soft: #94a89f;
  --accent: #7ee8c8; --accent-ink: #0a1613; --accent-strong: #7ee8c8;
  --accent-tint: rgba(126,232,200,.12); --accent-tint-b: rgba(126,232,200,.30);
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="peach"][data-theme="light"] {
  --bg: #fdf3ec; --frame: #ffffff; --frame-2: #f7e9de; --surface-inset: #fbeee4;
  --line: #f0dccb; --text: #3a2a20; --text-soft: #8a6f5c;
  --accent: #f4b183; --accent-ink: #3a1e0e; --accent-strong: #b5501f;
  --accent-tint: #fdeadd; --accent-tint-b: #f6d3b8;
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="peach"][data-theme="dark"] {
  --bg: #291c12; --frame: #392819; --frame-2: #211710; --surface-inset: #453321;
  --line: #57422c; --text: #f4ede7; --text-soft: #c6ad97;
  --accent: #f0b58c; --accent-ink: #241206; --accent-strong: #f0b58c;
  --accent-tint: rgba(240,181,140,.12); --accent-tint-b: rgba(240,181,140,.30);
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="lavender"][data-theme="light"] {
  --bg: #f3f0fc; --frame: #ffffff; --frame-2: #eae6f8; --surface-inset: #f0ecfb;
  --line: #e0d9f2; --text: #2c2740; --text-soft: #6f6885;
  --accent: #b9a6f0; --accent-ink: #1e163a; --accent-strong: #6c4fd0;
  --accent-tint: #efe9fb; --accent-tint-b: #ddd0f5;
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}
:root[data-skin="lavender"][data-theme="dark"] {
  --bg: #1e1a38; --frame: #2a2450; --frame-2: #1d183a; --surface-inset: #332c62;
  --line: #443c70; --text: #ece9f7; --text-soft: #a8a2ca;
  --accent: #b8a7f7; --accent-ink: #140f28; --accent-strong: #b8a7f7;
  --accent-tint: rgba(184,167,247,.12); --accent-tint-b: rgba(184,167,247,.30);
  --bg-grad: var(--bg); --frame-bg: var(--frame); --panel2: var(--surface-inset);
  --ink: var(--text); --ink-soft: var(--text-soft);
  --pastel-mint: var(--accent); --pastel-coral: var(--coral); --pastel-cream: var(--cream);
  --pastel-lav: var(--line); --lav: var(--lav-ink);
  --mint-tint: var(--accent-tint); --mint-tint-b: var(--accent-tint-b);
}

/* 스크롤바 — 토큰으로 자동 테마 대응 */
::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--line); border-radius: 999px; }
::-webkit-scrollbar-thumb:hover { background: var(--accent); }
```

> 참고: theme.ts가 엔트리에서 마운트 전 `paint()`로 `data-theme`/`data-skin`을 즉시 세팅하므로 별도 `:root` 폴백 팔레트는 두지 않는다(첫 페인트 전 잠깐의 빈 `#app`만 존재).

- [ ] **Step 5: 테스트 통과 확인**

Run: `npm test -- theme.test`
Expected: PASS (4 테스트).

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src/lib/theme.css a-mate/src/lib/theme.ts a-mate/src/lib/theme.test.ts
git commit -m "feat(frontend): add theme token system and resolver (skins x modes)"
```

---

### Task 2: Rust 테마 커맨드 (`theme_get`/`theme_set` + emit)

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (커맨드 + 검증 헬퍼 + 테스트)
- Modify: `a-mate/src-tauri/src/lib.rs:359-409` (invoke_handler 등록)

**Interfaces:**
- Produces(commands): `theme_get() -> Result<ThemeSettings{mode,skin}, String>`, `theme_set(mode:String, skin:String) -> Result<(),String>` (성공 시 `theme:changed` emit), 순수 `validate_theme(mode:&str, skin:&str) -> Result<(),String>`.
- Consumes: `AppState`, `lock(&state)`, `SqliteStore::get_setting/set_setting`, `tauri::Emitter`.

- [ ] **Step 1: 검증 헬퍼 단위 테스트 작성 (실패 예정)**

`commands.rs`의 기존 `#[cfg(test)] mod tests { use super::*; … }` 블록에 추가:

```rust
#[test]
fn validate_theme_accepts_known_and_rejects_unknown() {
    assert!(validate_theme("system", "sky").is_ok());
    assert!(validate_theme("dark", "peach").is_ok());
    assert!(validate_theme("light", "lavender").is_ok());
    assert!(validate_theme("neon", "sky").is_err());
    assert!(validate_theme("dark", "rainbow").is_err());
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent-mentor-app validate_theme` (a-mate/)
Expected: FAIL — `validate_theme` 미정의.

- [ ] **Step 3: 커맨드·헬퍼 구현**

`commands.rs`에 추가 (엔진 설정 커맨드 인근, `engine_settings_set` 아래 등):

```rust
/// 미니홈피 테마 설정 스냅샷. mode: light|dark|system, skin: sky|mint|peach|lavender.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeSettings {
    pub mode: String,
    pub skin: String,
}

const THEME_MODES: &[&str] = &["light", "dark", "system"];
const THEME_SKINS: &[&str] = &["sky", "mint", "peach", "lavender"];

/// 허용 집합 검증 (순수 — 테스트 대상)
pub fn validate_theme(mode: &str, skin: &str) -> Result<(), String> {
    if !THEME_MODES.contains(&mode) {
        return Err(format!("허용되지 않은 테마 모드: {mode}"));
    }
    if !THEME_SKINS.contains(&skin) {
        return Err(format!("허용되지 않은 색상 세트: {skin}"));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn theme_get(state: State<AppState>) -> Result<ThemeSettings, String> {
    let guard = lock(&state)?;
    let get = |k: &str, d: &str| {
        let v = guard.get_setting(k).ok().flatten().unwrap_or_default();
        if v.trim().is_empty() { d.to_string() } else { v }
    };
    let mode = get("theme_mode", "system");
    let skin = get("theme_skin", "sky");
    // 저장값이 깨졌으면 기본값으로 폴백
    if validate_theme(&mode, &skin).is_ok() {
        Ok(ThemeSettings { mode, skin })
    } else {
        Ok(ThemeSettings { mode: "system".into(), skin: "sky".into() })
    }
}

#[tauri::command(async)]
pub fn theme_set(
    app: tauri::AppHandle,
    state: State<AppState>,
    mode: String,
    skin: String,
) -> Result<(), String> {
    validate_theme(&mode, &skin)?;
    {
        let guard = lock(&state)?;
        guard.set_setting("theme_mode", &mode).map_err(|e| e.to_string())?;
        guard.set_setting("theme_skin", &skin).map_err(|e| e.to_string())?;
    } // 락 해제 후 브로드캐스트
    use tauri::Emitter;
    let _ = app.emit("theme:changed", ThemeSettings { mode, skin });
    Ok(())
}
```

- [ ] **Step 4: invoke_handler 등록**

`lib.rs`의 `tauri::generate_handler![ … ]` 목록에 추가 (`commands::engine_settings_set,` 인근):

```rust
                commands::theme_get,
                commands::theme_set,
```

- [ ] **Step 5: 테스트·컴파일 확인**

Run: `cargo test -p agent-mentor-app validate_theme`
Expected: PASS. 이어서 `cargo build -p agent-mentor-app` 로 커맨드 시그니처 컴파일 확인(경고 0 목표).

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs
git commit -m "feat(backend): add theme_get/theme_set commands with theme:changed event"
```

---

### Task 3: 세 창 엔트리 연동 (`initTheme`)

**Files:**
- Modify: `a-mate/src/chat.ts`, `a-mate/src/mascot.ts`, `a-mate/src/settings.ts`

**Interfaces:**
- Consumes: `initTheme()` (Task 1), `theme_get`/`theme:changed` (Task 2).
- 이 태스크 후 **레거시 별칭 덕에 앱 전체가 선택된 스킨·모드로 렌더**된다(컴포넌트 마이그레이션 전이라도).

- [ ] **Step 1: `chat.ts` 수정**

```ts
import { mount } from 'svelte';
import App from './App.svelte';
import { initTheme } from './lib/theme';

initTheme();
mount(App, { target: document.getElementById('app')! });
```

- [ ] **Step 2: `mascot.ts`·`settings.ts` 동일 패턴 적용**

각 파일 최상단 import에 `import { initTheme } from './lib/theme';` 추가하고, `mount(...)` 직전에 `initTheme();` 한 줄 추가. (루트 컴포넌트는 각각 기존 것 유지 — Mascot/Settings.)

- [ ] **Step 3: 개발 실행으로 렌더 확인 (수동)**

Run: `npm run tauri dev` (a-mate/, 네이티브 PowerShell)
Expected: 미니홈피가 **하늘 스킨**(시스템 밝기)으로 렌더. `console.warn` 테마 오류 없음. (아직 스킨 전환 UI는 Task 4.)

- [ ] **Step 4: 커밋**

```bash
git add a-mate/src/chat.ts a-mate/src/mascot.ts a-mate/src/settings.ts
git commit -m "feat(frontend): initialize theme on all three window entries"
```

---

### Task 4: 설정 탭 테마 UI (`LifeSettingsTab.svelte`)

**Files:**
- Modify: `a-mate/src/lib/ui/LifeSettingsTab.svelte`

**Interfaces:**
- Consumes: `getTheme()`, `setTheme()`, `ThemeMode`, `ThemeSkin` (Task 1).

- [ ] **Step 1: 스크립트에 테마 상태·핸들러 추가**

`<script lang="ts">` 상단 import에 추가:

```ts
import { getTheme, setTheme, type ThemeMode, type ThemeSkin } from '../theme';
```

`<script>` 내 상태/상수 추가:

```ts
let themeMode = $state<ThemeMode>(getTheme().mode);
let themeSkin = $state<ThemeSkin>(getTheme().skin);
const SKINS: { id: ThemeSkin; label: string; dot: string }[] = [
  { id: 'sky', label: '하늘', dot: '#84c9ef' },
  { id: 'mint', label: '민트', dot: '#5cd0b0' },
  { id: 'peach', label: '살구', dot: '#f4b183' },
  { id: 'lavender', label: '라벤더', dot: '#b9a6f0' },
];
const MODES: [ThemeMode, string][] = [['light','라이트'],['dark','다크'],['system','시스템']];
function pickMode(m: ThemeMode) { themeMode = m; setTheme({ mode: m, skin: themeSkin }); }
function pickSkin(s: ThemeSkin) { themeSkin = s; setTheme({ mode: themeMode, skin: s }); }
```

- [ ] **Step 2: `<section>` 마크업 맨 위에 테마 섹션 추가**

기존 `<section>...일촌 관리...` 안에서, 최상단(`<h2>일촌 관리</h2>` 앞)에 삽입:

```svelte
<h2>테마</h2>
<p>밝기</p>
<nav class="seg">
  {#each MODES as opt}
    <button class:active={themeMode === opt[0]} onclick={() => pickMode(opt[0])}>{opt[1]}</button>
  {/each}
</nav>
<p>색상 세트</p>
<div class="skins">
  {#each SKINS as s (s.id)}
    <button class="swatch" class:active={themeSkin === s.id} onclick={() => pickSkin(s.id)} title={s.label}>
      <span class="dot" style="background:{s.dot}"></span>{s.label}
    </button>
  {/each}
</div>
<hr />
```

> 스와치 dot 색은 스킨 고정색을 보여줘야 하므로 인라인 `style`(마크업)에 둔다 — `<style>` 블록의 하드코딩 금지 가드(Task 6)와 무관.

- [ ] **Step 3: `<style>`에 테마 섹션 스타일 추가 + 기존 하드코딩/구토큰 정정**

`<style>`에 추가:

```css
.seg { display: flex; border: 1px solid var(--line); border-radius: 9px; overflow: hidden; width: fit-content; margin-top: 6px; }
.seg button { border: 0; border-right: 1px solid var(--line); background: transparent; color: var(--text); padding: 7px 16px; cursor: pointer; font: inherit; }
.seg button:last-child { border-right: 0; }
.seg button.active { background: var(--accent); color: var(--accent-ink); font-weight: 700; }
.skins { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 6px; }
.swatch { display: flex; align-items: center; gap: 6px; border: 1px solid var(--line); background: var(--surface-inset); color: var(--text); border-radius: 99px; padding: 5px 12px 5px 6px; cursor: pointer; font: inherit; font-size: 12px; }
.swatch.active { border-color: var(--accent-strong); color: var(--text); font-weight: 700; }
.swatch .dot { width: 16px; height: 16px; border-radius: 50%; border: 1px solid var(--line); }
```

같은 `<style>`의 기존 값 정정(구 토큰/하드코딩):
- `section>p{color:var(--ink-soft)}` → `color: var(--text-soft)`
- `label{...background:var(--pastel-cream)...}` → `background: var(--cream); color: var(--cream-ink)`
- `button{...background:var(--pastel-lav)...}` → `background: var(--lav-surface); color: var(--lav-ink)`
- `button.active{background:var(--accent);color:white}` → `color: var(--accent-ink)`
- `hr{...border-top:1px solid var(--pastel-lav)...}` → `border-top: 1px solid var(--line)`
- `.error{color:#b74444}` → `color: var(--danger)`

- [ ] **Step 4: 실행 확인 (수동)**

Run: `npm run tauri dev`
Expected: 설정 탭에 "테마" 섹션. 밝기·색상 세트 변경 시 **미니홈피와 마스코트 창이 즉시** 바뀜(이벤트 동기화). 앱 재시작 후에도 선택 유지(영속).

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/LifeSettingsTab.svelte
git commit -m "feat(frontend): add theme controls (brightness + skin) to settings tab"
```

---

### Task 5: 하드코딩 색 제거 — 컴포넌트 마이그레이션

원 버그(코칭 흰-배경) 포함, 인스코프 컴포넌트의 `<style>` 하드코딩 hex를 토큰으로 치환한다. 별칭 덕에 렌더는 이미 되지만, **하드코딩 hex는 저대비 원인이자 Task 6 가드 대상**이므로 반드시 제거한다. 각 항목은 정확한 old→new 치환.

**Files (모두 `a-mate/src/`):**
- `lib/ui/CoachTab.svelte`, `App.svelte`, `lib/ui/DiaryTab.svelte`, `lib/ui/home/TipCard.svelte`, `lib/ui/SessionModal.svelte`, `lib/ui/GuestbookTab.svelte`, `Settings.svelte`, `Mascot.svelte`

- [ ] **Step 1: CoachTab — 원 버그 수정**

`lib/ui/CoachTab.svelte` `<style>`:
- `pre { background: #f4f1fa; … }` → `background: var(--surface-inset); color: var(--text);`
- `.draft-body { background: #f4f1fa; … }` → `background: var(--surface-inset); color: var(--text);`
- `.draft-saved code { background: #f4f1fa; … }` → `background: var(--surface-inset);`

- [ ] **Step 2: App.svelte — accent/coral 위 글자**

`App.svelte` `<style>`:
- `.tabs button.active { … color: #0b3327; … }` → `color: var(--accent-ink);`
- `.badge { … color: #3a1512; … }` → `color: var(--coral-ink);`

- [ ] **Step 3: DiaryTab / TipCard**

`lib/ui/DiaryTab.svelte`:
- `.cells button.sel { background: var(--accent); color: #fff; }` → `color: var(--accent-ink);`
- `.cells button.sel .dot { background: #fff; }` → `background: var(--accent-ink);`

`lib/ui/home/TipCard.svelte`:
- `… color: #0b3327; …` → `color: var(--accent-ink);`

- [ ] **Step 4: SessionModal / GuestbookTab**

`lib/ui/SessionModal.svelte`:
- `background: #fdf9ef44;` → `background: var(--cream); color: var(--cream-ink);`
- `article.assistant { … background: #eef7f344; }` → `background: var(--accent-tint);`

`lib/ui/GuestbookTab.svelte` `<style>` (버튼이 `--accent` 배경 위 흰 글자):
- `color: white`(accent 배경 버튼) → `color: var(--accent-ink)`. (그 외 구 토큰은 별칭으로 유지되나, 접하는 김에 `--ink*`→`--text*`, `--pastel-*`→해당 신 토큰으로 정정 권장.)

- [ ] **Step 5: Settings 창 / Mascot 창**

`Settings.svelte` `<style>`:
- `button.primary { … color: #fff; … }` → `color: var(--accent-ink);`

`Mascot.svelte` `<style>` (라이트 컨텍스트 메뉴 팔레트 → 토큰):
- `.menu { background: #f3f3f3; color: #1f1f1f; }` → `background: var(--frame); color: var(--text);`
- `.menu-title { … color: #666; … }` → `color: var(--text-soft);`
- 스크롤바 `#c7c7c7` → `var(--line)`, hover `#aaa` → `var(--text-soft)`
- `.item { border: 1px solid #d9d9d9; background: #fff; color: #1f1f1f; }` → `border: 1px solid var(--line); background: var(--frame); color: var(--text);`
- `.item:hover { background: #fafafa; border-color: #a9a9a9; }` → `background: var(--surface-inset); border-color: var(--text-soft);`
- `.menu .item:hover 계열의 accent 위 #fff`가 있으면 → `var(--accent-ink)`

- [ ] **Step 6: 인스코프 잔여 구 토큰 스윕 (권장, 접하는 파일 한정)**

접한 파일 내에서 `var(--ink)`→`var(--text)`, `var(--ink-soft)`→`var(--text-soft)`, `var(--frame-bg)`→`var(--frame)`, `var(--panel2)`→`var(--surface-inset)`, `var(--bg-grad)`→`var(--bg)`, `var(--pastel-mint)`→`var(--accent)`, `var(--pastel-coral)`→`var(--coral)`, `var(--pastel-cream)`→`var(--cream)`, `var(--mint-tint*)`→`var(--accent-tint*)`로 정정. (별칭은 아웃-스코프 방 뷰 때문에 **삭제하지 않는다**.)

- [ ] **Step 7: 빌드·실행 확인 (수동)**

Run: `npm run build` (a-mate/, 타입/컴파일 확인) 이어서 `npm run tauri dev`
Expected: 스킨 4 × 밝기 2 스팟체크. **코칭 탭 드래프트/코드 박스 글자가 또렷**(원 버그 해소). 라벤더/살구 다크가 과하게 어둡지 않음.

- [ ] **Step 8: 커밋**

```bash
git add a-mate/src/App.svelte a-mate/src/Settings.svelte a-mate/src/Mascot.svelte a-mate/src/lib/ui/CoachTab.svelte a-mate/src/lib/ui/DiaryTab.svelte a-mate/src/lib/ui/SessionModal.svelte a-mate/src/lib/ui/GuestbookTab.svelte a-mate/src/lib/ui/home/TipCard.svelte
git commit -m "fix(frontend): replace hardcoded colors with theme tokens, fix coaching low-contrast panels"
```

---

### Task 6: 하드코딩 색 회귀 가드 테스트

**Files:**
- Create: `a-mate/src/lib/no-hardcoded-colors.test.ts`

**Interfaces:**
- Consumes: `node:fs`, `node:path` (Vitest는 Node 환경). CWD = `a-mate/`.

- [ ] **Step 1: 가드 테스트 작성**

```ts
import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// v1 아웃-스코프(방 뷰·차트) 및 토큰 정의 파일은 예외.
const ALLOWLIST = new Set<string>([
  'lib/ui/LifeView.svelte',
  'lib/ui/MiniLife.svelte',
  'lib/ui/home/ModelMix.svelte',
]);

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (name.endsWith('.svelte')) out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
  }
}

/** <style>…</style> 내부만 추출 */
function styleBlocks(src: string): string {
  const m = src.match(/<style[^>]*>([\s\S]*?)<\/style>/gi) ?? [];
  return m.join('\n');
}

const HEX = /#[0-9a-fA-F]{3,8}\b/;

describe('컴포넌트 <style>에 하드코딩 hex 색 없음', () => {
  const root = join(process.cwd(), 'src');
  const files: string[] = [];
  walk(root, join(process.cwd()), files);

  for (const rel of files.map((f) => f.replace(/^src\//, ''))) {
    const target = `src/${rel}`;
    it(rel, () => {
      if (ALLOWLIST.has(rel)) return; // 아웃-스코프
      const css = styleBlocks(readFileSync(join(process.cwd(), target), 'utf8'));
      const hit = css.split('\n').find((l) => HEX.test(l));
      expect(hit, `하드코딩 hex 발견: ${hit?.trim()}`).toBeUndefined();
    });
  }
});
```

- [ ] **Step 2: 실행 — 통과 확인**

Run: `npm test -- no-hardcoded-colors`
Expected: PASS. 실패 시 해당 파일의 `<style>` hex를 Task 5 방식으로 토큰화(또는 정당한 아웃-스코프면 ALLOWLIST에 추가하고 사유를 주석으로).

- [ ] **Step 3: 커밋**

```bash
git add a-mate/src/lib/no-hardcoded-colors.test.ts
git commit -m "test(frontend): guard against hardcoded hex colors in component styles"
```

---

### Task 7: 전체 QA + 마무리

**Files:** 없음(검증/문서).

- [ ] **Step 1: 전체 테스트**

Run: `npm test` 그리고 `cargo test` (a-mate/)
Expected: 프론트/러스트 전부 PASS.

- [ ] **Step 2: 수동 QA 체크리스트 (`npm run tauri dev`)**

- [ ] 스킨 4종 × 밝기(라이트/다크) 각 미니홈피 홈/코칭/다이어리/채팅/방명록/설정 스팟체크
- [ ] **코칭 드래프트/코드 박스 가독성**(원 버그) — 모든 스킨×밝기에서 또렷
- [ ] 설정 탭에서 변경 시 **미니홈피·마스코트·설정 창 즉시 동기화**
- [ ] 밝기=`시스템`에서 OS 다크/라이트 토글 시 반영
- [ ] 앱 재시작 후 선택(모드·스킨) 유지
- [ ] 마스코트 우클릭 메뉴가 선택 테마로 렌더
- [ ] 방 뷰(LifeView/MiniLife)는 기존 웜톤 유지(별칭) — 깨지지 않음

- [ ] **Step 3: DoD — 문서 아카이브**

`docs-archive` 스킬로 이 계획과 스펙을 `docs/archive/` 미러로 이관(ADR 0013). 같은 PR에 포함.

- [ ] **Step 4: PR 준비 완료 처리**

draft PR #84를 ready로 전환하고 설명 갱신(구현 반영). base `main`.

---

## Self-Review

**Spec coverage:**
- §3 2축(data-theme/data-skin) → Task 1(css)·3(paint). ✓
- §4 토큰 계약 + §4.1 매핑 → Task 1(css 별칭)·5(치환). ✓
- §5 팔레트 값(스킨×모드, 상태색, --danger) → Task 1 css 전량. ✓
- §6 resolver·FOUC·이벤트·3창 init → Task 1(theme.ts)·3. ✓
- §7 SqliteStore·theme_get/set·theme:changed → Task 2. ✓
- §8 설정 탭 UI(LifeSettingsTab) → Task 4. ✓
- §9 마이그레이션(코칭 버그 포함, 아웃-스코프 제외) → Task 5. ✓
- §11 테스트(resolver 단위·Rust 검증·회귀 가드·수동 QA) → Task 1·2·6·7. ✓

**Placeholder scan:** 코드 스텝은 전부 실제 코드/치환쌍 포함. TODO/TBD 없음. ✓

**Type consistency:** `ThemeSettings{mode,skin}`(TS/Rust 동일 필드), `resolveTheme`/`normalizeTheme`/`setTheme`/`getTheme`/`initTheme` 시그니처가 Task 1 정의와 Task 3·4 사용처 일치. 커맨드명 `theme_get`/`theme_set`, 이벤트 `theme:changed` 전 태스크 일치. ✓
