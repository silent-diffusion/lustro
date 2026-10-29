import { describe, expect, it } from "vitest";
import { LiveScan, app } from "../lib/app.svelte";
import { MockBackend } from "../ipc/mock";
import type { NodeDto, ScanDone, ScanUpdate } from "../ipc/types";

const dto = (id: number, parent: number | null, name: string, allocated: number, kind: NodeDto["kind"] = "dir"): NodeDto => ({
  id, parent, name, kind, allocated, logical: allocated, cloudOnly: 0, fileCount: 0, dirCount: 0, category: "other",
  modified: null, accessed: null, cloud: null, delta: null, hidden: false, accessDenied: false, notDescended: null, childCount: 0,
});
const upd = (nodes: NodeDto[], busiest: number | null = null): ScanUpdate => ({
  scanId: "s", progress: { files: 0, dirs: 0, allocatedBytes: 0, elapsedMs: 0 }, nodes, busiest,
});

describe("LiveScan", () => {
  it("builds a folded tree from what's been found, without reordering the folder lists", () => {
    const l = new LiveScan("s", "C:\\", "standard");
    l.apply(upd([dto(0, null, "C:\\", 1000), dto(1, 0, "Small", 1), dto(2, 0, "Big", 900), dto(3, 0, "Tiny", 2)]));
    const t = l.tree(0, 3, 0.01)!;
    expect(t.children.map((c) => c.name)).toEqual(["Big", "2 smaller items"]);
    expect(t.children[1].id).toBe(-1);
    expect(t.children[1].allocated).toBe(3);
    // The live folder tree keeps insertion order until it re-sorts.
    expect(l.children(0, false)).toEqual([1, 2, 3]);
    expect(l.children(0)).toEqual([2, 3, 1]);
  });

  it("updates sizes in place, tracks the largest file and builds paths", () => {
    const l = new LiveScan("s", "C:\\", "standard");
    l.apply(upd([dto(0, null, "C:\\", 10), dto(1, 0, "Users", 10), dto(2, 1, "a.mp4", 5, "file")]));
    l.apply(upd([dto(1, 0, "Users", 50), dto(3, 1, "b.iso", 40, "file")], 1));
    expect(l.nodes.get(1)!.allocated).toBe(50);
    expect(l.nodes.get(1)!.childCount).toBe(2);
    expect(l.largestFile).toBe(3);
    expect(l.pathText(3, "\\")).toBe("C:\\Users\\b.iso");
    expect([...l.hot()]).toEqual([1, 0]);
    expect(l.search({ text: "iso" }, 10).map((n) => n.id)).toEqual([3]);
  });

  it("mirrors a mock scan: the live root ends up with the full size", async () => {
    const b = new MockBackend({ nodes: 3000, scanMs: 200, updateCheckMs: -1 });
    const root = (await b.root())!;
    const id = await b.startScan({ root: "C:\\", mode: "standard", crossFilesystems: false, includeRemote: false, excluded: [] });
    const l = new LiveScan(id, "C:\\", "standard");
    b.onScanUpdate((u) => l.apply(u));
    const done = await new Promise<ScanDone>((r) => b.onScanDone(r));
    expect(done.cancelled).toBe(false);
    expect(l.rootId).toBe(root.id);
    expect(l.nodes.get(root.id)!.allocated).toBeCloseTo(root.allocated, -3);
    const t = l.tree(root.id, 4, 0.001)!;
    expect(t.children.length).toBeGreaterThan(0);
    // Category totals add up to everything found.
    const sum = [...l.byCategory().values()].reduce((a, x) => a + x, 0);
    expect(sum).toBeCloseTo(root.allocated, -3);
  });

  it("a new scan drops the selection: node ids belong to one scan", async () => {
    const b = new MockBackend({ nodes: 2000, scanMs: 5000, updateCheckMs: -1 });
    app.backend = b;
    app.selected = 42;
    await app.startScan(undefined, "standard");
    expect(app.selected).toBeNull();
    const first = app.scan!.id;
    await b.cancelScan(first);
    app.scan!.done = { scanId: first, modeUsed: "standard", progress: app.scan!.progress, cancelled: true, warnings: [], snapshotId: null };
    app.selected = 7;
    await app.startScan(undefined, "standard");
    expect(app.selected).toBeNull();
    expect(app.scan!.id).not.toBe(first);
    await b.cancelScan(app.scan!.id);
  });

  it("opens only the root and its children automatically", () => {
    const l = new LiveScan("s", "C:\\", "standard");
    l.apply(upd([dto(0, null, "C:\\", 10), dto(1, 0, "Windows", 10), dto(2, 1, "System32", 10)]));
    expect([...l.open]).toEqual([0, 1]);
  });
});
