// Runtime telemetry for cracking the intermittent open/freeze bug.
//
// Opt-in: off unless launched with `EARWORM_DEBUG=1` (the Rust side reports the
// gate via the `debug_flag` command). When off, every entry point here is a
// cheap no-op — no timers, listeners or IPC.
//
// When on: the webview is WebKitGTK — its devtools aren't reachable from
// outside the window, so every trace is ALSO forwarded to the Rust side
// (`ui_log`), which prints to stderr and thus lands in
// `~/.local/share/earworm/earworm.log` alongside the backend's own dispatch/
// panic traces. That gives one unified, retrievable timeline of "what the UI
// did last" even after a hard freeze or a WebKit/Wayland crash.
//
// Low-noise by design: routine work is silent; only lifecycle milestones,
// slow/stuck commands, main-thread stalls and errors are emitted.

import { invoke } from "@tauri-apps/api/core";

let enabled = false;
/** Whether telemetry is active — callers gate their own setup on this. */
export function isTracing(): boolean {
  return enabled;
}

const t0 = performance.now();
const ms = () => Math.round(performance.now() - t0);

function forward(line: string): void {
  // fire-and-forget; telemetry must never throw or block
  invoke("ui_log", { line }).catch(() => {});
}

export function trace(scope: string, msg: string): void {
  if (!enabled) return;
  const line = `[ui:${scope} +${ms()}ms] ${msg}`;
  console.info(line);
  forward(line);
}

export function traceErr(scope: string, msg: string): void {
  if (!enabled) return;
  const line = `[ui:${scope} +${ms()}ms] !! ${msg}`;
  console.error(line);
  forward(line);
}

// A timer that should fire every PERIOD ms; a much larger gap means the main
// thread was blocked (a synchronous freeze) between ticks. When it crosses the
// threshold we log the gap on the FIRST tick that gets to run again — so even a
// recovered freeze leaves a fingerprint. A permanent freeze stops the timer
// entirely; the last forwarded trace before silence then brackets it.
function installStallDetector(): void {
  let last = performance.now();
  const PERIOD = 2000;
  setInterval(() => {
    const now = performance.now();
    const gap = now - last;
    last = now;
    if (gap > PERIOD * 2.5) traceErr("stall", `main thread blocked ~${Math.round(gap)}ms`);
  }, PERIOD);
}

export async function initTrace(): Promise<void> {
  try {
    enabled = await invoke<boolean>("debug_flag");
  } catch {
    enabled = false;
  }
  if (!enabled) return;
  trace("boot", "trace online (EARWORM_DEBUG)");
  installStallDetector();
  window.addEventListener("error", (e) =>
    traceErr("window", `error: ${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`));
  window.addEventListener("unhandledrejection", (e) =>
    traceErr("window", `unhandledrejection: ${(e.reason && e.reason.message) || e.reason}`));
}
