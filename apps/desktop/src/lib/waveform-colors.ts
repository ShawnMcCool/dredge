// Section-label palette for the waveform's structure lane. Muted hues on the
// dark canvas: translucent fill, stronger edge of the same hue.
//
// The fixed map covers SongFormer's 8-class inference set ("SongForm-HX-8Class"
// in the model snapshot: intro/verse/chorus/bridge/inst/outro/silence/
// pre-chorus). Chorus rides the amber accent family (--accent ≈ hue 36);
// everything else gets a desaturated distinct hue. Labels outside the set
// (novelty's A/B/C…, future model variants) hash to a deterministic hue.

export interface LabelColor {
  fill: string;
  edge: string;
}

const FILL_ALPHA = 0.16;
const EDGE_ALPHA = 0.8;

/** [hue, saturation%, lightness%] per known label. */
const KNOWN: Record<string, [number, number, number]> = {
  intro: [200, 35, 58],
  verse: [150, 30, 55],
  chorus: [36, 65, 58], // the amber accent family
  bridge: [275, 30, 62],
  inst: [320, 30, 58],
  outro: [222, 28, 62],
  silence: [220, 8, 48], // near-grey: structurally "nothing here"
  "pre-chorus": [58, 38, 52],
};

export const KNOWN_LABELS = Object.keys(KNOWN);

/** Deterministic fallback hue for labels outside the fixed map. */
function hashHue(label: string): number {
  let h = 0;
  for (let i = 0; i < label.length; i++) h = (h * 31 + label.charCodeAt(i)) % 360;
  return h;
}

export function labelColor(label: string): LabelColor {
  const key = label.trim().toLowerCase();
  const [h, s, l] = KNOWN[key] ?? [hashHue(key), 32, 56];
  return {
    fill: `hsla(${h}, ${s}%, ${l}%, ${FILL_ALPHA})`,
    edge: `hsla(${h}, ${s}%, ${l}%, ${EDGE_ALPHA})`,
  };
}
