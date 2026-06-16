// All UI state derives from dispatch responses + events — no second source
// of truth. Stores mirror the wire shapes of `server::app::App`.

import { derived, get, writable } from "svelte/store";
import { cmd, initialSong, onEvent } from "./ipc";
import { trace } from "./trace";
import { subdivisionTimes, type GridSubdivision } from "./waveform-math";
import { bisect, nudgeEdge, rateForRep, runUp, type Span } from "./drill";
import { deriveLoopName } from "./loop-name";

// --- wire types ----------------------------------------------------------

export interface Song {
  id: number;
  title: string;
  artist: string | null;
  path: string;
  file_hash: string;
  duration_secs: number;
}

export interface Section {
  id: number;
  song_id: number;
  name: string;
  start: number;
  end: number;
  position: number;
}

export type LoopKind =
  | { kind: "manual" }
  | { kind: "junction"; from_section: number; to_section: number };

export interface LoopRegion {
  id: number;
  song_id: number;
  name: string;
  /** Manual name pinned by the user; null when `name` is algorithm-derived. */
  name_override: string | null;
  start: number;
  end: number;
  kind: LoopKind;
}

export type TempoCurve =
  | { curve: "dwell"; rate: number }
  | { curve: "ladder"; start: number; step: number; target: number }
  | { curve: "oscillate"; low: number; high: number; period: number };

export interface Peaks {
  frames_per_bucket: number;
  buckets: [number, number][];
}

/** One suggested section from analysis — not user truth until saved. */
export interface AnalysisSection {
  label: string;
  start: number;
  end: number;
}

/** Cached `scripts/analyze` output for a song (times in seconds). */
export interface Analysis {
  bpm: number | null;
  beats: number[];
  downbeats: number[];
  sections: AnalysisSection[];
  engine: string;
}

export interface ProfileStage {
  name: string;
  ms: number;
  note?: string;
}

export interface ProfileRun {
  op: string;
  song_id?: number;
  started_at: string;
  total_ms: number;
  ok: boolean;
  error?: string;
  device?: string;
  engine?: string;
  stages: ProfileStage[];
  max_cpu_pct?: number;
  max_gpu_util?: number;
  max_vram_used_mb?: number;
  vram_total_mb?: number;
}

export interface WorkSample {
  op: string;
  stage: string;
  elapsed_ms: number;
  cpu_pct: number;
  gpu_util?: number;
  gpu_mem_used_mb?: number;
  gpu_mem_total_mb?: number;
  ram_used_mb?: number;
  ram_total_mb?: number;
}

export interface OpenSong {
  song: Song;
  sections: Section[];
  loops: LoopRegion[];
  peaks: Peaks;
  /** True when the engine was loaded with the song's 4 cached stems. */
  stems: boolean;
  analysis: Analysis | null;
}

/** Fixed stem order contract: vocals/drums/bass/other. */
export const STEM_LABELS = ["VOCALS", "DRUMS", "BASS", "OTHER"] as const;
export const BASS_STEM = 2;

export interface StemMix {
  levels: number[]; // 0..100 per stem
  mutes: boolean[];
  solos: boolean[];
}

function defaultStemMix(): StemMix {
  return {
    levels: [100, 100, 100, 100],
    mutes: [false, false, false, false],
    solos: [false, false, false, false],
  };
}

export interface Position {
  secs: number;
  rate: number;
  playing: boolean;
  /** performance.now() at receipt — playhead extrapolation anchor. */
  at: number;
}

/** An audio input device (mic / interface) for the tuner. */
export interface CaptureNode {
  id: number;
  /** object.serial — what the engine targets; registry ids don't link on modern PipeWire. */
  serial: number;
  app: string;
  /** media.name — often the song title currently playing. */
  media: string;
}

export interface TunerReading {
  hz: number;
  /** McLeod clarity 0..1; < 0.5 means no steady pitch. */
  confidence: number;
}

// --- stores ---------------------------------------------------------------

export const songs = writable<Song[]>([]);
export const openSong = writable<OpenSong | null>(null);
/** Song id with a `song.open` in flight — drives the library row spinner and
 *  the stage loading state; null once the open settles (also on error). */
export const openingSong = writable<number | null>(null);
export const position = writable<Position>({ secs: 0, rate: 1, playing: false, at: 0 });
export const selection = writable<{ start: number; end: number } | null>(null);
/** Loop currently driving the transport (clicked or plan-applied). */
export const currentLoop = writable<LoopRegion | null>(null);
/** A live, unsaved loop: drag a selection, hit "loop", and it plays + drills
 *  exactly like a saved loop — but nothing is persisted until you save it. At
 *  most one exists; a fresh "loop" silently replaces it, clicking away dismisses
 *  it. Stays null while a saved loop is active (the two are mutually exclusive). */
export const workingLoop = writable<Span | null>(null);
/** The loop currently driving the stage — a working loop if one is up, else the
 *  selected saved loop — normalized so the drill box, waveform and transport all
 *  read one shape. `id === null` marks a working (unsaved) loop. The working
 *  loop's name is derived from the song's sections exactly as the server names
 *  saved loops (e.g. "verse 2", "verse 2 → chorus 1"), so it reads like any
 *  other loop — there's no user-facing "working loop" concept. */
