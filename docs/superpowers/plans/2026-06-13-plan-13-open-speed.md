# earworm — Plan 13: fast song opens + loading indication

> **For agentic workers:** Use superpowers:executing-plans. Checkboxes track steps.

**Goal:** Song opens get (1) a visible loading state, (2) parallel decode of song+stems, (3) 48 kHz-normalized stem caches so prepared songs stop paying the sinc-resample tax on every open.

**Measured baseline:** open of a 4-min file ≈ 2 s decode+resample; with stems it's 5 sequential files (all caches are 44.1 kHz today) ≈ 5×.

---

### Task 1: Parallel decode in the open phase

- [x] In the heavy phase of `song.open` (`dispatch_shared` slow phase / `App::dispatch` inline equivalent): decode the original and the 4 stem WAVs concurrently with `std::thread::scope` (5 threads, each `decode_file`); join, propagate the first error. Peaks compute stays after the original's decode (needs it) but runs while stems decode if convenient — don't gold-plate.
- [x] `cargo test` green (existing open tests cover correctness). Commit: `perf(server): parallel decode of song + stems on open`

### Task 2: 48 kHz stem caches

- [x] `stems.rs`: after demucs (and in `FakeSeparator`), post-process each produced WAV: if its sample rate ≠ 48000, decode via `engine::decode::decode_file` (which resamples to 48k) and rewrite with `engine::capture::write_wav` (48k). One sinc pass at separation time, never again at open.
- [x] Lazy upgrade for existing caches: in the open slow-phase, after decoding a stem, if the source WAV header wasn't 48k, rewrite it (same helpers) so the *next* open is free. Cheap guard: read the header rate (hound `WavReader::spec()`), don't re-decode twice.
- [x] Tests: FakeSeparator output is 48k (assert header); a seeded 44.1k cache WAV gets rewritten to 48k after one open (app_stems-style test).
- [x] Commit: `perf(server): stem caches normalized to 48k at separation; lazy upgrade on open`

### Task 3: Loading indication

- [x] `stores.ts`: `openingSong: writable<number | null>` set around `openSong()` (id while in flight, null on settle — also on error).
- [x] Library rows: the clicked row shows the animated `◌` glyph (same as PrepareModal) next to the title while `openingSong === song.id`; rows disabled (no double-fire) while any open is in flight.
- [x] Stage: while opening and no song open yet, "no song open" becomes `opening…`; when switching songs, keep the old waveform but overlay a thin indeterminate bar at the top of the stage (2 px, accent, existing animation language).
- [x] `pnpm build && pnpm vitest run` clean. Commit: `feat(desktop): song-open loading indication`

### Task 4: Verify + gate

- [x] Timing proof on a generated 4-min file with FakeSeparator-style 44.1k stems seeded: time `song.open` via socket before/after Tasks 1–2 on the same data (expect ≥3× improvement on the stems case; report numbers). Second open after lazy upgrade should be faster still.
      Measured (release, `open_timing` harness, 240 s file seeded at 44.1 kHz): single decode (no stems) 1.78–1.81 s → sequential-5-decode baseline ≈ 9 s. With Tasks 1–2: stems open #1 (parallel decode + lazy 48 k upgrade) **2.27 s** (~4×), opens #2/#3 (48 k caches, peaks cached) **1.80–1.83 s** (~5×, ≈ the cost of the original mix alone).
- [x] Visual: screenshot the loading state mid-open (open the big file via sendshortcut-driven… clicking isn't possible — instead use `EARWORM_OPEN` for launch-time open and screenshot the stage `opening…` state during launch; the per-row spinner can be verified by code review if not capturable — note honestly which was seen).
      Seen live (temp DB, `EARWORM_OPEN=1`, grim mid-open): stage `opening…` text AND the amber `◌` spinner on the library row, both in the same frame. The 2 px song-switch bar needs a second song already open — verified by code review only.
- [x] Full gate: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run && pnpm build`. Commit: `feat: plan 13 complete — fast opens with loading indication`
