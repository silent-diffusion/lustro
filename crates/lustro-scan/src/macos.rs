//! macOS support.
//!
//! Today macOS scans use the portable [`crate::Walker`], with one safety net: every
//! scan thread turns off materialisation of dataless (cloud-only) files and folders,
//! so even an unexpected `stat` or `readdir` fails instead of starting a download.
//!
//! TODO(macos fast path): a `getattrlistbulk(2)` scanner. It returns name, type,
//! `ATTR_CMN_FLAGS` (for `SF_DATALESS`), file id, link count, dates and
//! `ATTR_FILE_ALLOCSIZE` for a whole directory in a few syscalls, without a
//! per-entry `lstat`. Plan:
//! - open each directory with `O_RDONLY | O_DIRECTORY | O_NOFOLLOW`;
//! - request `ATTR_CMN_RETURNED_ATTRS | NAME | OBJTYPE | FLAGS | FILEID | MODTIME |
//!   ACCTIME | DEVID` plus `ATTR_FILE_LINKCOUNT | ATTR_FILE_DATALENGTH | ATTR_FILE_ALLOCSIZE`;
//! - check `SF_DATALESS` via `CloudClassifier::should_descend` before opening a child folder;
//! - plug it in as `ScanMode::Fast` in `SystemScanner`, reusing the walker's task
//!   structure and `Emitter`.
//!
//! Also to handle there: firmlinks (`/System/Volumes/Data`) so scanning `/` doesn't
//! stop at the Data volume boundary or count it twice.

// OS API calls need `unsafe`; each block is small and has a SAFETY note.
#![allow(unsafe_code)]

/// `UF_HIDDEN` from `<sys/stat.h>`: hidden in Finder.
pub(crate) const UF_HIDDEN: u32 = 0x0000_8000;

// From <sys/resource.h>.
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: libc::c_int = 3;
const IOPOL_SCOPE_THREAD: libc::c_int = 1;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: libc::c_int = 1;

extern "C" {
    fn setiopolicy_np(iotype: libc::c_int, scope: libc::c_int, policy: libc::c_int) -> libc::c_int;
}

/// Makes the calling thread unable to trigger downloads of dataless files: access
/// fails with `EDEADLK` instead. Called on every scan thread.
pub(crate) fn forbid_dataless_materialization() {
    // SAFETY: plain syscall wrapper with integer arguments; affects only this thread.
    unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_THREAD,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        );
    }
}
