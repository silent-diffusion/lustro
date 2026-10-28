//! Windows directory listing via `GetFileInformationByHandleEx`.
//!
//! Every handle is opened with `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`
//! (or the `NtCreateFile` equivalents) and never with data access, so reparse
//! points aren't followed and cloud placeholders are never hydrated.
//!
//! Below the root, everything is opened *relative to its parent's handle*
//! (`NtCreateFile` with `RootDirectory`), never by re-parsing a full path. A folder
//! swapped for a junction mid-scan therefore can't redirect the walk elsewhere, and
//! a folder that has turned into a link is refused.

// OS API calls need `unsafe`; each block is small and has a SAFETY note.
#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use lustro_core::model::{EntryFlags, EntryMeta, FileId, NodeKind};
use lustro_core::scan::{RawAttrs, ScanOptions, SkipReason};
use rayon::prelude::*;
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    NtCreateFile, FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_FOR_BACKUP_INTENT,
    FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
};
use windows_sys::Win32::Foundation::{
    RtlNtStatusToDosError, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE,
    UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileAttributeTagInfo, FileFullDirectoryInfo, FileIdExtdDirectoryInfo,
    FileStandardInfo, GetDriveTypeW, GetFileInformationByHandle, GetFileInformationByHandleEx,
    GetVolumePathNameW, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_COMPRESSED,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_OFFLINE,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_ATTRIBUTE_SYSTEM,
    FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FULL_DIR_INFO, FILE_ID_EXTD_DIR_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FILE_TRAVERSE, OPEN_EXISTING, SYNCHRONIZE,
};
use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

use super::{filetime_to_unix, Entry};

const DRIVE_REMOTE: u32 = 4;
/// Reparse tags with this bit are name surrogates (symlinks, junctions, mount points).
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// Above this many entries, a directory's per-file queries run in parallel.
const PAR_STAT_MIN: usize = 64;
/// Files with any of these are cloud or HSM placeholders: even a metadata-only open
/// could involve the provider, so they keep their directory-listing sizes.
const RECALL_ATTRS: u32 =
    FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE;

/// An open folder being listed.
pub(crate) type DirHandle = OwnedHandle;
/// A folder opened by its parent's task, waiting for its own task.
pub(crate) type Pending = OwnedHandle;

pub(crate) struct Volume {
    serial: u64,
    /// Cleared the first time the filesystem rejects `FileIdExtdDirectoryInfo` (FAT etc.).
    extd: AtomicBool,
}

/// Opens `path` itself (not a reparse target) with metadata-only access.
pub(crate) fn open_meta(path: &Path, access: u32) -> io::Result<OwnedHandle> {
    let wide = verbatim(path);
    // SAFETY: `wide` is NUL-terminated and outlives the call; the security-attributes
    // and template pointers may be null.
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `h` is a valid handle we own and close exactly once via OwnedHandle.
    Ok(unsafe { OwnedHandle::from_raw_handle(h) })
}

