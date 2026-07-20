**Life input responsiveness design**

**Problem**

- Life state polling, movement, and room-list requests use the synchronous `ureq` client.
- The commands are marked `#[tauri::command(async)]`, so their synchronous HTTP bodies run directly on Tauri's async executor.
- A slow request can occupy executor workers and delay unrelated window IPC such as mascot dragging.
- The mascot room menu refresh interval can start another request before the previous refresh completes.

**Decision**

- Execute every synchronous Life HTTP operation through `tauri::async_runtime::spawn_blocking`.
- Keep SQLite reads outside the blocking task and never hold the store mutex during HTTP.
- Permit only one in-flight room-list refresh and one in-flight cell movement per view.
- Open the mascot room menu immediately, then populate it asynchronously.
- Keep the last rendered Life state during a transient failure, but expose movement failure through the existing message UI.

**Expected behavior**

- Window movement and pointer IPC remain responsive while the Life server is slow or unavailable.
- Polling does not accumulate overlapping requests.
- Repeated cell clicks do not create a burst of delayed move requests.
- Right-click gives immediate visual feedback even while the room list is loading.

**Verification**

- Rust tests and compilation cover the command signature changes.
- Frontend tests and production build must pass.
- Runtime verification checks that Life polling continues, movement returns `200`, and mascot dragging remains responsive while a Life request is pending.
