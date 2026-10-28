//! Privilege check and the volume list (`list_volumes`).

use serde::Serialize;

/// Mirrors `VolumeInfo` in `app/ui/src/ipc/types.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub id: String,
    /// "C:" / "Macintosh HD" / "/".
    pub label: String,
    /// Display only; never log it.
    pub mount_path: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub is_remote: bool,
    pub is_removable: bool,
}

/// True when the process runs elevated (Windows admin token) or as root.
pub fn is_elevated() -> bool {
    os::is_elevated()
}

/// Mounted volumes the user can scan. Remote includes network shares and virtual cloud
/// drives (e.g. Google Drive's `G:`).
pub fn list_volumes() -> Vec<VolumeInfo> {
    os::list_volumes()
}

#[cfg(windows)]
mod os {
    use super::*;
    use lustro_cloud::SyncRootKind;
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        SetThreadErrorMode, SEM_FAILCRITICALERRORS, THREAD_ERROR_MODE,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    #[allow(unsafe_code)]
    pub(super) fn is_elevated() -> bool {
        let mut token = HANDLE::default();
        // SAFETY: GetCurrentProcess returns a pseudo-handle; OpenProcessToken writes a real
        // handle into `token`, which is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        // SAFETY: `elevation` is a writable TOKEN_ELEVATION and its exact size is passed.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
        }
        .is_ok();
        // SAFETY: `token` is a valid handle opened above and closed exactly once.
        unsafe {
            let _ = CloseHandle(token);
        }
        ok && elevation.TokenIsElevated != 0
    }

    pub(super) fn list_volumes() -> Vec<VolumeInfo> {
        let virtual_drives: Vec<String> = lustro_cloud::list_sync_roots()
            .into_iter()
            .filter(|r| r.kind == SyncRootKind::VirtualDrive)
            .map(|r| r.root_path.to_string_lossy().to_ascii_uppercase())
            .collect();
        drive_roots()
            .into_iter()
            .filter_map(|root| {
                let kind = drive_type(&root);
                if !matches!(
                    kind,
                    DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_REMOTE | DRIVE_CDROM | DRIVE_RAMDISK
                ) {
                    return None;
                }
                // Empty card readers and DVD drives have no size: leave them out.
                let (total, free) = disk_space(&root)?;
                let fs = volume_fs(&root).unwrap_or_default();
                let letter = root.trim_end_matches('\\').to_string();
                let upper = root.to_ascii_uppercase();
                Some(VolumeInfo {
                    id: letter.clone(),
                    label: letter,
                    mount_path: root,
                    file_system: fs,
                    total_bytes: total,
                    free_bytes: free,
                    is_remote: kind == DRIVE_REMOTE
                        || virtual_drives
                            .iter()
                            .any(|v| Path::new(v) == Path::new(&upper)),
                    is_removable: matches!(kind, DRIVE_REMOVABLE | DRIVE_CDROM),
                })
            })
            .collect()
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[allow(unsafe_code)]
    fn drive_roots() -> Vec<String> {
        let mut buf = vec![0u16; 512];
        // SAFETY: the buffer is valid for its whole length; the API writes at most that many
        // u16s and returns the count written.
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

    #[allow(unsafe_code)]
    fn drive_type(root: &str) -> u32 {
        let w = wide(root);
        // SAFETY: `w` is NUL-terminated and outlives the call.
        unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) }
    }

    /// Runs `f` with "insert a disk" error dialogs suppressed for this thread.
    #[allow(unsafe_code)]
    fn quietly<T>(f: impl FnOnce() -> T) -> T {
        let mut old = THREAD_ERROR_MODE(0);
        // SAFETY: plain FFI calls; the previous mode is restored afterwards.
        let _ = unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, Some(&mut old)) };
        let r = f();
        // SAFETY: as above.
        let _ = unsafe { SetThreadErrorMode(old, None) };
        r
    }

    #[allow(unsafe_code)]
    fn disk_space(root: &str) -> Option<(u64, u64)> {
        let w = wide(root);
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: `w` is NUL-terminated; the three out-pointers are valid u64s.
        let r = quietly(|| unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(w.as_ptr()),
                Some(&mut avail),
                Some(&mut total),
                Some(&mut free),
            )
        });
        r.ok().map(|_| (total, free))
    }

    #[allow(unsafe_code)]
    fn volume_fs(root: &str) -> Option<String> {
        let w = wide(root);
        let mut fs = [0u16; 261];
        // SAFETY: `w` is NUL-terminated; `fs` is a caller-owned buffer whose length is passed.
        let r = quietly(|| unsafe {
            GetVolumeInformationW(PCWSTR(w.as_ptr()), None, None, None, None, Some(&mut fs))
        });
        r.ok()?;
        let end = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
        Some(String::from_utf16_lossy(&fs[..end]))
    }
}