/// Opens the scan root (a full path the user picked).
pub(crate) fn open_root_dir(path: &Path) -> io::Result<Pending> {
    open_meta(path, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
}

/// Opens subfolder `name` of `parent` for listing, relative to the parent's handle.
/// Fails if it is now a symlink/junction/mount point (changed since the listing).
pub(crate) fn open_child(parent: &DirHandle, name: &OsStr, _entry: &Entry) -> io::Result<Pending> {
    let h = open_at(
        parent,
        name,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | FILE_TRAVERSE,
        true,
    )?;
    let (attrs, tag) = attribute_tag(&h)?;
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && tag & TAG_NAME_SURROGATE != 0 {
        return Err(io::Error::other(
            "folder turned into a link during the scan",
        ));
    }
    Ok(h)
}

/// Windows opens folders in the parent's task already; nothing more to do.
pub(crate) fn activate(pending: Pending, _path: &Path) -> io::Result<DirHandle> {
    Ok(pending)
}

/// Opens `name` inside the folder `parent` via `NtCreateFile` with `RootDirectory`:
/// no full-path parsing, no following of reparse points (`FILE_OPEN_REPARSE_POINT`
/// for the name itself, `OBJ_DONT_REPARSE` for anything the object manager would
/// otherwise reparse), backup semantics. `name` must be a single plain component;
/// a crafted volume listing names with separators or `:` is refused here.
pub(crate) fn open_at(
    parent: &OwnedHandle,
    name: &OsStr,
    access: u32,
    directory: bool,
) -> io::Result<OwnedHandle> {
    let mut wide: Vec<u16> = name.encode_wide().collect();
    let bytes = wide.len() * 2;
    let dots = wide == [0x2E] || wide == [0x2E, 0x2E];
    // NUL, '/', ':' and '\' can't appear in a single NTFS name.
    let separators = wide.iter().any(|&u| matches!(u, 0 | 0x2F | 0x3A | 0x5C));
    if wide.is_empty() || bytes > usize::from(u16::MAX) || dots || separators {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let unicode = UNICODE_STRING {
        Length: bytes as u16,
        MaximumLength: bytes as u16,
        Buffer: wide.as_mut_ptr(),
    };
    let attrs = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &unicode,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut options =
        FILE_OPEN_REPARSE_POINT | FILE_OPEN_FOR_BACKUP_INTENT | FILE_SYNCHRONOUS_IO_NONALERT;
    if directory {
        options |= FILE_DIRECTORY_FILE;
    }
    let mut handle = std::ptr::null_mut();
    let mut iosb: IO_STATUS_BLOCK = zeroed_iosb();
    // SAFETY: `unicode`, `wide` and `attrs` outlive the call; `handle` and `iosb` are
    // writable. Synchronous I/O needs SYNCHRONIZE, which is added here.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access | SYNCHRONIZE,
            &attrs,
            &mut iosb,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            options,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: pure conversion of a status code.
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    // SAFETY: success means `handle` is a new handle that we now own.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn zeroed_iosb() -> IO_STATUS_BLOCK {
    // SAFETY: IO_STATUS_BLOCK is plain data; all-zero is a valid (unused) value.
    unsafe { std::mem::zeroed() }
}

/// Attributes and reparse tag of an open handle.
pub(crate) fn attribute_tag(h: &OwnedHandle) -> io::Result<(u32, u32)> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: `info` is a writable FILE_ATTRIBUTE_TAG_INFO of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            h.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((info.FileAttributes, info.ReparseTag))
}

pub(crate) fn root(path: &Path) -> io::Result<(Entry, Volume)> {
    let h = open_meta(path, FILE_READ_ATTRIBUTES)?;
    let mut entry = entry_from_handle(&h)?;
    if is_remote_volume(path) {
        entry.meta.flags |= EntryFlags::REMOTE;
    }
    let serial = entry.meta.file_id.map_or(0, |f| f.volume);
    Ok((
        entry,
        Volume {
            serial,
            extd: AtomicBool::new(true),
        },
    ))
}

/// The entry (name left empty) for an open handle, file id included.
pub(crate) fn entry_from_handle(h: &OwnedHandle) -> io::Result<Entry> {
    let info = by_handle_info(h)?;
    let attrs = info.dwFileAttributes;
    let flags = attr_flags(attrs);
    let kind = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && is_name_surrogate(h) {
        NodeKind::Link
    } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
        NodeKind::Dir
    } else {
        NodeKind::File
    };
    let serial = u64::from(info.dwVolumeSerialNumber);
    let entry = Entry {
        name: OsString::new(),
        meta: EntryMeta {
            name: OsString::new(),
            kind,
            logical_size: 0,
            allocated_size: 0,
            modified: filetime_to_unix(ft(info.ftLastWriteTime)),
            accessed: filetime_to_unix(ft(info.ftLastAccessTime)),
            file_id: Some(FileId {
                volume: serial,
                index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            }),
            link_count: info.nNumberOfLinks.max(1),
            cloud: None,
            flags,
        },
        raw: RawAttrs {
            win_attributes: attrs,
            st_flags: 0,
            is_dir: attrs & FILE_ATTRIBUTE_DIRECTORY != 0,
        },
        dev: 0,
    };
    Ok(entry)
}

/// Nothing to check on Windows: other volumes and network shares can only appear
/// inside a tree as reparse points, which are never followed.
pub(crate) fn boundary(
    _path: &Path,
    _entry: &Entry,
    _parent_dev: u64,
    _opts: &ScanOptions,
) -> (EntryFlags, Option<SkipReason>) {
    (EntryFlags::empty(), None)
}

