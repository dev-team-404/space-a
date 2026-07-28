# sprite 서브시스템 (G6 얼굴 아이콘 + H2 매일 마스코트 컷) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 방명록 아바타용 얼굴 아이콘을 sprite에서 잘라 캐시하고(G6), 새 일기 타이밍에 일기 파생 추상 장면으로 매일 홈 컷+캡션을 생성한다(H2).

**Architecture:** G6 = `get_face_icon` 커맨드의 **lazy 재료화**(mtime 비교 — sprite 쓰기 훅 불변). H2 = 스캔 말미 **리컨실리에이션**(`daily_cut.json` 상태 vs 최신 일기 날짜, 일일 3회 상한) → 텍스트 엔진 1회(장면+캡션 JSON) → 이미지 엔진 1회(스타일 앵커 공유) → 최신 1장 교체 + `daily_cut:ready` emit. 순수 로직(크롭·샷 pick·판단·프롬프트·파싱)은 `crates/core/src/sprite.rs`, IO·배선은 `src-tauri`.

**Tech Stack:** Rust(Tauri v2, png/sha2/serde — 기존 의존성만), Svelte 5 + TypeScript(Vitest).

**스펙:** [`docs/design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md`](../specs/2026-07-28-sprite-face-daily-cut-design.md)

## Global Constraints

- 빌드·테스트는 **네이티브 Windows PowerShell**에서, `a-mate/` 디렉토리 기준 (WSL 금지).
- **새 크레이트 의존성 금지** — png, sha2, serde, serde_json, base64, tempfile(dev) 기존 것만.
- 커밋은 영어 Conventional Commits, scope `agent` (문서는 `docs`). **G6 커밋(Task 1–3)과 H2 커밋(Task 4–11)을 섞지 않는다.**
- 프론트는 Svelte 5 runes(`$state`/`$effect`/`$derived`/`$props`) — v4 문법 금지.
- 외부 전송은 Engine 선택으로만. 이미지 엔진에 싣는 텍스트는 **추상 장면 묘사뿐** (ADR 0024, Task 11).
- 프론트 테스트는 순수 `.ts` 모듈만 (jsdom/컴포넌트 테스트 인프라 없음 — 스펙의 컴포넌트 분기 검증은 빌드+실환경 체크리스트로 대체).
- 이미지 실생성(과금)·실화면 확인은 구현 범위 밖 — PR 체크리스트로 사용자에게 이관.

**작업 디렉토리:** 워크트리 `D:\Project\space-a\.claude\worktrees\feat+sprite-face-daily-cut`, 브랜치 `feat/sprite-face-daily-cut`. 아래 모든 경로는 워크트리 루트 기준, 모든 명령은 `a-mate/`에서 실행.

---

### Task 1: G6 — `crop_face` 크롭 함수 (core)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (함수는 `make_background_transparent` 뒤, 테스트는 기존 `mod tests` 안)

**Interfaces:**
- Produces: `pub fn crop_face(png_bytes: &[u8]) -> Result<Vec<u8>>` — 128×128 RGBA PNG 바이트 (Task 2가 사용)

- [ ] **Step 1: 실패하는 테스트 작성**

`sprite.rs`의 기존 `#[cfg(test)] mod tests` 안에 추가:

```rust
    /// 테스트용 RGBA PNG 인코더 (crop_face 검증 전용).
    fn encode_rgba_png(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&px(x, y));
            }
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        out
    }

    fn decode_rgba_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        buf.truncate((info.width * info.height * 4) as usize);
        (info.width, info.height, buf)
    }

    #[test]
    fn crop_face_extracts_upper_center_window_at_128() {
        // 360×360: 윈도우 side = round(360/1.8) = 200, x0 = (360-200)/2 = 80,
        // y0 = round((360-200)×0.14) = 22 — CSS(background-size:180%, position 50% 14%) 환산.
        // 윈도우 안 = 초록, 밖 = 빨강. 크롭 결과는 전부 초록이어야 한다.
        let png = encode_rgba_png(360, 360, |x, y| {
            if (80..280).contains(&x) && (22..222).contains(&y) {
                [10, 200, 30, 255]
            } else {
                [200, 10, 30, 255]
            }
        });
        let out = crop_face(&png).unwrap();
        let (w, h, rgba) = decode_rgba_png(&out);
        assert_eq!((w, h), (128, 128));
        for (x, y) in [(0u32, 0u32), (64, 64), (127, 127)] {
            let i = ((y * 128 + x) * 4) as usize;
            assert_eq!(&rgba[i..i + 3], &[10, 200, 30], "픽셀 ({x},{y})는 크롭 윈도우 안이어야 함");
        }
    }

    #[test]
    fn crop_face_handles_tiny_image_by_clamping() {
        // 4×4 초소형: side = round(4/1.8) = 2 → 클램프·업스케일 경로도 에러 없이 128×128.
        let png = encode_rgba_png(4, 4, |_, _| [1, 2, 3, 255]);
        let (w, h, _) = decode_rgba_png(&crop_face(&png).unwrap());
        assert_eq!((w, h), (128, 128));
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test -p agent_mentor crop_face
```
Expected: FAIL — `cannot find function crop_face` 컴파일 에러.

- [ ] **Step 3: 최소 구현**

`make_background_transparent` 함수 바로 뒤에 추가:

```rust
/// G6 — 스프라이트에서 얼굴(머리) 영역을 잘라 128×128 아이콘 PNG로 만든다.
/// 크롭 기준은 시각 검증된 CSS(background-size:180%, position 50% 14%)의 환산:
/// 윈도우 한 변 = min(W,H)/1.8, 가로 중앙, 세로 top = 0.14 × (H − 윈도우).
/// 다운스케일은 nearest-neighbor — 치비 픽셀아트의 또렷한 픽셀 경계 보존.
pub fn crop_face(png_bytes: &[u8]) -> Result<Vec<u8>> {
    const FACE_ICON_SIZE: usize = 128;
    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 {
        return Err(anyhow!("빈 이미지"));
    }
    let ch = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(anyhow!("지원하지 않는 PNG 색 형식: {other:?}")),
    };
    let side = ((w.min(h) as f32 / 1.8).round() as usize).clamp(1, w.min(h));
    let x0 = (w - side) / 2;
    let y0 = (((h - side) as f32) * 0.14).round() as usize;
    let mut out_rgba = vec![0u8; FACE_ICON_SIZE * FACE_ICON_SIZE * 4];
    for oy in 0..FACE_ICON_SIZE {
        let sy = y0 + oy * side / FACE_ICON_SIZE;
        for ox in 0..FACE_ICON_SIZE {
            let sx = x0 + ox * side / FACE_ICON_SIZE;
            let si = sy * w + sx;
            let oi = (oy * FACE_ICON_SIZE + ox) * 4;
            out_rgba[oi] = buf[si * ch];
            out_rgba[oi + 1] = buf[si * ch + 1];
            out_rgba[oi + 2] = buf[si * ch + 2];
            out_rgba[oi + 3] = if ch == 4 { buf[si * ch + 3] } else { 255 };
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, FACE_ICON_SIZE as u32, FACE_ICON_SIZE as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&out_rgba)?;
    }
    Ok(out)
}
```

- [ ] **Step 4: 통과 확인**

```powershell
cargo test -p agent_mentor crop_face
```
Expected: PASS (2 tests).

- [ ] **Step 5: 커밋**

```powershell
git add crates/core/src/sprite.rs
git commit -m "feat(agent): add face crop helper for sprite face icon"
```

---

### Task 2: G6 — `get_face_icon` 커맨드 (lazy 재료화)

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`get_sprite` 함수 근처 + 기존 tests 모듈)
- Modify: `a-mate/src-tauri/src/lib.rs` (`commands::get_sprite,` 등록 줄 바로 아래)

