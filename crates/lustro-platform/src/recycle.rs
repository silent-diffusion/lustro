//! Windows "recycle or fail". On a volume without a Recycle Bin, the shell's delete can
//! silently become a permanent delete, so before trashing we resolve the item's real
//! volume (through `subst`, mapped drives and mount points) and only allow volumes that
//! have a bin. The delete itself runs with a progress sink that cancels any item the shell
//! would not recycle, so even a wrong guess here can't permanently delete anything.

use std::path::{Component, Path, Prefix};

/// `GetDriveTypeW` result for the item's real volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriveKind {
    Fixed,
    Removable,
    Remote,
    CdRom,
    RamDisk,
    Unknown,
}

/// What the Recycle Bin settings say about one volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BinConfig {
    /// The shell keeps Recycle Bin settings for this volume and reports a bin for it.
    pub confirmed: bool,
    /// "Don't move files to the Recycle Bin" is set for this volume, or by policy.
    pub nuke_on_delete: bool,
}

/// Only fixed drives, and removable drives with a confirmed bin, are allowed.
pub(crate) fn volume_has_bin(kind: DriveKind, bin: BinConfig) -> bool {
    match kind {
        DriveKind::Fixed => !bin.nuke_on_delete,
        DriveKind::Removable => bin.confirmed && !bin.nuke_on_delete,
        DriveKind::Remote | DriveKind::CdRom | DriveKind::RamDisk | DriveKind::Unknown => false,
    }
}

/// Only drive-letter paths (`C:\…`, `\\?\C:\…`) go to the shell. UNC, `\\?\UNC\`, device
/// and volume-GUID paths are refused from the text alone, before any API call.
pub(crate) fn is_drive_letter_path(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(std::path::Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    )
}

/// Windows names that the shell or Win32 would quietly rewrite or treat specially, so a
/// check on the name as written could be about a different item: a trailing dot or
/// space, a `:` (alternate data stream), or a DOS device name (`CON`, `NUL`, `COM1`…,
/// with or without an extension).
pub(crate) fn has_unsafe_windows_name(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(n) => is_unsafe_name(&n.to_string_lossy()),
        _ => false,
    })
}

fn is_unsafe_name(name: &str) -> bool {
    if name.ends_with('.') || name.ends_with(' ') || name.contains(':') {
        return true;
    }
    let stem = name.split('.').next().unwrap_or("").trim_end();
    let upper = stem.to_ascii_uppercase();
    let device = |prefix: &str| {
        upper.strip_prefix(prefix).is_some_and(|rest| {
            matches!(
                rest,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
            ) || matches!(rest, "\u{b9}" | "\u{b2}" | "\u{b3}")
        })
    };
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || device("COM")
        || device("LPT")
}

#[cfg(windows)]
pub(crate) use win::{check_volume, identity, open_item, recycle, unrecycle};

/// FILETIME (100 ns since 1601) to Unix seconds, exactly as the scanner converts it.
pub(crate) fn filetime_to_unix(ft: i64) -> Option<i64> {
    (ft > 0).then(|| ft / 10_000_000 - 11_644_473_600)
}

/// A 128-bit file id (FILE_ID_128, little-endian) folded to 64 bits exactly as the scanner
/// folds it (`lustro-scan` walker/windows.rs): `lo ^ hi.rotate_left(32)`. On NTFS `hi` is 0,
/// so this is the plain file index; on ReFS it keeps wide ids distinct.
pub(crate) fn fold_file_id(id: [u8; 16]) -> u64 {
    let lo = u64::from_le_bytes(id[..8].try_into().unwrap_or_default());
    let hi = u64::from_le_bytes(id[8..].try_into().unwrap_or_default());
    lo ^ hi.rotate_left(32)
}

