<script lang="ts">
  import type { Category, DiffDto, NodeDto, SpaceBreakdownDto, SuggestionDto, SuggestionGroup, TreeDto } from "../ipc/types";
  import type { PinKind, TerrainPin } from "../terrain/types";
  import { app } from "../lib/app.svelte";
  import { fmtMonthDay, fmtWhen } from "../lib/format";
  import { CATEGORY_LABEL, PROVIDER, catVar } from "../lib/labels";
  import type { IconName } from "../lib/icons";
  import Icon from "../components/Icon.svelte";
  import TerrainView from "../components/TerrainView.svelte";
  import Notes from "../components/Notes.svelte";

  let space = $state.raw<SpaceBreakdownDto | null>(null);
  let diff = $state.raw<DiffDto | null>(null);
  let sugg = $state.raw<SuggestionDto[]>([]);
  let tree = $state.raw<TreeDto | null>(null);
  let pinSrc = $state.raw<{ largest: NodeDto | null; cloud: NodeDto | null }>({ largest: null, cloud: null });

  $effect(() => {
    void app.version;
    const b = app.backend;
    const root = app.root;
    if (!root) return;
    b.spaceBreakdown(app.volumeId).then((s) => (space = s));
    b.listSnapshots(root.name).then(async (snaps) => {
      diff = snaps.length > 1 ? await b.diff(snaps[snaps.length - 2].id, snaps[snaps.length - 1].id, 50) : null;
    });
    b.suggestions().then((s) => (sugg = s));
    b.subtree(root.id, 8, 0.0002).then((t) => (tree = t));
    Promise.all([b.top("files", 1, {}), b.search({ cloud: "cloudOnly", minBytes: 1e9 }, 1)]).then(([l, c]) => (pinSrc = { largest: l[0] ?? null, cloud: c[0] ?? null }));
  });

  const vol = $derived(app.volume);
  const used = $derived(space?.usedBytes ?? 0);
  const cats = $derived(space?.segments.filter((s) => s.kind === "category") ?? []);
  const sys = $derived(space?.segments.filter((s) => s.kind === "system") ?? []);
  const cloudSeg = $derived(space?.segments.find((s) => s.kind === "cloudOnly"));
  const unexplained = $derived(space?.segments.find((s) => s.kind === "unexplained"));
  // The type list counts system items under System again, so it matches the bar.
  const byType = $derived.by(() => {
    const m = new Map<Category, number>();
    for (const s of cats) m.set(s.key as Category, s.bytes);
    m.set("system", (m.get("system") ?? 0) + sys.reduce((a, s) => a + s.bytes, 0));
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  });
  const maxType = $derived(Math.max(1, ...byType.map((x) => x[1])));

  const growers = $derived(diff?.entries.filter((e) => e.delta > 0 && e.status !== "deleted").slice(0, 3) ?? []);
  const trend = $derived(diff?.trend ?? []);
  const lo = $derived(trend.length ? Math.min(...trend.map((t) => t.usedBytes)) - 6e9 : 0);
  const hi = $derived(vol ? vol.totalBytes * 0.9 : 1);

  const sumGroup = (g: SuggestionGroup) => sugg.filter((s) => s.group === g).reduce((a, s) => a + s.bytes, 0);
  const insights = $derived<[string, number, string, IconName][]>([
    ["Duplicates", sumGroup("duplicates"), `${sugg.filter((s) => s.group === "duplicates").length} sets, hash-verified`, "twins"],
    ["Old downloads", sumGroup("oldDownloads") + sumGroup("staleLarge"), "installers & archives > 6 mo", "clock"],
    ["Dev caches", sumGroup("devCaches"), "rebuildable on demand", "treeIc"],
    ["Temp files", sumGroup("tempFiles"), "safe to clear", "cleanup"],
  ]);

  // Terrain pins.
  let pinOn = $state<Record<PinKind, boolean>>({ largest: true, grew: true, duplicate: true, cloud: true });
  let flat = $state(false);
  let under = $state(false);
  $effect(() => { flat = app.platform === "linux" || app.settings.terrain.prefer2d; });
  const pins = $derived.by(() => {
    const out: TerrainPin[] = [];
    if (pinOn.largest && pinSrc.largest) out.push({ kind: "largest", nodeId: pinSrc.largest.id, label: `${pinSrc.largest.name} · ${app.fb(pinSrc.largest.allocated)}` });
    if (pinOn.grew) for (const g of growers.slice(0, 2)) if (g.id != null) out.push({ kind: "grew", nodeId: g.id, label: `${g.name} ${app.fb(g.delta, true)}` });
    if (pinOn.duplicate) for (const d of sugg.filter((s) => s.group === "duplicates")) out.push({ kind: "duplicate", nodeId: d.nodeId, label: `${d.name} · duplicate` });
    if (pinOn.cloud && pinSrc.cloud) out.push({ kind: "cloud", nodeId: pinSrc.cloud.id, label: `${pinSrc.cloud.name} · cloud only` });
    return out.filter((p) => {
      const t = app.settings.terrain.pins;
      return p.kind === "largest" ? t.largest : p.kind === "grew" ? t.grew : p.kind === "duplicate" ? t.duplicates : t.cloud;
    });
  });
  const PIN_BTNS: [PinKind, string, IconName][] = [["largest", "Largest", "flag"], ["grew", "Growers", "arrowUp"], ["duplicate", "Dupes", "twins"], ["cloud", "Cloud", "cloud"]];
