// CONTRACT FILE: mirrors the Rust DTOs (lustro-core `dto` module, camelCase JSON).
// Keep in sync with the Rust DTOs. See docs/CONTRACTS.md.

export type NodeId = number;
export type Platform = "windows" | "macos" | "linux";

export type NodeKind = "file" | "dir" | "link" | "other";

export type Category =
  | "video" | "images" | "audio" | "documents" | "archives"
  | "installers" | "code" | "apps" | "system" | "other";

export type CloudProvider = "oneDrive" | "googleDrive" | "iCloud" | "dropbox" | "box" | "other";
export type CloudState = "cloudOnly" | "downloaded" | "pinned";
export interface CloudInfo { provider: CloudProvider; state: CloudState }

export type SkipReason = "excluded" | "remote" | "cloudOnlyFolder" | "otherFilesystem" | "pseudoFilesystem";

/** One node as the UI sees it. Sizes are bytes. Times are Unix seconds. */
export interface NodeDto {
  id: NodeId;
  parent: NodeId | null;
  name: string;
  kind: NodeKind;
  /** Bytes on this disk, hard links counted once. Directories: sum of descendants. */
  allocated: number;
  /** Logical size (what the file "says"). Directories: sum of descendants. */
  logical: number;
  /** Logical bytes of cloud-only descendants (0 bytes on disk). */
  cloudOnly: number;
  fileCount: number;
  dirCount: number;
  /** Dominant category by allocated bytes (files: own category). */
  category: Category;
  modified: number | null;
  accessed: number | null;
  cloud: CloudInfo | null;
  /** Change since the comparison snapshot, bytes (null if no snapshot). */
  delta: number | null;
  hidden: boolean;
  accessDenied: boolean;
  notDescended: SkipReason | null;
  /** Number of direct children (for lazy tree expansion). */
  childCount: number;
}

export interface NodeDetailDto extends NodeDto {
  /** Display path. Never logged or sent anywhere. */
  path: string;
  categoryMix: { category: Category; allocated: number }[];
  largestChildren: NodeDto[];
  /** Share of parent's allocated bytes, 0..1. */
  shareOfParent: number;
}

/** Nested subtree for charts. Children below `minFraction` of the root are folded into one "other" node per parent.
 *  Fold nodes have `kind: "other"`, `childCount` = number of folded items, and a NEGATIVE id: `-(parentId + 1)`,
 *  so they are unique and the parent is recoverable. Fold nodes cannot be selected or queried. */
export interface TreeDto extends NodeDto { children: TreeDto[] }

/** Per-provider rollup for Overview's "Cloud storage" card and Settings' provider list. */
export interface CloudProviderSummary {
  provider: CloudProvider;
  /** As the client names itself, e.g. "OneDrive - Personal", "Google Drive". */
  displayName: string;
  /** Sync root (display only). For virtual drives, the drive path, e.g. "G:\\". */
  rootPath: string;
  /** Downloaded (not pinned) bytes on this disk. */
  localBytes: number;
  /** Pinned ("always keep on this device") bytes on this disk. */
  pinnedBytes: number;
  /** Logical bytes of cloud-only files (0 bytes on disk). */
  cloudOnlyBytes: number;
  /** A remote/FUSE/virtual drive that is not scanned by default (Linux rclone mounts, Google Drive Stream). */
  remote: boolean;
  /** Virtual drive volume id, e.g. "G:", if the provider uses one. */
  virtualVolumeId: string | null;
  /** Local cache folder that holds the provider's on-disk bytes (Google Drive Stream), display only. */
  cacheRoot: string | null;
}

export interface VolumeInfo {
  id: string;
  /** "C:" / "Macintosh HD" / "/" */
  label: string;
  mountPath: string;
  fileSystem: string;
  totalBytes: number;
  freeBytes: number;
  isRemote: boolean;
  isRemovable: boolean;
}

export type ScanMode = "standard" | "fast";

export interface ScanRequest {
  root: string;
  mode: ScanMode;
  crossFilesystems: boolean;
  includeRemote: boolean;
  excluded: string[];
}

export interface ScanProgress { files: number; dirs: number; allocatedBytes: number; elapsedMs: number }

/** Live scan update, sent ~10x/second. `nodes` are new or changed nodes since the last update. */
export interface ScanUpdate {
  scanId: string;
  progress: ScanProgress;
  nodes: NodeDto[];
  /** Id of the branch currently growing fastest (for highlighting), if known. */
  busiest: NodeId | null;
}

export interface ScanDone {
  scanId: string;
  modeUsed: ScanMode;
  progress: ScanProgress;
  cancelled: boolean;
  warnings: { path: string; message: string }[];
  snapshotId: string | null;
}

