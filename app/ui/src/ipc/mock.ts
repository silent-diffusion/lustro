// Mock `Backend` for the browser: a deterministic fake tree (mockTree.ts) with a simulated live scan,
// snapshots for What grew, suggestions for Cleanup and an undo log. Fake paths only.

import type { Backend, Filter, SortKey, Unsubscribe } from "./backend";
import type {
  ApplyRequest, ApplyResult, BuildInfo, CloudProvider, CloudProviderSummary, DiffDto, DiffEntryDto, NodeDetailDto, NodeDto, NodeId, Platform,
  ScanDone, ScanRequest, ScanUpdate, Settings, SnapshotMeta, SpaceBreakdownDto, SpaceSegment, SuggestionDto,
  SuggestionGroup, Safety, TreeDto, UndoEntry, UpdateInfo, UpdateStatus, VolumeInfo, Category, Destination,
} from "./types";
import {
  buildMockTree, childrenOf, disp, finalize, pathOf, CATS, GB, MOCK_NOW, PROVIDERS, STATES,
  FL_BASE, FL_DENIED, FL_HIDDEN, FL_NEW, FL_REMOVED, FL_SYNCROOT, K_DIR, K_FILE, type MockTree,
} from "./mockTree";
import { homePath } from "./mockData";
import { DEFAULT_SETTINGS } from "./defaults";

export interface MockOptions {
  platform?: Platform;
  /** Approximate node count of the fake tree. */
  nodes?: number;
  /** How long a simulated scan takes. */
  scanMs?: number;
  /** Start with no completed scan (first-run state). */
  fresh?: boolean;
  seed?: number;
  /** Persist settings in localStorage (browser only). */
  persist?: boolean;
  /** How the simulated update check behaves (dev switch). */
  updateSim?: UpdateSim;
  /** Delay before the simulated launch check, ms (auto-update); -1 = no launch check. */
  updateCheckMs?: number;
  /** How long applying a cleanup takes (lets the UI show "busy"). */
  applyMs?: number;
  /** Simulate Fast scan being declined or unavailable: the scan runs as Standard, with a warning. */
  fastFallback?: boolean;
  /** Portable build: the updater is compiled out (no checks, no auto-update). */
  portable?: boolean;
}


/** ok: 0.3.0 is available · upToDate: nothing newer · offline / download / verify: that error ·
 *  unknownSize: the download doesn't report a total. */
export type UpdateSim = "ok" | "upToDate" | "offline" | "download" | "verify" | "unknownSize";

export { DEFAULT_SETTINGS };

/** Fake releases. The mock starts on 0.2.0; 0.3.0 is "published". */
export const MOCK_RELEASES: Record<string, { publishedAt: number; sizeBytes: number; notes: string }> = {
  "0.2.0": { publishedAt: Date.UTC(2026, 9, 1) / 1000, sizeBytes: 13.6e6, notes: "## What's new\n- **Live scan:** the folder tree grows as folders are measured.\n- **Cleanup** is a review queue: Remove, Keep or Later, one item at a time.\n- Cloud files from every provider show their state: cloud only, downloaded or always on this device." },
  "0.3.0": { publishedAt: Date.UTC(2026, 9, 27) / 1000, sizeBytes: 14.2e6, notes: "## What's new\n- **Faster scans** on large drives: the folder tree fills in about twice as fast.\n- **Terrain:** looking from below now shows the land's footprint, so cloud-only valleys are easier to read.\n- New **app icon** and logo.\n- Settings has a new **Updates** section.\n## Fixes\n- Hard-linked files are no longer counted twice in What grew.\n- The Cleanup queue keeps your place when you switch tabs.\n- `Copy path` now works for paths with spaces.\n## Notes\nEvery update is signed. Lustro checks the signature before installing and refuses anything that doesn't match." },
};
export const MOCK_FIRST_VERSION = "0.2.0";
export const MOCK_LATEST = "0.3.0";
const VERSION_KEY = "lustro-mock-version";

const KIND = ["file", "dir", "other"] as const;
const SETTINGS_KEY = "lustro-mock-settings";
const DAY = 86400;

// Snapshot history of used space on the main drive: GB below today's used space, oldest first.
const HISTORY: [string, number | "prev" | 0][] = [
  ["2026-07-28", 44.5], ["2026-08-11", 39.8], ["2026-08-25", 36.3], ["2026-09-08", 31.7],
  ["2026-09-22", 29.4], ["2026-10-06", 26.9], ["2026-10-21", "prev"], ["2026-10-28", 0],
];
const HISTORY_TIMES = [9 * 3600, 20 * 3600, 21 * 3600, 19 * 3600, 8 * 3600, 22 * 3600, 20 * 3600 + 12 * 60, 9 * 3600 + 41 * 60];

interface Snap { meta: SnapshotMeta; usedBytes: number; latest: boolean }

interface SuggestionSpec {
  key: string;
  group: SuggestionGroup;
  safety: Safety;
  reason: (o: string, w: { trash: string; temp: string }) => string;
  howFound: (w: { temp: string }) => string;
}

