---
status: done
archived: 2026-08-02
---

# G7 방명록 작성자 아이콘 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 방명록에서 봇 글과 사람 글이 아이콘으로 구분되고, 그 아이콘이 보는 사람과 무관하게 항상 같아진다.

**Architecture:** 봇/사람 판별은 서버가 이미 채우는 `author_kind`를 읽어서 한다. 봇은 `GET /life/agents/{id}/mascot-image`로 받은 PNG를 기존 `crop_face`로 잘라 쓰고, 사람은 `author_name`에서 파생한 이니셜 모노그램을 그린다. 아이콘 결정 함수는 관찰자 인자를 받지 않아 "누가 보든 같다"가 시그니처로 강제된다.

**Tech Stack:** Svelte 5(룬), TypeScript, Vitest / Rust, Tauri v2, ureq

**스펙:** [2026-08-02-guestbook-author-icon-design.md](../specs/2026-08-02-guestbook-author-icon-design.md)

## Global Constraints

- **a-hub 변경 금지.** 서버 스키마·API·`contracts/` 모두 손대지 않는다. 필요한 필드가 이미 전부 있다.
- **플랫폼**: Windows 전용. 빌드·테스트는 네이티브 PowerShell에서 (`a-mate/` 디렉터리), WSL 금지.
- **Svelte 5 룬 문법**: `$state`/`$derived`/`$effect`/`{#snippet}`. v1 Tauri API 금지.
- **`<style>` 블록에 hex 색 금지** — `no-hardcoded-colors.test.ts`가 모든 `.svelte`를 검사한다 (`GuestbookTab.svelte`는 allowlist에 없음). `style=` 인라인 속성은 검사 대상이 아니다.
- **커밋 메시지**: Conventional Commits, 영어, 소문자 시작, 마침표 없음.
- **테스트 명령**: `npm test`(vitest + svelte-check), `cargo test`(워크스페이스 전체). 둘 다 `a-mate/`에서 실행.
- **기존 `life_mascot_image` 커맨드는 건드리지 않는다** — 방(LifeView)이 전신 렌더에 쓴다.

---

### Task 1: 아이콘 결정 순수 함수

**Files:**
- Modify: `a-mate/src/lib/guestbook.ts`
- Test: `a-mate/src/lib/guestbook.test.ts`

**Interfaces:**
- Consumes: `GuestbookEntry` (`src/lib/api.ts:251` — `author_kind?: 'human' | 'bot' | null` 필드가 이미 있다)
- Produces:
  - `type AuthorIcon = { kind: 'bot'; agentId: string } | { kind: 'monogram'; initial: string; hue: number }`
  - `authorIcon(entry: GuestbookEntry): AuthorIcon`
  - `authorInitial(name: string): string`
  - `authorHue(name: string): number`

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src/lib/guestbook.test.ts`의 import 줄을 바꾸고(`showOwnerAvatar` → 새 함수들), `describe('showOwnerAvatar', ...)` 블록 전체(33~47행)를 아래로 교체한다.

```ts
// 파일 맨 위 import 교체
import { authorHue, authorIcon, authorInitial, autoVisitEnabled, groupGuestbook, visitGuestbookEnabled } from './guestbook';
```

```ts
describe('authorIcon', () => {
  const mk = (
    author_kind: 'human' | 'bot' | null | undefined,
    author_name = '홍길동',
  ): GuestbookEntry => ({
    entry_id: 'x', life_id: 'l1', author_agent_id: 'agent-1', author_name,
    body: 'b', author_kind, created_at: '2026-08-02T00:00:00Z',
  });

  it("author_kind='bot'이면 봇 아이콘 — agent_id로 서버 얼굴을 찾는다", () => {
    expect(authorIcon(mk('bot'))).toEqual({ kind: 'bot', agentId: 'agent-1' });
  });

  it("'human'과 구데이터(null·미제공)는 모두 모노그램 (서버 규약: 미제공=human 간주)", () => {
    expect(authorIcon(mk('human')).kind).toBe('monogram');
    expect(authorIcon(mk(null)).kind).toBe('monogram');
    expect(authorIcon(mk(undefined)).kind).toBe('monogram');
  });

  it('모노그램은 이름에서 글자와 색을 뽑는다', () => {
    expect(authorIcon(mk('human', '김영희'))).toEqual({
      kind: 'monogram', initial: '김', hue: authorHue('김영희'),
    });
  });

  it('관찰자 인자를 받지 않는다 — 같은 엔트리는 누가 보든 같은 아이콘 (스펙 목표 2)', () => {
    const entry = mk('human');
    expect(authorIcon(entry)).toEqual(authorIcon(entry));
    // 관찰자(meId·isOwner 등)를 인자로 추가하면 이 단언이 깨진다 — 회귀 고정
    expect(authorIcon.length).toBe(1);
  });
});