/** "Where did my space go?" One stacked bar that adds up to the volume's used space. */
export interface SpaceBreakdownDto {
  volumeId: string;
  usedBytes: number;
  segments: SpaceSegment[];
}
export interface SpaceSegment {
  kind: "category" | "system" | "cloudOnly" | "unexplained";
  /** Category name, or a system item id like "pagefile", "hibernation", "restorePoints", "winsxs",
   *  "systemData", "purgeable", "apfsSnapshots", "swap", "packageCache", "journal", "snapFlatpak", "docker". */
  key: string;
  label: string;
  bytes: number;
  explanation: string;
  /** Opens the relevant OS setting, if any (handled by the backend, never a web URL). */
  settingAction: string | null;
}

export interface SnapshotMeta { id: string; root: string; takenAt: number; allocatedBytes: number; fileCount: number }

export interface DiffEntryDto {
  id: NodeId | null;
  path: string;
  name: string;
  kind: NodeKind;
  category: Category;
  before: number;
  after: number;
  delta: number;
  /** Allocated bytes at each snapshot in the trend window, oldest first. */
  sparkline: number[];
  status: "grew" | "shrank" | "new" | "deleted";
}

export interface DiffDto {
  from: SnapshotMeta;
  to: SnapshotMeta;
  netDelta: number;
  grew: number;
  shrank: number;
  entries: DiffEntryDto[];
  /** Per-volume used bytes over time for the growth chart. */
  trend: { takenAt: number; usedBytes: number }[];
  /** Days until full at the current trend, or null if not growing. */
  daysUntilFull: number | null;
}

export type SuggestionGroup = "duplicates" | "devCaches" | "tempFiles" | "oldDownloads" | "staleLarge" | "freeUpCloud";
export type Safety = "safe" | "review" | "caution";

export interface SuggestionDto {
  id: string;
  group: SuggestionGroup;
  safety: Safety;
  nodeId: NodeId;
  path: string;
  name: string;
  bytes: number;
  reason: string;
  howFound: string;
  lastOpened: number | null;
  /** Duplicates: the copy that is kept. */
  keepPath: string | null;
  cloud: CloudInfo | null;
}

export type Decision = "remove" | "keep" | "later";
export type Destination = { kind: "trash" } | { kind: "move"; target: string } | { kind: "compress" } | { kind: "freeUpCloud" };

export interface ApplyRequest { decisions: { suggestionId: string; decision: Decision }[]; destination: Destination }
export interface ApplyResult { applied: number; freedBytes: number; failed: { path: string; message: string }[]; undoId: string }
export interface UndoEntry { id: string; at: number; items: number; bytes: number; destination: Destination; undoable: boolean }

/** An available update, from the signed update manifest on GitHub Releases. */
export interface UpdateInfo {
  /** e.g. "0.3.0" */
  version: string;
  currentVersion: string;
  /** Release date, Unix seconds. */
  publishedAt: number | null;
  /** Release notes in Markdown. Render as plain text or sanitized Markdown, never raw HTML. */
  notes: string;
  /** Download size in bytes, if known. */
  sizeBytes: number | null;
}

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "upToDate"; checkedAt: number }
  | { state: "available"; info: UpdateInfo }
  | { state: "downloading"; info: UpdateInfo; downloaded: number; total: number | null }
  /** Downloaded and verified; installs on restart (auto-update) or now (manual). */
  | { state: "ready"; info: UpdateInfo }
  /** offline = couldn't reach GitHub; download = transfer failed; verify = signature check failed (nothing installed). */
  | { state: "error"; kind: "offline" | "download" | "verify"; message: string };

/** Facts about this build. When `portable` is true the updater is compiled out: no network requests at all,
 *  update commands reject, `getUpdateStatus` returns idle, and `Settings.autoUpdate` is always false. */
export interface BuildInfo {
  portable: boolean;
}

export type MainView = "blockMap" | "ringChart" | "fileTree" | "terrain";
export type Theme = "dark" | "light" | "system";

export interface Settings {
  mainView: MainView;
  theme: Theme;
  density: "comfortable" | "compact";
  scanMode: ScanMode;
  excluded: string[];
  countCloudOnlyInTotals: boolean;
  hiddenProviders: CloudProvider[];
  keepSnapshots: boolean;
  snapshotsToKeep: number;
  terrain: { categoryTint: boolean; pins: { largest: boolean; grew: boolean; duplicates: boolean; cloud: boolean }; cloudValleys: boolean; prefer2d: boolean };
  keptItems: string[];
  protectedPaths: string[];
  /** Default true. Find duplicates by reading (hashing) the contents of same-size local files after a scan.
   *  Never reads cloud-only files. When false, no file contents are ever read. */
  findDuplicates: boolean;
  /** Default true. Check on launch and every 24 h, download in the background, install on next restart. */
  autoUpdate: boolean;
  /** Version the user chose to skip, if any. */
  skippedVersion: string | null;
  /** Unix seconds of the last successful update check (manual or automatic). Set by the backend. */
  lastUpdateCheck: number | null;
  /** Version whose "What's new" card was already shown. Show the card when this differs from appVersion(). */
  notesShownForVersion: string | null;
}