const SUGGESTIONS: SuggestionSpec[] = [
  { key: "dupA", group: "duplicates", safety: "safe", reason: () => "Byte-for-byte identical to another file. The original stays where it is.", howFound: () => "SHA-256 match" },
  { key: "dupB", group: "duplicates", safety: "safe", reason: () => "Same content as edit-v3-final.mp4 in the same folder.", howFound: () => "SHA-256 match" },
  { key: "cacache", group: "devCaches", safety: "safe", reason: () => "npm's download cache. It refills on its own the next time you install packages.", howFound: () => "Known cache folder" },
  { key: "nodemod", group: "devCaches", safety: "safe", reason: () => "Dependencies for the webshop project. Running npm install brings them back.", howFound: () => "Project dependency folder" },
  { key: "venv", group: "devCaches", safety: "safe", reason: () => "Python environment for ml-notebook. It can be recreated from requirements.txt.", howFound: () => "Project dependency folder" },
  { key: "tmp", group: "tempFiles", safety: "safe", reason: () => "Temporary files older than 7 days. Files that are open right now are skipped.", howFound: (w) => `${w.temp} folder` },
  { key: "bcache", group: "tempFiles", safety: "safe", reason: () => "Browser cache. Sites load a little slower the first time afterwards.", howFound: () => "Known cache folder" },
  { key: "inst14", group: "oldDownloads", safety: "review", reason: () => "Setup files for apps that are already installed.", howFound: () => "Installer in Downloads, > 6 months" },
  { key: "inst1", group: "oldDownloads", safety: "review", reason: () => "Photo Suite is installed; this is its installer.", howFound: () => "Installer in Downloads, > 6 months" },
  { key: "drv", group: "oldDownloads", safety: "review", reason: () => "An old driver package. Newer versions are already installed.", howFound: () => "Archive in Downloads, > 6 months" },
  { key: "oldbk", group: "staleLarge", safety: "review", reason: (o) => `Large archive you haven't opened in over two years. Consider moving it to ${o} instead.`, howFound: () => "Not opened in > 12 months" },
  { key: "older", group: "staleLarge", safety: "review", reason: (o) => `Screen recordings from 2023. Moving them to ${o} keeps them and frees space here.`, howFound: () => "Not opened in > 12 months" },
  { key: "freeOD", group: "freeUpCloud", safety: "safe", reason: () => "Downloaded from OneDrive. Freeing it makes it cloud-only again: it stays listed and re-downloads when you open it.", howFound: () => "Downloaded cloud file" },
  { key: "freeDB", group: "freeUpCloud", safety: "safe", reason: () => "Downloaded from Dropbox and not opened in months. The copy in Dropbox is untouched.", howFound: () => "Downloaded cloud file" },
  { key: "freeGD", group: "freeUpCloud", safety: "safe", reason: () => "Google Drive's local copy of files you opened recently. Freeing it keeps them in Google Drive, still listed.", howFound: () => "Google Drive cache" },
  { key: "pinOD", group: "freeUpCloud", safety: "caution", reason: () => "Downloaded from OneDrive and set to “Always keep on this device”.", howFound: () => "Pinned cloud file" },
];

interface UndoRecord {
  entry: UndoEntry;
  /** Files whose state changed: id, allocated, cloudOnly, pinned, state. */
  files: [number, number, number, number, number][];
  removed: number[];
  suggestionIds: string[];
}

export class MockBackend implements Backend {
  readonly opts: Required<Omit<MockOptions, "platform">> & { platform: Platform };
  private t: MockTree;
  private settings: Settings;
  private hasScan: boolean;
  private snaps: Snap[] = [];
  private items: number[] = [];
  private sortCache = new Map<string, Int32Array>();
  private updateCbs = new Set<(u: ScanUpdate) => void>();
  private doneCbs = new Set<(d: ScanDone) => void>();
  private scan: { id: string; timer: ReturnType<typeof setInterval>; started: number; mode: ScanRequest["mode"] } | null = null;
  private undoRecords: UndoRecord[] = [];
  private applied = new Set<string>();
  private seq = 0;

  constructor(options: MockOptions = {}) {
    this.opts = {
      platform: options.platform ?? "windows",
      nodes: options.nodes ?? 60_000,
      scanMs: options.scanMs ?? 8000,
      fresh: options.fresh ?? false,
      seed: options.seed ?? 7,
      persist: options.persist ?? false,
      updateSim: options.updateSim ?? "ok",
      updateCheckMs: options.updateCheckMs ?? -1,
      applyMs: options.applyMs ?? 0,
      fastFallback: options.fastFallback ?? false,
      portable: options.portable ?? false,
    };
    this.installed = MOCK_FIRST_VERSION;
    if (this.opts.persist) {
      try { this.installed = localStorage.getItem(VERSION_KEY) ?? MOCK_FIRST_VERSION; } catch { /* storage unavailable */ }
    }
    this.t = buildMockTree(this.opts.platform, this.opts.nodes, this.opts.seed);
    this.hasScan = !this.opts.fresh;
    this.settings = this.loadSettings();
    this.buildItems();
    this.buildSnapshots();
    this.seedUndo();
    // Auto-update: check on launch, download in the background, install on the next restart.
    if (!this.opts.portable && this.settings.autoUpdate && this.opts.updateCheckMs >= 0) {
      setTimeout(() => {
        void this.checkForUpdate().then((st) => {
          if (st.state === "available" && this.settings.autoUpdate) void this.download(false);
        });
      }, this.opts.updateCheckMs);
    }
  }

  /** Node count of the fake tree (for the dev panel and tests). */
  get nodeCount(): number {
    return this.t.count;
  }

  /** Test helper: find a node by its stable base key. */
  idOf(key: string): NodeId | undefined {
    return this.t.keys.get(key);
  }

  // ---- Platform and volumes ----

  async platform(): Promise<Platform> {
    return this.opts.platform;
  }

  private usedBytes(): number {
    const w = this.t.world;
    return this.t.allocated[0] + w.unexplainedGb * GB + w.extraSys.reduce((s, x) => s + x.gb * GB, 0);
  }

  async listVolumes(): Promise<VolumeInfo[]> {
    return this.t.world.volumes.map((v, i) => {
      const used = i === 0 ? this.usedBytes() : (v.used ?? 0) * GB;
      return {
        id: v.id, label: v.label, mountPath: v.mountPath, fileSystem: v.fileSystem,
        totalBytes: v.total * GB, freeBytes: Math.max(0, v.total * GB - used),
        isRemote: v.isRemote, isRemovable: v.isRemovable,
      };
    });
  }

  // ---- Scan ----

