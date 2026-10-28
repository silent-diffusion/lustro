//! Per-OS discovery of sync roots, provider caches and mounts. Runs once per scan.
//!
//! Reads only registry values, the names of entries in known folders, small client config
//! files (`info.json`) and the mount table. Never lists or opens anything inside a sync root.

use std::path::{Path, PathBuf};

use lustro_core::model::CloudProvider;

use crate::mounts::MountEntry;
use crate::{paths, provider_name, SyncRoot, SyncRootKind};

pub(crate) fn sync_roots() -> Vec<SyncRoot> {
    let mut out = Vec::new();
    os::sync_roots(&mut out);
    // Classic Dropbox (full local copies) on every OS, unless already registered.
    for p in dropbox_info_paths() {
        add(
            &mut out,
            CloudProvider::Dropbox,
            None,
            p,
            SyncRootKind::FullCopy,
        );
    }
    if let Some(cache) = google_drive_cache() {
        add(
            &mut out,
            CloudProvider::GoogleDrive,
            Some("Google Drive (cache)".into()),
            cache,
            SyncRootKind::Cache,
        );
    }
    out
}

pub(crate) fn google_drive_cache() -> Option<PathBuf> {
    os::google_drive_cache()
}

pub(crate) fn mounts() -> Vec<MountEntry> {
    os::mounts()
}

/// Adds a root unless one with the same path is already present (first one wins, so
/// registered sync roots take precedence over config-file guesses).
fn add(
    out: &mut Vec<SyncRoot>,
    provider: CloudProvider,
    display_name: Option<String>,
    root_path: PathBuf,
    kind: SyncRootKind,
) {
    let ci = crate::rules::Os::current().case_insensitive();
    let key = paths::key(&root_path.to_string_lossy(), ci);
    if out
        .iter()
        .any(|r| paths::key(&r.root_path.to_string_lossy(), ci) == key)
    {
        return;
    }
    out.push(SyncRoot {
        provider,
        display_name: display_name.unwrap_or_else(|| provider_name(provider).to_string()),
        root_path,
        kind,
    });
}

/// Dropbox's `info.json` lists each account's folder: `{"personal": {"path": "…"}, …}`.
fn dropbox_info_paths() -> Vec<PathBuf> {
    os::dropbox_info_files()
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .flat_map(|text| parse_dropbox_info(&text))
        .collect()
}

