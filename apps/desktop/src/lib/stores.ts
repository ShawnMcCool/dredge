// All UI state derives from dispatch responses + events — no second source
// of truth. Stores mirror the wire shapes of `server::app::App`.

import { get, writable } from "svelte/store";
import { cmd, onEvent } from "./ipc";

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

export interface OpenSong {
  song: Song;
  sections: Section[];
  loops: LoopRegion[];
  plans: Plan[];
  peaks: Peaks;
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
export const position = writable<Position>({ secs: 0, rate: 1, playing: false, at: 0 });
export const planStatus = writable<PlanStatus | null>(null);
export const selection = writable<{ start: number; end: number } | null>(null);
/** Loop currently driving the transport (clicked or plan-applied). */
export const currentLoop = writable<LoopRegion | null>(null);
export const pitch = writable({ semitones: 0, cents: 0, octaveUp: false });
export const bassFocusOn = writable(false);
export const muted = writable(false);
export const due = writable<DueItem[]>([]);
export const retention = writable<RetentionRow[]>([]);
/** Rating prompts queued by step_finished — drained by the runner UI. */
export const pendingRatings = writable<PendingRating[]>([]);
/** Set when plan_finished fires; cleared when the summary is dismissed. */
export const sessionSummary = writable<SessionSummary | null>(null);
export const captureNodes = writable<CaptureNode[]>([]);
export const captureStatus = writable<CaptureStatus>({ running: false });

/** Loops that due.list contained when the plan started: their first
 *  end-of-step rating is the retention probe (`is_retest: true`). */
let dueAtPlanStart = new Set<number>();
let repsThisPlan = 0;
let stepsThisPlan = 0;

function loopName(id: number): string {
  return get(openSong)?.loops.find((l) => l.id === id)?.name ?? `loop ${id}`;
}

export { loopName };

// --- actions ----------------------------------------------------------------

export const actions = {
  async refreshSongs(): Promise<void> {
    songs.set(await cmd<Song[]>("song.list"));
  },

  async importSong(path: string): Promise<Song> {
    const song = await cmd<Song>("song.import", { path });
    await this.refreshSongs();
    await this.openSong(song.id);
    return song;
  },

  async openSong(id: number): Promise<void> {
    const data = await cmd<OpenSong>("song.open", { song_id: id });
    openSong.set(data);
    selection.set(null);
    currentLoop.set(null);
    await this.refreshRetention();
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
  },

  async skipStep(): Promise<void> {
    const spec = await cmd<RepStatus | null>("plan.skip_step");
    const prev = get(planStatus);
    if (spec && prev) planStatus.set({ ...spec, plan_id: prev.plan_id });
    else planStatus.set(null);
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
    await cmd("capture.start", { node_id: nodeId });
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

  async refreshRetention(): Promise<void> {
    const open = get(openSong);
    if (!open) {
      retention.set([]);
      return;
    }
    retention.set(await cmd<RetentionRow[]>("retention", { song_id: open.song.id }));
  },
};

// --- events ----------------------------------------------------------------

export async function initEvents(): Promise<() => void> {
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
        sessionSummary.set({ reps: repsThisPlan, steps: stepsThisPlan });
        void actions.refreshDue();
        void actions.refreshRetention();
        break;
    }
  });
}
