<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { fmtWhen } from "../lib/format";
  import Icon from "./Icon.svelte";

  const v = $derived(app.volume);
  const used = $derived(v ? v.totalBytes - v.freeBytes : 0);

  function pick(id: string) {
    const target = app.volumes.find((x) => x.id === id);
    if (!target || id === app.volumeId) return;
    if (target.fileSystem === "Virtual") app.say(`${target.label} is a cloud drive: listed, never scanned for content.`);
    else app.say(`${target.label} hasn't been scanned in this preview.`);
  }
</script>

{#if v}
  <div class="context">
    <div class="title"><Icon name="drive" size={22} /><span>{v.label} <span class="fs faint">{v.fileSystem}</span></span></div>
    <div class="sep"></div>
    <div class="meta"><Icon name="clock" /><span>{app.lastScanAt ? `Scanned ${fmtWhen(app.lastScanAt)}` : "Not scanned yet"}</span></div>
    <div class="meta"><span class="mono">{app.fb(used)}</span><span class="faint">of {app.fb(v.totalBytes)} used</span></div>
    <div class="spacer"></div>
    <button class="btn small" onclick={() => app.scanFolder()}><Icon name="folder" />Scan a folder…</button>
    <div class="drive-chip" role="group" aria-label="Drives">
      {#each app.volumes as x (x.id)}
        <button class:on={x.id === app.volumeId} title="{x.label} · {x.fileSystem}" onclick={() => pick(x.id)}>{x.label}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .context {
    display: flex; align-items: center; gap: var(--sp-5); flex-wrap: wrap;
    background: var(--surface-1); border: 1px solid var(--stroke); border-radius: var(--r-pill);
    padding: 10px 12px 10px 22px; margin-bottom: var(--sp-4);
  }
  .title { font-size: var(--fs-lg); font-weight: var(--fw-strong); display: flex; gap: 10px; align-items: center; white-space: nowrap; color: var(--text); }
  .title :global(.icon) { color: var(--text-2); }
  .fs { font-size: var(--fs-xs); font-weight: var(--fw-regular); margin-left: 4px; }
  .sep { width: 1px; height: 20px; background: var(--stroke-strong); }
  .meta { display: flex; gap: 8px; align-items: center; color: var(--text-2); }
  .spacer { flex: 1; }
  .drive-chip { display: flex; gap: 4px; padding: 4px; background: var(--surface-2); border-radius: var(--r-pill); flex-wrap: wrap; }
  .drive-chip button { border: 0; background: none; padding: 6px 14px; border-radius: var(--r-pill); color: var(--text-2); }
  .drive-chip button.on { background: var(--surface-3); color: var(--text); }
</style>
