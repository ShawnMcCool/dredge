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
  const hit = sections.find((s) => playheadSecs >= s.start && playheadSecs < s.end);
  return (hit ?? sections[0]).label;
}
