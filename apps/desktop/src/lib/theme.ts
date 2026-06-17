// Accent colour theme. Pure DOM, no Tauri. The durable `color_theme` setting is
// authoritative; default is amber. A value is either a named preset ("amber" /
// "cyan") — which app.css keys off via a `data-accent` attribute — or a custom
// "#rrggbb" hex, which we apply by writing the accent CSS vars inline on <html>.

import { get } from "svelte/store";
import { COLOR_THEME, settings } from "./stores";

export type Accent = "amber" | "cyan";

/** Preset accent hexes — mirror the swatches in app.css. */
export const PRESET_HEX: Record<Accent, string> = {
  amber: "#e0a458",
  cyan: "#4fc3d4",
};

export interface AccentOption {
  /** Display name + tooltip. */
  name: string;
  /** Stored `color_theme` value — a preset name ("amber"/"cyan") or a hex. */
  value: string;
  /** The swatch colour shown in the picker. */
  hex: string;
}

// Curated accent palette: one tone (≈hsl 64% S, 59% L — the mean of amber &
// cyan), hues spread around the wheel but skipping the muddy yellow-green zone.
// amber & cyan keep their exact shipped hexes and their named-preset values so
// their hand-tuned --accent-dim/--shaky (in app.css) stay pixel-identical.
export const ACCENTS: AccentOption[] = [
  { name: "coral", value: "#da6e53", hex: "#da6e53" },
  { name: "amber", value: "amber", hex: PRESET_HEX.amber },
  { name: "green", value: "#53da96", hex: "#53da96" },
  { name: "teal", value: "#53dacc", hex: "#53dacc" },
  { name: "cyan", value: "cyan", hex: PRESET_HEX.cyan },
  { name: "azure", value: "#5396da", hex: "#5396da" },
  { name: "violet", value: "#8d53da", hex: "#8d53da" },
  { name: "rose", value: "#da538d", hex: "#da538d" },
];

const ACCENT_VARS = ["--accent", "--accent-dim", "--shaky"] as const;

/** Is this theme value a custom hex (vs a named preset)? */
export function isCustom(theme: string): boolean {
  return theme.startsWith("#");
}

/** Resolve any stored theme value to the accent hex it shows. */
export function accentHex(theme: string): string {
  if (isCustom(theme)) return theme;
  return PRESET_HEX[theme === "cyan" ? "cyan" : "amber"];
}

/** Fired on <window> after the accent vars change. DOM styled with var(--accent)
 *  updates live, but canvas painters (the waveform) read the vars at draw time
 *  and must repaint — they listen for this. */
export const THEME_EVENT = "earworm:theme";

export function applyTheme(theme: string): void {
  const root = document.documentElement;
  if (isCustom(theme)) {
    // custom accent: base palette is amber, then override the accent vars inline
    root.setAttribute("data-accent", "amber");
    root.style.setProperty("--accent", theme);
    root.style.setProperty("--accent-dim", dim(theme));
    root.style.setProperty("--shaky", theme);
  } else {
    // preset: drop any custom inline vars so app.css's rules win again
    for (const v of ACCENT_VARS) root.style.removeProperty(v);
    root.setAttribute("data-accent", theme === "cyan" ? "cyan" : "amber");
  }
  window.dispatchEvent(new Event(THEME_EVENT));
}

/** Call after `loadSettings()` — restore the saved accent. */
export function initTheme(): void {
  const saved = get(settings)[COLOR_THEME];
  applyTheme(typeof saved === "string" ? saved : "amber");
}

/** A dimmed companion shade for a hex accent (scaled toward black, ~the same
 *  relationship the amber/cyan presets have between --accent and --accent-dim). */
function dim(hex: string): string {
  const f = 0.64;
  const n = hex.slice(1);
  const ch = (i: number) =>
    Math.round(parseInt(n.slice(i, i + 2), 16) * f)
      .toString(16)
      .padStart(2, "0");
  return `#${ch(0)}${ch(2)}${ch(4)}`;
}