pub(crate) fn parse_dropbox_info(text: &str) -> Vec<PathBuf> {
    let Ok(serde_json::Value::Object(accounts)) = serde_json::from_str(text) else {
        return Vec::new();
    };
    accounts
        .values()
        .filter_map(|a| a.get("path")?.as_str())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[allow(dead_code)]
fn existing_dir(p: PathBuf) -> Option<PathBuf> {
    p.is_dir().then_some(p)
}

#[allow(dead_code)]
fn is_dir(p: &Path) -> bool {
    p.is_dir()
}

#[cfg(windows)]
mod os {
    //! Windows: `StorageProviderSyncRootManager` registrations, Google Drive's virtual drive
    //! and cache, and mapped network drives.

    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        SetThreadErrorMode, SEM_FAILCRITICALERRORS, THREAD_ERROR_MODE,
    };
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    const SYNC_ROOT_MANAGER: &str =
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager";
    const DRIVE_REMOTE: u32 = 4;

    pub(super) fn sync_roots(out: &mut Vec<SyncRoot>) {
        registered_sync_roots(out);
        for drive in google_drive_letters() {
            add(
                out,
                CloudProvider::GoogleDrive,
                Some("Google Drive".into()),
                PathBuf::from(drive),
                SyncRootKind::VirtualDrive,
            );
        }
    }

    /// Every provider registered with the Cloud Files API, for every user on this machine
    /// (classification is attribute-based, so other users' roots are valid too).
    fn registered_sync_roots(out: &mut Vec<SyncRoot>) {
        let Ok(manager) = LOCAL_MACHINE.open(SYNC_ROOT_MANAGER) else {
            return;
        };
        let Ok(ids) = manager.keys() else { return };
        for id in ids {
            let Ok(entry) = manager.open(&id) else {
                continue;
            };
            let provider = crate::provider_from_hint(&id);
            // Indirect resources ("@dll,-id") would need loading a third-party DLL to resolve;
            // fall back to the provider's name instead.
            let display = entry
                .get_string("DisplayNameResource")
                .ok()
                .filter(|s| !s.is_empty() && !s.starts_with('@'));
            let Ok(users) = entry.open("UserSyncRoots") else {
                continue;
            };
            let Ok(values) = users.values() else { continue };
            for (_sid, value) in values {
                let Ok(path) = String::try_from(value) else {
                    continue;
                };
                if path.is_empty() {
                    continue;
                }
                add(
                    out,
                    provider,
                    display.clone(),
                    PathBuf::from(path),
                    SyncRootKind::Placeholder,
                );
            }
        }
    }

    /// Google Drive for desktop's own settings (user, then machine policy).
    fn drivefs_setting(name: &str) -> Option<String> {
        [
            (CURRENT_USER, r"Software\Google\DriveFS"),
            (LOCAL_MACHINE, r"SOFTWARE\Policies\Google\DriveFS"),
            (LOCAL_MACHINE, r"SOFTWARE\Google\DriveFS"),
        ]
        .into_iter()
        .find_map(|(hive, key)| hive.open(key).ok()?.get_string(name).ok())
        .filter(|s| !s.trim().is_empty())
    }

    pub(super) fn google_drive_cache() -> Option<PathBuf> {
        if let Some(p) = drivefs_setting("ContentCachePath") {
            return existing_dir(PathBuf::from(p));
        }
        existing_dir(env_path("LOCALAPPDATA")?.join("Google").join("DriveFS"))
    }

    /// Drive letters of the Stream-mode virtual drive: the configured mount point if the
    /// client stores one, plus any volume labelled "Google Drive".
    fn google_drive_letters() -> Vec<String> {
        let mut out = Vec::new();
        if let Some(mp) = drivefs_setting("DefaultMountPoint") {
            let letter = mp.trim().trim_end_matches(['\\', ':']);
            if letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()) {
                let root = format!(r"{}:\", letter.to_ascii_uppercase());
                // Only if the drive is actually mounted right now.
                if Path::new(&root).exists() {
                    out.push(root);
                }
            }
        }
        for root in drive_roots() {
            if out.iter().any(|r| r.eq_ignore_ascii_case(&root)) {
                continue;
            }
            if let Some((label, fs)) = volume_label_and_fs(&root) {
                let label = label.to_ascii_lowercase();
                let fs = fs.to_ascii_lowercase();
                if label == "google drive" || fs.contains("drivefs") {
                    out.push(root);
                }
            }
        }
        out
    }

    /// Mapped network drives (SMB etc.): remote, not descended by default.
    pub(super) fn mounts() -> Vec<MountEntry> {
        drive_roots()
            .into_iter()
            .filter(|root| drive_type(root) == DRIVE_REMOTE)
            .map(|root| MountEntry {
                mount_point: root,
                fs_type: "cifs".into(),
                source: String::new(),
            })
            .collect()
    }

    pub(super) fn dropbox_info_files() -> Vec<PathBuf> {
        ["APPDATA", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| env_path(v))
            .map(|p| p.join("Dropbox").join("info.json"))
            .collect()
    }

    /// `"C:\"`, `"D:\"`, … for every logical drive.
    #[allow(unsafe_code)] // small FFI call, see SAFETY comments
    fn drive_roots() -> Vec<String> {
        let mut buf = vec![0u16; 512];
        // SAFETY: the buffer is valid for its whole length; the API writes at most that many
        // u16s and returns the count written (excluding the final NUL).
        let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
        if n == 0 || n > buf.len() {
            return Vec::new();
        }
        buf[..n]
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[allow(unsafe_code)] // small FFI call, see SAFETY comments
    fn drive_type(root: &str) -> u32 {
        let w = wide(root);
        // SAFETY: `w` is a NUL-terminated UTF-16 string that outlives the call.
        unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) }
    }

    /// Volume label and filesystem name. Suppresses "insert a disk" dialogs for empty drives.
    #[allow(unsafe_code)] // small FFI call, see SAFETY comments
    fn volume_label_and_fs(root: &str) -> Option<(String, String)> {
        let w = wide(root);
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        let mut old = THREAD_ERROR_MODE(0);
        // SAFETY: plain FFI calls with valid, NUL-terminated input and caller-owned output
        // buffers whose lengths are passed via the slices. The previous error mode is restored.
        let ok = unsafe {
            let _ = SetThreadErrorMode(SEM_FAILCRITICALERRORS, Some(&mut old));
            let r = GetVolumeInformationW(
                PCWSTR(w.as_ptr()),
                Some(&mut label),
                None,
                None,
                None,
                Some(&mut fs),
            );
            let _ = SetThreadErrorMode(old, None);
            r.is_ok()
        };
        ok.then(|| (from_wide(&label), from_wide(&fs)))
    }

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }
}

#[cfg(target_os = "macos")]
mod os {
    //! macOS: File Provider roots under `~/Library/CloudStorage`, iCloud Drive under
    //! `~/Library/Mobile Documents`, Google DriveFS cache, and network/FUSE mounts.

    use super::*;

    fn home() -> Option<PathBuf> {
        env_path("HOME")
    }

    pub(super) fn sync_roots(out: &mut Vec<SyncRoot>) {
        let Some(home) = home() else { return };
        let lib = home.join("Library");
        // Only the names of the provider folders are read; their contents are never listed.
        if let Ok(entries) = std::fs::read_dir(lib.join("CloudStorage")) {
            for e in entries.flatten() {
                if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let provider = crate::provider_from_hint(&name);
                add(
                    out,
                    provider,
                    Some(display_from_folder(&name, provider)),
                    e.path(),
                    SyncRootKind::Placeholder,
                );
            }
        }
        let icloud = lib.join("Mobile Documents");
        if is_dir(&icloud) {
            add(
                out,
                CloudProvider::ICloud,
                None,
                icloud,
                SyncRootKind::Placeholder,
            );
        }
    }

