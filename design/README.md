# Lustro: Design Brief

**Status:** direction approved 2026-10-28. All six key screens are mocked up. Cleanup was redesigned as a review queue on the same day, after early feedback.

## What it is
Lustro is a Windows desktop app that shows where your disk space went, what changed since last time, and what you can safely clear. It tells you the facts first. Cleanup lives in its own tab and is never a side effect of browsing.

## Principles
1. **Facts before actions.** Every view explains *what* and *where* before it offers *delete*.
2. **One color means one thing.** File categories keep the same color in the block map, ring chart, terrain and lists.
3. **Show the work.** Scans build visibly. Sizes say whether they are on disk or only in the cloud. Unexplained space is named as unexplained.
4. **Calm by default.** Dark, low-chroma surfaces. Accent color is reserved for selection and the one primary action on the screen.
5. **Private by design.** Nothing leaves the machine. Mockups use fake data only (`C:\Users\William\...`).

## Look and feel
Inspired by a third-party dashboard shot (not included in this repo):
- Near-black window with rounded cards (24px radius) and hairline borders.
- A centered **pill navigation**. The primary action (**Scan**) is a filled pale-blue pill on the right, like "Plan climb" in the reference.
- Muted accents: pale blue (selection and primary), lime, coral, amber.
- Thin-line data graphics: contour lines, dotted routes, pins on stems.

Tokens live in [`tokens.css`](tokens.css). The light theme is a full token override, not an afterthought.

### Logo
The terrain view in miniature: a hill on a disc, seen at an angle and sliced into contour lines, with a tall main peak and lower shoulders whose contours come in different sizes. It's generated from a small heightfield (three bumps on a disc), so the lines are real contours of real slopes, not hand-drawn rings. Each contour is filled with the background color, so higher levels hide the lines behind them. The disc has a thin shaded edge, and the summit contour is filled with the accent pale blue, like a selected contour. The in-app version uses `currentColor`, `--bg` and `--accent`, so it works in both themes.
- [`logo.svg`](logo.svg): the mark on its own. In the app it's drawn inline (`Lustro.icons.logo`) with `currentColor`.
- [`icon.svg`](icon.svg): the app icon, the mark on a dark rounded tile.

### Type
- UI: **Manrope** (fallback: Segoe UI Variable). Sizes 11 / 13 / 15 / 18 / 24 / 40 px, weights 500 and 650.
- Paths and sizes: **JetBrains Mono** (fallback: Cascadia Mono), with tabular numbers so columns line up.

### Spacing and shape
- 4px grid. Card padding 20–24px. Gap between cards 16px.
- Radii: 10 (controls), 16 (insets), 24 (cards), pill (nav and chips).

### Category colors
| Category | Dark | Light | Examples |
|---|---|---|---|
| Video | `#ef7654` coral | `#d4512d` | .mp4 .mkv .mov |
| Images | `#d6ec8c` lime | `#7fa61b` | .jpg .png .heic .raw |
| Audio | `#f3b05a` amber | `#c9861a` | .mp3 .flac .wav |
| Documents | `#a8cfde` pale blue | `#3f8fae` | .pdf .docx .xlsx |
| Archives | `#c4a5e8` lavender | `#8a63c9` | .zip .7z .iso |
| Installers | `#e88bb0` rose | `#c4527f` | .exe .msi in Downloads |
| Code & dev | `#7fd3b8` mint | `#2d9c7c` | node_modules, caches, repos |
| Apps & games | `#8fa8f0` periwinkle | `#4e6fd6` | Program Files, Steam |
| System | `#8a9097` slate | `#6b7178` | pagefile, hiberfil, WinSxS |
| Other | `#5c6168` graphite | `#a4a9af` | everything else |
| Cloud only | pale-blue **outline with hatching** | same | Cloud-only files from any provider (OneDrive, Google Drive, iCloud, Dropbox, Box…): listed, 0 bytes on this disk |

### Cloud files: one set of rules for every provider
Every cloud item has a **provider tag** (text abbreviations, not logos: OD OneDrive · GD Google Drive · iC iCloud · DB Dropbox · BX Box · ·· other) and one of three **states**:

