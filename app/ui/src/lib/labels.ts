// Display names for categories, cloud providers and cloud states, plus per-platform wording.
// Shared UI never hard-codes an OS: it asks `wording(platform)`.

import type { Category, CloudProvider, CloudState, Platform, SuggestionGroup } from "../ipc/types";

export const CATEGORIES: Category[] = [
  "video", "images", "audio", "documents", "archives", "installers", "code", "apps", "system", "other",
];

export const CATEGORY_LABEL: Record<Category, string> = {
  video: "Video", images: "Images", audio: "Audio", documents: "Documents", archives: "Archives",
  installers: "Installers", code: "Code & dev", apps: "Apps & games", system: "System", other: "Other",
};

/** CSS custom property for a category (tokens.css uses short names for two of them). */
export function catVar(c: Category | "cloud"): string {
  const key = c === "documents" ? "docs" : c === "code" ? "dev" : c;
  return `--cat-${key}`;
}

export const PROVIDER: Record<CloudProvider, { label: string; short: string }> = {
  oneDrive: { label: "OneDrive", short: "OD" },
  googleDrive: { label: "Google Drive", short: "GD" },
  iCloud: { label: "iCloud", short: "iC" },
  dropbox: { label: "Dropbox", short: "DB" },
  box: { label: "Box", short: "BX" },
  other: { label: "Other provider", short: "··" },
};

export const CLOUD_STATE: Record<CloudState, { label: string; note: string }> = {
  cloudOnly: { label: "Cloud only", note: "Listed here, 0 bytes on this disk" },
  downloaded: { label: "Downloaded", note: "On this disk and counted as local · can be freed back to the cloud" },
  pinned: { label: "Always on this device", note: "Pinned: on this disk and counted as local" },
};

export const GROUP_LABEL: Record<SuggestionGroup, string> = {
  duplicates: "Duplicates", devCaches: "Dev caches", tempFiles: "Temp files",
  oldDownloads: "Old downloads", staleLarge: "Stale large files", freeUpCloud: "Free up space",
};

export interface Wording {
  label: string;
  /** Modifier key label: "Ctrl" or "⌘". */
  mod: string;
  trash: string;
  fileManager: string;
  reveal: string;
  themeSystem: string;
  fastScan: string;
  standardScan: string;
  tempLabel: string;
  unexplained: string;
  protectedPaths: [string, string][];
  snapshotDir: string;
  cloudNote: string;
  spaceLead: string | null;
}

const WORDING: Record<Platform, Wording> = {
  windows: {
    label: "Windows", mod: "Ctrl", trash: "Recycle Bin", fileManager: "File Explorer",
    reveal: "Open in File Explorer", themeSystem: "Match Windows",
    fastScan: "Fast scan reads the NTFS file table directly. It asks Windows for admin permission each time and never stores it.",
    standardScan: "Standard scan needs no admin rights.",
    tempLabel: "Windows temp",
    unexplained: "Used space no folder accounts for: NTFS metadata and folders that need admin to read. Fast scan can see most of it.",
    protectedPaths: [
      ["pagefile.sys", "Managed by Windows · change in Virtual memory settings"],
      ["hiberfil.sys", "Turn off hibernation in Power settings to reclaim"],
      ["C:\\Windows", "Use Windows Disk Cleanup for system files"],
      ["C:\\Program Files", "Uninstall apps from Settings › Apps"],
    ],
    snapshotDir: "%LOCALAPPDATA%\\io.github.silent-diffusion.lustro",
    cloudNote: "Found from the sync folders registered with Windows, plus virtual drives such as Google Drive's G:. Downloaded and pinned files always count as local space.",
    spaceLead: null,
  },
  macos: {
    label: "macOS", mod: "⌘", trash: "Trash", fileManager: "Finder",
    reveal: "Reveal in Finder", themeSystem: "Match macOS",
    fastScan: "Fast scan reads file details in bulk. No admin password needed; Full Disk Access lets it see protected folders.",
    standardScan: "Standard scan walks folders one by one.",
    tempLabel: "macOS temp",
    unexplained: "Used space no folder accounts for: APFS metadata and folders that need Full Disk Access to read.",
    protectedPaths: [
      ["/System", "macOS itself · on a read-only system volume"],
      ["/private/var/vm", "Swap · managed by macOS"],
      ["Time Machine local snapshots", "macOS removes them when space runs low"],
      ["/Applications", "Drag an app to the Trash to uninstall it"],
    ],
    snapshotDir: "~/Library/Application Support/io.github.silent-diffusion.lustro",
    cloudNote: "Found under ~/Library/CloudStorage (and iCloud Drive). Same three states: cloud only, downloaded, always on this Mac. Folders that are cloud-only are never opened, which would start a download.",
    spaceLead: "macOS groups most of these as “System Data” in its Storage settings.",
  },
  linux: {
    label: "Linux", mod: "Ctrl", trash: "Trash", fileManager: "Files",
    reveal: "Show in Files", themeSystem: "Match system",
    fastScan: "Fast scan reads folders in parallel. It never needs root.",
    standardScan: "Standard scan walks folders one by one.",
    tempLabel: "/tmp",
    unexplained: "Used space no folder accounts for: filesystem metadata, reserved blocks and folders you don't have permission to read.",
    protectedPaths: [
      ["/usr", "Installed by your package manager · remove packages instead"],
      ["/boot", "Kernels and boot files"],
      ["/proc, /sys, /dev, /run", "Not real files · never scanned"],
      ["swapfile", "Swap · resize with your distribution's tools"],
    ],
    snapshotDir: "~/.local/share/io.github.silent-diffusion.lustro",
    cloudNote: "Dropbox keeps full local copies. Other cloud drives are usually network mounts (rclone, FUSE): listed, not scanned unless you turn it on.",
    spaceLead: "On Linux the usual suspects are package caches, logs, Snap/Flatpak and Docker.",
  },
};

export function wording(p: Platform): Wording {
  return WORDING[p];
}

/** Keyboard shortcut label in the platform's style: "Ctrl+K" or "⌘K". */
export function kbd(p: Platform, key: string): string {
  const m = WORDING[p].mod;
  return m === "⌘" ? `⌘${key}` : `${m}+${key}`;
}