  async startScan(req: ScanRequest): Promise<string> {
    if (this.scan) await this.cancelScan(this.scan.id);
    const id = `scan-${++this.seq}`;
    const t = this.t;
    const n = t.count;
    const ticks = Math.max(10, Math.round(this.opts.scanMs / 100));
    const per = Math.ceil(n / ticks);
    const pAlloc = new Float64Array(n), pCloud = new Float64Array(n), pFiles = new Float64Array(n), pDirs = new Float64Array(n);
    let next = 0, files = 0, dirs = 0, bytes = 0;
    const started = Date.now();
    const tick = () => {
      const end = Math.min(n, next + per);
      const changed = new Set<number>();
      const out: NodeDto[] = [];
      for (let i = next; i < end; i++) {
        if (t.flags[i] & FL_REMOVED) continue;
        const isDir = t.kind[i] === K_DIR;
        // Folders we can't read report their size from the volume, so they count like a file.
        const sized = !isDir || !!(t.flags[i] & FL_DENIED);
        if (isDir) dirs++;
        else files++;
        if (sized) {
          bytes += t.allocated[i];
          pAlloc[i] = t.allocated[i];
          pCloud[i] = t.cloudOnly[i];
        }
        // Propagate to ancestors.
        for (let p = t.parent[i]; p >= 0; p = t.parent[p]) {
          if (isDir) pDirs[p]++;
          else pFiles[p]++;
          if (sized) { pAlloc[p] += t.allocated[i]; pCloud[p] += t.cloudOnly[i]; }
          changed.add(p);
        }
        // Folders and shallow files go out; deep files only show up in their folder's totals.
        if (isDir || t.depth[i] <= 3) changed.add(i);
      }
      for (const i of changed) {
        const d = this.toDto(i);
        d.allocated = pAlloc[i];
        d.logical = pAlloc[i] + pCloud[i];
        d.cloudOnly = pCloud[i];
        if (t.kind[i] === K_DIR) { d.fileCount = pFiles[i]; d.dirCount = pDirs[i]; }
        out.push(d);
      }
      let busiest: number | null = null;
      if (end > 0) {
        let x = end - 1;
        while (x > 0 && t.depth[x] > 3) x = t.parent[x];
        busiest = x;
      }
      next = end;
      const update: ScanUpdate = { scanId: id, progress: { files, dirs, allocatedBytes: bytes, elapsedMs: Date.now() - started }, nodes: out, busiest };
      this.updateCbs.forEach((cb) => cb(update));
      if (next >= n) this.finishScan(false);
    };
    this.scan = { id, timer: setInterval(tick, 100), started, mode: req.mode };
    this.sumProgress = () => ({ files, dirs, allocatedBytes: bytes, elapsedMs: Date.now() - started });
    return id;
  }

  private sumProgress: () => ScanDone["progress"] = () => ({ files: 0, dirs: 0, allocatedBytes: 0, elapsedMs: 0 });

  private finishScan(cancelled: boolean) {
    if (!this.scan) return;
    clearInterval(this.scan.timer);
    const { id, mode } = this.scan;
    this.scan = null;
    let snapshotId: string | null = null;
    if (!cancelled) {
      this.hasScan = true;
      // A re-scan refreshes the latest snapshot (the fake tree doesn't change between scans).
      const latest = this.snaps[this.snaps.length - 1];
      latest.meta.takenAt = Math.floor(Date.now() / 1000);
      snapshotId = latest.meta.id;
    }
    const fellBack = mode === "fast" && this.opts.fastFallback;
    const done: ScanDone = {
      scanId: id, modeUsed: fellBack ? "standard" : mode, progress: this.sumProgress(), cancelled,
      warnings: [
        ...(fellBack ? [{ path: "", message: "Fast scan wasn't available (permission declined), so Lustro ran a Standard scan." }] : []),
        ...(this.opts.platform === "windows" && !(mode === "fast" && !fellBack) ? [{ path: this.t.world.rootPath + "System Volume Information", message: "Access denied: needs admin. Size taken from the volume." }] : []),
      ],
      snapshotId,
    };
    this.doneCbs.forEach((cb) => cb(done));
  }

  async cancelScan(scanId: string): Promise<void> {
    if (this.scan && this.scan.id === scanId) this.finishScan(true);
  }

  onScanUpdate(cb: (u: ScanUpdate) => void): Unsubscribe {
    this.updateCbs.add(cb);
    return () => this.updateCbs.delete(cb);
  }

  onScanDone(cb: (d: ScanDone) => void): Unsubscribe {
    this.doneCbs.add(cb);
    return () => this.doneCbs.delete(cb);
  }

  // ---- Tree queries ----

  private toDto(id: number): NodeDto {
    const t = this.t;
    const k = t.kind[id];
    const prov = PROVIDERS[t.provider[id]];
    const st = STATES[t.state[id]];
    return {
      id,
      parent: t.parent[id] < 0 ? null : t.parent[id],
      name: t.name[id],
      kind: KIND[k],
      allocated: t.allocated[id],
      logical: t.logical[id],
      cloudOnly: t.cloudOnly[id],
      fileCount: k === K_DIR ? t.fileCount[id] : 1,
      dirCount: t.dirCount[id],
      category: CATS[t.category[id]],
      modified: t.modified[id] || null,
      accessed: t.accessed[id] || null,
      cloud: prov && st ? { provider: prov, state: st } : null,
      delta: this.snaps.length > 1 ? t.delta[id] : null,
      hidden: !!(t.flags[id] & FL_HIDDEN),
      accessDenied: !!(t.flags[id] & FL_DENIED),
      notDescended: null,
      childCount: t.childCount[id],
    };
  }

  private check(id: NodeId) {
    if (!Number.isInteger(id) || id < 0 || id >= this.t.count || this.t.flags[id] & FL_REMOVED) throw new Error(`unknown node ${id}`);
  }

  async root(): Promise<NodeDto | null> {
    return this.hasScan ? this.toDto(0) : null;
  }

  private sorted(id: number, sort: SortKey): Int32Array {
    const base = childrenOf(this.t, id);
    if (sort === "allocated") return base;
    const key = `${id}:${sort}`;
    let s = this.sortCache.get(key);
    if (!s) {
      const t = this.t;
      s = Int32Array.from(base);
      const cmp: Record<Exclude<SortKey, "allocated">, (a: number, b: number) => number> = {
        logical: (a, b) => t.logical[b] - t.logical[a],
        name: (a, b) => t.name[a].localeCompare(t.name[b], "en", { numeric: true, sensitivity: "base" }),
        modified: (a, b) => t.modified[b] - t.modified[a],
        fileCount: (a, b) => t.fileCount[b] - t.fileCount[a],
        delta: (a, b) => Math.abs(t.delta[b]) - Math.abs(t.delta[a]),
      };
      s.sort(cmp[sort]);
      this.sortCache.set(key, s);
    }
    return s;
  }

