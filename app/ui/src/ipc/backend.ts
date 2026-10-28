// CONTRACT FILE: the backend interface the UI codes against.
// Implemented by `mock.ts` (browser, fake data) and by `tauri.ts` (the desktop app),
// each method mapping 1:1 to a Tauri command of the same name in snake_case.

import type {
  ApplyRequest, ApplyResult, BuildInfo, CloudProviderSummary, DiffDto, NodeDetailDto, NodeDto, NodeId, Platform, ScanDone,
  ScanRequest, ScanUpdate, Settings, SnapshotMeta, SpaceBreakdownDto, SuggestionDto, TreeDto,
  UndoEntry, UpdateStatus, VolumeInfo,
} from "./types";

export type SortKey = "allocated" | "logical" | "name" | "modified" | "fileCount" | "delta";

export interface Filter {
  text?: string;
  categories?: string[];
  minBytes?: number;
  maxBytes?: number;
  /** Only items not modified in this many days. */
  olderThanDays?: number;
  cloud?: "any" | "local" | "cloudOnly";
}

export type Unsubscribe = () => void;

export interface Backend {
  platform(): Promise<Platform>;
  listVolumes(): Promise<VolumeInfo[]>;

  startScan(req: ScanRequest): Promise<string>;
  cancelScan(scanId: string): Promise<void>;
  onScanUpdate(cb: (u: ScanUpdate) => void): Unsubscribe;
  onScanDone(cb: (d: ScanDone) => void): Unsubscribe;

  /** Root node of the current scan, or null before any scan. Its `name` is the scanned path (e.g. "C:\\"). */
  root(): Promise<NodeDto | null>;
  children(id: NodeId, sort: SortKey, offset: number, limit: number): Promise<NodeDto[]>;
  node(id: NodeId): Promise<NodeDetailDto>;
  /** Ancestors from root to `id` inclusive (breadcrumbs, dashed route). */
  pathTo(id: NodeId): Promise<NodeDto[]>;
  subtree(id: NodeId, maxDepth: number, minFraction: number): Promise<TreeDto>;
  top(kind: "files" | "dirs", n: number, filter: Filter): Promise<NodeDto[]>;
  search(filter: Filter, limit: number): Promise<NodeDto[]>;
  spaceBreakdown(volumeId: string): Promise<SpaceBreakdownDto>;
  cloudSummary(): Promise<CloudProviderSummary[]>;

  /** Oldest first. `root` is the scanned path, the same string as the root node's `name`. */
  listSnapshots(root: string): Promise<SnapshotMeta[]>;
  diff(fromId: string, toId: string, limit: number): Promise<DiffDto>;

  suggestions(): Promise<SuggestionDto[]>;
  applyCleanup(req: ApplyRequest): Promise<ApplyResult>;
  undoLog(): Promise<UndoEntry[]>;
  undo(undoId: string): Promise<void>;

  reveal(id: NodeId): Promise<void>;
  openSetting(action: string): Promise<void>;
  getSettings(): Promise<Settings>;
  setSettings(s: Settings): Promise<void>;
  clearHistory(): Promise<void>;

  buildInfo(): Promise<BuildInfo>;

  // Updates (tauri-plugin-updater against GitHub Releases; every update is signature-verified).
  appVersion(): Promise<string>;
  /** One click "Check for updates". Resolves with the new status (upToDate / available / error). */
  checkForUpdate(): Promise<UpdateStatus>;
  /** One click "Download and install": downloads, verifies, installs, then restarts the app.
   *  If a scan or a cleanup apply is running, it stops after verifying, emits `ready`, and never restarts
   *  until the user calls `restartToUpdate` once the app is idle. */
  installUpdate(): Promise<void>;
  /** Release notes of the installed version (for Settings' "Release notes" and the What's new card). */
  currentReleaseNotes(): Promise<string>;
  /** Restart now to apply an update that auto-update already downloaded ("ready"). */
  restartToUpdate(): Promise<void>;
  skipVersion(version: string): Promise<void>;
  onUpdateStatus(cb: (s: UpdateStatus) => void): Unsubscribe;
  /** Current update status, so a subscriber never misses a state emitted before it subscribed. */
  getUpdateStatus(): Promise<UpdateStatus>;

  /** Native folder picker. Returns null if cancelled. The backend records the returned path as
   *  user-picked; only scan results and user-picked paths are accepted by other commands. */
  pickFolder(): Promise<string | null>;
}
