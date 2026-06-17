# Per-Section Notes with Inline Tablature — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each song section a notes document — free monospace text plus inline, resizable tablature widgets — viewed/edited in a dedicated stage box.

**Architecture:** Notes key on a section's human-meaningful occurrence label ("verse 2") from `naming.rs`, not on `SectionId` (which is unstable across structure saves). A note is a JSON document of ordered `text`/`tab` blocks, stored in a new `section_notes` table and serialized into the open-song payload. The stage box resolves the active section (hybrid: follow playhead unless pinned), edits blocks, and debounce-autosaves. All grid math and the active-section rule live in pure, unit-tested `lib/*.ts` modules; the Svelte components are thin views over them.

**Tech Stack:** Rust (rusqlite, serde_json) for `practice`/`server`; Svelte 5 + TypeScript + vitest for the desktop frontend.

Spec: `docs/superpowers/specs/2026-06-17-section-notes-design.md`.

---

## File Structure

**Rust**
- `crates/practice/src/notes.rs` *(new)* — `NotesDoc`, `Block`, validation. One responsibility: the note document model.
- `crates/practice/src/lib.rs` *(modify)* — `pub mod notes;` and re-exports.
- `crates/practice/src/naming.rs` *(modify)* — make `occurrence_label` public.
- `crates/practice/src/store.rs` *(modify)* — schema v9 + `section_notes` CRUD.
- `crates/server/src/app.rs` *(modify)* — `section.notes.set` command, `sections_payload` enrichment helper, wire it into `finish_open` and `section_replace`.

**Frontend**
- `apps/desktop/src/lib/notes-doc.ts` *(new)* — note/tab types + pure tab transforms. + `notes-doc.test.ts`.
- `apps/desktop/src/lib/active-section.ts` *(new)* — the active-label resolver. + `active-section.test.ts`.
- `apps/desktop/src/lib/stores.ts` *(modify)* — wire types + `setSectionNotes` action + orphan state.
- `apps/desktop/src/components/TabBlock.svelte` *(new)* — the tablature widget (grid, cell editor, drag handles).
- `apps/desktop/src/components/Notes.svelte` *(new)* — the notes box (block list, autosave, orphans, active section).
- `apps/desktop/src/App.svelte` *(modify)* — mount the notes box in the stage row.
- `apps/desktop/src/components/SettingsPanel.svelte` *(modify)* — default strings/width fields.

---

## Phase 1 — Note model & persistence (Rust)

### Task 1: Note document model

**Files:**
- Create: `crates/practice/src/notes.rs`
- Modify: `crates/practice/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/practice/src/notes.rs`:

```rust
//! The note document for a section: an ordered list of text and tab blocks.
//! Tab blocks are a fixed grid (`strings` rows × `width` cols, `-` for an empty
//! cell); the bottom row is the lowest string. Stored as serde_json.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    Tab {
        strings: usize,
        width: usize,
        rows: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotesDoc {
    pub blocks: Vec<Block>,
}

impl NotesDoc {
    /// True when there's nothing worth storing: no blocks, or only empty text.
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|b| match b {
            Block::Text { text } => text.trim().is_empty(),
            Block::Tab { .. } => false,
        })
    }

    /// Enforce the grid invariants on every tab block: `rows.len() == strings`
    /// and every row is exactly `width` chars.
    pub fn validate(&self) -> Result<(), String> {
        for b in &self.blocks {
            if let Block::Tab { strings, width, rows } = b {
                if rows.len() != *strings {
                    return Err(format!("tab: {} rows for {strings} strings", rows.len()));
                }
                if let Some(bad) = rows.iter().find(|r| r.chars().count() != *width) {
                    return Err(format!("tab: row {bad:?} is not {width} wide"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let doc = NotesDoc {
            blocks: vec![
                Block::Text { text: "intro riff".into() },
                Block::Tab { strings: 2, width: 4, rows: vec!["--5-".into(), "7---".into()] },
            ],
        };
        let s = serde_json::to_string(&doc).unwrap();
        let back: NotesDoc = serde_json::from_str(&s).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn empty_doc_is_empty() {
        assert!(NotesDoc::default().is_empty());
        assert!(NotesDoc { blocks: vec![Block::Text { text: "  \n".into() }] }.is_empty());
        assert!(!NotesDoc { blocks: vec![Block::Text { text: "x".into() }] }.is_empty());
    }

    #[test]
    fn validate_rejects_malformed_tab() {
        let bad = NotesDoc { blocks: vec![Block::Tab { strings: 2, width: 4, rows: vec!["--5-".into()] }] };
        assert!(bad.validate().is_err());
        let bad2 = NotesDoc { blocks: vec![Block::Tab { strings: 1, width: 4, rows: vec!["--".into()] }] };
        assert!(bad2.validate().is_err());
        let ok = NotesDoc { blocks: vec![Block::Tab { strings: 1, width: 4, rows: vec!["----".into()] }] };
        assert!(ok.validate().is_ok());
    }
}
```