pub(crate) fn read_dir(handle: &DirHandle, _dir: &Path, vol: &Volume) -> io::Result<Vec<Entry>> {
    let mut entries = list(handle, vol)?;
    let stat = |e: &mut Entry| stat_entry(e, handle);
    if entries.len() >= PAR_STAT_MIN {
        entries.par_iter_mut().for_each(stat);
    } else {
        entries.iter_mut().for_each(stat);
    }
    Ok(entries)
}

/// Lists one directory from its handle. Sizes here come from the directory index,
/// which NTFS updates lazily; `stat_entry` replaces them for files.
fn list(handle: &OwnedHandle, vol: &Volume) -> io::Result<Vec<Entry>> {
    // u64 storage keeps the buffer 8-byte aligned as the API requires.
    let mut buf = vec![0u64; 8 * 1024];
    let mut extd = vol.extd.load(Ordering::Relaxed);
    let mut first = true;
    let mut out = Vec::new();
    loop {
        let class: FILE_INFO_BY_HANDLE_CLASS = if extd {
            FileIdExtdDirectoryInfo
        } else {
            FileFullDirectoryInfo
        };
        // SAFETY: the buffer is valid, writable and 8-byte aligned for its full length.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.as_raw_handle(),
                class,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            let code = err.raw_os_error().unwrap_or(0) as u32;
            if code == ERROR_NO_MORE_FILES {
                break;
            }
            if first
                && extd
                && matches!(
                    code,
                    ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED | ERROR_INVALID_LEVEL
                )
            {
                extd = false;
                vol.extd.store(false, Ordering::Relaxed);
                continue;
            }
            return Err(err);
        }
        first = false;
        // SAFETY: viewing initialised u64 storage as bytes; length is exact.
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), buf.len() * 8) };
        parse_listing(bytes, extd, vol.serial, &mut out);
    }
    Ok(out)
}

fn parse_listing(bytes: &[u8], extd: bool, serial: u64, out: &mut Vec<Entry>) {
    let (o_attrs, o_len, o_ea, o_name) = if extd {
        (
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileAttributes),
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileNameLength),
            offset_of!(FILE_ID_EXTD_DIR_INFO, ReparsePointTag),
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileName),
        )
    } else {
        (
            offset_of!(FILE_FULL_DIR_INFO, FileAttributes),
            offset_of!(FILE_FULL_DIR_INFO, FileNameLength),
            // FILE_FULL_DIR_INFO carries the reparse tag in EaSize for reparse points.
            offset_of!(FILE_FULL_DIR_INFO, EaSize),
            offset_of!(FILE_FULL_DIR_INFO, FileName),
        )
    };
    let o_id = offset_of!(FILE_ID_EXTD_DIR_INFO, FileId);
    let mut off = 0usize;
    while let Some(e) = bytes.get(off..) {
        if e.len() < o_name {
            break;
        }
        let next = u32_at(e, 0) as usize;
        let name_len = u32_at(e, o_len) as usize;
        let Some(name_bytes) = e.get(o_name..o_name + name_len) else {
            break;
        };
        let name16: Vec<u16> = name_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let is_dot = name16 == [b'.' as u16] || name16 == [b'.' as u16, b'.' as u16];
        if !is_dot {
            let attrs = u32_at(e, o_attrs);
            let tag = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                u32_at(e, o_ea)
            } else {
                0
            };
            let file_id = extd.then(|| {
                let lo = u64_at(e, o_id);
                let hi = u64_at(e, o_id + 8);
                // NTFS ids fit in 64 bits; fold wider (ReFS) ids so they stay distinct.
                FileId {
                    volume: serial,
                    index: lo ^ hi.rotate_left(32),
                }
            });
            let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
            let kind = if tag & TAG_NAME_SURROGATE != 0 {
                NodeKind::Link
            } else if is_dir {
                NodeKind::Dir
            } else {
                NodeKind::File
            };
            let flags = attr_flags(attrs);
            let name = OsString::from_wide(&name16);
            out.push(Entry {
                name: name.clone(),
                meta: EntryMeta {
                    name,
                    kind,
                    logical_size: i64_at(e, offset_of!(FILE_FULL_DIR_INFO, EndOfFile)).max(0)
                        as u64,
                    allocated_size: i64_at(e, offset_of!(FILE_FULL_DIR_INFO, AllocationSize)).max(0)
                        as u64,
                    modified: filetime_to_unix(i64_at(
                        e,
                        offset_of!(FILE_FULL_DIR_INFO, LastWriteTime),
                    )),
                    accessed: filetime_to_unix(i64_at(
                        e,
                        offset_of!(FILE_FULL_DIR_INFO, LastAccessTime),
                    )),
                    file_id,
                    link_count: 1,
                    cloud: None,
                    flags,
                },
                raw: RawAttrs {
                    win_attributes: attrs,
                    st_flags: 0,
                    is_dir,
                },
                // `dev` is unused on Windows; 1 marks a mount-point tag (junction or
                // volume mount point) for `stat_entry` to tell apart, then resets to 0.
                dev: u64::from(tag == IO_REPARSE_TAG_MOUNT_POINT),
            });
        }
        if next == 0 {
            break;
        }
        off += next;
    }
}

