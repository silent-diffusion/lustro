# Changelog

What changed in each version of Lustro, written for the people who use it. The section for a
version is also what the app shows when it offers that update.

## [Unreleased]

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
