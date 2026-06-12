<script lang="ts">
  import { onMount } from "svelte";
  import { actions, captureNodes, captureStatus } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

  // "all" passes a huge window — the server clamps to what's buffered
  const GRABS = [
    { label: "last 30s", secs: 30 },
    { label: "60s", secs: 60 },
    { label: "2m", secs: 120 },
    { label: "all", secs: 1_000_000 },
  ];

  let busy = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    void actions.refreshCaptureNodes();
    void actions.refreshCaptureStatus();
    // poll only while this tab is mounted
    const timer = setInterval(() => void actions.refreshCaptureStatus(), 2000);
    return () => clearInterval(timer);
  });

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await fn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  const refresh = () => run(() => actions.refreshCaptureNodes());
  const start = (id: number) => run(() => actions.startCapture(id));
  const stop = () => run(() => actions.stopCapture());
  const grab = (secs: number) => run(() => actions.grabCapture(secs));
</script>

<h2>capture</h2>

{#if $captureStatus.running}
  <p class="rec mono">
    <span class="dot">●</span> REC {$captureStatus.app}
    — {($captureStatus.filled_secs ?? 0).toFixed(0)}s buffered
  </p>
  {#if $captureStatus.media}
    <p class="media">{$captureStatus.media}</p>
  {/if}
  <div class="grabs">
    {#each GRABS as g (g.label)}
      <Button disabled={busy} onclick={() => grab(g.secs)}>{g.label}</Button>
    {/each}
  </div>
  <div class="bar">
    <Button disabled={busy} onclick={stop}>stop capture</Button>
  </div>
{:else}
  <div class="bar">
    <Button disabled={busy} onclick={refresh}>refresh apps</Button>
  </div>
  {#if $captureNodes.length === 0}
    <p class="empty">no apps playing audio</p>
  {:else}
    <ul>
      {#each $captureNodes as n (n.id)}
        <li>
          <button class="node" disabled={busy} onclick={() => start(n.id)}>
            <strong>{n.app}</strong>
            {#if n.media}<span class="muted">{n.media}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

{#if error}
  <p class="error">{error}</p>
{/if}

<p class="why">tap an app, let it roll, grab what just played.</p>

<style>
  .bar {
    margin-bottom: var(--space);
  }

  .empty {
    font-size: 11px;
    color: var(--muted);
  }

  .node {
    display: flex;
    justify-content: space-between;
    gap: calc(var(--space) / 2);
    width: 100%;
    background: none;
    border: none;
    text-align: left;
    padding: calc(var(--space) / 2);
  }

  .node:hover {
    background: var(--bg-raised);
  }

  .node .muted {
    color: var(--muted);
    font-size: 11px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rec {
    font-size: 12px;
  }

  .dot {
    color: var(--accent);
  }

  .media {
    font-size: 11px;
    color: var(--muted);
    margin-bottom: var(--space);
  }

  .grabs {
    display: flex;
    gap: calc(var(--space) / 2);
    margin: var(--space) 0;
    flex-wrap: wrap;
    min-width: 0;
  }

  .error {
    font-size: 11px;
    color: var(--miss);
  }

  .why {
    margin-top: calc(var(--space) * 3);
    font-size: 11px;
    color: var(--muted);
    font-style: italic;
  }
</style>
