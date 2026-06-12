<script lang="ts">
</script>

<div class="shell">
  <aside class="library"></aside>
  <main class="stage"></main>
  <aside class="panels"></aside>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 240px 1fr 320px;
    height: 100vh;
  }

  .library {
    border-right: 1px solid var(--line);
    padding: var(--space);
    overflow-y: auto;
  }

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: var(--space);
  }

  .panels {
    border-left: 1px solid var(--line);
    padding: var(--space);
    overflow-y: auto;
  }
</style>