**Interfaces:**
- Consumes: `agent_mentor::sprite::crop_face(&[u8]) -> Result<Vec<u8>>` (Task 1)
- Produces: `pub fn face_icon_inner(dir: &std::path::Path) -> anyhow::Result<Option<String>>`, Tauri 커맨드 `get_face_icon` → `Option<String>`(base64) — Task 3의 `getFaceIcon()`이 호출

- [ ] **Step 1: 실패하는 테스트 작성**

`commands.rs`의 기존 `#[cfg(test)] mod tests` 안에 추가 (1×1 PNG 리터럴로 png 크레이트 의존 없이 검증):

```rust
    #[test]
    fn face_icon_inner_lazy_materializes_and_caches() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().unwrap();
        // ① sprite.png 없음 → None (프론트 이모지 폴백)
        assert_eq!(face_icon_inner(dir.path()).unwrap(), None);
        // ② sprite.png 생성(1×1 투명 PNG) → face.png 재료화 + base64 반환
        let sprite = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap();
        std::fs::write(dir.path().join("sprite.png"), &sprite).unwrap();
        let b64 = face_icon_inner(dir.path()).unwrap().unwrap();
        assert!(dir.path().join("face.png").exists());
        // ③ 재호출 = 캐시 그대로 (내용 동일)
        assert_eq!(face_icon_inner(dir.path()).unwrap().unwrap(), b64);
        // ④ face.png를 sprite보다 과거로 백데이트(=리롤로 sprite가 더 새것) → 재크롭 경로
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(dir.path().join("face.png"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(face_icon_inner(dir.path()).unwrap().unwrap(), b64); // 같은 sprite → 같은 결과
        // 재크롭됐다면 mtime이 현재로 갱신됨
        let refreshed = std::fs::metadata(dir.path().join("face.png")).unwrap().modified().unwrap();
        assert!(refreshed > old, "stale face.png는 재크롭으로 갱신돼야 함");
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test -p agent-mentor-app face_icon_inner
```
Expected: FAIL — `cannot find function face_icon_inner` 컴파일 에러.
(패키지명이 다르면 `cargo test face_icon_inner`로 워크스페이스 전체 실행 — `src-tauri/Cargo.toml`의 `name` 확인.)

- [ ] **Step 3: 최소 구현**

`commands.rs`의 `get_sprite` 함수 위에 추가:

```rust
/// G6 — 얼굴 아이콘 lazy 재료화: face.png가 없거나 sprite.png보다 오래되면 그 자리에서
/// 크롭·저장한다. sprite 쓰기 경로(파이프라인 생성·리롤 승격)에 훅을 걸지 않는 이유:
/// mtime 비교가 모든 갱신 경로를 자동 커버한다 (스펙 2026-07-28 결정 7).
pub fn face_icon_inner(dir: &std::path::Path) -> anyhow::Result<Option<String>> {
    use base64::Engine as _;
    let sprite = dir.join("sprite.png");
    let face = dir.join("face.png");
    let Ok(sprite_meta) = std::fs::metadata(&sprite) else { return Ok(None) };
    let fresh = match std::fs::metadata(&face) {
        Ok(m) => match (m.modified(), sprite_meta.modified()) {
            (Ok(f), Ok(s)) => f >= s,
            _ => false, // mtime을 못 읽으면 보수적으로 재크롭
        },
        Err(_) => false,
    };
    let bytes = if fresh {
        std::fs::read(&face)?
    } else {
        let png = agent_mentor::sprite::crop_face(&std::fs::read(&sprite)?)?;
        std::fs::write(&face, &png)?;
        png
    };
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// G6 — 방명록 아바타 등 소형 UI용 얼굴 아이콘(128×128 캐시). 없으면 None(이모지 폴백).
#[tauri::command]
pub fn get_face_icon(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri::Manager as _;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    face_icon_inner(&dir).map_err(|e| e.to_string())
}
```

`lib.rs`의 커맨드 등록 매크로에서 `commands::get_sprite,` 바로 아래에 추가:

```rust
                commands::get_face_icon,
```

- [ ] **Step 4: 통과 확인**

```powershell
cargo test face_icon_inner
```
Expected: PASS (1 test). 이어서 전체 회귀:

```powershell
cargo test
```
Expected: 전체 PASS.

- [ ] **Step 5: 커밋**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(agent): add get_face_icon command with lazy face materialization"
```

---

### Task 3: G6 — 프론트 배선 (GuestbookTab 아바타 교체)

**Files:**
- Modify: `a-mate/src/lib/api.ts` (`getSprite` 함수 아래)
- Modify: `a-mate/src/lib/ui/GuestbookTab.svelte`

**Interfaces:**
- Consumes: Tauri 커맨드 `get_face_icon` (Task 2)
- Produces: `export async function getFaceIcon(): Promise<string | null>` (api.ts)

- [ ] **Step 1: api.ts에 `getFaceIcon` 추가** (`getSprite` 함수 바로 아래)

```ts
/** G6 — 얼굴 아이콘(128×128 캐시) base64 — sprite 없으면 null (이모지 폴백). */
export async function getFaceIcon(): Promise<string | null> {
  try { return await invoke<string | null>('get_face_icon'); } catch { return null; }
}
```

- [ ] **Step 2: GuestbookTab.svelte 교체**

2-a. import 줄(1행)에서 `getSprite` → `getFaceIcon`:

```ts
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, getFaceIcon, type GuestbookEntry } from '../api';
```

2-b. 상태·이펙트(9–10행) — `ownSprite` → `ownFace`. `sprite:ready` 리슨 유지(리롤 시 lazy 재크롭이 새 얼굴 반환):

```ts
  let ownFace=$state<string|null>(null);
  $effect(()=>{let un:(()=>void)|null=null;getFaceIcon().then(s=>ownFace=s);listen('sprite:ready',()=>getFaceIcon().then(s=>ownFace=s)).then(u=>un=u);return()=>un?.()});
