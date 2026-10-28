// App-wide state (Svelte 5 runes). Screens read from `app` and call `app.backend`.

import type {
  Category, CloudInfo, CloudProviderSummary, Decision, MainView, UpdateStatus, NodeDto, NodeId, Platform, ScanDone, ScanProgress, ScanUpdate, Settings, VolumeInfo,
} from "../ipc/types";
import { DEFAULT_SETTINGS } from "../ipc/defaults";
import type { Backend } from "../ipc/backend";
import { createBackend, readDevOptions, type DevOptions } from "./backend";
import { fmtBytes } from "./format";
import { catVar, wording, kbd, type Wording } from "./labels";
import type { VizNode } from "./viz";

export type Route = "overview" | "explore" | "grew" | "cleanup" | "settings" | "scan";
const ROUTES: Route[] = ["overview", "explore", "grew", "cleanup", "settings", "scan"];

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

export interface LiveNode {
  id: NodeId;
  parent: NodeId | null;
  name: string;
  kind: NodeDto["kind"];
  allocated: number;
  cloudOnly: number;
  category: Category;
  cloud: CloudInfo | null;
  depth: number;
  /** Update number when it first appeared (for the fade-in). */
  born: number;
}

/** Client-side mirror of a running scan, built from ScanUpdates. Big maps are deliberately not reactive;
 *  `tick` is bumped once per update so views re-render about 10 times a second. */
export class LiveScan {
  id: string;
  root: string;
  mode: Settings["scanMode"];
  nodes = new Map<NodeId, LiveNode>();
  kids = new Map<NodeId, NodeId[]>();
  open = new Set<NodeId>();
  rootId: NodeId | null = null;
  tick = $state(0);
  progress = $state<ScanProgress>({ files: 0, dirs: 0, allocatedBytes: 0, elapsedMs: 0 });
  busiest = $state<NodeId | null>(null);
  done = $state<ScanDone | null>(null);
  private dirty = new Set<NodeId>();

  constructor(id: string, root: string, mode: Settings["scanMode"]) {
    this.id = id;
    this.root = root;
    this.mode = mode;
  }

  apply(u: ScanUpdate) {
    const n = this.tick + 1;
    for (const d of u.nodes) {
      let x = this.nodes.get(d.id);
      if (!x) {
        const parent = d.parent == null ? null : this.nodes.get(d.parent);
        x = {
          id: d.id, parent: d.parent, name: d.name, kind: d.kind, allocated: 0, cloudOnly: 0,
          category: d.category, cloud: d.cloud, depth: parent ? parent.depth + 1 : 0, born: n,
        };
        this.nodes.set(d.id, x);
        if (d.parent == null) this.rootId = d.id;
        if (d.parent != null) {
          const list = this.kids.get(d.parent);
          if (list) list.push(d.id);
          else this.kids.set(d.parent, [d.id]);
          this.dirty.add(d.parent);
        }
        // Open the first levels automatically, like the mockup.
        if (d.kind === "dir" && x.depth <= 2) this.open.add(d.id);
      }
      x.allocated = d.allocated;
      x.cloudOnly = d.cloudOnly;
      x.category = d.category;
      if (d.parent != null) this.dirty.add(d.parent);
    }
    this.progress = u.progress;
    this.busiest = u.busiest;
    this.tick = n;
  }

  /** Children sorted by size (sorted lazily, only for folders that changed). */
  children(id: NodeId): NodeId[] {
    const list = this.kids.get(id);
    if (!list) return [];
    if (this.dirty.has(id)) {
      list.sort((a, b) => {
        const x = this.nodes.get(a)!, y = this.nodes.get(b)!;
        return y.allocated + y.cloudOnly - (x.allocated + x.cloudOnly);
      });
      this.dirty.delete(id);
    }
    return list;
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

  /** Scan the current drive, or `folder` (a path from the folder picker). */
  /** Scan the current drive, or `folder` (a path from the folder picker). On Windows, Fast scan starts an
   *  elevated helper that shows a UAC prompt, so the user sees an explanation first (`fastConfirm`). */
  async startScan(folder?: string, mode?: Settings["scanMode"]) {
    if (this.scan && !this.scan.done) {
      this.go("scan");
      return;
    }
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
    this.scan = new LiveScan(id, root, chosen);
    this.go("scan");
  }

  /** A scan or a cleanup apply is running: updates wait, and the ready toast is held. */
  get busy(): boolean {
    return (!!this.scan && !this.scan.done) || this.applying;
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