#[cfg(unix)]
mod os {
    use super::*;
    use lustro_cloud::mounts::{fs_skip_reason, MountEntry};
    use lustro_core::scan::SkipReason;
    use std::ffi::CString;

    #[allow(unsafe_code)]
    pub(super) fn is_elevated() -> bool {
        // SAFETY: geteuid has no preconditions and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }

    pub(super) fn list_volumes() -> Vec<VolumeInfo> {
        let mounts = lustro_cloud::system_mounts();
        let root_name = root_volume_name();
        mounts
            .iter()
            .filter(|m| is_user_volume(m))
            .filter_map(|m| {
                let (total, free) = statvfs(&m.mount_point)?;
                let label = if m.mount_point == "/" {
                    root_name.clone().unwrap_or_else(|| "/".into())
                } else {
                    m.mount_point
                        .rsplit('/')
                        .next()
                        .unwrap_or(&m.mount_point)
                        .to_string()
                };
                Some(VolumeInfo {
                    id: m.mount_point.clone(),
                    label,
                    mount_path: m.mount_point.clone(),
                    file_system: m.fs_type.clone(),
                    total_bytes: total,
                    free_bytes: free,
                    is_remote: fs_skip_reason(&m.fs_type) == Some(SkipReason::Remote),
                    is_removable: ["/media/", "/run/media/", "/Volumes/"]
                        .iter()
                        .any(|p| m.mount_point.starts_with(p)),
                })
            })
            .collect()
    }

    /// Real storage the user would scan: not pseudo filesystems, snap images or the macOS
    /// system sub-volumes that are already part of `/`.
    fn is_user_volume(m: &MountEntry) -> bool {
        if fs_skip_reason(&m.fs_type) == Some(SkipReason::PseudoFilesystem) {
            return false;
        }
        if matches!(
            m.fs_type.as_str(),
            "squashfs" | "overlay" | "nullfs" | "devfs"
        ) {
            return false;
        }
        if m.mount_point.starts_with("/System/Volumes/") || m.mount_point.starts_with("/snap/") {
            return false;
        }
        if ["/boot", "/efi", "/boot/efi"].contains(&m.mount_point.as_str()) {
            return false;
        }
        fs_skip_reason(&m.fs_type) == Some(SkipReason::Remote) || m.source.starts_with("/dev/")
    }

    /// macOS: `/Volumes/<name>` is a symlink to `/` for the startup disk.
    fn root_volume_name() -> Option<String> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        std::fs::read_dir("/Volumes").ok()?.flatten().find_map(|e| {
            let target = std::fs::read_link(e.path()).ok()?;
            (target == std::path::Path::new("/"))
                .then(|| e.file_name().to_string_lossy().into_owned())
        })
    }

    #[allow(unsafe_code)]
    fn statvfs(path: &str) -> Option<(u64, u64)> {
        let c = CString::new(path).ok()?;
        // SAFETY: zeroed statvfs is a valid out-parameter; `c` is NUL-terminated.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: valid pointers for the duration of the call.
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        #[allow(clippy::unnecessary_cast)]
        let frsize = if s.f_frsize > 0 {
            s.f_frsize as u64
        } else {
            s.f_bsize as u64
        };
        #[allow(clippy::unnecessary_cast)]
        Some((s.f_blocks as u64 * frsize, s.f_bavail as u64 * frsize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_check_runs() {
        // Normally false in tests; just make sure the call works.
        let _ = is_elevated();
    }

    /// Reads real volume info. Prints only counts and filesystem names.
    #[test]
    fn lists_volumes() {
        let v = list_volumes();
        assert!(!v.is_empty(), "at least the system volume");
        for vol in &v {
            assert!(vol.total_bytes >= vol.free_bytes);
            println!(
                "volume fs={} remote={} removable={}",
                vol.file_system, vol.is_remote, vol.is_removable
            );
        }
        if cfg!(windows) {
            let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
            let c = v.iter().find(|x| x.id.eq_ignore_ascii_case(&sys)).unwrap();
            assert!(c.total_bytes > 0);
            assert_eq!(c.label, c.id);
            assert!(!c.is_remote);
        }
    }

    #[test]
    fn serialises_like_the_ui_type() {
        let v = VolumeInfo {
            id: "C:".into(),
            label: "C:".into(),
            mount_path: r"C:\".into(),
            file_system: "NTFS".into(),
            total_bytes: 1,
            free_bytes: 0,
            is_remote: false,
            is_removable: false,
        };
        let j = serde_json::to_value(&v).unwrap();
        for k in [
            "id",
            "label",
            "mountPath",
            "fileSystem",
            "totalBytes",
            "freeBytes",
            "isRemote",
            "isRemovable",
        ] {
            assert!(j.get(k).is_some(), "{k}");
        }
    }
}