/// Per-file query for the true allocation size and hard-link count.
/// Opens with `FILE_READ_ATTRIBUTES` only, relative to the folder's handle: no data
/// access, no hydration. Cloud/offline placeholders aren't opened at all.
fn stat_entry(e: &mut Entry, dir: &OwnedHandle) {
    match e.meta.kind {
        NodeKind::File if e.raw.win_attributes & RECALL_ATTRS != 0 => {}
        NodeKind::File => {
            let Ok(h) = open_at(dir, &e.name, FILE_READ_ATTRIBUTES, false) else {
                // Keep the directory-index values; FILE_READ_ATTRIBUTES is almost
                // always granted, so this is rare (e.g. file deleted meanwhile).
                return;
            };
            let mut info = FILE_STANDARD_INFO::default();
            // SAFETY: `info` is a valid, writable FILE_STANDARD_INFO of the given size.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    h.as_raw_handle(),
                    FileStandardInfo,
                    (&mut info as *mut FILE_STANDARD_INFO).cast(),
                    size_of::<FILE_STANDARD_INFO>() as u32,
                )
            };
            if ok != 0 {
                e.meta.allocated_size = info.AllocationSize.max(0) as u64;
                e.meta.logical_size = info.EndOfFile.max(0) as u64;
                e.meta.link_count = info.NumberOfLinks.max(1);
            }
            if e.meta.file_id.is_none() {
                if let Ok(bh) = by_handle_info(&h) {
                    e.meta.file_id = Some(FileId {
                        volume: u64::from(bh.dwVolumeSerialNumber),
                        index: (u64::from(bh.nFileIndexHigh) << 32) | u64::from(bh.nFileIndexLow),
                    });
                }
            }
        }
        NodeKind::Link if e.dev == 1 => {
            if let Ok(h) = open_at(dir, &e.name, FILE_READ_ATTRIBUTES, false) {
                if is_volume_mount_point(&h) {
                    e.meta.flags |= EntryFlags::MOUNT_POINT;
                }
            }
        }
        _ => {}
    }
    e.dev = 0;
}

