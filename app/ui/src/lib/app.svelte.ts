// App-wide state (Svelte 5 runes). Screens read from `app` and call `app.backend`.

import type {
  Category, CloudProviderSummary, Decision, MainView, UpdateStatus, NodeDto, NodeId, Platform, ScanDone, ScanProgress, ScanUpdate, Settings, TreeDto, VolumeInfo,
} from "../ipc/types";
import { DEFAULT_SETTINGS } from "../ipc/defaults";
import type { Backend } from "../ipc/backend";
import { createBackend, readDevOptions, type DevOptions } from "./backend";
import { fmtBytes } from "./format";
import { catVar, wording, kbd, type Wording } from "./labels";
import { size, type VizNode } from "./viz";

export type Route = "overview" | "explore" | "grew" | "cleanup" | "settings";
const ROUTES: Route[] = ["overview", "explore", "grew", "cleanup", "settings"];

export type Tip =
  | { kind: "node"; node: VizNode; path?: string; x: number; y: number; extra?: string }
  | { kind: "text"; title: string; lines: string[]; x: number; y: number };

/** Resolved colors for canvas drawing, read from the CSS tokens of the current theme. */
export interface Palette {
  cat: Record<Category | "cloud", string>;
  text: string; text2: string; text3: string;
  surface1: string; surface2: string; surface3: string; stroke: string; strokeStrong: string;
  accent: string; accentInk: string; grew: string; shrank: string; coral: string; lime: string; amber: string; bg: string;
}

const CATS: Category[] = ["video", "images", "audio", "documents", "archives", "installers", "code", "apps", "system", "other"];

function readPalette(): Palette {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string) => cs.getPropertyValue(n).trim();
  const cat = {} as Palette["cat"];
  for (const c of [...CATS, "cloud" as const]) cat[c] = v(catVar(c));
  return {
    cat, text: v("--text"), text2: v("--text-2"), text3: v("--text-3"),
    surface1: v("--surface-1"), surface2: v("--surface-2"), surface3: v("--surface-3"),
    stroke: v("--stroke"), strokeStrong: v("--stroke-strong"), accent: v("--accent"), accentInk: v("--accent-ink"),
    grew: v("--grew") || v("--coral"), shrank: v("--shrank") || v("--lime"), coral: v("--coral"), lime: v("--lime"), amber: v("--amber"), bg: v("--bg"),
  };
}

// ---- Live scan mirror ----

/** A node as the running scan reported it, plus when it first appeared and was last updated (update numbers). */
export type LiveNode = NodeDto & { depth: number; born: number; seen: number };

/** What Explore's search can filter the live mirror by. */
export interface LiveFilter {
  text?: string;
  categories?: string[];
  minBytes?: number;
  olderThanDays?: number;
}

/** Runs `fn` now, then again every `minMs` or slower: the pause grows with how long `fn` took, so redrawing
 *  from a big scan mirror stays under about 1/8 of the main thread. Returns a stop function. */
export function liveLoop(fn: () => void, minMs: number): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  const run = () => {
    const t0 = performance.now();
    fn();
    if (!stopped) timer = setTimeout(run, Math.max(minMs, (performance.now() - t0) * 8));
  };
  timer = setTimeout(run, minMs);
  return () => { stopped = true; clearTimeout(timer); };
}

/** Client-side mirror of a scan, built from ScanUpdates. Overview and Explore draw from it while the scan runs,
 *  and after a stopped scan (the partial tree stays until the next scan). Big maps are deliberately not reactive;
 *  `tick` is bumped once per update (about 10 times a second), and views throttle their own redraws. */
export class LiveScan {
  id: string;
  root: string;
  mode: Settings["scanMode"];
  nodes = new Map<NodeId, LiveNode>();
  kids = new Map<NodeId, NodeId[]>();
  open = new Set<NodeId>();
  rootId: NodeId | null = null;
  /** Biggest file seen so far (the terrain's only pin while scanning). */
  largestFile: NodeId | null = null;
  /** Names near the top of the tree, so Overview can show system rows (pagefile.sys…) once they're reached. */
  topNames = new Set<string>();
  tick = $state(0);
  progress = $state<ScanProgress>({ files: 0, dirs: 0, allocatedBytes: 0, elapsedMs: 0 });
  busiest = $state<NodeId | null>(null);
  done = $state<ScanDone | null>(null);
  private dirty = new Set<NodeId>();
  private lastSort = 0;

  constructor(id: string, root: string, mode: Settings["scanMode"]) {
    this.id = id;
    this.root = root;
    this.mode = mode;
  }

  get running(): boolean {
    return !this.done;
  }

