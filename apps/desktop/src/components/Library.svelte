<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { actions, openingSong, openSong, songs } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Modal from "../lib/ui/Modal.svelte";

  let error = $state("");

  onMount(() => {
    void actions.refreshSongs();
  });

  function fmtDur(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  async function importSong() {
    error = "";
    const path = await open({
      multiple: false,
      filters: [{ name: "audio", extensions: ["mp3", "flac", "ogg", "wav", "m4a"] }],
    });
    if (typeof path !== "string") return;
    try {
      await actions.importSong(path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function openIt(id: number) {
    error = "";
    try {
      await actions.openSong(id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  let confirmDelete = $state<number | null>(null);
  let renaming = $state<number | null>(null);
  let renameTitle = $state("");
  let renameArtist = $state("");

  function startRename(id: number, title: string, artist: string | null) {
    renaming = id;
    renameTitle = title;
    renameArtist = artist ?? "";
  }

  async function saveRename() {
    if (renaming === null) return;
    error = "";
    try {
      await actions.updateSong(renaming, renameTitle.trim(), renameArtist.trim() || null);
      renaming = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function doDelete(id: number) {
    error = "";
    try {
      await actions.deleteSong(id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    confirmDelete = null;
  }
</script>

<h2>library</h2>
<ul>
  {#each $songs as song (song.id)}
    <li class="row">
      <button
        class="song"
        class:open={$openSong?.song.id === song.id}
        disabled={$openingSong !== null}
        onclick={() => openIt(song.id)}
      >
        <span class="title">
          {song.title}
          {#if $openingSong === song.id}<span class="opening mono">◌</span>{/if}
        </span>
        <span class="meta">
          {song.artist ?? ""}
          <span class="mono">{fmtDur(song.duration_secs)}</span>
        </span>
      </button>
      <span class="actions">
        <button class="act" title="rename" onclick={() => startRename(song.id, song.title, song.artist)}>✎</button>
        <button class="act" title="delete" onclick={() => (confirmDelete = song.id)}>✕</button>
      </span>
    </li>
  {/each}
</ul>

<Modal open={confirmDelete !== null} title="delete track" closable onclose={() => (confirmDelete = null)}>
  <p>Remove this track and its loops, plans, ratings, and analysis? The source audio file is kept.</p>
  <div class="modal-actions">
    <Button onclick={() => (confirmDelete = null)}>cancel</Button>
    <Button accent onclick={() => confirmDelete !== null && doDelete(confirmDelete)}>delete</Button>
  </div>
</Modal>

<Modal open={renaming !== null} title="rename track" closable onclose={() => (renaming = null)}>
  <label class="field">title <input bind:value={renameTitle} /></label>
  <label class="field">artist <input bind:value={renameArtist} /></label>
  <div class="modal-actions">
    <Button onclick={() => (renaming = null)}>cancel</Button>
    <Button accent onclick={saveRename}>save</Button>
  </div>
</Modal>
<Button style="width: 100%; margin-top: var(--space)" onclick={importSong}>+ import</Button>
{#if error}
  <p class="error">{error}</p>
{/if}

<style>
  .song {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    min-width: 0;
    text-align: left;
    background: none;
    border: none;
    padding: calc(var(--space) / 2);
    gap: 2px;
  }

  .song:hover {
    background: var(--bg-raised);
  }

  .song.open .title {
    color: var(--accent);
  }

  /* in-flight open: the dotted circle reads as a spinner once it turns */
  .opening {
    display: inline-block;
    margin-left: 4px;
    color: var(--accent);
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .meta {
    display: flex;
    justify-content: space-between;
    width: 100%;
    font-size: 11px;
    color: var(--muted);
  }

  .error {
    font-size: 11px;
    color: var(--miss);
  }

  .row {
    display: flex;
    align-items: stretch;
  }
  .row .song {
    flex: 1;
    min-width: 0;
  }
  .actions {
    display: none;
    align-items: center;
    gap: 2px;
    padding-right: calc(var(--space) / 2);
  }
  .row:hover .actions {
    display: flex;
  }
  .act {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 2px 4px;
  }
  .act:hover {
    color: var(--accent);
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
    margin-top: var(--space);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: var(--space);
    font-size: 12px;
    color: var(--muted);
  }
</style>
