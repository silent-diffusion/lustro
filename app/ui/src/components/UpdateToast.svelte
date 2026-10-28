<script lang="ts">
  // Auto-update "ready" toast. Not modal and never takes focus. It is held while a
  // scan or a cleanup runs (the gear dot shows instead) and appears as soon as that finishes.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  const st = $derived(app.update);
  const show = $derived(!app.portable && st.state === "ready" && !app.updateToastDismissed && !app.busy && !app.updateDialog && !app.installing);
</script>

{#if show && st.state === "ready"}
  <div class="toast-upd" role="status">
    <div class="ic"><Icon name="logo" size={22} /></div>
    <div class="tx">
      <b>Lustro {st.info.version} is ready</b>
      <small>Downloaded and verified. It installs the next time Lustro starts.</small>
      <button class="linkbtn" onclick={() => (app.notesDialog = { title: `What's new in ${st.info.version}`, notes: st.info.notes, restart: true })}>See what's new</button>
    </div>
    <div class="acts">
      <button class="btn small" onclick={() => (app.updateToastDismissed = true)}>Later</button>
      <button class="btn small primary" onclick={() => app.restartToUpdate()}>Restart to update</button>
    </div>
  </div>
{/if}

<style>
  .toast-upd {
    position: fixed; right: 16px; bottom: 16px; z-index: 55; width: min(420px, calc(100vw - 32px));
    display: grid; grid-template-columns: auto 1fr; gap: 10px 12px; background: var(--surface-2); border: 1px solid var(--stroke-strong);
    border-radius: var(--r-md); padding: 14px 16px; box-shadow: 0 12px 32px rgba(0,0,0,.35); animation: toast-in .3s var(--ease);
  }
  .ic { --bg: var(--surface-1); width: 36px; height: 36px; border-radius: 10px; background: var(--surface-1); display: grid; place-items: center; color: var(--text); }
  .tx small { display: block; color: var(--text-3); font-size: var(--fs-xs); margin-top: 2px; }
  .linkbtn { border: 0; background: none; padding: 4px 0 0; color: var(--text-3); font-size: var(--fs-xs); text-decoration: underline; text-underline-offset: 3px; }
  .linkbtn:hover { color: var(--text); }
  .acts { grid-column: 2; display: flex; gap: 8px; justify-content: flex-end; }
  @keyframes toast-in { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: none; } }
</style>
