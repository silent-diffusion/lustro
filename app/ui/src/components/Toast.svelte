<script lang="ts">
  import { app } from "../lib/app.svelte";
</script>

{#if app.toast}
  <div class="toast" role="status">{app.toast}</div>
{/if}

<style>
  .toast {
    position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); z-index: 80;
    background: var(--surface-3); border: 1px solid var(--stroke-strong); border-radius: var(--r-pill);
    padding: 8px 16px; box-shadow: var(--shadow); font-size: var(--fs-sm);
  }
</style>
