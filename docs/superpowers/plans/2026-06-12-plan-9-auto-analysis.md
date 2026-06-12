# earworm — Plan 9: auto analysis (beat grid + section suggestions)

> **For agentic workers:** Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** One "Analyze" action per song: beats/downbeats/BPM (beat_this) + suggested sections (SongFormer if it cooperates, novelty fallback otherwise). Results: beat ticks + bar-snapped loop drags on the waveform, suggested rows in the Sections lane (user edits → save → junctions re-derive **using downbeats** when available).

**Architecture:** A repo-shipped wrapper (`scripts/analyze`, extensionless, bootstraps its own uv venv at `~/.local/share/earworm/analyze-venv` on first run) prints one JSON contract to stdout:
```json
{"bpm": 98.2, "beats": [0.61, 1.22, ...], "downbeats": [0.61, 3.05, ...],
 "sections": [{"label": "A", "start": 0.0, "end": 31.4}, ...], "engine": "beat_this+novelty|songformer"}
```
Rust side mirrors the stems pattern exactly: `Analyzer` trait (Real = subprocess, Fake = tests), background job, `analysis_progress` event, results cached in a new `analysis` table (one row per song, JSON columns). The wrapper is the *only* place models live — swapping section models later never touches Rust.

**Spike results (already verified on this machine):**
- `beat_this` works: uv venv python 3.12, `beat_this @ git+https://github.com/CPJKU/beat_this`, deps `torch soundfile librosa einops rotary-embedding-torch`; `File2Beats(checkpoint_path="final0", device="cuda", dbn=False)`; checkpoint auto-downloads to torch hub cache. GPU inference ~2 s after warmup.
- `allin1` is DEAD on this stack (natten pinned to old API, no wheels for torch 2.12/cu130) — do not retry it.
- SongFormer via HF AutoModel is broken packaging; the real path is the cloned repo (`/tmp/SongFormer` already cloned): `src/SongFormer/infer/infer.py`, pinned torch==2.4.0, needs `muq`, `musicfm` (in HF model repo via snapshot_download "ASLP-lab/SongFormer"), ema_pytorch, omegaconf, loguru; input 24 kHz; 8-class functional labels.

---

### Task 1: Wrapper script + venv bootstrap

**Files:** Create `scripts/analyze` (extensionless, executable) + `scripts/analyze_impl.py`.

- [ ] `scripts/analyze` = bash bootstrap: ensures `~/.local/share/earworm/analyze-venv` exists (`uv venv --python 3.12` + `uv pip install --python ...` of the beat_this dep set), then `exec`s the venv python on `analyze_impl.py "$@"`. Idempotent, quiet when venv is ready; all diagnostics to **stderr** (stdout is the JSON contract).
- [ ] `analyze_impl.py`: args `<audio> [--no-sections]`. Runs beat_this → beats/downbeats; bpm = median inter-beat 60/Δ. Sections v1 = novelty: librosa CQT-chroma + MFCC stacked self-similarity, `librosa.segment` novelty peaks (or `librosa.onset` on the SSM diagonal — implementer's judgment), boundaries snapped to nearest downbeat, merged below 4 bars, labeled `A B C ...` (repeating segments may share a label via simple chroma-mean clustering — best effort, don't gold-plate). Output the JSON contract; `engine` field reports what produced sections.
- [ ] Verify live on `/home/shawn/downloads/Deftones - Kimdracula (Bass Only).mp3`: sane bpm, downbeats ≈ every 4 beats, ≥3 sections with boundaries on downbeats. Print the JSON to the report.
- [ ] Commit: `feat(analyze): wrapper script — beat grid + novelty sections behind a JSON contract`

### Task 2: SongFormer attempt (time-boxed)

- [ ] In a SECOND venv (`~/.local/share/earworm/songformer-venv`, torch==2.4.0 per its requirements): try to get `src/SongFormer/infer/infer.py` (repo: vendor the needed subset into `scripts/songformer/` or pip-install from git if possible) running on the Deftones file, checkpoints via `huggingface_hub.snapshot_download("ASLP-lab/SongFormer")`. If it works end-to-end: `analyze_impl.py` gains `--sections-engine songformer` (subprocess into that venv), and the wrapper prefers it when the venv exists, falling back to novelty on any failure (stderr-log, never die — beat grid must always ship).
- [ ] **Time-box: if after ~10 tool-call rounds of dependency fighting it does not produce labels, STOP**, keep novelty, record exactly what blocked it in the plan file, and move on. This task failing is an acceptable outcome.
- [ ] Commit (either): `feat(analyze): songformer section engine` / `docs(analyze): songformer attempt notes — novelty stands`