    /// "OneDrive-Personal" → "OneDrive - Personal". Account emails are dropped.
    fn display_from_folder(name: &str, provider: CloudProvider) -> String {
        match name.split_once('-') {
            Some((_, acct)) if !acct.contains('@') && !acct.is_empty() => {
                format!("{} - {}", provider_name(provider), acct)
            }
            _ => provider_name(provider).to_string(),
        }
    }

    pub(super) fn google_drive_cache() -> Option<PathBuf> {
        existing_dir(
            home()?
                .join("Library")
                .join("Application Support")
                .join("Google")
                .join("DriveFS"),
        )
    }

    pub(super) fn dropbox_info_files() -> Vec<PathBuf> {
        home()
            .map(|h| vec![h.join(".dropbox").join("info.json")])
            .unwrap_or_default()
    }

    /// Uses `getfsstat` with a buffer we own. (`getmntinfo` returns a buffer shared by
    /// the whole process that the next call on any thread can free, so it isn't safe here.)
    #[allow(unsafe_code)] // small FFI calls, see SAFETY comments
    pub(super) fn mounts() -> Vec<MountEntry> {
        // SAFETY: a null buffer with size 0 only asks for the number of mounts.
        let n = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
        if n <= 0 {
            return Vec::new();
        }
        // Room for a few mounts added between the two calls.
        let cap = n as usize + 8;
        let mut buf: Vec<libc::statfs> = Vec::with_capacity(cap);
        let Ok(bytes) = libc::c_int::try_from(cap * std::mem::size_of::<libc::statfs>()) else {
            return Vec::new();
        };
        // SAFETY: `buf` has room for `cap` structs and `bytes` says exactly that, so
        // getfsstat writes at most `cap` of them. MNT_NOWAIT avoids blocking on
        // unresponsive mounts.
        let got = unsafe { libc::getfsstat(buf.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
        if got <= 0 {
            return Vec::new();
        }
        // SAFETY: getfsstat initialised the first `got` structs, and `got` <= `cap`.
        unsafe { buf.set_len((got as usize).min(cap)) };
        buf.iter()
            .map(|s| MountEntry {
                mount_point: c_chars(&s.f_mntonname),
                fs_type: c_chars(&s.f_fstypename),
                source: c_chars(&s.f_mntfromname),
            })
            .collect()
    }

    fn c_chars(buf: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = buf
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    //! Linux and other Unix: no placeholder API. Cloud drives are FUSE/rclone mounts (remote,
    //! not descended); classic Dropbox keeps full local copies.

    use super::*;
    use crate::mounts::{is_cloud_mount, parse_mountinfo};

    pub(super) fn sync_roots(out: &mut Vec<SyncRoot>) {
        for m in mounts() {
            if crate::mounts::fs_skip_reason(&m.fs_type)
                == Some(lustro_core::scan::SkipReason::Remote)
                && is_cloud_mount(&m)
            {
                let provider = crate::provider_from_hint(&m.source);
                add(
                    out,
                    provider,
                    None,
                    PathBuf::from(&m.mount_point),
                    SyncRootKind::RemoteMount,
                );
            }
        }
    }

    pub(super) fn google_drive_cache() -> Option<PathBuf> {
        None
    }

    pub(super) fn dropbox_info_files() -> Vec<PathBuf> {
        env_path("HOME")
            .map(|h| vec![h.join(".dropbox").join("info.json")])
            .unwrap_or_default()
    }

    pub(super) fn mounts() -> Vec<MountEntry> {
        std::fs::read_to_string("/proc/self/mountinfo")
            .map(|t| parse_mountinfo(&t))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropbox_info_json() {
        let text = r#"{"personal": {"path": "C:\\Users\\William\\Dropbox", "host": 1},
                       "business": {"path": "C:\\Users\\William\\Dropbox (Contoso)"}}"#;
        let mut p = parse_dropbox_info(text);
        p.sort();
        assert_eq!(
            p,
            vec![
                PathBuf::from(r"C:\Users\William\Dropbox"),
                PathBuf::from(r"C:\Users\William\Dropbox (Contoso)"),
            ]
        );
        assert!(parse_dropbox_info("not json").is_empty());
        assert!(parse_dropbox_info(r#"{"personal": {}}"#).is_empty());
    }

    #[test]
    fn add_dedupes_by_path() {
        let mut out = Vec::new();
        add(
            &mut out,
            CloudProvider::Dropbox,
            None,
            PathBuf::from("/home/william/Dropbox"),
            SyncRootKind::Placeholder,
        );
        add(
            &mut out,
            CloudProvider::Dropbox,
            None,
            PathBuf::from("/home/william/Dropbox/"),
            SyncRootKind::FullCopy,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, SyncRootKind::Placeholder);
        assert_eq!(out[0].display_name, "Dropbox");
    }

    /// Runs discovery on this machine. Prints only provider names and kinds, never paths.
    #[test]
    fn discovery_runs_without_panicking() {
        for r in crate::list_sync_roots() {
            println!("found {:?} ({:?})", r.provider, r.kind);
        }
        let _ = mounts();
    }
}
