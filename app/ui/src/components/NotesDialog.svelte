<script lang="ts">
  // Release notes on their own: "See what's new" from the ready toast, and Settings' "Release notes".
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
  import Notes from "./Notes.svelte";

  const d = $derived(app.notesDialog);
  const close = () => (app.notesDialog = null);
</script>

<svelte:window onkeydown={(e) => d && e.key === "Escape" && close()} />

{#if d}
  <div class="modal-bg" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="card dlg" role="dialog" aria-modal="true" aria-labelledby="notes-title" tabindex="-1">
      <div class="card-head"><div class="ic mark"><Icon name="logo" size={22} /></div><h2 id="notes-title">{d.title}</h2></div>
      <div class="box"><Notes src={d.notes} /></div>
      <div class="foot">
        <span class="sp"></span>
        <button class="btn" onclick={close}>Close</button>
        {#if d.restart && app.update.state === "ready"}
          <button class="btn primary" disabled={app.busy} title={app.busy ? "Waits until the scan or cleanup finishes" : undefined} onclick={() => app.restartToUpdate()}>Restart to update</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.55); display: grid; place-items: center; z-index: 65; padding: 16px; }
  .dlg { width: min(560px, 100%); max-height: calc(100vh - 32px); display: flex; flex-direction: column; }
  .mark { --bg: var(--surface-2); color: var(--text); }
  .box { background: var(--surface-2); border-radius: var(--r-md); padding: 4px 16px; max-height: 360px; overflow: auto; }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 16px; }
  .sp { flex: 1; }
</style>
