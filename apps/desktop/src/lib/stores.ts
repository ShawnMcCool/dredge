// All UI state derives from dispatch responses + events — no second source
// of truth. Stores mirror the wire shapes of `server::app::App`.

import { get, writable } from "svelte/store";
import { cmd, initialSong, onEvent } from "./ipc";

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
  start: number;
  end: number;
  kind: LoopKind;
}

export type TempoCurve =
  | { curve: "dwell"; rate: number }
  | { curve: "ladder"; start: number; step: number; target: number }
  | { curve: "oscillate"; low: number; high: number; period: number };

export type PlanStep =
  | { step: "listen_first"; loop_id: number; reps: number }
  | { step: "play_reps"; loop_id: number; reps: number; curve: TempoCurve }
  | {
      step: "rotation";
      loop_ids: number[];
      rounds: number;
      reps_per_visit: number;
      curve: TempoCurve;
    }
  | { step: "recall_test"; loop_id: number; alternations: number; rate: number };

export interface Plan {
  id: number;
  song_id: number;
  name: string;
  steps: PlanStep[];
}

export interface Peaks {
  frames_per_bucket: number;
  buckets: [number, number][];
}

export type Rating = "miss" | "shaky" | "solid";
export type RepModeWire = "listen" | "play" | "recall_silent";

/** RepSpec as serialized by the plan runner. */
export interface RepStatus {
  loop_id: number;
  rate: number;
  mode: RepModeWire;
  step_idx: number;
  rep_idx: number;
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

export interface OpenSong {
  song: Song;
  sections: Section[];
  loops: LoopRegion[];
  plans: Plan[];
  peaks: Peaks;
  /** True when the engine was loaded with the song's 4 cached stems. */
  stems: boolean;
  analysis: Analysis | null;
}

/** Fixed stem order contract: vocals/drums/bass/other. */
export const STEM_LABELS = ["VOC", "DRM", "BASS", "OTH"] as const;
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

export interface PlanStatus extends RepStatus {
  plan_id: number;
}

export interface DueItem {
  loop_id: number;
  name: string;
  song_id: number;
}

export interface RetentionRow {
  loop_id: number;
  rating: Rating;
  at: string;
}

export interface PendingRating {
  loop_id: number;
  is_retest: boolean;
}

export interface SessionSummary {
  reps: number;
  steps: number;
}

export interface CaptureNode {
  id: number;
  /** object.serial — what the engine targets; registry ids don't link on modern PipeWire. */
  serial: number;
  app: string;
  /** media.name — often the song title currently playing. */
  media: string;
}

export interface CaptureStatus {
  running: boolean;
  filled_secs?: number;
  app?: string;
  media?: string;
}

// --- stores ---------------------------------------------------------------

export const songs = writable<Song[]>([]);
export const openSong = writable<OpenSong | null>(null);
/** Song id with a `song.open` in flight — drives the library row spinner and
 *  the stage loading state; null once the open settles (also on error). */
export const openingSong = writable<number | null>(null);
export const position = writable<Position>({ secs: 0, rate: 1, playing: false, at: 0 });
export const planStatus = writable<PlanStatus | null>(null);
export const selection = writable<{ start: number; end: number } | null>(null);
/** Loop currently driving the transport (clicked or plan-applied). */
export const currentLoop = writable<LoopRegion | null>(null);
export const pitch = writable({ semitones: 0, cents: 0, octaveUp: false });
export const bassFocusOn = writable(false);
export const muted = writable(false);
/** User playback volume 0..1.5 — engine multiplier, persisted as a setting. */
export const playbackVolume = writable(1.0);
export const due = writable<DueItem[]>([]);
export const retention = writable<RetentionRow[]>([]);
/** Rating prompts queued by step_finished — drained by the runner UI. */
export const pendingRatings = writable<PendingRating[]>([]);
/** Set when plan_finished fires; cleared when the summary is dismissed. */
export const sessionSummary = writable<SessionSummary | null>(null);
/** An ephemeral quick session is running (select → p). */
export const quickActive = writable(false);
/** plan_finished fired during a quick session — show the keep/discard prompt. */
export const quickPromptVisible = writable(false);
/** Name of the loop a quick rating just saved — brief confirmation. */
export const quickSavedName = writable<string | null>(null);
/** Escape with nothing else to dismiss → "exit earworm?" confirm. */
export const exitPromptVisible = writable(false);
export const captureNodes = writable<CaptureNode[]>([]);
export const captureStatus = writable<CaptureStatus>({ running: false });
/** Mixer state for the open song's stems (sliders × mute × solo). */
export const stemMix = writable<StemMix>(defaultStemMix());
export const stemsError = writable<string | null>(null);
export const analysisError = writable<string | null>(null);
/** Fresh suggestions for the Sections lane; consumed (set null) once shown. */
export const suggestedSections = writable<AnalysisSection[] | null>(null);
/** Loop edges snap to downbeats while on (only meaningful with analysis). */
export const gridSnap = writable(true);

// --- durable settings -------------------------------------------------------

/** Known keys in the server-side `settings` table. */
export const UI_SCALE = "ui_scale";
export const GRID_SNAP_DEFAULT = "grid_snap_default";
export const CAPTURE_BUFFER_SECS = "capture_buffer_secs";
export const PLAYBACK_VOLUME = "playback_volume";

/** Local mirror of the settings table; `loadSettings` fills it at launch and
 *  `setSetting` writes through. */
export const settings = writable<Record<string, unknown>>({});
/** Settings modal visibility (gear button or `,`). */
export const settingsOpen = writable(false);

// --- prepare flow -----------------------------------------------------------

export type PrepareStepState = "pending" | "running" | "done" | "failed" | "cached";

export interface PrepareState {
  open: boolean;
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

/** Loops that due.list contained when the plan started: their first
 *  end-of-step rating is the retention probe (`is_retest: true`). */
let dueAtPlanStart = new Set<number>();
let repsThisPlan = 0;
let stepsThisPlan = 0;

/** Debounce handle for the volume fader's settings write-through. */
let volumeSaveTimer: ReturnType<typeof setTimeout> | undefined;

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
    const vol = typeof all[PLAYBACK_VOLUME] === "number" ? all[PLAYBACK_VOLUME] : 1.0;
    playbackVolume.set(vol);
    await cmd("volume", { value: vol });
  },

