// Resolves which section's notes the box shows. Hybrid rule: a pinned label
// wins (set by clicking a section or focusing the editor); otherwise follow the
// playhead. A pin that no longer matches any section falls through to the
// playhead, so a rename never strands the box.

export interface SectionSpan {
  label: string;
  start: number;
  end: number;
}

export function activeLabel(
  sections: SectionSpan[],
  playheadSecs: number,
  pinned: string | null,
): string | null {
  if (sections.length === 0) return null;
  if (pinned && sections.some((s) => s.label === pinned)) return pinned;
  // The engine reports a frame-quantized playhead, so a position held on a
  // section boundary (a loop start during count-in) can sit a fraction of a
  // frame below it. Bias the lookup forward by EPS so a boundary reads as the
  // section it begins — otherwise the held count-in shows the previous
  // section's notes.
  const EPS = 0.001;
  const hit = sections.find((s) => playheadSecs >= s.start - EPS && playheadSecs < s.end - EPS);
  return (hit ?? sections[0]).label;
}
