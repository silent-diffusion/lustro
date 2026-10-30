# Changelog

What changed in each version of Lustro, written for the people who use it. The section for a
version is also what the app shows when it offers that update.

## [Unreleased]

## [0.1.2] - 2026-10-29

### What's new
- **Route card** on the full terrain view. Clicking something now lists every folder on the way from the
  drive down to it, numbered to match dots along the route on the map. Each row shows the
  folder's size and its share of the folder above. Click a row or a dot to go to that folder.

### Fixed
- Cloud-only folders now show up as lakes on the terrain, and by name in the other charts,
  instead of being lumped into "Other". Folders that have never been opened on this PC still
  aren't counted yet, because looking inside them would make your cloud provider download their
  file list.

## [0.1.1] - 2026-10-29

### What's new
- **Scans happen in place.** Starting a scan no longer opens its own page. Overview and Explore
  fill in live as Lustro counts, with progress and a Stop button in the bar under the top bar.
- **Full view** button on the terrain (or press Home) takes you back to the whole-drive view
  after zooming in or flying to something.
- The terrain keeps slowly turning at the full view. It pauses while you use it and eases back in
  when you stop, but never while you're zoomed in or have something selected.
- **Up next** in Cleanup is its own card, showing up to 8 items, with "Show all" for the rest.

### Fixed
- Overview now uses the main view chosen in Settings.
- A long cleanup queue no longer runs off the screen; it becomes a single progress bar.

## [0.1.0] - 2026-10-28

The first public preview of Lustro for Windows.

### What's new
- **Scan a drive or any folder** and watch the folder tree grow live as Lustro counts.
- **"Where did my space go?"** explains the space Windows uses for itself: page file, hibernation
  file, restore points and the component store, plus anything nobody can account for.
- **Explore** your drive four ways: a block map, a ring chart, a file tree and a 3D terrain you can
  orbit. Search and filter by type, size and date.
- **What grew** compares each scan with the previous one and shows how fast the drive is filling up.
- **Cleanup** is a review queue for duplicates (checked byte for byte), dev caches, temp files, old
  downloads and large files you haven't opened in a long time. Nothing changes until you click
  Apply, everything goes to the Recycle Bin, and you can undo it.
- **Cloud files** from any provider show whether they are cloud-only, downloaded, or always kept on
  this device. Lustro never downloads them.
- **Fast scan** (optional) reads the drive's index for a much quicker scan. It asks for
  administrator permission; the app itself never runs as administrator.
- **Automatic updates**, on by default and signed. Turn them off in Settings.
- Dark and light themes, and keyboard shortcuts throughout.

### Privacy
Lustro works offline. The only network request is the update check, which you can turn off in
Settings. Every update is signed, and Lustro checks the signature before installing it.

### Known issues
- The installer isn't code-signed yet, so Windows SmartScreen asks you to confirm the first install
  (**More info → Run anyway**).
- After you undo a cleanup, the restored items appear in the views after the next scan.