export const activeLoop = derived(
  [workingLoop, currentLoop, openSong],
  ([$working, $current, $open]): { id: number | null; start: number; end: number; name: string } | null =>
    $working
      ? {
          id: null,
          start: $working.start,
          end: $working.end,
          name: deriveLoopName($working.start, $working.end, $open?.sections ?? []),
        }
      : $current
        ? { id: $current.id, start: $current.start, end: $current.end, name: $current.name }
        : null,
);
/** Ephemeral scratch loop bounds for the drill box — what actually plays while
 *  a loop is active. Mirrors the active loop's bounds, then the drill region
 *  toys (nudge / isolate / run-up) edit *this* only; the saved LoopRegion is
 *  never touched. Null when no loop is active. */
export const drillSpan = writable<Span | null>(null);
/** The bounds the scratch span resets to — the "home" of the current drill,
 *  seeded whenever a loop is engaged (a saved loop, or a transient
 *  selection-loop). Null when no loop is active; the drill box shows iff this
 *  (and drillSpan) is non-null. */
export const drillHome = writable<Span | null>(null);
// The seeding/teardown is centralized in `actions.seedDrill`, driven by the
// active loop (working or saved): engaging a loop seeds it, clearing/reset tears
// it down. The activeLoop lifecycle hook is set up after `actions` is defined
// (see "drill box lifecycle" near the bottom).

/** Step-up tempo trainer for the drill box: a ramp recipe (a `TempoCurve`) that
 *  autopilots the *global* playback rate across loop cycles. No second tempo —
 *  it animates `position.rate`. `cycle` is the 0-based loop-wrap count since
 *  arming. The recipe persists across loops; arming resets the cycle. */
export interface DrillTrainer {
  recipe: TempoCurve;
  armed: boolean;
  cycle: number;
}
export const drillTrainer = writable<DrillTrainer>({
  recipe: { curve: "ladder", start: 0.7, step: 0.05, target: 1.0 },
  armed: false,
  cycle: 0,
});

/** Mute-to-recall for the drill box: play a pass "from memory" by muting the
 *  recording (the engine keeps the position advancing, so the loop stays in
 *  time). `armNext` silences the next pass; `everyN` silences every Nth. Recall
 *  drives the engine mute only while active, and restores audio when cleared. */
export const drillRecall = writable<{ everyN: number | null; armNext: boolean }>({
  everyN: null,
  armNext: false,
});

/** Which drill tools the user has opted into for the active loop. The box is
 *  minimal by default — a fresh loop just plays; tools are added on demand and
 *  reset (back to minimal) by seedDrill whenever the loop changes. */
export const drillShow = writable({ trainer: false, recall: false, region: false });
/** Bumped by `resetWorkspace()` — the waveform watches this to refit zoom and
 *  drop its local view/active-span state (which no store mirrors). */
export const workspaceReset = writable(0);
export const pitch = writable({ semitones: 0, cents: 0, octaveUp: false });
/** Bass focus on/off — low-pass + octave-up transcription trick. */
export const bassFocus = writable(false);
export const muted = writable(false);
/** User playback volume 0..1.5 — engine multiplier, persisted as a setting. */
export const playbackVolume = writable(1.0);
/** Input devices (mics / interfaces) for the tuner; CaptureNode shape. */
export const tunerInputs = writable<CaptureNode[]>([]);
/** Latest pitch reading while the tuner is on; null when off. */
export const tunerReading = writable<TunerReading | null>(null);
/** Whether the tuner box is powered on (listening). */
export const tunerOn = writable(false);
/** Sticky chosen input device name; restored from settings at launch. */
export const tunerInputName = writable<string | null>(null);
/** Mixer state for the open song's stems (sliders × mute × solo). */
export const stemMix = writable<StemMix>(defaultStemMix());
export const stemsError = writable<string | null>(null);
export const analysisError = writable<string | null>(null);
/** Loop edges snap to downbeats while on (only meaningful with analysis). */
export const gridSnap = writable(true);
/** Grid display (persisted): show/hide the drawn grid, full lines vs bottom
 *  ticks, and the subdivision used for both the grid and snapping. */
export const gridVisible = writable(true);
export const gridLines = writable(false);
export const gridSubdivision = writable<GridSubdivision>("bar");
/** Show every saved loop on the waveform (persisted). Off by default — the
 *  waveform draws only the active loop; this brings the rest back as a dim
 *  overlay. The full list is always available in the loops tab regardless. */
export const allLoopsVisible = writable(false);
/** Recent profiling runs, most-recent-first. Mirrors `profile_run` events
 *  plus a `profiles.list` fetch at launch. */
export const profiles = writable<ProfileRun[]>([]);
/** Latest live work sample while a prepare run is active; null when idle. */
export const workSample = writable<WorkSample | null>(null);
/** VRAM series for the active run: used-MB samples (rolling last 60), the run's
 *  peak used-MB (high-water mark), and total VRAM. Null when idle / no GPU. */
export const vram = writable<{ used: number[]; peak: number; min: number; total: number } | null>(null);

// --- durable settings -------------------------------------------------------