  async children(id: NodeId, sort: SortKey, offset: number, limit: number): Promise<NodeDto[]> {
    this.check(id);
    const list = this.sorted(id, sort);
    const out: NodeDto[] = [];
    for (let i = Math.max(0, offset); i < Math.min(list.length, offset + limit); i++) out.push(this.toDto(list[i]));
    return out;
  }

  async node(id: NodeId): Promise<NodeDetailDto> {
    this.check(id);
    const t = this.t;
    const d = this.toDto(id);
    let categoryMix: { category: Category; allocated: number }[];
    if (t.kind[id] === K_DIR) {
      const s = t.dirSlot[id] * 10;
      categoryMix = CATS.map((c, i) => ({ category: c, allocated: t.mix[s + i] })).filter((m) => m.allocated > 0).sort((a, b) => b.allocated - a.allocated);
    } else {
      categoryMix = [{ category: d.category, allocated: disp(t, id) }];
    }
    const kids = childrenOf(t, id);
    const largestChildren: NodeDto[] = [];
    for (let i = 0; i < Math.min(5, kids.length); i++) largestChildren.push(this.toDto(kids[i]));
    const p = t.parent[id];
    return { ...d, path: pathOf(t, id), categoryMix, largestChildren, shareOfParent: p >= 0 && disp(t, p) > 0 ? disp(t, id) / disp(t, p) : 1 };
  }

  async pathTo(id: NodeId): Promise<NodeDto[]> {
    this.check(id);
    const out: NodeDto[] = [];
    for (let x = id; x >= 0; x = this.t.parent[x]) out.push(this.toDto(x));
    return out.reverse();
  }

  async subtree(id: NodeId, maxDepth: number, minFraction: number): Promise<TreeDto> {
    this.check(id);
    const t = this.t;
    const threshold = disp(t, id) * minFraction;
    const build = (x: number, depth: number): TreeDto => {
      const node = this.toDto(x) as TreeDto;
      node.children = [];
      if (depth >= maxDepth) return node;
      const kids = childrenOf(t, x);
      let folded = 0, fAlloc = 0, fLogical = 0, fCloud = 0, fFiles = 0;
      for (let i = 0; i < kids.length; i++) {
        const c = kids[i];
        if (disp(t, c) >= threshold && disp(t, c) > 0) node.children.push(build(c, depth + 1));
        else {
          folded++;
          fAlloc += t.allocated[c]; fLogical += t.logical[c]; fCloud += t.cloudOnly[c];
          fFiles += t.kind[c] === K_DIR ? t.fileCount[c] : 1;
        }
      }
      if (folded && fAlloc + fCloud > 0) {
        node.children.push({
          id: -(x + 1), parent: x, name: `${folded.toLocaleString("en-US")} smaller item${folded === 1 ? "" : "s"}`, kind: "other",
          allocated: fAlloc, logical: fLogical, cloudOnly: fCloud, fileCount: fFiles, dirCount: 0, category: node.category,
          modified: null, accessed: null, cloud: null, delta: null, hidden: false, accessDenied: false, notDescended: null,
          childCount: folded, children: [],
        });
      }
      return node;
    };
    return build(id, 0);
  }

  private matcher(filter: Filter): (i: number) => boolean {
    const t = this.t;
    const text = filter.text?.trim().toLowerCase() || "";
    const cats = filter.categories?.length ? new Set(filter.categories.map((c) => CATS.indexOf(c as Category))) : null;
    const cutoff = filter.olderThanDays != null ? MOCK_NOW - filter.olderThanDays * DAY : null;
    const cloudOnlyState = STATES.indexOf("cloudOnly");
    return (i) => {
      if (t.flags[i] & FL_REMOVED) return false;
      const size = disp(t, i);
      if (filter.minBytes != null && size < filter.minBytes) return false;
      if (filter.maxBytes != null && size > filter.maxBytes) return false;
      if (cats && !cats.has(t.category[i])) return false;
      if (cutoff != null && t.modified[i] > cutoff) return false;
      if (filter.cloud === "local" && t.state[i] === cloudOnlyState) return false;
      if (filter.cloud === "cloudOnly" && t.state[i] !== cloudOnlyState) return false;
      if (text && !t.name[i].toLowerCase().includes(text)) return false;
      return true;
    };
  }

  async top(kind: "files" | "dirs", n: number, filter: Filter): Promise<NodeDto[]> {
    const t = this.t;
    const match = this.matcher(filter);
    const best: number[] = [];
    for (let i = 1; i < t.count; i++) {
      if ((t.kind[i] === K_DIR) !== (kind === "dirs")) continue;
      const v = disp(t, i);
      if (best.length === n && v <= disp(t, best[n - 1])) continue;
      if (!match(i)) continue;
      let j = best.length;
      while (j > 0 && disp(t, best[j - 1]) < v) j--;
      best.splice(j, 0, i);
      if (best.length > n) best.pop();
    }
    return best.map((i) => this.toDto(i));
  }

  async search(filter: Filter, limit: number): Promise<NodeDto[]> {
    const t = this.t;
    const match = this.matcher(filter);
    const hits: number[] = [];
    for (let i = 1; i < t.count; i++) if (match(i)) hits.push(i);
    hits.sort((a, b) => disp(t, b) - disp(t, a));
    return hits.slice(0, limit).map((i) => this.toDto(i));
  }

