// Squarified treemap layout (Bruls, Huizing, van Wijk). Pure functions, no DOM.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Placed<T> extends Rect {
  item: T;
}

/** Worst aspect ratio of a row of areas laid along a side of length `side`. */
function worst(sum: number, min: number, max: number, side: number): number {
  const s2 = sum * sum;
  const side2 = side * side;
  return Math.max((side2 * max) / s2, s2 / (side2 * min));
}

/**
 * Lay out `items` in the rectangle, areas proportional to `value(item)`.
 * Items should be sorted by value, largest first, for the best aspect ratios.
 * Items with a value of 0 or less are skipped.
 */
export function squarify<T>(items: readonly T[], rect: Rect, value: (t: T) => number): Placed<T>[] {
  const out: Placed<T>[] = [];
  let { x, y, w, h } = rect;
  if (w <= 0 || h <= 0) return out;
  let total = 0;
  for (const it of items) {
    const v = value(it);
    if (v > 0) total += v;
  }
  if (total <= 0) return out;
  const scale = (w * h) / total;
  const areas: { item: T; a: number }[] = [];
  for (const it of items) {
    const v = value(it);
    if (v > 0) areas.push({ item: it, a: v * scale });
  }

  let i = 0;
  while (i < areas.length) {
    const side = Math.min(w, h);
    // Grow the row while the worst aspect ratio improves.
    let sum = areas[i].a;
    let min = sum;
    let max = sum;
    let j = i + 1;
    while (j < areas.length) {
      const a = areas[j].a;
      const nsum = sum + a;
      const nmin = Math.min(min, a);
      const nmax = Math.max(max, a);
      if (worst(nsum, nmin, nmax, side) > worst(sum, min, max, side)) break;
      sum = nsum;
      min = nmin;
      max = nmax;
      j++;
    }
    // Lay out row [i, j) along the shorter side.
    if (w >= h) {
      const cw = Math.min(w, sum / h);
      let cy = y;
      for (let k = i; k < j; k++) {
        const ch = areas[k].a / cw;
        out.push({ item: areas[k].item, x, y: cy, w: cw, h: ch });
        cy += ch;
      }
      x += cw;
      w -= cw;
    } else {
      const ch = Math.min(h, sum / w);
      let cx = x;
      for (let k = i; k < j; k++) {
        const cw = areas[k].a / ch;
        out.push({ item: areas[k].item, x: cx, y, w: cw, h: ch });
        cx += cw;
      }
      y += ch;
      h -= ch;
    }
    i = j;
  }
  return out;
}

export interface TreeLike {
  children?: readonly TreeLike[];
}

export interface NestedCell<T> extends Rect {
  node: T;
  depth: number;
  /** True when the node is drawn as a group (its children are inside). */
  group: boolean;
  /** Group header strip height (0 if none). */
  head: number;
}

export interface NestedOpts<T> {
  value: (t: T) => number;
  children: (t: T) => readonly T[];
  maxDepth: number;
  /** Header height for groups that are big enough to carry a label. */
  headHeight?: number;
  pad?: number;
  /** Smallest group size (px) that is subdivided; smaller ones are drawn as a single cell. */
  minGroup?: number;
}

/**
 * Nested squarified layout, as in the block map: folders are drawn as groups with a header strip and
 * their children inside; leaves (and folders too small or too deep to open) are drawn as cells.
 */
export function nestedLayout<T>(root: T, rect: Rect, opts: NestedOpts<T>): NestedCell<T>[] {
  const out: NestedCell<T>[] = [];
  const headH = opts.headHeight ?? 16;
  const pad = opts.pad ?? 2;
  const minGroup = opts.minGroup ?? 18;
  const walk = (node: T, r: Rect, depth: number) => {
    const placed = squarify(opts.children(node), r, opts.value);
    for (const p of placed) {
      const kids = opts.children(p.item);
      const openable = kids.length > 0 && depth < opts.maxDepth && p.w > minGroup && p.h > minGroup;
      if (openable) {
        const head = p.w > 70 && p.h > 40 ? headH : 0;
        out.push({ node: p.item, x: p.x, y: p.y, w: p.w, h: p.h, depth, group: true, head });
        walk(p.item, { x: p.x + pad, y: p.y + pad + head, w: p.w - pad * 2, h: p.h - pad * 2 - head }, depth + 1);
      } else {
        out.push({ node: p.item, x: p.x, y: p.y, w: p.w, h: p.h, depth, group: false, head: 0 });
      }
    }
  };
  walk(root, rect, 1);
  return out;
}
