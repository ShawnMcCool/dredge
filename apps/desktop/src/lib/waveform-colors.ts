// Section-label palette for the waveform's structure lane. Hues are *derived
// from the current theme accent*: each section type sits at a fixed hue offset
// from the accent's base hue, all at one quiet saturation/lightness, so the lane
// reads as a single muted family that re-tints when the accent changes. Chorus
// rides the accent itself; everything else fans ±70° around it. Labels outside
// the SongFormer set (novelty's A/B/C…) hash into the same band deterministically.

export interface LabelColor {
  fill: string;
  edge: string;
}

const FILL_ALPHA = 0.16;
const EDGE_ALPHA = 0.5; // softened from the old bright outline
const SAT = 30; // one muted saturation across the lane
const LIGHT = 58;
const SILENCE_SAT = 8; // near-grey: structurally "nothing here"

/** Hue offset (degrees) from the accent base hue, per known label. */
const OFFSET: Record<string, number> = {
  chorus: 0, // the hook rides the accent itself
  "pre-chorus": 14,
  verse: 30,
  bridge: 58,
  intro: -28,
  outro: -48,
  inst: -66,
  silence: 0, // hue = base, but rendered near-grey (see SILENCE_SAT)
};

export const KNOWN_LABELS = Object.keys(OFFSET);

/** Deterministic fallback offset for labels outside the fixed set (±70° band). */
function hashHue(label: string): number {
  let h = 0;
  for (let i = 0; i < label.length; i++) h = (h * 31 + label.charCodeAt(i)) % 360;
  return h;
}

/** Hue (0–360) of a `#rrggbb` accent; falls back to amber's hue on bad input. */
export function hexToHue(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 36;
  const n = m[1];
  const r = parseInt(n.slice(0, 2), 16) / 255;
  const g = parseInt(n.slice(2, 4), 16) / 255;
  const b = parseInt(n.slice(4, 6), 16) / 255;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const d = mx - mn;
  if (d === 0) return 36;
  let h: number;
  if (mx === r) h = ((g - b) / d) % 6;
  else if (mx === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h *= 60;
  return (h + 360) % 360;
}

export function labelColor(label: string, baseHue: number): LabelColor {
  const key = label.trim().toLowerCase();
  const known = key in OFFSET;
  const offset = known ? OFFSET[key] : (hashHue(key) % 140) - 70;
  const hue = ((baseHue + offset) % 360 + 360) % 360;
  const sat = key === "silence" ? SILENCE_SAT : SAT;
  return {
    fill: `hsla(${hue}, ${sat}%, ${LIGHT}%, ${FILL_ALPHA})`,
    edge: `hsla(${hue}, ${sat}%, ${LIGHT}%, ${EDGE_ALPHA})`,
  };
}