#[cfg(windows)]
mod win {
    use super::*;
    use crate::trash::TrashError;
    use lustro_core::dto::ItemIdentity;
    use lustro_core::model::{FileId, NodeKind};
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use windows::core::{implement, Ref, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, E_ABORT, HANDLE, RPC_E_CHANGED_MODE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileAttributeTagInfo, FileIdInfo, FileRenameInfo, FileRenameInfoEx,
        GetDriveTypeW, GetFileInformationByHandle, GetFileInformationByHandleEx,
        GetFinalPathNameByHandleW, SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, DELETE,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
        FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_RENAME_INFO,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GETFINALPATHNAMEBYHANDLE_FLAGS,
        OPEN_EXISTING, VOLUME_NAME_DOS, VOLUME_NAME_GUID,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
        IShellItem, SHCreateItemFromParsingName, SHQueryRecycleBinW, FOFX_EARLYFAILURE,
        FOFX_RECYCLEONDELETE, FOF_ALLOWUNDO, FOF_NO_UI, SHQUERYRBINFO,
        TSF_DELETE_RECYCLE_IF_POSSIBLE,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(Some(0)).collect()
    }

    /// A `windows` error as an `io::Error` whose kind works (`NotFound`, `PermissionDenied`):
    /// Win32 errors wrapped in an HRESULT go back to their plain error code.
    fn win_io(e: windows::core::Error) -> std::io::Error {
        let hr = e.code().0 as u32;
        if hr & 0xFFFF_0000 == 0x8007_0000 {
            std::io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
        } else {
            std::io::Error::other(e)
        }
    }

    /// Closes the handle on drop.
    struct Handle(HANDLE);
    impl Drop for Handle {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful CreateFileW and is closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// The item to trash, opened for metadata only and held open until the delete is
    /// queued, with its final path as the file system reports it.
    pub(crate) struct OpenItem {
        handle: Handle,
        /// Drive-letter form (`C:\…`) when on a local volume: long names (no 8.3), real
        /// case, trailing dots and spaces gone, links in parent folders resolved. A
        /// network item keeps its `\\?\UNC\…` form and is refused by the caller.
        pub(crate) path: PathBuf,
    }

    /// Opens `path` itself (no access rights requested, OPEN_REPARSE_POINT): a link or
    /// cloud placeholder is opened as itself, so nothing is followed or hydrated.
    #[allow(unsafe_code)]
    fn open_meta(path: &Path) -> std::io::Result<Handle> {
        let w = wide(path);
        // SAFETY: `w` is NUL-terminated and outlives the call; the handle is owned by
        // `Handle` and closed on drop.
        let h = unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(win_io)?;
        Ok(Handle(h))
    }

