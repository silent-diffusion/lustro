//! Unix directory listing with `openat`/`fdopendir`/`fstatat`, plus
//! filesystem-boundary checks with `statfs`.
//!
//! Folders are opened with `O_NOFOLLOW | O_DIRECTORY`. A parent task checks it can
//! open each subfolder (relative to its own descriptor) and records the subfolder's
//! identity (`st_dev`, `st_ino`). The subfolder's task then reopens it by path and
//! refuses to list it unless the identity matches. So if any ancestor is swapped for
//! a symlink mid-scan, the walk can't leave the root. Entries are `fstatat`-ed
//! relative to the listed folder's descriptor with `AT_SYMLINK_NOFOLLOW`. No
//! descriptor is held between the two steps, so wide trees can't exhaust file
//! descriptors.

// OS API calls need `unsafe`; each block is small and has a SAFETY note.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use lustro_core::model::{EntryFlags, EntryMeta, FileId, NodeKind};
use lustro_core::scan::{RawAttrs, ScanOptions, SkipReason};

use super::Entry;

/// An open folder being listed.
pub(crate) type DirHandle = OwnedFd;
/// Identity (`st_dev`, `st_ino`) a folder must still have when its task opens it.
pub(crate) struct Pending((u64, u64));

pub(crate) struct Volume;

const DIR_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