describe('authorInitial', () => {
  it('한글·영문 첫 글자', () => {
    expect(authorInitial('홍길동')).toBe('홍');
    expect(authorInitial('John Doe')).toBe('J');
  });

  it('이모지는 서로게이트 페어를 쪼개지 않는다', () => {
    expect(authorInitial('🤖봇')).toBe('🤖');
  });

  it('앞뒤 공백은 무시', () => {
    expect(authorInitial('  김철수 ')).toBe('김');
  });

  it('빈 이름·공백뿐이면 물음표', () => {
    expect(authorInitial('')).toBe('?');
    expect(authorInitial('   ')).toBe('?');
  });
});

describe('authorHue', () => {
  it('같은 이름은 항상 같은 hue — 모든 관찰자가 같은 색을 본다', () => {
    expect(authorHue('홍길동')).toBe(authorHue('홍길동'));
  });

  it('다른 이름은 다른 hue', () => {
    expect(authorHue('홍길동')).not.toBe(authorHue('김영희'));
  });

  it('어떤 입력에도 0~359 범위', () => {
    for (const name of ['홍길동', 'John', '', '🤖', 'a'.repeat(80)]) {
      expect(authorHue(name)).toBeGreaterThanOrEqual(0);
      expect(authorHue(name)).toBeLessThan(360);
    }
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
cd a-mate
npx vitest run src/lib/guestbook.test.ts
```

Expected: FAIL — `authorIcon`, `authorInitial`, `authorHue`가 `./guestbook`에 없어 import 에러.

- [ ] **Step 3: 구현한다**

`a-mate/src/lib/guestbook.ts`에서 `showOwnerAvatar`(21~24행)를 삭제하고 그 자리에 넣는다.

```ts
/** G7 — 방명록 작성자 아이콘 (스펙 §3.1).
 *  봇 = 서버에 게시된 마스코트 얼굴, 사람 = 이름에서 뽑은 이니셜 모노그램. */
export type AuthorIcon =
  | { kind: 'bot'; agentId: string }
  | { kind: 'monogram'; initial: string; hue: number };

/** 이름 첫 글자. 코드포인트 단위라 이모지(서로게이트 페어)가 반으로 쪼개지지 않는다.
 *  빈 이름·공백뿐이면 '?'. */
export function authorInitial(name: string): string {
  const trimmed = name.trim();
  return trimmed ? [...trimmed][0] : '?';
}

/** 이름 → 색상 hue(0~359). 결정론적이라 모든 관찰자가 같은 색을 본다. */
export function authorHue(name: string): number {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + (ch.codePointAt(0) ?? 0)) % 360;
  return hash;
}

/** 이 항목의 아이콘. **관찰자를 나타내는 인자가 없다** — 같은 엔트리는 내 방이든 남의 방이든,
 *  내가 쓴 글이든 남이 쓴 글이든 같은 아이콘으로 보인다(스펙 목표 2).
 *  봇 판별은 서버가 채우는 author_kind 하나로 한다: 미제공(구데이터)은 사람 간주. */
export function authorIcon(entry: GuestbookEntry): AuthorIcon {
  if (entry.author_kind === 'bot') {
    return { kind: 'bot', agentId: entry.author_agent_id };
  }
  return {
    kind: 'monogram',
    initial: authorInitial(entry.author_name),
    hue: authorHue(entry.author_name),
  };
}
```

- [ ] **Step 4: 테스트 통과 확인**

```powershell
npx vitest run src/lib/guestbook.test.ts
```

Expected: PASS — `authorIcon` 4개 + `authorInitial` 4개 + `authorHue` 3개 + 기존 `groupGuestbook` 3개 + 토글 2개.

이 시점에 `GuestbookTab.svelte`는 아직 `showOwnerAvatar`를 import하므로 **전체 `npm test`는 아직 실패한다** — Task 3에서 고친다.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/guestbook.ts a-mate/src/lib/guestbook.test.ts
git commit -m "feat(frontend): decide guestbook author icon from the entry alone"
```

---

### Task 2: 타인 얼굴 크롭 커맨드

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`life_mascot_face`·`crop_face_b64` 추가, `get_face_icon`·`face_icon_inner`·해당 테스트 제거)
- Modify: `a-mate/src-tauri/src/lib.rs:397` (커맨드 등록 교체)

**Interfaces:**
- Consumes: `agent_mentor::sprite::crop_face(&[u8]) -> Result<Vec<u8>>` (`crates/core/src/sprite.rs:345`), `LifeClient::mascot_image(&str) -> Result<Option<Vec<u8>>>` (`crates/core/src/life_client.rs:288` — 404는 `Ok(None)`)
- Produces:
  - `pub fn crop_face_b64(png: &[u8]) -> Option<String>`
  - Tauri 커맨드 `life_mascot_face(agent_id: String) -> Result<Option<String>, String>`

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`a-mate/src-tauri/src/commands.rs`의 `mod tests` 안에서 `face_icon_inner_lazy_materializes_and_caches` 테스트(1685~1711행) 전체를 아래로 교체한다.

```rust
    #[test]
    fn crop_face_b64_returns_128px_png_and_none_on_garbage() {
        use base64::Engine as _;
        // 1×1 투명 PNG (제거된 face_icon_inner 테스트와 같은 픽스처)
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap();
        let b64 = crop_face_b64(&png).expect("유효 PNG는 크롭된다");
        let out = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
        assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n", "PNG 시그니처");
        // IHDR width(바이트 16..20) = 128 — 내 얼굴 아이콘과 같은 크기
        assert_eq!(u32::from_be_bytes([out[16], out[17], out[18], out[19]]), 128);

        // 손상·비PNG 입력은 None — 아이콘 하나 때문에 방명록 조회를 실패시키지 않는다
        assert!(crop_face_b64(b"not a png").is_none());
        assert!(crop_face_b64(&[]).is_none());
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

```powershell
cd a-mate
cargo test crop_face_b64
```

Expected: FAIL — `cannot find function 'crop_face_b64' in this scope`.

- [ ] **Step 3: 구현한다**

**3a.** `commands.rs`에서 `face_icon_inner`(1733~1756행)와 `get_face_icon`(1758~1764행)을 삭제하고, 그 자리에 넣는다.

```rust
/// G7 — 서버에서 받은 마스코트 PNG를 방명록 아이콘용 얼굴(128×128)로 크롭해 base64로.
/// 크롭 실패(손상·미지원 색 형식)는 None — 아이콘 하나 때문에 방명록 조회를 실패시키지 않는다.
pub fn crop_face_b64(png: &[u8]) -> Option<String> {
    use base64::Engine as _;
    match agent_mentor::sprite::crop_face(png) {
        Ok(face) => Some(base64::engine::general_purpose::STANDARD.encode(face)),
        Err(e) => {
            log::warn!("방명록 얼굴 크롭 실패(이모지 폴백): {e}");
            None
        }
    }
}
```

**3b.** `life_mascot_image`(1160~1169행) 바로 아래에 커맨드를 추가한다. **기존 커맨드는 그대로 둔다** — 방(LifeView)이 전신을 쓴다.

```rust
/// G7 — 방명록 작성자 아이콘용 얼굴. 서버에 게시된 타인 마스코트를 받아 내 얼굴과 같은
/// 기준으로 크롭한다 (스펙 §3.2). 게시본이 없으면(404) None → 프론트 이모지 폴백.
#[tauri::command]
pub async fn life_mascot_face(state: State<'_, AppState>, agent_id: String) -> Result<Option<String>, String> {
    let Some(client) = hub_client(&state)? else { return Ok(None) };
    run_life_http("life_mascot_face", move || {
        client
            .mascot_image(&agent_id)
            .map(|png| png.as_deref().and_then(crop_face_b64))
            .map_err(|e| e.to_string())
    })
    .await
}
```

**3c.** `lib.rs:397`의 `commands::get_face_icon,` 줄을 삭제하고, `commands::life_mascot_image,`(460행) 다음 줄에 추가한다.

```rust
                commands::life_mascot_face,
```

- [ ] **Step 4: 테스트 통과 확인**

```powershell
cargo test
```

Expected: PASS — 전체 워크스페이스 통과. `get_face_icon` 제거로 인한 미사용 import 경고가 뜨면 함께 정리한다.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs
git commit -m "feat(agent): add life_mascot_face to crop a neighbour's mascot for the guestbook"
```

---

### Task 3: 방명록 UI 교체

**Files:**
- Modify: `a-mate/src/lib/ui/GuestbookTab.svelte`
- Modify: `a-mate/src/lib/api.ts` (`lifeMascotFace` 추가, `getFaceIcon` 제거)

**Interfaces:**
- Consumes: `authorIcon`(Task 1), 커맨드 `life_mascot_face`(Task 2)
- Produces: `lifeMascotFace(agentId: string): Promise<string | null>`

- [ ] **Step 1: api.ts를 고친다**

`getFaceIcon`(330~333행)을 삭제하고, `lifeMascotImage`(268행) 다음 줄에 추가한다.

```ts
/** G7 — 방명록 작성자 아이콘용 얼굴(128×128 크롭) base64. 게시본 없으면 null(이모지 폴백). */
export const lifeMascotFace = (agentId: string) => invoke<string | null>('life_mascot_face', { agentId });
```

- [ ] **Step 2: GuestbookTab.svelte의 스크립트를 고친다**

import 두 줄(2~3행)을 교체한다.

```svelte
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, lifeMascotFace, type GuestbookEntry } from '../api';
  import { groupGuestbook, authorIcon } from '../guestbook';
```

`ownFace` 관련 두 줄(9~11행)을 삭제하고 아래로 교체한다. `threads` 선언(12행)보다 **뒤에** 와야 한다.

```svelte
  // G7 — 봇 얼굴은 서버 게시본. agent_id별로 1회만 받아 같은 작성자의 여러 글이 요청을 나눠 쓴다.
  // 값이 없으면 이모지 폴백. 탭을 다시 열면 새로 받으므로 리롤도 그때 반영된다.
  let botFace=$state<Record<string,string|null>>({});
  $effect(()=>{for(const t of threads)for(const e of [t.entry,...t.replies]){const icon=authorIcon(e);if(icon.kind!=='bot'||icon.agentId in botFace)continue;botFace[icon.agentId]=null;lifeMascotFace(icon.agentId).then(v=>{if(v)botFace={...botFace,[icon.agentId]:v}}).catch(()=>{})}});
```

- [ ] **Step 3: 아바타 렌더를 snippet으로 뽑는다**

원글(27행)과 답글(38행)이 같은 아바타 마크업을 반복하고 있었다. snippet 하나로 합친다.
`<section>` 여는 태그 바로 앞에 넣는다.

```svelte
{#snippet avatar(entry:GuestbookEntry)}
  {@const icon=authorIcon(entry)}
  {#if icon.kind==='bot'}
    {#if botFace[icon.agentId]}<span class="ava" style="background-image:url('data:image/png;base64,{botFace[icon.agentId]}')"></span>
    {:else}<span class="ava ava-fb">🤖</span>{/if}
  {:else}
    <span class="ava ava-mono" style="background-color:hsl({icon.hue} 55% 30%)">{icon.initial}</span>
  {/if}
{/snippet}
```

원글 `<header>`(27행)에서 `{#if showOwnerAvatar(...)}…{/if}` 전체를 `{@render avatar(t.entry)}`로 바꾼다.

```svelte
    <header>{@render avatar(t.entry)}<b><NameChip agentId={t.entry.author_agent_id} name={t.entry.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b><time>{new Date(t.entry.created_at).toLocaleString()}</time>
```

답글 `<header>`(38행)도 같은 방식으로 바꾼다.

```svelte
          <header>{@render avatar(reply)}<b><NameChip agentId={reply.author_agent_id} name={reply.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b><time>{new Date(reply.created_at).toLocaleString()}</time>
```

- [ ] **Step 4: 모노그램 스타일을 추가한다**

`<style>` 블록의 `.list .ava-fb{…}` 규칙 바로 뒤에 넣는다.

```css
.list .ava-mono{background-size:auto;font-size:14px;font-weight:700;line-height:30px;text-align:center;color:white}
```

배경색만 인라인인 이유: hue가 이름에서 파생된 **동적 값**이라 테마 토큰으로 표현할 수 없다.
글자색은 `white` 키워드 — `hsl(h 55% 30%)`는 hue가 어떤 값이어도(최악은 노랑 계열) 흰 글자와
대비 4.7:1 이상이라 WCAG AA를 만족한다. hex(`#fff`)를 쓰면 `no-hardcoded-colors` 테스트가 잡는다.

- [ ] **Step 5: 전체 테스트 통과 확인**

```powershell
cd a-mate
npm test
```

Expected: PASS — vitest 전체 + svelte-check. `showOwnerAvatar`·`getFaceIcon` 잔존 참조가 있으면 svelte-check가 잡는다.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/GuestbookTab.svelte
git commit -m "feat(frontend): show the same author icon to every visitor"
```

---

### Task 4: 마스코트 이미지 게시 보장

**Files:**
- Modify: `a-mate/src-tauri/src/commands.rs` (`maybe_sync_mascot_image` 추가)
- Modify: `a-mate/src-tauri/src/pipeline.rs:241` 부근 (스캔 루프에서 호출)

**Interfaces:**
- Consumes: `upload_cached_mascot(&AppHandle, &LifeClient) -> Result<bool, String>` (`commands.rs:830`, 같은 모듈이라 private 그대로 쓸 수 있다)
- Produces: `pub fn maybe_sync_mascot_image(app: &tauri::AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>)`

- [ ] **Step 1: 커맨드 모듈에 함수를 추가한다**

`commands.rs`의 `upload_cached_mascot`(830~835행) 바로 뒤에 넣는다.

```rust
/// G7 — 스캔마다 마스코트 이미지 게시를 보장한다 (스펙 §3.5). 지금까지는 방 탭을 열 때만
/// 올라가서, 방 탭을 한 번도 안 연 사용자의 봇은 남의 방명록에 글을 남겨도 얼굴이 안 보였다.
/// 서버가 같은 sha면 쓰기를 생략하므로 반복 비용은 조회 1회다.
/// hub 미연결·스프라이트 없음이면 no-op, 실패는 warn+skip (maybe_* 계열 규율).
pub fn maybe_sync_mascot_image(
    app: &tauri::AppHandle,
    store_mutex: &std::sync::Mutex<SqliteStore>,
) {
    let client = match store_mutex.lock() {
        Ok(store) => {
            let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
            let (url, token, api_key) = (get("hub_url"), get("hub_token"), get("hub_api_key"));
            if url.trim().is_empty() || token.is_empty() {
                return; // hub 미연결
            }
            agent_mentor::life_client::LifeClient {
                base_url: url,
                token,
                api_key: {
                    let k = api_key.trim();
                    (!k.is_empty()).then(|| k.to_string())
                },
            }
        }
        Err(e) => {
            log::warn!("store lock poisoned: {e}");
            return;
        }
    };
    if let Err(e) = upload_cached_mascot(app, &client) {
        log::warn!("마스코트 이미지 게시 실패(다음 스캔 재시도): {e}");
    }
}
```

- [ ] **Step 2: 스캔 루프에서 부른다**

`pipeline.rs:241`의 `crate::visit::maybe_auto_visit(&state.store);` 바로 **앞**에 넣는다.
자율 방문이 남길 방명록보다 얼굴이 먼저 올라가야 한다.

```rust
                // G7 마스코트 얼굴 게시 보장 — 방명록 아이콘이 남에게도 보이려면 서버에 있어야 한다
                crate::commands::maybe_sync_mascot_image(app, &state.store);
```

- [ ] **Step 3: 컴파일·테스트 확인**

```powershell
cd a-mate
cargo test
```

Expected: PASS. 컴파일 에러가 나면 `app`이 그 스코프에서 `&tauri::AppHandle`인지 확인한다 — 같은 루프의 `maybe_poll_inbound(app, &state.store)`(243행)가 선례다.

- [ ] **Step 4: 커밋**

```bash
git add a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/pipeline.rs
git commit -m "fix(agent): publish the mascot face on scan, not only when the room tab opens"
```

---

### Task 5: 최종 검증과 DoD

**Files:**
- Modify: `docs/design/a-mate/plans/2026-08-02-guestbook-author-icon.md` (체크박스)
- Move: 스펙·플랜 → `docs/archive/`

- [ ] **Step 1: 전체 테스트를 돌린다**

```powershell
cd a-mate
npm test
cargo test
```

Expected: 둘 다 PASS. 베이스라인은 vitest 224개 / cargo 47개(+타 크레이트)였고, Task 1이 테스트를 순증시킨다.

- [ ] **Step 2: 잔존 참조를 확인한다**

```bash
git grep -n "showOwnerAvatar\|getFaceIcon\|get_face_icon\|face_icon_inner" -- a-mate/
```

Expected: 출력 없음. 남아 있으면 그 파일을 정리한다.

- [ ] **Step 3: 실환경 스모크 (선택 — hub 연결 시)**

```powershell
cd a-mate
npm run tauri dev
```

확인 항목:
1. 내 방 방명록 — 봇 답글에 로봇 얼굴, 사람 글에 이니셜 원
2. 남의 방으로 이동 — 내가 쓴 글에도 아이콘이 그대로 보인다(예전엔 사라졌다)
3. 얼굴 없는 봇 — 🤖 이모지

hub 미연결이면 이 단계를 건너뛰고 스펙 §3.5의 스모크를 후속으로 남긴다.

- [ ] **Step 4: DoD — 작업 문서를 아카이브한다**

`docs-archive` 스킬을 실행해 이 플랜과 스펙을 `docs/archive/` 미러로 옮긴다
(CLAUDE.md의 완료 시 아카이브 규칙, [ADR 0013](../../../../adr/0013-docs-lifecycle-and-archive.md)).

- [ ] **Step 5: 커밋하고 PR을 연다**

```bash
git add -A
git commit -m "docs(design): archive guestbook author icon spec and plan"
```

PR 본문에 담을 것: 스펙 링크, 봇/사람 구분 방식(`author_kind`), 관찰자 불변을 어떻게 보장했는지(`authorIcon` 시그니처), a-hub 무변경, 실환경 스모크 상태.

---

## Self-Review

**스펙 커버리지**

| 스펙 | 태스크 |
|------|--------|
| §3.1 엔트리만의 함수 | Task 1 (`authorIcon.length === 1` 회귀 포함) |
| §3.2 봇 = 서버 얼굴 | Task 2 (커맨드) + Task 3 (렌더·캐시) |
| §3.3 🤖 폴백 | Task 3 Step 3 (`ava-fb` 유지) |
| §3.4 이니셜 모노그램 | Task 1 (함수) + Task 3 Step 4 (스타일) |
| §3.5 업로드 보장 | Task 4 |
| §3.6 제거 | Task 1(`showOwnerAvatar`), Task 2(`get_face_icon`·`face_icon_inner`), Task 3(`getFaceIcon`) — Task 5 Step 2가 잔존 확인 |
| §6 테스트 | Task 1·2에 실제 테스트 코드 포함 |

**타입 일관성**: `AuthorIcon`의 `agentId`/`initial`/`hue`가 Task 1 정의와 Task 3 사용에서 일치한다. `lifeMascotFace`(TS) ↔ `life_mascot_face`(Rust 커맨드명) 대응 확인. `crop_face_b64`는 Task 2에서 정의하고 같은 태스크에서만 쓴다.

**남은 판단**: 스펙 §4대로 방명록 응답에 sha를 넣지 않으므로, 봇이 얼굴을 리롤해도 열려 있는 방명록 화면은 옛 얼굴을 유지한다. 탭을 다시 열면 갱신된다.
