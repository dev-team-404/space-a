---
status: done
archived: 2026-08-05
---

# External Integration Opt-in Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure a fresh checkout demonstrates A-Mate and A-Lens without unreachable private endpoints or unauthenticated requests to the protected A-Hub deployment.

**Architecture:** External integration becomes opt-in at each boundary. A-Mate only renders an A-Lens link when a URL is stored and only resolves A-Hub configuration from stored settings or environment variables; A-Lens starts with bundled dummy data until an operator explicitly selects the Hub source.

**Tech Stack:** Svelte 5, TypeScript, Vitest, Rust, Cargo test, Python 3.11, pytest

## Global Constraints

- Keep `https://spacea.msalt.net` and `sw-innov` as examples, not an active unauthenticated connection.
- Never embed the A-Hub `x-api-key` in source code or an installer.
- Preserve explicit settings and environment-variable integrations.
- Keep the existing `off` A-Lens value compatible.
- Update only current-state documentation that describes these defaults.

---

### Task 1: Document the Opt-in Policy

**Files:**
- Modify: `docs/architecture/a-mate/02-features.md`
- Modify: `docs/architecture/a-mate/03-architecture.md`
- Modify: `docs/architecture/a-lens/build-and-run.md`
- Modify: `a-mate/README.md`
- Modify: `a-lens/CLAUDE.md`

**Interfaces:**
- Consumes: Approved external-integration policy.
- Produces: Current-state documentation that the code changes must match.

- [x] **Step 1: Replace active-default claims**

  State that A-Lens links are hidden until configured, A-Hub sharing is inactive until a URL is stored or provided by environment, and A-Lens defaults to bundled dummy data.

- [x] **Step 2: Check documentation consistency**

  Run: `rg -n "설치를 안 해도|설치 직후 바로 동작|A_LENS_SOURCE=auto|10\\.116\\.67\\.170" a-mate a-lens docs/architecture --glob '!**/node_modules/**'`

  Expected: no current-state claim says a protected or private endpoint works automatically.

### Task 2: Hide an Unconfigured A-Lens Link

**Files:**
- Modify: `a-mate/src/lib/lens.test.ts`
- Modify: `a-mate/src/lib/lens.ts`
- Modify: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte`

**Interfaces:**
- Consumes: `a_lens_url` from A-Mate settings.
- Produces: `lensRoomUrl(settings): string`, returning `""` when no valid URL is explicitly configured.

- [x] **Step 1: Write the failing tests**

  Change the empty-settings and space-only cases to expect `""`; retain a literal custom URL expectation.

- [x] **Step 2: Run the focused test and verify RED**

  Run in Windows PowerShell from `a-mate`: `npm exec vitest run src/lib/lens.test.ts`

  Expected: the empty-settings tests fail because the private default URL is still returned.

- [x] **Step 3: Implement the minimal link policy**

  Remove the private default URL, return `""` for an empty setting, keep HTTP(S) validation and `off` compatibility, and show a localhost example only as the settings placeholder.

- [x] **Step 4: Run the focused test and verify GREEN**

  Run: `npm exec vitest run src/lib/lens.test.ts`

  Expected: all `lens.test.ts` tests pass.

### Task 3: Make A-Hub Resolution Opt-in

**Files:**
- Modify: `a-mate/crates/core/src/hub.rs`
- Modify: `a-mate/src-tauri/src/commands.rs`
- Modify: `a-mate/src/lib/ui/settings/ConnectionGroup.svelte`

**Interfaces:**
- Consumes: stored `knowledge_hub_url` or `SPACE_A_HUB_URL`.
- Produces: `HubConfig::resolve(&SqliteStore) -> Option<HubConfig>`, returning `None` when neither source is configured.

- [x] **Step 1: Write the failing Rust test**

  Replace the unset-default test with `config_is_none_when_store_and_env_are_unset`, asserting `HubConfig::resolve(&store).is_none()`.

- [x] **Step 2: Run the focused test and verify RED**

  Run in Windows PowerShell from `a-mate`: `cargo test -p agent-mentor config_is_none_when_store_and_env_are_unset`

  Expected: failure because `resolve` still returns `spacea.msalt.net` defaults.

- [x] **Step 3: Implement minimal opt-in resolution**

  Require a stored URL or `SPACE_A_HUB_URL`; preserve stored API key, token, user, space fallback, and explicit `share=off`. Show `spacea.msalt.net` only as a UI placeholder and make empty settings visibly inactive.

- [x] **Step 4: Run focused Rust tests and verify GREEN**

  Run: `cargo test -p agent-mentor hub::tests::config_`

  Expected: Hub configuration tests pass.

### Task 4: Start A-Lens With Bundled Demo Data

**Files:**
- Create: `a-lens/backend/tests/test_settings.py`
- Modify: `a-lens/backend/alens/settings.py`
- Modify: `a-lens/backend/alens/collector.py`
- Modify: `a-lens/backend/.env.example`

**Interfaces:**
- Consumes: optional `A_LENS_SOURCE` and `A_LENS_WORK_URL` environment variables.
- Produces: settings defaulting to `source="dummy"` and `work_url=""` while preserving explicit environment overrides.

- [x] **Step 1: Write the failing default-settings test**

  Reload settings with related environment variables removed and assert `source == "dummy"` and `work_url == ""`; add a second test proving explicit Hub values still win.

- [x] **Step 2: Run the test and verify RED**

  Run from `a-lens/backend`: `.venv/bin/python -m pytest tests/test_settings.py -q -s`

  Expected: the default test fails with `source="auto"` and `work_url="https://spacea.msalt.net"`.

- [x] **Step 3: Implement the minimal defaults**

  Change only the settings defaults and matching comments/example environment file.

- [x] **Step 4: Run the test and verify GREEN**

  Run: `.venv/bin/python -m pytest tests/test_settings.py -q -s`

  Expected: both settings tests pass.

### Task 5: Verify and Integrate

**Files:**
- Verify all files changed by Tasks 1-4.

**Interfaces:**
- Consumes: Completed component changes.
- Produces: One coherent PR commit with matching behavior, tests, and docs.

- [x] **Step 1: Run full A-Mate verification**

  Run in Windows PowerShell from `a-mate`: `npm test` and `cargo test`.

- [x] **Step 2: Run full A-Lens backend verification**

  Run from `a-lens/backend`: `.venv/bin/python -m pytest -q`.

- [x] **Step 3: Check the diff**

  Run: `git diff --check` and inspect `git diff` for unrelated changes or committed secrets.

- [x] **Step 4: Archive this completed plan**

  Use the repository `docs-archive` skill so this working document leaves the active docs root.

- [x] **Step 5: Commit and push**

  Run: `git commit -m "fix(integration): make external services opt-in"`, then push the current PR branch.
