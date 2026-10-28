//! "Where did my space go?": system-managed items per OS with sizes and plain-language
//! explanations, plus [`open_setting`] to jump to the matching OS settings page.
//!
//! Sizes come from file metadata or APIs that don't need admin. When that isn't possible the
//! size is `None` ("unknown") rather than a guess.

use std::path::{Path, PathBuf};

use lustro_cloud::rules::Os;
use serde::Serialize;

/// One system item. Maps onto `SpaceSegment` in `app/ui/src/ipc/types.ts` with
/// `kind: "system"`; `bytes: None` means unknown (the UI shows "unknown" / skips it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSpaceItem {
    pub kind: &'static str,
    /// System item id, e.g. "pagefile", "hibernation", "restorePoints", "winsxs".
    pub key: &'static str,
    pub label: &'static str,
    pub bytes: Option<u64>,
    pub explanation: &'static str,
    /// Id for [`open_setting`], if there's a settings page for it.
    pub setting_action: Option<&'static str>,
}

fn item(
    key: &'static str,
    label: &'static str,
    bytes: Option<u64>,
    explanation: &'static str,
    setting_action: Option<&'static str>,
) -> SystemSpaceItem {
    SystemSpaceItem {
        kind: "system",
        key,
        label,
        bytes,
        explanation,
        setting_action,
    }
}

/// System items on the volume mounted at `volume_root` (e.g. `C:\`, `/`). Items that don't
/// exist on that volume are left out. Reads metadata only.
pub fn system_items(volume_root: &Path) -> Vec<SystemSpaceItem> {
    os::system_items(volume_root)
}

#[derive(Debug, thiserror::Error)]
pub enum SettingError {
    #[error("unknown setting action")]
    Unknown,
    #[error("couldn't open the settings page")]
    Launch,
}

/// Every action id this OS supports, for the app's IPC validation.
pub fn setting_actions() -> Vec<&'static str> {
    SETTINGS
        .iter()
        .filter(|s| s.os == CURRENT)
        .map(|s| s.id)
        .collect()
}

/// Opens the OS settings page for `action` (an id from [`SystemSpaceItem::setting_action`]).
/// Only ids from the fixed table below are accepted; nothing from the caller is passed to the
/// OS, and web URLs are never opened.
pub fn open_setting(action: &str) -> Result<(), SettingError> {
    let s = SETTINGS
        .iter()
        .find(|s| s.id == action && s.os == CURRENT)
        .ok_or(SettingError::Unknown)?;
    let mut cmd = std::process::Command::new(program(s.program));
    cmd.args(s.args);
    cmd.spawn().map(|_| ()).map_err(|_| SettingError::Launch)
}

const CURRENT: Os = Os::current();

struct Setting {
    id: &'static str,
    os: Os,
    /// `Win(exe)` is resolved inside `%SystemRoot%`; `Abs` is an absolute path.
    program: Program,
    args: &'static [&'static str],
}

