<script lang="ts">
  // Cleanup and What grew keep showing the previous snapshot while a scan runs; this says so.
  import { app } from "../lib/app.svelte";

  let { text }: { text: string } = $props();
</script>

{#if app.scanning}
  <div class="scanbanner" role="status"><span class="pulse" aria-hidden="true"></span>{text}</div>
{/if}

<style>
  .scanbanner {
    display: flex; align-items: center; gap: 10px; margin-bottom: var(--sp-4); padding: 10px 16px;
    background: var(--surface-2); border: 1px solid var(--stroke); border-radius: var(--r-md); color: var(--text-2); font-size: var(--fs-sm);
  }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); flex: none; animation: pulse 1.4s infinite; }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 60%, transparent); } 100% { box-shadow: 0 0 0 8px transparent; } }
</style>