```

2-c. 마크업 23행·34행의 두 곳 모두 `ownSprite` → `ownFace` (구조는 유지):

```svelte
{#if showOwnerAvatar(t.entry,meId,isOwner)}{#if ownFace}<span class="ava" style="background-image:url('data:image/png;base64,{ownFace}')"></span>{:else}<span class="ava ava-fb">🤖</span>{/if}{/if}
```
(34행은 `t.entry` 대신 `reply` — 기존 형태 그대로 변수명만.)

2-d. 49행 `<style>`의 `.ava` 규칙에서 CSS 줌 크롭 제거 — `background-size:180%;background-position:50% 14%` 부분을 다음으로 교체:

```css
background-size:cover;background-position:center
```

- [ ] **Step 3: 테스트·빌드 확인**

```powershell
npm test; npm run build
```
Expected: Vitest 전체 PASS(기존 173개), 빌드 성공. (`ownSprite` 잔여 참조가 있으면 빌드가 잡는다.)

- [ ] **Step 4: 커밋**

```powershell
git add src/lib/api.ts src/lib/ui/GuestbookTab.svelte
git commit -m "feat(agent): use cached face icon for guestbook avatars"
```

---

### Task 4: H2 — 샷 축·컷 상태·리컨실리에이션 판단 (core, 순수)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (파일 끝 `mod tests` 앞, H2 섹션 주석으로 구분)

**Interfaces:**
- Produces (Task 5·7·8이 사용):
  - `pub enum CutShot { Full, Bust, CloseUp, Scene }` + `as_str() -> &'static str`("full"/"bust"/"closeup"/"scene") + `has_mascot() -> bool`
  - `pub fn pick_cut_shot(date: &str, seed: &str) -> CutShot`
  - `pub struct CutState { cut_date: Option<String>, caption: String, shot: String, attempt_date: String, attempts: u8 }` (serde Serialize/Deserialize, Default)
  - `pub const MAX_CUT_ATTEMPTS_PER_DAY: u8 = 3`
  - `pub fn decide_cut(latest_diary_date: &str, s: &CutState) -> bool`
  - `pub fn register_attempt(s: &mut CutState, date: &str)` / `pub fn register_success(s: &mut CutState, date: &str, caption: &str, shot: CutShot)`

- [ ] **Step 1: 실패하는 테스트 작성** (`mod tests` 안에 추가)

```rust
    #[test]
    fn pick_cut_shot_is_deterministic_and_covers_all_variants() {
        assert_eq!(pick_cut_shot("2026-07-28", "uuid-1"), pick_cut_shot("2026-07-28", "uuid-1"));
        let mut seen = std::collections::HashSet::new();
        for d in 1..=60 {
            seen.insert(pick_cut_shot(&format!("2026-07-{d:02}"), "uuid-1").as_str());
        }
        // 60일 표본이면 4변형(마스코트 3 : 정경 1)이 모두 등장해야 한다
        assert_eq!(seen.len(), 4, "샷 축 4변형이 모두 나와야 함: {seen:?}");
    }

    #[test]
    fn cut_shot_scene_has_no_mascot() {
        assert!(CutShot::Full.has_mascot());
        assert!(CutShot::Bust.has_mascot());
        assert!(CutShot::CloseUp.has_mascot());
        assert!(!CutShot::Scene.has_mascot());
    }

    #[test]
    fn decide_cut_reconciliation_table() {
        let d = "2026-07-28";
        // 초기 상태(첫 실행) → 생성
        assert!(decide_cut(d, &CutState::default()));
        // 최신 일기 컷 완료 → skip (재실행 멱등)
        let mut done = CutState::default();
        register_attempt(&mut done, d);
        register_success(&mut done, d, "캡션", CutShot::Bust);
        assert!(!decide_cut(d, &done));
        // 오늘 3회 실패 소진 → skip (과금 상한)
        let mut spent = CutState::default();
        for _ in 0..MAX_CUT_ATTEMPTS_PER_DAY {
            register_attempt(&mut spent, d);
        }
        assert!(!decide_cut(d, &spent));
        // 어제 소진했어도 새 일기 날짜 → 생성 (카운터는 register_attempt가 리셋)
        assert!(decide_cut("2026-07-29", &spent));
        // 어제 성공 컷이 있고 오늘 일기가 새로 생김 → 생성
        assert!(decide_cut("2026-07-29", &done));
    }

    #[test]
    fn register_attempt_resets_counter_on_new_date() {
        let mut s = CutState::default();
        register_attempt(&mut s, "2026-07-28");
        register_attempt(&mut s, "2026-07-28");
        assert_eq!((s.attempt_date.as_str(), s.attempts), ("2026-07-28", 2));
        register_attempt(&mut s, "2026-07-29");
        assert_eq!((s.attempt_date.as_str(), s.attempts), ("2026-07-29", 1));
    }

    #[test]
    fn register_success_updates_display_state_and_roundtrips_json() {
        let mut s = CutState::default();
        register_attempt(&mut s, "2026-07-28");
        register_success(&mut s, "2026-07-28", "밤샘 끝, 뿌듯", CutShot::Scene);
        assert_eq!(s.cut_date.as_deref(), Some("2026-07-28"));
        assert_eq!(s.caption, "밤샘 끝, 뿌듯");
        assert_eq!(s.shot, "scene");
        // 손상 대비 직렬화 왕복 (daily_cut.json 포맷)
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<CutState>(&json).unwrap(), s);
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test -p agent_mentor cut_
```
Expected: FAIL — `CutShot`/`CutState` 미정의 컴파일 에러.

- [ ] **Step 3: 최소 구현** (`probe_endpoint` 뒤, `mod tests` 앞에 H2 섹션으로 추가)

```rust
// ── H2 매일 마스코트 컷 (2026-07-28) ─────────────────────────────────────────
// 스펙: docs/design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md

/// H2 — 컷의 샷 축. Full/Bust/CloseUp은 마스코트 등장, Scene은 캐릭터 없는 정경
/// (균등 4변형 → 마스코트:정경 = 3:1 가중).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutShot {
    Full,
    Bust,
    CloseUp,
    Scene,
}

impl CutShot {
    pub fn as_str(&self) -> &'static str {
        match self {
            CutShot::Full => "full",
            CutShot::Bust => "bust",
            CutShot::CloseUp => "closeup",
            CutShot::Scene => "scene",
        }
    }
    pub fn has_mascot(&self) -> bool {
        !matches!(self, CutShot::Scene)
    }
}

/// 날짜+정체성 시드 → 결정론적 샷 pick (재현·테스트 가능 — character_description 변주 시드 선례).
pub fn pick_cut_shot(date: &str, seed: &str) -> CutShot {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(format!("{date}|{seed}").as_bytes());
    match d[0] % 4 {
        0 => CutShot::Full,
        1 => CutShot::Bust,
        2 => CutShot::CloseUp,
        _ => CutShot::Scene,
    }
}

/// H2 — 컷 상태(app_data/daily_cut.json). `cut_date`는 화면의 daily_cut.png가 어느 일기의
/// 컷인지(재실행 멱등 키), `attempt_date`/`attempts`는 일일 과금 상한의 원장이다.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CutState {
    pub cut_date: Option<String>,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub shot: String,
    #[serde(default)]
    pub attempt_date: String,
    #[serde(default)]
    pub attempts: u8,
}

/// 하루 최대 생성 시도 — 이미지 건당 과금 가드 (스펙 결정 2).
pub const MAX_CUT_ATTEMPTS_PER_DAY: u8 = 3;

/// 리컨실리에이션 판단 — true면 생성 시도. 최신 일기 컷이 이미 있거나(멱등)
/// 그 날짜의 시도가 소진됐으면 skip.
pub fn decide_cut(latest_diary_date: &str, s: &CutState) -> bool {
    if s.cut_date.as_deref() == Some(latest_diary_date) {
        return false;
    }
    if s.attempt_date == latest_diary_date && s.attempts >= MAX_CUT_ATTEMPTS_PER_DAY {
        return false;
    }
    true
}

/// 시도 기록 — 대상 날짜가 바뀌면 카운터 리셋. 호출자는 네트워크 **전에** persist해
/// 실패·크래시에도 상한을 보장한다.
pub fn register_attempt(s: &mut CutState, date: &str) {
    if s.attempt_date != date {
        s.attempt_date = date.to_string();
        s.attempts = 0;
    }
    s.attempts = s.attempts.saturating_add(1);
}

/// 성공 기록 — 화면 상태(cut_date·caption·shot)를 새 컷으로 갱신.
pub fn register_success(s: &mut CutState, date: &str, caption: &str, shot: CutShot) {
    s.cut_date = Some(date.to_string());
    s.caption = caption.to_string();
    s.shot = shot.as_str().to_string();
}
```

- [ ] **Step 4: 통과 확인**

```powershell
cargo test -p agent_mentor
```
Expected: 전체 PASS (신규 5 + 기존).

- [ ] **Step 5: 커밋**

```powershell
git add crates/core/src/sprite.rs
git commit -m "feat(agent): add daily cut shot axis and reconciliation state"
```

---

### Task 5: H2 — 장면+캡션 프롬프트·파서·compute (core)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (Task 4의 H2 섹션에 이어서)

**Interfaces:**
- Consumes: `CutShot` (Task 4), `crate::mascot::mbti_voice_hint_style_only(Option<&str>) -> String`(기존), `crate::diary::engine::Engine` trait(기존 — `generate(&self, system, user) -> Result<…{ text }>`)
- Produces (Task 7이 사용):
  - `pub fn build_cut_scene_prompt(diary: &str, shot: CutShot, mbti: Option<&str>) -> String`
  - `pub fn parse_cut_scene(raw: &str) -> Option<(String, String)>` — `(scene_en, caption_ko)`
  - `pub fn compute_cut_scene(engine: &dyn crate::diary::engine::Engine, diary: &str, shot: CutShot, mbti: Option<&str>) -> anyhow::Result<Option<(String, String)>>`

- [ ] **Step 1: 실패하는 테스트 작성** (`mod tests` 안)

```rust
    #[test]
    fn cut_scene_prompt_embeds_diary_framing_and_privacy_rules() {
        let p = build_cut_scene_prompt("오늘은 리팩토링을 했다", CutShot::Full, Some("INTJ"));
        assert!(p.contains("오늘은 리팩토링을 했다"), "일기 본문 포함");
        assert!(p.contains("full-body"), "샷 프레이밍 포함");
        assert!(p.contains("고유명사"), "전송 수위 금지 지시(ADR 0024) 포함");
        assert!(p.contains("scene_en") && p.contains("caption_ko"), "JSON 출력 계약 포함");
        // 정경 샷은 캐릭터 없는 프레이밍
        let scene = build_cut_scene_prompt("일기", CutShot::Scene, None);
        assert!(scene.contains("WITHOUT any character"));
    }

    #[test]
    fn parse_cut_scene_accepts_clean_and_fenced_json_rejects_garbage() {
        let ok = r#"{"scene_en": "a robot at a desk", "caption_ko": "오늘도 무사히"}"#;
        assert_eq!(
            parse_cut_scene(ok),
            Some(("a robot at a desk".into(), "오늘도 무사히".into()))
        );
        let fenced = "```json\n{\"scene_en\": \"night sky\", \"caption_ko\": \"별 헤는 밤\"}\n```";
        assert_eq!(parse_cut_scene(fenced), Some(("night sky".into(), "별 헤는 밤".into())));
        assert_eq!(parse_cut_scene("그림 그려드릴게요!"), None);
        assert_eq!(parse_cut_scene(r#"{"scene_en": "", "caption_ko": "x"}"#), None);
        assert_eq!(parse_cut_scene(r#"{"scene_en": "x"}"#), None);
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test -p agent_mentor cut_scene
```
Expected: FAIL — 함수 미정의 컴파일 에러.

- [ ] **Step 3: 최소 구현** (H2 섹션에 이어서)

```rust
/// H2 — 일기 → {scene_en, caption_ko} 텍스트 엔진 시스템 프롬프트.
/// 전송 수위(ADR 0024): scene_en은 이미지 엔진으로 넘어가므로 **추상 장면만** —
/// 고유명사·프로젝트/회사명·코드 식별자·수치 금지를 여기서 지시한다.
pub fn build_cut_scene_prompt(diary: &str, shot: CutShot, mbti: Option<&str>) -> String {
    let framing = match shot {
        CutShot::Full => "a full-body shot of the robot mascot doing today's activity",
        CutShot::Bust => "a waist-up (bust) shot of the robot mascot mid-activity",
        CutShot::CloseUp => "a close-up of the robot mascot's face showing today's mood",
        CutShot::Scene => {
            "a cozy scene WITHOUT any character — desk, objects and lighting that hint at today's activity"
        }
    };
    let mood = crate::mascot::mbti_voice_hint_style_only(mbti);
    format!(
        "너는 픽셀아트 일러스트 연출가다. 아래 '오늘의 일기'를 읽고, 오늘 하루를 은유하는 \
         그림 한 컷을 기획하라.\n\
         구도: {framing}.\n{mood}\n\
         규칙:\n\
         - scene_en: 영어 1~2문장 장면 묘사. 반드시 **추상적으로** — 고유명사(사람·회사·프로젝트 \
           이름), 코드 식별자, 파일명, 숫자 수치를 절대 쓰지 마라. 분위기·행동·소품·조명 위주로.\n\
         - caption_ko: 그림 아래 붙일 한국어 감성 한 줄(40자 이내, 싸이월드 미니홈피 갬성, \
           마스코트 1인칭, 따옴표 없이).\n\
         - 그림 속에 글자는 넣을 수 없다 — 텍스트가 필요한 장면을 만들지 마라.\n\
         출력은 JSON 하나만: {{\"scene_en\": \"...\", \"caption_ko\": \"...\"}}\n\n\
         오늘의 일기:\n{diary}"
    )
}

/// 응답 JSON 관용 파싱 — 코드펜스·잡담을 걷어내고 첫 '{'…마지막 '}'만 취한다
/// (extract_image_bytes의 관용 파서와 같은 철학). 빈 필드는 실패로 본다.
pub fn parse_cut_scene(raw: &str) -> Option<(String, String)> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(&raw[start..=end]).ok()?;
    let scene = v.get("scene_en")?.as_str()?.trim().to_string();
    let caption = v.get("caption_ko")?.as_str()?.trim().to_string();
    (!scene.is_empty() && !caption.is_empty()).then_some((scene, caption))
}

/// H2 — 장면+캡션 계산 (store 접근 없음, 네트워크만 — mascot::compute_daily_line 선례).
/// Ok(None) = 응답 파싱 실패 (호출자는 시도 카운트만 남기고 skip).
pub fn compute_cut_scene(
    engine: &dyn crate::diary::engine::Engine,
    diary: &str,
    shot: CutShot,
    mbti: Option<&str>,
) -> Result<Option<(String, String)>> {
    let system = build_cut_scene_prompt(diary, shot, mbti);
    let raw = engine.generate(&system, "")?.text;
    Ok(parse_cut_scene(&raw))
}
```

주의: `engine.generate(...)`의 반환 필드가 `.text`인 것은 `mascot::compute_daily_line`(mascot.rs:409)과 동일 계약. 컴파일 에러가 나면 그 함수의 실제 사용을 따른다.

- [ ] **Step 4: 통과 확인**

```powershell
cargo test -p agent_mentor
```
Expected: 전체 PASS.

- [ ] **Step 5: 커밋**

```powershell
git add crates/core/src/sprite.rs
git commit -m "feat(agent): build daily cut scene prompt and response parser"
```

---

### Task 6: H2 — 이미지 프롬프트 조립 + `generate_cut` (core, 배관 리팩토링)

**Files:**
- Modify: `a-mate/crates/core/src/sprite.rs` (`generate` 함수 리팩토링 + H2 섹션 추가)

**Interfaces:**
- Consumes: `CutShot`(Task 4), 기존 `SpriteConfig`/`STYLE_REF_JPG`/`with_bearer`/`extract_image_bytes`
- Produces (Task 7이 사용):
  - `pub fn build_cut_image_prompt(shot: CutShot, character_desc: Option<&str>, scene_en: &str) -> String`
  - `pub fn generate_cut(cfg: &SpriteConfig, image_prompt: &str) -> Result<Vec<u8>>` — 배경 투명화 **없음**

- [ ] **Step 1: 실패하는 테스트 작성** (`mod tests` 안)

```rust
    #[test]
    fn cut_image_prompt_composes_style_scene_and_notext() {
        let p = build_cut_image_prompt(CutShot::Bust, Some("a navy robot"), "coding at night");
        assert!(p.contains("coding at night"), "장면 포함");
        assert!(p.contains("a navy robot"), "마스코트 샷은 캐릭터 묘사 포함");
        assert!(p.contains("waist-up"), "샷별 구도 포함");
        assert!(p.contains("No text"), "그림 안 텍스트 금지");
        assert!(p.contains("same art style"), "스타일 앵커 문구 포함");
        // 정경 샷: 캐릭터 묘사 없음 + 캐릭터 배제 구도
        let s = build_cut_image_prompt(CutShot::Scene, None, "a quiet desk");
        assert!(!s.contains("The character is"));
        assert!(s.contains("NO characters"));
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test -p agent_mentor cut_image_prompt
```
Expected: FAIL — 함수 미정의 컴파일 에러.

- [ ] **Step 3: 구현 — 공용 배관 추출 + 신규 함수**

3-a. `generate` 함수에서 HTTP 요청·이미지 추출 부분을 사설 함수로 추출 (동작 불변):

```rust
/// 스타일 앵커(레퍼런스 이미지)를 첨부해 이미지 1장을 요청한다 — generate/generate_cut 공용 배관.
fn request_image(cfg: &SpriteConfig, prompt: &str) -> Result<Vec<u8>> {
    let ref_b64 = base64::engine::general_purpose::STANDARD.encode(STYLE_REF_JPG);
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{ref_b64}")}}
            ]
        }],
        "modalities": ["image", "text"],
    });
    let req = ureq::post(&format!("{}/chat/completions", cfg.base_url))
        .timeout(std::time::Duration::from_secs(120))
        .set("Content-Type", "application/json");
    let resp: serde_json::Value = with_bearer(req, &cfg.api_key)
        .send_json(body)
        .map_err(|e| anyhow!("sprite 생성 요청 실패: {e}"))?
        .into_json()?;
    extract_image_bytes(&resp)
}
```

3-b. 기존 `generate`를 배관 재사용으로 축약 (프롬프트 문구·투명화 폴백은 **글자 그대로 유지**):

```rust
/// 이미지 생성 — 스타일 앵커 + 인물 묘사. 반환 = PNG 바이트.
pub fn generate(cfg: &SpriteConfig, description: &str) -> Result<Vec<u8>> {
    let prompt = format!(
        "Using the EXACT same art style as the attached reference image (16-bit pixel art sprite, \
         chibi proportions with large head, clean dark pixel outline, soft cel shading, \
         front-facing full body, centered, plain white background), draw a DIFFERENT character: \
         {description}. Match the reference's pixel density, outline thickness, shading style and \
         proportions exactly. Single character only, no text, no watermark, plain white background."
    );
    let png = request_image(cfg, &prompt)?;
    // 흰 배경 → 투명. 실패해도 캐릭터는 보여야 하므로 원본으로 폴백(무해).
    match make_background_transparent(&png) {
        Ok(t) => Ok(t),
        Err(e) => {
            eprintln!("[sprite] 배경 투명화 실패(원본 사용): {e}");
            Ok(png)
        }
    }
}
```

3-c. H2 섹션에 추가:

```rust
/// H2 — 컷 이미지 프롬프트 조립 (로컬, 네트워크 없음). 캐릭터 묘사는 마스코트 샷에만 붙는다
/// (Scene 컷은 캐릭터 없는 정경 — 스펙 데이터 흐름 ⑤).
pub fn build_cut_image_prompt(shot: CutShot, character_desc: Option<&str>, scene_en: &str) -> String {
    let composition = match shot {
        CutShot::Full => "full-body composition, character centered",
        CutShot::Bust => "waist-up bust composition",
        CutShot::CloseUp => "face close-up composition",
        CutShot::Scene => "environment-only composition, NO characters at all",
    };
    let character = character_desc
        .map(|d| format!(" The character is {d}."))
        .unwrap_or_default();
    format!(
        "Using the EXACT same art style as the attached reference image (16-bit pixel art, \
         chibi proportions, clean dark pixel outline, soft cel shading), draw one scene: \
         {scene_en}.{character} {composition}. \
         No text, no letters, no words, no watermark."
    )
}

/// H2 — 장면 컷 생성. sprite와 달리 배경 투명화를 하지 않는다 — 장면 전체가 그림이다.
pub fn generate_cut(cfg: &SpriteConfig, image_prompt: &str) -> Result<Vec<u8>> {
    request_image(cfg, image_prompt)
}
```

- [ ] **Step 4: 통과 확인 (리팩토링 회귀 포함)**

```powershell
cargo test -p agent_mentor
```
Expected: 전체 PASS — 특히 기존 `description_varies_by_seed_and_reflects_mbti` 등 sprite 테스트 녹색.

- [ ] **Step 5: 커밋**

```powershell
git add crates/core/src/sprite.rs
git commit -m "feat(agent): add scene cut image generation sharing style anchor"
```

---

### Task 7: H2 — 파이프라인 리컨실리에이션 배선 + 설정 allowlist

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`maybe_generate_sprite` 뒤에 신규 함수, 호출은 `run_pipeline_once`의 `maybe_generate_sprite(app);` 다음 줄)
- Modify: `a-mate/src-tauri/src/commands.rs:448` (set_setting `ALLOWED`)

**Interfaces:**
- Consumes: Task 4–6의 core 함수 전부, 기존 `crate::resolve_engine`/`crate::resolve_sprite_cfg`/`crate::commands::sprite_identity`/`crate::commands::diary_inner`/`store.diary_dates()`(ORDER BY date ASC — last가 최신)
- Produces: `app_data/daily_cut.png`+`daily_cut.json`, 이벤트 `daily_cut:ready`, 설정 키 `daily_cut_enabled` — Task 8·9가 소비

- [ ] **Step 1: 실패하는 테스트 작성** — `pipeline.rs`의 상태 파일 헬퍼용. `pipeline.rs`에 `#[cfg(test)] mod tests`가 없으면 파일 끝에 만든다:

```rust
#[cfg(test)]
mod cut_state_tests {
    #[test]
    fn cut_state_roundtrips_and_survives_corruption() {
        let dir = tempfile::tempdir().unwrap();
        // 없으면 Default
        let s = super::pipeline::cut_state_load(dir.path());
        assert_eq!(s, agent_mentor::sprite::CutState::default());
        // 저장 → 로드 왕복
        let mut st = agent_mentor::sprite::CutState::default();
        agent_mentor::sprite::register_attempt(&mut st, "2026-07-28");
        super::pipeline::cut_state_save(dir.path(), &st);
        assert_eq!(super::pipeline::cut_state_load(dir.path()), st);
        // 손상 → Default로 재초기화 (스펙 에러 처리)
        std::fs::write(dir.path().join("daily_cut.json"), "{corrupt").unwrap();
        assert_eq!(super::pipeline::cut_state_load(dir.path()), agent_mentor::sprite::CutState::default());
    }
}
```

(주의: `pipeline.rs`의 모듈 구조에 맞춰 경로 조정 — `run_pipeline_once`가 `pub mod pipeline` 안에 있으면 위처럼 `super::pipeline::`, 파일 루트에 있으면 `super::`. 헬퍼는 `pub(crate)`로 선언해 테스트에서 접근.)

- [ ] **Step 2: 실패 확인**

```powershell
cargo test cut_state_roundtrips
```
Expected: FAIL — `cut_state_load` 미정의 컴파일 에러.

- [ ] **Step 3: 구현**

3-a. `pipeline.rs`의 `maybe_generate_sprite` 함수 뒤에 추가:

```rust
    /// daily_cut.json 로드 — 없거나 손상이면 Default (다음 판단이 안전하게 재시작).
    pub(crate) fn cut_state_load(dir: &std::path::Path) -> agent_mentor::sprite::CutState {
        std::fs::read_to_string(dir.join("daily_cut.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub(crate) fn cut_state_save(dir: &std::path::Path, s: &agent_mentor::sprite::CutState) {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_string(s) {
            let _ = std::fs::write(dir.join("daily_cut.json"), json);
        }
    }

    /// H2 — 매일 마스코트 컷. 스캔 리컨실리에이션: 최신 일기 날짜 vs daily_cut.json을 비교해
    /// 필요할 때만 생성한다 (재실행 멱등 · 일일 3회 상한 · 옵트인 daily_cut_enabled 기본 off).
    /// 네트워크는 텍스트 → 이미지 순서(텍스트 실패 시 이미지 과금 없음), 모두 store 락 밖.
    /// 스펙: docs/design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md
    fn maybe_generate_daily_cut(app: &AppHandle) {
        use agent_mentor::sprite;
        let Ok(dir) = app.path().app_data_dir() else { return };
        let state = app.state::<crate::AppState>();

        // ① 짧은 락: 게이트 + 재료 읽기 → 즉시 해제 (네트워크 전 해제 규율)
        let (engine, cfg, uuid, mbti, date, diary) = match state.store.lock() {
            Ok(store) => {
                let enabled = store
                    .get_setting("daily_cut_enabled")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                if !enabled { return; }
                let Some(engine) = crate::resolve_engine(&store) else { return };
                let Some(cfg) = crate::resolve_sprite_cfg(&store) else { return };
                let (uuid, mbti) = match crate::commands::sprite_identity(&store) {
                    Ok(v) => v,
                    Err(e) => { log::warn!("daily-cut 프로필 해석 실패: {e}"); return; }
                };
                // diary_dates()는 date ASC 정렬 — 마지막이 최신 일기
                let Some(date) = store.diary_dates().ok().and_then(|v| v.last().cloned()) else { return };
                let diary = match crate::commands::diary_inner(&store, &date) {
                    Ok(Some(d)) => d,
                    Ok(None) => return,
                    Err(e) => { log::warn!("daily-cut 일기 읽기 실패: {e}"); return; }
                };
                (engine, cfg, uuid, mbti, date, diary)
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }; // guard drops here

        // ② 리컨실리에이션 (파일 IO만) — skip이면 조용히
        let mut cut = cut_state_load(&dir);
        if !sprite::decide_cut(&date, &cut) { return; }
        // 시도는 네트워크 **전에** persist — 실패·크래시에도 일일 상한 보장
        sprite::register_attempt(&mut cut, &date);
        cut_state_save(&dir, &cut);

        // ③ 텍스트 엔진: 일기 → 추상 장면 + 캡션 (실패 시 이미지 호출 안 감)
        let shot = sprite::pick_cut_shot(&date, &uuid);
        let scene = match sprite::compute_cut_scene(&engine, &diary, shot, mbti.as_deref()) {
            Ok(Some(v)) => v,
            Ok(None) => {
                log::warn!("daily-cut 장면 JSON 파싱 실패({}/{})", cut.attempts, sprite::MAX_CUT_ATTEMPTS_PER_DAY);
                return;
            }
            Err(e) => {
                log::warn!("daily-cut 장면 생성 실패({}/{}): {e}", cut.attempts, sprite::MAX_CUT_ATTEMPTS_PER_DAY);
                return;
            }
        };
        let (scene_en, caption) = scene;

        // ④ 이미지 엔진: 화풍 앵커 + (마스코트 샷이면 현재 정체성 묘사) + 추상 장면
        let desc = shot.has_mascot().then(|| {
            let spec = agent_mentor::mascot::robot_spec_from_profile(&uuid, mbti.as_deref());
            sprite::character_description(&spec, mbti.as_deref(), &uuid)
        });
        let prompt = sprite::build_cut_image_prompt(shot, desc.as_deref(), &scene_en);
        match sprite::generate_cut(&cfg, &prompt) {
            Ok(png) => {
                let _ = std::fs::create_dir_all(&dir);
                if std::fs::write(dir.join("daily_cut.png"), png).is_ok() {
                    sprite::register_success(&mut cut, &date, &caption, shot);
                    cut_state_save(&dir, &cut);
                    log::info!("매일 컷 생성 완료({date}, {})", shot.as_str());
                    let _ = app.emit("daily_cut:ready", &date);
                }
            }
            Err(e) => log::warn!("daily-cut 이미지 생성 실패({}/{}): {e}", cut.attempts, sprite::MAX_CUT_ATTEMPTS_PER_DAY),
        }
    }
```

3-b. `run_pipeline_once`의 `maybe_generate_sprite(app);` 바로 다음 줄에 추가:

```rust
                // H2 매일 마스코트 컷 — 옵트인(기본 off)·일기 파생·일일 3회 상한 (실패는 조용히)
                maybe_generate_daily_cut(app);
```

3-c. `commands.rs:448`의 `ALLOWED`에 `"daily_cut_enabled"` 추가:

```rust
    const ALLOWED: &[&str] = &["mascot_visible", "chatter_level", "content_protected", "mascot_pos", "realtime_advice", "last_advice_key", "visit_guestbook_enabled", "daily_cut_enabled"];
```

- [ ] **Step 4: 통과 확인**

```powershell
cargo test
```
Expected: 전체 PASS (신규 `cut_state_roundtrips_and_survives_corruption` 포함).

- [ ] **Step 5: 커밋**

```powershell
git add src-tauri/src/pipeline.rs src-tauri/src/commands.rs
git commit -m "feat(agent): generate daily mascot cut from diary on scan"
```

---

### Task 8: H2 — `get_daily_cut` 조회 커맨드

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`get_face_icon` 근처 + tests)
- Modify: `a-mate/src-tauri/src/lib.rs` (`commands::get_face_icon,` 아래)