  async spaceBreakdown(volumeId: string): Promise<SpaceBreakdownDto> {
    const t = this.t;
    const w = t.world;
    if (volumeId !== w.volumes[0].id) {
      const v = w.volumes.find((x) => x.id === volumeId);
      const used = (v?.used ?? 0) * GB;
      return { volumeId, usedBytes: used, segments: [{ kind: "unexplained", key: "unscanned", label: "Not scanned", bytes: used, explanation: "This drive hasn't been scanned yet.", settingAction: null }] };
    }
    // Category bytes on disk, with system items carved out as their own segments.
    const byCat = new Float64Array(10);
    for (let i = 1; i < t.count; i++) if (t.kind[i] !== K_DIR && !(t.flags[i] & FL_REMOVED)) byCat[t.category[i]] += t.allocated[i];
    const sysSegs: SpaceSegment[] = [];
    for (const [id, s] of t.sys) {
      if (t.flags[id] & FL_REMOVED) continue;
      const bytes = t.allocated[id];
      if (t.flags[id] & FL_DENIED) byCat[CATS.indexOf("system")] += bytes;
      byCat[t.category[id]] -= bytes;
      sysSegs.push({ kind: "system", key: s.key, label: t.name[id], bytes, explanation: s.explanation, settingAction: s.settingAction });
    }
    for (const x of w.extraSys) sysSegs.push({ kind: "system", key: x.key, label: x.label, bytes: x.gb * GB, explanation: x.explanation, settingAction: null });
    sysSegs.sort((a, b) => b.bytes - a.bytes);
    const catSegs: SpaceSegment[] = CATS.map((c, i) => ({
      kind: "category" as const, key: c, label: c, bytes: Math.max(0, byCat[i]), explanation: "", settingAction: null,
    })).filter((s) => s.bytes > 0).sort((a, b) => b.bytes - a.bytes);
    const segments: SpaceSegment[] = [
      ...catSegs,
      ...sysSegs,
      { kind: "cloudOnly", key: "cloudOnly", label: "Cloud-only files", bytes: t.cloudOnly[0],
        explanation: "Cloud-only files (OneDrive, Google Drive, iCloud, Dropbox, Box…): listed in folders, 0 bytes on this disk. Not counted in used space.", settingAction: null },
      { kind: "unexplained", key: "unexplained", label: "Unexplained", bytes: w.unexplainedGb * GB, explanation: "", settingAction: null },
    ];
    return { volumeId, usedBytes: this.usedBytes(), segments };
  }

  async cloudSummary(): Promise<CloudProviderSummary[]> {
    const t = this.t;
    const names: Record<CloudProvider, string> = {
      oneDrive: "OneDrive - Personal", googleDrive: "Google Drive", iCloud: "iCloud Drive", dropbox: "Dropbox", box: "Box", other: "Sync Folder",
    };
    const out = new Map<CloudProvider, CloudProviderSummary>();
    for (let i = 0; i < t.count; i++) {
      if (!(t.flags[i] & FL_SYNCROOT) || t.flags[i] & FL_REMOVED) continue;
      const p = PROVIDERS[t.provider[i]]!;
      const s = out.get(p) ?? { provider: p, displayName: names[p], rootPath: pathOf(t, i), localBytes: 0, pinnedBytes: 0, cloudOnlyBytes: 0, remote: false, virtualVolumeId: null, cacheRoot: null };
      s.localBytes += t.allocated[i] - t.pinned[i];
      s.pinnedBytes += t.pinned[i];
      s.cloudOnlyBytes += t.cloudOnly[i];
      out.set(p, s);
    }
    // Google Drive in Stream mode: files live on a virtual drive; only the cache folder holds local bytes.
    for (const v of t.world.volumes) {
      if (!v.provider || !v.cloudTotal) continue;
      const s = out.get(v.provider);
      if (s) {
        s.cacheRoot = s.rootPath;
        s.rootPath = v.mountPath;
        s.virtualVolumeId = v.id;
        s.remote = true;
        s.cloudOnlyBytes = Math.max(0, v.cloudTotal * GB - s.localBytes - s.pinnedBytes);
      }
    }
    const total = (x: CloudProviderSummary) => x.localBytes + x.pinnedBytes + x.cloudOnlyBytes;
    const list = [...out.values()].sort((a, b) => total(b) - total(a));
    for (const r of t.world.remotes) {
      list.push({ provider: r.provider, displayName: `${names[r.provider]} (${r.via})`, rootPath: r.path, localBytes: 0, pinnedBytes: 0, cloudOnlyBytes: 0, remote: true, virtualVolumeId: null, cacheRoot: null });
    }
    return list;
  }

  // ---- Snapshots and diff ----

  private buildItems() {
    const t = this.t;
    this.items = [];
    for (let i = 1; i < t.count; i++) {
      if (!(t.flags[i] & FL_BASE)) continue;
      const kids = childrenOf(t, i);
      let baseKids = false;
      for (let k = 0; k < kids.length; k++) if (t.flags[kids[k]] & FL_BASE) { baseKids = true; break; }
      if (!baseKids) this.items.push(i);
    }
  }

  private buildSnapshots() {
    const used = this.usedBytes();
    const prevOffset = this.t.delta[0];
    const root = this.t.world.rootPath;
    this.snaps = HISTORY.map(([date, off], k) => {
      const [y, m, d] = date.split("-").map(Number);
      const usedBytes = off === "prev" ? used - prevOffset : used - off * GB;
      return {
        meta: { id: `snap-${date}`, root, takenAt: Date.UTC(y, m - 1, d) / 1000 + HISTORY_TIMES[k], allocatedBytes: usedBytes, fileCount: Math.round(this.t.fileCount[0] * (usedBytes / used)) },
        usedBytes, latest: k === HISTORY.length - 1,
      };
    });
    if (!this.hasScan) this.snaps = [];
  }

  async listSnapshots(_root: string): Promise<SnapshotMeta[]> {
    return this.snaps.map((s) => ({ ...s.meta }));
  }

  /** Growth factor of snapshot k relative to the previous → latest change (1 = previous, 0 = latest). */
  private growthFactor(k: number): number {
    const n = this.snaps.length;
    const latest = this.snaps[n - 1].usedBytes;
    const prev = this.snaps[Math.max(0, n - 2)].usedBytes;
    if (latest === prev) return k === n - 1 ? 0 : 1;
    return (latest - this.snaps[k].usedBytes) / (latest - prev);
  }

  private sizeAt(id: number, k: number): number {
    const t = this.t;
    const latest = this.snaps.length - 1;
    if (k === latest) return t.allocated[id];
    if (t.flags[id] & FL_NEW) return 0;
    return Math.max(0, t.allocated[id] - t.delta[id] * this.growthFactor(k));
  }

