import { describe, expect, it } from "vitest";
import { boxBlur, buildHeightmap, fromDisc, MAX_HEIGHT, raycastHeightfield, sampleHeight, toDisc } from "./heightmap";
import { centroid, layoutTree } from "./layout";
import { demoTree, dirNode } from "./fakeTree";
import { ringLabels, routePoints, routeStops, terrainArrays } from "./geometry";
import { chainOf } from "./layout";
import { paletteFor } from "./palette";

describe("disc mapping", () => {
  it("maps the square onto the unit disc and back", () => {
    for (const [x, y] of [[0, 0], [1, 0], [0, -1], [0.5, 0.25], [-0.9, 0.7], [1, 1], [-1, 1]]) {
      const [dx, dz] = toDisc(x, y);
      expect(dx * dx + dz * dz).toBeLessThanOrEqual(1 + 1e-9);
      const [sx, sy] = fromDisc(dx, dz);
      expect(sx).toBeCloseTo(x, 6);
      expect(sy).toBeCloseTo(y, 6);
    }
  });
});

describe("boxBlur", () => {
  it("preserves a constant field and spreads a spike", () => {
    const n = 9, a = new Float32Array(n * n).fill(2);
    boxBlur(a, n, 2);
    expect(Math.max(...a)).toBeCloseTo(2, 6);
    expect(Math.min(...a)).toBeCloseTo(2, 6);
    const b = new Float32Array(n * n); b[4 * n + 4] = 1;
    boxBlur(b, n, 1);
    expect(b[4 * n + 4]).toBeCloseTo(1 / 9, 6);
    expect(b[3 * n + 3]).toBeCloseTo(1 / 9, 6);
    expect(b[0]).toBe(0);
  });
});

describe("buildHeightmap", () => {
  const tree = demoTree();
  const layout = layoutTree(tree);
  const hm = buildHeightmap(layout, 128, { cloudValleys: true });

  it("is normalized to the max height and flat at the rim", () => {
    expect(hm.max).toBeGreaterThan(MAX_HEIGHT * 0.3);
    expect(hm.max).toBeLessThanOrEqual(MAX_HEIGHT + 1e-6);
    const N = hm.n;
    for (let i = 0; i < N; i++) {
      expect(Math.abs(hm.h[i])).toBeLessThan(1e-6);
      expect(Math.abs(hm.h[(N - 1) * N + i])).toBeLessThan(1e-6);
    }
    expect(hm.h.every((v) => Number.isFinite(v))).toBe(true);
  });

  it("carves cloud-only data below ground", () => {
    expect(hm.min).toBeLessThan(0);
    let below = 0, belowCloud = 0;
    for (let k = 0; k < hm.h.length; k++) if (hm.h[k] < -1e-3) { below++; if (hm.cloud[k]) belowCloud++; }
    expect(below).toBeGreaterThan(0);
    expect(belowCloud / below).toBeGreaterThan(0.7);
    const flat = buildHeightmap(layoutTree(tree, { includeCloud: false }), 128, { cloudValleys: false });
    expect(flat.min).toBeGreaterThanOrEqual(-1e-6);
  });

  it("records the owning patch for every sample", () => {
    expect(hm.owner.every((o) => o >= 0 && o < layout.nodes.length)).toBe(true);
    const k = layout.nodes.findIndex((n, i) => i > 0 && n.leaf && n.x1 - n.x0 > 0.05);
    const [cx, cy] = centroid(layout.nodes[k]);
    const N = hm.n, cell = 2 / (N - 1);
    expect(hm.owner[Math.round((cy + 1) / cell) * N + Math.round((cx + 1) / cell)]).toBe(k);
  });

  it("caps resolution regardless of tree size", () => {
    expect(hm.h.length).toBe(128 * 128);
  });

  it("is all zero for an empty tree", () => {
    const e = buildHeightmap(layoutTree(dirNode(1, null, "C:\\", [])), 32, { cloudValleys: true });
    expect(e.max).toBe(0);
    expect(e.min).toBe(0);
  });

  it("raycasts down onto the surface and up from below", () => {
    const sx = 0.1, sy = -0.2;
    const [dx, dz] = toDisc(sx, sy);
    const h = sampleHeight(hm, sx, sy);
    const down = raycastHeightfield(hm, [dx, 3, -dz], [0, -1, 0])!;
    expect(down).not.toBeNull();
    expect(down.point[1]).toBeCloseTo(h, 3);
    expect(down.sx).toBeCloseTo(sx, 3);
    expect(down.sy).toBeCloseTo(sy, 3);
    const up = raycastHeightfield(hm, [dx, -3, -dz], [0, 1, 0])!;
    expect(up.point[1]).toBeCloseTo(h, 3);
    // An oblique ray from the default camera hits somewhere on the disc.
    const d = [0 - 0, 0.12 - 2.55, 0 - 3.55], m = Math.hypot(...d);
    const hit = raycastHeightfield(hm, [0, 2.55, 3.55], [d[0] / m, d[1] / m, d[2] / m]);
    expect(hit).not.toBeNull();
    // A ray that misses the disc entirely.
    expect(raycastHeightfield(hm, [5, 1, 5], [0, -1, 0])).toBeNull();
  });
});

describe("geometry", () => {
  const layout = layoutTree(demoTree());
  const hm = buildHeightmap(layout, 64, { cloudValleys: true });

  it("builds mesh arrays sized to the heightmap", () => {
    const a = terrainArrays(hm, layout, paletteFor("dark"));
    expect(a.position.length).toBe(64 * 64 * 3);
    expect(a.index.length).toBe(63 * 63 * 6);
    expect(Math.max(...a.index)).toBe(64 * 64 - 1);
    expect(a.tint.every((v) => v >= 0 && v <= 1)).toBe(true);
  });

  it("routes from the rim through each folder to the target", () => {
    const leaf = layout.nodes.findIndex((n) => n.depth >= 3 && n.leaf);
    const chain = chainOf(layout, leaf);
    const stops = routeStops(layout, chain);
    expect(stops.length).toBe(chain.length); // rim entry + one per non-root folder
    expect(Math.max(Math.abs(stops[0][0]), Math.abs(stops[0][1]))).toBeCloseTo(1, 6);
    expect(stops[stops.length - 1]).toEqual(centroid(layout.nodes[leaf]));
    const pts = routePoints(hm, stops, 10);
    expect(pts.length).toBe(((stops.length - 1) * 10 + 1) * 3);
    expect(routeStops(layout, [0])).toEqual([]);
  });

  it("spaces compass ring labels apart", () => {
    const labels = ringLabels(layout, 8, 0.32);
    expect(labels.length).toBeGreaterThan(2);
    for (let a = 0; a < labels.length; a++) for (let b = a + 1; b < labels.length; b++) {
      const d = Math.abs(Math.atan2(Math.sin(labels[a].angle - labels[b].angle), Math.cos(labels[a].angle - labels[b].angle)));
      expect(d).toBeGreaterThanOrEqual(0.32);
    }
  });
});