**Interfaces:**
- Consumes: `agent_mentor::sprite::CutState`(Task 4), Task 7이 쓰는 `daily_cut.png`/`daily_cut.json`
- Produces: `pub struct DailyCut { png: String, caption: String, date: String }`(Serialize), Tauri 커맨드 `get_daily_cut` → `Option<DailyCut>` — Task 9의 `getDailyCut()`이 호출

- [ ] **Step 1: 실패하는 테스트 작성** (`commands.rs` tests 모듈)

```rust
    #[test]
    fn daily_cut_inner_requires_png_and_cut_date() {
        let dir = tempfile::tempdir().unwrap();
        // 아무것도 없음 → None
        assert!(daily_cut_inner(dir.path()).unwrap().is_none());
        // png만 있고 상태 없음(cut_date 없음) → None (캡션·날짜 없는 컷은 표시하지 않음)
        std::fs::write(dir.path().join("daily_cut.png"), b"png-bytes").unwrap();
        assert!(daily_cut_inner(dir.path()).unwrap().is_none());
        // png + 성공 상태 → Some
        let mut st = agent_mentor::sprite::CutState::default();
        agent_mentor::sprite::register_attempt(&mut st, "2026-07-28");
        agent_mentor::sprite::register_success(&mut st, "2026-07-28", "오늘도 무사히", agent_mentor::sprite::CutShot::Bust);
        std::fs::write(dir.path().join("daily_cut.json"), serde_json::to_string(&st).unwrap()).unwrap();
        let cut = daily_cut_inner(dir.path()).unwrap().unwrap();
        assert_eq!(cut.date, "2026-07-28");
        assert_eq!(cut.caption, "오늘도 무사히");
        assert!(!cut.png.is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

```powershell
cargo test daily_cut_inner
```
Expected: FAIL — `daily_cut_inner` 미정의 컴파일 에러.

- [ ] **Step 3: 최소 구현** (`get_face_icon` 아래)

```rust
/// H2 — 홈 컷 조회 payload. 생성은 파이프라인만 한다 (읽기 전용 — 과금 가드).
#[derive(Debug, Clone, Serialize)]
pub struct DailyCut {
    pub png: String,
    pub caption: String,
    pub date: String,
}