Add to `crates/practice/src/lib.rs` (next to the other `pub mod` lines):

```rust
pub mod notes;
```

- [ ] **Step 2: Run the tests to verify they fail to compile/pass**

Run: `cargo test -p practice notes::`
Expected: compiles and PASSES (this is self-contained model code; if `lib.rs` re-exports a prelude, no change needed beyond `pub mod notes;`).

- [ ] **Step 3: Commit**

```bash
git add crates/practice/src/notes.rs crates/practice/src/lib.rs
git commit -m "feat(practice): NotesDoc/Block model for section notes"
```

### Task 2: Schema v9 + section_notes CRUD

**Files:**
- Modify: `crates/practice/src/store.rs`

- [ ] **Step 1: Add the schema constant**

After `SCHEMA_V8` (around `crates/practice/src/store.rs:98`):

```rust
/// v9: free-form per-section notes (text + tab blocks), keyed by the section's
/// occurrence label ("verse 2") rather than its unstable row id. `doc_json` is
/// a serialized `notes::NotesDoc`.
const SCHEMA_V9: &str = "
CREATE TABLE section_notes (
    song_id    INTEGER NOT NULL,
    label      TEXT    NOT NULL,
    doc_json   TEXT    NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (song_id, label)
);
";
```

- [ ] **Step 2: Add the migration step**

In `migrate()`, after the `version < 8` block (around `store.rs:199`):

```rust
        if version < 9 {
            self.conn.execute_batch(SCHEMA_V9)?;
            self.conn.pragma_update(None, "user_version", 9)?;
        }
```

- [ ] **Step 3: Write the failing CRUD test**

