# lustro-scan-helper protocol (version 1)

The app never runs elevated. For a Fast scan on Windows it starts `lustro-scan-helper.exe`
through a UAC prompt. The helper reads the NTFS Master File Table (MFT) and streams
metadata records back over a named pipe. The app applies exclusions and cloud
classification itself, then feeds the records to `TreeBuilder`.

Code:
- `crates/lustro-scan/src/helper/`
  - `protocol.rs`: the encoder and decoder.
  - `mod.rs`: the app side, `HelperScanner`.
  - `child.rs`: the helper side.
  - `sys.rs`: the Windows calls.
- `crates/lustro-scan/src/prune.rs`: validation and filtering on the app side.

## What the helper may do
The helper only reads the MFT of the volume holding the requested root and streams
records for the tree under that root.
- It has no other commands.
- It writes, deletes and opens no files. The raw volume handle is opened read-only.
- It never prints or logs anything. The binary has no console.

It exits when:
- the scan finishes;
- a write to the pipe fails (the app closed it, which is how the app cancels);
- the app's process exits (the helper waits on the app's process handle);
- anything in the startup handshake is wrong.

## Launch and connection
1. The app makes a pipe name `\\.\pipe\lustro-scan-<32 lowercase hex>`, from 16 bytes of
   the system CSPRNG (`BCryptGenRandom`), and creates the pipe:
   - `FILE_FLAG_FIRST_PIPE_INSTANCE`, at most 1 instance, `PIPE_REJECT_REMOTE_CLIENTS`, byte mode, overlapped.
   - DACL `D:P(A;;GA;;;<current user SID>)(A;;GA;;;BA)`: protected, so no inherited ACEs.
     - Only the current user, or an *elevated* administrator, may connect.
     - The `BA` entry covers over-the-shoulder UAC, where a standard user types an admin's password. Unelevated tokens hold `BA` as deny-only, so this grants nothing extra to normal processes.
2. The app pins and hashes the helper exe (see Hardening), then runs `ShellExecuteExW` with the verb `runas` and the arguments
   `--pipe <name> --parent <app pid>`.
   - No paths go on the command line.
   - If the user declines UAC (`ERROR_CANCELLED`), the scan falls back to the walker.
3. The helper checks its arguments strictly: exactly those four, the exact name
   format, and a non-zero pid. It then:
   - opens a handle on the app process, for waiting only;
   - opens the pipe with `SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS`, so the unelevated server can neither impersonate nor identify the elevated helper;
   - checks `GetNamedPipeServerProcessId` equals `--parent`.
4. The app waits up to 30 s to connect, giving up early if the helper exits. It then
   checks `GetNamedPipeClientProcessId` equals the pid of the process it launched.

If the app is already elevated, it skips all of this and reads the MFT in-process.

## Framing
Each message is one frame: `len: u32` followed by `len` bytes, which are `kind: u8`
and then the body.
- All integers are little-endian.
- `1 ≤ len ≤ 4 MiB`.
- The decoder rejects:
  - unknown kinds;
  - bad lengths;
  - unknown flag bits;
  - invalid booleans or enums;
  - invalid UTF-8;
  - truncated or trailing bytes.

Types:
- `wide`: `u16` count (at most 32,767), then that many UTF-16 code units. Unpaired surrogates are allowed, because NTFS names may contain them.
- `text`: `u16` byte count (at most 1,024), then UTF-8.

## Messages

