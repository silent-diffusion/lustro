import { describe, expect, it } from "vitest";
import { chainOf, layoutTree, locate, nodeAt, squarify } from "./layout";
import { bigTree, countNodes, demoTree, dirNode } from "./fakeTree";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("tiles the rectangle with areas proportional to values", () => {
    const vals = [6, 6, 4, 3, 2, 2, 1];
    const rects = squarify(vals, 0, 0, 6, 4);
    const total = vals.reduce((a, b) => a + b, 0);
    rects.forEach((r, i) => expect(area(r)).toBeCloseTo((vals[i] / total) * 24, 6));
    const sum = rects.reduce((a, r) => a + area(r), 0);
    expect(sum).toBeCloseTo(24, 9);
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-9);
      expect(r.y).toBeGreaterThanOrEqual(-1e-9);
      expect(r.x + r.w).toBeLessThanOrEqual(6 + 1e-9);
      expect(r.y + r.h).toBeLessThanOrEqual(4 + 1e-9);
    }
  });

  it("does not overlap", () => {
    const rects = squarify([50, 20, 10, 9, 5, 3, 2, 1], -1, -1, 2, 2);
    for (let a = 0; a < rects.length; a++) for (let b = a + 1; b < rects.length; b++) {
      const A = rects[a], B = rects[b];
      const ox = Math.min(A.x + A.w, B.x + B.w) - Math.max(A.x, B.x);
      const oy = Math.min(A.y + A.h, B.y + B.h) - Math.max(A.y, B.y);
      expect(ox > 1e-9 && oy > 1e-9).toBe(false);
    }
  });

  it("keeps aspect ratios reasonable", () => {
    const rects = squarify([1, 1, 1, 1, 1, 1, 1, 1, 1], 0, 0, 3, 3);
    for (const r of rects) expect(Math.max(r.w / r.h, r.h / r.w)).toBeLessThan(3);
  });

  it("handles empty and zero input", () => {
    expect(squarify([], 0, 0, 1, 1)).toEqual([]);
    expect(squarify([0, 0], 0, 0, 1, 1).every((r) => r.w === 0)).toBe(true);
  });
});

describe("layoutTree", () => {
  it("lays children inside their parent, parents first", () => {
    const L = layoutTree(demoTree());
    expect(L.nodes[0].x0).toBe(-1);
    expect(L.nodes[0].x1).toBe(1);
    L.nodes.forEach((n, k) => {
      if (k === 0) return;
      const p = L.nodes[n.parent];
      expect(n.parent).toBeLessThan(k);
      expect(n.x0).toBeGreaterThanOrEqual(p.x0 - 1e-9);
      expect(n.x1).toBeLessThanOrEqual(p.x1 + 1e-9);
      expect(n.y0).toBeGreaterThanOrEqual(p.y0 - 1e-9);
      expect(n.y1).toBeLessThanOrEqual(p.y1 + 1e-9);
      expect(n.depth).toBe(p.depth + 1);
    });
  });

  it("gives top-level folders land in proportion to their size", () => {
    const t = demoTree();
    const L = layoutTree(t, { includeCloud: false });
    const top = L.nodes.filter((n) => n.depth === 1 && !n.folded);
    const total = top.reduce((a, n) => a + n.weight, 0);
    const totalArea = top.reduce((a, n) => a + (n.x1 - n.x0) * (n.y1 - n.y0), 0);
    for (const n of top) expect((n.x1 - n.x0) * (n.y1 - n.y0) / totalArea).toBeCloseTo(n.weight / total, 2);
  });

  it("counts cloud-only bytes as land only when asked", () => {
    const t = demoTree();
    const withCloud = layoutTree(t, { includeCloud: true });
    const without = layoutTree(t, { includeCloud: false });
    const cloudLeaf = withCloud.nodes.find((n) => n.src?.cloud?.state === "cloudOnly");
    expect(cloudLeaf).toBeDefined();
    expect(without.nodes.some((n) => n.src?.cloud?.state === "cloudOnly")).toBe(false);
  });

  it("caps the node budget and folds small children (level of detail)", () => {
    const t = bigTree(200_000, 3);
    expect(countNodes(t)).toBeGreaterThan(150_000);
    const L = layoutTree(t, { maxNodes: 3000, minArea: 1e-4 });
    expect(L.nodes.length).toBeLessThanOrEqual(3000);
    expect(L.nodes.some((n) => n.folded)).toBe(true);
    // Folded patches carry their parent's id and are not indexed by id.
    for (const n of L.nodes.filter((x) => x.folded)) {
      expect(n.id).toBe(L.nodes[n.parent].id);
      expect(n.src).toBeNull();
    }
  });

  it("finds folded nodes at their nearest laid-out ancestor", () => {
    const t = bigTree(50_000, 5);
    const L = layoutTree(t, { maxNodes: 200 });
    // Pick a deep node that is not laid out.
    let deep = t;
    while (deep.children.length) deep = deep.children[deep.children.length - 1];
    expect(L.byId.has(deep.id)).toBe(false);
    const found = locate(t, L, [deep.id, t.id]);
    expect(found.get(t.id)).toBe(0);
    const k = found.get(deep.id)!;
    expect(k).toBeGreaterThanOrEqual(0);
    expect(chainOf(L, k)[0]).toBe(0);
  });

  it("returns the deepest patch under a point", () => {
    const L = layoutTree(demoTree());
    const leaf = L.nodes.findIndex((n, k) => k > 0 && n.leaf && n.x1 - n.x0 > 0.01);
    const n = L.nodes[leaf];
    expect(nodeAt(L, (n.x0 + n.x1) / 2, (n.y0 + n.y1) / 2)).toBe(leaf);
  });

  it("handles a tree with no bytes", () => {
    const L = layoutTree(dirNode(1, null, "C:\\", []));
    expect(L.nodes.length).toBe(1);
  });

  it("lays out a 1M-node tree quickly", () => {
    const t = bigTree(1_000_000, 9);
    const t0 = performance.now();
    const L = layoutTree(t, { maxNodes: 8000, minArea: (2 / 255) ** 2 * 1.5 });
    const ms = performance.now() - t0;
    expect(L.nodes.length).toBeLessThanOrEqual(8000);
    expect(ms).toBeLessThan(1500);
  }, 60_000);
});