  async diff(fromId: string, toId: string, limit: number): Promise<DiffDto> {
    const a = this.snaps.findIndex((s) => s.meta.id === fromId);
    const b = this.snaps.findIndex((s) => s.meta.id === toId);
    if (a < 0 || b < 0) throw new Error("unknown snapshot");
    const t = this.t;
    const entries: DiffEntryDto[] = [];
    const ks: number[] = [];
    // Sparklines show the whole history up to B, so a short comparison still shows the trend.
    for (let k = 0; k <= Math.max(a, b); k++) ks.push(k);
    for (const id of this.items) {
      if (t.flags[id] & FL_REMOVED || !t.delta[id]) continue;
      const before = this.sizeAt(id, a), after = this.sizeAt(id, b);
      const delta = after - before;
      if (Math.abs(delta) < 1) continue;
      entries.push({
        id, path: pathOf(t, id), name: t.name[id], kind: KIND[t.kind[id]], category: CATS[t.category[id]],
        before, after, delta, sparkline: ks.map((k) => this.sizeAt(id, k)),
        status: before === 0 ? "new" : delta > 0 ? "grew" : "shrank",
      });
    }
    const net = entries.reduce((s, e) => s + e.delta, 0);
    // Deleted items that explain the shrinkers (only when comparing to an older snapshot).
    const sep = t.world.sep;
    if (a < b) {
      const pixel = t.keys.get("pixel");
      const trashId = this.items.find((i) => t.name[i] === "$Recycle.Bin" || t.name[i] === ".Trash" || (t.name[i] === "files" && pathOf(t, i).includes("Trash")));
      const f = this.growthFactor(a) - this.growthFactor(b);
      if (pixel !== undefined) entries.push(this.deletedEntry(`${pathOf(t, pixel)}${sep}DLC`, "DLC", "apps", 5.0 * GB * f, ks.length));
      if (trashId !== undefined) entries.push(this.deletedEntry(`${pathOf(t, trashId)}${sep}(emptied)`, `${this.opts.platform === "windows" ? "Recycle Bin" : "Trash"} emptied`, "other", 2.1 * GB * f, ks.length));
    }
    entries.sort((x, y) => Math.abs(y.delta) - Math.abs(x.delta));
    const trend = this.snaps.map((s) => ({ takenAt: s.meta.takenAt, usedBytes: s.usedBytes }));
    return {
      from: { ...this.snaps[a].meta }, to: { ...this.snaps[b].meta }, netDelta: net,
      grew: entries.filter((e) => e.status !== "deleted" && e.delta > 0).reduce((s, e) => s + e.delta, 0),
      shrank: entries.filter((e) => e.status !== "deleted" && e.delta < 0).reduce((s, e) => s + e.delta, 0),
      entries: entries.slice(0, limit), trend, daysUntilFull: this.daysUntilFull(trend),
    };
  }

  private deletedEntry(path: string, name: string, category: Category, bytes: number, points: number): DiffEntryDto {
    const spark = Array.from({ length: points }, (_, i) => (i === points - 1 ? 0 : bytes));
    return { id: null, path, name, kind: "dir", category, before: bytes, after: 0, delta: -bytes, sparkline: spark, status: "deleted" };
  }

  private daysUntilFull(trend: { takenAt: number; usedBytes: number }[]): number | null {
    if (trend.length < 2) return null;
    const n = trend.length;
    const mx = trend.reduce((s, p) => s + p.takenAt, 0) / n;
    const my = trend.reduce((s, p) => s + p.usedBytes, 0) / n;
    let num = 0, den = 0;
    for (const p of trend) { num += (p.takenAt - mx) * (p.usedBytes - my); den += (p.takenAt - mx) ** 2; }
    const perSec = den ? num / den : 0;
    if (perSec <= 0) return null;
    const total = this.t.world.volumes[0].total * GB;
    return (total - trend[n - 1].usedBytes) / perSec / DAY;
  }

  // ---- Cleanup ----

  async suggestions(): Promise<SuggestionDto[]> {
    if (!this.hasScan) return [];
    const t = this.t;
    const other = this.t.world.volumes[1]?.label ?? "another drive";
    const kept = new Set(this.settings.keptItems);
    const w = { trash: this.opts.platform === "windows" ? "Recycle Bin" : "Trash", temp: { windows: "Windows temp", macos: "macOS temp", linux: "/tmp" }[this.opts.platform] };
    const out: SuggestionDto[] = [];
    for (const s of SUGGESTIONS) {
      // Duplicates need file contents; with "Find duplicate files" off, none are read and none are offered.
      if (s.group === "duplicates" && !this.settings.findDuplicates) continue;
      const id = t.keys.get(s.key);
      const sid = `s-${s.key}`;
      if (id === undefined || t.flags[id] & FL_REMOVED || this.applied.has(sid)) continue;
      const path = pathOf(t, id);
      if (kept.has(path)) continue;
      const prov = PROVIDERS[t.provider[id]];
      const st = STATES[t.state[id]];
      if (s.group === "freeUpCloud" && (!prov || st === "cloudOnly" || (st === "pinned" && s.key !== "pinOD") || this.opts.platform === "linux")) continue;
      const orig = t.dupOf.get(id);
      out.push({
        id: sid, group: s.group, safety: s.safety, nodeId: id, path, name: t.name[id],
        bytes: t.allocated[id], reason: s.reason(other, w), howFound: s.howFound(w),
        lastOpened: t.accessed[id] || null,
        keepPath: orig !== undefined ? pathOf(t, orig) : null,
        cloud: prov && st ? { provider: prov, state: st } : null,
      });
    }
    return out;
  }

  private subtreeIds(id: number): number[] {
    const out: number[] = [];
    const stack = [id];
    while (stack.length) {
      const x = stack.pop()!;
      out.push(x);
      const kids = childrenOf(this.t, x);
      for (let i = 0; i < kids.length; i++) stack.push(kids[i]);
    }
    return out;
  }

  async applyCleanup(req: ApplyRequest): Promise<ApplyResult> {
    this.applying = true;
    try {
      if (this.opts.applyMs > 0) await new Promise((r) => setTimeout(r, this.opts.applyMs));
      return await this.applyNow(req);
    } finally {
      this.applying = false;
    }
  }

