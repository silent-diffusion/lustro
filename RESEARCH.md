# Lustro: Research & Feature Plan

_Compiled 2026-10-28. Target platform: Windows 11 (desktop)._

## 1. Existing apps

| App | Platform / price | What people like | What people complain about |
|---|---|---|---|
| **WizTree** | Windows; free for personal use, ~$25 supporter/commercial | Extremely fast (reads the NTFS Master File Table directly), portable, Size vs Allocated columns | Fast path only on NTFS **with admin rights**; slow on FAT/exFAT/network drives; proprietary; treemap was a late add-on and is basic |
| **WinDirStat** (2.x) | Windows; free, open source | Classic cushion treemap, now actively maintained again (2.0.1 in Oct 2024 after 19 years; 2.5 added dark mode and MFT scanning; 2.8 hardened duplicate finder), hash-based duplicate finder, CSV export | Long-standing reputation for being slow and heavy; dated UI |
| **TreeSize** (Free / Personal / Pro) | Windows; Free is non-commercial only, paid tiers are annual subscriptions (~€20–47/yr) | Familiar Explorer-like hierarchy, deep reporting; Pro has **snapshot comparison** (grew = red, shrank = green), scheduled scans, CLI | Best features locked behind subscription; Free edition limited; Server support pushed to Pro |
| **SpaceSniffer** | Windows; free, portable | Live, zoomable treemap, practical filters | Effectively unmaintained; dated |
| **FolderSizes** | Windows; paid | Trend analyzer: charts folder/disk growth across snapshots | Paid, enterprise-oriented |
| **DaisyDisk** | macOS; paid | Beautiful sunburst chart, easy "collect and delete" flow (G2 4.8/5) | Mac-only, paid, "one-trick pony" |
| **GrandPerspective** | macOS; free, open source | Lightweight treemap, Finder integration | Visualization only, no cleanup actions |
| **Filelight / Baobab / QDirStat / ncdu / dust** | Linux (some cross-platform); free | Sunburst (Filelight), WinDirStat-style (QDirStat), terminal speed (ncdu/dust) | Little Windows love; KDE users have an open request for saved snapshots and comparison (Bug 395973 → 290349) |

## 2. Recurring wishes and gaps

1. **WizTree speed with WinDirStat's visuals.** People pick one or the other. Nobody fully owns "instant scan + great visualization".
2. **"What changed?"** Users want to know what *grew* since last week, not just what's big now. Only paid tools (TreeSize Pro, FolderSizes) do this well; open-source ones lack it.
3. **Honest sizes for cloud files.** Every sync service (OneDrive, Google Drive, iCloud, Dropbox, Box and others) can leave "online-only" files that show a large size but take ~0 bytes locally. Files from the same services that *are* downloaded do use space. Most tools confuse users here.
4. **Explain the mystery space.** "Windows says my disk is full but the folders don't add up": hidden system files (hiberfil.sys, pagefile.sys, shadow copies/System Restore, WinSxS, hard links, compressed files) are poorly explained.
5. **Safe cleanup.** Users want to act on findings (delete, move, compress) but fear deleting something important. DaisyDisk's "collect then confirm" flow is praised; Windows tools mostly offer a raw delete.
6. **No subscription, no admin requirement for basic use.** Paywalls (TreeSize) and admin prompts (WizTree) are frequent friction.
7. **Modern UI.** Dark mode, high-DPI, and a non-1990s look. WinDirStat only got dark mode in 2026.

## 3. Proposed feature set

### Core (MVP)
- Scan a whole drive, all drives, or a chosen folder; show progress and allow browsing **while** scanning.
- Fast NTFS path (MFT read when elevated), with a graceful non-admin fallback that clearly says what it can't see.
- Linked views: **folder tree** (size, % of parent, file count, modified date) + **treemap** and/or **sunburst**; clicking one highlights the other.
- **Top N largest files and folders** list, filterable.
- **Breakdown by file type/category** (video, images, documents, archives, installers, code, system), color-coded consistently across views.
- Search and filters: name/extension, size range, age (last modified/accessed), location.
- Open in Explorer, copy path, properties, delete to Recycle Bin (never permanent by default).
- Export to CSV/JSON.