  /** Write-through: update the local mirror, persist server-side. */
  async setSetting(key: string, value: unknown): Promise<void> {
    settings.update((s) => ({ ...s, [key]: value }));
    await cmd("settings.set", { key, value });
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
    openingSong.set(id);
    try {
      const data = await cmd<OpenSong>("song.open", { song_id: id });
      localStorage.setItem(LAST_SONG_KEY, String(id));
      openSong.set(data);
      selection.set(null);
      currentLoop.set(null);
      stemMix.set(defaultStemMix());
      stemsError.set(null);
      analysisError.set(null);
      suggestedSections.set(null);
      await this.refreshRetention();
    } finally {
      openingSong.set(null);
    }
  },

  async refreshLoops(): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    const loops = await cmd<LoopRegion[]>("loop.list", { song_id: open.song.id });
    openSong.update((o) => (o ? { ...o, loops } : o));
  },

  async refreshPlans(): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    const plans = await cmd<Plan[]>("plan.list", { song_id: open.song.id });
    openSong.update((o) => (o ? { ...o, plans } : o));
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

  /** One key: octave-up pitch + low-pass, the bass transcription trick. */
  async bassFocus(on: boolean): Promise<void> {
    bassFocusOn.set(on);
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

  /** Point the transport at a span without persisting anything. */
  async setTransportLoop(start: number, end: number): Promise<void> {
    await cmd("loop.set", { start, end });
  },

  async selectLoop(l: LoopRegion): Promise<void> {
    currentLoop.set(l);
    await cmd("loop.set", { start: l.start, end: l.end });
  },

  async clearTransportLoop(): Promise<void> {
    currentLoop.set(null);
    await cmd("loop.clear");
  },

  // --- annotations ---

  async createLoop(name: string, start: number, end: number): Promise<LoopRegion> {
    const open = get(openSong);
    if (!open) throw new Error("no song open");
    const l = await cmd<LoopRegion>("loop.create", {
      song_id: open.song.id,
      name,
      start,
      end,
    });
    await this.refreshLoops();
    return l;
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

  /** Replace the whole section lane; server re-derives junction loops. */
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

  async deriveJunctions(tail: number, head: number): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    await cmd("junctions.derive", { song_id: open.song.id, tail, head });
    await this.refreshLoops();
  },

  // --- plans ---

  async savePlan(name: string, steps: PlanStep[]): Promise<Plan> {
    const open = get(openSong);
    if (!open) throw new Error("no song open");
    const plan = await cmd<Plan>("plan.save", { song_id: open.song.id, name, steps });
    await this.refreshPlans();
    return plan;
  },

  async startPlan(planId: number): Promise<void> {
    // Retest rule: loops due *today at plan start* get their end-of-step
    // rating flagged is_retest — the retention probe.
    const dueNow = await cmd<DueItem[]>("due.list");
    dueAtPlanStart = new Set(dueNow.map((d) => d.loop_id));
    repsThisPlan = 0;
    stepsThisPlan = 0;
    pendingRatings.set([]);
    sessionSummary.set(null);
    const spec = await cmd<RepStatus>("plan.start", { plan_id: planId });
    planStatus.set({ ...spec, plan_id: planId });
  },

  async stopPlan(): Promise<void> {
    await cmd("plan.stop");
    planStatus.set(null);
    pendingRatings.set([]);
    quickActive.set(false);
    quickPromptVisible.set(false);
  },

  async skipStep(): Promise<void> {
    const spec = await cmd<RepStatus | null>("plan.skip_step");
    const prev = get(planStatus);
    if (spec && prev) planStatus.set({ ...spec, plan_id: prev.plan_id });
    else planStatus.set(null);
  },

  // --- ephemeral practice (select → p) ---

  /** Instant micro-session on a raw span: listen ×2 → 6 oscillating reps.
   *  Nothing persists unless quickRate is called at the end. */
  async quickPractice(start: number, end: number): Promise<void> {
    const spec = await cmd<RepStatus>("practice.quick", { start, end });
    quickActive.set(true);
    quickPromptVisible.set(false);
    pendingRatings.set([]);
    sessionSummary.set(null);
    selection.set(null);
    planStatus.set({ ...spec, plan_id: 0 });
  },

  /** Keep: the loop is auto-named and saved, the rated rep recorded, and
   *  the resurfacing scheduler picks it up. */
  async quickRate(rating: Rating): Promise<void> {
    const out = await cmd<{ loop: LoopRegion }>("practice.quick_rate", { rating });
    quickActive.set(false);
    quickPromptVisible.set(false);
    planStatus.set(null);
    quickSavedName.set(out.loop.name);
    setTimeout(() => quickSavedName.set(null), 2500);
    await this.refreshLoops();
    await this.refreshDue();
  },

  /** Discard leaves no trace. */
  async quickDiscard(): Promise<void> {
    await cmd("practice.quick_discard");
    quickActive.set(false);
    quickPromptVisible.set(false);
    planStatus.set(null);
  },

  /** Self-rating; consumes the retest flag for loops due at plan start. */
  async rate(loopId: number, rating: Rating, isRetest: boolean): Promise<void> {
    await cmd("rep.rate", { loop_id: loopId, rating, is_retest: isRetest });
    if (isRetest) dueAtPlanStart.delete(loopId);
    await this.refreshDue();
    await this.refreshRetention();
  },

  /** Answer the head of the rating queue (UI prompt or 1/2/3 keys). */
  async resolveRating(rating: Rating): Promise<void> {
    const q = get(pendingRatings);
    const head = q[0];
    if (!head) return;
    pendingRatings.set(q.slice(1));
    await this.rate(head.loop_id, rating, head.is_retest);
  },

  async refreshDue(): Promise<void> {
    due.set(await cmd<DueItem[]>("due.list"));
  },

  // --- capture ---

  async refreshCaptureNodes(): Promise<void> {
    captureNodes.set(await cmd<CaptureNode[]>("capture.nodes"));
  },

  async refreshCaptureStatus(): Promise<void> {
    captureStatus.set(await cmd<CaptureStatus>("capture.status"));
  },

  async startCapture(nodeId: number): Promise<void> {
    const buffer = Number(get(settings)[CAPTURE_BUFFER_SECS] ?? 180);
    await cmd("capture.start", { node_id: nodeId, buffer_secs: buffer });
    await this.refreshCaptureStatus();
  },

  async stopCapture(): Promise<void> {
    await cmd("capture.stop");
    captureStatus.set({ running: false });
  },

  /** Snapshot the last N seconds to a WAV, import it, and open it. */
  async grabCapture(lastSecs: number): Promise<Song> {
    const song = await cmd<Song>("capture.grab", { last_secs: lastSecs });
    await this.refreshSongs();
    await this.openSong(song.id);
    return song;
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

  // --- prepare (analysis → stems) ---

  /** One button: structure/beat analysis, then stem separation —
   *  sequentially, never in parallel (both are GPU-heavy; SongFormer alone
   *  peaks ~8 GiB of VRAM). The modal mirrors prepareState; each step is
   *  resolved by its `*_progress` event or a `cached` short-circuit, and a
   *  failure never blocks the other step. */
  async prepare(): Promise<void> {
    const open = get(openSong);
    if (!open || get(prepareState)) return;
    const id = open.song.id;
    prepareSongId = id;
    prepareState.set({
      open: true,
      steps: { analysis: "pending", stems: "pending" },
      errors: {},
    });

    const run = async (step: "analysis" | "stems", command: string): Promise<void> => {
      setPrepareStep(step, "running");
      try {
        // register before dispatch — the terminal event must not slip past
        const report = new Promise<{ state: string; error?: string }>((resolve) => {
          prepareWaiters[step] = resolve;
        });
        const out = await cmd<{ state: string }>(command, { song_id: id });
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

    await run("analysis", "analysis.run");
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
      setTimeout(() => prepareState.set(null), 1500);
    }
    // failures leave the modal open with its close button
  },

  closePrepare(): void {
    prepareState.set(null);
  },

  // --- analysis ---

  /** Pull the cached analysis into the open song and surface suggestions. */
  async loadAnalysis(songId: number): Promise<void> {
    const analysis = await cmd<Analysis | null>("analysis.get", { song_id: songId });
    if (get(openSong)?.song.id !== songId) return;
    openSong.update((o) => (o ? { ...o, analysis } : o));
    if (analysis) suggestedSections.set(analysis.sections);
  },

  async refreshRetention(): Promise<void> {
    const open = get(openSong);
    if (!open) {
      retention.set([]);
      return;
    }
    retention.set(await cmd<RetentionRow[]>("retention", { song_id: open.song.id }));
  },
};

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
      case "loop_wrapped":
        if (get(planStatus)) repsThisPlan += 1;
        break;
      case "rep_changed": {
        const prev = get(planStatus);
        if (prev) planStatus.set({ ...(ev.data as RepStatus), plan_id: prev.plan_id });
        break;
      }
      case "step_finished": {
        // quick sessions rate once at the end, never per step
        if (get(quickActive)) break;
        // Server reports step_idx only; the finished step's loop is the one
        // the runner was on before the following rep_changed lands.
        const prev = get(planStatus);
        stepsThisPlan += 1;
        if (prev) {
          const is_retest = dueAtPlanStart.has(prev.loop_id);
          pendingRatings.update((q) => [...q, { loop_id: prev.loop_id, is_retest }]);
        }
        break;
      }
      case "plan_finished":
        planStatus.set(null);
        if (get(quickActive)) {
          // keep/discard is the whole prompt — no session summary
          quickPromptVisible.set(true);
          break;
        }
        sessionSummary.set({ reps: repsThisPlan, steps: stepsThisPlan });
        void actions.refreshDue();
        void actions.refreshRetention();
        break;
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
        const data = ev.data as { song_id: number; state: string; error?: string };
        const waiter = takePrepareWaiter("analysis", data);
        if (waiter) {
          waiter(data);
          break;
        }
        if (data.state === "done") {
          if (get(openSong)?.song.id === data.song_id) void actions.loadAnalysis(data.song_id);
        } else if (data.state === "failed") {
          analysisError.set(data.error ?? "analysis failed");
        }
        break;
      }
      case "library_changed":
        // socket-driven imports (incl. capture.grab) land in the sidebar
        void actions.refreshSongs();
        break;
    }
  });
}
