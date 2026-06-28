<script lang="ts">
  // A select rendered as a button + hovering menu, so a box isn't forced to the
  // width of a native <select> bar and the options can read vertically. Shares
  // the app's menu idiom (the + tool / tuner pickers): a click-away backdrop, an
  // accent-marked current option. Pure presentational — value in, change out.
  interface Option {
    value: string;
    label: string;
    disabled?: boolean;
  }
  let {
    value,
    options,
    onchange,
    disabled = false,
    label,
  }: {
    value: string;
    options: Option[];
    onchange: (value: string) => void;
    disabled?: boolean;
    label?: string;
  } = $props();

  let open = $state(false);
  const current = $derived(options.find((o) => o.value === value));

  function pick(o: Option) {
    open = false;
    if (!o.disabled && o.value !== value) onchange(o.value);
  }
</script>

<div class="dropdown">
  <button
    class="trigger"
    {disabled}
    aria-label={label}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <span class="cur">{current?.label ?? value}</span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  {#if open}
    <!-- click-away catcher -->
    <button class="backdrop" aria-label="close menu" onclick={() => (open = false)}></button>
    <div class="menu" role="listbox">
      {#each options as o (o.value)}
        <button
          class="opt"
          class:sel={o.value === value}
          role="option"
          aria-selected={o.value === value}
          disabled={o.disabled}
          onclick={() => pick(o)}>{o.label}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }
  .trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    height: var(--control-h);
    padding: 0 8px;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .trigger:hover:not(:disabled) {
    border-color: var(--accent-dim);
  }
  .trigger:disabled {
    color: var(--muted);
    cursor: default;
  }
  .cur {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caret {
    color: var(--muted);
    font-size: 10px;
    flex: 0 0 auto;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: none;
    border: none;
    cursor: default;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 41;
    min-width: 100%;
    display: flex;
    flex-direction: column;
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: 4px;
    box-shadow: 0 4px 12px -4px rgb(0 0 0 / 0.5);
  }
  .opt {
    text-align: left;
    background: none;
    border: none;
    color: var(--fg);
    cursor: pointer;
    padding: 6px 10px;
    font: inherit;
    font-size: 12px;
    white-space: nowrap;
  }
  .opt:hover:not(:disabled) {
    background: var(--accent-dim);
  }
  .opt:disabled {
    color: var(--muted);
    cursor: default;
  }
  .opt.sel {
    color: var(--accent);
  }
</style>