fn c_name(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

fn owned_fd(fd: libc::c_int) -> io::Result<OwnedFd> {
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a new descriptor that we own and close exactly once.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_path(path: &Path) -> io::Result<OwnedFd> {
    let c = c_name(path.as_os_str())?;
    // SAFETY: `c` is a valid C string for the call.
    owned_fd(unsafe { libc::open(c.as_ptr(), DIR_FLAGS) })
}

fn open_in(dir: &OwnedFd, name: &OsStr) -> io::Result<OwnedFd> {
    let c = c_name(name)?;
    // SAFETY: `dir` is an open descriptor; `c` is a valid C string.
    owned_fd(unsafe { libc::openat(dir.as_raw_fd(), c.as_ptr(), DIR_FLAGS) })
}

fn fstat(fd: &OwnedFd) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fd` is open; `st` is writable and initialised on success.
    if unsafe { libc::fstat(fd.as_raw_fd(), st.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat returned 0, so it filled `st`.
    Ok(unsafe { st.assume_init() })
}

fn changed() -> io::Error {
    io::Error::other("folder changed during the scan")
}

pub(crate) fn root(path: &Path) -> io::Result<(Entry, Volume)> {
    let c = c_name(path.as_os_str())?;
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `c` is a valid C string; `st` is writable and initialised on success.
    if unsafe { libc::lstat(c.as_ptr(), st.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: lstat returned 0, so it filled `st`.
    let st = unsafe { st.assume_init() };
    Ok((entry_from(OsString::new(), &st), Volume))
}

/// Checks the root opens as a real directory and records its identity.
pub(crate) fn open_root_dir(path: &Path) -> io::Result<Pending> {
    let fd = open_path(path)?;
    Ok(Pending(identity(&fstat(&fd)?)))
}

/// Checks subfolder `name` opens (relative to `parent`, not following links) and is
/// still the folder that was listed; records its identity for its own task.
pub(crate) fn open_child(parent: &DirHandle, name: &OsStr, entry: &Entry) -> io::Result<Pending> {
    let fd = open_in(parent, name)?;
    let id = identity(&fstat(&fd)?);
    if entry
        .meta
        .file_id
        .is_some_and(|f| (f.volume, f.index) != id)
    {
        return Err(changed());
    }
    Ok(Pending(id))
}

/// Reopens a folder by path for listing and refuses it if it isn't the one its
/// parent checked (an ancestor was swapped for a link, say).
pub(crate) fn activate(pending: Pending, path: &Path) -> io::Result<DirHandle> {
    let fd = open_path(path)?;
    if identity(&fstat(&fd)?) != pending.0 {
        return Err(changed());
    }
    Ok(fd)
}

/// Owns a `DIR*` from `fdopendir`.
struct Dir(*mut libc::DIR);

impl Drop for Dir {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful fdopendir and is closed once.
        unsafe { libc::closedir(self.0) };
    }
}

pub(crate) fn read_dir(handle: &DirHandle, _dir: &Path, _vol: &Volume) -> io::Result<Vec<Entry>> {
    // fdopendir takes ownership of the descriptor it's given, so give it a duplicate.
    // SAFETY: `handle` is open; F_DUPFD_CLOEXEC returns a new descriptor or -1.
    let dup = owned_fd(unsafe { libc::fcntl(handle.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) })?;
    // SAFETY: `dup` is a valid directory descriptor; on success the DIR owns it.
    let dirp = unsafe { libc::fdopendir(dup.as_raw_fd()) };
    if dirp.is_null() {
        return Err(io::Error::last_os_error());
    }
    std::mem::forget(dup);
    let dir = Dir(dirp);

    let mut out = Vec::new();
    loop {
        // SAFETY: `dir.0` is a valid DIR* used only by this thread.
        let ent = unsafe { libc::readdir(dir.0) };
        if ent.is_null() {
            break;
        }
        // SAFETY: readdir returned a valid dirent whose d_name is NUL-terminated and
        // lives until the next readdir call; we copy it out right away.
        let (name, d_type) = unsafe {
            (
                CStr::from_ptr((*ent).d_name.as_ptr()).to_bytes().to_vec(),
                (*ent).d_type,
            )
        };
        if name == b"." || name == b".." {
            continue;
        }
        let name = OsString::from(OsStr::from_bytes(&name));
        let c = c_name(&name)?;
        let mut st = MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `handle` is the listed folder; `c` is valid; `st` is writable.
        let rc = unsafe {
            libc::fstatat(
                handle.as_raw_fd(),
                c.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc == 0 {
            // SAFETY: fstatat returned 0, so it filled `st`.
            out.push(entry_from(name, &unsafe { st.assume_init() }));
        } else {
            out.push(unreadable(name, d_type, io::Error::last_os_error()));
        }
    }
    Ok(out)
}

/// An entry whose metadata couldn't be read. Folders among them aren't entered.
fn unreadable(name: OsString, d_type: u8, e: io::Error) -> Entry {
    let kind = match d_type {
        libc::DT_LNK => NodeKind::Link,
        libc::DT_DIR => NodeKind::Dir,
        libc::DT_REG => NodeKind::File,
        _ => NodeKind::Other,
    };
    let mut flags = hidden_flag(&name);
    if e.kind() == io::ErrorKind::PermissionDenied {
        flags |= EntryFlags::ACCESS_DENIED;
    }
    // Without metadata we can't check boundaries, so don't enter it.
    if kind == NodeKind::Dir {
        flags |= EntryFlags::NOT_DESCENDED;
    }
    Entry {
        name: name.clone(),
        meta: EntryMeta {
            name,
            kind,
            logical_size: 0,
            allocated_size: 0,
            modified: None,
            accessed: None,
            file_id: None,
            link_count: 1,
            cloud: None,
            flags,
        },
        raw: RawAttrs {
            is_dir: kind == NodeKind::Dir,
            ..RawAttrs::default()
        },
        dev: 0,
    }
}

// `stat` field widths differ between Linux and macOS, so some casts are no-ops on one
// of them.
#[allow(clippy::unnecessary_cast)]
fn identity(st: &libc::stat) -> (u64, u64) {
    (st.st_dev as u64, st.st_ino as u64)
}

#[allow(clippy::unnecessary_cast)]
fn entry_from(name: OsString, st: &libc::stat) -> Entry {
    let kind = match (st.st_mode as u32) & (libc::S_IFMT as u32) {
        m if m == libc::S_IFLNK as u32 => NodeKind::Link,
        m if m == libc::S_IFDIR as u32 => NodeKind::Dir,
        m if m == libc::S_IFREG as u32 => NodeKind::File,
        _ => NodeKind::Other,
    };
    let st_flags = st_flags(st);
    let mut flags = hidden_flag(&name);
    #[cfg(target_os = "macos")]
    if st_flags & crate::macos::UF_HIDDEN != 0 {
        flags |= EntryFlags::HIDDEN;
    }
    if (st.st_mode as u32) & 0o222 == 0 {
        flags |= EntryFlags::READONLY;
    }
    let len = st.st_size.max(0) as u64;
    let allocated = (st.st_blocks.max(0) as u64).saturating_mul(512);
    if kind == NodeKind::File && allocated < len {
        flags |= EntryFlags::SPARSE;
    }
    let (dev, ino) = identity(st);
    Entry {
        name: name.clone(),
        meta: EntryMeta {
            name,
            kind,
            logical_size: len,
            allocated_size: allocated,
            modified: Some(st.st_mtime as i64),
            accessed: Some(st.st_atime as i64),
            file_id: Some(FileId {
                volume: dev,
                index: ino,
            }),
            link_count: (st.st_nlink as u64).clamp(1, u64::from(u32::MAX)) as u32,
            cloud: None,
            flags,
        },
        raw: RawAttrs {
            win_attributes: 0,
            st_flags,
            is_dir: kind == NodeKind::Dir,
        },
        dev,
    }
}

#[cfg(target_os = "macos")]
fn st_flags(st: &libc::stat) -> u32 {
    st.st_flags
}

#[cfg(not(target_os = "macos"))]
fn st_flags(_: &libc::stat) -> u32 {
    0
}

fn hidden_flag(name: &std::ffi::OsStr) -> EntryFlags {
    if name.as_bytes().first() == Some(&b'.') {
        EntryFlags::HIDDEN
    } else {
        EntryFlags::empty()
    }
}

/// Pseudo filesystems, mount points, remote mounts.
pub(crate) fn boundary(
    path: &Path,
    entry: &Entry,
    parent_dev: u64,
    opts: &ScanOptions,
) -> (EntryFlags, Option<SkipReason>) {
    if cfg!(any(target_os = "linux", target_os = "android"))
        && ["/proc", "/sys", "/dev", "/run"]
            .iter()
            .any(|p| path == Path::new(p))
    {
        return (EntryFlags::empty(), Some(SkipReason::PseudoFilesystem));
    }
    if entry.dev == parent_dev {
        return (EntryFlags::empty(), None);
    }
    let mut flags = EntryFlags::MOUNT_POINT;
    match fs_class(path) {
        FsClass::Pseudo => return (flags, Some(SkipReason::PseudoFilesystem)),
        FsClass::Remote => {
            flags |= EntryFlags::REMOTE;
            if !opts.include_remote {
                return (flags, Some(SkipReason::Remote));
            }
        }
        FsClass::Local => {}
    }
    if !opts.cross_filesystems {
        return (flags, Some(SkipReason::OtherFilesystem));
    }
    (flags, None)
}

#[derive(Debug, PartialEq, Eq)]
enum FsClass {
    Local,
    Remote,
    Pseudo,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn fs_class(path: &Path) -> FsClass {
    // Magic numbers from linux/magic.h and statfs(2).
    const REMOTE: &[u32] = &[
        0x6969,      // NFS
        0x517B,      // SMB
        0xFF53_4D42, // CIFS
        0xFE53_4D42, // SMB2
        0x6573_5546, // FUSE (sshfs, rclone, ...)
        0x7375_7245, // CODA
        0x5346_414F, // AFS
        0x00C3_6400, // CEPH
        0x0102_1997, // 9P
        0x564C,      // NCP
    ];
    const PSEUDO: &[u32] = &[
        0x9FA0,      // proc
        0x6265_6572, // sysfs
        0x1CD1,      // devpts
        0x0027_E0EB, // cgroup
        0x6367_7270, // cgroup2
        0x6462_6720, // debugfs
        0x7363_6673, // securityfs
        0x7472_6163, // tracefs
        0xCAFE_4A11, // bpf
        0x6165_676C, // pstore
        0x6265_6570, // configfs
        0x6573_5543, // fusectl
        0x1980_0202, // mqueue
        0x4249_4E4D, // binfmt_misc
        0x0187,      // autofs
        0xDE5E_81E4, // efivarfs
        0x6E73_6673, // nsfs
        0xF97C_FF8C, // selinuxfs
    ];
    let Some(st) = statfs(path) else {
        return FsClass::Local;
    };
    // f_type's width varies by architecture; the magic numbers are 32-bit.
    let magic = st.f_type as u32;
    if REMOTE.contains(&magic) {
        FsClass::Remote
    } else if PSEUDO.contains(&magic) {
        FsClass::Pseudo
    } else {
        FsClass::Local
    }
}

#[cfg(target_vendor = "apple")]
fn fs_class(path: &Path) -> FsClass {
    let Some(st) = statfs(path) else {
        return FsClass::Local;
    };
    // SAFETY: f_fstypename is a NUL-terminated C string filled in by statfs.
    let name = unsafe { std::ffi::CStr::from_ptr(st.f_fstypename.as_ptr()) }.to_bytes();
    if matches!(name, b"devfs" | b"autofs") {
        FsClass::Pseudo
    } else if st.f_flags & (libc::MNT_LOCAL as u32) == 0
        || name.starts_with(b"macfuse")
        || name.starts_with(b"osxfuse")
        || name.starts_with(b"fuse")
    {
        FsClass::Remote
    } else {
        FsClass::Local
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
fn fs_class(_: &Path) -> FsClass {
    FsClass::Local
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn statfs(path: &Path) -> Option<libc::statfs> {
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `c` is a valid C string; `st` is writable and initialised on success.
    let rc = unsafe { libc::statfs(c.as_ptr(), st.as_mut_ptr()) };
    // SAFETY: statfs returned 0, so it filled `st`.
    (rc == 0).then(|| unsafe { st.assume_init() })
}
