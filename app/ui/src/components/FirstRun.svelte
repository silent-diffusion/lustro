<script lang="ts">
  // No scan yet: one centered card. After pressing Scan, Overview and Explore fill in live.
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  const label = $derived(app.volume?.label ?? "this drive");
</script>

<section class="card first">
  <div class="ic"><Icon name="drive" size={26} /></div>
  <h2>Scan {label} to see where your space went</h2>
  <p class="muted">Scanning only reads file names, sizes and dates. Nothing leaves this computer.</p>
  {#if app.volumes.length > 1}
    <div class="drive-chip" role="group" aria-label="Drives">
      {#each app.volumes as x (x.id)}
        <button class:on={x.id === app.volumeId} title="{x.label} · {x.fileSystem}" onclick={() => app.pickVolume(x.id)}>{x.label}</button>
      {/each}
    </div>
  {/if}
  <div class="acts">
    <button class="btn" onclick={() => app.scanFolder()}><Icon name="folder" />Scan a folder…</button>
    <button class="btn primary" onclick={() => app.startScan()}><Icon name="scan" />Scan {label}</button>
  </div>
</section>

<style>
  .first { text-align: center; padding: 56px 24px; display: grid; justify-items: center; gap: 12px; max-width: 640px; margin: 24px auto 0; }
  .first h2 { margin: 0; font-size: var(--fs-xl); }
  .first p { margin: 0; }
  .ic { width: 52px; height: 52px; border-radius: 16px; background: var(--surface-2); display: grid; place-items: center; color: var(--text-2); }
  .drive-chip { display: flex; gap: 4px; padding: 4px; background: var(--surface-2); border-radius: var(--r-pill); flex-wrap: wrap; justify-content: center; }
  .drive-chip button { border: 0; background: none; padding: 6px 14px; border-radius: var(--r-pill); color: var(--text-2); }
  .drive-chip button.on { background: var(--surface-3); color: var(--text); }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; margin-top: 4px; }
</style>