Add to the `tests` module at the bottom of `store.rs` (use the existing test helper that opens an in-memory/temp store — match the surrounding tests' setup, e.g. `Store::open_in_memory()` or the temp-file helper already used there):

```rust
    #[test]
    fn section_notes_upsert_get_delete() {
        use crate::notes::{Block, NotesDoc};
        let store = test_store(); // match the helper used by other store tests
        let song = store
            .insert_song(NewSong { title: "t", artist: None, path: "/p", file_hash: "h", duration_secs: 1.0 })
            .unwrap();

        // absent → None
        assert_eq!(store.get_section_notes(song.id, "verse 1").unwrap(), None);

        // upsert
        let doc = NotesDoc { blocks: vec![Block::Text { text: "hello".into() }] };
        store.set_section_notes(song.id, "verse 1", &doc).unwrap();
        assert_eq!(store.get_section_notes(song.id, "verse 1").unwrap(), Some(doc.clone()));

        // overwrite
        let doc2 = NotesDoc { blocks: vec![Block::Text { text: "world".into() }] };
        store.set_section_notes(song.id, "verse 1", &doc2).unwrap();
        assert_eq!(store.get_section_notes(song.id, "verse 1").unwrap(), Some(doc2));

        // empty doc deletes the row
        store.set_section_notes(song.id, "verse 1", &NotesDoc::default()).unwrap();
        assert_eq!(store.get_section_notes(song.id, "verse 1").unwrap(), None);

        // list returns (label, doc) pairs
        store.set_section_notes(song.id, "chorus 1", &doc).unwrap();
        let all = store.list_section_notes(song.id).unwrap();
        assert_eq!(all, vec![("chorus 1".to_string(), doc)]);
    }
```

> If other store tests use a different constructor/helper name (`Store::open(":memory:")`, a `tmp` helper, `insert_song` vs `add_song`), match theirs exactly — read the existing `tests` module first.

- [ ] **Step 4: Run it to verify it fails**

Run: `cargo test -p practice store::tests::section_notes_upsert_get_delete`
Expected: FAIL — `get_section_notes`/`set_section_notes`/`list_section_notes` not found.

- [ ] **Step 5: Implement the CRUD methods**

Add to the `impl Store` block (near `list_sections`, around `store.rs:328`):

```rust
    /// Upsert a section's notes by occurrence label. An empty doc deletes the row.
    pub fn set_section_notes(
        &self,
        song_id: SongId,
        label: &str,
        doc: &crate::notes::NotesDoc,
    ) -> Result<()> {
        if doc.is_empty() {
            self.conn.execute(
                "DELETE FROM section_notes WHERE song_id = ?1 AND label = ?2",
                params![song_id.0, label],
            )?;
            return Ok(());
        }
        let json = serde_json::to_string(doc)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO section_notes (song_id, label, doc_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(song_id, label) DO UPDATE SET doc_json = ?3, updated_at = ?4",
            params![song_id.0, label, json, now],
        )?;
        Ok(())
    }

    pub fn get_section_notes(
        &self,
        song_id: SongId,
        label: &str,
    ) -> Result<Option<crate::notes::NotesDoc>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT doc_json FROM section_notes WHERE song_id = ?1 AND label = ?2")?;
        let json: Option<String> = stmt
            .query_row(params![song_id.0, label], |r| r.get(0))
            .optional()?;
        Ok(match json {
            Some(j) => Some(serde_json::from_str(&j)?),
            None => None,
        })
    }

    /// All stored notes for a song as `(label, doc)`, label order.
    pub fn list_section_notes(
        &self,
        song_id: SongId,
    ) -> Result<Vec<(String, crate::notes::NotesDoc)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT label, doc_json FROM section_notes WHERE song_id = ?1 ORDER BY label",
        )?;
        let rows = stmt
            .query_map(params![song_id.0], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(label, json)| Ok((label, serde_json::from_str(&json)?)))
            .collect()
    }
```

> `.optional()` comes from `rusqlite::OptionalExtension`. If it isn't already imported in `store.rs`, add `use rusqlite::OptionalExtension;` at the top (check first — the file may already use it).

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p practice store::tests::section_notes_upsert_get_delete`
Expected: PASS

- [ ] **Step 7: Run the full practice crate**

Run: `cargo test -p practice`
Expected: PASS (existing migration tests still green at version 9).

- [ ] **Step 8: Commit**

```bash
git add crates/practice/src/store.rs
git commit -m "feat(practice): section_notes table (schema v9) + CRUD"
```

### Task 3: Make occurrence_label public

**Files:**
- Modify: `crates/practice/src/naming.rs`

- [ ] **Step 1: Expose the function**

In `crates/practice/src/naming.rs:16`, change:

```rust
fn occurrence_label(section: &Section, sections: &[Section]) -> String {
```

to:

```rust
/// Public so the server can key section notes on this same label.
pub fn occurrence_label(section: &Section, sections: &[Section]) -> String {
```

- [ ] **Step 2: Verify it still builds and tests pass**

Run: `cargo test -p practice naming::`
Expected: PASS (no behavior change).

- [ ] **Step 3: Commit**

```bash
git add crates/practice/src/naming.rs
git commit -m "refactor(practice): expose naming::occurrence_label"
```

---

## Phase 2 — Dispatcher wiring (Rust)

### Task 4: section.notes.set command + payload enrichment

**Files:**
- Modify: `crates/server/src/app.rs`

- [ ] **Step 1: Add the enrichment helper**

Add this method to `impl App` (near `finish_open`, around `app.rs:1260`). Ensure `use std::collections::HashSet;` and `use practice::notes::NotesDoc;` are present at the top of the file (add if missing):

```rust
    /// Build the open-song `sections` array (each section enriched with its
    /// occurrence `label` and stored `notes`) plus the `orphan_notes` list
    /// (stored notes whose label matches no current section). Shared by
    /// `song.open`, `section.replace`, and `section.notes.set`.
    fn sections_payload(&self, song_id: SongId) -> Result<(Value, Value), String> {
        let sections = self.store.list_sections(song_id).err_str()?;
        let notes: std::collections::HashMap<String, NotesDoc> = self
            .store
            .list_section_notes(song_id)
            .err_str()?
            .into_iter()
            .collect();
        let mut used: HashSet<String> = HashSet::new();
        let enriched: Vec<Value> = sections
            .iter()
            .map(|s| {
                let label = practice::naming::occurrence_label(s, &sections);
                used.insert(label.clone());
                let mut v = serde_json::to_value(s).expect("section serializes");
                v["label"] = json!(label);
                v["notes"] = serde_json::to_value(notes.get(&label)).expect("doc serializes");
                v
            })
            .collect();
        let orphans: Vec<Value> = notes
            .iter()
            .filter(|(label, _)| !used.contains(label.as_str()))
            .map(|(label, doc)| json!({ "label": label, "doc": doc }))
            .collect();
        Ok((json!(enriched), json!(orphans)))
    }
```

- [ ] **Step 2: Use it in `finish_open`**

Replace the `out` json in `finish_open` (`app.rs:1247-1254`) with:

```rust
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        let out = json!({
            "song": song,
            "sections": sections,
            "loops": self.store.list_loops(song_id).err_str()?,
            "peaks": decoded.peaks,
            "stems": decoded.stems,
            "analysis": self.store.get_analysis(song_id).err_str()?,
            "orphan_notes": orphan_notes,
        });
```

- [ ] **Step 3: Use it in `section_replace`**

Replace the tail of `section_replace` (`app.rs:1286-1287`) with:

```rust
        let _ = self.commit_sections(p.song_id, &news)?;
        let (sections, orphan_notes) = self.sections_payload(p.song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
```

- [ ] **Step 4: Add the command + dispatch arm**

Add the dispatch arm next to `"section.replace"` (`app.rs:397`):

```rust
            "section.notes.set" => self.section_notes_set(p),
```

Add the handler (near `section_replace`):

```rust
    fn section_notes_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            label: String,
            doc: NotesDoc,
        }
        let p: P = from_params(p)?;
        let song_id = self
            .open_song
            .as_ref()
            .map(|o| o.song.id)
            .ok_or_else(|| "no open song".to_string())?;
        p.doc.validate()?;
        self.store
            .set_section_notes(song_id, &p.label, &p.doc)
            .err_str()?;
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
    }
```

- [ ] **Step 5: Write an App-level test**

In the `app.rs` tests module (match its existing `App` construction helper), add:

```rust
    #[test]
    fn section_notes_roundtrip_and_orphan() {
        use practice::notes::{Block, NotesDoc};
        let mut app = test_app(); // match the helper other app tests use
        // import + open a song and save two sections named "verse"/"verse"
        // (reuse whatever helper the existing section tests use to seed sections)
        let song_id = seed_open_song_with_sections(&mut app, &["verse", "verse"]);

        let doc = NotesDoc { blocks: vec![Block::Text { text: "tab".into() }] };
        let res = app.dispatch_inner(
            "section.notes.set",
            json!({ "label": "verse 2", "doc": doc }),
        ).unwrap();

        // section "verse 2" carries the note; nothing orphaned
        let secs = res["sections"].as_array().unwrap();
        let v2 = secs.iter().find(|s| s["label"] == "verse 2").unwrap();
        assert_eq!(v2["notes"]["blocks"][0]["text"], "tab");
        assert!(res["orphan_notes"].as_array().unwrap().is_empty());

        // rename so "verse 2" no longer exists → the note orphans, not lost
        let res2 = app.dispatch_inner(
            "section.replace",
            json!({ "song_id": song_id, "sections": [
                { "name": "verse", "start": 0.0, "end": 1.0, "position": 0 },
                { "name": "bridge", "start": 1.0, "end": 2.0, "position": 1 }
            ]}),
        ).unwrap();
        let orphans = res2["orphan_notes"].as_array().unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0]["label"], "verse 2");
    }