  private async applyNow(req: ApplyRequest): Promise<ApplyResult> {
    const t = this.t;
    const all = await this.suggestions();
    const byId = new Map(all.map((s) => [s.id, s]));
    const rec: UndoRecord = { entry: { id: `undo-${++this.seq}`, at: Math.floor(Date.now() / 1000), items: 0, bytes: 0, destination: req.destination, undoable: true }, files: [], removed: [], suggestionIds: [] };
    let freed = 0;
    const failed: ApplyResult["failed"] = [];
    const keep = new Set(this.settings.keptItems);
    for (const d of req.decisions) {
      const s = byId.get(d.suggestionId);
      if (!s) { failed.push({ path: d.suggestionId, message: "No longer suggested" }); continue; }
      if (d.decision === "keep") { keep.add(s.path); continue; }
      if (d.decision !== "remove") continue;
      const ids = this.subtreeIds(s.nodeId);
      if (s.group === "freeUpCloud") {
        for (const i of ids) {
          if (t.kind[i] === K_DIR) continue;
          rec.files.push([i, t.allocated[i], t.cloudOnly[i], t.pinned[i], t.state[i]]);
          freed += t.allocated[i];
          t.cloudOnly[i] = t.logical[i];
          t.allocated[i] = 0;
          t.pinned[i] = 0;
          t.state[i] = STATES.indexOf("cloudOnly");
        }
      } else if (req.destination.kind === "compress") {
        for (const i of ids) {
          if (t.kind[i] !== K_FILE) continue;
          rec.files.push([i, t.allocated[i], t.cloudOnly[i], t.pinned[i], t.state[i]]);
          const smaller = Math.ceil((t.allocated[i] * 0.6) / 4096) * 4096;
          freed += t.allocated[i] - smaller;
          t.allocated[i] = smaller;
        }
      } else {
        freed += t.allocated[s.nodeId];
        for (const i of ids) t.flags[i] |= FL_REMOVED;
        rec.removed.push(...ids);
      }
      rec.suggestionIds.push(s.id);
      this.applied.add(s.id);
      rec.entry.items++;
    }
    this.settings = { ...this.settings, keptItems: [...keep] };
    this.saveSettings();
    rec.entry.bytes = freed;
    if (rec.entry.items) {
      this.refresh();
      this.undoRecords.unshift(rec);
    }
    return { applied: rec.entry.items, freedBytes: freed, failed, undoId: rec.entry.id };
  }

  private refresh() {
    finalize(this.t);
    this.sortCache.clear();
  }

  async undoLog(): Promise<UndoEntry[]> {
    return this.undoRecords.map((r) => ({ ...r.entry }));
  }

  async undo(undoId: string): Promise<void> {
    const i = this.undoRecords.findIndex((r) => r.entry.id === undoId);
    if (i < 0) throw new Error("unknown undo entry");
    const r = this.undoRecords[i];
    if (!r.entry.undoable) throw new Error("this cleanup can't be undone");
    const t = this.t;
    for (const [id, a, c, p, s] of r.files) { t.allocated[id] = a; t.cloudOnly[id] = c; t.pinned[id] = p; t.state[id] = s; }
    for (const id of r.removed) t.flags[id] &= ~FL_REMOVED;
    r.suggestionIds.forEach((s) => this.applied.delete(s));
    this.undoRecords.splice(i, 1);
    if (r.files.length || r.removed.length) this.refresh();
  }

  private seedUndo() {
    const trash: Destination = { kind: "trash" };
    const other = this.t.world.volumes[1];
    this.undoRecords = [
      { entry: { id: "undo-past-1", at: Date.UTC(2026, 9, 21, 20, 30) / 1000, items: 41, bytes: 2.1 * GB, destination: trash, undoable: true }, files: [], removed: [], suggestionIds: [] },
      { entry: { id: "undo-past-2", at: Date.UTC(2026, 8, 30, 18, 0) / 1000, items: 1, bytes: 18.4 * GB, destination: { kind: "move", target: other?.mountPath ?? homePath(this.opts.platform) }, undoable: true }, files: [], removed: [], suggestionIds: [] },
    ];
  }

  // ---- Misc ----

  async reveal(id: NodeId): Promise<void> {
    this.check(id);
  }

  async openSetting(_action: string): Promise<void> {}

  async getSettings(): Promise<Settings> {
    if (this.opts.portable) this.settings = { ...this.settings, autoUpdate: false };
    return structuredClone(this.settings);
  }

  async setSettings(s: Settings): Promise<void> {
    this.settings = structuredClone(s);
    this.saveSettings();
  }

  async clearHistory(): Promise<void> {
    this.snaps = this.snaps.filter((s) => s.latest);
    this.undoRecords = [];
  }

  // ---- Updates (simulated; the real app uses the signed updater against GitHub Releases) ----

  private installed: string;
  private applying = false;
  private updateCbs2 = new Set<(s: UpdateStatus) => void>();
  private updateStatus: UpdateStatus = { state: "idle" };
  private installRequested = false;

  /** Stand-in for the native folder picker: always "picks" a fake Projects folder in the home folder. */
  async pickFolder(): Promise<string | null> {
    const sep = this.opts.platform === "windows" ? "\\" : "/";
    return homePath(this.opts.platform) + sep + "Projects";
  }

  /** Dev switch: how the next update check and download behave. */
  setUpdateSim(sim: UpdateSim) {
    this.opts.updateSim = sim;
  }

  /** Dev switch: Fast scan is declined or unavailable and falls back to Standard. */
  setFastFallback(on: boolean) {
    this.opts.fastFallback = on;
  }

  /** Dev switch: how long a simulated scan takes. */
  setScanMs(ms: number) {
    this.opts.scanMs = ms;
  }

  /** A scan or a cleanup apply is running: never restart now. */
  private busy(): boolean {
    return this.scan !== null || this.applying;
  }

  private setUpdate(s: UpdateStatus) {
    this.updateStatus = s;
    this.updateCbs2.forEach((cb) => cb(s));
  }

