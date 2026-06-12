# earworm — Plan 10: Fader primitive (slider abstraction)

> **For agentic workers:** Use superpowers:executing-plans. Checkboxes track steps.

**Goal:** One slider primitive that renders identically everywhere, replacing both the native `<input type=range>` (Transport) and the webkit-mangled vertical range (StemMixer). Plus: reopen-last-song on launch (real feature, also makes the mixer visually verifiable).

**Diagnosis:** webkit2gtk does not support vertical native range inputs (`appearance: slider-vertical` is Blink-only; `writing-mode` vertical ranges render as broken horizontal stubs — confirmed by user screenshot of the stem mixer). Any UI relying on native range styling is at webkit's mercy; we already own Button/Group/Toolbar, so own the slider too.

**The abstraction:** `lib/ui/Fader.svelte` — fully custom, pointer-based:
- Props: `value: number` (bindable), `min/max/step`, `orientation: "horizontal" | "vertical"`, `accent?: boolean`, `disabled?`, `onchange?: (v) => void`, `format?: (v) => string` (title/aria text).
- Rendering: track div (2 px line, `--line`) + filled portion (`--accent` when accent, else `--muted`) + thumb (12 px square, 2 px radius — matches design language). No native input element.
- Interaction: Pointer events with `setPointerCapture` (drag anywhere on track jumps + drags); ArrowUp/Right +step, ArrowDown/Left −step, Home/End (focusable, `role="slider"`, `aria-valuenow/min/max`, `tabindex=0`). Wheel optional: ignore (waveform owns wheel semantics; avoid surprise).
- Sizing: horizontal = `height: var(--control-h); min-width: 80px; flex: 1` within Group(grow); vertical = `width: var(--control-h); height: 100%` (parent decides height, mixer uses ~96px).

---

### Task 1: Fader primitive + math tests

- [x] `lib/ui/Fader.svelte` per spec above. Extract the pure value math to `lib/ui/fader-math.ts`: `posToValue(pos01, min, max, step)` and `valueToPos01(v, min, max)` with clamping/step-rounding — vitest both (5+ cases incl. step rounding at bounds, min==max guard).
- [x] `pnpm vitest run && pnpm build` clean. Commit: `feat(desktop): custom Fader primitive (webkit-proof slider)`

### Task 2: Adopt everywhere

- [ ] Transport rate slider → horizontal `Fader` (accent), same 0.25–2.0 step 0.05 behavior.
- [ ] StemMixer → vertical `Fader` per channel (~96 px tall, accent on BASS), values 0–1 step 0.01; M/S behavior unchanged; remove all the old vertical-range CSS.
- [ ] Grep for any remaining `type="range"` in src/ — there must be none afterward.
- [ ] `pnpm build` clean. Commit: `refactor(desktop): transport + mixer on Fader`

### Task 3: Reopen last song on launch

- [ ] `stores.ts`: after a successful `openSong(id)`, persist id to localStorage (`earworm-last-song`); on `initEvents()` startup, if present, attempt `openSong` (silently ignore failure — song may be gone).
- [ ] Commit: `feat(desktop): reopen last song on launch`

### Task 4: Visual verification (the gate)

- [ ] Rebuild release. Launch (the real DB's last-opened song should auto-open; if no stems song was last, `song.open` the Deftones one once via UI… NO — instead: before launching, run a release earwormd? Cannot share DB. Simplest: launch app, it auto-opens last song (user's screenshot shows a stems song was open — its id is in localStorage? localStorage starts empty for this feature!). Therefore: open the song programmatically ONCE through the UI is impossible without clicks — so instead seed localStorage: webkit2gtk localStorage lives under `~/.local/share/earworm/` app data dir (`dev.shawn.earworm/localstorage` or similar) — too fragile. FALLBACK plan: temporary `EARWORM_OPEN` env: in `host.rs` add a tauri command `initial_song()` returning `std::env::var("EARWORM_OPEN").ok()` parsed as i64; stores calls it at init and opens that id when set (tiny, permanent, harmless dev affordance). Launch with `EARWORM_OPEN=<id of stems song>`.
- [ ] Float window via hyprctl (pattern from plan 8), screenshot at ~1800 and ~1100 wide, Read the PNGs: faders render as proper vertical faders (track + thumb), BASS accented, rate Fader matches design, drag affordance visible (thumb), nothing overflows. Iterate until clean; leave PNGs in /tmp.
- [ ] Commit: `fix(desktop): fader visual verification` (if fixes) 

### Task 5: Gate

- [ ] `cargo test && cargo clippy --workspace -- -D warnings && pnpm vitest run && pnpm build`. Commit final state.
