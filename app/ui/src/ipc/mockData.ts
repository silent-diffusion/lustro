// Fake sample tree for the mock backend, ported from design/prototype/data.js.
// Every name, path and number here is invented. Do not replace with real machine data.
// Sizes are GB (10^9 bytes). `d` = change since the previous snapshot (GB).
// Leaves that the mock refers to (suggestions, pins) carry a stable `key`, because names differ per platform.

import type { Category, CloudProvider, Platform } from "./types";

export type BaseState = "cloudOnly" | "downloaded" | "pinned";

export interface BaseNode {
  name: string;
  /** GB. Leaves only; folders are summed. */
  size?: number;
  cat?: Category;
  children?: BaseNode[];
  d?: number;
  key?: string;
  isNew?: boolean;
  stale?: boolean;
  /** Key of the original, for duplicates. */
  dupOf?: string;
  provider?: CloudProvider;
  state?: BaseState;
  /** System-space item for "Where did my space go?". */
  sys?: { key: string; explanation: string; settingAction: string | null };
  /** A single file (never expanded into a generated subtree). */
  file?: boolean;
  /** Not a real file or folder (e.g. APFS snapshots); shown as kind "other". */
  other?: boolean;
  accessDenied?: boolean;
  /** Sync root of a cloud provider. */
  syncRoot?: boolean;
}

export interface BaseVolume {
  id: string;
  label: string;
  mountPath: string;
  fileSystem: string;
  /** GB */
  total: number;
  /** GB, for volumes other than the scanned one. */
  used?: number;
  isRemote: boolean;
  isRemovable: boolean;
  /** Virtual cloud drive (Google Drive Stream): GB stored in the cloud. */
  cloudTotal?: number;
  provider?: CloudProvider;
}

export interface BaseRemote { provider: CloudProvider; path: string; via: string }

export interface BaseWorld {
  platform: Platform;
  sep: string;
  rootPath: string;
  root: BaseNode;
  volumes: BaseVolume[];
  /** Space the OS reports that isn't a folder (macOS purgeable). */
  extraSys: { key: string; label: string; gb: number; explanation: string }[];
  unexplainedGb: number;
  remotes: BaseRemote[];
}

const F = (name: string, children: BaseNode[], extra?: Partial<BaseNode>): BaseNode => ({ name, children, ...extra });
const L = (name: string, size: number, cat: Category, extra?: Partial<BaseNode>): BaseNode => ({ name, size, cat, ...extra });