  /** Stopped before the end: what's on screen is partial. */
  get partial(): boolean {
    return !!this.done?.cancelled;
  }

  apply(u: ScanUpdate) {
    const n = this.tick + 1;
    for (const d of u.nodes) {
      let x = this.nodes.get(d.id);
      if (!x) {
        const parent = d.parent == null ? null : this.nodes.get(d.parent);
        x = { ...d, childCount: 0, depth: parent ? parent.depth + 1 : 0, born: n, seen: n };
        this.nodes.set(d.id, x);
        if (d.parent == null) this.rootId = d.id;
        else {
          const list = this.kids.get(d.parent);
          if (list) list.push(d.id);
          else this.kids.set(d.parent, [d.id]);
          if (parent) parent.childCount = this.kids.get(d.parent)!.length;
        }
        if (x.depth <= 2) this.topNames.add(d.name);
        // Open the root and its children automatically; deeper folders open on click.
        if (d.kind === "dir" && x.depth <= 1) this.open.add(d.id);
      } else {
        const childCount = x.childCount;
        Object.assign(x, d);
        x.childCount = childCount;
        x.seen = n;
      }
      if (x.kind !== "dir" && (this.largestFile == null || x.allocated > (this.nodes.get(this.largestFile)?.allocated ?? 0))) this.largestFile = x.id;
      if (d.parent != null) this.dirty.add(d.parent);
    }
    this.progress = u.progress;
    this.busiest = u.busiest;
    this.tick = n;
  }

  /** Children sorted by size. With `resort` false, lists keep their last order (new children are appended at
   *  the end), so a growing tree doesn't jump around on every update; see `resortDue`. */
  children(id: NodeId, resort = true): NodeId[] {
    const list = this.kids.get(id);
    if (!list) return [];
    if (resort && this.dirty.has(id)) {
      list.sort((a, b) => size(this.nodes.get(b)!) - size(this.nodes.get(a)!));
      this.dirty.delete(id);
    }
    return list;
  }

  /** True at most once every `every` ms: the caller may re-sort now. */
  resortDue(every: number): boolean {
    if (Date.now() - this.lastSort < every) return false;
    this.lastSort = Date.now();
    return true;
  }

  /** A folded subtree like `Backend.subtree`, built from what has been found so far. Doesn't reorder `kids`. */
  tree(id: NodeId, maxDepth: number, minFraction: number): TreeDto | null {
    const top = this.nodes.get(id);
    if (!top) return null;
    const threshold = size(top) * minFraction;
    const build = (x: LiveNode, depth: number): TreeDto => {
      const kids = this.kids.get(x.id);
      const node: TreeDto = { ...x, childCount: kids?.length ?? 0, children: [] };
      if (depth >= maxDepth || !kids) return node;
      const big: LiveNode[] = [];
      let folded = 0, fAlloc = 0, fLogical = 0, fCloud = 0, fFiles = 0;
      for (const k of kids) {
        const c = this.nodes.get(k)!;
        if (size(c) >= threshold && size(c) > 0) big.push(c);
        else {
          folded++;
          fAlloc += c.allocated; fLogical += c.logical; fCloud += c.cloudOnly;
          fFiles += c.kind === "dir" ? c.fileCount : 1;
        }
      }
      big.sort((a, b) => size(b) - size(a));
      for (const c of big) node.children.push(build(c, depth + 1));
      if (folded && fAlloc + fCloud > 0) {
        node.children.push({
          id: -(x.id + 1), parent: x.id, name: `${folded.toLocaleString("en-US")} smaller item${folded === 1 ? "" : "s"}`, kind: "other",
          allocated: fAlloc, logical: fLogical, cloudOnly: fCloud, fileCount: fFiles, dirCount: 0, category: x.category,
          modified: null, accessed: null, cloud: null, delta: null, hidden: false, accessDenied: false, notDescended: null,
          childCount: folded, children: [],
        });
      }
      return node;
    };
    return build(top, 0);
  }

  /** Root → node, as found so far (empty if the node hasn't been reported). */
  pathTo(id: NodeId): LiveNode[] {
    const out: LiveNode[] = [];
    for (let x = this.nodes.get(id); x; x = x.parent == null ? undefined : this.nodes.get(x.parent)) out.unshift(x);
    return out;
  }

  /** Full path text, e.g. "C:\Users\William\Videos". */
  pathText(id: NodeId, sep: string): string {
    const names = this.pathTo(id).map((n) => n.name);
    if (!names.length) return "";
    const tail = names.slice(1).join(sep);
    return !tail ? names[0] : names[0].endsWith(sep) ? names[0] + tail : names[0] + sep + tail;
  }