| State | On this disk? | Counted as used space? | How it's drawn |
|---|---|---|---|
| **Cloud only** | No (0 bytes) | No (unless the user turns on "Count cloud-only files in totals") | Hatched pale-blue outline; a valley on the terrain |
| **Downloaded** | Yes | Yes | Normal category color + an outlined tag (provider + download icon). Cleanup can offer to free it back to cloud-only |
| **Always on this device** (pinned) | Yes | Yes | Normal category color + a filled tag (provider + pin icon). Never freed without a warning |

- **Google Drive in Stream mode** shows up as a virtual `G:` drive. It's listed in the drive switcher and in Overview's **Cloud storage** card. Its local bytes are the `AppData\Local\Google\DriveFS` cache on C:, which counts as local; the rest of G: is cloud-only.
- **Overview → Cloud storage:** one row per provider, with a bar for what's on this device vs cloud only.
- **Settings → Scanning → Cloud files:** show/hide per provider, "Count cloud-only files in totals" (off by default), and a locked "Never download cloud files" rule. Duplicate checks skip cloud-only files.

Comparison colors: **grew = coral**, **shrank = lime**. These are only used in the What grew view and growth pins, where category color is turned off.

## Signature view: the terrain of your computer
A 3D, rotatable contour map that shows real data:

| Terrain feature | Meaning |
|---|---|
| Ground layout | A squarified treemap of the scanned folder: each folder owns a patch of land in proportion to its size. |
| Elevation | A cushion surface (the same math as WinDirStat's shaded block map, lifted into 3D). Every folder adds a rounded hill over its patch, so **deeply nested, heavy folders become tall peaks** and shallow sprawl stays low. |
| Contour lines | Each band is one equal step of height. Tightly packed lines mean many nested folders. Major lines are every fifth band. |
| Ground tint (optional) | The dominant file category of that patch, so the map can be read like a geology map. |
| Compass ring | Top-level folders are labeled around the rim at the angle where they sit (`Users`, `Windows`, `Games` and so on). |
| Cloud-only files | **Valleys that dip below ground.** The land around them slopes down to ground level like hillsides, and the shoreline is drawn in pale blue. From there the valley floor sinks below ground, deeper for more cloud-only data, so you can see how much lives only in the cloud (any provider) without it counting as local space. Downloaded and pinned cloud files are ordinary land, because they do use local space. Orbit underneath, or press **Look under**, to see the depth. From below, the land is a solid base, not a hollow shell: it's the terrain pressed flat, in the same category colors, slightly darker, with each mountain's contours as faint grey rings only (no bright major lines). The valleys keep their full contour lines, so they stand out clearly against the base. |
| Pins | **Flag** = largest file. **Coral arrow** = top grower since the last scan. **Pale-blue twin** = duplicate set. **Hollow cloud** = cloud-only placeholder. |
| Dashed route | The path from the drive root to the selected item, drawn across the terrain like the climbing route in the reference. |

You can drag to rotate, scroll to zoom, hover a pin for details and click to open it in Explore. When zoomed in, a **Full view** button (bottom-left, or the Home key) flies back to the whole drive. A 2D flat-map toggle draws the same contours from above for anyone who finds 3D hard to read.

The prototype uses a square treemap inside a round ring. For production, a **Voronoi treemap** would fill the circle naturally. That's a later refinement.

## Cleanup: a review queue, not a checklist
Early feedback on the first Cleanup design: it was "too much information and still not enough". It had long grouped checklists but no sense of *where* files were. The redesign:

- **One item at a time.** A decision card shows the item's name, size, share of its folder, why it was suggested, how it was found, and when it was last opened. For a duplicate, it shows which copy is kept and which is removed.
- **Three choices:** **Remove** (R), **Keep** (K: never suggest it again), **Later** (L). After you choose, the queue moves to the next undecided item.
- **Where it lives.** A map shows the drive in your **main view from Settings** (block map, ring chart, file tree or terrain). A pin marks the current item, and folders outside its neighbourhood are dimmed. Items marked to remove are hatched coral, kept items have a lime outline, and items still in the queue are outlined. In the terrain view, the camera flies in on the item and the dashed route shows its path. Items too small to draw are pinned on the smallest folder that contains them.
- **Visible progress.** A queue bar has one segment per suggestion, sized by how much space it holds and colored by your decision. Group chips (Duplicates, Dev caches and so on) filter the queue and show how many are left.
- **Free up space (optional group).** Downloaded cloud files can be returned to cloud-only instead of deleted. The provider keeps the file and it stays listed. The button reads **Free up space** in pale blue, the map marks these items with the cloud hatch, and Apply lists them separately. Pinned files carry an amber warning that freeing them removes offline access.
- **Apply at the end.** Nothing moves until you press Apply. The review dialog lists every path and lets you choose where the items go: the Recycle Bin (the default), D:, or compressed in place.
- **Less clutter.** Protected paths and History are folded into one line at the bottom, which you can expand.

## Platforms: Windows first, macOS and Linux to follow
The design is platform-neutral. Shared UI never hard-codes an OS; a small platform layer (`Lustro.P` in `prototype/data.js`) supplies the words, paths and conventions.

| | Windows | macOS | Linux |
|---|---|---|---|
| Sample home | `C:\Users\William` | `/Users/william` | `/home/william` |
| Delete goes to | Recycle Bin | Trash | Trash |
| Reveal action | Open in File Explorer | Reveal in Finder | Show in Files |
| Shortcuts | Ctrl+K | ⌘K | Ctrl+K |
| Window chrome | Controls top-right | **Traffic lights top-left**: the top bar leaves room for them | GNOME-style close button top-right |
| Fast scan | Reads the NTFS file table; asks for admin each time | Bulk metadata read; **Full Disk Access** explained before asking | Parallel walk; never needs root |
| "Where did my space go?" | pagefile, hibernation, restore points, WinSxS | swap, Time Machine local snapshots, purgeable space (macOS calls most of this "System Data") | apt package cache, journal logs, Snap/Flatpak, Docker |
| Cloud | Sync folders + Google Drive's virtual G: drive | Everything under `~/Library/CloudStorage` (and iCloud Drive), same three states | Dropbox keeps full local copies; other cloud drives are **remote mounts** (rclone/FUSE): listed, "not scanned", with **Scan anyway** |
| Terrain | 3D by default | 3D by default | **Starts in 2D** with "Switched to 2D for smoother performance · Try 3D anyway" (weaker WebGL in WebKitGTK) |

- Wording is neutral everywhere: "Fast scan" (never "Fast NTFS"), "this computer" (never "this PC"), "Match system" theme labels per OS.
- **Settings → Scanning** shows only the options that apply to the current OS (APFS clones and firmlinks on macOS; "stay on one filesystem" and skipping /proc, /sys, /dev and /run on Linux; junctions on Windows).
- **Prototype only:** a dashed "Prototype platform" switch at the bottom-left of every page swaps the sample tree, wording, shortcuts and window chrome. It isn't product UI.

## Files
- [`IA.md`](IA.md): tabs, screens, navigation.
- [`tokens.css`](tokens.css): design tokens.
- [`logo.svg`](logo.svg), [`icon.svg`](icon.svg): the logo mark and the app icon.
- [`prototype/`](prototype/): clickable mockups. They make no network requests: the fonts (Manrope, JetBrains Mono; SIL Open Font License) and three.js (MIT) are bundled in `prototype/vendor/` with their license files.
  - `index.html` Overview · `explore.html` Explore (Block map / Ring chart / File tree / Terrain 3D) · `scan.html` Live scan · `grew.html` What grew · `cleanup.html` Cleanup · `settings.html` Settings.
  - Shared code: `app.css` (components), `shell.js` (navigation, theme, helpers), `viz.js` (2D charts), `terrain.js` (3D terrain), `updates.js` (update dialog, toast, What's new), `data.js` (**fake** sample data).
  - Updates: Settings → Updates has a dashed **Prototype · simulate** row that triggers every update state (available, up to date, offline, failed download, failed signature, auto-update ready, ready during a scan, after restart).
  - To view them, run `python -m http.server 5500 --bind 127.0.0.1 --directory design` and visit `http://localhost:5500/prototype/`. Opening the files directly works too, but browsers block local module scripts there, so the terrain views stay blank.
  - The choices you make in Settings (default view, theme) are saved in the browser, so they carry across the mockups. Cleanup's map uses the same default view.
  - Cleanup keyboard shortcuts: R remove · K keep · L later · ← → move through the queue.
