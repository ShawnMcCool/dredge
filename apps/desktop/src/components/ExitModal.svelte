<script lang="ts">
  // Escape with nothing else to dismiss asks before quitting. Keyboard
  // first: Enter or y exits, Escape or n stays (Modal handles Escape).
  import { quit } from "../lib/ipc";
  import { exitPromptVisible } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Modal from "../lib/ui/Modal.svelte";

  const stay = () => exitPromptVisible.set(false);
  const exit = () => void quit();

  function onkeydown(e: KeyboardEvent) {
    if (!$exitPromptVisible) return;
    if (e.key === "Enter" || e.key === "y") {
      e.preventDefault();
      exit();
    } else if (e.key === "n") {
      e.preventDefault();
      stay();
    }
  }
</script>

<svelte:window {onkeydown} />

<Modal open={$exitPromptVisible} title="exit earworm?" closable onclose={stay}>
  <div class="actions">
    <Button accent onclick={exit}>exit</Button>
    <Button onclick={stay}>stay</Button>
  </div>
</Modal>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
  }
</style>