</script>

{#if app.whatsNew}
  <section class="card whatsnew">
    <div class="card-head">
      <div class="ic"><Icon name="spark" size={18} /></div><h2>Updated to {app.whatsNew.version}</h2><span class="sub">Here's what changed</span>
      <div class="tools"><button class="icon-btn" title="Dismiss" aria-label="Dismiss" onclick={() => (app.whatsNew = null)}>✕</button></div>
    </div>
    <Notes src={app.whatsNew.notes} columns />
  </section>
{/if}

<div class="grid ov">
  <div class="col">
    <section class="card">
      <div class="card-head"><div class="ic"><Icon name="pie" size={18} /></div><h2>Where did my space go?</h2></div>
      {#if space && vol}
        <div class="stat-big">{(used / 1e9).toFixed(0)}<small>GB used</small></div>
        <div class="muted lead"><span class="mono">{app.fb(vol.freeBytes)}</span> free of {app.fb(vol.totalBytes)} · every gigabyte accounted for below</div>
        <div class="bar big" role="img" aria-label="Used space by type">
          {#each cats as s (s.key)}<span title={CATEGORY_LABEL[s.key as Category]} style:width="{(s.bytes / vol.totalBytes) * 100}%" style:background="var({catVar(s.key as Category)})"></span>{/each}
          {#each sys as s, i (i)}<span title={s.label} style:width="{(s.bytes / vol.totalBytes) * 100}%" style:background="var(--cat-system)"></span>{/each}
          {#if unexplained}<span class="unexplained" title="Unexplained" style:width="{(unexplained.bytes / vol.totalBytes) * 100}%"></span>{/if}
        </div>
        <div class="list space-list">
          {#if app.w.spaceLead}<div class="faint lead2">{app.w.spaceLead}</div>{/if}
          {#each sys as s, i (i)}
            <div class="row">
              <span class="dot" style:background="var(--cat-system)"></span><span>{s.label}</span><span class="mono muted">{app.fb(s.bytes)}</span>
              <span class="exp">{s.explanation}{#if s.settingAction} <button class="link" onclick={() => app.backend.openSetting(s.settingAction!).then(() => app.say("Opening the system setting…"))}>Open setting</button>{/if}</span>
            </div>
          {/each}
          {#if cloudSeg}
            <div class="row"><span class="dot cloud"></span><span>{cloudSeg.label}</span><span class="mono muted">{app.fb(cloudSeg.bytes)}</span><span class="exp">{cloudSeg.explanation}</span></div>
          {/if}
          {#if unexplained}
            <div class="row"><span class="dot" style:background="var(--text-3)"></span><span>Unexplained</span><span class="mono muted">{app.fb(unexplained.bytes)}</span><span class="exp">{app.w.unexplained}</span></div>
          {/if}
        </div>
      {/if}
    </section>

    <section class="card">
      <div class="card-head">
        <div class="ic"><Icon name="grew" size={18} /></div><h2>Since last scan</h2>
        {#if diff}<span class="sub">vs {fmtWhen(diff.from.takenAt)}</span>{/if}
        <div class="tools"><button class="btn small" onclick={() => app.go("grew")}>What grew →</button></div>
      </div>
      {#if diff}
        <div class="net"><div class="stat-big" class:up={diff.netDelta > 0} class:down={diff.netDelta < 0}>{app.fb(diff.netDelta, true)}</div><span class="muted">net change</span></div>
        <div class="hist">
          {#each trend as t, i (i)}
            <div><i class:last={i === trend.length - 1} style:height="{Math.max(4, ((t.usedBytes - lo) / (hi - lo)) * 100)}%" title={app.fb(t.usedBytes)}></i><span>{fmtMonthDay(t.takenAt)}</span></div>
          {/each}
        </div>
        <div class="list growers">
          {#each growers as g (g.path)}
            <button class="row" onclick={() => g.id != null && app.go("explore", g.id)}>
              <div class="min0"><div class="name">{g.name}</div><div class="path mono">{g.path}</div></div>
              <span class="mono up">{app.fb(g.delta, true)}</span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="faint">Scan again later to see what changed.</p>
      {/if}
    </section>

    <section class="card">
      <div class="card-head"><div class="ic"><Icon name="cloud" size={18} /></div><h2>Cloud storage</h2><span class="sub">all providers, same rules</span></div>
      <div class="list cloud-list">
        {#each app.cloud as c (c.provider + c.rootPath)}
          {@const local = c.localBytes + c.pinnedBytes}
          {@const tot = local + c.cloudOnlyBytes}
          <div class="row">
            <span class="pv">{PROVIDER[c.provider].short}</span>
            <div class="min0">
              <div>{c.displayName} {#if c.remote && !c.virtualVolumeId}<span class="badge">remote mount</span>{/if}</div>
              {#if c.virtualVolumeId}
                <div class="sub2">{c.virtualVolumeId} virtual drive (Stream) · cache in <span class="mono">{c.cacheRoot}</span></div>
              {:else}
                <div class="sub2 mono">{c.rootPath}</div>
              {/if}
              {#if c.remote && !c.virtualVolumeId}
                <div class="sub2 gap">Not scanned: reading a network drive is slow and may download files.</div>
              {:else if !c.cloudOnlyBytes}
                <div class="sub2 gap">Full local copies · {app.fb(local)} on this device</div>
              {:else}
                <div class="bar thin"><span style:width="{(local / tot) * 100}%" style:background="var(--cat-cloud)"></span><span class="hatch" style:width="{(c.cloudOnlyBytes / tot) * 100}%"></span></div>
                <div class="sub2 gap">{app.fb(local)} on this device{#if c.pinnedBytes} ({app.fb(c.pinnedBytes)} always kept){/if} · {app.fb(c.cloudOnlyBytes)} cloud only</div>
              {/if}
            </div>
            {#if c.remote && !c.virtualVolumeId}
              <button class="btn small" onclick={() => app.say("Scanning remote mounts isn't available in this preview.")}>Scan anyway</button>
            {:else}
              <span class="mono muted">{app.fb(local)}</span>
            {/if}
          </div>
        {:else}
          <p class="faint">No cloud sync folders found.</p>
        {/each}
      </div>
      <div class="legend">
        <span><span class="sw2" style:background="var(--cat-cloud)"></span>On this device</span>
        <span><span class="sw2 hatch"></span>Cloud only (not counted)</span>
      </div>
    </section>
  </div>

  <div class="col">
    <section class="card terrain-card">
      <div class="card-head">
        <div class="ic"><Icon name="route" size={18} /></div><h2>Terrain of {vol?.label}</h2><span class="sub accent">{flat ? "top-down map" : "3D view"}</span>
        <div class="tools">
          <div class="seg" role="group" aria-label="Pins">
            {#each PIN_BTNS as [k, l, ic] (k)}
              <button class:on={pinOn[k]} aria-pressed={pinOn[k]} onclick={() => (pinOn = { ...pinOn, [k]: !pinOn[k] })}><Icon name={ic} size={14} />{l}</button>
            {/each}
          </div>
          <button class="icon-btn" class:active={under} title="Look under: cloud-only valleys" aria-label="Look under" onclick={() => { under = !under; if (under) flat = false; }}><Icon name="lake" /></button>
          <button class="icon-btn" class:active={flat} title="Top-down map" aria-label="Top-down map" onclick={() => { flat = !flat; if (flat) under = false; }}><Icon name="map" /></button>
          <button class="icon-btn" title="Open in Explore" aria-label="Open in Explore" onclick={() => { app.exploreView = "terrain"; app.go("explore"); }}><Icon name="expand" /></button>
        </div>
      </div>
      <div class="terrain-box">
        <TerrainView {tree} {pins} compact {flat} {under} onSelect={(id) => { app.exploreView = "terrain"; app.go("explore", id); }} onAutoFlat={() => (flat = true)} />
      </div>
    </section>

    <div class="grid lower">
      <section class="card">
        <div class="card-head"><div class="ic"><Icon name="blocks" size={18} /></div><h2>By file type</h2><span class="sub">on disk</span></div>
        <div class="list">
          {#each byType as [c, v] (c)}
            <div class="row type-row">
              <span class="dot" style:background="var({catVar(c)})"></span><span>{CATEGORY_LABEL[c]}</span>
              <div class="bar thin"><span style:width="{(v / maxType) * 100}%" style:background="var({catVar(c)})"></span></div>
              <span class="mono muted r">{app.fb(v)}</span>
            </div>
          {/each}
        </div>
      </section>
      <section class="card">
        <div class="card-head">
          <div class="ic"><Icon name="spark" size={18} /></div><h2>Insights</h2><span class="sub">nothing is deleted from here</span>
          <div class="tools"><button class="btn small" onclick={() => app.go("cleanup")}>Review in Cleanup →</button></div>
        </div>
        <div class="ins">
          {#each insights as [t, v, s, ic] (t)}
            <button class="tile" onclick={() => app.go("cleanup")}>
              <span class="muted tl"><Icon name={ic} />{t}</span>
              {#if t === "Duplicates" && !app.settings.findDuplicates}
                <span class="v mono">Off</span><span class="faint xs">turn on in Settings → Scanning</span>
              {:else}
                <span class="v mono">{app.fb(v)}</span><span class="faint xs">{s}</span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    </div>
  </div>
</div>

<style>
  .whatsnew { margin-bottom: var(--sp-4); border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--stroke)); }
  .ov { grid-template-columns: minmax(320px, 400px) 1fr; align-items: start; }
  .col { display: grid; gap: var(--sp-4); min-width: 0; }
  .lead { margin: 6px 0 14px; }
  .bar.big { height: 18px; }
  .bar.thin { height: 8px; margin-top: 6px; }
  .space-list { margin-top: 14px; }
  .space-list .row { grid-template-columns: 14px 1fr auto; padding: 7px 6px; }
  .exp { grid-column: 2 / 4; color: var(--text-3); font-size: var(--fs-xs); margin-top: -4px; }
  .lead2 { font-size: var(--fs-xs); padding: 0 6px 6px; }
  .link { border: 0; background: none; color: var(--accent); padding: 0; font-size: inherit; }
  .net { display: flex; align-items: baseline; gap: 10px; }
  .hist { display: flex; align-items: flex-end; gap: 10px; height: 110px; padding-top: 8px; }
  .hist div { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px; height: 100%; justify-content: flex-end; }
  .hist i { width: 100%; max-width: 18px; border-radius: 6px; background: color-mix(in srgb, var(--lime) 40%, transparent); }
  .hist i.last { background: var(--lime); }
  .hist span { font-size: 10px; color: var(--text-3); }
  .growers { margin-top: 12px; }
  .growers .row { grid-template-columns: minmax(0, 1fr) auto; cursor: pointer; }
  .min0 { min-width: 0; }
  .cloud-list .row { grid-template-columns: 34px minmax(0, 1fr) auto; gap: 10px; padding: 8px 4px; align-items: start; }
  .pv { font: 650 11px var(--font-mono); color: var(--cat-cloud); border: 1px solid var(--cat-cloud); border-radius: 8px; text-align: center; padding: 4px 0; }
  .sub2 { font-size: var(--fs-xs); color: var(--text-3); overflow: hidden; text-overflow: ellipsis; }
  .sub2.gap { margin-top: 4px; }
  .legend { margin-top: 10px; }
  .sw2 { width: 12px; height: 8px; border-radius: 3px; display: inline-block; }
  .terrain-box { position: relative; height: 520px; border-radius: var(--r-md); overflow: hidden; }
  .lower { grid-template-columns: 1fr 1fr; }
  .type-row { grid-template-columns: 10px 110px 1fr 70px; }
  .type-row .bar { margin: 0; height: 6px; }
  .r { text-align: right; }
  .ins { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-4); }
  .tile { background: var(--surface-2); border-radius: var(--r-md); padding: var(--sp-4); display: grid; gap: 6px; border: 1px solid transparent; text-align: left; color: inherit; }
  .tile:hover { border-color: var(--stroke-strong); }
  .tl { display: flex; gap: 6px; align-items: center; }
  .v { font-size: var(--fs-xl); font-weight: var(--fw-strong); }
  .xs { font-size: var(--fs-xs); }
  @media (max-width: 1100px) { .ov, .lower { grid-template-columns: 1fr; } }
</style>
