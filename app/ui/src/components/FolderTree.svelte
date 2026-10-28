<script lang="ts">
  // Lazy folder tree: children are fetched page by page from the backend when a folder opens,
  // and only the visible rows are rendered (VirtualList), so a million nodes stay responsive.
  import type { SortKey } from "../ipc/backend";
  import type { NodeDto, NodeId } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { catVar } from "../lib/labels";
  import { size } from "../lib/viz";
  import CloudMark from "./CloudMark.svelte";
  import VirtualList from "./VirtualList.svelte";
  import { untrack } from "svelte";

  interface Props {
    rootId: NodeId;
    selPath?: NodeId[] | null;
    sort?: SortKey;
    onSelect?: (n: NodeDto) => void;
  }
  let { rootId, selPath = null, sort = "allocated", onSelect }: Props = $props();

  const PAGE = 300;
  type Row = { kind: "node"; n: NodeDto; depth: number; parentSize: number } | { kind: "more"; parent: NodeId; depth: number; left: number } | { kind: "loading"; parent: NodeId; depth: number };

  // Caches live outside reactivity; `tick` re-derives the rows.
  let cache = new Map<NodeId, { items: NodeDto[]; total: number }>();
  let pending = new Set<NodeId>();
  let open = $state.raw(new Set<NodeId>());
  let tick = $state(0);
  let rootNode = $state.raw<NodeDto | null>(null);
  let list: VirtualList<Row> | undefined = $state();

  async function load(id: NodeId, more = false): Promise<void> {
    if (pending.has(id)) return;
    const have = cache.get(id);
    if (have && !more) return;
    pending.add(id);
    tick++;
    const offset = have?.items.length ?? 0;
    const items = await app.backend.children(id, sort, offset, PAGE);
    const total = have?.total ?? (await app.backend.node(id)).childCount;
    cache.set(id, { items: [...(have?.items ?? []), ...items], total });
    pending.delete(id);
    tick++;
  }

  // Reset when the root, sort order or data changes.
  $effect(() => {
    void app.version;
    const id = rootId, s = sort;
    void s;
    untrack(() => {
      cache = new Map();
      pending = new Set();
      open = new Set([id]);
      app.backend.node(id).then((n) => { rootNode = n; tick++; });
      load(id);
    });
  });

  const rows: Row[] = $derived.by(() => {
    void tick;
    const out: Row[] = [];
    const walk = (id: NodeId, depth: number, parentSize: number) => {
      const c = cache.get(id);
      if (!c) {
        if (pending.has(id)) out.push({ kind: "loading", parent: id, depth });
        return;
      }
      for (const n of c.items) {
        out.push({ kind: "node", n, depth, parentSize });
        if (open.has(n.id) && n.childCount > 0) walk(n.id, depth + 1, size(n) || 1);
      }
      if (pending.has(id)) out.push({ kind: "loading", parent: id, depth });
      else if (c.items.length < c.total) out.push({ kind: "more", parent: id, depth, left: c.total - c.items.length });
    };
    if (rootNode) walk(rootId, 0, size(rootNode) || 1);
    return out;
  });

  const selected = $derived(selPath?.length ? selPath[selPath.length - 1] : null);

  // Reveal an item selected elsewhere: open its ancestors, load pages until it shows up, scroll to it.
  $effect(() => {
    const path = selPath;
    if (!path?.length) return;
    let cancelled = false;
    const root = rootId;
    untrack(() => (async () => {
      const start = path.indexOf(root);
      if (start < 0) return;
      for (let i = start; i < path.length - 1; i++) {
        open.add(path[i]);
        await load(path[i]);
        for (let page = 0; page < 10 && !cache.get(path[i])!.items.some((n) => n.id === path[i + 1]) && cache.get(path[i])!.items.length < cache.get(path[i])!.total; page++) {
          await load(path[i], true);
        }
        if (cancelled) return;
      }
      tick++;
      await Promise.resolve();
      const idx = rows.findIndex((r) => r.kind === "node" && r.n.id === path[path.length - 1]);
      if (idx >= 0) list?.scrollToIndex(idx);
    })());
    return () => { cancelled = true; };
  });

  function toggle(n: NodeDto) {
    if (n.childCount > 0) {
      if (open.has(n.id)) open.delete(n.id);
      else { open.add(n.id); load(n.id); }
      tick++;
    }
    onSelect?.(n);
  }

