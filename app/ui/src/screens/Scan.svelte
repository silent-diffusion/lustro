<script lang="ts">
  import type { NodeId } from "../ipc/types";
  import { app, type LiveNode } from "../lib/app.svelte";
  import { fmtCount, fmtDuration, fmtWhen } from "../lib/format";
  import { catVar } from "../lib/labels";
  import type { VizNode } from "../lib/viz";
  import Icon from "../components/Icon.svelte";
  import CanvasMap from "../components/CanvasMap.svelte";
  import CloudMark from "../components/CloudMark.svelte";
  import Legend from "../components/Legend.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import { untrack } from "svelte";

  const s = $derived(app.scan);
  const done = $derived(s?.done ?? null);
  /** The mode that actually ran: Fast scan can fall back to Standard (permission declined, helper unavailable). */
  const mode = $derived(done && !done.cancelled ? done.modeUsed : s?.mode);
  const used = $derived(app.volume ? app.volume.totalBytes - app.volume.freeBytes : 0);
  const pct = $derived(!s ? 0 : done && !done.cancelled ? 1 : Math.min(0.99, s.progress.allocatedBytes / Math.max(1, used)));
  const sec = $derived((s?.progress.elapsedMs ?? 0) / 1000);

  // Highlight the branch being read right now.
  const hot = $derived.by(() => {
    void s?.tick;
    const set = new Set<NodeId>();
    if (!s || done) return set;
    for (let x = s.busiest; x != null; x = s.nodes.get(x)?.parent ?? null) set.add(x);
    return set;
  });
  const current = $derived.by(() => {
    void s?.tick;
    if (!s) return "";
    if (done) return done.cancelled ? "Stopped. The previous snapshot is unchanged." : `Saved as snapshot · ${fmtWhen(Math.floor(Date.now() / 1000))}`;
    const names: string[] = [];
    for (let x = s.busiest; x != null; ) {
      const n = s.nodes.get(x);
      if (!n) break;
      names.unshift(n.name);
      x = n.parent;
    }
    const sep = app.platform === "windows" ? "\\" : "/";
    return names.length ? names[0] + names.slice(1).join(sep) : "";
  });

  // Sizes are copied into the row: LiveNode objects are mutated in place, so reading them through
  // the same object reference wouldn't tell Svelte that anything changed.
  interface Row { n: LiveNode; depth: number; parentSize: number; size: number; category: LiveNode["category"]; cloudOnly: boolean; open: boolean; hot: boolean; fresh: boolean }
  const rows = $derived.by(() => {
    void s?.tick;
    const out: Row[] = [];
    if (!s || s.rootId == null) return out;
    const walk = (id: NodeId, depth: number, parentSize: number) => {
      for (const c of s.children(id)) {
        const n = s.nodes.get(c)!;
        const size = n.allocated + n.cloudOnly;
        out.push({ n, depth, parentSize, size, category: n.category, cloudOnly: n.allocated === 0 && n.cloudOnly > 0, open: s.open.has(c), hot: hot.has(c), fresh: s.tick - n.born < 3 && !done });
        if (s.open.has(c)) walk(c, depth + 1, n.allocated + n.cloudOnly || 1);
      }
    };
    const r = s.nodes.get(s.rootId)!;
    walk(s.rootId, 0, r.allocated + r.cloudOnly || 1);
    return out;
  });

  // Block map of the partial tree, rebuilt twice a second (three levels deep, small items folded away).
  let mapTree = $state.raw<VizNode | null>(null);
  function build() {
    const scan = app.scan;
    if (!scan || scan.rootId == null) return;
    const root = scan.nodes.get(scan.rootId)!;
    const min = (root.allocated + root.cloudOnly) * 0.0005;
    const mk = (n: LiveNode, depth: number): VizNode => ({
      id: n.id, name: n.name, kind: n.kind, allocated: n.allocated, cloudOnly: n.cloudOnly, category: n.category, cloud: n.cloud, delta: null,
      children: depth >= 3 ? [] : scan.children(n.id).map((c) => scan.nodes.get(c)!).filter((c) => c.allocated + c.cloudOnly >= min).map((c) => mk(c, depth + 1)),
    });
    mapTree = mk(root, 0);
  }
  $effect(() => {
    if (!s || done) return;
    const t = setInterval(build, 500);
    return () => clearInterval(t);
  });
  $effect(() => {
    void s;
    void done;
    untrack(build);
  });

  function toggle(id: NodeId) {
    if (!s) return;
    if (s.open.has(id)) s.open.delete(id); else s.open.add(id);
    s.tick++;
  }
