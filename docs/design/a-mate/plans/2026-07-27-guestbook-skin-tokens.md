# G7 방명록 스킨 토큰 + 원글/답글 시인성 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> 실행 방식은 사용자가 `/jb-tools:impl-select`로 결정한다.

**Goal:** `GuestbookTab.svelte`의 고정 파스텔 토큰을 스킨 의미 토큰으로 교체하고 원글/답글 위계(인셋 답글)를 만든다.

**Architecture:** 변경은 `GuestbookTab.svelte`의 `<style>` 한 줄 블록 한정 — 마크업·스크립트·theme.css 불변. 토큰 매핑은 스펙([2026-07-27-guestbook-skin-tokens-design.md](../specs/2026-07-27-guestbook-skin-tokens-design.md))의 매핑표를 그대로 따른다.

**Tech Stack:** Svelte 5 + Vite + Vitest (스타일 전용 — 신규 테스트 없음, 기존 `no-hardcoded-colors` 테스트가 권위)

## Global Constraints

- 네이티브 Windows PowerShell에서 실행 (**WSL 금지**) — a-mate/CLAUDE.md
- `<style>`에 hex 색·전경 `color: var(--accent)` 금지 — `src/lib/no-hardcoded-colors.test.ts` (GuestbookTab은 ALLOWLIST 아님)
- 커밋은 영어 Conventional Commits, scope `agent`/`plan`/`docs`
- 워크트리: `D:\Project\space-a\.claude\worktrees\feat+guestbook-skin-tokens`, 브랜치 `feat/guestbook-skin-tokens` (npm 커맨드는 그 아래 `a-mate/`에서)
- 로드맵 파일은 병렬 V1 세션과 공유 — **작업 마지막에 origin/main 반영 후** 수정·커밋 (Task 2)

---

### Task 1: `<style>` 토큰 마이그레이션

**Files:**
- Modify: `a-mate/src/lib/ui/GuestbookTab.svelte:49` (`<style>` 줄만)

**Interfaces:**
- Consumes: `src/lib/theme.css`의 기존 의미 토큰 (`--line`, `--frame-bg`, `--ink`, `--panel2`, `--accent-tint`, `--radius-s`, `--radius-m`, `--shadow-soft`) — 신설 없음
- Produces: 없음 (말단 스타일)

- [ ] **Step 1: `<style>` 줄 교체**

현재 49행 전체:

```css
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--pastel-lav);border-radius:8px}button{border:0;border-radius:8px;padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--pastel-cream);border-radius:10px}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list .ava{display:inline-block;flex:0 0 auto;width:30px;height:30px;border-radius:50%;vertical-align:middle;margin-right:5px;background-color:var(--pastel-lav);background-repeat:no-repeat;background-size:180%;background-position:50% 14%}.list .ava-fb{background-size:auto;font-size:17px;line-height:30px;text-align:center}.list time{color:var(--ink-soft)}.list .acts{margin-left:auto;display:flex;gap:4px}.list header button{padding:3px 7px;background:var(--pastel-lav);color:var(--ink)}.list p{margin:7px 0 0}.replies{margin-top:8px;display:grid;gap:6px;border-left:2px solid var(--pastel-lav);padding-left:10px}.list .reply{padding:8px 10px;background:transparent;border:1px solid var(--pastel-lav);border-radius:8px}</style>
```

다음으로 교체 (기존 한 줄 포맷 유지):

```css
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--line);border-radius:var(--radius-s);background:var(--frame-bg);color:var(--ink)}button{border:0;border-radius:var(--radius-s);padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--frame-bg);border-radius:var(--radius-m);box-shadow:var(--shadow-soft)}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list .ava{display:inline-block;flex:0 0 auto;width:30px;height:30px;border-radius:50%;vertical-align:middle;margin-right:5px;background-color:var(--line);background-repeat:no-repeat;background-size:180%;background-position:50% 14%}.list .ava-fb{background-size:auto;font-size:17px;line-height:30px;text-align:center}.list time{color:var(--ink-soft)}.list .acts{margin-left:auto;display:flex;gap:4px}.list header button{padding:3px 7px;background:var(--accent-tint);color:var(--ink)}.list p{margin:7px 0 0}.replies{margin-top:8px;display:grid;gap:6px;border-left:2px solid var(--line);padding-left:14px}.list .reply{padding:8px 10px;background:var(--panel2);border:0;border-radius:var(--radius-s);box-shadow:none}</style>
```

변경 diff 요약 (8곳): input border `--pastel-lav`→`--line` + `background:var(--frame-bg);color:var(--ink)` 추가 + radius 토큰화 / button radius 토큰화 / `.list article` bg `--pastel-cream`→`--frame-bg` + radius `--radius-m` + `box-shadow:var(--shadow-soft)` / `.ava` bg `--pastel-lav`→`--line` / `.list header button` bg `--pastel-lav`→`--accent-tint` / `.replies` border `--pastel-lav`→`--line` + padding-left 10→14px / `.list .reply` bg transparent→`--panel2` + border 1px→0 + radius 토큰화 + `box-shadow:none` 추가(`.list article` 그림자 상속 차단 — 답글도 `<article>`).