#[derive(Clone, Copy)]
enum Program {
    Win(&'static str),
    #[allow(dead_code)]
    Abs(&'static str),
}

const SETTINGS: &[Setting] = &[
    Setting {
        id: "windows:virtualMemory",
        os: Os::Windows,
        program: Program::Win(r"System32\SystemPropertiesPerformance.exe"),
        args: &[],
    },
    Setting {
        id: "windows:systemProtection",
        os: Os::Windows,
        program: Program::Win(r"System32\SystemPropertiesProtection.exe"),
        args: &[],
    },
    Setting {
        id: "windows:power",
        os: Os::Windows,
        program: Program::Win("explorer.exe"),
        args: &["ms-settings:powersleep"],
    },
    Setting {
        id: "windows:diskCleanup",
        os: Os::Windows,
        program: Program::Win("explorer.exe"),
        args: &["ms-settings:storagesense"],
    },
    Setting {
        id: "windows:storage",
        os: Os::Windows,
        program: Program::Win("explorer.exe"),
        args: &["ms-settings:storagesense"],
    },
    Setting {
        id: "windows:recycleBin",
        os: Os::Windows,
        program: Program::Win("explorer.exe"),
        args: &["shell:RecycleBinFolder"],
    },
    Setting {
        id: "macos:storage",
        os: Os::MacOs,
        program: Program::Abs("/usr/bin/open"),
        args: &["x-apple.systempreferences:com.apple.settings.Storage"],
    },
    Setting {
        id: "macos:timeMachine",
        os: Os::MacOs,
        program: Program::Abs("/usr/bin/open"),
        args: &["x-apple.systempreferences:com.apple.Time-Machine-Settings.extension"],
    },
];

fn program(p: Program) -> PathBuf {
    match p {
        Program::Win(exe) => windows_exe(exe),
        Program::Abs(p) => PathBuf::from(p),
    }
}

/// `%SystemRoot%\<rel>`, so a program of the same name elsewhere on PATH is never used.
pub(crate) fn windows_exe(rel: &str) -> PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(rel)
}

/// Size of one file from metadata, without following links. `None` if it doesn't exist or
/// can't be read. (Unused on Linux, which reports swap from `/proc/swaps`.)
#[allow(dead_code)]
fn file_size(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| allocated(&m))
}

#[cfg(unix)]
fn allocated(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.blocks() * 512
}
#[cfg(not(unix))]
fn allocated(m: &std::fs::Metadata) -> u64 {
    m.len()
}

/// Total size under `dir` from metadata, never following links. Stops after `max_entries`.
/// `None` when `dir` itself can't be listed (missing or permission denied).
#[allow(dead_code)]
pub(crate) fn dir_size(dir: &Path, max_entries: usize) -> Option<u64> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut seen = 0;
    Some(dir_size_inner(rd, &mut seen, max_entries))
}

fn dir_size_inner(rd: std::fs::ReadDir, seen: &mut usize, max: usize) -> u64 {
    let mut total = 0;
    for e in rd.flatten() {
        *seen += 1;
        if *seen > max {
            return total;
        }
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if m.is_dir() {
            if let Ok(sub) = std::fs::read_dir(e.path()) {
                total += dir_size_inner(sub, seen, max);
            }
        } else if m.is_file() {
            total += allocated(&m);
        }
    }
    total
}

#[cfg(windows)]
mod os {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    pub(super) fn system_items(root: &Path) -> Vec<SystemSpaceItem> {
        let mut out = Vec::new();
        let page = [
            file_size(&root.join("pagefile.sys")),
            file_size(&root.join("swapfile.sys")),
        ];
        if page.iter().any(Option::is_some) {
            out.push(item(
                "pagefile",
                "Page file",
                Some(page.iter().flatten().sum()),
                "Virtual memory: Windows uses this file as overflow RAM. Its size is managed by Windows.",
                Some("windows:virtualMemory"),
            ));
        }
        if let Some(b) = file_size(&root.join("hiberfil.sys")) {
            out.push(item(
                "hibernation",
                "Hibernation file",
                Some(b),
                "Stores the contents of RAM for hibernate and Fast Startup. Turning hibernation off removes it.",
                Some("windows:power"),
            ));
        }
        if root.join("System Volume Information").exists() {
            out.push(item(
                "restorePoints",
                "Restore points and shadow copies",
                // Reading shadow-copy storage needs admin; reported as unknown.
                None,
                "System restore points and previous versions of files. Limit their space in System Protection.",
                Some("windows:systemProtection"),
            ));
        }
        if root.join(r"Windows\WinSxS").is_dir() {
            out.push(item(
                "winsxs",
                "Component store (WinSxS)",
                // Most of WinSxS is hard links shared with System32; its real cost comes from
                // the scan tree (hard links counted once), not from a separate walk here.
                None,
                "Copies of system components kept for updates and repairs. Much of it is shared with Windows itself. Clean it up with Storage settings.",
                Some("windows:diskCleanup"),
            ));
        }
        if let Some(b) = recycle_bin_bytes(root) {
            out.push(item(
                "recycleBin",
                "Recycle Bin",
                Some(b),
                "Deleted files waiting in the Recycle Bin. They still use space until you empty it.",
                Some("windows:recycleBin"),
            ));
        }
        out
    }

