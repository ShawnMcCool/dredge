// The note document for a section: an ordered list of text and tab blocks.
// Pure data + transforms — no Svelte, no IO. The Svelte views call these and
// hand the result back to the autosave path. Mirrors `practice::notes`.

export interface TextBlock {
  kind: "text";
  text: string;
}

export interface TabBlock {
  kind: "tab";
  /** Row count = string count; the BOTTOM row is the lowest string. */
  strings: number;
  /** Column count; each cell is one char, "-" when empty. */
  width: number;
  /** Exactly `strings` rows, each exactly `width` chars. */
  rows: string[];
}

export type Block = TextBlock | TabBlock;

export interface NotesDoc {
  blocks: Block[];
}

export interface Cursor {
  row: number;
  col: number;
}

export const TAB_MIN_STRINGS = 1;
export const TAB_MAX_STRINGS = 12;
export const TAB_MIN_WIDTH = 1;
export const TAB_MAX_WIDTH = 256;

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(n)));

export function emptyTab(strings: number, width: number): TabBlock {
  const s = clamp(strings, TAB_MIN_STRINGS, TAB_MAX_STRINGS);
  const w = clamp(width, TAB_MIN_WIDTH, TAB_MAX_WIDTH);
  return { kind: "tab", strings: s, width: w, rows: Array.from({ length: s }, () => "-".repeat(w)) };
}

/** Resize string count. Growth prepends blank rows at the TOP (new higher
 *  strings); shrink drops rows from the top, keeping the bottom `n`. */
export function setStrings(t: TabBlock, n: number): TabBlock {
  const target = clamp(n, TAB_MIN_STRINGS, TAB_MAX_STRINGS);
  if (target === t.strings) return t;
  let rows: string[];
  if (target > t.rows.length) {
    const add = target - t.rows.length;
    rows = [...Array.from({ length: add }, () => "-".repeat(t.width)), ...t.rows];
  } else {
    rows = t.rows.slice(t.rows.length - target);
  }
  return { ...t, strings: target, rows };
}

/** Resize width on the RIGHT only. Growth appends dashes (content kept); shrink
 *  truncates from the right (content past the new width is erased). */
export function setWidth(t: TabBlock, n: number): TabBlock {
  const target = clamp(n, TAB_MIN_WIDTH, TAB_MAX_WIDTH);
  if (target === t.width) return t;
  const rows = t.rows.map((r) =>
    r.length < target ? r + "-".repeat(target - r.length) : r.slice(0, target),
  );
  return { ...t, width: target, rows };
}

/** Overtype one cell; width unchanged. `ch` uses its first char, or "-". */
export function setCell(t: TabBlock, row: number, col: number, ch: string): TabBlock {
  if (row < 0 || row >= t.strings || col < 0 || col >= t.width) return t;
  const c = (ch || "-").slice(0, 1);
  const r = t.rows[row];
  const next = r.slice(0, col) + c + r.slice(col + 1);
  const rows = t.rows.slice();
  rows[row] = next;
  return { ...t, rows };
}

export function clearCell(t: TabBlock, row: number, col: number): TabBlock {
  return setCell(t, row, col, "-");
}

export function moveCursor(t: TabBlock, cur: Cursor, dir: "up" | "down" | "left" | "right"): Cursor {
  let { row, col } = cur;
  if (dir === "up") row -= 1;
  if (dir === "down") row += 1;
  if (dir === "left") col -= 1;
  if (dir === "right") col += 1;
  return {
    row: clamp(row, 0, t.strings - 1),
    col: clamp(col, 0, t.width - 1),
  };
}
