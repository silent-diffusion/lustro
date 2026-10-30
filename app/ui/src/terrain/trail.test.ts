import { describe, expect, it } from "vitest";
import { chainOf, layoutTree } from "./layout";
import { routeStops } from "./geometry";
import { demoTree } from "./fakeTree";
import { fmtShare, trailOf, visibleHops } from "./trail";

describe("trail", () => {
  const layout = layoutTree(demoTree());
  const leaf = layout.nodes.findIndex((n) => n.depth >= 3 && n.leaf && n.src);
  const chain = chainOf(layout, leaf);

  it("has one hop per route stop, root first, with each hop's share of its parent", () => {
    const t = trailOf(layout, leaf, layout.nodes[leaf].id);
    expect(t.hops.map((h) => h.idx)).toEqual(chain);
    expect(t.hops.length).toBe(routeStops(layout, chain).length); // waypoint k ↔ stop k
    expect(t.hops[0].share).toBeNull();
    expect(t.hops[0].name).toBe(demoTree().name);
    expect(t.hidden).toBe(0);
    for (let i = 1; i < t.hops.length; i++) {
      const h = t.hops[i];
      expect(h.share).toBeCloseTo(h.bytes / t.hops[i - 1].bytes, 9);
      expect(h.parentName).toBe(t.hops[i - 1].name);
    }
  });

  it("counts cloud-only bytes, as the map does, so a cloud-only folder isn't 0 B · 0%", () => {
    const l = layoutTree(demoTree(), { includeCloud: true });
    const k = l.nodes.findIndex((n) => n.depth >= 2 && n.src);
    const c = chainOf(l, k);
    // Make the last hop a cloud-only folder (a lake) and add its bytes to its ancestors.
    const gb = 4.2 * 1024 ** 3;
    const hop = l.nodes[k];
    for (const a of c) l.nodes[a].allocated -= a === k ? 0 : hop.allocated;
    hop.allocated = 0;
    hop.cloudOnly = gb;
    for (const a of c) if (a !== k) l.nodes[a].cloudOnly += gb;
    const t = trailOf(l, k, hop.id);
    const last = t.hops[t.hops.length - 1], parent = l.nodes[c[c.length - 2]];
    expect(last.bytes).toBe(gb);
    expect(last.cloud).toBe(true);
    expect(last.cloudOnly).toBe(gb);
    expect(last.share).toBeCloseTo(gb / (parent.allocated + parent.cloudOnly), 9);
    expect(last.share).toBeGreaterThan(0);
    expect(t.hops[0].cloud).toBe(false);
  });

  it("counts levels too small to draw below the last hop", () => {
    const ids = chain.map((k) => layout.nodes[k].id);
    expect(trailOf(layout, leaf, -42, [...ids, -41, -42]).hidden).toBe(2);
    expect(trailOf(layout, leaf, -42).hidden).toBe(-1);
  });

  it("keeps the root and the hops nearest the selection when long", () => {
    expect(visibleHops(4)).toEqual([0, 1, 2, 3]);
    expect(visibleHops(6)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(visibleHops(9)).toEqual([0, 1, null, 6, 7, 8]);
  });

  it("formats shares", () => {
    expect(fmtShare(0.514)).toBe("51%");
    expect(fmtShare(0.001)).toBe("<1%");
    expect(fmtShare(0)).toBe("0%");
    expect(fmtShare(1)).toBe("100%");
  });
});
