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
  // Deliberately non-reactive: an effect-local latch so reseeding doesn't
  // retrigger its own effect. Do NOT make this $state.
  let bufferedLabel: string | null = null;

  function clone(d: NotesDoc): NotesDoc {
    return JSON.parse(JSON.stringify(d));
  }

  $effect(() => {
    // reseed the buffer when the active section changes (and we're not editing)
    const label = active;
    if (label !== bufferedLabel && !editing) {
      bufferedLabel = label ?? null;
      const stored = activeSection?.notes;
      // an empty-or-missing doc still gets one text block so there's a place to type
      doc = clone(stored && stored.blocks.length ? stored : { blocks: [{ kind: "text", text: "" }] });
    }
  });

  // Pin when a section is selected on the waveform / structure tab.
  $effect(() => {
    const sel = $selection;
    if (!sel) return;
    const hit = spans.find((s) => Math.abs(s.start - sel.start) < 0.05 && Math.abs(s.end - sel.end) < 0.05);
    if (hit) pinned = hit.label;
  });

  // Release the pin on the rising edge of playback (resume following the
  // playhead). Edge-triggered, not level: `$position` is a fresh object every
  // ~50ms tick, so a level check would clear `editing` 20×/s and yank the
  // editor mid-keystroke. Flush first so a pending edit isn't lost.
  let wasPlaying = false;
  $effect(() => {
    const playing = $position.playing;
    if (playing && !wasPlaying) {
      commit();
      pinned = null;
      editing = false;
    }
    wasPlaying = playing;
  });

  // Autosave holds the label+doc captured when the edit happened, so a flush or
  // a section switch always persists the RIGHT content under the RIGHT label —
  // never whatever `active`/`doc` have since become.
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let pending: { label: string; snapshot: NotesDoc } | null = null;

  /** Persist the pending edit immediately and disarm the timer. */
  function commit() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pending) {
      void actions.setSectionNotes(pending.label, pending.snapshot);
      pending = null;
    }
  }

  function queueSave() {
    if (!active) return;
    // a different section is already pending — persist it before re-arming, so
    // switching sections within the debounce window can't drop or misroute it
    if (pending && pending.label !== active) commit();
    pending = { label: active, snapshot: clone(doc) };
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(commit, 500);
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
            onblur={() => { commit(); editing = false; }}
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
