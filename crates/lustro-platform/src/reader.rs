//! `lustro_core::insights::FileReader` for duplicate hashing: reads only file data that is
//! already on this disk, never follows links, and leaves "last opened" times alone where
//! the OS allows it (they drive the stale-file suggestions).

use std::fs::{File, Metadata};
use std::io;
use std::path::Path;

use lustro_core::insights::{FileReader, ReadSeek};

/// Use this instead of `lustro_core::insights::StdFileReader`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformFileReader;

fn not_local() -> io::Error {
    io::Error::other("file data is not on this disk")
}

/// A plain file whose data is on this disk: not a link, not cloud-only, recall-on-open,
/// recall-on-data-access or offline (Windows), not dataless (macOS).
fn local_file(md: &Metadata) -> bool {
    md.is_file() && os::data_is_local(md)
}

impl FileReader for PlatformFileReader {
    fn is_local(&self, path: &Path) -> io::Result<bool> {
        // symlink_metadata reads attributes only and never follows links.
        Ok(local_file(&std::fs::symlink_metadata(path)?))
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        if !self.is_local(path)? {
            return Err(not_local());
        }
        let f = os::open(path)?;
        // Again on the open handle, in case the file changed in between.
        if !local_file(&f.metadata()?) {
            return Err(not_local());
        }
        Ok(Box::new(io::BufReader::with_capacity(256 * 1024, f)))
    }
}

#[cfg(windows)]
mod os {
    use super::*;
    use lustro_cloud::rules::win;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        SetFileTime, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_SEQUENTIAL_SCAN, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_WRITE_ATTRIBUTES, SYNCHRONIZE,
    };

    pub(super) fn data_is_local(md: &Metadata) -> bool {
        md.file_attributes() & win::CLOUD_ONLY == 0
    }

    /// Opens for reading with OPEN_REPARSE_POINT (a link is never followed) and
    /// FILE_WRITE_ATTRIBUTES, which is needed only to switch off last-access updates on
    /// this handle before anything is read. If that can't be done the file is skipped
    /// rather than read with its "last opened" time changing.
    #[allow(unsafe_code)]
    pub(super) fn open(path: &Path) -> io::Result<File> {
        let f = std::fs::OpenOptions::new()
            .access_mode(
                FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0 | SYNCHRONIZE.0,
            )
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_SEQUENTIAL_SCAN.0)
            .open(path)?;
        // 0xFFFFFFFF in both halves: reads through this handle don't update the time.
        let keep = FILETIME {
            dwLowDateTime: u32::MAX,
            dwHighDateTime: u32::MAX,
        };
        // SAFETY: `f` owns a valid open handle for the whole call; only the last-access
        // argument is passed, and it points to a live FILETIME.
        unsafe { SetFileTime(HANDLE(f.as_raw_handle()), None, Some(&keep), None) }
            .map_err(|_| io::Error::other("couldn't keep the file's last-opened time"))?;
        Ok(f)
    }
}

#[cfg(unix)]
mod os {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;

    #[cfg(target_os = "macos")]
    pub(super) fn data_is_local(md: &Metadata) -> bool {
        use std::os::macos::fs::MetadataExt;
        md.st_flags() & lustro_cloud::rules::mac::SF_DATALESS == 0
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn data_is_local(_md: &Metadata) -> bool {
        true
    }

    /// O_NOFOLLOW: a link is never followed. On Linux, O_NOATIME keeps access times; it's
    /// only allowed on the user's own files, so other files are opened without it (most
    /// systems mount with `relatime`, which updates at most once a day). macOS has no
    /// equivalent flag.
    pub(super) fn open(path: &Path) -> io::Result<File> {
        let open = |flags: i32| {
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | flags)
                .open(path)
        };
        #[cfg(any(target_os = "linux", target_os = "android"))]
        match open(libc::O_NOATIME) {
            Err(e) if e.raw_os_error() == Some(libc::EPERM) => {}
            other => return other,
        }
        open(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn reads_a_local_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("dup.txt");
        std::fs::write(&f, b"same bytes").unwrap();
        let r = PlatformFileReader;
        assert!(r.is_local(&f).unwrap());
        let mut s = String::new();
        r.open(&f).unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "same bytes");
    }

    #[test]
    fn folders_and_missing_files_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let r = PlatformFileReader;
        assert!(!r.is_local(dir.path()).unwrap());
        assert!(r.open(dir.path()).is_err());
        assert!(r.open(&dir.path().join("gone")).is_err());
    }

    /// Reading must not move the last-access time.
    #[cfg(windows)]
    #[test]
    fn last_access_time_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("old.txt");
        std::fs::write(&f, vec![7u8; 4096]).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_accessed(old))
            .unwrap();
        let mut buf = Vec::new();
        PlatformFileReader
            .open(&f)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf.len(), 4096);
        assert_eq!(std::fs::metadata(&f).unwrap().accessed().unwrap(), old);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let j = dir.path().join("j");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&j)
            .arg(&real)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok);
        assert!(!PlatformFileReader.is_local(&j).unwrap());
        assert!(PlatformFileReader.open(&j).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f");
        std::fs::write(&f, b"x").unwrap();
        let l = dir.path().join("l");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        assert!(!PlatformFileReader.is_local(&l).unwrap());
        assert!(PlatformFileReader.open(&l).is_err());
    }
}