  private latestInfo(): UpdateInfo {
    const r = MOCK_RELEASES[MOCK_LATEST];
    return {
      version: MOCK_LATEST, currentVersion: this.installed, publishedAt: r.publishedAt, notes: r.notes,
      sizeBytes: this.opts.updateSim === "unknownSize" ? null : r.sizeBytes,
    };
  }

  async appVersion(): Promise<string> {
    return this.installed;
  }

  async currentReleaseNotes(): Promise<string> {
    return MOCK_RELEASES[this.installed]?.notes ?? "";
  }

  async buildInfo(): Promise<BuildInfo> {
    return { portable: this.opts.portable };
  }

  private noUpdater() {
    if (this.opts.portable) throw new Error("This portable build has no updater.");
  }

  async checkForUpdate(): Promise<UpdateStatus> {
    this.noUpdater();
    if (this.updateStatus.state === "downloading" || this.updateStatus.state === "ready") return this.updateStatus;
    this.setUpdate({ state: "checking" });
    await new Promise((r) => setTimeout(r, 900));
    const sim = this.opts.updateSim;
    if (sim === "offline") {
      this.setUpdate({ state: "error", kind: "offline", message: "Couldn't reach GitHub Releases." });
      return this.updateStatus;
    }
    const now = Math.floor(Date.now() / 1000);
    this.settings = { ...this.settings, lastUpdateCheck: now };
    this.saveSettings();
    // A check that only finds the skipped version reports "up to date".
    const newer = sim !== "upToDate" && this.installed !== MOCK_LATEST && this.settings.skippedVersion !== MOCK_LATEST;
    this.setUpdate(newer ? { state: "available", info: this.latestInfo() } : { state: "upToDate", checkedAt: now });
    return this.updateStatus;
  }

  private download(install: boolean): Promise<void> {
    const info = this.latestInfo();
    const size = MOCK_RELEASES[MOCK_LATEST].sizeBytes;
    const sim = this.opts.updateSim;
    this.installRequested = install;
    return new Promise((resolve) => {
      let got = 0;
      const t = setInterval(() => {
        got = Math.min(size, got + size / 14);
        if (sim === "download" && got > size * 0.55) {
          clearInterval(t);
          this.setUpdate({ state: "error", kind: "download", message: "The connection dropped during the download." });
          resolve();
          return;
        }
        this.setUpdate({ state: "downloading", info, downloaded: got, total: info.sizeBytes });
        if (got < size) return;
        clearInterval(t);
        // Checking the signature takes a moment; the status stays at 100% meanwhile.
        setTimeout(() => {
          if (sim === "verify") {
            this.setUpdate({ state: "error", kind: "verify", message: "The download didn't match Lustro's signature." });
          } else {
            this.setUpdate({ state: "ready", info });
            // Install and restart only when idle; otherwise wait for restartToUpdate.
            if (this.installRequested && !this.busy()) setTimeout(() => void this.restartToUpdate(), 1800);
          }
          resolve();
        }, 1100);
      }, 160);
    });
  }

  async installUpdate(): Promise<void> {
    this.noUpdater();
    if (this.updateStatus.state === "ready") {
      if (!this.busy()) await this.restartToUpdate();
      return;
    }
    await this.download(true);
  }

  /** Simulated restart: the browser build records the new version and reloads; tests just switch versions. */
  async restartToUpdate(): Promise<void> {
    this.noUpdater();
    if (this.updateStatus.state !== "ready" || this.busy()) return;
    this.installed = MOCK_LATEST;
    this.installRequested = false;
    if (this.opts.persist && typeof location !== "undefined") {
      try { localStorage.setItem(VERSION_KEY, MOCK_LATEST); } catch { /* storage unavailable */ }
      setTimeout(() => location.reload(), 300);
      return;
    }
    this.setUpdate({ state: "idle" });
  }

  /** Dev switch: go back to the first version, as if 0.3.0 was never installed. */
  resetVersion() {
    this.installed = MOCK_FIRST_VERSION;
    this.settings = { ...this.settings, notesShownForVersion: MOCK_FIRST_VERSION, skippedVersion: null };
    this.saveSettings();
    try { localStorage.removeItem(VERSION_KEY); } catch { /* storage unavailable */ }
    this.setUpdate({ state: "idle" });
  }

  async skipVersion(version: string): Promise<void> {
    this.settings = { ...this.settings, skippedVersion: version };
    this.saveSettings();
    this.setUpdate({ state: "idle" });
  }

  async getUpdateStatus(): Promise<UpdateStatus> {
    return this.updateStatus;
  }

  onUpdateStatus(cb: (s: UpdateStatus) => void): Unsubscribe {
    this.updateCbs2.add(cb);
    return () => this.updateCbs2.delete(cb);
  }

  private loadSettings(): Settings {
    const base = structuredClone(DEFAULT_SETTINGS);
    base.protectedPaths = protectedFor(this.opts.platform);
    base.excluded = excludedFor(this.opts.platform);
    // A fresh install has nothing new to announce.
    base.notesShownForVersion = this.installed;
    if (this.opts.portable) base.autoUpdate = false;
    if (!this.opts.persist) return base;
    try {
      const raw = localStorage.getItem(SETTINGS_KEY);
      if (raw) return { ...base, ...JSON.parse(raw), terrain: { ...base.terrain, ...JSON.parse(raw).terrain } };
    } catch { /* storage unavailable */ }
    return base;
  }

  private saveSettings() {
    if (!this.opts.persist) return;
    try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(this.settings)); } catch { /* storage unavailable */ }
  }
}

function protectedFor(p: Platform): string[] {
  return p === "windows" ? ["C:\\Windows", "C:\\Program Files", "C:\\pagefile.sys", "C:\\hiberfil.sys"]
    : p === "macos" ? ["/System", "/Applications", "/private/var/vm"]
    : ["/usr", "/boot", "/proc", "/sys", "/dev", "/run"];
}

function excludedFor(p: Platform): string[] {
  return p === "windows" ? ["C:\\Users\\William\\AppData\\Local\\Temp", "D:\\VM Images"]
    : p === "macos" ? ["/Users/william/Library/Caches", "/Volumes/Backup"]
    : ["/home/william/.cache", "/mnt/data/vm-images"];
}
