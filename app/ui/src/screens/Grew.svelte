<script lang="ts">
  import type { DiffDto, DiffEntryDto, SnapshotMeta } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { fmtMonthDay, fmtWhen } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import Sparkline from "../components/Sparkline.svelte";
  import MainViz from "../components/MainViz.svelte";

  let snaps = $state.raw<SnapshotMeta[]>([]);
  let fromId = $state("");
  let toId = $state("");
  let diff = $state.raw<DiffDto | null>(null);
  let tab = $state<"grew" | "shrank" | "folders">("grew");

  $effect(() => {
    void app.version;
    app.backend.listSnapshots(app.root?.name ?? "").then((s) => {
      snaps = s;
      if (s.length > 1) {
        if (!s.some((x) => x.id === toId)) toId = s[s.length - 1].id;
        if (!s.some((x) => x.id === fromId)) fromId = s[s.length - 2].id;
      }
    });
  });
  $effect(() => {
    void app.version;
    if (!fromId || !toId || fromId === toId) { diff = null; return; }
    app.backend.diff(fromId, toId, 500).then((d) => (diff = d));
  });

  const RANGES: [string, number][] = [["Last scan", 0], ["1 week", 7], ["1 month", 30], ["3 months", 90]];
  const range = $derived.by(() => {
    const b = snaps.findIndex((s) => s.id === toId), a = snaps.findIndex((s) => s.id === fromId);
    if (a < 0 || b < 0) return -1;
    if (a === b - 1) return 0;
    return RANGES.findIndex(([, d]) => d && pickFrom(d) === fromId);
  });
  function pickFrom(days: number): string {
    const b = snaps.find((s) => s.id === toId);
    if (!b) return fromId;
    if (!days) return snaps[Math.max(0, snaps.indexOf(b) - 1)].id;
    const target = b.takenAt - days * 86400;
    let best = snaps[0];
    for (const s of snaps) if (s.takenAt <= target + 86400 && s.takenAt < b.takenAt) best = s;
    return best.id;
  }

  const items = $derived(diff?.entries.filter((e) => e.status !== "deleted") ?? []);
  const grew = $derived(items.filter((e) => e.delta > 0));
  const shrank = $derived(items.filter((e) => e.delta < 0).sort((a, b) => a.delta - b.delta));
  const added = $derived(items.filter((e) => e.status === "new"));
  const deleted = $derived(diff?.entries.filter((e) => e.status === "deleted") ?? []);
  const sep = $derived(app.platform === "windows" ? "\\" : "/");

  // Roll item changes up into the folders that hold them (2–4 levels below the drive root).
  const folders = $derived.by(() => {
    const root = app.volume?.mountPath ?? "";
    const m = new Map<string, DiffEntryDto>();
    for (const e of items) {
      if (!e.path.startsWith(root)) continue;
      const parts = e.path.slice(root.length).split(sep).filter(Boolean);
      for (let k = 2; k <= Math.min(4, parts.length - 1); k++) {
        const path = root + parts.slice(0, k).join(sep);
        const f = m.get(path) ?? { id: null, path, name: parts[k - 1], kind: "dir", category: e.category, before: 0, after: 0, delta: 0, sparkline: e.sparkline.map(() => 0), status: "grew" };
        f.before += e.before; f.after += e.after; f.delta += e.delta;
        f.sparkline = f.sparkline.map((v, i) => v + (e.sparkline[i] ?? 0));
        f.status = f.delta >= 0 ? "grew" : "shrank";
        m.set(path, f);
      }
    }
    return [...m.values()].filter((f) => Math.abs(f.delta) > 1e8).sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta)).slice(0, 10);
  });
  const rows = $derived(tab === "grew" ? grew : tab === "shrank" ? shrank : folders);

  // Used space chart with a linear forecast.
  let chartW = $state(360);
  const H = 190, pad = { l: 34, r: 10, t: 12, b: 22 };
  const chart = $derived.by(() => {
    const tr = diff?.trend ?? [];
    if (tr.length < 2) return null;
    const vals = tr.map((t) => t.usedBytes / 1e9);
    const n = vals.length, W = chartW;
    const lo = Math.floor((Math.min(...vals) - 10) / 10) * 10, hi = Math.ceil((Math.max(...vals) + 10) / 10) * 10;
    const X = (k: number) => pad.l + (k / (n - 1)) * (W - pad.l - pad.r);
    const Y = (v: number) => pad.t + (1 - (v - lo) / (hi - lo)) * (H - pad.t - pad.b);
    const line = vals.map((v, k) => `${X(k)},${Y(v)}`).join(" ");
    const area = `${X(0)},${Y(lo)} ${line} ${X(n - 1)},${Y(lo)}`;
    const ai = tr.findIndex((t) => t.takenAt === diff!.from.takenAt), bi = tr.findIndex((t) => t.takenAt === diff!.to.takenAt);
    const days = (tr[n - 1].takenAt - tr[0].takenAt) / 86400;
    const perWeek = ((vals[n - 1] - vals[0]) / Math.max(1, days)) * 7;
    return { W, lo, hi, X, Y, line, area, vals, tr, ai, bi, perWeek, grid: [lo, (lo + hi) / 2, hi] };
  });
  const forecast = $derived(diff?.daysUntilFull != null && chart
    ? `≈ ${chart.perWeek.toFixed(1)} GB/week · full in ~${Math.max(1, Math.round(diff.daysUntilFull / 30))} months`
    : "not growing");