fn by_handle_info(h: &OwnedHandle) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `info` is a valid, writable BY_HANDLE_FILE_INFORMATION.
    if unsafe { GetFileInformationByHandle(h.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

/// Reads the reparse buffer (metadata, not file data) of an open reparse point.
fn reparse_data(h: &OwnedHandle) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 16 * 1024];
    let mut returned = 0u32;
    // SAFETY: output buffer is valid for `buf.len()` bytes; no input buffer; synchronous.
    let ok = unsafe {
        DeviceIoControl(
            h.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then(|| {
        buf.truncate(returned as usize);
        buf
    })
}

fn is_name_surrogate(h: &OwnedHandle) -> bool {
    reparse_data(h).is_some_and(|b| b.len() >= 4 && u32_at(&b, 0) & TAG_NAME_SURROGATE != 0)
}

/// A mount-point reparse tag is either a junction or a volume mount point; the
/// latter's substitute name is `\??\Volume{...}`.
fn is_volume_mount_point(h: &OwnedHandle) -> bool {
    let Some(b) = reparse_data(h) else {
        return false;
    };
    if b.len() < 16 || u32_at(&b, 0) != IO_REPARSE_TAG_MOUNT_POINT {
        return false;
    }
    let off = 16 + u16::from_le_bytes([b[8], b[9]]) as usize;
    let len = u16::from_le_bytes([b[10], b[11]]) as usize;
    let Some(name) = b.get(off..off + len) else {
        return false;
    };
    let prefix: Vec<u8> = r"\??\Volume{"
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    name.starts_with(&prefix)
}

fn is_remote_volume(path: &Path) -> bool {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut vol = vec![0u16; 1024];
    // SAFETY: `wide` is NUL-terminated; `vol` is writable for the given length.
    if unsafe { GetVolumePathNameW(wide.as_ptr(), vol.as_mut_ptr(), vol.len() as u32) } == 0 {
        return false;
    }
    // SAFETY: `vol` now holds a NUL-terminated path.
    unsafe { GetDriveTypeW(vol.as_ptr()) == DRIVE_REMOTE }
}

fn attr_flags(attrs: u32) -> EntryFlags {
    let mut f = EntryFlags::empty();
    for (bit, flag) in [
        (FILE_ATTRIBUTE_HIDDEN, EntryFlags::HIDDEN),
        (FILE_ATTRIBUTE_SYSTEM, EntryFlags::SYSTEM),
        (FILE_ATTRIBUTE_READONLY, EntryFlags::READONLY),
        (FILE_ATTRIBUTE_COMPRESSED, EntryFlags::COMPRESSED),
        (FILE_ATTRIBUTE_SPARSE_FILE, EntryFlags::SPARSE),
    ] {
        if attrs & bit != 0 {
            f |= flag;
        }
    }
    f
}

/// `\\?\` form of an absolute path, NUL-terminated, so long paths work.
pub(crate) fn verbatim(path: &Path) -> Vec<u16> {
    let s: Vec<u16> = path.as_os_str().encode_wide().collect();
    let w = |t: &str| t.encode_utf16().collect::<Vec<u16>>();
    let mut out = Vec::with_capacity(s.len() + 9);
    if s.starts_with(&w(r"\\?\")) || s.starts_with(&w(r"\\.\")) {
        out.extend_from_slice(&s);
    } else if s.starts_with(&w(r"\\")) {
        out.extend(w(r"\\?\UNC\"));
        out.extend_from_slice(&s[2..]);
    } else {
        out.extend(w(r"\\?\"));
        out.extend_from_slice(&s);
    }
    out.push(0);
    out
}

fn ft(t: windows_sys::Win32::Foundation::FILETIME) -> i64 {
    ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) as i64
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn i64_at(b: &[u8], o: usize) -> i64 {
    u64_at(b, o) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_at_opens_links_themselves_and_refuses_bad_names() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("real")).unwrap();
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(t.path().join("j"))
            .arg(t.path().join("real"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        let dir = open_meta(t.path(), FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES).unwrap();
        if made {
            // The junction itself opens (despite OBJ_DONT_REPARSE) and reads as a link...
            let h = open_at(&dir, OsStr::new("j"), FILE_READ_ATTRIBUTES, false).unwrap();
            let (attrs, tag) = attribute_tag(&h).unwrap();
            assert_ne!(attrs & FILE_ATTRIBUTE_REPARSE_POINT, 0);
            assert_eq!(tag, IO_REPARSE_TAG_MOUNT_POINT);
            // ...and is never entered as a folder.
            let entry = Entry {
                name: "j".into(),
                meta: EntryMeta {
                    name: "j".into(),
                    kind: NodeKind::Dir,
                    logical_size: 0,
                    allocated_size: 0,
                    modified: None,
                    accessed: None,
                    file_id: None,
                    link_count: 1,
                    cloud: None,
                    flags: EntryFlags::empty(),
                },
                raw: RawAttrs::default(),
                dev: 0,
            };
            assert!(open_child(&dir, OsStr::new("j"), &entry).is_err());
            assert!(open_child(&dir, OsStr::new("real"), &entry).is_ok());
        }
        for bad in ["", ".", "..", "a/b", "a:b", "real\\x", "a\0b"] {
            assert_eq!(
                open_at(&dir, OsStr::new(bad), FILE_READ_ATTRIBUTES, true)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput,
                "accepted {bad:?}"
            );
        }
    }
}
