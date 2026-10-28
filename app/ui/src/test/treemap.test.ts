import { describe, expect, it } from "vitest";
import { nestedLayout, squarify } from "../lib/treemap";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("fills the rectangle with areas proportional to values", () => {
    const vals = [6, 6, 4, 3, 2, 2, 1];
    const out = squarify(vals, { x: 0, y: 0, w: 600, h: 400 }, (v) => v);
    expect(out).toHaveLength(vals.length);
    const total = vals.reduce((a, b) => a + b, 0);
    out.forEach((r) => expect(area(r)).toBeCloseTo((r.item / total) * 600 * 400, 6));
    expect(out.reduce((s, r) => s + area(r), 0)).toBeCloseTo(600 * 400, 6);
  });

  it("stays inside the bounds and doesn't overlap", () => {
    const vals = Array.from({ length: 40 }, (_, i) => 100 / (i + 1));
    const out = squarify(vals, { x: 10, y: 20, w: 300, h: 200 }, (v) => v);
    for (const r of out) {
      expect(r.x).toBeGreaterThanOrEqual(10 - 1e-9);
      expect(r.y).toBeGreaterThanOrEqual(20 - 1e-9);
      expect(r.x + r.w).toBeLessThanOrEqual(310 + 1e-6);
      expect(r.y + r.h).toBeLessThanOrEqual(220 + 1e-6);
    }
    for (let i = 0; i < out.length; i++)
      for (let j = i + 1; j < out.length; j++) {
        const a = out[i], b = out[j];
        const ox = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
        const oy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
        expect(ox > 1e-6 && oy > 1e-6).toBe(false);
      }
  });

  it("produces reasonable aspect ratios (the classic example)", () => {
    const out = squarify([6, 6, 4, 3, 2, 2, 1], { x: 0, y: 0, w: 6, h: 4 }, (v) => v);
    const worst = Math.max(...out.map((r) => Math.max(r.w / r.h, r.h / r.w)));
    expect(worst).toBeLessThan(3);
  });

  it("skips zero values and handles empty input", () => {
    expect(squarify([], { x: 0, y: 0, w: 10, h: 10 }, (v: number) => v)).toEqual([]);
    expect(squarify([0, 0], { x: 0, y: 0, w: 10, h: 10 }, (v) => v)).toEqual([]);
    expect(squarify([1, 0, 1], { x: 0, y: 0, w: 10, h: 10 }, (v) => v)).toHaveLength(2);
    expect(squarify([1], { x: 0, y: 0, w: 0, h: 10 }, (v) => v)).toEqual([]);
  });
});

describe("nestedLayout", () => {
  type N = { v: number; children?: N[] };
  const tree: N = { v: 10, children: [
    { v: 6, children: [{ v: 4 }, { v: 2 }] },
    { v: 4 },
  ] };
  const opts = { value: (n: N) => n.v, children: (n: N) => n.children ?? [], maxDepth: 4 };

  it("nests children inside groups", () => {
    const cells = nestedLayout(tree, { x: 0, y: 0, w: 400, h: 300 }, opts);
    const group = cells.find((c) => c.group)!;
    expect(group.node.v).toBe(6);
    const inner = cells.filter((c) => c.depth === 2);
    expect(inner).toHaveLength(2);
    for (const c of inner) {
      expect(c.x).toBeGreaterThanOrEqual(group.x);
      expect(c.y).toBeGreaterThanOrEqual(group.y + group.head);
      expect(c.x + c.w).toBeLessThanOrEqual(group.x + group.w + 1e-6);
    }
  });

  it("stops at maxDepth", () => {
    const cells = nestedLayout(tree, { x: 0, y: 0, w: 400, h: 300 }, { ...opts, maxDepth: 1 });
    expect(cells.every((c) => !c.group)).toBe(true);
    expect(cells).toHaveLength(2);
  });
});