    #[allow(unsafe_code)]
    fn recycle_bin_bytes(root: &Path) -> Option<u64> {
        let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `wide` is NUL-terminated and outlives the call; `info` is a properly sized,
        // writable struct with cbSize set as the API requires.
        unsafe { SHQueryRecycleBinW(PCWSTR(wide.as_ptr()), &mut info) }.ok()?;
        u64::try_from(info.i64Size).ok()
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::*;

    pub(super) fn system_items(root: &Path) -> Vec<SystemSpaceItem> {
        let mut out = Vec::new();
        if root != Path::new("/") {
            return out;
        }
        let vm = Path::new("/private/var/vm");
        let swap: u64 = std::fs::read_dir(vm)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("swapfile"))
                    .filter_map(|e| file_size(&e.path()))
                    .sum()
            })
            .unwrap_or(0);
        out.push(item(
            "swap",
            "Swap files",
            Some(swap),
            "Virtual memory: macOS moves memory here when RAM is full. It shrinks after a restart.",
            None,
        ));
        if let Some(b) = file_size(&vm.join("sleepimage")) {
            out.push(item(
                "hibernation",
                "Sleep image",
                Some(b),
                "Stores the contents of RAM so your Mac can resume after deep sleep.",
                None,
            ));
        }
        out.push(item(
            "apfsSnapshots",
            "Local snapshots",
            None,
            "Time Machine and system update snapshots. macOS deletes them automatically when space is needed.",
            Some("macos:timeMachine"),
        ));
        out.push(item(
            "purgeable",
            "Purgeable",
            None,
            "Space macOS can reclaim automatically (caches, old snapshots, iCloud copies). Counted as used until macOS frees it.",
            Some("macos:storage"),
        ));
        out.push(item(
            "systemData",
            "System Data",
            None,
            "Caches, logs and other files macOS manages. Much of it is cleaned up automatically.",
            Some("macos:storage"),
        ));
        out
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    use super::*;

    const MAX_ENTRIES: usize = 200_000;

    pub(super) fn system_items(root: &Path) -> Vec<SystemSpaceItem> {
        let mut out = Vec::new();
        if root != Path::new("/") {
            return out;
        }
        if let Ok(text) = std::fs::read_to_string("/proc/swaps") {
            let b = parse_proc_swaps(&text);
            if b > 0 {
                out.push(item(
                    "swap",
                    "Swap files",
                    Some(b),
                    "Virtual memory on disk, used when RAM is full.",
                    None,
                ));
            }
        }
        let sum = |dirs: &[&str]| -> Option<u64> {
            let sizes: Vec<u64> = dirs
                .iter()
                .filter_map(|d| dir_size(Path::new(d), MAX_ENTRIES))
                .collect();
            (!sizes.is_empty()).then(|| sizes.iter().sum())
        };
        if Path::new("/var/log/journal").is_dir() {
            out.push(item(
                "journal",
                "System logs",
                dir_size(Path::new("/var/log/journal"), MAX_ENTRIES),
                "systemd journal logs. Their size can be limited with journald settings.",
                None,
            ));
        }
        let pkg = [
            "/var/cache/apt/archives",
            "/var/cache/dnf",
            "/var/cache/yum",
            "/var/cache/pacman/pkg",
            "/var/cache/zypp/packages",
        ];
        if pkg.iter().any(|d| Path::new(d).is_dir()) {
            out.push(item(
                "packageCache",
                "Package cache",
                sum(&pkg),
                "Downloaded package files kept by the package manager. Its clean command removes them safely.",
                None,
            ));
        }
        let sf = ["/var/lib/snapd/snaps", "/var/lib/flatpak"];
        if sf.iter().any(|d| Path::new(d).is_dir()) {
            out.push(item(
                "snapFlatpak",
                "Snap and Flatpak",
                sum(&sf),
                "Apps and runtimes installed with Snap or Flatpak, including older revisions.",
                None,
            ));
        }
        if Path::new("/var/lib/docker").is_dir() {
            out.push(item(
                "docker",
                "Docker",
                // Usually root-only; unknown when unreadable.
                dir_size(Path::new("/var/lib/docker"), MAX_ENTRIES),
                "Docker images, containers and volumes. Use Docker's prune commands to clean up.",
                None,
            ));
        }
        out
    }
}

