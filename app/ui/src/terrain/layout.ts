// Ground layout: a squarified treemap of the tree in square coordinates [-1, 1]².
// Pure functions only (no DOM, no three.js) so they can be unit-tested.

import type { Category, NodeId, TreeDto } from "../ipc/types";

export interface Rect { x: number; y: number; w: number; h: number }

/**
 * Squarified treemap (Bruls, Huizing, van Wijk). `values` should be sorted descending and > 0.
 * Returns one rect per value, in the same order. The rects tile (x, y, w, h) exactly.
 */
export function squarify(values: readonly number[], x: number, y: number, w: number, h: number): Rect[] {
  const out: Rect[] = new Array(values.length);
  let total = 0;
  for (const v of values) total += Math.max(0, v);
  if (total <= 0 || w <= 0 || h <= 0) {
    for (let k = 0; k < values.length; k++) out[k] = { x, y, w: 0, h: 0 };
    return out;
  }
  const scale = (w * h) / total;
  let i = 0;
  while (i < values.length) {
    const side = Math.min(w, h);
    let rowSum = 0, rowMin = Infinity, rowMax = 0, worst = Infinity, j = i;
    // Grow the row while its worst aspect ratio keeps improving.
    while (j < values.length) {
      const a = Math.max(0, values[j]) * scale;
      const s = rowSum + a, mn = Math.min(rowMin, a), mx = Math.max(rowMax, a);
      const wst = mn > 0 ? Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn)) : Infinity;
      if (j > i && wst > worst) break;
      rowSum = s; rowMin = mn; rowMax = mx; worst = wst; j++;
    }
    const last = j >= values.length;
    // Lay the row along the shorter side; the last row takes whatever is left (no float drift).
    const thick = last ? (w >= h ? w : h) : rowSum / side;
    let off = 0;
    for (let k = i; k < j; k++) {
      let len = rowSum > 0 ? ((Math.max(0, values[k]) * scale) / rowSum) * side : 0;
      if (k === j - 1) len = side - off;
      out[k] = w >= h ? { x, y: y + off, w: thick, h: len } : { x: x + off, y, w: len, h: thick };
      off += len;
    }
    if (w >= h) { x += thick; w -= thick; } else { y += thick; h -= thick; }
    i = j;
  }
  return out;
}

export interface LayoutNode {
  /** Real node id. A folded "smaller items" patch carries its parent's id. */
  id: NodeId;
  /** Index of the parent in `Layout.nodes`, -1 for the root. */
  parent: number;
  depth: number;
  x0: number; y0: number; x1: number; y1: number;
  /** Layout weight in bytes: on-disk bytes, plus cloud-only bytes when valleys are drawn. */
  weight: number;
  allocated: number;
  cloudOnly: number;
  category: Category;
  /** True when no children were laid out inside this patch. */
  leaf: boolean;
  /** Synthetic patch holding siblings too small to draw (level of detail). */
  folded: boolean;
  /** Source node; null for folded patches. */
  src: TreeDto | null;
}

export interface Layout {
  nodes: LayoutNode[];
  /** Real node id -> index in `nodes` (folded patches excluded). */
  byId: Map<NodeId, number>;
}

export interface LayoutOptions {
  /** Upper bound on laid-out patches (level of detail for huge trees). */
  maxNodes: number;
  /** Patches smaller than this area (square units) are folded into a "smaller items" patch. */
  minArea: number;
  /** At most this many children per folder are drawn individually. */
  maxChildren: number;
  maxDepth: number;
  /** Count cloud-only bytes as land area (they are drawn as valleys). */
  includeCloud: boolean;
}

export const DEFAULT_LAYOUT: LayoutOptions = { maxNodes: 8000, minArea: 1e-4, maxChildren: 200, maxDepth: 24, includeCloud: true };

export function cloudBytes(n: TreeDto): number {
  const own = n.cloud?.state === "cloudOnly" ? n.logical : 0;
  return Math.max(n.cloudOnly, own);
}

export function nodeWeight(n: TreeDto, includeCloud: boolean): number {
  return Math.max(0, n.allocated) + (includeCloud ? cloudBytes(n) : 0);
}

/** Inner padding between a folder's edge and its children, so sibling hills stay apart. */
const padFor = (depth: number, w: number, h: number) => (depth === 0 ? 0 : Math.min(w, h) * 0.04);

/**
 * Lays the tree out largest-patch-first, so the node budget goes to what is visible.
 * Children that are too small, or beyond `maxChildren`, become one folded patch per folder.
 */
