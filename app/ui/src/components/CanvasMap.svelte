<script lang="ts" module>
  export type Mark = "remove" | "keep" | "free" | "queued";
</script>

<script lang="ts">
  // Block map and file tree (icicle) drawn on canvas, so tens of thousands of rectangles stay fast.
  // The base canvas holds the cells; a second canvas on top holds selection, hover, dimming and marks.
  import type { NodeId } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { blockCells, deltaColor, hatch, hitTest, icicleCells, isCloudOnly, isInside, roundRect, size, type Cell, type VizNode } from "../lib/viz";
  import { PROVIDER } from "../lib/labels";

  interface Props {
    tree: VizNode | null;
    mode?: "blocks" | "icicle";
    maxDepth?: number;
    colorBy?: "cat" | "delta";
    /** Root → selected item. */
    selPath?: NodeId[] | null;
    /** Cleanup: root → current item; it is outlined and its neighbourhood stays bright. */
    focusPath?: NodeId[] | null;
    marks?: Map<NodeId, Mark> | null;
    onSelect?: (n: VizNode) => void;
    onOpen?: (n: VizNode) => void;
    label?: string;
  }
  let { tree, mode = "blocks", maxDepth = 4, colorBy = "cat", selPath = null, focusPath = null, marks = null, onSelect, onOpen, label = "Block map" }: Props = $props();

  let box: HTMLDivElement | undefined = $state();
  let base: HTMLCanvasElement | undefined = $state();
  let over: HTMLCanvasElement | undefined = $state();
  let w = $state(0);
  let h = $state(0);
  let hover = $state(-1);
  let fontsReady = $state(0);

  const cells: Cell[] = $derived(!tree || w < 4 || h < 4 ? [] : mode === "blocks" ? blockCells(tree, w, h, maxDepth) : icicleCells(tree, w, h));

  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(([e]) => {
      w = Math.floor(e.contentRect.width);
      h = Math.floor(e.contentRect.height);
    });
    ro.observe(box);
    document.fonts?.ready.then(() => fontsReady++);
    return () => ro.disconnect();
  });

  function setup(c: HTMLCanvasElement): CanvasRenderingContext2D {
    const dpr = devicePixelRatio || 1;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    return ctx;
  }

  function fitText(ctx: CanvasRenderingContext2D, s: string, max: number): string {
    if (ctx.measureText(s).width <= max) return s;
    let lo = 0, hi = s.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (ctx.measureText(s.slice(0, mid) + "…").width <= max) lo = mid;
      else hi = mid - 1;
    }
    return lo > 0 ? s.slice(0, lo) + "…" : "";
  }

  // Base layer.
  $effect(() => {
    const p = app.palette;
    void fontsReady;
    if (!base || !p) return;
    const ctx = setup(base);
    const ui = getComputedStyle(document.documentElement).getPropertyValue("--font-ui");
    const mono = getComputedStyle(document.documentElement).getPropertyValue("--font-mono");
    const ink = "rgba(10,10,10,.82)";
    for (const c of cells) {
      const n = c.node;
      if (c.group) {
        if (c.head) {
          ctx.font = `650 10px ${ui}`;
          ctx.fillStyle = p.text2;
          ctx.letterSpacing = "0.6px";
          ctx.fillText(fitText(ctx, `${n.name.toUpperCase()} · ${app.fb(size(n))}`, c.w - 12), c.x + 6, c.y + 13);
          ctx.letterSpacing = "0px";
        }
        continue;
      }
      const inset = mode === "blocks" ? 1 : 1;
      const x = c.x + inset, y = c.y + inset, cw = c.w - inset * 2, ch = c.h - inset * 2;
      if (cw <= 0.3 || ch <= 0.3) continue;
      const cloud = isCloudOnly(n);
      const big = cw > 4 && ch > 4;
      if (big) roundRect(ctx, x, y, cw, ch, 4);
      else { ctx.beginPath(); ctx.rect(x, y, cw, ch); }
      if (cloud) {
        ctx.fillStyle = hatch(ctx, p.cat.cloud) ?? p.cat.cloud;
        ctx.fill();
        ctx.strokeStyle = p.cat.cloud;
        ctx.lineWidth = 1.5;
        if (big) ctx.stroke();
      } else {
        ctx.globalAlpha = n.id < 0 ? 0.55 : mode === "icicle" && n.kind === "dir" ? 0.85 : 1;
        ctx.fillStyle = colorBy === "delta" ? deltaColor(p, n.delta) : p.cat[n.category];
        ctx.fill();
        ctx.globalAlpha = 1;
        if (big) {
          ctx.strokeStyle = "rgba(0,0,0,.35)";
          ctx.lineWidth = 1;
          ctx.stroke();
        }
      }
      // Labels.
      const color = cloud ? p.cat.cloud : colorBy === "delta" ? p.text : ink;
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, cw, ch);
      ctx.clip();
      ctx.fillStyle = color;
      if (mode === "blocks" && cw > 60 && ch > 30) {
        ctx.font = `650 11px ${ui}`;
        ctx.fillText(fitText(ctx, n.name, cw - 12), x + 6, y + 15);
        ctx.font = `500 11px ${mono}`;
        const v = colorBy === "delta" && n.delta ? app.fb(n.delta, true) : app.fb(size(n));
        ctx.fillText(v, x + 6, y + 29);
      } else if (mode === "icicle" && ch > 18) {
        ctx.font = `650 11px ${ui}`;
        const name = fitText(ctx, n.name, cw - 70);
        ctx.fillText(name, x + 6, y + 14);
        const nw = ctx.measureText(name).width;
        ctx.font = `500 11px ${mono}`;
        ctx.fillText(app.fb(size(n)), x + 12 + nw, y + 14);
      }
      ctx.restore();
      // Cloud provider tag in the corner (downloaded and pinned files keep their category color).
      if (n.cloud && mode === "blocks" && cw > 34 && ch > 18) {
        const s = PROVIDER[n.cloud.provider].short;
        ctx.font = `650 9px ${mono}`;
        const tw = ctx.measureText(s).width + 8;
        roundRect(ctx, x + cw - tw - 3, y + 3, tw, 13, 5);
        ctx.fillStyle = n.cloud.state === "pinned" ? p.cat.cloud : p.surface1;
        ctx.fill();
        ctx.strokeStyle = p.cat.cloud;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.fillStyle = n.cloud.state === "pinned" ? p.accentInk : p.cat.cloud;
        ctx.fillText(s, x + cw - tw + 1, y + 13);
      }
    }
  });

  /** Deepest drawn cell on a root → item path. */
  function deepestOn(path: NodeId[] | null): number {
    if (!path?.length) return -1;
    const set = new Set(path);
    let best = -1, bestRank = -1;
    const rank = new Map(path.map((id, i) => [id, i]));
    for (let i = 0; i < cells.length; i++) {
      const id = cells[i].node.id;
      if (set.has(id) && rank.get(id)! > bestRank) { best = i; bestRank = rank.get(id)!; }
    }
    return best;
  }

  /** Where an item is drawn (for pins): the item's cell, or the smallest drawn folder holding it. */
  export function rectOf(path: NodeId[]): { x: number; y: number; w: number; h: number } | null {
    const i = deepestOn(path);
    return i < 0 ? null : { x: cells[i].x, y: cells[i].y, w: cells[i].w, h: cells[i].h };
  }

  // Overlay layer.
  $effect(() => {
    const p = app.palette;
    if (!over || !p) return;
    const ctx = setup(over);
    const shade = (i: number, alpha: number) => {
      const c = cells[i];
      ctx.globalAlpha = alpha;
      ctx.fillStyle = p.surface1;
      ctx.fillRect(c.x, c.y, c.w, c.h);
      ctx.globalAlpha = 1;
    };
    const outline = (i: number, color: string, width: number) => {
      const c = cells[i];
      roundRect(ctx, c.x + 1 + width / 2, c.y + 1 + width / 2, Math.max(1, c.w - 2 - width), Math.max(1, c.h - 2 - width), 4);
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      ctx.stroke();
    };

    // Explore selection: outline it and dim what's outside a selected folder.
    const si = deepestOn(selPath);
    if (si >= 0) {
      const selIsDeeper = selPath && cells[si].node.id !== selPath[selPath.length - 1];
      const dimOutside = !selIsDeeper && (cells[si].group || cells[si].node.kind === "dir");
      if (dimOutside) {
        for (let i = 0; i < cells.length; i++) {
          if (cells[i].group || isInside(cells, i, si)) continue;
          if (mode === "icicle" && isInside(cells, si, i)) continue;
          shade(i, 0.75);
        }
      }
      outline(si, p.text, 2);
    }

    // Cleanup: neighbourhood, decisions and the current item.
    if (focusPath?.length) {
      const hoodPath = focusPath.slice(0, Math.max(2, focusPath.length - 2));
      const hood = deepestOn(hoodPath);
      for (let i = 0; i < cells.length; i++) {
        if (cells[i].group) continue;
        if (hood >= 0 && !isInside(cells, i, hood) && !(mode === "icicle" && isInside(cells, hood, i))) shade(i, 0.7);
      }
    }
    if (marks) {
      for (let i = 0; i < cells.length; i++) {
        const m = marks.get(cells[i].node.id);
        if (!m || cells[i].group) continue;
        const c = cells[i];
        if (m === "remove" || m === "free") {
          roundRect(ctx, c.x + 1, c.y + 1, c.w - 2, c.h - 2, 4);
          ctx.fillStyle = (m === "remove" ? hatch(ctx, "rgba(0,0,0,.5)", 6, 2) : hatch(ctx, p.cat.cloud, 6, 1.5)) ?? "transparent";
          ctx.fill();
          outline(i, m === "remove" ? p.coral : p.cat.cloud, 2);
        } else if (m === "keep") outline(i, p.lime, 2);
        else outline(i, p.text, 1.5);
      }
    }
    if (focusPath?.length) {
      const cur = deepestOn(focusPath);
      if (cur >= 0) outline(cur, p.accent, 2.5);
    }

    if (hover >= 0 && hover < cells.length && !cells[hover].group) {
      const c = cells[hover];
      ctx.fillStyle = app.theme === "dark" ? "rgba(255,255,255,.14)" : "rgba(255,255,255,.22)";
      ctx.fillRect(c.x + 1, c.y + 1, c.w - 2, c.h - 2);
    }
  });

  function at(e: MouseEvent): number {
    const r = over!.getBoundingClientRect();
    return hitTest(cells, e.clientX - r.left, e.clientY - r.top);
  }

  function move(e: MouseEvent) {
    const i = at(e);
    hover = i;
    app.showTip(i >= 0 ? { kind: "node", node: cells[i].node, x: e.clientX, y: e.clientY } : null);
  }
</script>

<div class="cmap" bind:this={box} role="img" aria-label={label}>
  <canvas bind:this={base}></canvas>
  <canvas
    bind:this={over}
    class:pointer={hover >= 0}
    onmousemove={move}
    onmouseleave={() => { hover = -1; app.showTip(null); }}
    onclick={(e) => { const i = at(e); if (i >= 0) onSelect?.(cells[i].node); }}
    ondblclick={(e) => { const i = at(e); if (i >= 0) onOpen?.(cells[i].node); }}
  ></canvas>
  {#if tree && !cells.length && w > 4}
    <div class="empty faint">Nothing to draw yet</div>
  {/if}
</div>

<style>
  .cmap { position: absolute; inset: 0; overflow: hidden; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  canvas.pointer { cursor: pointer; }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; }
</style>
