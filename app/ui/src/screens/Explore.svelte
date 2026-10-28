<script lang="ts">
  import type { Filter } from "../ipc/backend";
  import type { Category, MainView, NodeDetailDto, NodeDto, NodeId } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { fmtAge, fmtCount, fmtPct } from "../lib/format";
  import { CATEGORIES, CATEGORY_LABEL, CLOUD_STATE, PROVIDER, catVar } from "../lib/labels";
  import type { IconName } from "../lib/icons";
  import { size, type VizNode } from "../lib/viz";
  import Icon from "../components/Icon.svelte";
  import FolderTree from "../components/FolderTree.svelte";
  import MainViz from "../components/MainViz.svelte";
  import Legend from "../components/Legend.svelte";
  import VirtualList from "../components/VirtualList.svelte";

  const VIEWS: [MainView, string, IconName][] = [
    ["blockMap", "Block map", "blocks"], ["ringChart", "Ring chart", "ring"], ["fileTree", "File tree", "treeIc"], ["terrain", "Terrain 3D", "mountain"],
  ];
  const rootId = $derived(app.root?.id ?? 0);
  let viewRoot = $state<NodeId | null>(null);
  const zoomRoot = $derived(viewRoot ?? rootId);
  let selPath = $state.raw<NodeId[] | null>(null);
  let crumbs = $state.raw<NodeDto[]>([]);
  let detail = $state.raw<NodeDetailDto | null>(null);
  let flat = $state(false);
  let under = $state(false);
  $effect(() => { flat = app.platform === "linux" || app.settings.terrain.prefer2d; });

  // Selection → path, crumbs and details.
  $effect(() => {
    void app.version;
    const id = app.selected ?? zoomRoot;
    let stale = false;
    Promise.all([app.backend.pathTo(id), app.backend.node(id)])
      .then(([p, d]) => {
        if (stale) return;
        crumbs = p;
        selPath = app.selected == null ? null : p.map((x) => x.id);
        detail = d;
      })
      .catch(() => { if (!stale) app.selected = null; });
    return () => { stale = true; };
  });

  function select(id: NodeId) {
    app.selected = id;
  }
  function open(n: VizNode) {
    if (n.kind === "dir" && n.id >= 0 && (n.childCount ?? n.children?.length ?? 0) > 0) {
      viewRoot = n.id;
      app.selected = n.id;
    }
  }

  // ---- Filters ----
  let text = $state("");
  let hidden = $state.raw(new Set<Category>());
  let minBytes = $state(0);
  let olderThan = $state(0);
  let results = $state.raw<NodeDto[] | null>(null);
  let searching = $state(false);
  let searchEl: HTMLInputElement | undefined = $state();
  const filter = $derived<Filter | null>(
    text.trim() || hidden.size || minBytes || olderThan
      ? {
          text: text.trim() || undefined,
          categories: hidden.size ? CATEGORIES.filter((c) => !hidden.has(c)) : undefined,
          minBytes: minBytes || undefined,
          olderThanDays: olderThan || undefined,
        }
      : null,
  );
  $effect(() => {
    const f = filter;
    void app.version;
    if (!f) { results = null; return; }
    searching = true;
    const t = setTimeout(() => app.backend.search($state.snapshot(f) as Filter, 1000).then((r) => { results = r; searching = false; }), 180);
    return () => clearTimeout(t);
  });
  function toggleCat(c: Category) {
    const s = new Set(hidden);
    if (s.has(c)) s.delete(c); else s.add(c);
    hidden = s;
  }
  $effect(() => {
    const focus = () => searchEl?.focus();
    document.addEventListener("lustro-focus-search", focus);
    return () => document.removeEventListener("lustro-focus-search", focus);
  });

  const d = $derived(detail);
  const cloudOnly = $derived(!!d && d.allocated === 0 && d.cloudOnly > 0);
  const cloudNote = $derived.by(() => {
    if (!d?.cloud) return null;
    const p = PROVIDER[d.cloud.provider].label;
    return {
      cloudOnly: `Stored in ${p} only. It is listed here but uses no space on this disk, and Lustro never opens it (that would download it).`,
      downloaded: `A ${p} file that has been downloaded. It uses space here like any local file, and you can free it back to cloud-only from Cleanup.`,
      pinned: `Set to “Always keep on this device” in ${p}. It uses space here and stays available offline.`,
    }[d.cloud.state];
  });

  async function copyPath() {
    if (!d) return;
    try {
      await navigator.clipboard.writeText(d.path);
      app.say("Path copied");
    } catch {
      app.say("Couldn't copy the path");
    }
  }