    /// The item's identity, built exactly like the scanner's (`lustro-scan` walker/windows.rs):
    /// volume serial + file index, name-surrogate reparse points are links, last write
    /// time in Unix seconds. `Ok(None)` when nothing is at `path`.
    #[allow(unsafe_code)]
    pub(crate) fn identity(path: &Path) -> std::io::Result<Option<ItemIdentity>> {
        match open_meta(path) {
            Ok(h) => identity_of(&h).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    impl OpenItem {
        /// The identity read from the handle this item will be trashed through.
        pub(crate) fn identity(&self) -> std::io::Result<ItemIdentity> {
            identity_of(&self.handle)
        }
    }

    #[allow(unsafe_code)]
    fn identity_of(h: &Handle) -> std::io::Result<ItemIdentity> {
        const TAG_NAME_SURROGATE: u32 = 0x2000_0000;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: valid open handle; `info` is a writable struct of the right type.
        unsafe { GetFileInformationByHandle(h.0, &mut info) }.map_err(win_io)?;
        let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
        // SAFETY: valid open handle; `tag` is a writable struct and its exact size is passed.
        unsafe {
            GetFileInformationByHandleEx(
                h.0,
                FileAttributeTagInfo,
                &mut tag as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        }
        .map_err(win_io)?;
        // The scanner takes the index from the 128-bit id (FILE_ID_EXTD_DIR_INFO), folded;
        // read the same id here so ReFS items match too. The volume stays the 32-bit
        // serial, as in the scanner. Fall back to the 64-bit index where FileIdInfo isn't
        // supported (the same value on NTFS).
        let mut wide_id = FILE_ID_INFO::default();
        // SAFETY: valid open handle; `wide_id` is writable and its exact size is passed.
        let index = match unsafe {
            GetFileInformationByHandleEx(
                h.0,
                FileIdInfo,
                &mut wide_id as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        } {
            Ok(()) => super::fold_file_id(wide_id.FileId.Identifier),
            Err(_) => (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        };
        let attrs = info.dwFileAttributes;
        let kind = if attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            && tag.ReparseTag & TAG_NAME_SURROGATE != 0
        {
            NodeKind::Link
        } else if attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
            NodeKind::Dir
        } else {
            NodeKind::File
        };
        let ft = |t: windows::Win32::Foundation::FILETIME| {
            ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) as i64
        };
        Ok(ItemIdentity {
            kind,
            file_id: Some(FileId {
                volume: u64::from(info.dwVolumeSerialNumber),
                index,
            }),
            logical_size: if kind == NodeKind::Dir {
                0
            } else {
                (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow)
            },
            modified: super::filetime_to_unix(ft(info.ftLastWriteTime)),
        })
    }

    pub(crate) fn open_item(path: &Path) -> Result<OpenItem, TrashError> {
        let handle = open_meta(path).map_err(crate::trash::io_err)?;
        let dos = final_path(&handle, VOLUME_NAME_DOS).ok_or(TrashError::NotFound)?;
        let path = match dos.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
            _ => PathBuf::from(dos),
        };
        Ok(OpenItem { handle, path })
    }

    /// `GetFinalPathNameByHandleW(FILE_NAME_NORMALIZED | volume_form)`.
    #[allow(unsafe_code)]
    fn final_path(h: &Handle, volume_form: GETFINALPATHNAMEBYHANDLE_FLAGS) -> Option<String> {
        let mut buf = vec![0u16; 1024];
        loop {
            // SAFETY: valid open handle; `buf` is writable and its length is passed.
            let n = unsafe {
                GetFinalPathNameByHandleW(
                    h.0,
                    &mut buf,
                    GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | volume_form.0),
                )
            } as usize;
            if n == 0 {
                return None;
            }
            if n < buf.len() {
                buf.truncate(n);
                return Some(String::from_utf16_lossy(&buf));
            }
            // Too small: `n` is the size needed, including the NUL.
            buf.resize(n + 1, 0);
        }
    }

    /// `Ok` only when the item's real volume has a Recycle Bin. The volume comes from the
    /// open handle, so it sees through subst, mapped drives and mount points.
    pub(crate) fn check_volume(item: &OpenItem) -> Result<(), TrashError> {
        if !is_drive_letter_path(&item.path) {
            return Err(TrashError::NoRecycleBin);
        }
        // A network file has no volume GUID, so this fails for it.
        let volume = final_path(&item.handle, VOLUME_NAME_GUID)
            .and_then(|p| guid_root(&p))
            .ok_or(TrashError::NoRecycleBin)?;
        let kind = drive_kind(&volume);
        let bin = if matches!(kind, DriveKind::Fixed | DriveKind::Removable) {
            bin_config(&volume)
        } else {
            BinConfig::default()
        };
        if volume_has_bin(kind, bin) {
            Ok(())
        } else {
            Err(TrashError::NoRecycleBin)
        }
    }

    #[allow(unsafe_code)]
    fn drive_kind(volume_root: &str) -> DriveKind {
        let w = wide(volume_root);
        // SAFETY: `w` is NUL-terminated and outlives the call.
        match unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } {
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_REMOTE => DriveKind::Remote,
            DRIVE_CDROM => DriveKind::CdRom,
            DRIVE_RAMDISK => DriveKind::RamDisk,
            _ => DriveKind::Unknown,
        }
    }