</script>

{#if !s}
  <section class="card idle">
    <h2>No scan running</h2>
    <p class="muted">{app.w.standardScan} {app.w.fastScan.split(".")[0]}.</p>
    <button class="btn primary" onclick={() => app.startScan()}><Icon name="scan" />Scan {app.volume?.label}</button>
  </section>
{:else}
  <section class="card top" class:finished={!!done}>
    <div class="card-head">
      <div class="ic"><span class="pulse"></span></div>
      <h2>{done ? (done.cancelled ? "Scan stopped" : "Scan complete") : "Scanning"} · {s.root}</h2>
      <span class="sub mode"><span class="badge">{mode === "fast" ? "Fast scan" : "Standard scan"}</span><span class="faint">{mode === "fast" ? app.w.fastScan.split(".")[0] + "." : app.w.standardScan}</span></span>
      <div class="tools">
        {#if done && !done.cancelled}
          <button class="btn" onclick={() => app.go("grew")}>See what grew</button>
        {:else if !done}
          <button class="btn" onclick={() => app.backend.cancelScan(s.id)}>Stop</button>
        {:else}
          <button class="btn" onclick={() => app.startScan()}>Scan again</button>
        {/if}
        <button class="btn primary" onclick={() => app.go("explore")}>{done ? "Open in Explore" : "Explore now"}</button>
      </div>
    </div>
    <div class="counters">
      <div><div class="k">Files</div><div class="v">{fmtCount(s.progress.files)}</div></div>
      <div><div class="k">Folders</div><div class="v">{fmtCount(s.progress.dirs)}</div></div>
      <div><div class="k">Found</div><div class="v">{app.fb(s.progress.allocatedBytes)}</div></div>
      <div><div class="k">Elapsed</div><div class="v">{fmtDuration(s.progress.elapsedMs)}</div></div>
      <div><div class="k">Speed</div><div class="v">{sec > 0.2 ? `${fmtCount(s.progress.files / sec)} files/s` : "–"}</div></div>
    </div>
    <div class="progress" role="progressbar" aria-valuenow={Math.round(pct * 100)} aria-valuemin={0} aria-valuemax={100}><i style:width="{pct * 100}%"></i></div>
    <div class="current">{current}</div>
    {#if done && !done.cancelled && done.modeUsed !== s.mode}
      <div class="warn fell"><Icon name="info" size={14} />Ran as {done.modeUsed === "fast" ? "Fast scan" : "Standard scan"}.</div>
    {/if}
    {#if done?.warnings.length}
      <ul class="warns">
        {#each done.warnings as w, i (i)}
          <li>{#if w.path}<span class="mono">{w.path}</span> · {/if}{w.message}</li>
        {/each}
      </ul>
    {/if}
  </section>

  <div class="grid scan">
    <section class="card">
      <div class="card-head"><div class="ic"><Icon name="treeIc" size={18} /></div><h2>Folder tree</h2><span class="sub">{done ? "measured" : "building live"}</span></div>
      <div class="body">
        <VirtualList items={rows} rowHeight={30} label="Folder tree">
          {#snippet row(r: Row)}
            {@const n = r.n}
            <button class="t-row" class:hot={r.hot} class:fresh={r.fresh} onclick={() => toggle(n.id)}>
              <span class="t-name" style:padding-left="{r.depth * 14}px">
                <span class="caret" class:open={r.open}>{n.kind === "dir" ? "›" : ""}</span>
                <span class="dot" class:cloud={r.cloudOnly} style:background={r.cloudOnly ? undefined : `var(${catVar(r.category)})`}></span>
                <span class="nm">{n.name}</span>
                {#if n.cloud && n.kind !== "dir"}<CloudMark cloud={n.cloud} mini />{/if}
              </span>
              <span class="pct"><i style:width="{Math.min(100, (r.size / r.parentSize) * 100)}%" style:background={r.cloudOnly ? "var(--cat-cloud)" : `var(${catVar(r.category)})`}></i></span>
              <span class="t-size">{app.fb(r.size)}</span>
            </button>
          {/snippet}
        </VirtualList>
      </div>
    </section>
    <section class="card">
      <div class="card-head"><div class="ic"><Icon name="blocks" size={18} /></div><h2>Block map</h2><span class="sub">fills in as folders are measured</span></div>
      <div class="body"><CanvasMap tree={mapTree} maxDepth={3} /></div>
      <div class="legend-row"><Legend /></div>
    </section>
  </div>
{/if}

<style>
  .idle { text-align: center; padding: 50px 20px; display: grid; justify-items: center; gap: 6px; }
  .idle h2 { margin: 0; }
  .top { margin-bottom: var(--sp-4); }
  .mode { display: flex; gap: 10px; align-items: center; }
  .counters { display: grid; grid-template-columns: repeat(5, auto) 1fr; gap: var(--sp-8); align-items: end; }
  .counters .k { font-size: var(--fs-xs); color: var(--text-3); text-transform: uppercase; letter-spacing: .06em; }
  .counters .v { font-size: var(--fs-xl); font-weight: var(--fw-strong); font-family: var(--font-mono); white-space: nowrap; }
  .progress { height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; margin-top: 14px; }
  .progress i { display: block; height: 100%; background: var(--accent); transition: width .3s var(--ease); }
  .current { font-family: var(--font-mono); font-size: var(--fs-xs); color: var(--text-3); margin-top: 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-height: 1.4em; }
  .warn { font-size: var(--fs-xs); margin-top: 6px; display: flex; gap: 6px; align-items: center; }
  .warn.fell { color: var(--amber); }
  .warns { margin: 6px 0 0; padding-left: 18px; font-size: var(--fs-xs); color: var(--text-3); max-height: 96px; overflow: auto; }
  .warns li { margin: 2px 0; overflow-wrap: anywhere; }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); animation: pulse 1.4s infinite; }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 60%, transparent); } 100% { box-shadow: 0 0 0 10px transparent; } }
  .finished .pulse { animation: none; background: var(--lime); }
  .scan { grid-template-columns: 420px minmax(0, 1fr); height: calc(100vh - 330px); min-height: 520px; }
  .scan .card { display: flex; flex-direction: column; min-height: 0; }
  .body { flex: 1; min-height: 0; position: relative; }
  .legend-row { margin-top: 12px; }
  .t-row {
    display: grid; grid-template-columns: minmax(0, 1fr) 52px 62px; align-items: center; gap: 8px; padding: 0 8px; height: 28px;
    border-radius: 8px; width: 100%; border: 0; background: none; color: inherit; text-align: left; font: inherit; font-size: var(--fs-sm);
  }
  .t-row:hover { background: var(--surface-2); }
  .t-row.fresh { animation: grow-in .45s var(--ease) both; }
  .t-row.hot { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .t-row.hot .nm { color: var(--accent); }
  @keyframes grow-in { from { opacity: 0; transform: translateX(-8px); } to { opacity: 1; transform: none; } }
  .t-name { display: flex; align-items: center; gap: 6px; overflow: hidden; white-space: nowrap; min-width: 0; }
  .nm { overflow: hidden; text-overflow: ellipsis; }
  .caret { width: 14px; flex: none; color: var(--text-3); transition: transform .15s; display: inline-block; text-align: center; }
  .caret.open { transform: rotate(90deg); }
  .pct { height: 4px; border-radius: 2px; background: var(--surface-3); position: relative; overflow: hidden; }
  .pct i { position: absolute; inset: 0 auto 0 0; border-radius: 2px; transition: width .3s var(--ease); }
  .t-size { text-align: right; font-family: var(--font-mono); font-size: var(--fs-xs); color: var(--text-2); white-space: nowrap; }
  @media (max-width: 1100px) { .scan { grid-template-columns: 1fr; height: auto; } .scan .card { min-height: 420px; } .counters { grid-template-columns: repeat(3, 1fr); } }
</style>