</script>

<div class="ftree">
  <VirtualList bind:this={list} items={rows} rowHeight={30} label="Folders">
    {#snippet row(r: Row)}
      {#if r.kind === "node"}
        {@const n = r.n}
        {@const cloud = n.allocated === 0 && n.cloudOnly > 0}
        <button
          class="t-row"
          class:sel={n.id === selected}
          onclick={() => toggle(n)}
          onmousemove={(e) => app.showTip({ kind: "node", node: n, x: e.clientX, y: e.clientY })}
          onmouseleave={() => app.showTip(null)}
          aria-expanded={n.childCount > 0 ? open.has(n.id) : undefined}
        >
          <span class="t-name" style:padding-left="{r.depth * 14}px">
            <span class="caret" class:open={open.has(n.id)}>{n.childCount > 0 ? "›" : ""}</span>
            <span class="dot" class:cloud style:background={cloud ? undefined : `var(${catVar(n.category)})`}></span>
            <span class="nm" class:hidden={n.hidden}>{n.name}</span>
            {#if n.cloud && n.kind !== "dir"}<CloudMark cloud={n.cloud} mini />{/if}
            {#if n.accessDenied}<span class="badge" title="Needs admin to read">locked</span>{/if}
          </span>
          <span class="pct"><i style:width="{Math.min(100, (size(n) / r.parentSize) * 100)}%" style:background={cloud ? "var(--cat-cloud)" : `var(${catVar(n.category)})`}></i></span>
          <span class="t-size">{app.fb(size(n))}</span>
        </button>
      {:else if r.kind === "more"}
        <button class="t-row more" style:padding-left="{r.depth * 14 + 28}px" onclick={() => load(r.parent, true)}>
          Show {Math.min(PAGE, r.left).toLocaleString("en-US")} more of {r.left.toLocaleString("en-US")}…
        </button>
      {:else}
        <div class="t-row more faint" style:padding-left="{r.depth * 14 + 28}px">Loading…</div>
      {/if}
    {/snippet}
  </VirtualList>
</div>

<style>
  .ftree { position: absolute; inset: 0; font-size: var(--fs-sm); }
  .t-row {
    display: grid; grid-template-columns: minmax(0, 1fr) 52px 62px; align-items: center; gap: 8px; padding: 0 8px; height: 28px;
    border-radius: 8px; cursor: pointer; width: 100%; border: 0; background: none; color: inherit; text-align: left; font: inherit;
  }
  .t-row:hover { background: var(--surface-2); }
  .t-row.sel { background: var(--surface-3); }
  .t-row.more { display: block; color: var(--accent); font-size: var(--fs-xs); }
  .t-name { display: flex; align-items: center; gap: 6px; overflow: hidden; white-space: nowrap; min-width: 0; }
  .nm { overflow: hidden; text-overflow: ellipsis; }
  .nm.hidden { color: var(--text-2); }
  .caret { width: 14px; flex: none; color: var(--text-3); transition: transform .15s; display: inline-block; text-align: center; }
  .caret.open { transform: rotate(90deg); }
  .pct { height: 4px; border-radius: 2px; background: var(--surface-3); position: relative; overflow: hidden; }
  .pct i { position: absolute; inset: 0 auto 0 0; border-radius: 2px; }
  .t-size { text-align: right; font-family: var(--font-mono); font-size: var(--fs-xs); color: var(--text-2); white-space: nowrap; }
</style>
