<script lang="ts">
  // Windows only: Fast scan launches an elevated helper, so Windows shows a permission (UAC) prompt.
  // Explain it first; the user can go ahead, or run a Standard scan instead.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  const c = $derived(app.fastConfirm);
  const close = () => (app.fastConfirm = null);
  const focus = (el: HTMLElement) => { el.focus(); };
</script>

<svelte:window onkeydown={(e) => c && e.key === "Escape" && close()} />

{#if c}
  <div class="modal-bg" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="card dlg" role="dialog" aria-modal="true" aria-labelledby="fast-title" tabindex="-1">
      <div class="card-head"><div class="ic"><Icon name="shield" size={18} /></div><h2 id="fast-title">Fast scan</h2><span class="badge review">Needs permission</span></div>
      <p><b>Windows will ask for permission.</b> Fast scan reads the drive index directly; it can't change files.</p>
      <ul class="muted">
        <li>Permission is asked for this scan only and never stored.</li>
        <li>If you say no, Lustro runs a Standard scan instead. It takes longer and needs no permission.</li>
      </ul>
      <div class="mono faint xs">{c.root}</div>
      <div class="foot">
        <button class="btn" onclick={close}>Cancel</button>
        <span class="sp"></span>
        <button class="btn" onclick={() => app.startScan(c.root, "standard")}>Use Standard scan</button>
        <button class="btn primary" use:focus onclick={() => app.startScan(c.root, "fast")}>Continue</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.55); display: grid; place-items: center; z-index: 65; padding: 16px; }
  .dlg { width: min(480px, 100%); }
  p { margin: 0 0 8px; }
  ul { margin: 0 0 12px; padding-left: 18px; font-size: var(--fs-sm); }
  li { margin: 4px 0; }
  .xs { font-size: var(--fs-xs); }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 16px; }
  .sp { flex: 1; }
</style>
