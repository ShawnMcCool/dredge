// Recording nudge conversions. The engine works in source frames at 48 kHz;
// the UI shows milliseconds.
export const SAMPLE_RATE = 48_000;

export const msToFrames = (ms: number): number => Math.round((ms / 1000) * SAMPLE_RATE);

export const framesToMs = (frames: number): number => (frames / SAMPLE_RATE) * 1000;

// A layer occupies [anchor, anchor+len) in source frames; expose it as a
// seconds span for time-aligned lane rendering.
export const layerSpanSecs = (anchorFrame: number, lenFrames: number) => ({
  start: anchorFrame / SAMPLE_RATE,
  end: (anchorFrame + lenFrames) / SAMPLE_RATE,
});