/** Known keys in the server-side `settings` table. */
export const UI_SCALE = "ui_scale";
export const GRID_SNAP_DEFAULT = "grid_snap_default";
export const PLAYBACK_VOLUME = "playback_volume";
export const ANALYSIS_DEVICE = "analysis_device";
export const LIBRARY_COLLAPSED = "library_collapsed";
export const PANELS_COLLAPSED = "panels_collapsed";
export const GRID_VISIBLE = "grid_visible";
export const GRID_LINES = "grid_lines";
export const GRID_SUBDIV = "grid_subdivision";
export const ALL_LOOPS_VISIBLE = "all_loops_visible";
/** Native window frame (title bar + min/max/close). Default on. */
export const WINDOW_DECORATIONS = "window_decorations";
/** Accent colour theme: "amber" (default) or "cyan". */
export const COLOR_THEME = "color_theme";
export const TUNER_INPUT_NAME = "tuner_input_name";
/** Export tab: last-used target folder + format, restored across sessions. */
export const EXPORT_DIR = "export_dir";
export const EXPORT_FORMAT = "export_format";

/** Side-column collapse state — persisted to settings, restored at launch. */
export const libraryCollapsed = writable(false);
export const panelsCollapsed = writable(false);

/** Local mirror of the settings table; `loadSettings` fills it at launch and
 *  `setSetting` writes through. */
export const settings = writable<Record<string, unknown>>({});
/** Settings modal visibility (gear button or `,`). */
export const settingsOpen = writable(false);
/** Request: jump to the sections tab (e.g. from the structure summary). */
export const sectionsOpen = writable(false);
/** Request: jump to the loops tab (e.g. after saving a loop). */
export const loopsOpen = writable(false);

// --- prepare flow -----------------------------------------------------------

export type PrepareStepState = "pending" | "running" | "done" | "failed" | "cached";

export interface PrepareState {
  open: boolean;
  song_id: number;
  steps: { analysis: PrepareStepState; stems: PrepareStepState };
  errors: { analysis?: string; stems?: string };
}

/** Progress-modal state machine for `prepare()`; null when idle. */
export const prepareState = writable<PrepareState | null>(null);

type PrepareStep = "analysis" | "stems";
type ProgressReport = { song_id: number; state: string; error?: string };

/** `prepare()` awaits these; the matching `*_progress` event resolves one and
 *  is then handled by prepare's own refresh instead of the default branch. */
const prepareWaiters: Partial<Record<PrepareStep, (r: ProgressReport) => void>> = {};
let prepareSongId: number | null = null;

function setPrepareStep(step: PrepareStep, state: PrepareStepState, error?: string): void {
  prepareState.update((s) =>
    s
      ? {
          ...s,
          steps: { ...s.steps, [step]: state },
          errors: error ? { ...s.errors, [step]: error } : s.errors,
        }
      : s,
  );
}

/** A `*_progress` event for the song being prepared — claims the waiter. */
function takePrepareWaiter(
  step: PrepareStep,
  data: ProgressReport,
): ((r: ProgressReport) => void) | null {
  const waiter = prepareWaiters[step];
  if (!waiter || data.song_id !== prepareSongId) return null;
  delete prepareWaiters[step];
  return waiter;
}

/** Drill-box recall bookkeeping: loop-wrap counter and whether the *recall*
 *  feature (not the user's speaker toggle) currently owns the engine mute. */
let drillPass = 0;
let recallMuted = false;

/** Debounce handle for the volume fader's settings write-through. */
let volumeSaveTimer: ReturnType<typeof setTimeout> | undefined;

/** The active loop's bounds the drill was last seeded from — the seeder (set up
 *  near the bottom) compares against this so a save-promotion or an in-place
 *  resize of the *same* loop doesn't tear down the drill. */
let lastDrillBounds: string | null = null;

function loopName(id: number): string {
  return get(openSong)?.loops.find((l) => l.id === id)?.name ?? `loop ${id}`;
}

export { loopName };

// --- actions ----------------------------------------------------------------