  /** The branch being read right now (busiest folder and its ancestors). */
  hot(): Set<NodeId> {
    const set = new Set<NodeId>();
    if (!this.running) return set;
    for (let x = this.busiest; x != null; x = this.nodes.get(x)?.parent ?? null) set.add(x);
    return set;
  }

  /** Still being measured: on the busiest branch, or its size changed in the last half second. */
  measuring(id: NodeId): boolean {
    const x = this.nodes.get(id);
    if (!this.running || !x || x.kind !== "dir") return false;
    return this.tick - x.seen < 5 || this.hot().has(id);
  }

  /** Bytes found so far per category. Each node counts what its reported children don't explain (files the
   *  scanner only sends as folder totals), under its own category, so the parts add up to the root's total. */
  byCategory(): Map<Category, number> {
    const m = new Map<Category, number>();
    for (const x of this.nodes.values()) {
      let own = x.allocated;
      const kids = this.kids.get(x.id);
      if (kids) for (const k of kids) own -= this.nodes.get(k)!.allocated;
      if (own > 0) m.set(x.category, (m.get(x.category) ?? 0) + own);
    }
    return m;
  }

  /** Search what's been found so far (largest first). */
  search(f: LiveFilter, limit: number): LiveNode[] {
    const text = f.text?.trim().toLowerCase() ?? "";
    const cats = f.categories?.length ? new Set<string>(f.categories) : null;
    const cutoff = f.olderThanDays ? Date.now() / 1000 - f.olderThanDays * 86400 : null;
    const out: LiveNode[] = [];
    for (const x of this.nodes.values()) {
      if (x.id === this.rootId) continue;
      if (text && !x.name.toLowerCase().includes(text)) continue;
      if (cats && !cats.has(x.category)) continue;
      if (f.minBytes && size(x) < f.minBytes) continue;
      if (cutoff != null && (x.modified == null || x.modified > cutoff)) continue;
      out.push(x);
    }
    return out.sort((a, b) => size(b) - size(a)).slice(0, limit);
  }
}

class AppState {
  backend!: Backend;
  dev = $state<DevOptions>({ platform: "windows", nodes: 60_000, fresh: false, scanMs: 8000, updateSim: "ok", portable: false });
  /** Portable build: no updater at all (no checks, toast, dialog or gear dot). */
  portable = $state(false);
  ready = $state(false);
  route = $state<Route>("overview");
  platform = $state<Platform>("windows");
  settings = $state<Settings>(structuredClone(DEFAULT_SETTINGS));
  theme = $state<"dark" | "light">("dark");
  palette = $state<Palette | null>(null);
  volumes = $state<VolumeInfo[]>([]);
  volumeId = $state("");
  root = $state<NodeDto | null>(null);
  lastScanAt = $state<number | null>(null);
  /** Bumped when the data changes (scan finished, cleanup applied or undone). */
  version = $state(0);
  /** Selection shared between Overview (terrain clicks) and Explore. */
  selected = $state<NodeId | null>(null);
  /** View picked in Explore for this session (Settings holds the default). */
  exploreView = $state<MainView | null>(null);
  scan = $state<LiveScan | null>(null);
  tip = $state<Tip | null>(null);
  toast = $state<string | null>(null);
  binary = $state(false);
  cloud = $state<CloudProviderSummary[]>([]);
  appVersion = $state("");
  update = $state<UpdateStatus>({ state: "idle" });
  /** "Restart to update" toast dismissed for this session ("Later"). */
  updateToastDismissed = $state(false);
  /** The update dialog is open. */
  updateDialog = $state(false);
  /** "Download and install" was pressed in the dialog: show its progress there. */
  installing = $state(false);
  /** Release-notes dialog: title, notes and whether to offer "Restart to update". */
  notesDialog = $state<{ title: string; notes: string; restart: boolean } | null>(null);
  /** "What's new" card on Overview, once after an update. */
  whatsNew = $state<{ version: string; notes: string } | null>(null);
  /** A cleanup is being applied. */
  applying = $state(false);
  /** Fast scan explainer before the Windows permission prompt. */
  fastConfirm = $state<{ root: string } | null>(null);
  /** Cleanup review decisions not applied yet. Kept in localStorage so they survive a restart (and tab switches). */
  cleanupDecisions = $state.raw(new Map<string, Decision>());
  cleanupCursor = $state(0);
  private toastTimer: ReturnType<typeof setTimeout> | undefined;

  get w(): Wording {
    return wording(this.platform);
  }

  kbd(key: string): string {
    return kbd(this.platform, key);
  }

  /** Format bytes with the user's unit preference. */
  fb = (bytes: number, sign = false): string => fmtBytes(bytes, { sign, binary: this.binary });

