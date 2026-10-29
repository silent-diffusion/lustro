<script lang="ts">
  // Explore's folder tree while a scan runs (or after a stopped one): it grows from the scan mirror.
  // New rows fade in, sizes count up and the branch being read is highlighted. Rows are rebuilt at most 4 times
  // a second and re-sort at most every 2s, and the selected row keeps its place in the viewport when rows above it move.
  import type { NodeId } from "../ipc/types";
  import { app, liveLoop, type LiveNode, type LiveScan } from "../lib/app.svelte";
  import { untrack } from "svelte";
  import { catVar } from "../lib/labels";
  import CloudMark from "./CloudMark.svelte";
  import VirtualList from "./VirtualList.svelte";

  interface Props {
    scan: LiveScan;
    selected?: NodeId | null;
    onSelect?: (id: NodeId) => void;
  }
  let { scan, selected = null, onSelect }: Props = $props();

  const ROW = 30;
  // Sizes are copied into the row: LiveNode objects are mutated in place, so reading them through the same
  // object reference wouldn't tell Svelte that anything changed.
  interface Row { n: LiveNode; depth: number; parentSize: number; size: number; cloudOnly: boolean; open: boolean; hot: boolean; fresh: boolean; hasKids: boolean }

  let list: VirtualList<Row> | undefined = $state();
  let host: HTMLDivElement | undefined = $state();
  let rows = $state.raw<Row[]>([]);
  /** Update number at the previous build: rows born after it are new and fade in. */
  let builtTick = 0;

  function build() {
    const out: Row[] = [];
    if (scan.rootId == null) { rows = out; return; }
    const hot = scan.hot();
    const resort = scan.resortDue(2000) || !scan.running;
    const since = builtTick;
    const walk = (id: NodeId, depth: number, parentSize: number) => {
      for (const c of scan.children(id, resort)) {
        const n = scan.nodes.get(c)!;
        const size = n.allocated + n.cloudOnly;
        const hasKids = scan.kids.has(c);
        out.push({ n, depth, parentSize, size, cloudOnly: n.allocated === 0 && n.cloudOnly > 0, open: scan.open.has(c), hot: hot.has(c), fresh: scan.running && since > 0 && n.born > since, hasKids });
        if (hasKids && scan.open.has(c)) walk(c, depth + 1, size || 1);
      }
    };
    const r = scan.nodes.get(scan.rootId)!;
    walk(scan.rootId, 0, r.allocated + r.cloudOnly || 1);
    builtTick = scan.tick;
    rows = out;
  }
  // Rebuilt at most 4 times a second while scanning (slower if the tree is big), not on every update.
  $effect(() => {
    const running = scan.running;
    untrack(build);
    if (!running) return;
    return liveLoop(build, 250);
  });

  // Keep the selected row where it is on screen: when rows above it move by k, scroll by k rows too.
  // (Selecting a different row doesn't scroll.)
  let lastIndex = -1;
  let lastSel: NodeId | null = null;
  $effect(() => {
    const idx = selected == null ? -1 : rows.findIndex((r) => r.n.id === selected);
    const vp = host?.querySelector<HTMLElement>(".vlist");
    if (vp && selected === lastSel && idx >= 0 && lastIndex >= 0 && idx !== lastIndex) vp.scrollTop += (idx - lastIndex) * ROW;
    lastIndex = idx;
    lastSel = selected;
  });

  function toggle(r: Row) {
    const id = r.n.id;
    if (r.hasKids) {
      if (scan.open.has(id)) scan.open.delete(id); else scan.open.add(id);
      build();
    }
    onSelect?.(id);
  }
</script>

<div class="ftree" bind:this={host}>
  <VirtualList bind:this={list} items={rows} rowHeight={ROW} label="Folders">
    {#snippet row(r: Row)}
      {@const n = r.n}
      <button class="t-row" class:hot={r.hot} class:fresh={r.fresh} class:sel={n.id === selected} onclick={() => toggle(r)} aria-expanded={r.hasKids ? r.open : undefined}>
        <span class="t-name" style:padding-left="{r.depth * 14}px">
          <span class="caret" class:open={r.open}>{r.hasKids ? "›" : ""}</span>
          <span class="dot" class:cloud={r.cloudOnly} style:background={r.cloudOnly ? undefined : `var(${catVar(n.category)})`}></span>
          <span class="nm">{n.name}</span>
          {#if n.cloud && n.kind !== "dir"}<CloudMark cloud={n.cloud} mini />{/if}
        </span>
        <span class="pct"><i style:width="{Math.min(100, (r.size / r.parentSize) * 100)}%" style:background={r.cloudOnly ? "var(--cat-cloud)" : `var(${catVar(n.category)})`}></i></span>
        <span class="t-size">{app.fb(r.size)}</span>
      </button>
    {/snippet}
  </VirtualList>
</div>

<style>
  .ftree { position: absolute; inset: 0; font-size: var(--fs-sm); }
  .t-row {
    display: grid; grid-template-columns: minmax(0, 1fr) 52px 62px; align-items: center; gap: 8px; padding: 0 8px; height: 28px;
    border-radius: 8px; width: 100%; border: 0; background: none; color: inherit; text-align: left; font: inherit; cursor: pointer;
  }
  .t-row:hover { background: var(--surface-2); }
  .t-row.sel { background: var(--surface-3); }
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
</style>
