# lustro-app: the Lustro desktop app (Tauri 2)

The Tauri shell around the `lustro-*` crates. The UI is `app/ui` (Svelte); it talks to this crate only
through the commands in `src/commands.rs` (one per `Backend` method in `app/ui/src/ipc/backend.ts`).

## Run and build

```bash
cd app/ui && npm ci && cd ../src-tauri   # once; installs the pinned Tauri CLI too
../ui/node_modules/.bin/tauri dev            # starts Vite on 127.0.0.1:1420 and opens the app
../ui/node_modules/.bin/tauri dev --no-watch # same, without rebuilding when other crates change
```

The Tauri CLI is pinned to an exact version (2.12.4 or later, which honours
`requireSignedVersion`) in `app/ui/package.json`.

Installer (NSIS, per-user, no admin, WebView2 bootstrapper embedded, Start-menu shortcut):

```bash
powershell -File app/src-tauri/scripts/build-installer.ps1
```

It builds `lustro-scan-helper`, copies it to `binaries/` as a sidecar (installed next to the app exe),
builds the app (whose `build.rs` bakes the helper's SHA-256 into `LUSTRO_HELPER_SHA256`), and writes a
single installer, `target/release/bundle/nsis/Lustro-Setup-<version>.exe`. It always builds
unsigned (`tauri build --no-sign`) and refuses to run while a signing key is in the environment,
so no build script or npm tool can see the key. `scripts/release-assets.ps1` makes the portable zip
and the release notes. In the release workflow, a separate `sign` job on a fresh runner signs the
installer with `scripts/sign-installer.ps1` (Tauri CLI fetched by `scripts/fetch-tauri-cli.mjs`
and checked against the lockfile), and `scripts/release-manifest.ps1` then writes `latest.json`
and `SHA256SUMS.txt`. See `docs/RELEASING.md`.

The bundle `identifier` is `io.github.silent-diffusion.lustro`. It decides the app's data folders
and update identity, so don't change it after the first release.

## Fast scan (elevated helper)

- Only when the user chose Fast (`ScanRequest.mode = "fast"`) does the app pass the helper to
  `lustro_scan::SystemScanner`, which shows a UAC prompt. Never otherwise.
- The helper path is absolute (beside the app exe), never PATH or the current folder, and its hash
  must match the one baked in at build time; otherwise it isn't launched.
- If the user declines, the hash fails or the helper is missing (dev builds, tests), the walker runs
  and `ScanDone.warnings` says so (no paths).

## Layout

| File | What |
|---|---|
| `src/commands.rs` | IPC commands, argument validation |
| `src/scan.rs` | Scan sessions: worker thread → `TreeBuilder`, live `scan-update` (~10/s), `scan-done`, snapshot, suggestions |
| `src/space.rs` | "Where did my space go?" breakdown |
| `src/updates.rs` | Updater behind our own commands, `update-status` event, auto-update loop |
| `src/settings.rs` | Settings JSON in the app config dir |
| `src/state.rs` | Shared state; undo log, last suggestions and snapshots in the app local data dir |
| `capabilities/main.json` | The only permissions the webview has: our commands + event listen |
| `RELEASE_NOTES.md` | Notes of this version (`current_release_notes`) |

## Security

- **CSP** `default-src 'self'`; `connect-src` only the IPC origin; no remote scripts, frames or forms.
  `style-src` allows inline styles (Svelte style attributes); scripts never run inline.
- **Capabilities:** `build.rs` generates an `allow-<command>` permission per command, and
  `capabilities/main.json` grants exactly those plus `core:event:allow-listen/unlisten`.
  No shell, fs, http, opener or dialog plugin is exposed to the webview. The updater plugin is
  used from Rust only.
- **IPC validation** (`commands.rs`):
  - node ids must exist in the current tree (fold ids are negative and fail to parse);
  - a scan root must be a volume, a folder picked with the native dialog (`pick_folder`), or a root
    scanned before; `excluded` must come from Settings;
  - new entries in Settings' `excluded`/`protectedPaths`, and a Move destination, must be picked or
    be a path of the current tree;
  - `reveal` takes a node id; `open_setting` takes an id from `lustro_platform`'s fixed list;
  - paths with `..` or relative paths are rejected; list sizes are capped.
- **Cleanup** goes through `lustro_core::cleanup::plan` + `apply` with `lustro_platform::PlatformTrash` and
  `protected_rules()` + the user's protected paths. Trash only; every apply lands in the undo log.
- **No logging of paths.** Error strings returned to the UI never contain paths.

## Updates

- The endpoint is `https://github.com/silent-diffusion/lustro/releases/latest/download/latest.json`.
  The public key in `tauri.conf.json` (`plugins.updater.pubkey`) is a **placeholder** until the
  maintainers create the key pair; the release workflow refuses to run while it is. **No signing
  key is in the repo.**
- `requireSignedVersion` is on (we have no old releases), so a manifest can't pair a newer version
  with an older signed package.
- `createUpdaterArtifacts` is on, but builds run with `--no-sign`; the installer is signed for the
  updater afterwards by `scripts/sign-installer.ps1`, in the release workflow's separate `sign`
  job, the only job that can read the `TAURI_SIGNING_PRIVATE_KEY` secret.
- The updater is the default cargo feature `updater` (`tauri-plugin-updater` is optional). The
  portable build uses `--no-default-features --features portable`, so the plugin isn't linked.
- Auto-update (default on): check 5 s after launch and every 24 h, download in the background, install
  when the app closes (so the next start runs the new version), or right away via `restart_to_update`.
  Never while a scan or a cleanup runs.

### Dev-only switches (debug builds only; ignored in release builds)

| Env var | Effect |
|---|---|
| `LUSTRO_DEV_UPDATE_MANIFEST` | Check this manifest URL instead (http allowed), e.g. a local `latest.json` |
| `LUSTRO_DEV_UPDATE_PUBKEY` | Use this public key (a throwaway key you keep outside the repo) |
| `LUSTRO_DEV_SCAN_ROOT` | Offer this folder as a "Test folder" volume, so the Scan button scans it |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` | WebView2's own switch, for driving the UI in tests |

A test manifest looks like:

```json
{
  "version": "0.2.0",
  "notes": "## What's new\n- ...",
  "pub_date": "2026-10-27T12:00:00Z",
  "platforms": {
    "windows-x86_64": { "signature": "<contents of the .sig file>", "url": "http://127.0.0.1:18765/Lustro-Setup-0.2.0.exe" }
  }
}
```

With the placeholder key, a check finds the update and the install is refused with
`{ state: "error", kind: "verify" }`, which is the expected result.