</script>

{#if snaps.length < 2}
  <section class="card"><p class="faint">What grew needs two scans. Scan again later to compare.</p></section>
{:else}
  <div class="cmp">
    <label class="snap"><span class="k">Compare from (A)</span>
      <select bind:value={fromId}>{#each snaps.slice(0, -1) as s (s.id)}<option value={s.id} disabled={s.takenAt >= (snaps.find((x) => x.id === toId)?.takenAt ?? Infinity)}>{fmtWhen(s.takenAt)}</option>{/each}</select>
    </label>
    <span class="faint">▸</span>
    <label class="snap"><span class="k">To (B)</span>
      <select bind:value={toId}>{#each snaps.slice(1) as s (s.id)}<option value={s.id}>{fmtWhen(s.takenAt)}</option>{/each}</select>
    </label>
    <div class="seg ranges">
      {#each RANGES as [l, dd], i (l)}<button class:on={range === i} onclick={() => (fromId = pickFrom(dd))}>{l}</button>{/each}
    </div>
    <span class="faint note">{snaps.length} snapshots kept on this computer · nothing is uploaded</span>
  </div>

  {#if diff}
    <div class="grid gr">
      <div class="col">
        <section class="card">
          <div class="card-head"><div class="ic"><Icon name="grew" size={18} /></div><h2>Net change</h2><span class="sub">A ▸ B</span></div>
          <div class="stat-big" class:up={diff.netDelta > 0} class:down={diff.netDelta < 0}>{app.fb(diff.netDelta, true)}</div>
          <div class="trio">
            <div><div class="k">Grew</div><div class="v up">{app.fb(diff.grew, true)}</div></div>
            <div><div class="k">Shrank</div><div class="v down">{app.fb(diff.shrank, true)}</div></div>
            <div><div class="k">New items</div><div class="v">{added.length}</div></div>
          </div>
        </section>
        <section class="card">
          <div class="card-head"><div class="ic"><Icon name="overview" size={18} /></div><h2>Used space over time</h2><span class="sub accent">{forecast}</span></div>
          <div class="chart" bind:clientWidth={chartW}>
            {#if chart}
              <svg viewBox="0 0 {chart.W} {H}" role="img" aria-label="Used space over time">
                {#each chart.grid as v (v)}
                  <line x1={pad.l} x2={chart.W - pad.r} y1={chart.Y(v)} y2={chart.Y(v)} stroke="var(--stroke)" stroke-dasharray="2 4" />
                  <text x={pad.l - 6} y={chart.Y(v) + 4} text-anchor="end" font-size="10" fill="var(--text-3)">{v.toFixed(0)}</text>
                {/each}
                <polygon points={chart.area} fill="var(--lime)" fill-opacity=".10" />
                <polyline points={chart.line} fill="none" stroke="var(--lime)" stroke-width="2" />
                {#each [[chart.ai, "A", "--text-2"], [chart.bi, "B", "--accent"]] as [k, l, c] (l)}
                  {#if (k as number) >= 0}
                    <circle cx={chart.X(k as number)} cy={chart.Y(chart.vals[k as number])} r="4.5" fill="var(--surface-1)" stroke="var({c})" stroke-width="2" />
                    <text x={chart.X(k as number)} y={chart.Y(chart.vals[k as number]) - 10} text-anchor="middle" font-size="10" font-weight="700" fill="var(--text)">{l}</text>
                  {/if}
                {/each}
                {#each chart.tr as t, k (k)}
                  {#if k % 2 === 0}<text x={chart.X(k)} y={H - 4} text-anchor="middle" font-size="10" fill="var(--text-3)">{fmtMonthDay(t.takenAt)}</text>{/if}
                {/each}
              </svg>
            {/if}
          </div>
        </section>
        <section class="card">
          <div class="card-head"><div class="ic"><Icon name="spark" size={18} /></div><h2>New since A</h2></div>
          <div class="list">
            {#each added as e (e.path)}
              <button class="row two" onclick={() => e.id != null && app.go("explore", e.id)}><div class="min0"><div class="name">{e.name}</div><div class="path mono">{e.path}</div></div><span class="mono up">{app.fb(e.delta, true)}</span></button>
            {:else}
              <p class="faint">Nothing new.</p>
            {/each}
            {#if deleted.length}
              <div class="row two"><div class="min0"><div class="name faint">Deleted since A: {deleted.map((d) => d.name).join(", ")}</div></div><span class="mono down">{app.fb(deleted.reduce((s, d) => s + d.delta, 0), true)}</span></div>
            {/if}
          </div>
        </section>
      </div>

      <div class="col">
        <section class="card">
          <div class="card-head"><div class="ic"><Icon name="treeIc" size={18} /></div><h2>Biggest changes</h2>
            <div class="tools"><div class="seg">
              <button class:on={tab === "grew"} onclick={() => (tab = "grew")}>Grew</button>
              <button class:on={tab === "shrank"} onclick={() => (tab = "shrank")}>Shrank</button>
              <button class:on={tab === "folders"} onclick={() => (tab = "folders")}>By folder</button>
            </div></div>
          </div>
          <div class="list tbl">
            <div class="row hd"><span>Item</span><span class="r">Before</span><span class="r">Now</span><span class="r">Change</span><span class="r">Trend</span></div>
            {#each rows as e (e.path)}
              <button class="row" onclick={() => e.id != null && app.go("explore", e.id)}>
                <div class="min0"><div class="name">{#if e.status === "new"}<span class="badge">new</span> {/if}{e.name}</div><div class="path mono">{e.path}</div></div>
                <span class="mono muted r">{app.fb(e.before)}</span>
                <span class="mono r">{app.fb(e.after)}</span>
                <span class="mono r" class:up={e.delta > 0} class:down={e.delta < 0}>{app.fb(e.delta, true)}</span>
                <span class="r"><Sparkline values={e.sparkline} color={e.delta > 0 ? "--grew" : "--shrank"} /></span>
              </button>
            {:else}
              <p class="faint">No changes here.</p>
            {/each}
          </div>
        </section>
        <section class="card">
          <div class="card-head"><div class="ic"><Icon name="blocks" size={18} /></div><h2>Change map</h2><span class="sub">size = now · color = change since last scan</span>
            <div class="tools legend"><span><span class="dot" style:background="var(--grew)"></span>grew</span><span><span class="dot" style:background="var(--shrank)"></span>shrank</span><span><span class="dot" style:background="var(--surface-3)"></span>no change</span></div>
          </div>
          <div class="dmap">{#if app.root}<MainViz rootId={app.root.id} view="blockMap" colorBy="delta" maxDepth={4} onSelect={(id) => app.go("explore", id)} />{/if}</div>
        </section>
      </div>
    </div>
  {/if}
{/if}

<style>
  .cmp { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: var(--sp-4); }
  .snap { display: flex; flex-direction: column; background: var(--surface-1); border: 1px solid var(--stroke); border-radius: var(--r-md); padding: 8px 14px; min-width: 190px; }
  .snap .k { font-size: 10px; color: var(--text-3); text-transform: uppercase; letter-spacing: .06em; }
  .snap select { background: none; border: 0; color: var(--text); font: inherit; padding: 0; cursor: pointer; }
  .snap select option { background: var(--surface-1); }
  .ranges { margin-left: 8px; }
  .note { margin-left: auto; }
  .gr { grid-template-columns: minmax(340px, 420px) minmax(0, 1fr); align-items: start; }
  .col { display: grid; gap: var(--sp-4); min-width: 0; }
  .trio { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-top: 14px; }
  .trio > div { background: var(--surface-2); border-radius: var(--r-sm); padding: 10px; }
  .trio .k { font-size: var(--fs-xs); color: var(--text-3); }
  .trio .v { font-family: var(--font-mono); font-size: var(--fs-md); font-weight: var(--fw-strong); white-space: nowrap; }
  .chart svg { width: 100%; height: 190px; display: block; }
  .two { grid-template-columns: minmax(0, 1fr) auto; }
  .min0 { min-width: 0; }
  .tbl .row { grid-template-columns: minmax(0, 1fr) 80px 80px 90px 84px; }
  .tbl .hd { font-size: var(--fs-xs); color: var(--text-3); text-transform: uppercase; letter-spacing: .05em; }
  .tbl .hd:hover { background: none; }
  .r { text-align: right; }
  .dmap { height: 380px; position: relative; }
  @media (max-width: 1100px) { .gr { grid-template-columns: 1fr; } .tbl .row { grid-template-columns: 1fr 80px 70px; } .tbl .row > :nth-child(2), .tbl .row > :nth-child(5) { display: none; } }
</style>