export const actions = {
  // --- settings ---

  /** Pull the durable settings once at launch and apply the ones that act
   *  as session defaults (grid snap, playback volume). */
  async loadSettings(): Promise<void> {
    const all = await cmd<Record<string, unknown>>("settings.get_all");
    settings.set(all);
    if (typeof all[GRID_SNAP_DEFAULT] === "boolean") gridSnap.set(all[GRID_SNAP_DEFAULT]);
    if (typeof all[LIBRARY_COLLAPSED] === "boolean") libraryCollapsed.set(all[LIBRARY_COLLAPSED]);
    if (typeof all[PANELS_COLLAPSED] === "boolean") panelsCollapsed.set(all[PANELS_COLLAPSED]);
    if (typeof all[GRID_VISIBLE] === "boolean") gridVisible.set(all[GRID_VISIBLE]);
    if (typeof all[GRID_LINES] === "boolean") gridLines.set(all[GRID_LINES]);
    if (all[GRID_SUBDIV] === "bar" || all[GRID_SUBDIV] === "beat" || all[GRID_SUBDIV] === "eighth")
      gridSubdivision.set(all[GRID_SUBDIV]);
    if (typeof all[ALL_LOOPS_VISIBLE] === "boolean") allLoopsVisible.set(all[ALL_LOOPS_VISIBLE]);
    const vol = typeof all[PLAYBACK_VOLUME] === "number" ? all[PLAYBACK_VOLUME] : 1.0;
    playbackVolume.set(vol);
    await cmd("volume", { value: vol });
    if (typeof all[TUNER_INPUT_NAME] === "string") tunerInputName.set(all[TUNER_INPUT_NAME]);
    void actions.loadProfiles();
  },

  /** Pull recent profiling runs (most-recent-first) at launch. */
  async loadProfiles(): Promise<void> {
    profiles.set(await cmd<ProfileRun[]>("profiles.list", { limit: 50 }));
  },

  /** Prepend a freshly finished run (from a `profile_run` event). */
  recordProfile(run: ProfileRun): void {
    profiles.update((list) => [run, ...list].slice(0, 100));
  },

  /** Store the latest live work sample (from a `work_sample` event). */
  recordWorkSample(sample: WorkSample): void {
    workSample.set(sample);
    if (sample.gpu_mem_used_mb != null && sample.gpu_mem_total_mb != null) {
      const used = sample.gpu_mem_used_mb;
      const total = sample.gpu_mem_total_mb;
      vram.update((v) => ({
        used: [...(v?.used ?? []), used].slice(-60),
        peak: Math.max(v?.peak ?? 0, used),
        min: v?.min != null ? Math.min(v.min, used) : used,
        total,
      }));
    }
  },

  /** Write-through: update the local mirror, persist server-side. */
  async setSetting(key: string, value: unknown): Promise<void> {
    settings.update((s) => ({ ...s, [key]: value }));
    await cmd("settings.set", { key, value });
  },

  async toggleLibrary(): Promise<void> {
    const v = !get(libraryCollapsed);
    libraryCollapsed.set(v);
    await this.setSetting(LIBRARY_COLLAPSED, v);
  },

  async togglePanels(): Promise<void> {
    const v = !get(panelsCollapsed);
    panelsCollapsed.set(v);
    await this.setSetting(PANELS_COLLAPSED, v);
  },

  async setGridSnap(on: boolean): Promise<void> {
    gridSnap.set(on);
    await this.setSetting(GRID_SNAP_DEFAULT, on);
  },
  async setGridVisible(on: boolean): Promise<void> {
    gridVisible.set(on);
    await this.setSetting(GRID_VISIBLE, on);
  },
  async setGridLines(on: boolean): Promise<void> {
    gridLines.set(on);
    await this.setSetting(GRID_LINES, on);
  },
  async setGridSubdivision(sub: GridSubdivision): Promise<void> {
    gridSubdivision.set(sub);
    await this.setSetting(GRID_SUBDIV, sub);
  },
  async setAllLoopsVisible(on: boolean): Promise<void> {
    allLoopsVisible.set(on);
    await this.setSetting(ALL_LOOPS_VISIBLE, on);
  },

  async refreshSongs(): Promise<void> {
    songs.set(await cmd<Song[]>("song.list"));
  },

  async importSong(path: string): Promise<Song> {
    const song = await cmd<Song>("song.import", { path });
    await this.refreshSongs();
    await this.openSong(song.id);
    return song;
  },

  async deleteSong(id: number): Promise<void> {
    await cmd("song.delete", { song_id: id });
    if (get(openSong)?.song.id === id) openSong.set(null);
    await this.refreshSongs();
  },

  async updateSong(id: number, title: string, artist: string | null): Promise<void> {
    const song = await cmd<Song>("song.update", { song_id: id, title, artist });
    openSong.update((o) => (o && o.song.id === id ? { ...o, song } : o));
    await this.refreshSongs();
  },

  async reanalyze(): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    // the analysis_progress event handler reloads the open song's analysis
    await cmd("analysis.run", { song_id: open.song.id, force: true });
  },

  async openSong(id: number): Promise<void> {
    // Phase tracing: a stuck spinner means this flow never reached `finally`.
    // Each milestone is forwarded to earworm.log, so the LAST line logged tells
    // us exactly which step the open froze on (network/backend, or the reactive
    // waveform render after `openSong.set`).
    trace("open", `#${id} begin`);
    openingSong.set(id);
    try {
      const data = await cmd<OpenSong>("song.open", { song_id: id });
      trace("open", `#${id} song.open ok — ${data.peaks?.buckets?.length ?? "?"} buckets`);
      localStorage.setItem(LAST_SONG_KEY, String(id));
      openSong.set(data);
      trace("open", `#${id} openSong store set (waveform render scheduled) — open complete`);
      selection.set(null);
      currentLoop.set(null);
      workingLoop.set(null);
      stemMix.set(defaultStemMix());
      stemsError.set(null);
      analysisError.set(null);
    } finally {
      openingSong.set(null);
      trace("open", `#${id} spinner cleared`);
    }
  },

  async refreshLoops(): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    const loops = await cmd<LoopRegion[]>("loop.list", { song_id: open.song.id });
    openSong.update((o) => (o ? { ...o, loops } : o));
  },

  // --- transport ---

  play: () => cmd("play"),
  pause: () => cmd("pause"),
  seek: (secs: number) => cmd("seek", { secs }),

  async setRate(value: number): Promise<void> {
    const v = Math.min(2.0, Math.max(0.25, value));
    await cmd("rate", { value: v });
    position.update((p) => ({ ...p, rate: v }));
  },

  async setPitch(semitones: number, cents: number): Promise<void> {
    const octaveUp = get(pitch).octaveUp;
    pitch.set({ semitones, cents, octaveUp });
    await cmd("pitch", { semitones, cents, octave_up: octaveUp });
  },

  /** Bass focus: low-pass + the octave-up transcription trick (so the
   *  bassline reads clearly an octave up). Off clears both. */
  async bassFocus(on: boolean): Promise<void> {
    bassFocus.set(on);
    const p = get(pitch);
    pitch.set({ ...p, octaveUp: on });
    await cmd("bass_focus", { on });
    await cmd("pitch", { semitones: p.semitones, cents: p.cents, octave_up: on });
  },

  async mute(on: boolean): Promise<void> {
    muted.set(on);
    await cmd("mute", { on });
  },

  /** Live engine volume on every change; the setting write is debounced so
   *  a fader drag lands as one row, not hundreds. */
  async setVolume(value: number): Promise<void> {
    const v = Math.min(1.5, Math.max(0, value));
    playbackVolume.set(v);
    clearTimeout(volumeSaveTimer);
    volumeSaveTimer = setTimeout(() => {
      void this.setSetting(PLAYBACK_VOLUME, v);
    }, 300);
    await cmd("volume", { value: v });
  },

  /** Point the transport at a span without persisting anything (a dumb
   *  primitive used by restart/play; it must NOT touch drill state). */
  async setTransportLoop(start: number, end: number): Promise<void> {
    await cmd("loop.set", { start, end });
  },

  async selectLoop(l: LoopRegion): Promise<void> {
    // set the saved loop, then drop any working loop — they're mutually
    // exclusive and currentLoop is what drives the drill once working is gone.
    currentLoop.set(l);
    workingLoop.set(null);
    await cmd("loop.set", { start: l.start, end: l.end });
  },

  /** The "loop" gesture: spin up a *working* loop over a span — it plays and
   *  drills exactly like a saved loop, but persists nothing until saved. Clears
   *  any selected saved loop and silently replaces a prior working loop. */
  async loopSpan(start: number, end: number): Promise<void> {
    currentLoop.set(null);
    workingLoop.set({ start, end });
    await cmd("loop.set", { start, end });
    await this.seek(start);
    await this.play();
  },

  /** Persist the working loop as a real LoopRegion (adopting an existing loop
   *  with matching bounds rather than duplicating it). Promotes it to the active
   *  saved loop *without* disturbing the live drill: the bounds don't change, so
   *  the bounds-keyed seeder won't reseed and an armed trainer / recall survive.
   *  Set the saved loop FIRST, then clear the working one, so `activeLoop`'s
   *  bounds never momentarily go null between the two writes. */
  async saveWorkingLoop(): Promise<void> {
    const w = get(workingLoop);
    if (!w) return;
    const open = get(openSong);
    const existing = open?.loops.find(
      (l) => Math.abs(l.start - w.start) < 0.01 && Math.abs(l.end - w.end) < 0.01,
    );
    const l = existing ?? (await this.createLoop(w.start, w.end));
    currentLoop.set(l);
    workingLoop.set(null);
  },

  /** Resize the working loop in place (right-drag on the waveform). Moves its
   *  bounds and follows the drill home/scratch to them WITHOUT a teardown — the
   *  trainer and any opened tools survive, mirroring a saved-loop resize. Save
   *  persists these bounds. Pre-setting `lastDrillBounds` makes the seeder treat
   *  this as the same engagement rather than a fresh loop. */
  async setWorkingLoopBounds(start: number, end: number): Promise<void> {
    if (!get(workingLoop)) return;
    lastDrillBounds = `${start},${end}`;
    drillHome.set({ start, end });
    drillSpan.set({ start, end });
    workingLoop.set({ start, end });
    await cmd("loop.set", { start, end });
  },

  async clearTransportLoop(): Promise<void> {
    workingLoop.set(null);
    currentLoop.set(null);
    await cmd("loop.clear");
  },

  // --- drill box: live edits to the scratch span (saved loops untouched) ---

  /** (Re)seed the drill from an engaged loop's bounds, or tear it down (null).
   *  Sets both the home and the live scratch span, and resets live drill state
   *  (disarm trainer, zero cycle, clear recall) so nothing leaks across loops. */
  seedDrill(span: Span | null): void {
    drillHome.set(span);
    drillSpan.set(span);
    drillTrainer.update((t) => ({ ...t, armed: false, cycle: 0 }));
    drillShow.set({ trainer: false, recall: false, region: false });
    void this.clearRecall();
  },

  /** Reveal a drill tool (opt-in; doesn't change playback by itself). */
  showDrillTool(tool: "trainer" | "recall" | "region"): void {
    drillShow.update((s) => ({ ...s, [tool]: true }));
  },

  /** Hide a drill tool and undo its effect, so the loop plays normally again. */
  async hideDrillTool(tool: "trainer" | "recall" | "region"): Promise<void> {
    drillShow.update((s) => ({ ...s, [tool]: false }));
    if (tool === "trainer") {
      this.disarmTrainer();
      await this.resetRate();
    } else if (tool === "recall") {
      await this.clearRecall();
    } else {
      await this.drillResetSpan();
    }
  },

  /** Set the scratch span and point the transport at it. */
  async applyDrillSpan(span: Span): Promise<void> {
    drillSpan.set(span);
    await cmd("loop.set", { start: span.start, end: span.end });
  },

  /** Grid lines for the current subdivision, and the downbeats, for snapping. */
  drillGrid(): { grid: number[]; downbeats: number[] } {
    const a = get(openSong)?.analysis;
    if (!a) return { grid: [], downbeats: [] };
    return { grid: subdivisionTimes(a.beats, a.downbeats, get(gridSubdivision)), downbeats: a.downbeats };
  },

  drillDuration(): number {
    return get(openSong)?.song.duration_secs ?? get(drillSpan)?.end ?? 0;
  },

  /** Move one scratch edge by a grid step (or 0.25 s without a grid). */
  async drillNudge(edge: "start" | "end", dir: 1 | -1): Promise<void> {
    const span = get(drillSpan);
    if (!span) return;
    await this.applyDrillSpan(nudgeEdge(span, edge, dir, this.drillGrid().grid, this.drillDuration(), 0.25));
  },

  /** Shrink the scratch span to its first or second half. */
  async drillIsolate(half: "first" | "second"): Promise<void> {
    const span = get(drillSpan);
    if (!span) return;
    await this.applyDrillSpan(bisect(span, half, this.drillGrid().grid));
  },

  /** Extend (+) or retract (−) the scratch start by N bars to drill the entrance. */
  async drillRunUp(deltaBars: number): Promise<void> {
    const span = get(drillSpan);
    if (!span) return;
    await this.applyDrillSpan(runUp(span, deltaBars, this.drillGrid().downbeats, this.drillDuration()));
  },

  /** Snap the scratch span back to the drill's home bounds. */
  async drillResetSpan(): Promise<void> {
    const home = get(drillHome);
    if (home) await this.applyDrillSpan({ ...home });
  },

  // --- drill box: step-up tempo trainer ---

  /** Arm the trainer: reset the cycle and apply the recipe's rep-0 rate now;
   *  each subsequent loop wrap advances it (see the `loop_wrapped` handler). */
  async armTrainer(): Promise<void> {
    drillTrainer.update((t) => ({ ...t, armed: true, cycle: 0 }));
    await this.setRate(rateForRep(get(drillTrainer).recipe, 0));
  },

  disarmTrainer(): void {
    drillTrainer.update((t) => ({ ...t, armed: false }));
  },

  /** Toggle the trainer — the drill box's primary verb (the `d` key). */
  async toggleTrainer(): Promise<void> {
    if (get(drillTrainer).armed) this.disarmTrainer();
    else await this.armTrainer();
  },

  /** Edit the ramp recipe; re-apply at the current cycle if already armed. */
  async setTrainerRecipe(recipe: TempoCurve): Promise<void> {
    drillTrainer.update((t) => ({ ...t, recipe }));
    const t = get(drillTrainer);
    if (t.armed) await this.setRate(rateForRep(recipe, t.cycle));
  },

  /** Return the global rate to 100% (the trainer leaves it where it landed). */
  async resetRate(): Promise<void> {
    await this.setRate(1.0);
  },

  // --- drill box: mute-to-recall ---

  /** Silence the next loop pass (play it from memory). */
  armRecallNext(): void {
    drillRecall.update((r) => ({ ...r, armNext: true }));
  },

  /** Silence every Nth pass (null disables). Resets the pass counter. */
  async setRecallEveryN(n: number | null): Promise<void> {
    drillPass = 0;
    drillRecall.update((r) => ({ ...r, everyN: n }));
    if (n === null) await this.maybeUnmuteRecall();
  },

  /** Fully clear recall and hand the mute back (used on teardown). */
  async clearRecall(): Promise<void> {
    drillRecall.set({ everyN: null, armNext: false });
    drillPass = 0;
    await this.maybeUnmuteRecall();
  },

  /** Restore audio iff recall (not the user) was holding the mute. */
  async maybeUnmuteRecall(): Promise<void> {
    if (recallMuted) {
      recallMuted = false;
      await this.mute(false);
    }
  },

  /** Reset the stage to a clean slate: stop playback, refit the waveform zoom,
   *  drop the selection, the clicked active span, the active loop, return the
   *  playhead to the start, and restore speed to 100% and pitch to 0 — without
   *  touching volume. The zoom + active span live as local state in Waveform,
   *  so we signal it via workspaceReset. */
  async resetWorkspace(): Promise<void> {
    selection.set(null);
    if (get(position).playing) await this.pause();
    await this.clearTransportLoop();
    await this.seek(0);
    await this.setRate(1.0);
    await this.setPitch(0, 0);
    workspaceReset.update((n) => n + 1);
  },

  // --- annotations ---

  /** Persist a loop for the current span; the server names it dynamically. */
  async createLoop(start: number, end: number): Promise<LoopRegion> {
    const open = get(openSong);
    if (!open) throw new Error("no song open");
    const l = await cmd<LoopRegion>("loop.create", {
      song_id: open.song.id,
      start,
      end,
    });
    await this.refreshLoops();
    return l;
  },

  /** Snap a loop's edges to the nearest section boundaries (renames it). */
  async fitLoop(loopId: number): Promise<void> {
    await cmd("loop.fit", { loop_id: loopId });
    await this.refreshLoops();
  },

  async updateLoop(
    loopId: number,
    fields: { name?: string; start?: number; end?: number },
  ): Promise<void> {
    await cmd("loop.update", { loop_id: loopId, ...fields });
    await this.refreshLoops();
  },

  async deleteLoop(loopId: number): Promise<void> {
    await cmd("loop.delete", { loop_id: loopId });
    if (get(currentLoop)?.id === loopId) await this.clearTransportLoop();
    await this.refreshLoops();
  },

  /** Replace the whole section lane. */
  async replaceSections(
    sections: { name: string; start: number; end: number; position: number }[],
  ): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    const out = await cmd<{ sections: Section[] }>("section.replace", {
      song_id: open.song.id,
      sections,
    });
    openSong.update((o) => (o ? { ...o, sections: out.sections } : o));
    await this.refreshLoops();
  },

  // --- tuner ---

  async refreshTunerInputs(): Promise<void> {
    tunerInputs.set(await cmd<CaptureNode[]>("tuner.inputs"));
  },

  /** Power on: resolve the sticky device (or first available) and start. */
  async tunerPowerOn(): Promise<void> {
    await this.refreshTunerInputs();
    const inputs = get(tunerInputs);
    if (inputs.length === 0) throw new Error("no audio input devices found");
    const savedName = get(tunerInputName);
    const node = inputs.find((n) => n.app === savedName) ?? inputs[0];
    tunerInputName.set(node.app);
    await cmd("tuner.start", { node_id: node.id });
    tunerOn.set(true);
  },

  async tunerPowerOff(): Promise<void> {
    await cmd("tuner.stop");
    tunerOn.set(false);
    tunerReading.set(null);
  },

  /** Pick a specific input; persist it and restart capture if already on. */
  async setTunerInput(node: CaptureNode): Promise<void> {
    tunerInputName.set(node.app);
    await cmd("settings.set", { key: TUNER_INPUT_NAME, value: node.app });
    if (get(tunerOn)) {
      await cmd("tuner.start", { node_id: node.id });
    }
  },

  // --- stems ---

  /** The 4-vector sent to the engine: sliders × mute × solo. */
  stemGainsVector(mix: StemMix): number[] {
    const anySolo = mix.solos.some(Boolean);
    return mix.levels.map((level, i) =>
      mix.mutes[i] || (anySolo && !mix.solos[i]) ? 0 : level / 100,
    );
  },

  async applyStemMix(): Promise<void> {
    if (!get(openSong)?.stems) return;
    await cmd("stems.gains", { gains: this.stemGainsVector(get(stemMix)) });
  },

  async setStemLevel(idx: number, level: number): Promise<void> {
    stemMix.update((m) => ({ ...m, levels: m.levels.map((v, i) => (i === idx ? level : v)) }));
    await this.applyStemMix();
  },

  async toggleStemMute(idx: number): Promise<void> {
    stemMix.update((m) => ({ ...m, mutes: m.mutes.map((v, i) => (i === idx ? !v : v)) }));
    await this.applyStemMix();
  },

  async toggleStemSolo(idx: number): Promise<void> {
    stemMix.update((m) => ({ ...m, solos: m.solos.map((v, i) => (i === idx ? !v : v)) }));
    await this.applyStemMix();
  },

  /** Restore all faders to 100% and clear every mute/solo, in one engine call. */
  async resetStemMix(): Promise<void> {
    stemMix.set(defaultStemMix());
    await this.applyStemMix();
  },

  // --- prepare (analysis → stems) ---

  /** One button: structure/beat analysis, then stem separation —
   *  sequentially, never in parallel (both are GPU-heavy; SongFormer alone
   *  peaks ~8 GiB of VRAM). The modal mirrors prepareState; each step is
   *  resolved by its `*_progress` event or a `cached` short-circuit, and a
   *  failure never blocks the other step. */
  async prepare(force = false): Promise<void> {
    const open = get(openSong);
    if (!open || get(prepareState)) return;
    const id = open.song.id;
    prepareSongId = id;
    prepareState.set({
      open: true,
      song_id: id,
      steps: { analysis: "pending", stems: "pending" },
      errors: {},
    });
    workSample.set(null);
    vram.set(null);

    const run = async (
      step: "analysis" | "stems",
      command: string,
      extra: Record<string, unknown> = {},
    ): Promise<void> => {
      setPrepareStep(step, "running");
      try {
        // register before dispatch — the terminal event must not slip past
        const report = new Promise<{ state: string; error?: string }>((resolve) => {
          prepareWaiters[step] = resolve;
        });
        const out = await cmd<{ state: string }>(command, { song_id: id, ...extra });
        if (out.state === "cached") {
          delete prepareWaiters[step];
          setPrepareStep(step, "cached");
          return;
        }
        const r = await report;
        if (r.state === "done") setPrepareStep(step, "done");
        else setPrepareStep(step, "failed", r.error ?? `${step} failed`);
      } catch (e) {
        // shown verbatim: install/setup hints ride on the error message
        delete prepareWaiters[step];
        setPrepareStep(step, "failed", e instanceof Error ? e.message : String(e));
      }
    };

    // RE-PREPARE forces a fresh analysis (new SongFormer sections + a profile);
    // stems are expensive, so they re-run only when not yet cached.
    await run("analysis", "analysis.run", force ? { force: true } : {});
    await run("stems", "stems.separate");
    prepareSongId = null;

    // refresh exactly as the scattered flows did: re-open auto-loads cached
    // stems + analysis, loadAnalysis surfaces the section suggestions
    if (get(openSong)?.song.id === id) {
      await this.openSong(id);
      const s = get(prepareState);
      if (s && (s.steps.analysis === "done" || s.steps.analysis === "cached")) {
        await this.loadAnalysis(id);
      }
    }
    const s = get(prepareState);
    const ok = (st: PrepareStepState) => st === "done" || st === "cached";
    if (s && ok(s.steps.analysis) && ok(s.steps.stems)) {
      // all green: linger just long enough to read the two ✓s
      setTimeout(() => { prepareState.set(null); workSample.set(null); vram.set(null); }, 1500);
    }
    // failures leave the modal open with its close button
  },

  closePrepare(): void {
    prepareState.set(null);
    workSample.set(null);
    vram.set(null);
  },

  // --- analysis ---

  /** Pull the cached analysis into the open song and surface suggestions. */
  async loadAnalysis(songId: number): Promise<void> {
    const analysis = await cmd<Analysis | null>("analysis.get", { song_id: songId });
    if (get(openSong)?.song.id !== songId) return;
    openSong.update((o) => (o ? { ...o, analysis } : o));
  },
};