- [ ] **Step 2: 테스트 실행 (no-hardcoded-colors 포함 전체 회귀)**

Run (`a-mate/`에서): `npm test`
Expected: `Test Files 19 passed (19)`, `Tests 166 passed (166)` — 신규 테스트 없음, 기존 전부 녹색

- [ ] **Step 3: 빌드 검증**

Run (`a-mate/`에서): `npm run build`
Expected: `✓ built in …s` — 에러 0

- [ ] **Step 4: 커밋**

```powershell
git add a-mate/src/lib/ui/GuestbookTab.svelte
git commit -m "fix(agent): use skin tokens in guestbook and inset replies"
```

---

### Task 2: 로드맵 완료 기록 (공유 파일 — main 반영 후)

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (G7 항목)

**Interfaces:**
- Consumes: Task 1 완료 사실 (커밋 존재)
- Produces: 로드맵 G7 완료 표기 (V1 세션과 충돌 없는 최신 base 위)

- [ ] **Step 1: origin/main 반영 (병렬 V1 세션 대비)**

```powershell
git fetch origin
git merge origin/main
```

Expected: `Already up to date.` 또는 무충돌 머지. 로드맵 충돌 시 양쪽(G7/V1) 기록을 모두 보존해 해소.

- [ ] **Step 2: G7 항목에 완료 표기**

`**G7 — 방명록 텍스트 박스 스킨 토큰 + 원글/답글 시인성 (소품)** (백로그 — 2026-07-27 검증 피드백)` 헤딩 끝에 ` — ✅ 완료(2026-07-27, feat/guestbook-skin-tokens)` 추가하고, 항목 마지막에 한 줄 추가:

```markdown
- **구현 결과(2026-07-27)**: 기존 의미 토큰 재사용(theme.css 변경 0) — 원글 `--frame-bg` 카드+`--shadow-soft`, 답글 `--panel2` 인셋+들여쓰기 14px, 소버튼 `--accent-tint`, radius 토큰화. `<style>` 한정, 마크업 불변. 실화면 스크린샷 확인은 PR 체크리스트로 사용자 진행.
```

- [ ] **Step 3: 커밋**

```powershell
git add docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md
git commit -m "docs(plan): record G7 guestbook skin tokens completion"
```

---

### Task 3: docs-archive DoD + PR 생성

**Files:**
- Move (docs-archive 스킬이 수행): 본 계획 + 스펙 → `docs/archive/` 미러

**Interfaces:**
- Consumes: Task 1·2 커밋 완료 상태
- Produces: PR (스크린샷 체크리스트 포함)

- [ ] **Step 1: docs-archive 스킬 실행** (ADR 0013 DoD)

Skill `docs-archive` 호출 — 대상: `docs/design/a-mate/specs/2026-07-27-guestbook-skin-tokens-design.md`, `docs/design/a-mate/plans/2026-07-27-guestbook-skin-tokens.md`. 스킬 규약대로 미러 이동·커밋. (로드맵은 진행 중 문서라 아카이브 대상 아님.)

- [ ] **Step 2: 푸시 + PR 생성**

```powershell
git push -u origin feat/guestbook-skin-tokens
gh pr create --title "fix(agent): guestbook skin tokens and reply visibility (G7)" --body @'
## Summary
- GuestbookTab <style>의 고정 파스텔 토큰(--pastel-cream/--pastel-lav)을 스킨 의미 토큰으로 교체 (theme.css 변경 0)
- 원글 = --frame-bg 카드 + --shadow-soft, 답글 = --panel2 인셋 + 들여쓰기 14px — 원글/답글 위계 명확화
- 입력창 배경/글자 명시(--frame-bg/--ink), 헤더 소버튼 --accent-tint, radius 토큰화
- 스펙·계획 문서는 ADR 0013 DoD로 docs/archive/ 미러 이동

## Test plan
- [x] npm test — no-hardcoded-colors 포함 166건 녹색
- [x] npm run build — vite build 에러 0
- 실화면 스크린샷 확인 (hub 연결 환경, 사용자):
  - [ ] 원글/답글 층위 대비 — 라이트·다크 각 1스킨 이상
  - [ ] 목록에서 카드 그림자(--shadow-soft) 과함 여부 — 과하면 그림자 제거 + border 1px var(--line) 폴백
  - [ ] 입력창·헤더 소버튼(--accent-tint) 시인성
  - [ ] 답글 폼이 --panel2 인셋 옆에서 어색하지 않은지

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'@
```

Expected: PR URL 출력. PR 번호를 로드맵 완료 표기에 반영할지는 선택(브랜치명 기재로 충분 — G5 선례).
