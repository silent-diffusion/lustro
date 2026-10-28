import { describe, expect, it, vi } from "vitest";
import { MockBackend } from "../ipc/mock";
import { buildMockTree, childrenOf, disp, GB, K_DIR } from "../ipc/mockTree";
import type { ScanDone, ScanUpdate, UpdateStatus } from "../ipc/types";

describe("mock tree generator", () => {
  it("is deterministic", () => {
    const a = buildMockTree("windows", 5000, 3);
    const b = buildMockTree("windows", 5000, 3);
    expect(a.count).toBe(b.count);
    expect(a.name.slice(0, a.count)).toEqual(b.name.slice(0, b.count));
    expect(a.allocated[0]).toBe(b.allocated[0]);
  });

  it("hits the requested node count and keeps parents before children", () => {
    const t = buildMockTree("linux", 20000);
    expect(t.count).toBe(20000);
    for (let i = 1; i < t.count; i++) expect(t.parent[i]).toBeLessThan(i);
  });

  it("aggregates folder sizes from their children", () => {
    const t = buildMockTree("macos", 8000);
    for (let i = 0; i < t.count; i++) {
      if (t.kind[i] !== K_DIR || t.childCount[i] === 0) continue;
      let sum = 0;
      for (const c of childrenOf(t, i)) sum += t.allocated[c];
      expect(Math.abs(sum - t.allocated[i])).toBeLessThan(1);
    }
  });

  it("keeps the prototype's totals (sizes in GB, within cluster rounding)", () => {
    const t = buildMockTree("windows", 3000);
    const docker = t.keys.get("docker")!;
    expect(t.allocated[docker] / GB).toBeCloseTo(15.3, 1);
    // Cloud-only bytes use no disk space.
    const od = [...Array(t.count).keys()].find((i) => t.name[i] === "OneDrive")!;
    expect(t.cloudOnly[od] / GB).toBeCloseTo(22.0, 1);
    expect(t.allocated[od] / GB).toBeCloseTo(8.9, 1);
  });

  it("sorts children by display size", () => {
    const t = buildMockTree("windows", 4000);
    const kids = childrenOf(t, 0);
    for (let i = 1; i < kids.length; i++) expect(disp(t, kids[i - 1])).toBeGreaterThanOrEqual(disp(t, kids[i]));
  });

  it("builds a million nodes", () => {
    const t0 = performance.now();
    const t = buildMockTree("windows", 1_000_000);
    expect(t.count).toBe(1_000_000);
    expect(performance.now() - t0).toBeLessThan(20000);
  }, 30000);
});