/// Sum of file-backed swap sizes from `/proc/swaps` (KiB → bytes). Partitions are excluded:
/// they aren't files on the scanned filesystem.
#[allow(dead_code)]
pub(crate) fn parse_proc_swaps(text: &str) -> u64 {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let _name = f.next()?;
            let kind = f.next()?;
            let size: u64 = f.next()?.parse().ok()?;
            (kind == "file").then_some(size * 1024)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_swaps() {
        let t = "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n\
                 /swap.img                               file\t\t2097148\t\t0\t\t-2\n\
                 /dev/sda3                               partition\t8388604\t\t0\t\t-3\n";
        assert_eq!(parse_proc_swaps(t), 2097148 * 1024);
        assert_eq!(parse_proc_swaps(""), 0);
    }

    #[test]
    fn dir_size_counts_nested_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), vec![0u8; 1000]).unwrap();
        std::fs::create_dir_all(d.path().join("x/y")).unwrap();
        std::fs::write(d.path().join("x/y/b"), vec![0u8; 3000]).unwrap();
        let s = dir_size(d.path(), 1000).unwrap();
        if cfg!(unix) {
            assert!(s >= 4000);
        } else {
            assert_eq!(s, 4000);
        }
        assert_eq!(dir_size(&d.path().join("missing"), 10), None);
    }

    #[test]
    fn unknown_or_web_actions_are_refused() {
        for a in [
            "",
            "https://example.com",
            "ms-settings:storagesense",
            "windows:../../cmd",
            "file:///etc/passwd",
        ] {
            assert!(matches!(open_setting(a), Err(SettingError::Unknown)), "{a}");
        }
    }

    #[test]
    fn every_item_action_is_in_the_table() {
        let actions = setting_actions();
        for s in SETTINGS {
            for a in s.args {
                assert!(!a.starts_with("http"), "no web URLs");
            }
        }
        let root = if cfg!(windows) {
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\"
        } else {
            "/".into()
        };
        for it in system_items(Path::new(&root)) {
            assert_eq!(it.kind, "system");
            if let Some(a) = it.setting_action {
                assert!(actions.contains(&a), "{a}");
            }
            // Print key and size only; never paths.
            println!("{} = {:?}", it.key, it.bytes);
        }
    }

    #[test]
    fn mock_ui_action_ids_exist() {
        // Ids the UI mock already uses (app/ui/src/ipc/mockData.ts).
        let ids: Vec<_> = SETTINGS.iter().map(|s| s.id).collect();
        for id in [
            "windows:diskCleanup",
            "windows:virtualMemory",
            "windows:power",
            "windows:systemProtection",
            "macos:timeMachine",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
    }

    #[test]
    fn serialises_camel_case() {
        let j = serde_json::to_value(item(
            "pagefile",
            "Page file",
            None,
            "x",
            Some("windows:virtualMemory"),
        ))
        .unwrap();
        assert_eq!(j["kind"], "system");
        assert_eq!(j["settingAction"], "windows:virtualMemory");
        assert!(j["bytes"].is_null());
    }
}
