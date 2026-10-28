# Contracts

These interfaces connect the parts of Lustro, so the scanner, core, app and UI can change independently. Change them deliberately: open an issue or a pull request that explains what you need and why, and update both sides (Rust and TypeScript) together.

| Contract | File | Used by |
|---|---|---|
| Data model | `crates/lustro-core/src/model.rs` | everyone (Rust) |
| Scanner and cloud traits | `crates/lustro-core/src/scan.rs` | Scanner (implements `Scanner`), Cloud (implements `CloudClassifier`), Core (implements `ScanSink` via `TreeBuilder`) |
| UI DTOs | `app/ui/src/ipc/types.ts` | UI, Terrain; mirrored by the Rust `lustro-core::dto` module |
| Backend interface | `app/ui/src/ipc/backend.ts` | UI (`mock.ts` in the browser, `tauri.ts` in the app) |
| Terrain API | `app/ui/src/terrain/types.ts` | Terrain (implements), UI (calls) |

## Data flow
```
lustro-cloud ──CloudClassifier──┐
                            ▼
OS ──► lustro-scan (Scanner) ──ScanRecord batches──► lustro-core TreeBuilder (ScanSink)
                                                    │ live deltas (NodeDto) ~10/s
                                                    ▼
                                    Tauri commands/events ──► UI (Backend)
                                                    │
                       lustro-core SnapshotStore ◄──────┘ on scan done → diff, trend
                       lustro-core insights ──► SuggestionDto ──► Cleanup ──► lustro-platform trash
```

## Key rules baked into the contracts
- **`allocated_size` is the ground truth for "used space".** Cloud-only files have ~0 allocated bytes and are reported separately (`NodeDto.cloudOnly`). Downloaded and pinned cloud files count as local.
- **Hard links count once:** use `FileId` and `link_count`.
- **Scanners emit `ScanRecord`s in any order.** The MFT path emits children before their parents, so `TreeBuilder` buffers orphans until their parent arrives.
- **`CloudClassifier` decides from `RawAttrs` alone.** It never opens files. `should_descend` stops the scanner entering remote drives and cloud-only folders (macOS dataless folders, where even `stat` can trigger a download).
- **Names are `OsString` in Rust.** The UI gets `to_string_lossy()`.
- **Paths are display-only.** They are never logged, never sent over the network, and only included in exports if the user asks (with optional anonymisation).
- **Fold nodes in `TreeDto`** (the "other" bucket for small children) have id `-(parentId + 1)`. Rust serialises TreeDto ids as `i64`.
- **`cloudSummary()`** gives a per-provider rollup.
- **Updates:** `appVersion`, `checkForUpdate`, `installUpdate`, `restartToUpdate`, `skipVersion`, `onUpdateStatus`, plus `Settings.autoUpdate` (default true) and `skippedVersion`. Added 2026-10-28. Release notes are Markdown: render them as text or sanitized Markdown, never raw HTML.
  - Revision: `UpdateStatus` errors carry `kind` (offline, download or verify); `Settings.lastUpdateCheck` and `notesShownForVersion` were added, plus `currentReleaseNotes()`.
  - `installUpdate` must never restart during a scan or cleanup.
  - No "installed date" is provided, so the UI leaves that line out.
- **`getUpdateStatus()`** (`update_status`) and **`pickFolder()`** (`pick_folder`) were added 2026-10-28. `pick_folder` registers the returned path as user-picked for IPC path validation.
- **`listSnapshots` returns oldest first.** The root `NodeDto.name` is the scanned path (for example `C:\`), and snapshots are keyed by it.
- **`buildInfo()`** (`build_info`) returns `BuildInfo { portable }`. The portable build compiles the updater out and makes no network requests at all.
- **`Settings.findDuplicates`** (default true) controls whether file contents are ever read for duplicate detection.
- **DTO JSON is camelCase.** Rust enums serialise to the camelCase strings used in `types.ts`.

## IPC mapping
Each `Backend` method becomes a Tauri command of the same name in snake_case (for example `listVolumes` → `list_volumes`). The scan streams through the events `scan-update` (`ScanUpdate`) and `scan-done` (`ScanDone`), emitted at most about 10 times per second.
