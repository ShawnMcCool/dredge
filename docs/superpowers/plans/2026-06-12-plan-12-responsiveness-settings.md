# earworm — Plan 12: UI responsiveness, exit confirm, settings

> **For agentic workers:** Use superpowers:executing-plans. Checkboxes track steps.

**Goal:** (1) The UI must never freeze: async command dispatch + heavy operations decode outside the app lock. (2) Escape asks "exit?" via the Modal primitive. (3) Durable settings (server-side table) with a Settings modal: UI scale, grid-snap default, capture buffer length.

---

### Task 1: Kill the hangs

**Diagnosis:** `host::dispatch` is a sync Tauri command → runs on the GTK main thread → any slow command freezes the window. Worst offenders hold the `Mutex<App>` while decoding entire songs (`song.open`: symphonia + rubato + peaks, seconds per song; `song.import`; `capture.grab`), which also starves the 50 ms tick pump (position events + socket stall).

- [x] `host.rs`: make the command `async fn` (Tauri runs it on a worker thread — the window keeps painting even while a command waits on the lock). *(`async fn` + `spawn_blocking` so decodes don't tie up runtime workers.)*
- [x] Phase the heavy commands so decode runs WITHOUT the lock. In `server`, add a shared entry point used by BOTH the socket handler and the Tauri command:
  ```rust
  /// Dispatch that holds the App lock only for state work — known-heavy
  /// commands (song.open, song.import, capture.grab) run their decode/
  /// hash/IO phase outside the lock.
  pub fn dispatch_shared(app: &Arc<Mutex<App>>, req: Request) -> Response
  ```
  Mechanics per command, e.g. `song.open`: lock → look up `(path, hash)` → unlock → `decode_file` + `peaks::load_or_compute` (pure, slow) → lock → `finish_open(song_id, buf_or_stems, peaks)` (loads engine, sets open_song, builds response). `App` gains the small phase methods (`open_lookup`, `finish_open`, `import_prepared`, `grab_snapshot_then_import` analog); `App::dispatch` keeps working for everything else (and remains what tests use), with heavy commands in `App::dispatch` simply delegating to the phases inline (same behavior, single-threaded tests unaffected).
  Stems auto-load on open: decode all stem WAVs also outside the lock (they're part of the slow phase).
- [x] `socket.rs` request loop and `host.rs` both call `dispatch_shared`. The tick pump and other clients now stall at most for the short lock phases.
- [x] Tests: existing suites stay green (they exercise `App::dispatch`). Add one: `dispatch_shared` on `song.open` returns the same payload as `App::dispatch` (parity test with a small WAV). *(`crates/server/tests/dispatch_shared.rs` — byte-identical serialized responses, plus import-dedupe parity.)*
- [ ] Measured proof (in Task 5's live check): while `song.open` of a ~4-minute file is in flight, a concurrent `status` over the socket answers in < 250 ms (was: blocked for the whole decode). *(Before measured on pre-change earwormd: status blocked 2089 ms during a 2.15 s open.)*
- [x] Commit: `perf(server,desktop): async dispatch; heavy commands decode outside the app lock`

### Task 2: Escape → exit confirmation

- [x] Escape cascade in `keys.ts` (in order): quick-prompt discard → clear selection → **open exit modal**. Exit modal (Modal primitive, `closable`): "exit earworm?" — Buttons `exit` (accent) / `stay`; Enter or `y` = exit, Escape/`n` = stay. *(Modal's Escape now `preventDefault`s so the global cascade can tell a consumed Escape apart.)*
- [x] Host command `quit()` (`app_handle.exit(0)`) registered alongside `dispatch`; wire the exit Button to it.
- [x] Commit: `feat(desktop): escape exit confirmation`

### Task 3: Durable settings + menu

- [x] Store migration **v3**: `settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL)` + `Store::{get_setting, set_setting}` (serde_json round-trip, tests in store suite style). *(Plus `all_settings` for `settings.get_all`.)*
- [x] Commands: `settings.get_all` → `{key: value, ...}`; `settings.set {key, value}` (value = arbitrary JSON). Both trivial dispatch arms + one app test (`tests/app_settings.rs`).
- [x] Known keys (constants in stores.ts): `ui_scale` (number, default per-screen heuristic), `grid_snap_default` (bool, default true), `capture_buffer_secs` (number, default 180 — pass into `capture.start` when the UI calls it).
- [x] `zoom.ts` rework: read `ui_scale` from `settings.get_all` at init (one-time migration: if absent and localStorage `earworm-zoom` exists, adopt + persist it); ctrl±/0 still work and write through to `settings.set`. localStorage no longer authoritative.
- [x] `SettingsModal.svelte`: gear Button (icon variant, `⚙` or `settings` text chip) at the right end of the tabs row + `,` keybinding. Rows: **UI scale** (horizontal Fader 0.75–2.5 step 0.05, live-applied via `setZoom` on drag, readout %), **grid snap by default** (toggle Button), **capture buffer** (chips 60/120/180/300 s). All writes immediate via `settings.set`. Help footer gains `, settings`.
- [x] `pnpm build && pnpm vitest run` clean. Commit: `feat: durable settings table + settings modal (ui scale, grid snap, capture buffer)`

### Task 4: Visual verification

- [ ] Temp-DB app launch (EARWORM_DB + EARWORM_OPEN pattern). Screenshots, Read and judge each: (a) settings modal open (`,` via sendshortcut — remember ~1 s latency) with the scale Fader and rows; (b) exit modal (Escape); (c) after dragging nothing — confirm `stay` dismisses (send `n`). Leave /tmp/ew-set-*.png.
- [ ] Commit fixes if any: `fix(desktop): settings/exit verification`

### Task 5: Live responsiveness proof + gate

- [ ] Generate a 4-minute file (`ffmpeg ... anoisesrc=d=240` or sine), import via socket to the temp DB, then: send `song.open` from one socket client and immediately hammer `status` from another, timing each reply (python, monotonic clock). Assert max `status` latency < 250 ms while open is in flight. Report the numbers.
- [ ] Full gate: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run && pnpm build`. README: settings + escape notes (two lines).
- [ ] Commit: `feat: plan 12 complete — responsive dispatch, exit confirm, settings`