    /// Reads the per-volume Recycle Bin settings (`BitBucket\Volume\{GUID}`) and the
    /// "no Recycle Bin" policy. A volume counts as confirmed only when the shell keeps
    /// settings for it and `SHQueryRecycleBinW` answers for it.
    #[allow(unsafe_code)]
    fn bin_config(volume_root: &str) -> BinConfig {
        use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
        const POLICY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";
        const BITBUCKET: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume";
        let policy_nuke = [CURRENT_USER, LOCAL_MACHINE].iter().any(|hive| {
            hive.open(POLICY)
                .and_then(|k| k.get_u32("NoRecycleFiles"))
                .is_ok_and(|v| v != 0)
        });
        let guid = volume_root
            .trim_start_matches(r"\\?\Volume")
            .trim_end_matches('\\');
        let key = CURRENT_USER.open(format!(r"{BITBUCKET}\{guid}")).ok();
        let volume_nuke = key
            .as_ref()
            .and_then(|k| k.get_u32("NukeOnDelete").ok())
            .is_some_and(|v| v != 0);
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };
        let w = wide(volume_root);
        // SAFETY: `w` is NUL-terminated and outlives the call; `info` is writable with
        // its size set. Only reads the bin's size and item count.
        let answered = unsafe { SHQueryRecycleBinW(PCWSTR(w.as_ptr()), &mut info) }.is_ok();
        BinConfig {
            confirmed: key.is_some() && answered,
            nuke_on_delete: policy_nuke || volume_nuke,
        }
    }

    /// `\\?\Volume{…}\rest` → `\\?\Volume{…}\`.
    pub(super) fn guid_root(final_path: &str) -> Option<String> {
        let rest = final_path.strip_prefix(r"\\?\Volume{")?;
        let end = rest.find("}\\")?;
        Some(format!(r"\\?\Volume{{{}}}\", &rest[..end]))
    }

    /// The progress sink. `implement` generates the COM vtable glue, which is unsafe by
    /// nature; the methods themselves are safe Rust.
    #[allow(unsafe_code)]
    mod sink {
        use super::*;

        /// Cancels any delete the shell would not send to the Recycle Bin.
        #[implement(IFileOperationProgressSink)]
        pub(super) struct RecycleGuard {
            pub(super) refused: Arc<AtomicBool>,
        }

        impl IFileOperationProgressSink_Impl for RecycleGuard_Impl {
            fn PreDeleteItem(
                &self,
                flags: u32,
                _item: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
                    // The shell would delete this permanently: stop.
                    self.refused.store(true, Ordering::SeqCst);
                    return Err(E_ABORT.into());
                }
                Ok(())
            }
            fn StartOperations(&self) -> windows::core::Result<()> {
                Ok(())
            }
            fn FinishOperations(&self, _hr: HRESULT) -> windows::core::Result<()> {
                Ok(())
            }
            fn PreRenameItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: &PCWSTR,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PostRenameItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: &PCWSTR,
                _: HRESULT,
                _: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PreMoveItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: Ref<IShellItem>,
                _: &PCWSTR,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PostMoveItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: Ref<IShellItem>,
                _: &PCWSTR,
                _: HRESULT,
                _: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PreCopyItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: Ref<IShellItem>,
                _: &PCWSTR,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PostCopyItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: Ref<IShellItem>,
                _: &PCWSTR,
                _: HRESULT,
                _: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PostDeleteItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: HRESULT,
                _: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PreNewItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: &PCWSTR,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn PostNewItem(
                &self,
                _: u32,
                _: Ref<IShellItem>,
                _: &PCWSTR,
                _: &PCWSTR,
                _: u32,
                _: HRESULT,
                _: Ref<IShellItem>,
            ) -> windows::core::Result<()> {
                Ok(())
            }
            fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
                Ok(())
            }
            fn ResetTimer(&self) -> windows::core::Result<()> {
                Ok(())
            }
            fn PauseTimer(&self) -> windows::core::Result<()> {
                Ok(())
            }
            fn ResumeTimer(&self) -> windows::core::Result<()> {
                Ok(())
            }
        }
    }
    use sink::RecycleGuard;

