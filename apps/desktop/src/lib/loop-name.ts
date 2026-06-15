// Dynamic loop names derived from a song's sections — the TypeScript twin of
// crates/practice/src/naming.rs. The server names saved loops; this names the
// live working loop client-side so the two read identically (e.g. "verse 2",
// "verse 2 → chorus 1"). Keep the two in sync.

import type { Section } from "./stores";

/** Boundary tolerance (seconds): an edge within EPS of a section boundary counts
 *  as "on" it. Mirrors naming.rs. */
const EPS = 0.05;

/** `name occurrence` — 1-based count among same-named sections, in position
 *  order. The only `chorus` is `chorus 1`. */
function occurrenceLabel(section: Section, sections: Section[]): string {
  const ordered = [...sections].sort((a, b) => a.position - b.position);
  let n = 1;
  for (const s of ordered) {
    if (s.id === section.id) break;
    if (s.name === section.name) n++;
  }
  return `${section.name} ${n}`;
}

/** Sections the loop [start, end] overlaps, in position order. A section barely
 *  touched at a shared boundary (within EPS) does not count. */
function overlapping(start: number, end: number, sections: Section[]): Section[] {
  return sections
    .filter((s) => s.start < end - EPS && s.end > start + EPS)
    .sort((a, b) => a.position - b.position);
}

/** `m:ss.t` — mirrors naming::fmt_ts / server::app::fmt_ts. */
function fmtTs(secs: number): string {
  const tenths = Math.round(secs * 10);
  const s = Math.floor((tenths % 600) / 10);
  return `${Math.floor(tenths / 600)}:${String(s).padStart(2, "0")}.${tenths % 10}`;
}

/** `verse 2` when the loop fully covers this endpoint section, `sub verse 2`
 *  when it only partially covers it. */
function edgeLabel(section: Section, sections: Section[], full: boolean): string {
  const label = occurrenceLabel(section, sections);
  return full ? label : `sub ${label}`;
}

function baseName(start: number, end: number, sections: Section[]): string {
  const hit = overlapping(start, end, sections);
  if (hit.length === 0) return `riff ${fmtTs(start)}–${fmtTs(end)}`;
  if (hit.length === 1) {
    const s = hit[0];
    const full = start <= s.start + EPS && end >= s.end - EPS;
    return edgeLabel(s, sections, full);
  }
  const first = hit[0];
  const last = hit[hit.length - 1];
  const left = edgeLabel(first, sections, start <= first.start + EPS);
  const right = edgeLabel(last, sections, end >= last.end - EPS);
  return `${left} → ${right}`;
}

/** Append `(2)`, `(3)`, … until the name is unique among `existing`. */
function disambiguate(base: string, existing: string[]): string {
  if (!existing.includes(base)) return base;
  let n = 2;
  while (existing.includes(`${base} (${n})`)) n++;
  return `${base} (${n})`;
}

/** A loop's display name from its bounds and the song's sections — mirrors
 *  practice::naming::loop_name. `existing` disambiguates with a `(n)` suffix. */
export function deriveLoopName(
  start: number,
  end: number,
  sections: Section[],
  existing: string[] = [],
): string {
  return disambiguate(baseName(start, end, sections), existing);
}
