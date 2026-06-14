// Accent colour theme. Pure DOM (a `data-accent` attribute on <html> that
// app.css keys off), no Tauri. The durable `color_theme` setting is
// authoritative; default is amber.

import { get } from "svelte/store";
import { COLOR_THEME, settings } from "./stores";

export type Accent = "amber" | "cyan";

export function applyTheme(theme: Accent): void {
  // amber is the base (no override); only cyan needs the attribute
  document.documentElement.setAttribute("data-accent", theme === "cyan" ? "cyan" : "amber");
}

/** Call after `loadSettings()` — restore the saved accent. */
export function initTheme(): void {
  applyTheme(get(settings)[COLOR_THEME] === "cyan" ? "cyan" : "amber");
}
