# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Earworm is an ear-first practice looper for Linux: load a song, loop sections,
slow them down pitch-preserving, and run evidence-based practice plans. It ships
as a Tauri desktop app (Svelte 5 frontend, Rust host) and a headless daemon that
share the same Rust core. See `README.md` for the feature-level "what it does".

## Commands

Use `just` (the task runner) for everything; `just` alone lists recipes.

- `just dev` — desktop app in dev mode (vite hot-reload + debug Rust host)
- `just build` — release build of the desktop app (`target/release/earworm`) + headless daemon (`target/release/earwormd`)
- `just run` / `just daemon` — run the release UI / headless daemon
- `just test` — full suite: `cargo test --workspace` + `pnpm vitest run`
- `just lint` — clippy (`-D warnings`), `cargo fmt --check`, `svelte-check`
- `just check` — pre-commit gate (test + lint)
- `just cmd '{"id":1,"cmd":"song.list"}'` — send a raw JSON command to a running instance over its Unix socket

Targeted runs (no recipe — invoke directly):
- Single Rust test: `cargo test -p practice store::tests::name_of_test`
- Single Rust crate: `cargo test -p engine`
- Single frontend test: `cd apps/desktop && pnpm vitest run lib/waveform-math.test.ts`

Frontend tooling lives under `apps/desktop` and uses **pnpm** (not npm).

## Architecture

### One dispatch surface, many clients

The entire backend is reached through a single command dispatcher,
`server::app::App` (`crates/server/src/app.rs`). Requests are
`{id, cmd, params}` JSON; responses are `{id, ok, data?, error?}`; pushed
state is `{event, data}`. Two transports wrap the *same* dispatcher:

- **Unix socket** (`crates/server/src/socket.rs`) — JSON-lines at
  `$XDG_RUNTIME_DIR/earworm.sock`. Used by the headless daemon (`earwormd`)
  and by `just cmd` / shell scripts.
- **Tauri webview** (`apps/desktop/src-tauri/src/host.rs`) — one `dispatch`
  command in, one `earworm://event` channel out. The UI is "just another
  client".

There is exactly **one tick pump** (`server::socket::serve`, ~50 ms), regardless
of how many clients attach. The desktop passes an `on_events` hook so the same
pump that broadcasts to socket subscribers also mirrors ticks into the webview.

**Lock phasing:** known-heavy commands (`song.open`, `song.import`,
`capture.grab`) run their decode/hash/IO phase *outside* the `App` mutex via
`dispatch_shared`, so multi-second decodes never block the pump or other
clients. When adding a heavy command, follow the `*_phased` pattern in `app.rs`.

### The three crates

- **`engine`** (`crates/engine`) — real-time audio. Decode (symphonia),
  pitch-preserving stretch (Rubber Band R3 via `ffi.rs` + `stretch.rs`),
  sample-accurate crossfaded looping, PipeWire output and capture, filters
  (bass focus), waveform peaks. Audio thread talks to control thread over
  lock-free ring buffers (`ring.rs`, rtrb); commands/events flow through
  `pipeline.rs` (`EngineCmd`/`EngineEvent`).
- **`practice`** (`crates/practice`) — domain + persistence. `model.rs` holds
  the wire types (Song, Section, LoopRegion, Plan, Rating, TempoCurve…),
  `tempo.rs`/`junction.rs`/`schedule.rs` are the practice intelligence,
  `runner.rs` drives plan steps, and `store.rs` owns all SQLite I/O.
- **`server`** (`crates/server`) — the dispatcher + transports above, plus
  the bridges to external work: `analysis.rs`, `stems.rs`, `capture_control.rs`.

The desktop app (`apps/desktop/src-tauri`, binary name `earworm`) depends on all
three and embeds the built Svelte frontend.

### Persistence

Single SQLite DB (rusqlite, bundled) at `~/.local/share/earworm/earworm.db`.
Schema is **embedded in `crates/practice/src/store.rs`** — no migration files;
versioning is incremental via `PRAGMA user_version` (currently V1 core tables →
V2 `analysis` cache → V3 `settings`). To evolve the schema, add a new version
block in `store.rs` rather than editing existing ones. App settings live in the
SQLite `settings` table as JSON; there are no TOML/JSON config files. Override
the DB path with `--db` (daemon) or `EARWORM_DB` (desktop).

Complex sub-objects (LoopKind, PlanStep arrays, analysis vectors) are stored as
`serde_json` in `*_json` columns, not normalized.

### Frontend (Svelte 5 + Tauri)

`apps/desktop/src`. **All UI state derives from dispatch responses + events —
no second source of truth** (`lib/stores.ts` mirrors the wire shapes of
`server::app::App`). `lib/ipc.ts` is the only place that touches Tauri: `cmd()`
sends a request, `onEvent()` subscribes to the event channel. Pure logic
(waveform math, fader math, zoom, colors) lives in `lib/*.ts` with colocated
`*.test.ts` vitest files; `components/` are the Svelte views; `lib/ui/` is the
shared widget kit.

**UI vocabulary** — names used in conversation and in code/CSS, so a spoken
term maps to one thing. The three columns are **panes**:

- **Library** (left, `aside.library`) — the song list.
- **Stage** (center, `main.stage`) — the work surface. Down it sit the
  **waveform**, then a stack of **boxes** (`<section class="box">`, label header
  + body): the **controls box** (`Transport.svelte`), the **stems box**
  (`StemMixer.svelte`), the **structure box** (`Analysis.svelte`), and the
  **analyze box** (`AnalyzePrompt.svelte`, the CTA shown until a track has any
  analysis/stems). Call them *boxes*, never "containers" or "panels".
- **Panel** (right, `aside.panels`) — its switchable views are **tabs**
  (sections, loops, plan, capture, due, profile, settings, guide). Note the
  *structure box* (center) and the *sections tab* (right) are different things.

Some stage state is purely client-side and mirrored by no store — the
waveform's zoom (`view`) and clicked active span live as local `$state` in
`Waveform.svelte`. The **reset workspace** control (⟲ in the controls box)
refits the zoom and clears that local state plus the selection, active loop, and
playhead; because the action (`resetWorkspace` in `stores.ts`) can't reach the
local state directly, it bumps the `workspaceReset` nonce store that the
waveform watches.

### External analysis & stems (Python)

Beat/downbeat/section analysis and Demucs stem separation run as out-of-process
Python, invoked through repo-shipped wrappers in `scripts/` (`analyze`,
`analyze_impl.py`, `songformer_impl.py`). The `analyze` wrapper **bootstraps its
own `uv` venv on first run** (downloads torch etc.) at
`~/.local/share/earworm/analyze-venv` (override `$EARWORM_ANALYZE_VENV`).
Contract: **stdout carries exactly one JSON object; all diagnostics go to
stderr.** Swapping models stays entirely in Python — the Rust side
(`server::analysis`, `server::stems`) only parses the JSON, so never let
non-JSON leak to stdout.

## Conventions

- The `time` crate is pinned `>=0.3, <0.3.48` (a regression breaks tauri-utils);
  don't bump past it.
- Errors crossing the protocol boundary collapse to the `error: String` channel
  via the `ErrStr`/`err_str` helper in `app.rs`.