// --- drill box lifecycle ----------------------------------------------------
// Whenever the ACTIVE loop's bounds change — a different loop engaged (working
// or saved), or all loops cleared (incl. via resetWorkspace / song open) —
// reseed the scratch span and tear down live drill state so nothing leaks
// across loops: disarm the trainer, zero its cycle, and clear recall (which
// hands the engine mute back if recall held it).
//
// Keyed on bounds, NOT identity: promoting a working loop to a saved one keeps
// the same bounds (it only gains an id + name), so the seeder skips it and an
// armed trainer / recall survive the save. `lastDrillBounds` is hoisted up top
// so `setWorkingLoopBounds` can pre-seed it for the same reason on resize.
activeLoop.subscribe((al) => {
  const key = al ? `${al.start},${al.end}` : null;
  if (key === lastDrillBounds) return;
  lastDrillBounds = key;
  actions.seedDrill(al ? { start: al.start, end: al.end } : null);
});

// --- launch restore ---------------------------------------------------------

const LAST_SONG_KEY = "earworm-last-song";

/** Pick up where the last session left off (`EARWORM_OPEN` wins when set);
 *  the song may be gone — start empty rather than surfacing an error. */
async function openLastSong(): Promise<void> {
  const forced = await initialSong().catch(() => null);
  const stored = Number(localStorage.getItem(LAST_SONG_KEY));
  const id = forced ?? (Number.isInteger(stored) && stored > 0 ? stored : null);
  if (id == null) return;
  try {
    await actions.openSong(id);
  } catch {
    // song may be gone — start empty
  }
}