</script>

<div class="filters">
  <label class="search">
    <Icon name="search" /><input bind:this={searchEl} bind:value={text} placeholder="Search files and folders" aria-label="Search files and folders" />
    {#if text}<button class="clear" onclick={() => (text = "")} aria-label="Clear search">✕</button>{:else}<kbd>{app.kbd("F")}</kbd>{/if}
  </label>
  <div class="chips">
    {#each CATEGORIES.slice(0, 8) as c (c)}
      <button class="chip on" class:off={hidden.has(c)} aria-pressed={!hidden.has(c)} onclick={() => toggleCat(c)}><span class="dot" style:background="var({catVar(c)})"></span>{CATEGORY_LABEL[c]}</button>
    {/each}
  </div>
  <select class="chip" bind:value={minBytes} aria-label="Minimum size">
    <option value={0}>Any size</option><option value={1e8}>Size ≥ 100 MB</option><option value={1e9}>Size ≥ 1 GB</option><option value={1e10}>Size ≥ 10 GB</option>
  </select>
  <select class="chip" bind:value={olderThan} aria-label="Last modified">
    <option value={0}>Modified: any time</option><option value={180}>Not modified in 6 months</option><option value={365}>Not modified in a year</option><option value={730}>Not modified in 2 years</option>
  </select>
</div>

<div class="grid ex">
  <section class="card">
    {#if results}
      <div class="card-head"><div class="ic"><Icon name="search" size={18} /></div><h2>Results</h2><span class="sub">{searching ? "searching…" : `${fmtCount(results.length)}${results.length === 1000 ? "+" : ""} by size`}</span></div>
      <div class="body">
        <VirtualList items={results} rowHeight={44} label="Search results">
          {#snippet row(n: NodeDto)}
            <button class="res" class:sel={n.id === app.selected} onclick={() => select(n.id)}>
              <span class="dot" style:background="var({catVar(n.category)})"></span>
              <span class="min0"><span class="name">{n.name}</span><span class="faint xs">{n.kind === "dir" ? `${fmtCount(n.fileCount)} files` : CATEGORY_LABEL[n.category]} · {fmtAge(n.modified)}</span></span>
              <span class="mono muted xs">{app.fb(size(n))}</span>
            </button>
          {/snippet}
        </VirtualList>
        {#if !results.length && !searching}<p class="faint none">Nothing matches.</p>{/if}
      </div>
    {:else}
      <div class="card-head"><div class="ic"><Icon name="treeIc" size={18} /></div><h2>Folders</h2><span class="sub">by size</span></div>
      <div class="body"><FolderTree rootId={rootId} {selPath} onSelect={(n) => select(n.id)} /></div>
    {/if}
  </section>

  <section class="card">
    <div class="card-head">
      <div class="ic"><Icon name={VIEWS.find((v) => v[0] === app.mainView)?.[2] ?? "blocks"} size={18} /></div>
      <h2>{VIEWS.find((v) => v[0] === app.mainView)?.[1]}</h2>
      <div class="tools">
        <div class="seg" role="group" aria-label="View">
          {#each VIEWS as [v, l, ic] (v)}
            <button class:on={app.mainView === v} aria-pressed={app.mainView === v} onclick={() => (app.exploreView = v)}><Icon name={ic} size={14} />{l}</button>
          {/each}
        </div>
      </div>
    </div>
    <div class="crumbs">
      {#each crumbs as c, i (c.id)}
        {#if i}<span>›</span>{/if}
        <button class:here={c.id === zoomRoot} onclick={() => select(c.id)} ondblclick={() => c.kind === "dir" && (viewRoot = c.id)}>{c.name}</button>
      {/each}
      <span class="zoom">
        {#if zoomRoot !== rootId}<button class="btn small" onclick={() => (viewRoot = crumbs.find((c) => c.id === zoomRoot)?.parent ?? null)}><Icon name="up" size={12} />Up a level</button>{/if}
        {#if d && d.kind === "dir" && d.id !== zoomRoot && d.childCount > 0}<button class="btn small" onclick={() => (viewRoot = d.id)}><Icon name="zoomIn" size={12} />Zoom in</button>{/if}
      </span>
    </div>
    <div class="body viz">
      {#if app.mainView === "terrain"}
        <div class="seg tmodes">
          <button class:on={!flat && !under} onclick={() => { flat = false; under = false; }}><Icon name="mountain" size={14} />3D</button>
          <button class:on={flat} onclick={() => { flat = true; under = false; }}><Icon name="map" size={14} />Top-down</button>
          <button class:on={under} onclick={() => { under = true; flat = false; }}><Icon name="lake" size={14} />Look under</button>
        </div>
        {#if flat && app.platform === "linux"}
          <div class="perf-note">Switched to 2D for smoother performance <button class="btn small" onclick={() => (flat = false)}>Try 3D anyway</button></div>
        {/if}
      {/if}
      <MainViz rootId={zoomRoot} view={app.mainView} {selPath} {flat} {under} follow onSelect={select} onOpen={open} onAutoFlat={() => (flat = true)} />
    </div>
    <div class="legend-row"><Legend /><span class="faint xs">Click to select · double-click a folder to zoom in</span></div>
  </section>

  <section class="card det">
    {#if d}
      <div class="card-head">
        <div class="ic"><Icon name={d.kind === "dir" ? "folder" : "info"} size={18} /></div><h2>Details</h2>
        {#if d.accessDenied}<span class="badge review">Needs admin</span>{/if}
        {#if d.hidden}<span class="badge">Hidden</span>{/if}
      </div>
      {#if d.cloud}<div class="cb"><span class="badge cloud">{PROVIDER[d.cloud.provider].label} · {CLOUD_STATE[d.cloud.state].label}</span></div>{/if}
      <h3>{d.name}</h3>
      <div class="mono faint xs brk">{d.path}</div>
      <div class="kv">
        <span>Size</span><span>{app.fb(d.logical)}</span>
        <span>On this disk</span><span>{app.fb(d.allocated)}</span>
        {#if cloudOnly || d.cloudOnly > 0}<span>Cloud only</span><span>{app.fb(d.cloudOnly)}</span>{/if}
        {#if d.kind === "dir"}<span>Items</span><span>{fmtCount(d.fileCount + d.dirCount)}</span>{:else}<span>Type</span><span>{CATEGORY_LABEL[d.category]}</span>{/if}
        {#if app.root}<span>% of {app.volume?.label}</span><span>{fmtPct(size(d) / Math.max(1, size(app.root)))}</span>{/if}
        <span>Since last scan</span><span class:up={(d.delta ?? 0) > 0} class:down={(d.delta ?? 0) < 0}>{d.delta ? app.fb(d.delta, true) : "no change"}</span>
        <span>Modified</span><span>{fmtAge(d.modified)}</span>
      </div>
      {#if cloudNote}<div class="muted xs">{cloudNote}</div>{/if}
      {#if d.accessDenied}<div class="muted xs">Lustro can't look inside without admin rights. Its size comes from the volume. Fast scan can read it.</div>{/if}
      {#if d.kind === "dir" && d.categoryMix.length}
        <div class="sec-t">Category mix</div>
        <div class="bar mix">
          {#each d.categoryMix as m (m.category)}<span title={CATEGORY_LABEL[m.category]} style:width="{(m.allocated / Math.max(1, size(d))) * 100}%" style:background="var({catVar(m.category)})"></span>{/each}
        </div>
        <div class="sec-t">Largest inside</div>
        <div class="list">
          {#each d.largestChildren as k (k.id)}
            <button class="row kid" onclick={() => select(k.id)}>
              <span class="dot" class:cloud={k.allocated === 0 && k.cloudOnly > 0} style:background={k.allocated === 0 && k.cloudOnly > 0 ? undefined : `var(${catVar(k.category)})`}></span>
              <span class="name">{k.name}</span><span class="mono muted">{app.fb(size(k))}</span>
            </button>
          {/each}
        </div>
      {/if}
      <div class="actions">
        <button class="btn" onclick={() => app.backend.reveal(d.id).then(() => app.say(`Opening in ${app.w.fileManager}…`))}><Icon name="folder" />{app.w.reveal}</button>
        <button class="btn" onclick={copyPath}><Icon name="copy" />Copy path</button>
        <button class="btn" onclick={() => app.go("cleanup")}><Icon name="basket" />Review in Cleanup</button>
      </div>
    {/if}
  </section>
</div>

<style>
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: var(--sp-4); }
  .search { display: flex; align-items: center; gap: 8px; background: var(--surface-1); border: 1px solid var(--stroke); border-radius: var(--r-pill); padding: 7px 14px; min-width: 260px; color: var(--text-3); }
  .search:focus-within { border-color: var(--accent); }
  .search input { background: none; border: 0; outline: 0; color: var(--text); font: inherit; flex: 1; min-width: 0; }
  .clear { border: 0; background: none; color: var(--text-3); padding: 0; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .ex { grid-template-columns: 320px minmax(0, 1fr) 300px; height: calc(100vh - 220px); min-height: 560px; }
  .ex .card { display: flex; flex-direction: column; min-height: 0; }
  .body { flex: 1; min-height: 0; position: relative; }
  .crumbs { display: flex; gap: 4px; flex-wrap: wrap; align-items: center; font-size: var(--fs-xs); color: var(--text-3); margin: -6px 0 10px; min-height: 26px; }
  .crumbs > button { border: 0; background: none; color: var(--text-2); padding: 0; font-size: inherit; }
  .crumbs > button:hover { color: var(--text); }
  .crumbs > button.here { color: var(--text); font-weight: var(--fw-strong); }
  .zoom { margin-left: auto; display: flex; gap: 6px; }
  .zoom .btn { padding: 3px 10px; }
  .tmodes { position: absolute; top: 10px; left: 10px; z-index: 5; }
  .perf-note {
    position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); z-index: 8; display: flex; gap: 10px; align-items: center;
    background: var(--surface-2); border: 1px solid var(--stroke-strong); border-radius: var(--r-pill); padding: 5px 6px 5px 14px;
    font-size: var(--fs-xs); color: var(--text-2); box-shadow: var(--shadow); white-space: nowrap;
  }
  .legend-row { margin-top: 12px; display: flex; gap: 12px; justify-content: space-between; align-items: center; flex-wrap: wrap; }
  .res { display: grid; grid-template-columns: 10px minmax(0, 1fr) auto; gap: 10px; align-items: center; width: 100%; height: 42px; padding: 0 8px; border: 0; background: none; color: inherit; text-align: left; border-radius: 8px; }
  .res:hover { background: var(--surface-2); }
  .res.sel { background: var(--surface-3); }
  .min0 { min-width: 0; display: grid; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { position: absolute; inset: 0; text-align: center; padding-top: 40px; }
  .xs { font-size: var(--fs-xs); }
  .det { overflow-y: auto; }
  .det > * { flex-shrink: 0; }
  .det h3 { margin: 0 0 2px; font-size: var(--fs-lg); word-break: break-all; }
  .cb { margin-bottom: 8px; }
  .brk { word-break: break-all; }
  .kv { display: grid; grid-template-columns: 1fr auto; gap: 6px 10px; font-size: var(--fs-sm); margin: 14px 0; }
  .kv span:nth-child(odd) { color: var(--text-2); }
  .kv span:nth-child(even) { font-family: var(--font-mono); text-align: right; }
  .bar.mix { height: 10px; }
  .kid { grid-template-columns: 10px minmax(0, 1fr) auto; padding: 5px 4px; cursor: pointer; }
  .actions { display: grid; gap: 8px; margin-top: auto; padding-top: 14px; }
  .actions .btn { justify-content: center; }
  @media (max-width: 1200px) { .ex { grid-template-columns: 1fr; height: auto; } .ex .card { min-height: 420px; } }
</style>
