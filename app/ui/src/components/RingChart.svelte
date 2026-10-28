<script lang="ts">
  // Ring chart (sunburst) in SVG. The subtree it gets is already folded to a few hundred arcs.
  import type { NodeId } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { isCloudOnly, size, type VizNode } from "../lib/viz";
  import { catVar } from "../lib/labels";

  interface Props {
    tree: VizNode | null;
    selPath?: NodeId[] | null;
    focusPath?: NodeId[] | null;
    marks?: Map<NodeId, string> | null;
    onSelect?: (n: VizNode) => void;
    onOpen?: (n: VizNode) => void;
  }
  let { tree, selPath = null, focusPath = null, marks = null, onSelect, onOpen }: Props = $props();

  let box: HTMLDivElement | undefined = $state();
  let w = $state(0);
  let h = $state(0);
  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(([e]) => { w = e.contentRect.width; h = e.contentRect.height; });
    ro.observe(box);
    return () => ro.disconnect();
  });

  interface Arc { n: VizNode; d: string; depth: number; anc: NodeId[] }
  const levels = 4;
  const geo = $derived.by(() => {
    const R = Math.max(10, Math.min(w, h) / 2 - 8), cx = w / 2, cy = h / 2;
    const r0 = R * 0.28, step = (R - r0) / levels;
    const arcs: Arc[] = [];
    const p = (r: number, a: number) => `${cx + r * Math.sin(a)},${cy - r * Math.cos(a)}`;
    const path = (ri: number, ro: number, a0: number, a1: number) => {
      const gap = Math.min(0.004, (a1 - a0) / 4);
      a0 += gap; a1 -= gap;
      if (a1 - a0 >= Math.PI * 2 - 0.01) a1 = a0 + Math.PI * 2 - 0.01;
      const large = a1 - a0 > Math.PI ? 1 : 0;
      return `M${p(ro, a0)}A${ro},${ro} 0 ${large} 1 ${p(ro, a1)}L${p(ri, a1)}A${ri},${ri} 0 ${large} 0 ${p(ri, a0)}Z`;
    };
    const walk = (n: VizNode, a0: number, a1: number, d: number, anc: NodeId[]) => {
      if (d > levels) return;
      const total = size(n);
      if (total <= 0) return;
      let a = a0;
      for (const c of n.children ?? []) {
        const span = ((a1 - a0) * size(c)) / total;
        if (span >= 0.004) {
          arcs.push({ n: c, d: path(r0 + (d - 1) * step + 1, r0 + d * step - 1, a, a + span), depth: d, anc });
          walk(c, a, a + span, d + 1, [...anc, c.id]);
        }
        a += span;
      }
    };
    if (tree) walk(tree, 0, Math.PI * 2, 1, [tree.id]);
    return { arcs, cx, cy };
  });

  const sel = $derived(selPath?.length ? selPath[selPath.length - 1] : focusPath?.length ? focusPath[focusPath.length - 1] : null);
  const selSet = $derived(new Set(selPath ?? focusPath ?? []));
  const center = $derived.by(() => {
    if (!tree) return null;
    if (sel == null) return tree;
    let found: VizNode | null = null;
    const find = (n: VizNode) => { if (n.id === sel) found = n; else n.children?.forEach(find); };
    find(tree);
    return found ?? tree;
  });

  function lit(a: Arc): boolean {
    if (sel == null) return true;
    return a.n.id === sel || a.anc.includes(sel) || selSet.has(a.n.id);
  }
  function fill(a: Arc): string {
    return isCloudOnly(a.n) ? "none" : `var(${catVar(a.n.category)})`;
  }
  function stroke(a: Arc): string {
    if (a.n.id === sel) return "var(--text)";
    const m = marks?.get(a.n.id);
    if (m === "remove") return "var(--coral)";
    if (m === "keep") return "var(--lime)";
    if (m === "free") return "var(--cat-cloud)";
    return isCloudOnly(a.n) ? "var(--cat-cloud)" : "none";
  }
</script>

<div class="ring" bind:this={box}>
  {#if w > 0}
    <svg width={w} height={h} role="img" aria-label="Ring chart">
      {#each geo.arcs as a (a.n.id)}
        <path
          d={a.d}
          fill={fill(a)}
          fill-opacity={a.n.kind === "dir" ? 0.45 + 0.12 * a.depth : 0.95}
          stroke={stroke(a)}
          stroke-width={a.n.id === sel || marks?.has(a.n.id) ? 2 : 1}
          stroke-dasharray={isCloudOnly(a.n) ? "3 3" : undefined}
          opacity={lit(a) ? 1 : 0.25}
          role="button"
          tabindex="-1"
          onmousemove={(e) => app.showTip({ kind: "node", node: a.n, x: e.clientX, y: e.clientY })}
          onmouseleave={() => app.showTip(null)}
          onclick={() => onSelect?.(a.n)}
          ondblclick={() => onOpen?.(a.n)}
          onkeydown={() => {}}
        />
      {/each}
      {#if center}
        <text x={geo.cx} y={geo.cy - 4} text-anchor="middle" fill="var(--text)" font-size="22" font-weight="650">{app.fb(size(center))}</text>
        <text x={geo.cx} y={geo.cy + 16} text-anchor="middle" fill="var(--text-3)" font-size="11">{center.name.length > 26 ? center.name.slice(0, 25) + "…" : center.name}</text>
      {/if}
    </svg>
  {/if}
</div>

<style>
  .ring { position: absolute; inset: 0; }
  svg { display: block; }
  path { cursor: pointer; outline: none; }
  path:hover { filter: brightness(1.15); }
</style>