// --- events ----------------------------------------------------------------

export async function initEvents(): Promise<() => void> {
  void openLastSong();
  return onEvent((ev) => {
    switch (ev.event) {
      case "position":
        position.set({ ...(ev.data as Omit<Position, "at">), at: performance.now() });
        break;
      case "loop_wrapped": {
        // drill trainer: advance the cycle and let the global rate follow the
        // recipe.
        const t = get(drillTrainer);
        if (t.armed) {
          const cycle = t.cycle + 1;
          drillTrainer.set({ ...t, cycle });
          void actions.setRate(rateForRep(t.recipe, cycle));
        }
        // recall: decide whether the pass that just began plays from memory.
        const r = get(drillRecall);
        if (r.everyN != null || r.armNext) {
          drillPass += 1;
          const silent = r.armNext || (r.everyN != null && drillPass % r.everyN === 0);
          if (silent !== recallMuted) {
            recallMuted = silent;
            void actions.mute(silent);
          }
          if (r.armNext) drillRecall.set({ ...r, armNext: false });
        }
        break;
      }
      case "stems_progress": {
        const data = ev.data as { song_id: number; state: string; error?: string };
        const waiter = takePrepareWaiter("stems", data);
        if (waiter) {
          // prepare() owns the modal update and the end-of-flow refresh
          waiter(data);
          break;
        }
        if (data.state === "done") {
          // re-opening the song auto-loads the freshly cached stems
          if (get(openSong)?.song.id === data.song_id) void actions.openSong(data.song_id);
        } else if (data.state === "failed") {
          stemsError.set(data.error ?? "stem separation failed");
        }
        break;
      }
      case "analysis_progress": {
        const data = ev.data as {
          song_id: number;
          state: string;
          error?: string;
          sections?: Section[];
        };
        // analysis now auto-commits its sections server-side: apply the saved
        // layout and refresh loops (section changes may have pruned some) so the
        // structure is live without a manual save. Runs for the prepare flow too.
        if (data.state === "done" && get(openSong)?.song.id === data.song_id) {
          if (data.sections) {
            openSong.update((o) => (o ? { ...o, sections: data.sections! } : o));
          }
          void actions.loadAnalysis(data.song_id);
          void actions.refreshLoops();
        } else if (data.state === "failed") {
          analysisError.set(data.error ?? "analysis failed");
        }
        const waiter = takePrepareWaiter("analysis", data);
        if (waiter) waiter(data);
        break;
      }
      case "work_sample":
        actions.recordWorkSample(ev.data as WorkSample);
        break;
      case "tuner_pitch":
        tunerReading.set(ev.data as TunerReading);
        break;
      case "profile_run":
        actions.recordProfile(ev.data as ProfileRun);
        break;
      case "library_changed":
        // socket-driven imports land in the sidebar
        void actions.refreshSongs();
        break;
    }
  });
}