pub fn daily_cut_inner(dir: &std::path::Path) -> anyhow::Result<Option<DailyCut>> {
    use base64::Engine as _;
    let Ok(bytes) = std::fs::read(dir.join("daily_cut.png")) else { return Ok(None) };
    let state: agent_mentor::sprite::CutState = std::fs::read_to_string(dir.join("daily_cut.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let Some(date) = state.cut_date else { return Ok(None) };
    Ok(Some(DailyCut {
        png: base64::engine::general_purpose::STANDARD.encode(bytes),
        caption: state.caption,
        date,
    }))
}

#[tauri::command]
pub fn get_daily_cut(app: tauri::AppHandle) -> Result<Option<DailyCut>, String> {
    use tauri::Manager as _;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    daily_cut_inner(&dir).map_err(|e| e.to_string())
}
```

`lib.rs` 등록 매크로의 `commands::get_face_icon,` 아래에 추가:

```rust
                commands::get_daily_cut,
```

- [ ] **Step 4: 통과 확인**

```powershell
cargo test
```
Expected: 전체 PASS.

- [ ] **Step 5: 커밋**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(agent): add get_daily_cut command"
```

---

### Task 9: H2 — 옵트인 토글 + 프론트 api 배선 (TDD)

**Files:**
- Create: `a-mate/src/lib/daily-cut.ts`
- Create: `a-mate/src/lib/daily-cut.test.ts`
- Modify: `a-mate/src/lib/api.ts` (`getFaceIcon` 아래)
- Modify: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte` (캐릭터 이미지 섹션)

**Interfaces:**
- Consumes: Tauri 커맨드 `get_daily_cut`(Task 8), 기존 `getSettings`/`setSetting`(api.ts), 설정 키 `daily_cut_enabled`(Task 7)
- Produces: `dailyCutEnabled(settings): boolean`, `interface DailyCut { png; caption; date }`, `getDailyCut(): Promise<DailyCut | null>` — Task 10이 사용

- [ ] **Step 1: 실패하는 테스트 작성** — `src/lib/daily-cut.test.ts` 생성:

```ts
import { describe, it, expect } from 'vitest';
import { dailyCutEnabled } from './daily-cut';

describe('dailyCutEnabled', () => {
  it('기본 off — 미설정·빈값·false는 비활성 (visit_guestbook과 달리 옵트인)', () => {
    expect(dailyCutEnabled({})).toBe(false);
    expect(dailyCutEnabled({ daily_cut_enabled: '' })).toBe(false);
    expect(dailyCutEnabled({ daily_cut_enabled: 'false' })).toBe(false);
  });
  it("'true'일 때만 활성", () => {
    expect(dailyCutEnabled({ daily_cut_enabled: 'true' })).toBe(true);
  });
});
```

- [ ] **Step 2: 실패 확인**

```powershell
npm test
```
Expected: FAIL — `./daily-cut` 모듈 없음.

- [ ] **Step 3: 구현**

3-a. `src/lib/daily-cut.ts` 생성:

```ts
/** H2 — 매일 컷 옵트인 여부. 기본 off — 매일 반복 과금 + 일기 파생 소재가
 *  이미지 엔진으로 전송되므로 명시 동의가 필요하다 (ADR 0024). */
export function dailyCutEnabled(settings: Record<string, string>): boolean {
  return settings['daily_cut_enabled'] === 'true';
}
```

3-b. `api.ts`의 `getFaceIcon` 아래에 추가:

```ts
/** H2 — 오늘의 컷 (png base64 + 하단 캡션 + 일기 날짜). 없으면 null (sprite 폴백). */
export interface DailyCut { png: string; caption: string; date: string }
export async function getDailyCut(): Promise<DailyCut | null> {
  try { return await invoke<DailyCut | null>('get_daily_cut'); } catch { return null; }
}
```

3-c. `ConnectionGroup.svelte` — import 블록(2–8행)에 `getSettings, setSetting` 추가 및 daily-cut 헬퍼 import:

```ts
  import {
    engineSettingsGet, engineSettingsSet, engineTest,
    getSettings, hubConnect, hubDisconnect, hubSettingsGet,
    imageSettingsGet, imageSettingsSet, imageTest,
    knowledgeHubSettingsGet, knowledgeHubSettingsSet, knowledgeHubShareSet, setSetting,
    type EngineSettings, type HubSettings, type ImageSettings, type KnowledgeHubSettings,
  } from '../../api';
  import { dailyCutEnabled } from '../../daily-cut';
```

3-d. 캐릭터 이미지 상태 블록(`loadImage()` 호출 아래, 52–74행 영역)에 추가:

```ts
  // --- H2 매일 컷 (옵트인 — 하루 1장 이미지 과금 + 일기 파생 추상 장면 전송: ADR 0024) ---
  let dailyCut = $state(false);
  getSettings().then((s) => { dailyCut = dailyCutEnabled(s); });
  async function toggleDailyCut(){
    dailyCut = !dailyCut;
    try { await setSetting('daily_cut_enabled', dailyCut ? 'true' : 'false'); }
    catch { dailyCut = !dailyCut; } // 저장 실패 시 원복 (PrivacyGroup 선례)
  }
```

3-e. 캐릭터 이미지 섹션 마크업의 `<StatusLine status={imgStatus}/>`(214행) 바로 아래에 추가:

```svelte
  <label class="cut-toggle"><input type="checkbox" checked={dailyCut} onchange={toggleDailyCut}/>
    <span>매일 일기 컷 생성 — 하루 1장 이미지를 생성(과금)하고, 일기에서 뽑은 <b>추상 장면 묘사</b>가 위 이미지 모델로 전송됩니다</span></label>
```

3-f. `<style>` 블록에 추가:

```css
  .cut-toggle{display:flex;gap:8px;align-items:flex-start;margin-top:10px;font-size:12px;color:var(--text-soft)}
  .cut-toggle input{margin-top:2px}
```

- [ ] **Step 4: 통과 확인**

```powershell
npm test; npm run build
```
Expected: Vitest 전체 PASS(신규 2 포함), 빌드 성공.

- [ ] **Step 5: 커밋**

```powershell
git add src/lib/daily-cut.ts src/lib/daily-cut.test.ts src/lib/api.ts src/lib/ui/settings/ConnectionGroup.svelte
git commit -m "feat(agent): add daily cut opt-in toggle and api plumbing"
```

---

### Task 10: H2 — RobotPortrait 컷 프레임 (컷 + 하단 캡션)

**Files:**
- Modify: `a-mate/src/lib/ui/RobotPortrait.svelte`

**Interfaces:**
- Consumes: `getDailyCut()`/`DailyCut`(Task 9), 이벤트 `daily_cut:ready`(Task 7)
- Produces: 홈 좌측 상단 초상 — 컷 있으면 `이미지+캡션` 프레임(미니홈피 대문사진+감성 글귀 결), 없으면 기존 sprite→canvas 폴백 체인

- [ ] **Step 1: 구현** — `RobotPortrait.svelte` 수정 (현재 파일 56줄 전체 구조 기준):

1-a. import(3행)에 추가:

```ts
  import { getDailyCut, getMascotSeed, getSprite, lifeMascotImage, robotSpecForSeed, type DailyCut } from '../api';
```

1-b. 상태(11행 아래)에 추가 + 내 화면 전용 이펙트 (기존 sprite 이펙트는 그대로 둔다 — 컷 없을 때 폴백):

```ts
  // H2 — 오늘의 컷 (내 화면 전용). 있으면 sprite/canvas 대신 컷+캡션 프레임.
  let cut = $state<DailyCut | null>(null);

  $effect(() => {
    if (seed) return; // 남의 초상엔 매일 컷 없음
    let un: (() => void) | null = null;
    getDailyCut().then((c) => (cut = c));
    listen('daily_cut:ready', () => getDailyCut().then((c) => (cut = c))).then((u) => (un = u));
    return () => un?.();
  });
```

1-c. 마크업(37–43행)을 3분기로:

```svelte
<div class="portrait">
  {#if cut}
    <figure class="cut">
      <img src={'data:image/png;base64,' + cut.png} alt="오늘의 컷" />
      <figcaption>{cut.caption}</figcaption>
    </figure>
  {:else if sprite}
    <img class="sprite" src={'data:image/png;base64,' + sprite} alt="내 캐릭터" />
  {:else}
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  {/if}
</div>
```

1-d. `<style>`에 추가 (폴라로이드 결 — 이미지 위, 감성 캡션 아래):

```css
  .cut { margin: 0; display: flex; flex-direction: column; gap: 6px; align-items: center; }
  .cut img { width: 116px; height: 116px; object-fit: cover; border-radius: var(--radius-s); image-rendering: pixelated; }
  .cut figcaption { font-size: 11px; color: var(--ink-soft); text-align: center; line-height: 1.35; max-width: 124px; word-break: keep-all; }
```

(색·크기 토큰은 파일 내 기존 변수(`--radius-m`, `--pastel-mint` 등)와 같은 팔레트의 `--radius-s`/`--ink-soft`를 쓴다 — 전역 테마에 이미 존재. 없다면 `App.svelte`의 aside에서 쓰는 토큰명으로 맞춘다.)

- [ ] **Step 2: 테스트·빌드 확인**

```powershell
npm test; npm run build
```
Expected: 전체 PASS + 빌드 성공. (canvas 폴백·타 초상(seed) 경로가 기존 그대로인지 diff로 확인 — `seed` 분기 이펙트는 손대지 않는다.)

- [ ] **Step 3: 커밋**

```powershell
git add src/lib/ui/RobotPortrait.svelte
git commit -m "feat(agent): show daily mascot cut with caption on home portrait"
```

---

### Task 11: H2 — ADR 0024 (이미지 엔진 전송 소재 수위)

**Files:**
- Create: `docs/adr/0024-image-engine-material-boundary.md`

**Interfaces:**
- Consumes: ADR 0019(전송 경계 선례), 스펙의 "ADR 0024" 섹션
- Produces: 채택 ADR — Task 12의 PR 본문이 링크

- [ ] **Step 1: ADR 작성** — `docs/adr/0024-image-engine-material-boundary.md` 생성:

```markdown
# ADR 0024: 이미지 엔진에 일기 파생 추상 장면까지 허용한다 (매일 컷)

- 상태: 채택
- 날짜: 2026-07-28
- 대상: a-mate(Agent Mentor) 이미지 생성 경로의 전송 소재 수위
- 관련: [ADR 0019](0019-owner-memory-transmission-boundary.md)(전송 경계),
  [설계 스펙](../design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md)

## 배경

지금까지 이미지 엔진(설정창 image_* / env — 사용자가 명시 설정한 Engine)이 받는 텍스트는
`character_description`이 합성한 **로봇 외형 묘사뿐**이었다 — 사용자 데이터가 아니다.

H2(매일 마스코트 컷)는 "오늘 하루를 반영한 그림"이 요구라, 일기에서 파생한 장면 묘사를
이미지 엔진에 실어야 한다. 일기 본문은 텍스트 엔진이 생성한 것이지만(세션 요약 기반),
그 파생물이 **다른 엔진(이미지)** 으로 흘러가는 것은 전송 소재의 새 범주다. 되돌리기
어려운 규범이므로 ADR로 남긴다.

## 결정

- **허용 소재**: 일기 파생 **추상 장면 묘사(scene_en)** 까지. 생성 프롬프트가 고유명사
  (사람·회사·프로젝트 이름)·코드 식별자·파일명·숫자 수치 금지를 지시한다
  (`sprite::build_cut_scene_prompt`).
- **비전송**: 트랜스크립트 원문·일기 원문은 이미지 엔진에 보내지 않는다. 일기 본문은
  이를 생성한 텍스트 엔진에만 재전송된다(장면 변환 호출).
- **옵트인**: 기능 전체가 `daily_cut_enabled`(기본 off) 뒤에 있다 — 설정 UI가 과금과
  전송 수위를 함께 고지한다.
- **캡션(caption_ko)** 은 로컬 표시 전용 — 외부로 나가지 않는다.

## 대안

- **거친 신호만**(바쁨/보통/한가함): 가장 안전하나 "대화·작업 내용 반영" 요구를 채우지
  못한다 — 장면 다양성이 로컬 축뿐이라 매일 컷의 재미가 급감. 기각.
- **완전 로컬 축만**(MBTI·계절·요일): 요구 자체를 포기하는 안. 기각.
- **일기 원문 전송**: 변환 없이 이미지 모델에 일기를 직접 주면 파이프라인은 단순하지만
  고유명사·수치가 그대로 노출되고 수위 통제가 프롬프트 한 겹뿐이다. 기각 —
  텍스트 엔진(이미 일기를 아는 경계 안)이 추상화를 먼저 수행한다.
```

- [ ] **Step 2: 커밋**

```powershell
git add ../docs/adr/0024-image-engine-material-boundary.md
git commit -m "docs(adr): record image-engine material boundary for daily cuts"
```
(경로 주의: 명령을 `a-mate/`에서 실행 중이면 `../docs/...`, 워크트리 루트면 `docs/...`.)

---

### Task 12: 마무리 — 전체 검증 · 로드맵 · 아카이브 · PR

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (G6·H2·묶음 ⑤ 완료 기록)
- docs-archive 스킬이 스펙·플랜을 `docs/archive/` 미러로 이동

- [ ] **Step 1: 전체 테스트·빌드 최종 확인** (`a-mate/`에서)

```powershell
cargo test; npm test; npm run build
```
Expected: 전부 녹색. 실패 시 원인 수정 전에는 다음 단계 진행 금지.

- [ ] **Step 2: main 최신 반영**

```powershell
git fetch origin main; git rebase origin/main
```
충돌 예상 지점: `src-tauri/src/lib.rs` 커맨드 등록부, `commands.rs` — 양쪽 변경 모두 유지하는 방향으로 해결 후 `cargo test; npm test` 재확인.

- [ ] **Step 3: 로드맵 완료 기록** — 로드맵의 G6 항목 끝에 구현 결과 불릿, H2 항목 끝에 구현 결과 불릿, 묶음 표의 ⑤ 행에 `— ✅ 완료(2026-07-28, feat/sprite-face-daily-cut)` 추가. 구현 결과에는 확정 결정(수위·멱등·3회 상한·최신 1장·옵트인·lazy face)을 한 줄씩 요약.

```powershell
git add docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md
git commit -m "docs(plan): record bundle 5 completion in roadmap"
```

- [ ] **Step 4: docs-archive DoD** — `docs-archive` 스킬 실행: 이 플랜과 스펙(`2026-07-28-sprite-face-daily-cut-design.md`)을 `docs/archive/` 미러로 이동 (ADR 0024는 `docs/adr/`에 남는다). 스킬 안내대로 커밋.

- [ ] **Step 5: 푸시 + PR 생성**

```powershell
git push -u origin feat/sprite-face-daily-cut
gh pr create --title "feat(agent): sprite face icon and daily mascot cut (bundle 5)" --body-file <본문 파일>
```

PR 본문에 반드시 포함:
- G6/H2 요약 + 스펙·ADR 0024 링크, G6/H2 커밋 분리 안내
- **사용자 실환경 체크리스트** (스펙 그대로):
  - [ ] G6: 방명록 아바타 육안 비교 — Rust 크롭이 기존 CSS 크롭과 동등한가 (1회)
  - [ ] H2: `daily_cut_enabled` on 후 실제 생성 1회 (과금 발생) — 컷·캡션 표시 확인
  - [ ] H2: 앱 재실행 시 재생성 없음 (멱등) 확인
  - [ ] H2: 토글 off 시 미생성 확인
- 말미: `🤖 Generated with [Claude Code](https://claude.com/claude-code)`

---

## Self-Review 결과 (플랜 작성 시 수행)

- **스펙 커버리지**: 확정 결정 1–9 전부 태스크에 매핑 (1→T5·T11, 2→T4·T7, 3→T7 최신1장 덮어쓰기, 4→T5, 5→T7·T9, 6→커밋 규율, 7→T2, 8→T7, 9→T4). 스펙 테스트 표의 "RobotPortrait/GuestbookTab 컴포넌트 분기"는 인프라(jsdom) 부재로 순수 헬퍼+빌드+실환경 체크리스트로 대체(Global Constraints 명기).
- **타입 일관성**: `CutState`/`CutShot`/`DailyCut`/`face_icon_inner`/`daily_cut_inner` 시그니처를 태스크 간 대조 완료. `engine.generate(...).text` 계약은 mascot.rs:409 실사용과 동일.
- **플레이스홀더**: 없음 — 모든 코드 스텝에 실제 코드 포함.
