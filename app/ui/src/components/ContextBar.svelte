<script lang="ts">
  // Drive, last scan and used space. While a scan runs, the "Scanned …" meta becomes the scan strip
  // (counters, current path, Stop) with a progress line along the bottom edge. When it ends, a completion
  // line shows for 10 seconds or until you click elsewhere in the bar.
  import type { SnapshotMeta } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { fmtCount, fmtDuration, fmtWhen } from "../lib/format";
  import Icon from "./Icon.svelte";

  const v = $derived(app.volume);
  const s = $derived(app.scan);
  const running = $derived(app.scanning);
  const done = $derived(s?.done ?? null);
  const sep = $derived(app.platform === "windows" ? "\\" : "/");

  // The completion line: shown for 10s after a scan finishes, or until a click elsewhere in the bar.
  let ended = $state<string | null>(null);
  let prevSnap = $state.raw<{ delta: number; from: number } | null>(null);
  $effect(() => {
    const d = done;
    if (!d) return;
    ended = d.scanId;
    prevSnap = null;
    if (!d.cancelled) {
      app.backend.listSnapshots(app.root?.name ?? "").then((snaps: SnapshotMeta[]) => {
        const n = snaps.length;
        prevSnap = n > 1 ? { delta: snaps[n - 1].allocatedBytes - snaps[n - 2].allocatedBytes, from: snaps[n - 2].takenAt } : null;
      }).catch(() => {});
      const t = setTimeout(() => { if (ended === d.scanId) ended = null; }, 10_000);
      return () => clearTimeout(t);
    }
  });
  const showEnd = $derived(!!done && ended === done.scanId && !done.cancelled);
  /** A stopped scan keeps its line (and the partial tree) until the next scan. */
  const stopped = $derived(!!done?.cancelled);

  // Current path, read from the busiest branch.
  const current = $derived.by(() => {
    void s?.tick;
    if (!s || !running || s.busiest == null) return "";
    return s.pathText(s.busiest, sep);
  });
  /** Ellipsis in the middle, so both the drive and the folder being read stay visible. */
  function middle(text: string, max = 64): string {
    if (text.length <= max) return text;
    const k = Math.floor((max - 1) / 2);
    return text.slice(0, k) + "…" + text.slice(text.length - (max - 1 - k));
  }
  const rootLabel = $derived(s ? s.root : "");
  const modeLabel = (m: string) => (m === "fast" ? "Fast scan" : "Standard scan");

  // Screen readers hear the start, every 25% and the finish, never every tick.
  const announce = $derived.by(() => {
    if (!s) return "";
    if (done) return done.cancelled ? "Scan stopped. Showing what was found." : "Scan complete.";
    const q = Math.floor(app.scanPct * 4) * 25;
    return q === 0 ? `Scanning ${rootLabel}` : `Scanning, ${q}%`;
  });

  let warnsOpen = $state(false);
  const fellBack = $derived(!!done && !done.cancelled && done.modeUsed !== s?.mode);

  function pick(id: string) {
    if (running) return;
    app.pickVolume(id);
  }
  function stop() {
    if (s && running) app.backend.cancelScan(s.id);
  }
  function clickBar(e: MouseEvent) {
    if (showEnd && !(e.target as Element | null)?.closest?.(".endline")) ended = null;
  }
</script>