```

> Use the existing app-test seeding helpers — read the tests module first to match names (`test_app`, how songs/sections are seeded, whether `dispatch_inner` is callable from tests or you go through `dispatch`). Adapt the two helper calls accordingly; the assertions are the point.

- [ ] **Step 6: Run it**

Run: `cargo test -p server section_notes_roundtrip_and_orphan`
Expected: PASS

- [ ] **Step 7: Full server crate + clippy**

Run: `cargo test -p server && cargo clippy -p server --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): section.notes.set + notes/label enrichment in open-song payload"
```

---

## Phase 3 — Frontend pure logic (TDD)

### Task 5: notes-doc.ts — types + tab transforms

**Files:**
- Create: `apps/desktop/src/lib/notes-doc.ts`
- Test: `apps/desktop/src/lib/notes-doc.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/desktop/src/lib/notes-doc.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  clearCell,
  emptyTab,
  moveCursor,
  setCell,
  setStrings,
  setWidth,
  type TabBlock,
} from "./notes-doc";

const tab = (rows: string[]): TabBlock => ({ kind: "tab", strings: rows.length, width: rows[0].length, rows });

describe("emptyTab", () => {
  it("fills a strings×width grid of dashes", () => {
    expect(emptyTab(3, 4)).toEqual({ kind: "tab", strings: 3, width: 4, rows: ["----", "----", "----"] });
  });
});

describe("setStrings (top-anchored growth)", () => {
  it("prepends blank rows at the top, keeping bottom content", () => {
    const t = tab(["11--", "22--"]);
    expect(setStrings(t, 4).rows).toEqual(["----", "----", "11--", "22--"]);
  });
  it("shrinks from the top, erasing the highest rows", () => {
    const t = tab(["aa--", "bb--", "cc--"]);
    expect(setStrings(t, 1).rows).toEqual(["cc--"]);
  });
  it("clamps to [1,12]", () => {
    expect(setStrings(tab(["----"]), 0).strings).toBe(1);
    expect(setStrings(tab(["----"]), 99).strings).toBe(12);
  });
});

describe("setWidth (right-anchored)", () => {
  it("appends dashes on grow, keeping content", () => {
    expect(setWidth(tab(["12--"]), 6).rows).toEqual(["12----"]);
  });
  it("truncates from the right on shrink", () => {
    expect(setWidth(tab(["12345-"]), 3).rows).toEqual(["123"]);
  });
  it("clamps to [1,256]", () => {
    expect(setWidth(tab(["----"]), 0).width).toBe(1);
  });
});

describe("setCell / clearCell (overtype)", () => {
  it("writes one char without changing width", () => {
    const t = setCell(tab(["----", "----"]), 1, 2, "7");
    expect(t.rows).toEqual(["----", "--7-"]);
    expect(t.width).toBe(4);
  });
  it("clears a cell back to dash", () => {
    expect(clearCell(tab(["--7-"]), 0, 2).rows).toEqual(["----"]);
  });
  it("ignores out-of-bounds", () => {
    const t = tab(["----"]);
    expect(setCell(t, 5, 5, "9")).toEqual(t);
  });
});