    /// Uninitializes COM on drop if this thread initialized it here.
    struct Com(bool);
    impl Drop for Com {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the successful CoInitializeEx in `recycle`.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Sends `path` (absolute, drive-letter form, parent already resolved) to the Recycle
    /// Bin. Fails, deleting nothing, if the shell would delete it permanently.
    #[allow(unsafe_code)]
    pub(crate) fn recycle(path: &Path) -> Result<(), TrashError> {
        // SAFETY: plain COM initialization for this thread, balanced by `Com`'s drop.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        // RPC_E_CHANGED_MODE: the thread already uses COM in another mode; that's fine.
        if hr.is_err() && hr != RPC_E_CHANGED_MODE {
            return Err(TrashError::TrashFailed);
        }
        let _com = Com(hr.is_ok());

        let refused = Arc::new(AtomicBool::new(false));
        let sink: IFileOperationProgressSink = RecycleGuard {
            refused: refused.clone(),
        }
        .into();
        let name = wide(shell_name(path));
        // SAFETY: COM is initialized on this thread; every pointer argument is a live,
        // NUL-terminated buffer or a COM reference that outlives the call.
        let aborted = unsafe {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)
                .map_err(|_| TrashError::TrashFailed)?;
            // FOF_NO_UI suppresses dialogs; it can't cause a permanent delete because
            // RecycleGuard cancels every item that wouldn't be recycled. No
            // FOF_WANTNUKEWARNING: a nuke never gets as far as a warning.
            op.SetOperationFlags(
                FOF_ALLOWUNDO | FOF_NO_UI | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE,
            )
            .map_err(|_| TrashError::TrashFailed)?;
            op.Advise(&sink).map_err(|_| TrashError::TrashFailed)?;
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None)
                .map_err(|_| TrashError::NotFound)?;
            op.DeleteItem(&item, None)
                .map_err(|_| TrashError::TrashFailed)?;
            let performed = op.PerformOperations();
            let aborted = op.GetAnyOperationsAborted().map(|b| b.as_bool());
            performed.is_err() || aborted.unwrap_or(true)
        };
        if refused.load(Ordering::SeqCst) {
            Err(TrashError::NoRecycleBin)
        } else if aborted {
            Err(TrashError::TrashFailed)
        } else {
            Ok(())
        }
    }

    /// Moves an item out of the Recycle Bin (`…\$Recycle.Bin\<sid>\$R…`) back to `to`,
    /// then removes its `$I…` record.
    /// - `to` must be a plain absolute drive path: no `.`/`..`, no name Win32 would
    ///   rewrite.
    /// - Its folder is opened with links followed, and its final path must be exactly
    ///   that folder, so a folder in the chain that has since become a junction is refused.
    ///   It is opened without FILE_SHARE_DELETE (with list access, since share modes
    ///   only bind handles with data access; nothing is enumerated) and held until the
    ///   move is done, so neither it nor any folder above it can be renamed, deleted or
    ///   swapped for a junction in between.
    /// - The bin item is opened as itself, and its file id is compared on that handle.
    /// - That handle is renamed. It never replaces anything (FileRenameInfoEx flags 0,
    ///   else FileRenameInfo with ReplaceIfExists false). It is a same-volume rename,
    ///   never a copy. The folder isn't recreated.
    pub(crate) fn unrecycle(
        in_bin: &Path,
        to: &Path,
        expected: Option<FileId>,
    ) -> std::io::Result<()> {
        use std::io::{Error, ErrorKind};
        let invalid = || Error::from(ErrorKind::InvalidInput);
        let name = in_bin.file_name().ok_or_else(invalid)?.to_string_lossy();
        let in_a_bin = in_bin
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case("$Recycle.Bin"));
        let rest = name
            .get(..2)
            .filter(|p| p.eq_ignore_ascii_case("$R"))
            .map(|_| &name[2..]);
        let (true, Some(rest)) = (in_a_bin, rest) else {
            return Err(invalid());
        };
        let record = in_bin.with_file_name(format!("$I{rest}"));

        let plain = to.is_absolute()
            && super::is_drive_letter_path(to)
            && !to
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
            && !super::has_unsafe_windows_name(to);
        let (true, Some(folder)) = (plain, to.parent().filter(|_| to.file_name().is_some())) else {
            return Err(invalid());
        };
        let pinned = open_with(
            folder,
            (FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES).0,
            true,
            false,
        )?;
        let key = |p: &str| lustro_cloud::paths::key(p, true);
        let real = final_path(&pinned, VOLUME_NAME_DOS).ok_or_else(invalid)?;
        if key(&real) != key(&folder.to_string_lossy()) {
            return Err(invalid());
        }

        let item = open_with(in_bin, (DELETE | FILE_READ_ATTRIBUTES).0, false, true)?;
        if expected.is_some_and(|id| identity_of(&item).ok().and_then(|i| i.file_id) != Some(id)) {
            return Err(Error::other("not the item that was trashed"));
        }
        // The rename API takes a full path here (a name relative to a folder handle is
        // rejected); with the folder pinned it can only resolve to the checked folder.
        let target = Path::new(r"\\?\").join(shell_name(to));
        rename(&item, target.as_os_str())?;
        drop(item);
        drop(pinned);
        // Best effort: without it the bin shows a stale entry, but the item is back.
        let _ = std::fs::remove_file(record);
        Ok(())
    }

    /// The folder pin `unrecycle` takes, for the test that it holds.
    #[cfg(test)]
    pub(super) fn pin_for_test(folder: &Path) -> std::io::Result<impl Drop> {
        open_with(
            folder,
            (FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES).0,
            true,
            false,
        )
    }

    /// Opens `path` with `access`. Links are followed only if `follow`; other openers
    /// may rename or delete it only if `share_delete`.
    #[allow(unsafe_code)]
    fn open_with(
        path: &Path,
        access: u32,
        follow: bool,
        share_delete: bool,
    ) -> std::io::Result<Handle> {
        let w = wide(path);
        let flags = if follow {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
        };
        let share = if share_delete {
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        } else {
            FILE_SHARE_READ | FILE_SHARE_WRITE
        };
        // SAFETY: `w` is NUL-terminated and outlives the call; the handle is owned by
        // `Handle` and closed on drop.
        let h = unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                access,
                share,
                None,
                OPEN_EXISTING,
                flags,
                None,
            )
        }
        .map_err(win_io)?;
        Ok(Handle(h))
    }

    /// Renames the item open as `item` to the full path `to`, never replacing anything.
    #[allow(unsafe_code)]
    fn rename(item: &Handle, to: &std::ffi::OsStr) -> std::io::Result<()> {
        const ERROR_INVALID_FUNCTION: u32 = 1;
        const ERROR_NOT_SUPPORTED: u32 = 50;
        const ERROR_INVALID_PARAMETER: u32 = 87;
        let name: Vec<u16> = to.encode_wide().collect();
        let bytes = name.len() * 2;
        // The struct plus the name, as Rust's std passes it; that leaves room for a NUL.
        let size = std::mem::size_of::<FILE_RENAME_INFO>() + bytes;
        // Zeroed, so ReplaceIfExists / Flags are 0; u64 storage keeps it 8-aligned.
        let mut buf = vec![0u64; size.div_ceil(8)];
        let info = buf.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: `buf` is aligned and large enough for the header and the name; the name
        // is copied within those bounds.
        unsafe {
            (*info).FileNameLength = bytes as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
        }
        let code = |e: &windows::core::Error| (e.code().0 as u32) & 0xFFFF;
        for class in [FileRenameInfoEx, FileRenameInfo] {
            // SAFETY: valid handle (kept open by the caller); `info` points to `size`
            // initialized bytes of a FILE_RENAME_INFO.
            let done = unsafe {
                SetFileInformationByHandle(
                    item.0,
                    class,
                    info as *const core::ffi::c_void,
                    size as u32,
                )
            };
            match done {
                Ok(()) => return Ok(()),
                // FileRenameInfoEx is Windows 10 1709+.
                Err(e)
                    if class == FileRenameInfoEx
                        && [
                            ERROR_INVALID_FUNCTION,
                            ERROR_NOT_SUPPORTED,
                            ERROR_INVALID_PARAMETER,
                        ]
                        .contains(&code(&e)) => {}
                Err(e) if [ERROR_ALREADY_EXISTS.0, ERROR_FILE_EXISTS.0].contains(&code(&e)) => {
                    return Err(std::io::ErrorKind::AlreadyExists.into())
                }
                Err(e) => return Err(win_io(e)),
            }
        }
        Err(std::io::ErrorKind::Unsupported.into())
    }

    /// The shell doesn't parse `\\?\C:\…`; turn it back into `C:\…`.
    fn shell_name(path: &Path) -> std::ffi::OsString {
        let s = path.as_os_str().to_string_lossy();
        match s.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.into(),
            _ => path.as_os_str().to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_and_confirmed_removable_drives_have_a_bin() {
        let none = BinConfig::default();
        let confirmed = BinConfig {
            confirmed: true,
            nuke_on_delete: false,
        };
        let nuke = BinConfig {
            confirmed: true,
            nuke_on_delete: true,
        };
        assert!(volume_has_bin(DriveKind::Fixed, none));
        assert!(volume_has_bin(DriveKind::Fixed, confirmed));
        assert!(!volume_has_bin(DriveKind::Fixed, nuke));
        assert!(!volume_has_bin(DriveKind::Removable, none));
        assert!(volume_has_bin(DriveKind::Removable, confirmed));
        assert!(!volume_has_bin(DriveKind::Removable, nuke));
        for k in [
            DriveKind::Remote,
            DriveKind::CdRom,
            DriveKind::RamDisk,
            DriveKind::Unknown,
        ] {
            assert!(!volume_has_bin(k, confirmed), "{k:?}");
        }
    }

    #[test]
    fn unsafe_windows_names() {
        for n in [
            "a.",
            "a ",
            "a. ",
            "a:b",
            "a::$DATA",
            "CON",
            "con",
            "NUL.txt",
            "nul .txt",
            "COM1",
            "com9.log",
            "LPT1",
            "COM\u{b9}",
            "AUX",
            "PRN.x",
            "CONIN$",
        ] {
            assert!(is_unsafe_name(n), "{n}");
        }
        for n in [
            "a",
            "a.txt",
            ".gitignore",
            "CONSOLE",
            "COM10",
            "nullable",
            "aux2",
            "com",
        ] {
            assert!(!is_unsafe_name(n), "{n}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn only_drive_letter_paths_are_accepted() {
        assert!(is_drive_letter_path(Path::new(r"C:\Users\William\a.txt")));
        assert!(is_drive_letter_path(Path::new(
            r"\\?\C:\Users\William\a.txt"
        )));
        for p in [
            r"\\localhost\c$\Users\William\a.txt",
            r"\\server\share\a.txt",
            r"\\?\UNC\server\share\a.txt",
            r"\\.\C:\a.txt",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\a.txt",
        ] {
            assert!(!is_drive_letter_path(Path::new(p)), "{p}");
        }
    }

    #[test]
    fn wide_file_ids_fold_like_the_scanner() {
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&0x0005_0000_0001_2345u64.to_le_bytes());
        // NTFS: high half is zero, so the fold is the plain 64-bit index.
        assert_eq!(fold_file_id(id), 0x0005_0000_0001_2345);
        // ReFS: the high half changes the result, the same way as `lo ^ hi.rotate_left(32)`.
        id[8..].copy_from_slice(&0x0000_0000_0000_0007u64.to_le_bytes());
        assert_eq!(fold_file_id(id), 0x0005_0000_0001_2345 ^ (7u64 << 32));
    }

    /// What `unrecycle` relies on: while the destination folder is held without
    /// FILE_SHARE_DELETE, neither it nor a folder above it can be renamed away (so
    /// nothing can be swapped in at that path); released, they can.
    #[cfg(windows)]
    #[test]
    fn a_pinned_folder_and_its_parents_cannot_be_moved() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("top");
        let folder = top.join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        let pinned = win::pin_for_test(&folder).unwrap();
        assert!(std::fs::rename(&folder, top.join("moved")).is_err());
        assert!(std::fs::rename(&top, dir.path().join("moved")).is_err());
        assert!(folder.exists());
        drop(pinned);
        std::fs::rename(&top, dir.path().join("moved")).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn guid_root_is_cut_from_a_final_path() {
        assert_eq!(
            win::guid_root(r"\\?\Volume{0a1b2c3d-0000-0000-0000-000000000001}\Users\William\a"),
            Some(r"\\?\Volume{0a1b2c3d-0000-0000-0000-000000000001}\".to_string())
        );
        assert_eq!(win::guid_root(r"\\?\UNC\server\share\a"), None);
        assert_eq!(win::guid_root(r"\\?\C:\a"), None);
    }
}