describe("MockBackend", () => {
  const mk = (p: "windows" | "macos" | "linux" = "windows") => new MockBackend({ platform: p, nodes: 6000, scanMs: 300 });

  it("uses fake per-platform paths", async () => {
    for (const [p, home] of [["windows", "C:\\Users\\William"], ["macos", "/Users/william"], ["linux", "/home/william"]] as const) {
      const b = mk(p);
      const s = await b.suggestions();
      expect(s.length).toBeGreaterThan(5);
      expect(s.some((x) => x.path.startsWith(home))).toBe(true);
    }
  });

  it("pages children and finds paths", async () => {
    const b = mk();
    const root = (await b.root())!;
    const kids = await b.children(root.id, "allocated", 0, 3);
    expect(kids).toHaveLength(3);
    expect(kids[0].allocated + kids[0].cloudOnly).toBeGreaterThanOrEqual(kids[1].allocated + kids[1].cloudOnly);
    const byName = await b.children(root.id, "name", 0, 100);
    const names = byName.map((k) => k.name);
    expect([...names].sort((x, y) => x.localeCompare(y, "en", { numeric: true, sensitivity: "base" }))).toEqual(names);
    const id = b.idOf("nodemod")!;
    const chain = await b.pathTo(id);
    expect(chain[0].id).toBe(root.id);
    expect(chain[chain.length - 1].name).toBe("node_modules");
    const det = await b.node(id);
    expect(det.path).toBe("C:\\Users\\William\\source\\webshop\\node_modules");
  });

  it("folds small children in subtrees", async () => {
    const b = mk();
    const tree = await b.subtree(0, 3, 0.01);
    const walk = (n: typeof tree, depth: number): number => n.children.reduce((s, c) => Math.max(s, walk(c, depth + 1)), depth);
    expect(walk(tree, 0)).toBeLessThanOrEqual(3);
    const sum = tree.children.reduce((s, c) => s + c.allocated, 0);
    expect(Math.abs(sum - tree.allocated)).toBeLessThan(1);
    const fold = tree.children.find((c) => c.id < 0)!;
    expect(fold.id).toBe(-(tree.id + 1));
    expect(fold.kind).toBe("other");
    expect(fold.childCount).toBeGreaterThan(0);
  });

  it("breaks down used space so the segments add up", async () => {
    const b = mk();
    const vols = await b.listVolumes();
    const sb = await b.spaceBreakdown(vols[0].id);
    const used = sb.segments.filter((s) => s.kind !== "cloudOnly").reduce((s, x) => s + x.bytes, 0);
    expect(Math.abs(used - sb.usedBytes) / sb.usedBytes).toBeLessThan(0.001);
    expect(sb.segments.some((s) => s.key === "pagefile")).toBe(true);
    expect(vols[0].totalBytes - vols[0].freeBytes).toBeCloseTo(sb.usedBytes, -3);
  });

  it("diffs the last two snapshots", async () => {
    const b = mk();
    const snaps = await b.listSnapshots("C:\\");
    expect(snaps.length).toBe(8);
    const d = await b.diff(snaps[6].id, snaps[7].id, 100);
    expect(d.entries.find((e) => e.name === "os-image-24.04.iso")?.status).toBe("new");
    expect(d.netDelta / GB).toBeCloseTo(16.0, 0);
    expect(d.entries.some((e) => e.status === "deleted")).toBe(true);
    expect(d.daysUntilFull).toBeGreaterThan(0);
  });

  it("searches and finds the top files", async () => {
    const b = mk();
    const hits = await b.search({ text: "node_modules" }, 10);
    expect(hits[0].name).toBe("node_modules");
    const top = await b.top("files", 5, {});
    expect(top[0].name).toBe("pagefile.sys");
    const cloud = await b.search({ cloud: "cloudOnly" }, 50);
    expect(cloud.every((n) => n.cloud?.state === "cloudOnly")).toBe(true);
  });

  it("applies and undoes a cleanup", async () => {
    const b = mk();
    const before = (await b.root())!.allocated;
    const s = await b.suggestions();
    const nm = s.find((x) => x.name === "node_modules")!;
    const free = s.find((x) => x.group === "freeUpCloud" && x.cloud?.state === "downloaded")!;
    const res = await b.applyCleanup({ decisions: [{ suggestionId: nm.id, decision: "remove" }, { suggestionId: free.id, decision: "remove" }], destination: { kind: "trash" } });
    expect(res.applied).toBe(2);
    expect(res.freedBytes).toBeCloseTo(nm.bytes + free.bytes, -3);
    expect((await b.root())!.allocated).toBeCloseTo(before - res.freedBytes, -3);
    expect((await b.suggestions()).find((x) => x.id === nm.id)).toBeUndefined();
    await b.undo(res.undoId);
    expect((await b.root())!.allocated).toBeCloseTo(before, -3);
    expect((await b.undoLog()).length).toBe(2);
  });

  it("clear history deletes snapshots, the undo log and keep decisions", async () => {
    const b = mk();
    const s = await b.suggestions();
    await b.applyCleanup({ decisions: [{ suggestionId: s[0].id, decision: "remove" }, { suggestionId: s[1].id, decision: "keep" }], destination: { kind: "trash" } });
    expect((await b.undoLog()).length).toBeGreaterThan(0);
    expect((await b.getSettings()).keptItems.length).toBe(1);
    await b.clearHistory();
    expect(await b.undoLog()).toEqual([]);
    expect(await b.listSnapshots("")).toEqual([]);
    expect((await b.getSettings()).keptItems).toEqual([]);
    // The current scan stays: its suggestions are still there, and the kept item comes back.
    expect((await b.suggestions()).some((x) => x.id === s[1].id)).toBe(true);
  });

  it("offers no duplicates when duplicate detection is off", async () => {
    const b = mk();
    expect((await b.suggestions()).some((s) => s.group === "duplicates")).toBe(true);
    await b.setSettings({ ...(await b.getSettings()), findDuplicates: false });
    expect((await b.suggestions()).some((s) => s.group === "duplicates")).toBe(false);
  });

  it("remembers kept items", async () => {
    const b = mk();
    const s = (await b.suggestions())[0];
    await b.applyCleanup({ decisions: [{ suggestionId: s.id, decision: "keep" }], destination: { kind: "trash" } });
    expect((await b.getSettings()).keptItems).toContain(s.path);
    expect((await b.suggestions()).find((x) => x.id === s.id)).toBeUndefined();
  });

  it("streams a live scan about 10 times a second", async () => {
    vi.useFakeTimers();
    try {
      const b = mk();
      const updates: ScanUpdate[] = [];
      let done: ScanDone | null = null;
      b.onScanUpdate((u) => updates.push(u));
      b.onScanDone((d) => (done = d));
      await b.startScan({ root: "C:\\", mode: "standard", crossFilesystems: false, includeRemote: false, excluded: [] });
      vi.advanceTimersByTime(250);
      expect(updates.length).toBe(2);
      vi.advanceTimersByTime(2000);
      expect(done).not.toBeNull();
      expect(done!.cancelled).toBe(false);
      const last = updates[updates.length - 1];
      expect(last.progress.files + last.progress.dirs).toBe(b.nodeCount);
      const rootUpd = last.nodes.find((n) => n.id === 0)!;
      expect(rootUpd.allocated).toBeCloseTo((await b.root())!.allocated, -3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("falls back to Standard when Fast scan is unavailable, with a path-less warning", async () => {
    vi.useFakeTimers();
    try {
      for (const fallback of [false, true]) {
        const b = new MockBackend({ platform: "windows", nodes: 3000, scanMs: 300, fastFallback: fallback });
        let done: ScanDone | null = null;
        b.onScanDone((d) => (done = d));
        await b.startScan({ root: "C:\\", mode: "fast", crossFilesystems: false, includeRemote: false, excluded: [] });
        vi.advanceTimersByTime(2000);
        expect(done!.modeUsed).toBe(fallback ? "standard" : "fast");
        const w = done!.warnings.find((x) => x.path === "");
        if (fallback) expect(w?.message).toMatch(/^Fast scan wasn't available/);
        else expect(w).toBeUndefined();
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels a scan", async () => {
    vi.useFakeTimers();
    try {
      const b = mk();
      let done: ScanDone | null = null;
      b.onScanDone((d) => (done = d));
      const id = await b.startScan({ root: "/", mode: "fast", crossFilesystems: false, includeRemote: false, excluded: [] });
      vi.advanceTimersByTime(100);
      await b.cancelScan(id);
      expect(done!.cancelled).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("summarizes cloud storage per provider", async () => {
    const win = await mk("windows").cloudSummary();
    const od = win.find((c) => c.provider === "oneDrive")!;
    expect(od.rootPath).toBe("C:\\Users\\William\\OneDrive");
    expect(od.cloudOnlyBytes / GB).toBeCloseTo(22.0, 1);
    expect(od.pinnedBytes / GB).toBeCloseTo(4.3, 1);
    expect(od.localBytes / GB).toBeCloseTo(4.6, 1);
    const gd = win.find((c) => c.provider === "googleDrive")!;
    expect(gd.virtualVolumeId).toBe("G:");
    expect(gd.cacheRoot).toContain("DriveFS");
    const linux = await mk("linux").cloudSummary();
    expect(linux.filter((c) => c.remote).map((c) => c.provider)).toEqual(["oneDrive", "googleDrive"]);
  });

  describe("updates", () => {
    const run = async <T,>(p: Promise<T>, ms: number) => { await vi.advanceTimersByTimeAsync(ms); return p; };

    it("checks, downloads, verifies and installs", async () => {
      vi.useFakeTimers();
      try {
        const b = mk();
        const seen: string[] = [];
        b.onUpdateStatus((s) => seen.push(s.state));
        expect(await b.appVersion()).toBe("0.2.0");
        expect(await b.currentReleaseNotes()).toContain("Live scan");
        expect((await b.getUpdateStatus()).state).toBe("idle");
        const st = await run(b.checkForUpdate(), 1000);
        expect(st.state).toBe("available");
        expect((await b.getUpdateStatus()).state).toBe("available");
        expect((await b.getSettings()).lastUpdateCheck).not.toBeNull();
        await run(b.installUpdate(), 5000);
        await vi.advanceTimersByTimeAsync(2000);
        expect(seen).toEqual(expect.arrayContaining(["checking", "available", "downloading", "ready", "idle"]));
        expect(await b.appVersion()).toBe("0.3.0");
        expect((await run(b.checkForUpdate(), 1000)).state).toBe("upToDate");
      } finally {
        vi.useRealTimers();
      }
    });

    it("reports each error kind", async () => {
      vi.useFakeTimers();
      try {
        const b = mk();
        b.setUpdateSim("offline");
        const st = await run(b.checkForUpdate(), 1000);
        expect(st).toMatchObject({ state: "error", kind: "offline" });
        expect((await b.getSettings()).lastUpdateCheck).toBeNull();
        for (const kind of ["download", "verify"] as const) {
          const c = mk();
          let last: UpdateStatus | null = null;
          c.onUpdateStatus((s) => (last = s));
          c.setUpdateSim(kind);
          await run(c.checkForUpdate(), 1000);
          await run(c.installUpdate(), 5000);
          expect(last).toMatchObject({ state: "error", kind });
          expect(await c.appVersion()).toBe("0.2.0");
        }
      } finally {
        vi.useRealTimers();
      }
    });

    it("downloads without a known size", async () => {
      vi.useFakeTimers();
      try {
        const b = mk();
        const totals: (number | null)[] = [];
        b.onUpdateStatus((s) => { if (s.state === "downloading") totals.push(s.total); });
        b.setUpdateSim("unknownSize");
        await run(b.checkForUpdate(), 1000);
        await run(b.installUpdate(), 5000);
        expect(totals.length).toBeGreaterThan(0);
        expect(totals.every((t) => t === null)).toBe(true);
      } finally {
        vi.useRealTimers();
      }
    });

    it("never restarts while a scan runs", async () => {
      vi.useFakeTimers();
      try {
        const b = new MockBackend({ platform: "windows", nodes: 6000, scanMs: 60_000 });
        await run(b.checkForUpdate(), 1000);
        await b.startScan({ root: "C:\\", mode: "standard", crossFilesystems: false, includeRemote: false, excluded: [] });
        let last: UpdateStatus | null = null;
        b.onUpdateStatus((s) => (last = s));
        await run(b.installUpdate(), 5000);
        await vi.advanceTimersByTimeAsync(3000);
        expect(last).toMatchObject({ state: "ready" });
        await b.restartToUpdate();
        expect(await b.appVersion()).toBe("0.2.0");
      } finally {
        vi.useRealTimers();
      }
    });

    it("has no updater in the portable build", async () => {
      vi.useFakeTimers();
      try {
        const b = new MockBackend({ platform: "windows", nodes: 3000, portable: true, updateCheckMs: 10 });
        const seen: string[] = [];
        b.onUpdateStatus((s) => seen.push(s.state));
        await vi.advanceTimersByTimeAsync(5000);
        expect(seen).toEqual([]);
        expect(await b.buildInfo()).toEqual({ portable: true });
        expect((await b.getSettings()).autoUpdate).toBe(false);
        expect((await b.getUpdateStatus()).state).toBe("idle");
        await expect(b.checkForUpdate()).rejects.toThrow(/portable/);
        await expect(b.installUpdate()).rejects.toThrow(/portable/);
        expect(await new MockBackend({ nodes: 3000 }).buildInfo()).toEqual({ portable: false });
      } finally {
        vi.useRealTimers();
      }
    });

    it("treats a skipped version as up to date", async () => {
      vi.useFakeTimers();
      try {
        const b = mk();
        await b.skipVersion("0.3.0");
        expect((await b.getSettings()).skippedVersion).toBe("0.3.0");
        expect((await run(b.checkForUpdate(), 1000)).state).toBe("upToDate");
        expect((await b.getSettings()).notesShownForVersion).toBe("0.2.0");
      } finally {
        vi.useRealTimers();
      }
    });
  });
});
