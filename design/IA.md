# Information Architecture

## Window layout
```
┌──────────────────────────────────────────────────────────────────────┐
│ ◣ Lustro        ( Overview | Explore | What grew | Cleanup )  [⟳ Scan] ⚙ │
├──────────────────────────────────────────────────────────────────────┤
│ Context bar: drive · last scan time · size on disk / capacity · scope│
├──────────────────────────────────────────────────────────────────────┤
│ Cards                                                                 │
└──────────────────────────────────────────────────────────────────────┘
```
- **Center pill nav:** the four tabs.
- **Right:** **Scan** (primary pill) and **Settings** (gear icon).
- **Context bar:** the current scan target (drive or folder), when it was scanned, and a drive switcher. Every tab shares it.

## Platforms
Same tabs and layout on Windows, macOS and Linux. What changes is the wording, sample paths, window chrome, system-space categories, cloud handling and which scanning options appear. See the "Platforms" table in `README.md`. On macOS the top bar starts after the traffic-light buttons.

## Tabs

### 1. Overview (home)
The answer to "how is my disk doing?".
- **Drive cards:** used / free, with a capacity bar for each drive.
- **Where did my space go?** One stacked bar that adds up: *scanned files by category* + *system files* (pagefile, hibernation, restore points) + *cloud-only (0 bytes, any provider)* + *unexplained*. Each segment has a plain-English explanation.
- **Cloud storage:** one row per provider (OneDrive, Google Drive, iCloud, Dropbox, Box, other), showing what's on this device (downloaded + pinned) vs cloud only. Google Drive's virtual G: drive appears here, with its local cache counted as local.
- **Terrain hero card:** a small 3D terrain with pins. "Open in Explore" takes you to it.
- **Since last scan:** net change, plus the top three growers. Links to What grew.
- **Insights:** duplicates, old installers, dev caches, stale large files. These link to **Cleanup**, they don't delete.

### 2. Explore (main analysis)
Three linked panes:
- **Left: folder tree.** Size, % of parent, file count, modified. Sortable.
- **Center: main visualization.** **Block map** by default. A view menu in the card header switches between **Block map · Ring chart · File tree · Terrain 3D**. Settings stores the default.
- **Right: details.** The selected item's path, size on disk vs logical size, category mix, largest children, actions (Open in Explorer, Copy path, Add to cleanup basket).
- **Top filter row:** search, category chips, size range, age.

Selecting in any pane updates the other two.

### 3. What grew
- **Compare:** snapshot A ▸ snapshot B (default: previous scan ▸ latest).
- **Net change** headline and a growth chart for each drive over time.
- **Growers / shrinkers** table with deltas (coral up, lime down) and a sparkline for each row.
- **Delta block map:** blocks sized by current size and colored by change.
- **New since A** and **Deleted since A** lists.

### 4. Cleanup
Separate on purpose. It's a **review queue**: you decide on one suggestion at a time, and nothing runs until you apply.
- **Queue bar:** one segment per suggestion, sized by space and colored by decision (coral remove, lime keep, amber outline later). Long queues that don't fit switch to a compact progress bar (one equal slice per item, a marker on the current one). Shows "n of N decided", running totals, and **Apply · free X GB**.
- **Group chips:** Duplicates · Dev caches · Temp files · Old downloads · Stale large files · **Free up space** (optional: returns downloaded cloud files to cloud-only; pinned files show a warning). Each filters the queue and shows how many are left.
- **Where it lives** (left): the main view from Settings, with the current item pinned and its neighbourhood highlighted. It shows breadcrumbs and a line like "Inside X (size). Removing this frees N% of that folder." The view can be switched here too. Clicking another queued item on the map jumps to it.
- **Decision card** (right): group and safety label, name, folder, size, why it's suggested, how it was found, and when it was last opened. Duplicates show which copy is kept and which is removed. Buttons: **Remove (R)**, **Keep (K)**, **Later (L)**, then previous/next.
- **Up next** (own card under the decision card, same column): the next 8 undecided items with group icon, name and size; "+N more · Show all" expands the rest.
- **Apply dialog:** lists every path. Destination: Recycle Bin (default: "Items go to the Recycle Bin. You can undo from History while they're still there."), move to another drive, or compress. Kept items stop being suggested (they can be managed in Settings). Drives without a Recycle Bin (network drives, some USB drives) refuse deletion: "This drive has no Recycle Bin, so Lustro won't delete from it. Use Move instead."
- **Footer (collapsed):** protected paths with the reason for each, and History, a log of the last 100 cleanups. Undo for deleted items only works while they're still in the Recycle Bin, which the user or the system can empty at any time, so no time limit is promised.

### Live scan (a state, not a screen)
Pressing **Scan** never changes the screen. The scan runs in place:
- **Scan strip** in the context bar: pulse dot, what is being scanned, the scan mode (Standard or Fast scan; on Windows Fast scan needs admin, on macOS it benefits from Full Disk Access, on Linux it never needs root), live counters (files, found, elapsed), the current path, **Stop**, and a thin progress line. The Scan button reads "Scanning · 41%" and never cancels.
- **Overview and Explore draw the tree as it grows.** Cards using scan data show a "Live" tag; "Where did my space go?" fills in with a hatched "Not scanned yet" remainder; Explore's folder tree grows in real time with the busiest branch highlighted.
- Cards that need a finished scan (Since last scan, Insights, Cleanup suggestions, What grew) keep the previous results, dimmed, until it finishes.
- When it finishes, the strip briefly shows the result ("Scan complete · +4.2 GB since Oct 21", **See what grew**, any folders that couldn't be read). A stopped scan is marked **Partial**.
- First run: Overview shows a single "Scan {drive}" card, then fills in live.

### Settings (gear)
- **Visualization:** main view for Explore and the Cleanup map (Block map / Ring chart / File tree / Terrain 3D), terrain options (category tint, pin types, cloud valleys, 2D fallback), how blocks are labeled.
- **Appearance:** theme (Dark / Light / Match Windows), density.
- **Scanning:** scan mode (Standard / Fast scan; how Fast scan works differs by OS), excluded folders, hard-link handling, and **Cloud files**: show/hide per provider, count cloud-only files in totals (off by default), and never download cloud files (locked on).
- **Snapshots:** keep automatically (yes/no), how many to keep, storage location (local only).
- **Cleanup safety:** protected paths, always use the Recycle Bin, confirmation level, and kept items (things you chose to keep, which are never suggested again).
- **Updates:** current version, **Auto-update** (on by default: checks daily, downloads in the background, installs on next start, never during a scan or cleanup), **Check for updates** with its status (last checked, up to date, available, downloading, ready, couldn't check) and a skipped-version row with Undo. A found update opens the **Update available** dialog (notes, **Download and install** in one click, Not now, Skip this version).
- **Privacy:** no telemetry; the only network request is the update check, which sends no personal information and stops when auto-update is off. Plus a button to clear all saved snapshots.

### Update surfaces outside Settings
- **Ready toast** (bottom-right, not modal): "Lustro x.y.z is ready" with **Restart to update**, **Later** and *See what's new*. Held while a scan or cleanup runs; meanwhile a dot on the Settings gear.
- **What's new** card on Overview, shown once after an update.