export function layoutTree(root: TreeDto, opts: Partial<LayoutOptions> = {}): Layout {
  const o = { ...DEFAULT_LAYOUT, ...opts };
  const nodes: LayoutNode[] = [];
  const byId = new Map<NodeId, number>();
  const rootW = nodeWeight(root, o.includeCloud);
  nodes.push({
    id: root.id, parent: -1, depth: 0, x0: -1, y0: -1, x1: 1, y1: 1,
    weight: rootW, allocated: root.allocated, cloudOnly: cloudBytes(root), category: root.category,
    leaf: true, folded: false, src: root,
  });
  byId.set(root.id, 0);

  // Max-heap of expandable patches keyed by area.
  const heap: number[] = [0];
  const area = (k: number) => (nodes[k].x1 - nodes[k].x0) * (nodes[k].y1 - nodes[k].y0);
  const push = (k: number) => {
    heap.push(k);
    let c = heap.length - 1;
    while (c > 0) {
      const p = (c - 1) >> 1;
      if (area(heap[p]) >= area(heap[c])) break;
      [heap[p], heap[c]] = [heap[c], heap[p]];
      c = p;
    }
  };
  const pop = (): number => {
    const top = heap[0];
    const last = heap.pop()!;
    if (heap.length) {
      heap[0] = last;
      let p = 0;
      for (;;) {
        const l = 2 * p + 1, r = l + 1;
        let m = p;
        if (l < heap.length && area(heap[l]) > area(heap[m])) m = l;
        if (r < heap.length && area(heap[r]) > area(heap[m])) m = r;
        if (m === p) break;
        [heap[p], heap[m]] = [heap[m], heap[p]];
        p = m;
      }
    }
    return top;
  };

  while (heap.length && nodes.length < o.maxNodes) {
    const pi = pop();
    const parent = nodes[pi];
    const src = parent.src!;
    const kids = src.children.filter((c) => nodeWeight(c, o.includeCloud) > 0);
    if (!kids.length || parent.weight <= 0) continue;
    kids.sort((a, b) => nodeWeight(b, o.includeCloud) - nodeWeight(a, o.includeCloud));

    const pad = padFor(parent.depth, parent.x1 - parent.x0, parent.y1 - parent.y0);
    const x = parent.x0 + pad, y = parent.y0 + pad;
    const w = parent.x1 - parent.x0 - 2 * pad, h = parent.y1 - parent.y0 - 2 * pad;
    if (w <= 0 || h <= 0) continue;
    let kidSum = 0;
    for (const c of kids) kidSum += nodeWeight(c, o.includeCloud);
    // Children may not add up to the parent (files directly inside a folder are children too,
    // but DTO folding can drop some); scale to the larger so shares stay honest.
    const denom = Math.max(kidSum, parent.weight);
    const unit = (w * h) / denom;

    const budget = o.maxNodes - nodes.length;
    let keep = 0, keptSum = 0;
    while (keep < kids.length && keep < o.maxChildren && keep < budget - 1) {
      const kw = nodeWeight(kids[keep], o.includeCloud);
      if (kw * unit < o.minArea) break;
      keptSum += kw; keep++;
    }
    if (keep === 0) continue;
    const rest = kidSum - keptSum;
    const values = kids.slice(0, keep).map((c) => nodeWeight(c, o.includeCloud));
    const restIdx = rest > 0 ? values.length : -1;
    if (rest > 0) values.push(rest);
    // Unattributed bytes (parent bigger than its children) stay as empty ground at the end.
    const gap = denom - kidSum;
    if (gap > 0) values.push(gap);
    const order = values.map((_, k) => k).sort((a, b) => values[b] - values[a]);
    const rects = squarify(order.map((k) => values[k]), x, y, w, h);

    parent.leaf = false;
    order.forEach((k, r) => {
      if (k >= keep && k !== restIdx) return; // the unattributed gap
      const rc = rects[r];
      const folded = k === restIdx;
      const c = folded ? null : kids[k];
      const idx = nodes.length;
      nodes.push({
        id: c ? c.id : parent.id, parent: pi, depth: parent.depth + 1,
        x0: rc.x, y0: rc.y, x1: rc.x + rc.w, y1: rc.y + rc.h,
        weight: values[k],
        allocated: c ? c.allocated : sumOf(kids, keep, (n) => n.allocated),
        cloudOnly: c ? cloudBytes(c) : (o.includeCloud ? sumOf(kids, keep, cloudBytes) : 0),
        category: c ? c.category : parent.category,
        leaf: true, folded, src: c,
      });
      if (c) {
        byId.set(c.id, idx);
        if (c.children.length && parent.depth + 1 < o.maxDepth && rc.w * rc.h >= o.minArea * 4) push(idx);
      }
    });
  }
  return { nodes, byId };
}

function sumOf(kids: TreeDto[], from: number, f: (n: TreeDto) => number): number {
  let s = 0;
  for (let k = from; k < kids.length; k++) s += f(kids[k]);
  return s;
}

/** Laid-out ancestors of node `idx`, root first, ending with `idx` itself. */
export function chainOf(layout: Layout, idx: number): number[] {
  const out: number[] = [];
  for (let k = idx; k >= 0; k = layout.nodes[k].parent) out.unshift(k);
  return out;
}

/**
 * Finds where nodes that may have been folded away live on the map: for each id, the index of
 * the node itself or its nearest laid-out ancestor. One iterative walk, so 1M-node trees are fine.
 */
export function locate(root: TreeDto, layout: Layout, ids: Iterable<NodeId>): Map<NodeId, number> {
  const want = new Set<NodeId>();
  const out = new Map<NodeId, number>();
  for (const id of ids) {
    const k = layout.byId.get(id);
    if (k !== undefined && !layout.nodes[k].folded) out.set(id, k);
    else want.add(id);
  }
  if (!want.size) return out;
  const stack: [TreeDto, number][] = [[root, 0]];
  while (stack.length && want.size) {
    const [n, near] = stack.pop()!;
    const own = layout.byId.get(n.id);
    const here = own !== undefined ? own : near;
    if (want.has(n.id)) { out.set(n.id, here); want.delete(n.id); }
    for (const c of n.children) stack.push([c, here]);
  }
  return out;
}

/** Index of the patch that owns square point (sx, sy): the deepest laid-out node containing it. */
export function nodeAt(layout: Layout, sx: number, sy: number): number {
  let best = -1, depth = -1;
  const ns = layout.nodes;
  for (let k = 0; k < ns.length; k++) {
    const n = ns[k];
    if (n.depth > depth && sx >= n.x0 && sx <= n.x1 && sy >= n.y0 && sy <= n.y1) { best = k; depth = n.depth; }
  }
  return best;
}

export const centroid = (n: LayoutNode): [number, number] => [(n.x0 + n.x1) / 2, (n.y0 + n.y1) / 2];
