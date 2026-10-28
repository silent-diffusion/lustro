// Layout and color helpers for the 2D views (block map, file tree / icicle). Pure apart from canvas helpers.

import type { Category, CloudInfo, NodeId } from "../ipc/types";
import { nestedLayout } from "./treemap";
import type { Palette } from "./app.svelte";

/** The subset of TreeDto the 2D views need (the live scan builds these too). */
export interface VizNode {
  id: NodeId;
  name: string;
  kind: "file" | "dir" | "link" | "other";
  allocated: number;
  cloudOnly: number;
  category: Category;
  cloud: CloudInfo | null;
  delta: number | null;
  children?: VizNode[];
  accessDenied?: boolean;
  parent?: NodeId | null;
  logical?: number;
  fileCount?: number;
  dirCount?: number;
  modified?: number | null;
  accessed?: number | null;
  hidden?: boolean;
  notDescended?: null | string;
  childCount?: number;
}

export interface Cell {
  node: VizNode;
  x: number;
  y: number;
  w: number;
  h: number;
  group: boolean;
  head: number;
  /** Index of the enclosing cell, or -1. */
  parent: number;
  depth: number;
}

type Sized = { allocated: number; cloudOnly: number };
export const size = (n: Sized) => n.allocated + n.cloudOnly;
export const isCloudOnly = (n: Sized) => n.allocated === 0 && n.cloudOnly > 0;

export function blockCells(root: VizNode, w: number, h: number, maxDepth: number): Cell[] {
  const raw = nestedLayout(root, { x: 0, y: 0, w, h }, { value: size, children: (n) => n.children ?? [], maxDepth });
  // Parents are emitted before their children; recover the parent index by id.
  const index = new Map<NodeId, number>();
  const parentOf = new Map<NodeId, NodeId>();
  const walk = (n: VizNode) => n.children?.forEach((c) => { parentOf.set(c.id, n.id); walk(c); });
  walk(root);
  return raw.map((c, i) => {
    index.set(c.node.id, i);
    const p = parentOf.get(c.node.id);
    return { ...c, parent: p !== undefined ? index.get(p) ?? -1 : -1 };
  });
}

/** File tree as an icicle chart: one column per folder level, bar height proportional to size. */
export function icicleCells(root: VizNode, w: number, h: number, cols = 5): Cell[] {
  const out: Cell[] = [];
  const cw = w / cols;
  const walk = (n: VizNode, y0: number, y1: number, d: number, parent: number) => {
    if (d >= cols) return;
    const total = size(n);
    if (total <= 0) return;
    let y = y0;
    for (const c of n.children ?? []) {
      const ch = ((y1 - y0) * size(c)) / total;
      let idx = parent;
      if (ch >= 1) {
        idx = out.length;
        out.push({ node: c, x: d * cw, y, w: cw, h: ch, group: false, head: 0, parent, depth: d + 1 });
        walk(c, y, y + ch, d + 1, idx);
      }
      y += ch;
    }
  };
  walk(root, 0, h, 0, -1);
  return out;
}

/** Index of the deepest cell containing the point, preferring leaves. */
export function hitTest(cells: Cell[], x: number, y: number): number {
  let hit = -1;
  for (let i = 0; i < cells.length; i++) {
    const c = cells[i];
    if (x >= c.x && x < c.x + c.w && y >= c.y && y < c.y + c.h) hit = i;
  }
  return hit;
}

export function isInside(cells: Cell[], i: number, ancestor: number): boolean {
  for (let x = i; x >= 0; x = cells[x].parent) if (x === ancestor) return true;
  return false;
}

// ---- Colors ----

function parseColor(c: string): [number, number, number] {
  const s = c.trim();
  if (s.startsWith("#")) {
    const h = s.length === 4 ? s.slice(1).split("").map((x) => x + x).join("") : s.slice(1);
    return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
  }
  const m = s.match(/\d+(\.\d+)?/g);
  return m ? [Number(m[0]), Number(m[1]), Number(m[2])] : [128, 128, 128];
}

/** Mix `a` into `b`: t = 1 is all `a`. */
export function mix(a: string, b: string, t: number): string {
  const x = parseColor(a), y = parseColor(b);
  const c = x.map((v, i) => Math.round(v * t + y[i] * (1 - t)));
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

/** What grew: coral for growth, lime for shrinkage, stronger for bigger changes (5 GB = full). */
export function deltaColor(p: Palette, delta: number | null): string {
  if (!delta) return p.surface3;
  const t = Math.min(1, Math.abs(delta) / 5e9);
  return mix(delta > 0 ? p.grew : p.shrank, p.surface3, 0.3 + t * 0.7);
}

const patterns = new WeakMap<CanvasRenderingContext2D, Map<string, CanvasPattern | null>>();

/** Diagonal hatch, as used for cloud-only data and items marked for removal. */
export function hatch(ctx: CanvasRenderingContext2D, color: string, gap = 7, width = 1.5): CanvasPattern | null {
  let m = patterns.get(ctx);
  if (!m) patterns.set(ctx, (m = new Map()));
  const key = `${color}|${gap}|${width}`;
  if (m.has(key)) return m.get(key)!;
  const c = document.createElement("canvas");
  c.width = gap;
  c.height = gap;
  const g = c.getContext("2d")!;
  g.strokeStyle = color;
  g.lineWidth = width;
  g.beginPath();
  for (let k = -1; k <= 1; k++) {
    g.moveTo(k * gap, gap);
    g.lineTo(k * gap + gap, 0);
  }
  g.stroke();
  const p = ctx.createPattern(c, "repeat");
  m.set(key, p);
  return p;
}

export function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, rr);
}