### Task 3: Server — Analyzer trait, commands, downbeat-aware junctions

**Files:** `crates/server/src/analysis.rs` (new), `app.rs`, `practice` store migration, `tests/app_analysis.rs`.

- [ ] `Analyzer` trait (mirror `StemSeparator`): `analyze(&self, audio: &Path) -> Result<AnalysisResult, String>`, `is_available()`. Real impl runs `scripts/analyze` (resolve relative to exe: `../../scripts/analyze` fallback to `$EARWORM_ANALYZE` env, then PATH). Fake returns a fixture.
- [ ] Store migration v2: `analysis (song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE, bpm REAL, beats_json TEXT, downbeats_json TEXT, sections_json TEXT, engine TEXT)` + `Store::{save_analysis, get_analysis}` (+ store tests, existing style; migration must upgrade existing v1 DBs — guard on `user_version`).
- [ ] Commands: `analysis.run {song_id}` (background, like stems.separate; cached → `{state:"cached"}`), `analysis.status {song_id}`, `analysis.get {song_id}`. `song.open` response gains `"analysis": {...}|null`.
- [ ] Junction derivation upgrade: when the song has downbeats, `section.replace`/`junctions.derive` compute per-pair windows = from the **last downbeat strictly before** the boundary to the **first downbeat strictly after** (clamped inside the sections); else the existing tail/head seconds. Add a focused practice-crate function `junction_window(downbeats, boundary) -> (f64, f64)` with tests (boundary exactly on a downbeat, between, before first, after last).
- [ ] Tests (`app_analysis.rs`, FakeAnalyzer): run→done event→cached; open returns analysis; junctions use downbeat windows when analysis present (assert exact bounds from fixture).
- [ ] Commit: `feat(server): analysis pipeline — beat grid cached, downbeat-aware junctions`

### Task 4: UI

**Files:** `Sections.svelte`, `Waveform.svelte`, `stores.ts`.

- [ ] Sections tab: "Analyze" Button (shows running state from `analysis_progress`; same UX as Separate stems). On done: suggested sections appear as **prefilled unsaved rows** (visually marked `suggested`, muted accent) — user edits/renames/deletes, then the normal save persists them (and junctions re-derive bar-aware server-side). If sections already exist, suggestions append below with a "replace with suggestions" Button.
- [ ] Waveform: when analysis exists, draw beat ticks (1 px, very muted) and downbeat ticks (slightly stronger) along the bottom edge — only at zoom levels where beat spacing ≥ 6 px (declutter). Loop create/drag/resize snaps to nearest downbeat when within 0.08 s × (1/rate)… keep simple: snap when within 10 px; `g` toggles grid-snap (default on when analysis exists), help footer updated.
- [ ] `pnpm build && pnpm vitest run` clean. Add a vitest for the pure snap function (`snapToGrid(sec, downbeats, view, thresholdPx)`).
- [ ] Commit: `feat(desktop): analyze action, suggested sections, beat grid + snap`

### Task 5: Live verification + gate

- [ ] Through a live earwormd (real engine + real Analyzer): import the Deftones mp3 (already in library on the real DB — use a temp DB and re-import), `analysis.run`, poll to cached, `song.open` shows analysis, `section.replace` with two suggested sections → junction loop bounds land exactly on downbeats from `analysis.get`. Report the actual numbers.
- [ ] Full gate: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run && pnpm build`. README: add Analyze to the feature list + `scripts/analyze` note.
- [ ] Commit: `feat(analysis): live-verified beat-aware pipeline`

---

## Self-review

- Suggestions-not-truth ✔ (unsaved rows, user gate). Beat grid always ships even if section model fails ✔ (wrapper fallback). Model swap = wrapper-only change ✔. Mirrors proven stems pattern (trait/job/event/cache) ✔. User conventions: extensionless script ✔.