export function baseWorld(platform: Platform): BaseWorld {
  const win = platform === "windows";
  const mac = platform === "macos";
  const linux = platform === "linux";

  const videos = F(mac ? "Movies" : "Videos", [
    F("Family 2019-2024", [
      L("beach-trip-2023.mp4", 9.4, "video", { file: true }),
      L("wedding-full.mov", 12.1, "video", { file: true, key: "dupAorig" }),
      L("birthday-clips", 6.8, "video"),
      L("wedding-full (1).mov", 5.7, "video", { file: true, dupOf: "dupAorig", key: "dupA" }),
    ]),
    F("Screen recordings", [
      L("2026-10 tutorials", 8.6, "video", { d: 4.1, key: "tutorials" }),
      L("older", 5.2, "video", { key: "older", stale: true }),
    ]),
    F("Exports", [
      L("edit-v3-final.mp4", 6.3, "video", { file: true, key: "dupBorig" }),
      L("edit-v3-final-FINAL.mp4", 4.1, "video", { file: true, dupOf: "dupBorig", key: "dupB" }),
    ]),
  ]);
  const pictures = F("Pictures", [
    F(win ? "Camera Roll" : mac ? "Photos Library" : "Camera", [
      L("2024", 7.9, "images"), L("2025", 8.4, "images"), L("2026", 5.6, "images", { d: 0.9, key: "pics2026" }),
    ]),
    F("RAW 2023", [L("hiking.raw set", 6.2, "images"), L("city.raw set", 4.7, "images")]),
    L("Screenshots", 3.1, "images"),
  ]);
  const downloads = F("Downloads", [
    L("os-image-24.04.iso", 6.1, "archives", { file: true, d: 6.1, isNew: true, key: "iso" }),
    L("recovery-tools.iso", 3.2, "archives", { file: true }),
    L(win ? "Setup_PhotoSuite_x64.exe" : mac ? "PhotoSuite.dmg" : "photosuite.AppImage", 2.4, "installers", { file: true, stale: true, key: "inst1" }),
    L(win ? "driver-pack-2024.zip" : mac ? "printer-driver-2024.pkg" : "firmware-tools-2024.tar.gz", 3.6, "archives", { file: true, stale: true, key: "drv" }),
    L("old installers", 6.9, "installers", { stale: true, key: "inst14" }),
    L("misc", 3.1, "other"),
  ]);
  const documents = F("Documents", [
    F("Work", [L("reports", 2.8, "documents"), L("presentations", 3.4, "documents")]),
    L("School", 2.9, "documents"),
    L("old-backup.zip", 5.1, "archives", { file: true, stale: true, key: "oldbk" }),
  ]);
  const code = F(win ? "source" : "code", [
    F("webshop", [L("node_modules", 7.1, "code", { key: "nodemod" }), L("src", 0.4, "code"), L(".git", 1.6, "code")]),
    F("ml-notebook", [L(".venv", 4.2, "code", { key: "venv" }), L("datasets", 2.3, "code")]),
    L("other repos", 2.9, "code"),
  ]);
  const games = (name: string) => F(name, [
    L("Skyward Rally", 38.4, "apps", { key: "bigGame" }),
    L("Pixel Harbor", 21.2, "apps", { d: -5.0, key: "pixel" }),
    L("shadercache", 6.1, "apps", { d: 1.1 }),
    L("other games", 8.7, "apps"),
  ]);
  const npmCache = (name: string) => F(name, [L("_cacache", 7.2, "code", { d: 1.8, key: "cacache" })]);
  const browser = F("Browser", [L("Cache", 3.9, "other", { key: "bcache" }), L("Profile", 2.2, "other")]);
  const temp = L("temp files", 9.1, "other", { d: 2.3, key: "tmp" });
  const apps = (name: string) => F(name, [
    L("Photo Suite", 6.4, "apps"), L("Office Suite", 4.2, "apps"), L("IDE Studio", 9.1, "apps"),
    L(linux ? "GPU driver (dkms)" : "GPU Driver", 3.3, "apps"), L("other apps", 12.8, "apps"),
  ]);

  // Cloud sync folders. The provider is set on the folder and inherited.
  const onedrive = (name: string) => F(name, [
    L("Photos", 14.2, "images", { state: "cloudOnly" }),
    L("Shared with me", 7.8, "documents", { state: "cloudOnly" }),
    L("Desktop", 4.6, "documents", { state: "downloaded", key: "freeOD" }),
    L("Notes", 4.3, "documents", { state: "pinned", key: "pinOD" }),
  ], { provider: "oneDrive", syncRoot: true });
  const icloud = (name: string) => F(name, [
    L("iCloud Photos", 9.6, "images", { state: "cloudOnly" }),
    L("Pages documents", 1.2, "documents", { state: "downloaded" }),
    L("Keynote", 0.8, "documents", { state: "pinned" }),
  ], { provider: "iCloud", syncRoot: true });
  // Dropbox on Linux syncs full copies: ordinary local files, tagged with the provider (treated as pinned).
  const dropbox = (name: string, states: boolean) => F(name, [
    L("Client work", 6.4, "documents", { state: states ? "cloudOnly" : "pinned" }),
    L("Design assets", 3.3, "images", { key: "freeDB", state: states ? "downloaded" : "pinned" }),
    L("Contracts", 0.9, "documents", { state: "pinned" }),
  ], { provider: "dropbox", syncRoot: true });
  const box = (name: string) => F(name, [
    L("Team share", 4.1, "documents", { state: "cloudOnly" }),
    L("Reports Q3", 1.1, "documents", { state: "downloaded" }),
  ], { provider: "box", syncRoot: true });
  const other = F("Sync Folder", [
    L("Archive 2022", 2.0, "archives", { state: "cloudOnly" }),
    L("Shared music", 0.6, "audio", { state: "downloaded" }),
  ], { provider: "other", syncRoot: true });
  const gdriveCache = (name: string, withCloud: boolean) => F(name, [
    ...(withCloud ? [L("My Drive", 24.0, "documents", { state: "cloudOnly" })] : []),
    L("Available offline", 3.8, "other", { state: "pinned" }),
    L("Recently opened", 1.6, "other", { state: "downloaded", key: "freeGD" }),
  ], { provider: "googleDrive", syncRoot: true });
  const docker = (name: string, file: boolean) => L(name, 15.3, "code", { d: 6.2, key: "docker", file });

  if (win) {
    const root = F("C:\\", [
      F("Users", [
        F("William", [
          videos, pictures, downloads, documents,
          onedrive("OneDrive"), icloud("iCloud Drive"), dropbox("Dropbox", true), box("Box"), other,
          F("AppData", [
            F("Local", [
              F("Docker", [docker("wsl data.vhdx", true)]),
              F("Temp", [temp]),
              npmCache("npm-cache"),
              F("Packages", [L("store apps", 11.0, "apps")]),
              browser,
              // Google Drive for desktop in Stream mode: only this cache holds local bytes.
              F("Google", [gdriveCache("DriveFS", false)]),
            ]),
            F("Roaming", [L("app settings", 5.4, "apps"), L("editor extensions", 2.7, "code")]),
          ]),
          code, L("Music", 4.1, "audio"),
        ]),
        L("Public", 1.8, "other"),
      ]),
      F("Games", [games("SteamLibrary")]),
      apps("Program Files"),
      F("Program Files (x86)", [L("legacy apps", 11.9, "apps")]),
      F("Windows", [
        L("WinSxS", 11.2, "system", { sys: { key: "winsxs", explanation: "Component store: copies of system files kept for updates and repairs.", settingAction: "windows:diskCleanup" } }),
        L("System32", 7.4, "system"),
        L("Installer", 6.1, "system", { d: 0.6 }), L("other", 6.6, "system"),
      ]),
      L("ProgramData", 7.8, "apps"),
      L("pagefile.sys", 16.0, "system", { file: true, sys: { key: "pagefile", explanation: "Virtual memory: Windows uses this file as overflow RAM.", settingAction: "windows:virtualMemory" } }),
      L("hiberfil.sys", 12.8, "system", { file: true, sys: { key: "hibernation", explanation: "Hibernation file: stores RAM contents for hibernate and Fast Startup.", settingAction: "windows:power" } }),
      L("System Volume Information", 9.3, "system", { accessDenied: true, sys: { key: "restorePoints", explanation: "Restore points and shadow copies.", settingAction: "windows:systemProtection" } }),
      L("$Recycle.Bin", 3.0, "other", { d: -2.1 }),
    ]);
    return {
      platform, sep: "\\", rootPath: "C:\\", root,
      volumes: [
        { id: "C:", label: "C:", mountPath: "C:\\", fileSystem: "NTFS", total: 512, isRemote: false, isRemovable: false },
        { id: "D:", label: "D:", mountPath: "D:\\", fileSystem: "NTFS", total: 2000, used: 1312, isRemote: false, isRemovable: false },
        { id: "E:", label: "E:", mountPath: "E:\\", fileSystem: "exFAT", total: 256, used: 61, isRemote: false, isRemovable: true },
        // Virtual drive from Google Drive (Stream mode). Listed, never scanned for content.
        { id: "G:", label: "G:", mountPath: "G:\\", fileSystem: "Virtual", total: 100, used: 86.4, isRemote: true, isRemovable: false, cloudTotal: 86.4, provider: "googleDrive" },
      ],
      extraSys: [], unexplainedGb: 6.4, remotes: [],
    };
  }

  if (mac) {
    const root = F("Macintosh HD", [
      F("Users", [
        F("william", [
          videos, pictures, downloads, documents,
          F("Library", [
            F("Caches", [npmCache("npm"), browser]),
            F("Containers", [F("Docker", [docker("Docker.raw", true)])]),
            F("Application Support", [L("app settings", 5.4, "apps"), L("editor extensions", 2.7, "code"), F("Steam", [games("steamapps")])]),
            F("CloudStorage", [onedrive("OneDrive-Personal"), gdriveCache("GoogleDrive-william", true), dropbox("Dropbox", true), box("Box-Box")]),
            F("Mobile Documents", [icloud("iCloud Drive")]),
          ]),
          other, code, L("Music", 4.1, "audio"),
          L(".Trash", 3.0, "other", { d: -2.1 }),
        ]),
        L("Shared", 1.8, "other"),
      ]),
      apps("Applications"),
      L("System", 14.8, "system", { sys: { key: "systemData", explanation: "macOS itself, on a separate read-only system volume.", settingAction: null } }),
      F("Library", [L("system-wide app data", 7.8, "apps"), L("Updates", 6.1, "system", { d: 0.6 })]),
      F("private", [F("var", [
        F("vm", [L("swapfile", 8.0, "system", { file: true, sys: { key: "swap", explanation: "Swap: macOS uses this as overflow memory.", settingAction: null } })]),
        F("folders", [temp]),
        L("log", 2.4, "system", { sys: { key: "journal", explanation: "System and app logs.", settingAction: null } }),
      ])]),
      L("Time Machine local snapshots", 9.3, "system", { other: true, sys: { key: "apfsSnapshots", explanation: "APFS snapshots kept by Time Machine. macOS deletes them when space runs low.", settingAction: "macos:timeMachine" } }),
    ]);
    return {
      platform, sep: "/", rootPath: "/", root,
      volumes: [
        { id: "disk3s1", label: "Macintosh HD", mountPath: "/", fileSystem: "APFS", total: 512, isRemote: false, isRemovable: false },
        { id: "disk5s1", label: "Backup", mountPath: "/Volumes/Backup", fileSystem: "APFS", total: 2000, used: 1312, isRemote: false, isRemovable: true },
      ],
      // Purgeable isn't a folder: macOS reports it as space it can free on its own.
      extraSys: [{ key: "purgeable", label: "Purgeable", gb: 12.4, explanation: "Space macOS can reclaim automatically (caches, old snapshots, iCloud copies). Counted as used until macOS frees it." }],
      unexplainedGb: 6.4, remotes: [],
    };
  }

  const root = F("/", [
    F("home", [
      F("william", [
        videos, pictures, downloads, documents,
        dropbox("Dropbox", false),
        F(".cache", [npmCache("npm"), browser]),
        F(".local", [F("share", [
          F("Steam", [games("steamapps")]),
          L("app data", 5.4, "apps"),
          F("Trash", [L("files", 3.0, "other", { d: -2.1 })]),
        ])]),
        F(".config", [L("editor extensions", 2.7, "code")]),
        code, L("Music", 4.1, "audio"),
      ]),
    ]),
    apps("opt"),
    F("usr", [L("lib", 5.1, "system"), L("share", 3.2, "system"), L("bin", 1.3, "system")]),
    F("var", [
      F("cache", [F("apt", [L("archives", 3.4, "system", { sys: { key: "packageCache", explanation: "Package cache: installers apt keeps after installing. Safe to clear with apt clean.", settingAction: null } })])]),
      F("log", [F("journal", [L("journal files", 2.1, "system", { sys: { key: "journal", explanation: "systemd journal logs. journalctl --vacuum-size trims them.", settingAction: null } })])]),
      F("lib", [
        F("snapd", [L("snaps", 6.2, "system", { d: 0.6, sys: { key: "snapFlatpak", explanation: "Snap packages, including old revisions kept for rollback.", settingAction: null } })]),
        F("flatpak", [L("runtimes and apps", 5.1, "apps", { sys: { key: "snapFlatpak", explanation: "Flatpak runtimes shared between apps.", settingAction: null } })]),
        F("docker", [docker("overlay2", false)]),
      ]),
    ]),
    F("tmp", [temp]),
    L("swapfile", 8.0, "system", { file: true, sys: { key: "swap", explanation: "Swap: the kernel uses this as overflow memory.", settingAction: null } }),
  ]);
  return {
    platform, sep: "/", rootPath: "/", root,
    volumes: [
      { id: "nvme0n1p2", label: "/", mountPath: "/", fileSystem: "ext4", total: 512, isRemote: false, isRemovable: false },
      { id: "sda1", label: "/mnt/data", mountPath: "/mnt/data", fileSystem: "ext4", total: 2000, used: 1312, isRemote: false, isRemovable: false },
    ],
    extraSys: [], unexplainedGb: 6.4,
    // Cloud drives on Linux are usually network mounts (rclone, FUSE). Not scanned unless asked.
    remotes: [
      { provider: "oneDrive", path: "/home/william/OneDrive", via: "rclone mount" },
      { provider: "googleDrive", path: "/home/william/GoogleDrive", via: "rclone mount" },
    ],
  };
}

/** Home folder per platform (fake). */
export function homePath(p: Platform): string {
  return p === "windows" ? "C:\\Users\\William" : p === "macos" ? "/Users/william" : "/home/william";
}