  get volume(): VolumeInfo | undefined {
    return this.volumes.find((v) => v.id === this.volumeId);
  }

  /** A scan is running. */
  get scanning(): boolean {
    return !!this.scan && !this.scan.done;
  }

  /** The tree Overview and Explore draw from the scan mirror instead of the backend: while a scan runs, and
   *  after a stopped one (a stopped scan isn't saved; its partial tree stays until the next scan or a restart). */
  get live(): LiveScan | null {
    const s = this.scan;
    return s && (!s.done || s.done.cancelled) ? s : null;
  }

  /** Scan progress, 0–1: bytes found over used space, capped at 99% until the scan is done. */
  get scanPct(): number {
    const s = this.scan;
    if (!s) return 0;
    if (s.done && !s.done.cancelled) return 1;
    return Math.min(0.99, s.progress.allocatedBytes / Math.max(1, this.usedBytes));
  }

  get usedBytes(): number {
    const v = this.volume;
    return v ? v.totalBytes - v.freeBytes : 0;
  }

  get mainView(): MainView {
    return this.exploreView ?? this.settings.mainView;
  }

  async init() {
    this.dev = readDevOptions();
    try {
      this.binary = localStorage.getItem("lustro-binary") === "1";
    } catch { /* storage unavailable */ }
    this.routeFromHash();
    addEventListener("hashchange", () => this.routeFromHash());
    // Let the loading screen paint before a big fake tree is generated.
    await new Promise((r) => setTimeout(r, 30));
    this.backend = await createBackend(this.dev);
    this.platform = await this.backend.platform();
    this.settings = await this.backend.getSettings();
    this.applyTheme();
    matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => this.applyTheme());
    this.volumes = await this.backend.listVolumes();
    this.volumeId = this.volumes[0]?.id ?? "";
    await this.reload();
    const snaps = await this.backend.listSnapshots(this.root?.name ?? "");
    this.lastScanAt = snaps.length ? snaps[snaps.length - 1].takenAt : null;
    this.appVersion = await this.backend.appVersion();
    this.portable = (await this.backend.buildInfo()).portable;
    this.loadCleanupDecisions();
    this.backend.onUpdateStatus(async (st) => {
      this.update = st;
      // The backend records the time of every successful check.
      if (st.state === "upToDate" || st.state === "available") this.settings = await this.backend.getSettings();
    });
    // A status emitted before we subscribed (e.g. auto-update finished during startup) would be lost: ask once.
    if (!this.portable) {
      const current = await this.backend.getUpdateStatus();
      if (this.update.state === "idle" && current.state !== "idle") this.update = current;
    }
    // What's new: once, on the first launch after an update. Marked as seen as soon as it is shown.
    if (this.settings.notesShownForVersion !== this.appVersion) {
      this.whatsNew = { version: this.appVersion, notes: await this.backend.currentReleaseNotes() };
      await this.saveSettings({ notesShownForVersion: this.appVersion });
    }
    this.backend.onScanUpdate((u) => {
      if (this.scan && u.scanId === this.scan.id) this.scan.apply(u);
    });
    this.backend.onScanDone(async (d) => {
      if (!this.scan || d.scanId !== this.scan.id) return;
      this.scan.done = d;
      if (!d.cancelled) {
        this.lastScanAt = Math.floor(Date.now() / 1000);
        await this.reload();
        this.version++;
      }
    });
    this.ready = true;
  }

  async reload() {
    this.root = await this.backend.root();
    this.volumes = await this.backend.listVolumes();
    this.cloud = await this.backend.cloudSummary();
  }

  /** Data changed (cleanup, undo): refresh shared state and tell screens to refetch. */
  async changed() {
    await this.reload();
    this.version++;
  }

  private routeFromHash() {
    const h = location.hash.replace(/^#\/?/, "") as Route;
    // The Scan screen is gone: the scan shows in place on Overview and Explore. Old links land on Overview.
    if ((h as string) === "scan") history.replaceState(null, "", "#/overview");
    this.route = ROUTES.includes(h) ? h : "overview";
  }

  go(r: Route, selected?: NodeId | null) {
    if (selected !== undefined) this.selected = selected;
    if (location.hash !== `#/${r}`) location.hash = `#/${r}`;
    this.route = r;
  }

  applyTheme() {
    const t = this.settings.theme;
    this.theme = t === "system" ? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark") : t;
    document.documentElement.dataset.theme = this.theme;
    this.palette = readPalette();
  }

  async saveSettings(patch: Partial<Settings>) {
    this.settings = { ...this.settings, ...patch };
    await this.backend.setSettings($state.snapshot(this.settings));
    if ("theme" in patch) this.applyTheme();
  }

  /** Drive chips. Only the current drive is scanned in this version; others explain why. */
  pickVolume(id: string) {
    const target = this.volumes.find((x) => x.id === id);
    if (!target || id === this.volumeId) return;
    if (target.fileSystem === "Virtual") this.say(`${target.label} is a cloud drive: listed, never scanned for content.`);
    else this.say(`${target.label} hasn't been scanned in this preview.`);
  }

  /** "Scan a folder…": pick a folder natively, then scan it. */
  async scanFolder() {
    try {
      const path = await this.backend.pickFolder();
      if (path) await this.startScan(path);
    } catch (e) {
      this.say(e instanceof Error ? e.message : "Couldn't open the folder picker");
    }
  }

  setBinary(on: boolean) {
    this.binary = on;
    try { localStorage.setItem("lustro-binary", on ? "1" : "0"); } catch { /* storage unavailable */ }
  }

  /** Scan the current drive, or `folder` (a path from the folder picker). On Windows, Fast scan starts an
   *  elevated helper that shows a UAC prompt, so the user sees an explanation first (`fastConfirm`). */
  async startScan(folder?: string, mode?: Settings["scanMode"]) {
    // Starting a scan never changes the screen; the context bar shows it.
    if (this.scanning) return;
    const root = folder ?? this.volume?.mountPath ?? "/";
    const chosen = mode ?? this.settings.scanMode;
    if (!mode && chosen === "fast" && this.platform === "windows") {
      this.fastConfirm = { root };
      return;
    }
    this.fastConfirm = null;
    const id = await this.backend.startScan({
      root, mode: chosen, crossFilesystems: false, includeRemote: false, excluded: $state.snapshot(this.settings.excluded),
    });
    // Node ids are per scan (arena indices): an id from the previous tree means something else in the new one.
    this.selected = null;
    this.scan = new LiveScan(id, root, chosen);
  }

  /** A scan or a cleanup apply is running: updates wait, and the ready toast is held. */
  get busy(): boolean {
    return this.scanning || this.applying;
  }

  /** The gear dot: an update is available, or ready but held back (busy, or "Later"). */
  get updatePending(): boolean {
    if (this.portable) return false;
    return this.update.state === "available" || (this.update.state === "ready" && (this.busy || this.updateToastDismissed));
  }

  /** Manual "Check for updates": if a new version is found, the dialog opens straight away. */
  async checkForUpdates() {
    if (this.portable) return;
    const st = await this.backend.checkForUpdate();
    if (st.state === "available") {
      this.installing = false;
      this.updateDialog = true;
    }
  }

  async installUpdate() {
    this.installing = true;
    await this.backend.installUpdate();
  }

  async restartToUpdate() {
    if (this.busy) return;
    this.notesDialog = null;
    await this.backend.restartToUpdate();
  }

  setCleanupDecisions(m: Map<string, Decision>) {
    this.cleanupDecisions = m;
    try { localStorage.setItem("lustro-cleanup", JSON.stringify([...m])); } catch { /* storage unavailable */ }
  }

  /** Settings → "Clear history": the backend deletes everything with file names (snapshots, undo log, saved
   *  suggestions, Keep decisions); the review decisions kept here go too. Screens refetch through `changed()`. */
  async clearHistory() {
    await this.backend.clearHistory();
    this.setCleanupDecisions(new Map());
    this.cleanupCursor = 0;
    this.settings = await this.backend.getSettings();
    await this.changed();
  }

  private loadCleanupDecisions() {
    try {
      const raw = localStorage.getItem("lustro-cleanup");
      if (raw) this.cleanupDecisions = new Map(JSON.parse(raw) as [string, Decision][]);
    } catch { /* storage unavailable */ }
  }

  say(msg: string, ms = 2600) {
    this.toast = msg;
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = null), ms);
  }

  /** Refresh after a cleanup or undo. Returns false when the backend hasn't updated the tree yet
   *  (the drive total didn't move although bytes were freed), so callers can say sizes update after the next scan. */
  async changedAfterCleanup(freedBytes: number): Promise<boolean> {
    const before = this.root?.allocated ?? 0;
    await this.changed();
    const after = this.root?.allocated ?? 0;
    return freedBytes <= 0 || Math.abs(after - before) > 0;
  }

  showTip(t: Tip | null) {
    this.tip = t;
  }
}

export const app = new AppState();

// Dev builds: expose the state for debugging in the browser console.
if (import.meta.env.DEV) (globalThis as unknown as { __sv: AppState }).__sv = app;
