<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { actions, openSong, songs } from "../lib/stores";

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
</script>

<h2>library</h2>
<ul>
  {#each $songs as song (song.id)}
    <li>
      <button
        class="song"
        class:open={$openSong?.song.id === song.id}
        onclick={() => openIt(song.id)}
      >
        <span class="title">{song.title}</span>
        <span class="meta">
          {song.artist ?? ""}
          <span class="mono">{fmtDur(song.duration_secs)}</span>
        </span>
      </button>
    </li>
  {/each}
</ul>
<button class="import" onclick={importSong}>+ import</button>
{#if error}
  <p class="error">{error}</p>
{/if}

<style>
  .song {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
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

  .meta {
    display: flex;
    justify-content: space-between;
    width: 100%;
    font-size: 11px;
    color: var(--muted);
  }

  .import {
    width: 100%;
    margin-top: var(--space);
  }

  .error {
    font-size: 11px;
    color: var(--miss);
  }
</style>