### Gap-closers (our differentiators)
- **Snapshots and "What grew"**: save scans automatically, diff any two, show top growers since last scan and a simple growth trend per drive. Free.
- **Cloud-aware sizes for every provider**: separate "size on disk" from "logical size". Every cloud file gets a provider badge (OneDrive, Google Drive, iCloud, Dropbox, Box, other) and one of three states:
  - **Cloud only**: 0 bytes here.
  - **Downloaded**: on disk, but can be freed back to the cloud.
  - **Always on this device**: on disk, pinned by the user.
  
  Downloaded and pinned files count as local space, like any other file. There's also an option to hide cloud-only files.
- **"Where did my space go?" explainer**: a drive-level bar that reconciles used space = scanned files + system files (pagefile, hibernation, restore points, WinSxS) + unaccounted, each with a plain-English explanation and safe link to the right Windows setting.
- **Smart insights**: duplicates (hash-based), old installers in Downloads, stale large files not opened in N months, dev caches (node_modules, .gradle, pip/npm caches, Docker images), game libraries, temp folders.
- **Cleanup basket**: collect candidates across views, review the total, then send to Recycle Bin, move to another drive, or zip. Includes an undo log. Protected-path guardrails for system and app folders.

### Later / nice-to-have
- Scheduled background scans + low-space notifications.
- Hard-link and compression awareness (don't double count).
- Network/NAS and external drives.
- Command-line mode for power users.
- Age heatmap view (color treemap by last-accessed instead of type).

## 4. Decisions so far
- **Main chart:** the block map (treemap) is the default. A Settings page switches the main view to a ring chart (sunburst), a file tree, or other views we add.
- **Signature view:** a 3D, rotatable topographic "terrain of your computer".
- **Live scan:** the folder tree visibly builds while the scan runs.
- **"What grew":** confirmed as a key feature.
- **Cleanup:** its own top-level tab.
- **Public repo:** the project will go on GitHub. No private data may ever be committed, and security and privacy are first-class requirements.
- **Cloud files:** the same rules apply to all providers (OneDrive, Google Drive, iCloud, Dropbox, Box and others). Files saved on the device must be shown as on the device. See section 6.
- **Code signing:** deferred for now; see section 7.
- **Design:** the direction was approved on 2026-10-28, and all six screens are mocked up in `design/`. See `design/README.md` and `design/IA.md`. Key design decisions:
  - **Tabs:** Overview · Explore · What grew · Cleanup, plus a **Scan** button and a Settings gear. Live scan is a state of the app, not a tab.
  - **Explore:** linked folder tree, main view and details pane. The view menu offers Block map / Ring chart / File tree / Terrain 3D, and Settings stores the default.
  - **Terrain:** the elevation is a 3D cushion surface over a squarified treemap. Cloud-only data shows as **valleys below ground**. Pins: flag = largest file, coral arrow = top grower, twin = duplicate set, hollow cloud = cloud-only. A dashed route traces the path to the selection, and a 2D flat-map fallback is available. Later: a Voronoi treemap to fill the round ring.
  - **Cleanup** is a **review queue**: one item at a time, with Remove / Keep / Later (R/K/L). A "Where it lives" map uses the user's main view. A queue bar shows progress. Nothing moves until **Apply**, which goes to the Recycle Bin by default (move or compress are options). "Keep" means the item is never suggested again.
  - **Colors:** one color per category, used everywhere. Grew = coral, shrank = lime.
  - **Type:** Manrope for the UI, JetBrains Mono for paths and sizes.
  - **Sample data:** all fake (`C:\Users\William\...`).

## 5. Tech stack recommendation (Rust confirmed; macOS and Linux planned, see section 8)

**Rust scanning core + Tauri 2 desktop shell + web UI (TypeScript, with three.js for 3D).**

- **Why Rust for the scanner:** speed comparable to WizTree's, with memory safety. Existing crates:
  - `ntfs-reader` and `usn-journal-rs` read the NTFS Master File Table and the USN change journal.
  - `usn-journal-rs` can also resolve file IDs to paths.
  - Both are Windows-only and need admin rights, so we pair them with a normal directory walker as the fallback.
- **Why Tauri over Electron:** it uses the OS's built-in WebView, so installers are about 3–10 MB instead of 80–200 MB, and it uses less memory. It also has a deny-by-default permission model (capabilities), which suits our security goals.
- **Why a web UI:** the design reference (soft cards, contour terrain) is much easier to build with HTML/CSS/WebGL than with native Windows UI. three.js handles the 3D terrain; Canvas or WebGL handles fast treemaps with 100k+ rectangles.
- **Alternatives considered:**
  - **C# + WinUI 3:** most "native Windows" feel, but 3D and custom visuals are harder.
  - **Electron:** easiest, but heavy, and it ships a full Chromium to keep patched.
  - **Pure Rust UI (egui/Slint):** fast, but harder to reach the polished look we want.

## 6. Implementation notes for key features

- **Live tree building:** the scanner streams batched updates to the UI about 10×/sec (not one message per file) over a Tauri channel. The UI uses a virtualized tree so 1M+ nodes stay smooth.
  - Gotcha: MFT records arrive in table order, not folder order, and the fast path finishes in seconds. For the "watch it build" effect, either rebuild the tree incrementally from parent references or let the UI animate the reveal.
  - The slower walker path naturally builds the tree top-down.
- **What grew / snapshots:**
  - Store per-folder totals (not every file) in a local SQLite database after each scan, keyed by root path. Comparisons only make sense for the same root.
  - Diffing two snapshots gives the top growers and shrinkers.
  - A simple trend line per drive can forecast "full in N days".
  - Later: use the USN journal for instant incremental rescans.
- **Cloud-aware sizes (all providers):** the same rules apply to every provider.
  - **Two ways providers put files on a PC:**
    1. **Placeholder files in a normal folder.** Windows' Cloud Files API ("Files On-Demand"), which OneDrive uses.
       - Other modern sync clients can use the same API. I couldn't confirm from public docs exactly which of Dropbox, iCloud for Windows and Box Drive do, so we verify this on a test machine with each client installed.
       - Find these folders, and the provider's name, through Windows' sync-root registration (`StorageProviderSyncRootManager`). That works for any provider, including ones we haven't heard of.
    2. **A virtual drive.** For example, Google Drive for desktop in **Stream** mode shows a `G:` drive.
       - The actual local bytes live in a cache folder (by default `%LOCALAPPDATA%\Google\DriveFS`).
       - Files marked "Available offline" are stored in that cache.
       - Count local usage from the cache folder.
       - List the virtual drive's contents as cloud files, but don't count them as local space and never read them.
       - Google Drive's **Mirror** mode stores ordinary local files, which count normally.
  - **One state per cloud file:**
    - **Cloud only:** `RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN` / `OFFLINE` attributes, ~0 allocated bytes.
    - **Downloaded:** hydrated, not pinned, so it can be freed back to the cloud.
    - **Always on this device:** the `PINNED` attribute or pin state.
  - **Ground truth is the allocated size on disk.** A downloaded or pinned file counts toward used space like any local file. Only cloud-only files are excluded from local totals.
  - **Never open or read a cloud file's contents**, which would trigger a download. That includes hashing for duplicate detection, which must skip cloud-only files. Only read attributes and metadata.
  - **Show logical size and on-disk size separately**, with a provider badge and the state.
  - **Cleanup can suggest "free up space"** (return a downloaded file to cloud-only) as a safer alternative to deleting. The provider still holds the file. Pinned files are never suggested without a clear warning.
- **Accurate totals:** count hard links once (by file ID), and don't follow junctions or symlinks (this prevents double counting and infinite loops). Use allocated size for compressed and sparse files.
- **3D terrain:**
  - Lay out folders with a squarified treemap, then turn it into a heightmap: elevation = size, or depth, by setting. Contour lines come from the heightmap.
  - Render with three.js: a plane plus a height texture, orbit controls, and level-of-detail for big drives.
  - Waypoint pins mark the largest files, top growers, duplicates and placeholders. Clicking a pin or region selects it in the tree.
  - Always offer the 2D views as an accessible fallback.
- **Switchable views:** one data model (the scanned tree), with every view as a plug-in renderer, so adding new visualizations later is cheap.

## 7. Security and privacy principles

**The app itself**
1. **Offline except for updates.**
   - No telemetry, analytics, crash uploads or ads.
   - **Auto-update is on by default** (decided 2026-10-28). The only request the app makes is an HTTPS GET of the signed update manifest from GitHub Releases, which carries no identifiers. Turning auto-update off stops all network access.
   - Updates are signature-verified before install. The updater's private key is never committed.
   - The UI loads no remote content (strict Content Security Policy, `default-src 'self'`).
2. **Least privilege.**
   - Runs unelevated by default.
   - Admin is requested only when the user chooses "fast scan". Ideally a tiny, read-only elevated helper does the MFT read, so the UI never runs as admin.
   - Tauri capabilities expose only the commands we need. Every path received over IPC is validated.
3. **Read-only scanning.** Scanning never modifies, opens for writing, or downloads (hydrates) anything.
4. **Safe cleanup.**
   - Delete goes to the Recycle Bin only.
   - Always a review step with a total, plus an undo log.
   - Protected-path guardrails (Windows, Program Files, the system's own files) and no "delete everything matching".
5. **Local data stays local.**
   - Snapshots contain file paths, which can reveal names and projects. They live in `%LOCALAPPDATA%`, can be cleared in one click, and have a retention setting.
   - Exports offer "anonymize paths" (for example, replacing the user folder with `%USERPROFILE%`).

**The repository**
6. **Repo hygiene.**
   - `.gitignore` for builds, snapshots, logs and `.env` files.
   - Test data is generated (fake tree with names like `C:\Users\William\...`), never captured from a real machine.
   - Screenshots for the README use the fake data only.
   - Note that this working folder sits under a real user profile path, so absolute local paths must never be committed.
   - `Design ideas/` (third-party reference images) is excluded via `.gitignore` and must never be published. The README and screenshots use only our own mockups.
7. **Secret scanning in three layers:**
   - a `gitleaks` pre-commit hook;
   - GitHub secret scanning with push protection turned on;
   - a `gitleaks` check in CI.
   - Any leaked secret must be revoked, not just deleted, because git history keeps it.
8. **Supply chain.**
   - Locked dependency versions, plus `cargo audit`, `npm audit` and Dependabot.
   - Releases built by GitHub Actions, not on a personal machine.
   - Checksums published.
   - **One-click installer:** an NSIS `.exe` that installs per user (no admin), attached to every GitHub Release. Updates use Tauri's updater signature, a free key pair separate from Windows code signing, so auto-update works even before code signing.
  - **Code signing: deferred until the user says so.** Note: until the installer is code-signed, Windows SmartScreen shows "Windows protected your PC" (More info → Run anyway) on first install. That's a real hurdle for non-technical users, so revisit signing before promoting the download widely. Options for later: SignPath Foundation (free for qualifying open-source projects) or Azure Artifact Signing (low cost). SmartScreen reputation builds over time either way.

## 8. Cross-platform plan (Windows first, then macOS and Linux)

The user confirmed Rust and plans macOS and Linux versions. **The stack doesn't change.** Rust and Tauri 2 both support all three. What changes is how the code is organized, and a short list of platform differences.

### Code structure: Cargo workspace
| Crate | Role | Platform-specific? |
|---|---|---|
| `lustro-core` | Tree model, size math, categories, snapshots (SQLite), diffs, insights, cleanup rules | No: pure Rust, tested on every OS |
| `lustro-scan` | A `Scanner` trait with backends: `windows` (MFT/USN fast path + walker), `macos` (bulk attribute walker), `linux` (parallel walker) | Yes, behind `#[cfg(target_os)]` |
| `lustro-cloud` | A `CloudProvider` trait: detects sync roots and each file's cloud state per OS | Yes |
| `lustro-platform` | Trash, "reveal in file manager", privilege prompts, the system-space explainer, protected paths | Yes |
| `lustro-app` | Tauri shell plus the web UI | Mostly no |

Rules:
- Everything above the scanner works on a platform-neutral tree, so most logic is tested once on any OS.
- Tests use a **generated fake file tree**, never a real disk.

### What differs by OS
| Topic | Windows | macOS | Linux |
|---|---|---|---|
| Fast scan | MFT read (needs admin), USN journal for rescans | `getattrlistbulk` bulk metadata; FSEvents for change tracking | Parallel walk (`statx`/`getdents`); `fanotify`/`inotify` for changes |
| Permissions | Admin only for the fast path | **Full Disk Access** prompt needed to see protected folders; explain it before asking | Runs as the user; root never required |
| Cloud files | Cloud Files API placeholders + Google Drive virtual drive (section 6) | **File Provider**: iCloud, OneDrive, Dropbox and Google Drive live under `~/Library/CloudStorage`. Cloud-only files are "dataless" (`SF_DATALESS` flag) | No official OneDrive, iCloud or Google Drive clients. Dropbox syncs full copies. Cloud drives are usually **FUSE/rclone mounts**: treat them as remote and don't descend by default |
| ⚠ Scan trap | Reading content downloads the file | **Even `stat` on a dataless *folder* can trigger a download.** Check the folder's flag before descending | Scanning a network or FUSE mount can be very slow |
| Double counting | Hard links (file ID), junctions | APFS clones share blocks, firmlinks, Time Machine local snapshots | Hard links (`dev`+`inode`), bind mounts, btrfs/ZFS reflinks and snapshots |
| Don't descend | — | `/System/Volumes` duplicates via firmlinks | `/proc`, `/sys`, `/dev`, `/run`; don't cross into other filesystems unless asked |
| "Where did my space go?" | pagefile, hiberfil, restore points, WinSxS | "System Data", purgeable space, APFS snapshots, VM swap | Package caches (apt/dnf), journald logs, Snap/Flatpak, Docker images |
| Delete | Recycle Bin | Trash | freedesktop Trash. The `trash` crate covers all three |
| Webview (UI) | WebView2 (Chromium): fastest WebGL | WKWebView (Safari engine) | **WebKitGTK: weakest WebGL.** Tauri's own docs warn of slow WebGL on some GPUs. The 2D fallback for the terrain becomes essential, and the app should automatically suggest it on Linux if frame rate is low |
| File names | Case-insensitive, UTF-16 | Usually case-insensitive, Unicode normalization quirks | Case-sensitive, **can be non-UTF-8**: store raw `OsString`, display with replacement characters |
| Packages | MSI / NSIS | `.dmg`. Apple's notarization is effectively required for normal users to open it; this connects to the deferred code-signing decision | AppImage, `.deb`, `.rpm`, later Flatpak |

### Design implications
- **Neutral wording:**
  - "Fast scan" instead of "Fast NTFS mode".
  - "Trash / Recycle Bin" per OS.
  - "Reveal in Explorer / Finder / Files".
- **Sample paths** switch per platform in mockups (`C:\Users\William`, `/Users/william`, `/home/william`).
- **Shortcuts:** Ctrl ↔ ⌘.
- **Window chrome:** macOS traffic-light buttons sit on the left, so the top bar needs space for them.
- **Overview's system-space explainer** has per-OS categories.
- **Settings → Scanning** shows only the options that apply to the current OS.

### Testing plan
- **Windows:** locally, in this environment (full UI and scanner).
- **Linux:**
  - CI on Linux runners for builds, unit tests, and scanner tests against fake trees.
  - Hands-on checks on a Linux machine or VM.
  - WSL2 on this PC is also an option.
- **macOS:** GitHub Actions macOS runners build the app and run the tests on every push. Hands-on UI checks need a real Mac (the user or a tester) before any macOS release.
- **CI** runs on all three OSes from the first commit, so platform breakage shows up immediately even before the Mac and Linux UIs get attention.

## Sources
- https://www.itechguides.com/best/disk-space-analyzers/
- https://www.alternativeto.net/software/windirstat/
- https://alternativeto.net/software/wiztree
- https://alternativeto.net/news/2024/10/windirstat-releases-first-update-in-19-years-with-many-new-features-and-improvements/
- https://alternativeto.net/news/2026/1/windirstat-2-5-adds-dark-mode-ntfs-mft-scanning-improved-search-and-new-file-actions/
- https://www.neowin.net/amp/windirstat-280/
- https://diskanalyzer.com/about
- https://www.neowin.net/amp/wiztree-432/
- https://yomotherboard.com/question/should-i-try-wiztree-if-i-already-use-windirstat/
- https://www.capterra.ae/software/1011586/treesize
- https://jam-software.com/treesize_free/comparison.shtml
- https://manuals.jam-software.com/treesize/EN/disk_usage_comparison.html
- https://foldersizes.com/screens/trend-analyzer
- https://bugs.kde.org/show_bug.cgi?id=395973
- https://www.techradar.com/reviews/pc-mac/software/utilities/other-software/daisydisk-2-0-6-955340/review
- https://www.g2.com/products/daisydisk/reviews
- https://alternativeto.net/software/grandperspective
- https://support.microsoft.com/en-us/office/onedrive-sizes-and-dates-don-t-match-c42f93ad-f565-45e1-a91c-2491ef0dd138
- https://tech-insider.org/tauri-vs-electron-2026/
- https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/
- https://docs.rs/crate/ntfs-reader/0.4.6
- https://docs.rs/usn-journal-rs
- https://pkg.go.dev/github.com/chaugan/swinv/internal/usn
- https://libraries.io/cargo/mathom-scanner
- https://github.com/mattes3/THREE.Terrain
- https://notes.dsebastien.net/30+Areas/33+Permanent+notes/33.02+Content/Treemap
- https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfgetplaceholderinfo
- https://newreleases.io/project/github/m3trik/pythontk/release/v0.8.72
- https://v2.tauri.app/reference/acl/capability
- https://oneuptime.com/blog/post/2026-01-25-secret-scanning-gitleaks/view
- https://www.decryptiondigest.com/blog/stop-secrets-leaking-to-github-developer-playbook
- https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
- https://nullprogram.com/blog/2026/04/25/
- https://support.google.com/drive/answer/13401938?hl=en
- https://knowledge.workspace.google.com/admin/drive/advanced-drive-for-desktop-configuration
- https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants
- https://learn.microsoft.com/en-us/windows/win32/api/cfapi/ne-cfapi-cf_pin_state
- https://learn.microsoft.com/en-us/onedrive/files-on-demand-windows
- https://learn.microsoft.com/en-us/windows/win32/cfapi/cloud-files-api-portal
- https://v2.tauri.app/develop/debug/linux-graphics/
- https://bugs.webkit.org/show_bug.cgi?id=278695
- https://eclecticlight.co/2023/10/29/last-week-on-my-mac-dataless-files-arrive/
- https://eclecticlight.co/2026/05/09/explainer-file-provider-and-cloud-services/
- https://mjtsai.com/blog/2023/05/11/getting-ready-for-dataless-files
- https://developer.apple.com/documentation/system/fileflags.md
- https://docs.rs/trash