{#if v}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="context" class:scanning={running || stopped} onclick={clickBar}>
    <div class="title"><Icon name="drive" size={22} /><span>{v.label} <span class="fs faint">{v.fileSystem}</span></span></div>
    <div class="sep"></div>
    <div class="live-region sr-only" role="status" aria-live="polite">{announce}</div>
    {#if running && s}
      <div class="strip">
        <span class="pulse" aria-hidden="true"></span>
        <span class="what">Scanning <b>{rootLabel}</b></span>
        <span class="badge">{modeLabel(s.mode)}</span>
        <span class="counters mono">{fmtCount(s.progress.files)} files · {app.fb(s.progress.allocatedBytes)} found · {fmtDuration(s.progress.elapsedMs)}</span>
      </div>
      <div class="spacer"></div>
      <span class="path mono" title={current}>{middle(current)}</span>
      <button class="btn small" onclick={stop}>Stop</button>
    {:else if stopped && s && done}
      <div class="strip">
        <span class="what">Scan stopped · showing what was found</span>
        <span class="badge review">Partial</span>
        <span class="counters mono">{fmtCount(done.progress.files)} files · {app.fb(done.progress.allocatedBytes)} found</span>
      </div>
      <div class="spacer"></div>
      <button class="btn small" onclick={() => app.startScan(s.root === app.volume?.mountPath ? undefined : s.root)}><Icon name="scan" />Scan again</button>
    {:else if showEnd && done}
      <div class="strip endline">
        <span class="ok"><Icon name="check" size={14} /></span>
        <span class="what">Scan complete · {fmtDuration(done.progress.elapsedMs)}{#if prevSnap}{" · "}<span class="mono">{app.fb(prevSnap.delta, true)}</span> since {fmtWhen(prevSnap.from, undefined, false)}{/if}</span>
        {#if done.warnings.length}<span class="faint">·</span><button class="linkbtn" onclick={() => (warnsOpen = true)}>{done.warnings.length} {done.warnings.length === 1 ? "folder" : "folders"} couldn't be read</button>{/if}
        {#if fellBack}<span class="faint">· Ran as {modeLabel(done.modeUsed)}</span>{/if}
        {#if prevSnap}<button class="btn small" onclick={() => app.go("grew")}>See what grew</button>{/if}
        <button class="icon-btn x" title="Dismiss" aria-label="Dismiss" onclick={() => (ended = null)}>✕</button>
      </div>
      <div class="spacer"></div>
    {:else}
      <div class="meta"><Icon name="clock" /><span>{app.lastScanAt ? `Scanned ${fmtWhen(app.lastScanAt)}` : "Not scanned yet"}</span></div>
      <div class="meta"><span class="mono">{app.fb(app.usedBytes)}</span><span class="faint">of {app.fb(v.totalBytes)} used</span></div>
      <div class="spacer"></div>
    {/if}
    {#if !running && !stopped}
      <button class="btn small" onclick={() => app.scanFolder()}><Icon name="folder" />Scan a folder…</button>
    {/if}
    <div class="drive-chip" role="group" aria-label="Drives">
      {#each app.volumes as x (x.id)}
        <button class:on={x.id === app.volumeId} disabled={running} title="{x.label} · {x.fileSystem}" onclick={() => pick(x.id)}>{x.label}</button>
      {/each}
    </div>
    {#if running}
      <div class="progress" aria-hidden="true"><i style:width="{app.scanPct * 100}%"></i></div>
    {/if}
  </div>
{/if}

<svelte:window onkeydown={(e) => warnsOpen && e.key === "Escape" && (warnsOpen = false)} />

{#if warnsOpen && done}
  <div class="modal-bg" role="presentation" onclick={(e) => e.target === e.currentTarget && (warnsOpen = false)}>
    <div class="card dlg" role="dialog" aria-modal="true" aria-labelledby="warns-title" tabindex="-1">
      <div class="card-head"><div class="ic"><Icon name="info" size={18} /></div><h2 id="warns-title">Couldn't be read</h2></div>
      <ul class="warns">
        {#each done.warnings as w, i (i)}
          <li>{#if w.path}<span class="mono">{w.path}</span> · {/if}{w.message}</li>
        {/each}
      </ul>
      <div class="foot"><button class="btn" onclick={() => (warnsOpen = false)}>Close</button></div>
    </div>
  </div>
{/if}

<style>
  .context {
    position: relative; overflow: hidden;
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
  .strip { display: flex; align-items: center; gap: 10px; min-width: 0; flex-wrap: wrap; }
  .what { font-size: var(--fs-sm); white-space: nowrap; }
  .counters { font-size: var(--fs-xs); color: var(--text-2); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .path { font-size: var(--fs-xs); color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 40%; min-width: 0; }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); flex: none; animation: pulse 1.4s infinite; }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 60%, transparent); } 100% { box-shadow: 0 0 0 8px transparent; } }
  .ok { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; background: color-mix(in srgb, var(--lime) 18%, transparent); color: var(--lime); flex: none; }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--text-2); font-size: var(--fs-sm); text-decoration: underline; text-underline-offset: 3px; }
  .linkbtn:hover { color: var(--text); }
  .x { width: 28px; height: 28px; font-size: 12px; }
  .progress { position: absolute; left: 0; right: 0; bottom: 0; height: 3px; }
  .progress i { display: block; height: 100%; background: var(--accent); transition: width .3s var(--ease); }
  .drive-chip { display: flex; gap: 4px; padding: 4px; background: var(--surface-2); border-radius: var(--r-pill); flex-wrap: wrap; }
  .drive-chip button { border: 0; background: none; padding: 6px 14px; border-radius: var(--r-pill); color: var(--text-2); }
  .drive-chip button.on { background: var(--surface-3); color: var(--text); }
  .drive-chip button:disabled { opacity: .5; cursor: default; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.55); display: grid; place-items: center; z-index: 65; padding: 16px; }
  .dlg { width: min(520px, 100%); }
  .warns { margin: 0; padding-left: 18px; font-size: var(--fs-sm); color: var(--text-2); max-height: 50vh; overflow: auto; }
  .warns li { margin: 4px 0; overflow-wrap: anywhere; }
  .foot { display: flex; justify-content: flex-end; margin-top: 16px; }
  @media (max-width: 1100px) { .path { display: none; } }
</style>
