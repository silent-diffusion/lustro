/* Fake sample data for the Lustro prototype.
   Every name, path and number here is invented. Do not replace with real machine data.
   Sizes are in GB. d = change since the previous snapshot (GB).
   The tree is built for the prototype platform (Windows, macOS or Linux) picked in the prototype bar.
   Leaves that pages refer to carry a stable `id`, because names differ between platforms. */
(function () {
  const F = (name, children, extra) => Object.assign({ name, children }, extra);
  const L = (name, size, cat, extra) => Object.assign({ name, size, cat }, extra);

  let pid = "win";
  try { pid = localStorage.getItem("lustro-platform") || "win"; } catch (e) {}
  if (!["win", "mac", "linux"].includes(pid)) pid = "win";
  const win = pid === "win", mac = pid === "mac", linux = pid === "linux";

  // ---- Platform wording and conventions (shared UI never hard-codes an OS) ----
  const PLATFORMS = {
    win: {
      id: "win", label: "Windows", sep: "\\", mod: "Ctrl", trash: "Recycle Bin", fileManager: "File Explorer",
      reveal: "Open in File Explorer", home: "C:\\Users\\William", themeSystem: "Match Windows",
      fastScan: "Fast scan reads the NTFS file table directly. It asks Windows for admin permission each time and never stores it.",
      standardScan: "Standard scan needs no admin rights.",
      tempLabel: "Windows temp", protected: [
        ["pagefile.sys", "Managed by Windows · change in Virtual memory settings"],
        ["hiberfil.sys", "Turn off hibernation in Power settings to reclaim"],
        ["C:\\Windows", "Use Windows Disk Cleanup for system files"],
        ["C:\\Program Files", "Uninstall apps from Settings › Apps"],
      ],
      excluded: ["C:\\Users\\William\\AppData\\Local\\Temp", "D:\\VM Images"],
      unexplained: "Used space no folder accounts for: NTFS metadata and folders that need admin to read. Fast scan can see most of it.",
    },
    mac: {
      id: "mac", label: "macOS", sep: "/", mod: "⌘", trash: "Trash", fileManager: "Finder",
      reveal: "Reveal in Finder", home: "/Users/william", themeSystem: "Match macOS",
      fastScan: "Fast scan reads file details in bulk. No admin password needed; Full Disk Access lets it see protected folders.",
      standardScan: "Standard scan walks folders one by one.",
      tempLabel: "macOS temp", protected: [
        ["/System", "macOS itself · on a read-only system volume"],
        ["/private/var/vm", "Swap · managed by macOS"],
        ["Time Machine local snapshots", "macOS removes them when space runs low"],
        ["/Applications", "Drag an app to the Trash to uninstall it"],
      ],
      excluded: ["/Users/william/Library/Caches", "/Volumes/Backup"],
      unexplained: "Used space no folder accounts for: APFS metadata and folders that need Full Disk Access to read.",
    },
    linux: {
      id: "linux", label: "Linux", sep: "/", mod: "Ctrl", trash: "Trash", fileManager: "Files",
      reveal: "Show in Files", home: "/home/william", themeSystem: "Match system",
      fastScan: "Fast scan reads folders in parallel. It never needs root.",
      standardScan: "Standard scan walks folders one by one.",
      tempLabel: "/tmp", protected: [
        ["/usr", "Installed by your package manager · remove packages instead"],
        ["/boot", "Kernels and boot files"],
        ["/proc, /sys, /dev, /run", "Not real files · never scanned"],
        ["swapfile", "Swap · resize with your distribution's tools"],
      ],
      excluded: ["/home/william/.cache", "/mnt/data/vm-images"],
      unexplained: "Used space no folder accounts for: filesystem metadata, reserved blocks and folders you don't have permission to read.",
    },
  };
  const P = PLATFORMS[pid];

  // ---- Shared building blocks (same files, platform-appropriate names) ----
  const videos = F(mac ? "Movies" : "Videos", [
    F("Family 2019-2024", [
      L("beach-trip-2023.mp4", 9.4, "video"),
      L("wedding-full.mov", 12.1, "video", { big: true, id: "dupAorig" }),
      L("birthday-clips", 6.8, "video"),
      L("wedding-full (1).mov", 5.7, "video", { dup: "wedding-full.mov", id: "dupA" }),
    ]),
    F("Screen recordings", [
      L("2026-10 tutorials", 8.6, "video", { d: 4.1 }),
      L("older", 5.2, "video", { id: "older" }),
    ]),
    F("Exports", [L("edit-v3-final.mp4", 6.3, "video", { id: "dupBorig" }), L("edit-v3-final-FINAL.mp4", 4.1, "video", { dup: "edit-v3-final.mp4", id: "dupB" })]),
  ]);
  const pictures = F("Pictures", [
    F(win ? "Camera Roll" : mac ? "Photos Library" : "Camera", [L("2024", 7.9, "images"), L("2025", 8.4, "images"), L("2026", 5.6, "images", { d: 0.9 })]),
    F("RAW 2023", [L("hiking.raw set", 6.2, "images"), L("city.raw set", 4.7, "images")]),
    L("Screenshots", 3.1, "images"),
  ]);
  const downloads = F("Downloads", [
    L("os-image-24.04.iso", 6.1, "archives", { d: 6.1, isNew: true }),
    L("recovery-tools.iso", 3.2, "archives"),
    L(win ? "Setup_PhotoSuite_x64.exe" : mac ? "PhotoSuite.dmg" : "photosuite.AppImage", 2.4, "installers", { stale: true, id: "inst1" }),
    L(win ? "driver-pack-2024.zip" : mac ? "printer-driver-2024.pkg" : "firmware-tools-2024.tar.gz", 3.6, "archives", { stale: true, id: "drv" }),
    L(win ? "installers (14 files)" : mac ? "installers (14 .dmg/.pkg)" : "installers (14 .deb/.AppImage)", 6.9, "installers", { stale: true, id: "inst14" }),
    L("misc (312 files)", 3.1, "other"),
  ]);
  const documents = F("Documents", [
    F("Work", [L("reports", 2.8, "docs"), L("presentations", 3.4, "docs")]),
    L("School", 2.9, "docs"),
    L("old-backup.zip", 5.1, "archives", { stale: true, id: "oldbk" }),
  ]);
  const code = F(win ? "source" : "code", [
    F("webshop", [L("node_modules", 7.1, "dev", { id: "nodemod" }), L("src", 0.4, "dev"), L(".git", 1.6, "dev")]),
    F("ml-notebook", [L(".venv", 4.2, "dev", { id: "venv" }), L("datasets", 2.3, "dev")]),
    L("other repos", 2.9, "dev"),
  ]);
  const games = (name) => F(name, [
    L("Skyward Rally", 38.4, "apps", { big: true }),
    L("Pixel Harbor", 21.2, "apps", { d: -5.0 }),
    L("shadercache", 6.1, "apps", { d: 1.1 }),
    L("other games (6)", 8.7, "apps"),
  ]);
  const npmCache = (name) => F(name, [L("_cacache", 7.2, "dev", { d: 1.8, id: "cacache" })]);
  const browser = F("Browser", [L("Cache", 3.9, "other", { id: "bcache" }), L("Profile", 2.2, "other")]);
  const temp = L("temp files (8,214)", 9.1, "other", { d: 2.3, id: "tmp" });
  const apps = (name) => F(name, [
    L("Photo Suite", 6.4, "apps"), L("Office Suite", 4.2, "apps"), L("IDE Studio", 9.1, "apps"),
    L(linux ? "GPU driver (dkms)" : "GPU Driver", 3.3, "apps"), L("other apps (48)", 12.8, "apps"),
  ]);

  // Cloud sync folders. state: "cloud" = cloud only (0 bytes here), "downloaded" = on disk and can be
  // freed back to the cloud, "pinned" = always on this device. provider is set on the folder and inherited.
  const onedrive = (name) => F(name, [
    L("Photos", 14.2, "images", { state: "cloud" }),
    L("Shared with me", 7.8, "docs", { state: "cloud" }),
    L("Desktop", 4.6, "docs", { state: "downloaded", id: "freeOD" }),
    L("Notes", 4.3, "docs", { state: "pinned", id: "pinOD" }),
  ], { provider: "onedrive" });
  const icloud = (name) => F(name, [
    L("iCloud Photos", 9.6, "images", { state: "cloud" }),
    L("Pages documents", 1.2, "docs", { state: "downloaded" }),
    L("Keynote", 0.8, "docs", { state: "pinned" }),
  ], { provider: "icloud" });
  const dropbox = (name, states) => F(name, [
    L("Client work", 6.4, "docs", states ? { state: "cloud" } : {}),
    L("Design assets", 3.3, "images", Object.assign({ id: "freeDB" }, states ? { state: "downloaded" } : {})),
    L("Contracts", 0.9, "docs", states ? { state: "pinned" } : {}),
  ], { provider: "dropbox" });
  const box = (name) => F(name, [
    L("Team share", 4.1, "docs", { state: "cloud" }),
    L("Reports Q3", 1.1, "docs", { state: "downloaded" }),
  ], { provider: "box" });
  const other = F("Sync Folder", [
    L("Archive 2022", 2.0, "archives", { state: "cloud" }),
    L("Shared music", 0.6, "audio", { state: "downloaded" }),
  ], { provider: "other" });
  const gdriveCache = (name, withCloud) => F(name, [
    ...(withCloud ? [L("My Drive", 24.0, "docs", { state: "cloud" })] : []),
    L("Available offline", 3.8, "other", { state: "pinned" }),
    L("Recently opened", 1.6, "other", { state: "downloaded", id: "freeGD" }),
  ], { provider: "gdrive" });
  const docker = (name) => L(name, 15.3, "dev", { d: 6.2, big: true, id: "docker" });

  let root, drive, drives, remotes = [], extraSys = [];

  if (win) {
    root = F("C:\\", [
      F("Users", [
        F("William", [
          videos, pictures, downloads, documents,
          onedrive("OneDrive"), icloud("iCloud Drive"), dropbox("Dropbox", true), box("Box"), other,
          F("AppData", [
            F("Local", [
              F("Docker", [docker("wsl data.vhdx")]),
              F("Temp", [temp]),
              npmCache("npm-cache"),
              F("Packages", [L("store apps", 11.0, "apps")]),
              browser,
              // Google Drive for desktop in Stream mode: files appear on the virtual G: drive; only this
              // cache holds local bytes, so it counts toward C: like any other folder.
              F("Google", [gdriveCache("DriveFS", false)]),
            ]),
            F("Roaming", [L("app settings", 5.4, "apps"), L("editor extensions", 2.7, "dev")]),
          ]),
          code, L("Music", 4.1, "audio"),
        ]),
        L("Public", 1.8, "other"),
      ]),
      F("Games", [games("SteamLibrary")]),
      apps("Program Files"),
      F("Program Files (x86)", [L("legacy apps (22)", 11.9, "apps")]),
      F("Windows", [
        L("WinSxS", 11.2, "system", { sys: "Component store: copies of system files kept for updates and repairs." }),
        L("System32", 7.4, "system"),
        L("Installer", 6.1, "system", { d: 0.6 }), L("other", 6.6, "system"),
      ]),
      L("ProgramData", 7.8, "apps"),
      L("pagefile.sys", 16.0, "system", { sys: "Virtual memory: Windows uses this file as overflow RAM." }),
      L("hiberfil.sys", 12.8, "system", { sys: "Hibernation file: stores RAM contents for hibernate and Fast Startup." }),
      L("System Volume Information", 9.3, "system", { sys: "Restore points and shadow copies." }),
      L("$Recycle.Bin", 3.0, "other", { d: -2.1 }),
    ], { rootPath: "C:\\" });
    drive = { short: "C:", title: "C:", name: "C: Windows SSD", capacity: 512, fs: "NTFS" };
    drives = [
      drive,
      { short: "D:", name: "D: Data HDD", capacity: 2000, used: 1312, fs: "NTFS" },
      { short: "E:", name: "E: USB Backup", capacity: 256, used: 61, fs: "exFAT" },
      // Virtual drive from Google Drive (Stream mode). Listed, never scanned for content.
      { short: "G:", name: "G: Google Drive", virtual: true, provider: "gdrive", cloudTotal: 86.4, fs: "Virtual" },
    ];
  }

  if (mac) {
    root = F("Macintosh HD", [
      F("Users", [
        F("william", [
          videos, pictures, downloads, documents,
          F("Library", [
            F("Caches", [npmCache("npm"), browser]),
            F("Containers", [F("Docker", [docker("Docker.raw")])]),
            F("Application Support", [L("app settings", 5.4, "apps"), L("editor extensions", 2.7, "dev"), F("Steam", [games("steamapps")])]),
            // File Provider: every provider's sync root lives here, with the same three states.
            F("CloudStorage", [onedrive("OneDrive-Personal"), gdriveCache("GoogleDrive-william", true), dropbox("Dropbox", true), box("Box-Box")]),
            F("Mobile Documents", [icloud("iCloud Drive")]),
          ]),
          other, code, L("Music", 4.1, "audio"),
          L(".Trash", 3.0, "other", { d: -2.1 }),
        ]),
        L("Shared", 1.8, "other"),
      ]),
      apps("Applications"),
      L("System", 14.8, "system", { sys: "macOS itself, on a separate read-only system volume." }),
      F("Library", [L("system-wide app data", 7.8, "apps"), L("Updates", 6.1, "system", { d: 0.6 })]),
      F("private", [F("var", [
        F("vm", [L("swapfile", 8.0, "system", { sys: "Swap: macOS uses this as overflow memory." })]),
        F("folders", [temp]),
        L("log", 2.4, "system", { sys: "System and app logs." }),
      ])]),
      L("Time Machine local snapshots", 9.3, "system", { sys: "APFS snapshots kept by Time Machine. macOS deletes them when space runs low." }),
    ], { rootPath: "/" });
    drive = { short: "HD", title: "Macintosh HD", name: "Macintosh HD", capacity: 512, fs: "APFS" };
    drives = [drive, { short: "Backup", name: "Backup (external)", capacity: 2000, used: 1312, fs: "APFS" }];
    // Purgeable isn't a folder: macOS reports it as space it can free on its own.
    extraSys = [{ t: "Purgeable", v: 12.4, e: "Space macOS can reclaim automatically (caches, old snapshots, iCloud copies). Counted as used until macOS frees it." }];
  }

  if (linux) {
    root = F("/", [
      F("home", [
        F("william", [
          videos, pictures, downloads, documents,
          // Dropbox on Linux syncs full copies: ordinary local files, tagged with the provider.
          dropbox("Dropbox", false),
          F(".cache", [npmCache("npm"), browser]),
          F(".local", [F("share", [
            F("Steam", [games("steamapps")]),
            L("app data", 5.4, "apps"),
            F("Trash", [L("files", 3.0, "other", { d: -2.1 })]),
          ])]),
          F(".config", [L("editor extensions", 2.7, "dev")]),
          code, L("Music", 4.1, "audio"),
        ]),
      ]),
      apps("opt"),
      F("usr", [L("lib", 5.1, "system"), L("share", 3.2, "system"), L("bin", 1.3, "system")]),
      F("var", [
        F("cache", [F("apt", [L("archives", 3.4, "system", { sys: "Package cache: installers apt keeps after installing. Safe to clear with apt clean." })])]),
        F("log", [F("journal", [L("journal files", 2.1, "system", { sys: "systemd journal logs. journalctl --vacuum-size trims them." })])]),
        F("lib", [
          F("snapd", [L("snaps", 6.2, "system", { d: 0.6, sys: "Snap packages, including old revisions kept for rollback." })]),
          F("flatpak", [L("runtimes and apps", 5.1, "apps", { sys: "Flatpak runtimes shared between apps." })]),
          F("docker", [docker("overlay2")]),
        ]),
      ]),
      F("tmp", [temp]),
      L("swapfile", 8.0, "system", { sys: "Swap: the kernel uses this as overflow memory." }),
    ], { rootPath: "/" });
    drive = { short: "/", title: "/", name: "/ (root · nvme0n1p2)", capacity: 512, fs: "ext4" };
    drives = [drive, { short: "/mnt/data", name: "/mnt/data", capacity: 2000, used: 1312, fs: "ext4" }];
    // Cloud drives on Linux are usually network mounts (rclone, FUSE). Not scanned unless asked.
    remotes = [
      { provider: "onedrive", path: "/home/william/OneDrive", via: "rclone mount" },
      { provider: "gdrive", path: "/home/william/GoogleDrive", via: "rclone mount" },
    ];
  }

  // Fill in sizes, parents and paths.
  (function walk(n, parent) {
    n.parent = parent || null;
    n.path = !parent ? n.rootPath : parent.parent ? parent.path + P.sep + n.name : parent.path + n.name;
    if (!n.provider && parent && parent.provider) n.provider = parent.provider;
    if (n.state === "cloud") n.cloud = true; // cloud only: listed, but no bytes on this disk
    if (n.children) {
      n.size = 0; n.onDisk = 0; n.d = 0;
      n.children.forEach((c) => { walk(c, n); n.size += c.size; n.onDisk += c.onDisk; n.d += c.d || 0; });
      n.children.sort((a, b) => b.size - a.size);
    } else {
      n.onDisk = n.cloud ? 0 : n.size;
      n.d = n.d || 0;
    }
  })(root);

  drive.used = root.onDisk + 6.4 + extraSys.reduce((s, x) => s + x.v, 0);
  drive.unexplained = 6.4;

  // Snapshot history of used space on the main drive (GB), oldest first.
  const history = [
    { date: "2026-07-28", used: drive.used - 44.5 }, { date: "2026-08-11", used: drive.used - 39.8 },
    { date: "2026-08-25", used: drive.used - 36.3 }, { date: "2026-09-08", used: drive.used - 31.7 },
    { date: "2026-09-22", used: drive.used - 29.4 }, { date: "2026-10-06", used: drive.used - 26.9 },
    { date: "2026-10-21", used: drive.used - root.d }, { date: "2026-10-28", used: drive.used },
  ];

  window.Lustro = window.Lustro || {};
  window.Lustro.P = P;
  window.Lustro.PLATFORMS = PLATFORMS;
  window.Lustro.data = { root, drive, drives, remotes, extraSys, history, scannedAt: "Today, 9:41 AM", previousAt: "Oct 21, 8:12 PM" };
})();