describe("moveCursor", () => {
  const t = tab(["----", "----", "----"]);
  it("clamps within the grid", () => {
    expect(moveCursor(t, { row: 0, col: 0 }, "up")).toEqual({ row: 0, col: 0 });
    expect(moveCursor(t, { row: 0, col: 0 }, "down")).toEqual({ row: 1, col: 0 });
    expect(moveCursor(t, { row: 0, col: 3 }, "right")).toEqual({ row: 0, col: 3 });
    expect(moveCursor(t, { row: 1, col: 1 }, "left")).toEqual({ row: 1, col: 0 });
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/notes-doc.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the module**

Create `apps/desktop/src/lib/notes-doc.ts`:

```ts
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
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/notes-doc.test.ts`
Expected: PASS (all cases).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/notes-doc.ts apps/desktop/src/lib/notes-doc.test.ts
git commit -m "feat(ui): notes-doc model + pure tab transforms"
```

### Task 6: active-section.ts — the hybrid resolver

**Files:**
- Create: `apps/desktop/src/lib/active-section.ts`
- Test: `apps/desktop/src/lib/active-section.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/desktop/src/lib/active-section.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { activeLabel, type SectionSpan } from "./active-section";

const secs: SectionSpan[] = [
  { label: "intro 1", start: 0, end: 10 },
  { label: "verse 1", start: 10, end: 20 },
  { label: "verse 2", start: 20, end: 30 },
];

describe("activeLabel", () => {
  it("follows the playhead when nothing is pinned", () => {
    expect(activeLabel(secs, 5, null)).toBe("intro 1");
    expect(activeLabel(secs, 25, null)).toBe("verse 2");
  });
  it("returns the pinned label regardless of playhead", () => {
    expect(activeLabel(secs, 5, "verse 2")).toBe("verse 2");
  });
  it("falls back to the playhead when the pin no longer exists", () => {
    expect(activeLabel(secs, 25, "bridge 1")).toBe("verse 2");
  });
  it("clamps to the first section past the end / before the start", () => {
    expect(activeLabel(secs, 999, null)).toBe("intro 1");
  });
  it("returns null with no sections", () => {
    expect(activeLabel([], 5, null)).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/active-section.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the module**

Create `apps/desktop/src/lib/active-section.ts`:

```ts
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
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/active-section.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/active-section.ts apps/desktop/src/lib/active-section.test.ts
git commit -m "feat(ui): active-section resolver for the notes box"
```

---

## Phase 4 — Store wiring + components

### Task 7: stores.ts — wire types + setSectionNotes action

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Extend the wire types**

Add near the `Section` interface (`stores.ts:22`):

```ts
import type { NotesDoc } from "./notes-doc";
```

Change `Section` (`stores.ts:22-29`) to add the enriched fields:

```ts
export interface Section {
  id: number;
  song_id: number;
  name: string;
  start: number;
  end: number;
  position: number;
  /** Occurrence label ("verse 2") — present on open-song payloads. */
  label?: string;
  /** Stored notes for this section, if any. */
  notes?: NotesDoc | null;
}

export interface OrphanNote {
  label: string;
  doc: NotesDoc;
}
```

Add `orphan_notes` to `OpenSong` (`stores.ts:106-114`):

```ts
export interface OpenSong {
  song: Song;
  sections: Section[];
  loops: LoopRegion[];
  peaks: Peaks;
  stems: boolean;
  analysis: Analysis | null;
  /** Notes whose label matches no current section. Never auto-deleted. */
  orphan_notes: OrphanNote[];
}
```

- [ ] **Step 2: Add the action + fold orphans into replaceSections**

In the `replaceSections` action (`stores.ts:828-832`), change the response type and merge to carry orphans:

```ts
    const out = await cmd<{ sections: Section[]; orphan_notes: OrphanNote[] }>("section.replace", {
      song_id: open.song.id,
      sections,
    });
    openSong.update((o) =>
      o ? { ...o, sections: out.sections, orphan_notes: out.orphan_notes } : o,
    );
```

Add a new action alongside it:

```ts
  /** Save a section's notes by occurrence label; empty doc clears it. The
   *  server returns the refreshed sections + orphan list, which we mirror. */
  async setSectionNotes(label: string, doc: NotesDoc): Promise<void> {
    if (!get(openSong)) return;
    const out = await cmd<{ sections: Section[]; orphan_notes: OrphanNote[] }>(
      "section.notes.set",
      { label, doc },
    );
    openSong.update((o) =>
      o ? { ...o, sections: out.sections, orphan_notes: out.orphan_notes } : o,
    );
  },
```

- [ ] **Step 3: Verify types**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors. (Existing `OpenSong` constructions get `orphan_notes` from the server; any test/mock building an `OpenSong` literal in TS must add `orphan_notes: []` — fix those if svelte-check flags them.)

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(ui): wire section notes + orphans into the open-song store"
```

### Task 8: TabBlock.svelte — the tablature widget

**Files:**
- Create: `apps/desktop/src/components/TabBlock.svelte`

- [ ] **Step 1: Write the component**

Create `apps/desktop/src/components/TabBlock.svelte`:

```svelte
<script lang="ts">
  // Interactive tab grid. Renders rows bounded by | and an overtype cell editor;
  // a top handle resizes string count (growth prepends higher strings on top),
  // a right handle resizes width (growth appends to the right). All grid math is
  // in lib/notes-doc; this component only maps pointer/keyboard to those calls.
  import {
    clearCell,
    moveCursor,
    setCell,
    setStrings,
    setWidth,
    type Cursor,
    type TabBlock,
  } from "../lib/notes-doc";

  let { block, onchange, ondelete }: {
    block: TabBlock;
    onchange: (b: TabBlock) => void;
    ondelete: () => void;
  } = $props();

  let cursor = $state<Cursor>({ row: 0, col: 0 });

  const CELL_W = 11; // px per column; keep in sync with .cell width
  const CELL_H = 20; // px per row

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowUp") { cursor = moveCursor(block, cursor, "up"); e.preventDefault(); return; }
    if (e.key === "ArrowDown") { cursor = moveCursor(block, cursor, "down"); e.preventDefault(); return; }
    if (e.key === "ArrowLeft") { cursor = moveCursor(block, cursor, "left"); e.preventDefault(); return; }
    if (e.key === "ArrowRight") { cursor = moveCursor(block, cursor, "right"); e.preventDefault(); return; }
    if (e.key === "Backspace") {
      onchange(clearCell(block, cursor.row, cursor.col));
      cursor = moveCursor(block, cursor, "left");
      e.preventDefault();
      return;
    }
    if (e.key.length === 1 && /[0-9a-zA-Z/\\~().]/.test(e.key)) {
      onchange(setCell(block, cursor.row, cursor.col, e.key));
      cursor = moveCursor(block, cursor, "right");
      e.preventDefault();
    }
  }

  // Drag the top edge: every CELL_H of vertical travel = ±1 string.
  function dragStrings(e: PointerEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const start = block.strings;
    const move = (ev: PointerEvent) => {
      const delta = Math.round((startY - ev.clientY) / CELL_H); // up = more
      onchange(setStrings(block, start + delta));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  // Drag the right edge: every CELL_W of horizontal travel = ±1 column.
  function dragWidth(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const start = block.width;
    const move = (ev: PointerEvent) => {
      const delta = Math.round((ev.clientX - startX) / CELL_W);
      onchange(setWidth(block, start + delta));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
</script>

<div class="tabblock">
  <button class="handle top" onpointerdown={dragStrings} title="drag: add/remove strings" aria-label="resize strings"></button>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="grid mono" tabindex="0" role="grid" onkeydown={onKey}>
    {#each block.rows as row, r (r)}
      <div class="row" role="row">
        <span class="bar">|</span>
        {#each row.split("") as ch, c (c)}
          <button
            class="cell"
            class:active={cursor.row === r && cursor.col === c}
            role="gridcell"
            onclick={() => (cursor = { row: r, col: c })}
          >{ch}</button>
        {/each}
        <span class="bar">|</span>
      </div>
    {/each}
  </div>
  <button class="handle right" onpointerdown={dragWidth} title="drag: add/remove width" aria-label="resize width"></button>
  <button class="del" onclick={ondelete} title="delete tab" aria-label="delete tab">×</button>
</div>

<style>
  .tabblock {
    position: relative;
    display: inline-block;
    padding: 6px 10px;
  }
  .grid {
    display: flex;
    flex-direction: column;
    outline: none;
  }
  .grid:focus-visible {
    outline: 1px solid var(--accent-dim);
    outline-offset: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    height: 20px;
  }
  .bar {
    color: var(--muted);
  }
  .cell {
    width: 11px;
    text-align: center;
    background: none;
    border: none;
    color: var(--fg);
    cursor: text;
    padding: 0;
    font: inherit;
    line-height: 20px;
  }
  .cell.active {
    background: var(--accent);
    color: var(--bg);
  }
  .handle {
    position: absolute;
    background: none;
    border: none;
    padding: 0;
  }
  .handle.top {
    top: 0;
    left: 10px;
    right: 22px;
    height: 6px;
    cursor: ns-resize;
  }
  .handle.top:hover { background: var(--accent-dim); }
  .handle.right {
    top: 6px;
    bottom: 6px;
    right: 10px;
    width: 6px;
    cursor: ew-resize;
  }
  .handle.right:hover { background: var(--accent-dim); }
  .del {
    position: absolute;
    top: 2px;
    right: 0;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
  }
  .del:hover { color: var(--fg); }
</style>
```

- [ ] **Step 2: Verify it type-checks**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/TabBlock.svelte
git commit -m "feat(ui): TabBlock tablature widget (grid editor + resize handles)"
```

### Task 9: Notes.svelte — the notes box

**Files:**
- Create: `apps/desktop/src/components/Notes.svelte`

- [ ] **Step 1: Write the component**

Create `apps/desktop/src/components/Notes.svelte`:

```svelte
<script lang="ts">
  // The notes box: shows/edits the active section's note document. Active
  // section is hybrid (lib/active-section): follow the playhead unless a label
  // is pinned by clicking a section or focusing the editor. Edits debounce-
  // autosave via actions.setSectionNotes; orphaned notes surface in a footer.
  import { activeLabel } from "../lib/active-section";
  import { emptyTab, type Block, type NotesDoc, type TabBlock } from "../lib/notes-doc";
  import { actions, openSong, position, selection, settings } from "../lib/stores";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  // Component aliased to avoid colliding with the `TabBlock` type imported above.
  import TabBlockView from "./TabBlock.svelte";

  let pinned = $state<string | null>(null);
  let editing = $state(false);
  let showOrphans = $state(false);

  let spans = $derived(
    ($openSong?.sections ?? [])
      .filter((s) => s.label)
      .map((s) => ({ label: s.label as string, start: s.start, end: s.end })),
  );

  // Pin while editing so the active section can't switch mid-keystroke.
  let active = $derived(
    editing && pinned ? pinned : activeLabel(spans, $position.secs, pinned),
  );

  let activeSection = $derived($openSong?.sections.find((s) => s.label === active) ?? null);

  // Local edit buffer for the active section; mirrors the store doc otherwise.
  let doc = $state<NotesDoc>({ blocks: [] });
  let bufferedLabel: string | null = null;

  $effect(() => {
    // reseed the buffer when the active section changes (and we're not editing)
    const label = active;
    if (label !== bufferedLabel && !editing) {
      bufferedLabel = label ?? null;
      doc = clone(activeSection?.notes ?? { blocks: [{ kind: "text", text: "" }] });
    }
  });

  function clone(d: NotesDoc): NotesDoc {
    return JSON.parse(JSON.stringify(d));
  }

  // Pin when a section is selected on the waveform / structure tab.
  $effect(() => {
    const sel = $selection;
    if (!sel) return;
    const hit = spans.find((s) => Math.abs(s.start - sel.start) < 0.05 && Math.abs(s.end - sel.end) < 0.05);
    if (hit) pinned = hit.label;
  });

  // Release the pin when playback starts (resume following the playhead).
  $effect(() => {
    if ($position.playing) {
      pinned = null;
      editing = false;
    }
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  function queueSave() {
    if (!active) return;
    const label = active;
    const snapshot = clone(doc);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void actions.setSectionNotes(label, snapshot), 500);
  }

  function editText(i: number, text: string) {
    const blocks = doc.blocks.slice();
    blocks[i] = { kind: "text", text };
    doc = { ...doc, blocks };
    queueSave();
  }

  function editTab(i: number, b: TabBlock) {
    const blocks = doc.blocks.slice();
    blocks[i] = b;
    doc = { ...doc, blocks };
    queueSave();
  }

  function deleteBlock(i: number) {
    let blocks = doc.blocks.filter((_, j) => j !== i);
    if (blocks.length === 0 || blocks[blocks.length - 1].kind !== "text") {
      blocks = [...blocks, { kind: "text", text: "" }];
    }
    doc = { ...doc, blocks };
    queueSave();
  }

  function addTab() {
    const strings = Number($settings["default_tab_strings"] ?? 4);
    const width = Number($settings["default_tab_width"] ?? 16);
    const blocks: Block[] = [...doc.blocks, emptyTab(strings, width), { kind: "text", text: "" }];
    doc = { ...doc, blocks };
    queueSave();
  }
</script>

{#if $openSong && spans.length > 0}
  <Box label={active ? `notes — ${active}` : "notes"} wide>
    {#snippet tools()}
      <button onclick={addTab} title="add a tablature block" aria-label="add tab">+ tab</button>
    {/snippet}

    <div class="doc">
      {#each doc.blocks as block, i (i)}
        {#if block.kind === "text"}
          <textarea
            class="text mono"
            value={block.text}
            placeholder={`jot tab or notes for ${active}…`}
            onfocus={() => { editing = true; if (active) pinned = active; }}
            onblur={() => (editing = false)}
            oninput={(e) => editText(i, e.currentTarget.value)}
          ></textarea>
        {:else}
          <TabBlockView
            block={block as TabBlock}
            onchange={(b) => editTab(i, b)}
            ondelete={() => deleteBlock(i)}
          />
        {/if}
      {/each}
    </div>

    {#if $openSong.orphan_notes.length > 0}
      <button class="orphan-toggle mono" onclick={() => (showOrphans = !showOrphans)}>
        {$openSong.orphan_notes.length} notes from removed sections {showOrphans ? "▾" : "▸"}
      </button>
      {#if showOrphans}
        <ul class="orphans">
          {#each $openSong.orphan_notes as o (o.label)}
            <li>
              <span class="olabel mono">{o.label}</span>
              <Button variant="chip" onclick={() => void actions.setSectionNotes(o.label, { blocks: [] })}>clear</Button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </Box>
{/if}

<style>
  .doc {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 280px;
    overflow-y: auto;
  }
  .text {
    width: 100%;
    min-height: 44px;
    resize: vertical;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--fg);
    padding: 6px 8px;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
  }
  .orphan-toggle {
    margin-top: 8px;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 11px;
    padding: 0;
    text-align: left;
  }
  .orphan-toggle:hover { color: var(--fg); }
  .orphans {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .orphans li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .olabel {
    font-size: 11px;
    color: var(--muted);
  }
</style>
```

- [ ] **Step 2: Verify it type-checks**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors. (If `selection`/`position`/`settings` import names differ, fix to match `stores.ts`.)

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/Notes.svelte
git commit -m "feat(ui): Notes box — block editor, autosave, orphan footer"
```

### Task 10: Mount the box + settings fields

**Files:**
- Modify: `apps/desktop/src/App.svelte`
- Modify: `apps/desktop/src/components/SettingsPanel.svelte`

- [ ] **Step 1: Import and mount Notes in the stage row**

In `App.svelte`, add the import alongside the others:

```ts
  import Notes from "./components/Notes.svelte";
```

In the `.boxes` block, add `<Notes />` after `<Isolation />`:

```svelte
    <div class="boxes">
      {#if $openSong}
        <Isolation />
        <Notes />
      {/if}
      <Tuner />
      {#if $openSong && $drillSpan}
        <Drill />
      {/if}
    </div>
```

- [ ] **Step 2: Add the two settings fields**

In `SettingsPanel.svelte`, follow the existing pattern for a numeric setting (read from the `settings` store, write via `actions.setSetting`). Add fields for `default_tab_strings` (default 4, min 1, max 12) and `default_tab_width` (default 16, min 1, max 256). Use the existing `NumberField` widget and label them "tab strings" and "tab width". Match the exact markup of an existing setting row in that file.

- [ ] **Step 3: Lint + type-check the whole frontend**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors.

- [ ] **Step 4: Manual verification in the running app**

Run: `just dev` (or rebuild + run). With a song that has sections:
- The **notes box** appears in the stage; header shows the section the playhead is in, and switches as it plays.
- Click a section in the structure tab → the box pins to it; press play → it resumes following.
- Type in the text block; reload the app → the note persists.
- Click **+ tab** → a tab block appears. Type digits (overtype), arrow-navigate, backspace clears. Drag the **top** edge → strings change (new rows on top, content anchored to the bottom). Drag the **right** edge → width changes (dashes appended on grow; content kept). Delete the tab with ×.
- Rename that section in the structure tab → its note shows under "N notes from removed sections"; "clear" removes it; renaming back restores it.
- Change "tab strings"/"tab width" in settings → a newly added tab uses those defaults.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/App.svelte apps/desktop/src/components/SettingsPanel.svelte
git commit -m "feat(ui): mount the notes box + tab default settings"
```

---

## Phase 5 — Gate

### Task 11: Full check + CLAUDE.md vocabulary

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Document the box in the UI vocabulary**

In `CLAUDE.md`, in the Stage bullet of the UI vocabulary section, add the **notes box** (`Notes.svelte`) to the list of stage boxes — "per-section notes (free text + inline `TabBlock` tablature), keyed by the section's occurrence label; appears once a song has sections."

- [ ] **Step 2: Run the full gate**

Run: `just check`
Expected: PASS — `cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`, `pnpm vitest run`, `svelte-check`, theme guardrail all green.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: note the notes box in the UI vocabulary"
```

---

## Self-Review notes (for the implementer)

- **Section-test helpers**: Tasks 2 and 4 reference `test_store()` / `test_app()` / `seed_open_song_with_sections` as stand-ins — read each crate's existing `tests` module and substitute the real helper names and seeding flow. The assertions are the contract; the setup must match the codebase.
- **`OptionalExtension`**: `get_section_notes` uses `.optional()`; ensure the import exists in `store.rs`.
- **TS `OpenSong` literals**: adding `orphan_notes` makes it required; any TS test/mock that builds an `OpenSong` literal needs `orphan_notes: []`.
- **Settings keys**: `default_tab_strings` / `default_tab_width` are read in `Notes.addTab` with `?? 4` / `?? 16` fallbacks, so the feature works before the settings rows exist.
- **Template narrowing**: Svelte doesn't always narrow `block` to `TabBlock` inside the `{:else}` branch, so the `block as TabBlock` cast is deliberate. If svelte-check is happy without it, the cast is harmless.
```