| kind | direction | name | body |
|---|---|---|---|
| 1 | helper → app | Hello | `"SVSH"`, `version: u16` (=1), `pid: u32` (the helper's own) |
| 0x81 | app → helper | Request | `version: u16`, `root: wide` (a local drive-letter path `X:\...`, see "Roots"; at most 128 KiB frame) |
| 2 | helper → app | Records | `count: u16` (1–4096), then `count` × Record |
| 3 | helper → app | Warning | `path: wide`, `message: text` |
| 4 | helper → app | Ping | (empty) |
| 5 | helper → app | Done | `cancelled: u8` (0/1) |
| 6 | helper → app | Unavailable | `reason: text` (fixed strings, no paths) |
| 7 | helper → app | Failed | `message: text` (no paths) |

Record fields, in order:

| field | type | notes |
|---|---|---|
| id | u32 | Unique, below 2^27. The root is 0. |
| parent | u32 | `0xFFFFFFFF` means none (root only). |
| kind | u8 | 0 File, 1 Dir, 2 Link, 3 Other |
| flags | u32 | `EntryFlags` bits |
| win_attributes | u32 | `FILE_ATTRIBUTE_*` bits, used by the app's `CloudClassifier` |
| logical_size | u64 | |
| allocated_size | u64 | |
| modified | i64 | Unix seconds; `i64::MIN` means none |
| accessed | i64 | Unix seconds; `i64::MIN` means none |
| has_file_id | u8 | 0 or 1 |
| volume | u64 | Present only if `has_file_id` is 1 |
| index | u64 | Present only if `has_file_id` is 1 |
| link_count | u32 | |
| name | wide | Empty for the root. The app names the root itself. |

The record is about 70 bytes plus the name, so a full 4,096-record batch of
255-character names fits well inside 4 MiB.

## Sequence
```
helper → Hello
app    → Request
helper → Ping*            (every 250 ms from here on, while reading the MFT and streaming)
helper → Records+         (root first; then one folder's children at a time, parents before children)
         Warning*
helper → Done | Unavailable | Failed
```
- `Unavailable` comes before any `Records`, for example "not NTFS" or "needs administrator". The app then falls back to the walker.
- After `Records` have started, a failure is a scan error.

## The app treats the pipe as untrusted
All of this is checked in the app, which never panics on bad input:
- **Frames:** each frame's length is checked before anything is allocated, and only one frame (at most 4 MiB) is held at a time.
- **Timeouts:** no data for 10 s (pings stop) means the helper is stuck. Every wait also polls the app's `CancelToken`.
- **Hello:** the magic, version and `pid` must match the launched process.
- **Record stream** (`prune.rs`):
  - exactly one root, first, with id 0, and it must be a Dir;
  - every other record's parent must be a Dir already seen;
  - ids are unique and below 2^27, tracked in a bitset of 16 MiB at most;
  - names are 1–255 UTF-16 units with no `\`, `/`, `:` or NUL, and are not `.` or `..`. A name can never escape its parent folder.
  - folders deeper than 1,024 levels, or whose path exceeds 32,767 UTF-16 units, are
    recorded with `NOT_DESCENDED` and their subtrees dropped. The walker applies the
    same limits.
  - folders are remembered as (parent, name), not full paths, so memory grows with the
    input rather than with depth × width.
- **Classifier:** the app's `CloudClassifier` and exclusions run on paths the app builds from validated names. Folders not entered are dropped together with everything under them.
- **Fields:** `cloud` is always set by the app, never taken from the helper. `link_count` is clamped to at least 1.

Any violation ends the scan with an error, or with a fallback if no records have been accepted yet.

## Cancelling
The app closes the pipe. Within about 250 ms the helper's next ping fails; it cancels
the MFT read or walk and exits. The app waits up to 2 s for the process to go. The
app cannot kill the elevated helper itself, and doesn't need to.

## Roots the helper accepts
The helper runs elevated on a root supplied over the pipe. Any same-user process
can launch the genuine helper itself, and then it really is the parent, so the PID
checks pass. The root is therefore restricted *before anything is opened*:
- **Path form** (`windows_mft::local_drive_path`, checked in both `child.rs` and
  `MftScanner`):
  - only `X:\...` or `\\?\X:\...` on a drive letter;
  - every part must be a plain name: no `.`, `..` or separators;
  - UNC (`\\server\share`, `\\?\UNC\...`), device (`\\.\...`), volume-GUID and
    `GLOBALROOT` paths are refused with `Unavailable`. An elevated open of a UNC path
    would send the administrator's credentials to that server (an NTLM leak and relay).
- **Drive type:** `GetDriveTypeW("X:\")` must be FIXED or REMOVABLE, and the file
  system must be NTFS, before any file is opened.
- **No links:** the root is opened from the drive root down, one folder at a time
  with `NtCreateFile` relative to the previous handle and `FILE_OPEN_REPARSE_POINT`.
  Any symlink, junction or mount point on the way is refused, so the path can't be
  redirected to another volume or a network share.
- **Same volume:** the volume read must be the root's volume. The boot sector's
  serial must match the root handle's, and the root's MFT record must be an in-use
  folder with the same sequence number (S5).

Tests:
- `child::tests::roots_must_be_plain_absolute_paths`;
- `windows_mft::tests::only_local_drive_paths_are_accepted`;
- `unc_roots_are_refused_before_any_open`.

## What the helper reveals
For one local NTFS folder tree, it reveals names, sizes, timestamps, attributes and
file ids. That includes folders whose ACLs would deny the user, for example other
users' profiles and `System Volume Information`, because reading the MFT bypasses
ACLs. File contents are never read. A same-user process that launches the helper and
gets the UAC prompt approved gets that listing. This is the inherent price of an
MFT-based fast scan, and the reason it sits behind UAC.

## Threat model
| Actor | Can | Defence |
|---|---|---|
| Remote attacker | Nothing: the pipe rejects remote clients, and the helper never opens UNC paths | `PIPE_REJECT_REMOTE_CLIENTS`; root restrictions |
| Other local users | Can't connect to the pipe | DACL: current user and elevated admins only |
| Same-user process, no UAC approval | Can't elevate anything; can feed a fake helper's records to the app | Elevation requires the UAC prompt. The app validates every record (`prune.rs`) |
| Same-user process + UAC approved | Gets the MFT listing above; can't make the helper read data, write, delete, or touch the network | The helper has one command; roots are restricted |
| Same-user malware with write access to the install folder | Can replace **both** `lustro.exe` and the helper, or plant files next to them | DLL planting is blocked (below). Binary swap is caught *if* `lustro.exe` itself is intact (below). See residual risk |
| Crafted NTFS media (USB stick) | Feeds hostile MFT bytes to elevated code | Checked parsing and bounded memory and walk (below) |

**Residual risk:** with a per-user install, everything in the install folder is
writable by same-user code. That code can replace both executables, so the UAC
prompt would approve whatever it put there. UAC is not a boundary against
already-running same-user malware in this setup; such malware could equally just
ask for elevation itself. The real fixes are future items, not part of this crate:
- **code signing**, so the UAC prompt shows a verified publisher and a swapped
  binary shows "Unknown publisher";
- an optional **per-machine install** (Program Files, admin-writable only) for the
  helper.

## Hardening
The app installs per user, so the helper's folder may be writable by the user, and
by any malware running as the user. These defences stop the helper's own folder
from being used to run code as admin, and stop hostile disks from crashing it.

### 1. DLL planting and side-by-side redirection
An elevated helper must never load code from its own folder. A planted
`version.dll`, a `lustro-scan-helper.exe.local\` folder or a `lustro-scan-helper.exe.manifest`
next to it would otherwise run as admin. `lustro-scan-helper/build.rs` sets four things
for the MSVC target:
- **Static imports:** `/DEPENDENTLOADFLAG:0x800` (`LOAD_LIBRARY_SEARCH_SYSTEM32`), so
  the loader resolves the helper's imports from System32 only.
- **Embedded manifest** (`lustro-scan-helper.manifest`, via `/MANIFEST:EMBED`):
  `requireAdministrator`, Windows 10/11 compatibility, and no `<dependency>`
  elements. When a manifest is embedded, Windows ignores `.local` redirection and
  external `.exe.manifest` files.
- **Hybrid CRT:** the VC runtime is linked statically, so there is no
  `VCRUNTIME140.dll`, which isn't a system DLL and could be found next to the exe.
  The Universal CRT stays dynamic through the `api-ms-win-crt-*` API sets, which the
  loader resolves inside System32.
- **Run time:** `main` calls `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`
  before anything else (`lustro_scan::helper::harden_dll_search`). If that call fails, the
  helper exits.

**Check:** `tests/pe_hardening.rs` parses every helper exe it finds (this build's, the
other `target/` profile's, any sidecar in `app/src-tauri/binaries/`, and
`LUSTRO_HELPER_EXE`) and asserts all of this:
- `DependentLoadFlags == 0x800`;
- an embedded `RT_MANIFEST` with `requireAdministrator` and no dependencies;
- every import (normal and delay-load) is an API set or a DLL present in System32,
  with no `VCRUNTIME*` or `MSVCP*`.

The release helper imports only `kernel32`, `ntdll`, `api-ms-win-core-synch-*` and
`api-ms-win-crt-*`. `kernel32` is a KnownDLL, `ntdll` is always mapped from System32,
and API sets aren't searched for at all.

Release builds use the MSVC toolchain. GNU targets get none of the link-time
measures, only the run-time call.

### 2. Crafted NTFS media
The helper parses MFT bytes from whatever volume the root is on, which may be a
crafted USB stick. So:
- **Parsing (`parse.rs`):** every field read is bounds-checked, with offsets added
  using checked arithmetic. Shifts are checked; the cluster size must be a power of
  two up to 2 MiB; the record size must be a power of two from 1 to 64 KiB. Each
  file keeps at most 1,024 names.
- **Reading:** the table size is capped by its data-run extents and by 2^27 records,
  and memory grows only with records actually read. Run start and length arithmetic
  is checked.
- **Walking:** each folder record is entered at most once, through the first name
  met. A folder that is its own ancestor, or a folder with many names, can't loop or
  multiply the output.
- **Tests:** fuzz-style mutation of records, boot sectors and run lists (no panics),
  plus cycle and many-names walk tests.

### 3. Binary swap
Swapping `lustro-scan-helper.exe` for another program would let that program be the one
the user approves at the UAC prompt. So the app only launches a helper whose bytes
match a SHA-256 baked into the app at build time:
- `SystemScanner.elevation_helper` / `HelperScanner::new` take a `HelperExe { path, sha256 }`.
- Right before `ShellExecuteExW`, the app:
  1. opens the helper with **`FILE_SHARE_READ` only**, so nobody can write, rename or delete it while the handle is open;
  2. hashes the bytes through that same handle;
  3. keeps the handle open across the launch and until the scan ends. Starting the
     process and showing the UAC prompt only need read access, so they still work.
- **On a mismatch:**
  - the helper is not launched;
  - the sink gets the warning "Fast scan unavailable: helper failed integrity check" (`helper::INTEGRITY_WARNING`, with an empty path);
  - `try_scan` returns `Unavailable`, so `SystemScanner` falls back to Standard.
- A malformed or missing hash (`HelperExe::new` returns `None`) means no helper, so
  Fast works only if the app is already elevated.

**How the hash gets in:**
1. The build builds the helper first (`cargo build -p lustro-scan-helper --release`).
2. `app/src-tauri/build.rs` hashes the built exe with `lustro_scan::sha256_file` (a
   build-dependency; pure Rust, so it works on any OS) and emits
   `cargo:rustc-env=LUSTRO_HELPER_SHA256=<hex>`, plus `rerun-if-changed` on the exe.
   If the exe isn't there, it emits an empty value.
3. The app constructs `HelperExe::new(path, env!("LUSTRO_HELPER_SHA256"))`.

The exe is hashed after linking and before bundling. Code signing (deferred) would
change the bytes, so if signing is added later, hash the *signed* exe. Embedding the
helper in the app and extracting it at run time was rejected: the extracted copy
would land in the same kind of user-writable folder.

Tests: `tampered_helper_is_refused` (wrong hash: warning, then a walker fallback) and
`malformed_hashes_are_rejected` in `tests/e2e.rs`.

The pin protects against swapping the helper alone. The expected hash lives in
`lustro.exe` in the same folder, so it doesn't protect against replacing both (see
residual risk).

## Privacy note
The names the helper reveals (see "What the helper reveals") reach only the app's
own process and are shown only to the person who approved the UAC prompt. Nothing
is logged or sent anywhere.

## Testing without UAC
Because of the manifest, starting the helper normally always prompts for elevation.
`HelperScanner::no_uac_for_tests` (tests only) instead starts it with `CreateProcess`
and the documented `__COMPAT_LAYER=RunAsInvoker` compatibility layer. It then runs
unelevated, completes the handshake, and reports `Unavailable` because it can't open
the volume. The helper binary's own unit-test harness is disabled (`test = false`),
since it would inherit the manifest.

## Versioning
`version` is checked in both directions, and the two sides must match exactly. The
helper ships next to the app, so they are always built together. Any change to the
format bumps `VERSION` in `protocol.rs` and this document.
