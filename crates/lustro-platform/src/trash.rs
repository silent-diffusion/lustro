//! Move to Recycle Bin / Trash (the only destructive operation), and "free up space" for
//! downloaded cloud files.
//!
//! `lustro-core`'s cleanup module calls this through a trait it owns; [`PlatformTrash`] has the
//! same three methods so the `impl` is a thin wrapper.

use std::path::{Component, Path, PathBuf};

use lustro_cloud::{Classifier, SyncRootKind};
use lustro_core::dto::ItemIdentity;
use lustro_core::model::FileId;

use crate::protected::ProtectedPaths;

/// Errors carry no paths, so they're safe to log.
#[derive(Debug, thiserror::Error)]
pub enum TrashError {
    #[error("protected: {0}")]
    Protected(String),
    #[error("path must be absolute and must not contain '..'")]
    BadPath,
    /// A Windows name that could stand for a different item (trailing dot or space,
    /// stream name, device name).
    #[error("this name can't be safely moved to the trash")]
    UnsafeName,
    #[error("item not found")]
    NotFound,
    #[error("not supported: {0}")]
    Unsupported(&'static str),
    /// The item's drive has no usable Recycle Bin / Trash, so deleting would be permanent.
    #[error("This drive has no Recycle Bin, so Lustro won't delete from it. Use Move instead.")]
    NoRecycleBin,
    /// The item at the path is no longer the one the user reviewed.
    #[error("changed since review")]
    Changed,
    #[error("the system refused to move the item to the trash")]
    TrashFailed,
    #[error("I/O error: {0}")]
    Io(std::io::ErrorKind),
}

/// Result of [`PlatformTrash::free_up_space`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeUpOutcome {
    /// The local copy was removed; the file is now cloud-only.
    Dehydrated,
    /// Marked "free up space"; the provider removes the local copy shortly.
    Requested,
    /// Nothing to do: it was already cloud-only.
    AlreadyCloudOnly,
}

pub struct PlatformTrash {
    protected: ProtectedPaths,
    cloud: Classifier,
}

impl Default for PlatformTrash {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTrash {
    /// Uses this OS's default protected list and discovered sync roots.
    pub fn new() -> Self {
        Self {
            protected: ProtectedPaths::for_current_os(),
            cloud: Classifier::detect(),
        }
    }

    pub fn with(protected: ProtectedPaths, cloud: Classifier) -> Self {
        Self { protected, cloud }
    }

    pub fn protected_paths(&self) -> &ProtectedPaths {
        &self.protected
    }

    /// `Some(reason)` when cleanup must not touch `path`.
    pub fn protected_reason(&self, path: &Path) -> Option<String> {
        match self.precheck(path) {
            Err(TrashError::Protected(r)) => Some(r),
            Err(TrashError::BadPath | TrashError::UnsafeName) => {
                Some("Not a valid absolute path.".into())
            }
            Err(_) => None,
            Ok(()) => os::final_check(path, &self.protected)
                .err()
                .and_then(|e| match e {
                    TrashError::Protected(r) => Some(r),
                    TrashError::UnsafeName => Some("Not a valid absolute path.".into()),
                    _ => None,
                }),
        }
    }

    /// Checks on the path as written: shape, names, protected list (parents resolved).
    fn precheck(&self, path: &Path) -> Result<(), TrashError> {
        validate(path)?;
        if cfg!(windows) {
            // Network paths are refused from the text alone, before anything touches them.
            if !crate::recycle::is_drive_letter_path(path) {
                return Err(TrashError::NoRecycleBin);
            }
            if crate::recycle::has_unsafe_windows_name(path) {
                return Err(TrashError::UnsafeName);
            }
        }
        if let Some(r) = self.protected.check_resolved(path) {
            return Err(TrashError::Protected(r.reason));
        }
        Ok(())
    }

    /// Moves `path` to the Recycle Bin / Trash / freedesktop trash. Never deletes
    /// permanently: where there's no usable trash it fails with
    /// [`TrashError::NoRecycleBin`].
    pub fn move_to_trash(&self, path: &Path) -> Result<(), TrashError> {
        self.trash_final(path, None).map(drop)
    }

    /// Like [`move_to_trash`](Self::move_to_trash), but only if the item is still
    /// `expected`. On Windows the comparison is made on a metadata handle that stays open
    /// until the delete has run. The shell still re-opens the item by path (our handle
    /// allows that with FILE_SHARE_DELETE), so a same-user process could rename it away in
    /// that tiny window; whatever is then deleted still only goes to the Recycle Bin.
    pub fn move_to_trash_checked(
        &self,
        path: &Path,
        expected: &ItemIdentity,
    ) -> Result<(), TrashError> {
        self.trash_final(path, Some(expected)).map(drop)
    }

    /// Trashes and returns the path the item was actually trashed from (on Windows the
    /// long final path: no 8.3 names, no `\\?\`) and, on Windows, its file id, which the
    /// Recycle Bin keeps (the item is renamed into the bin on the same volume). Undo uses
    /// both to find exactly this item again.
    pub(crate) fn trash_final(
        &self,
        path: &Path,
        expected: Option<&ItemIdentity>,
    ) -> Result<(PathBuf, Option<FileId>), TrashError> {
        self.precheck(path)?;
        // symlink_metadata: a link is trashed as a link, never followed.
        std::fs::symlink_metadata(path).map_err(io_err)?;
        os::trash(path, &self.protected, expected)
    }

    /// Returns a downloaded cloud file (or folder, recursively) to cloud-only. Only for
    /// placeholder sync roots; the provider still holds the data.
    pub fn free_up_space(&self, path: &Path) -> Result<FreeUpOutcome, TrashError> {
        validate(path)?;
        let meta = std::fs::symlink_metadata(path).map_err(io_err)?;
        if meta.file_type().is_symlink() {
            return Err(TrashError::Unsupported("links can't be freed up"));
        }
        match self.cloud.sync_root_of(path) {
            Some((_, SyncRootKind::Placeholder)) => {}
            _ => return Err(TrashError::Unsupported("not in a cloud sync folder")),
        }
        os::free_up_space(path, &meta)
    }
}

fn validate(path: &Path) -> Result<(), TrashError> {
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(TrashError::BadPath);
    }
    Ok(())
}

/// `path` with its parent canonicalized (links in parent folders resolved, the item
/// itself not followed).
#[cfg(not(windows))]
fn resolved(path: &Path) -> Result<std::path::PathBuf, TrashError> {
    let parent = path.parent().ok_or(TrashError::BadPath)?;
    let name = path.file_name().ok_or(TrashError::BadPath)?;
    Ok(std::fs::canonicalize(parent).map_err(io_err)?.join(name))
}

pub(crate) fn io_err(e: std::io::Error) -> TrashError {
    if e.kind() == std::io::ErrorKind::NotFound {
        TrashError::NotFound
    } else {
        TrashError::Io(e.kind())
    }
}

#[cfg(windows)]
mod os {
    use super::*;
    use lustro_cloud::rules::win;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::CloudFilters::{
        CfDehydratePlaceholder, CfGetPlaceholderStateFromAttributeTag, CfSetPinState,
        CF_DEHYDRATE_FLAG_NONE, CF_PIN_STATE_UNPINNED, CF_PLACEHOLDER_STATE_INVALID,
        CF_PLACEHOLDER_STATE_PLACEHOLDER, CF_SET_PIN_FLAG_NONE, CF_SET_PIN_FLAG_RECURSE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_TAG_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING,
    };

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

    /// Recycle Bin only: the real volume must have a bin, and the delete is cancelled if
    /// the shell would delete permanently anyway.
    ///
    /// The protected check runs again on the item's final path from an open handle (long
    /// names instead of 8.3, links in parents resolved), and exactly that path goes to
    /// the shell while the handle stays open. The shell re-opens it by path, so a rename
    /// in that tiny window isn't prevented; the Recycle Bin guard still applies.
    pub(super) fn trash(
        path: &Path,
        protected: &ProtectedPaths,
        expected: Option<&ItemIdentity>,
    ) -> Result<(PathBuf, Option<FileId>), TrashError> {
        let item = crate::recycle::open_item(path)?;
        let now = item.identity().map_err(io_err)?;
        if expected.is_some_and(|want| !want.still_matches(&now)) {
            return Err(TrashError::Changed);
        }
        checked_final(&item.path, protected)?;
        crate::recycle::check_volume(&item)?;
        let result = crate::recycle::recycle(&item.path);
        let full = item.path.clone();
        drop(item);
        result.map(|()| (full, now.file_id))
    }

    /// The checks on the final path, for [`PlatformTrash::protected_reason`] too.
    pub(super) fn final_check(path: &Path, protected: &ProtectedPaths) -> Result<(), TrashError> {
        match crate::recycle::open_item(path) {
            Ok(item) => checked_final(&item.path, protected),
            Err(_) => Ok(()),
        }
    }

    fn checked_final(full: &Path, protected: &ProtectedPaths) -> Result<(), TrashError> {
        if !crate::recycle::is_drive_letter_path(full) {
            return Err(TrashError::NoRecycleBin);
        }
        if crate::recycle::has_unsafe_windows_name(full) {
            return Err(TrashError::UnsafeName);
        }
        match protected.check(full) {
            Some(r) => Err(TrashError::Protected(r.reason)),
            None => Ok(()),
        }
    }

    #[allow(unsafe_code)]
    pub(super) fn free_up_space(
        path: &Path,
        meta: &std::fs::Metadata,
    ) -> Result<FreeUpOutcome, TrashError> {
        let attrs = meta.file_attributes();
        let is_dir = meta.is_dir();
        if !is_dir && attrs & win::CLOUD_ONLY != 0 {
            return Ok(FreeUpOutcome::AlreadyCloudOnly);
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Attribute-only access: opening this way never reads data, so it can't hydrate.
        // OPEN_REPARSE_POINT opens the placeholder itself rather than going through recall.
        // SAFETY: `wide` is NUL-terminated and outlives the call; the returned handle is
        // owned by `Handle` and closed on drop.
        let h = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                (FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(|_| TrashError::Unsupported("the cloud provider refused the request"))?;
        let h = Handle(h);
        // Only real Cloud Files placeholders: pinning an ordinary file would just add an
        // attribute to it, which we never want.
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
        .map_err(|e| TrashError::Io(std::io::Error::from(e).kind()))?;
        // SAFETY: pure function of two integers.
        let state =
            unsafe { CfGetPlaceholderStateFromAttributeTag(tag.FileAttributes, tag.ReparseTag) };
        if state == CF_PLACEHOLDER_STATE_INVALID
            || state.0 & CF_PLACEHOLDER_STATE_PLACEHOLDER.0 == 0
        {
            return Err(TrashError::Unsupported("not a cloud placeholder"));
        }
        let flags = if is_dir {
            CF_SET_PIN_FLAG_RECURSE
        } else {
            CF_SET_PIN_FLAG_NONE
        };
        // SAFETY: valid open handle; synchronous call (no OVERLAPPED).
        unsafe { CfSetPinState(h.0, CF_PIN_STATE_UNPINNED, flags, None) }
            .map_err(|_| TrashError::Unsupported("not a cloud placeholder"))?;
        if is_dir {
            return Ok(FreeUpOutcome::Requested);
        }
        // Best effort: drop the local data now. If the file is in use the provider does it
        // later because the file is now unpinned.
        // SAFETY: valid open handle; offset 0 and length -1 mean the whole file.
        match unsafe { CfDehydratePlaceholder(h.0, 0, -1, CF_DEHYDRATE_FLAG_NONE, None) } {
            Ok(()) => Ok(FreeUpOutcome::Dehydrated),
            Err(_) => Ok(FreeUpOutcome::Requested),
        }
    }
}

#[cfg(not(windows))]
mod os {
    use super::*;

    /// macOS: `NSFileManager trashItem` only. The default Finder method can offer to
    /// delete immediately on volumes without a Trash; this one just fails there.
    /// Parents are already resolved by `check_resolved`; nothing more to check.
    pub(super) fn final_check(_path: &Path, _protected: &ProtectedPaths) -> Result<(), TrashError> {
        Ok(())
    }

    /// No handle-based delete here: compare right before trashing.
    fn still_expected(path: &Path, expected: Option<&ItemIdentity>) -> Result<(), TrashError> {
        let Some(want) = expected else {
            return Ok(());
        };
        match crate::fileops::stat_identity(path).map_err(io_err)? {
            Some(now) if want.still_matches(&now) => Ok(()),
            Some(_) => Err(TrashError::Changed),
            None => Err(TrashError::NotFound),
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn trash(
        path: &Path,
        _protected: &ProtectedPaths,
        expected: Option<&ItemIdentity>,
    ) -> Result<(PathBuf, Option<FileId>), TrashError> {
        still_expected(path, expected)?;
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut ctx = trash::TrashContext::default();
        ctx.set_delete_method(DeleteMethod::NsFileManager);
        let full = resolved(path)?;
        ctx.delete(&full).map_err(|_| TrashError::TrashFailed)?;
        Ok((full, None))
    }

    /// Linux and other Unix: only when the freedesktop trash for the item's mount is
    /// usable (the home trash on the home mount, else `.Trash/$uid` or `.Trash-$uid`).
    /// The `trash` crate never unlinks: it renames, or copies then removes the source.
    #[cfg(not(target_os = "macos"))]
    pub(super) fn trash(
        path: &Path,
        _protected: &ProtectedPaths,
        expected: Option<&ItemIdentity>,
    ) -> Result<(PathBuf, Option<FileId>), TrashError> {
        still_expected(path, expected)?;
        let full = resolved(path)?;
        let mounts = lustro_cloud::system_mounts();
        let points: Vec<&str> = mounts.iter().map(|m| m.mount_point.as_str()).collect();
        let top = super::unix_trash::topdir(&full, &points);
        if let Some(m) = mounts.iter().find(|m| Some(m.mount_point.as_str()) == top) {
            if lustro_cloud::mounts::fs_skip_reason(&m.fs_type)
                == Some(lustro_core::scan::SkipReason::PseudoFilesystem)
            {
                return Err(TrashError::NoRecycleBin);
            }
        }
        let home_trash = super::unix_trash::home_trash().ok_or(TrashError::NoRecycleBin)?;
        let home_top =
            super::unix_trash::topdir(&super::unix_trash::existing(&home_trash), &points);
        let ok = top.is_some() && top == home_top
            || top.is_some_and(|t| {
                super::unix_trash::mount_trash_usable(Path::new(t), super::unix_trash::uid())
            });
        if !ok {
            return Err(TrashError::NoRecycleBin);
        }
        // The freedesktop trash may copy across file systems, so no file id here; its
        // .trashinfo records the real name, so the path alone finds the item.
        trash::delete(&full).map_err(|_| TrashError::TrashFailed)?;
        Ok((full, None))
    }

    pub(super) fn free_up_space(
        _path: &Path,
        _meta: &std::fs::Metadata,
    ) -> Result<FreeUpOutcome, TrashError> {
        // macOS: evicting needs NSFileManager (Objective-C); Linux has no placeholders.
        Err(TrashError::Unsupported(
            "freeing up cloud files isn't available on this OS yet",
        ))
    }
}

/// freedesktop trash checks (Linux and other non-macOS Unix).
#[cfg(all(unix, not(target_os = "macos")))]
mod unix_trash {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    /// The mount point holding `path`: the longest one it starts with.
    pub(super) fn topdir<'a>(path: &Path, mount_points: &[&'a str]) -> Option<&'a str> {
        mount_points
            .iter()
            .copied()
            .filter(|m| path.starts_with(m))
            .max_by_key(|m| Path::new(m).components().count())
    }

    /// `$XDG_DATA_HOME/Trash`, else `$HOME/.local/share/Trash`.
    pub(super) fn home_trash() -> Option<PathBuf> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| Path::new(v).is_absolute())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))?;
        Some(data.join("Trash"))
    }

    /// Nearest existing ancestor (the home trash may not exist yet).
    pub(super) fn existing(path: &Path) -> PathBuf {
        path.ancestors()
            .find(|p| p.exists())
            .and_then(|p| std::fs::canonicalize(p).ok())
            .unwrap_or_else(|| path.to_path_buf())
    }

    #[allow(unsafe_code)]
    pub(super) fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and can't fail.
        unsafe { libc::getuid() }
    }

    #[allow(unsafe_code)]
    fn writable(p: &Path) -> bool {
        let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c` is a NUL-terminated string that outlives the call.
        unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
    }

    /// A real directory, not a link to somewhere else.
    fn real_dir(p: &Path) -> Option<std::fs::Metadata> {
        std::fs::symlink_metadata(p)
            .ok()
            .filter(|m| m.file_type().is_dir())
    }

    /// The per-mount trash per the freedesktop spec: `$top/.Trash/$uid` (`.Trash` must be
    /// a sticky, non-link directory) or `$top/.Trash-$uid` (existing and writable, or
    /// creatable because the mount root is writable).
    pub(super) fn mount_trash_usable(top: &Path, uid: u32) -> bool {
        let admin = top.join(".Trash");
        if let Some(m) = real_dir(&admin) {
            if m.permissions().mode() & 0o1000 != 0 {
                let mine = admin.join(uid.to_string());
                if real_dir(&mine).is_some_and(|_| writable(&mine))
                    || (!mine.exists() && writable(&admin))
                {
                    return true;
                }
            }
        }
        let own = top.join(format!(".Trash-{uid}"));
        match std::fs::symlink_metadata(&own) {
            Ok(m) => m.file_type().is_dir() && writable(&own),
            Err(_) => writable(top),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn topdir_is_the_longest_containing_mount() {
            let mounts = ["/", "/home", "/media/william/usb", "/mediax"];
            assert_eq!(topdir(Path::new("/home/william/a"), &mounts), Some("/home"));
            assert_eq!(
                topdir(Path::new("/media/william/usb/b"), &mounts),
                Some("/media/william/usb")
            );
            assert_eq!(topdir(Path::new("/media/other"), &mounts), Some("/"));
        }

        #[test]
        fn mount_trash_needs_a_writable_place() {
            let dir = tempfile::tempdir().unwrap();
            assert!(mount_trash_usable(dir.path(), 4242));
            // Read-only mount root and no trash folder: nowhere to trash.
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
            let refused = !mount_trash_usable(dir.path(), 4242) || uid() == 0;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(refused);
            // A `.Trash-$uid` that is a link is never used.
            std::os::unix::fs::symlink("/tmp", dir.path().join(".Trash-4242")).unwrap();
            assert!(!mount_trash_usable(dir.path(), 4242));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protected::{Env, Scope};
    use lustro_cloud::rules::Os;
    use lustro_cloud::SyncRoot;
    use std::path::PathBuf;

    /// A trash with the default list for this OS, but no real sync roots.
    fn trash_with(extra_protected: Option<&Path>, roots: &[SyncRoot]) -> PlatformTrash {
        let mut p = ProtectedPaths::defaults(Os::current(), &Env::default());
        if let Some(x) = extra_protected {
            p.push(x, "test protection", Scope::Subtree);
        }
        PlatformTrash::with(p, Classifier::new(Os::current(), roots))
    }

    #[test]
    fn refuses_relative_and_dotdot_paths() {
        let t = trash_with(None, &[]);
        assert!(matches!(
            t.move_to_trash(Path::new("relative.txt")),
            Err(TrashError::BadPath)
        ));
        let sneaky = std::env::temp_dir().join("a").join("..").join("b");
        assert!(matches!(t.move_to_trash(&sneaky), Err(TrashError::BadPath)));
        assert!(t.protected_reason(Path::new("relative")).is_some());
    }

    #[test]
    fn refuses_protected_path_and_leaves_it_alone() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("keep.txt");
        std::fs::write(&file, b"fake data").unwrap();
        let t = trash_with(Some(dir.path()), &[]);
        let err = t.move_to_trash(&file).unwrap_err();
        assert!(matches!(err, TrashError::Protected(_)));
        assert!(!err.to_string().contains(&*dir.path().to_string_lossy()));
        assert!(file.exists());
        // Its parent folder contains a protected path too.
        assert!(t.protected_reason(dir.path().parent().unwrap()).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_protected_path_through_a_symlinked_parent() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("f"), b"x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let t = trash_with(Some(&std::fs::canonicalize(&real).unwrap()), &[]);
        assert!(matches!(
            t.move_to_trash(&link.join("f")),
            Err(TrashError::Protected(_))
        ));
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let t = trash_with(None, &[]);
        assert!(matches!(
            t.move_to_trash(&dir.path().join("nope.txt")),
            Err(TrashError::NotFound)
        ));
    }

    /// Moves one freshly created temp file to the real Recycle Bin / Trash.
    #[test]
    fn trashes_a_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lustro-trash-test.txt");
        std::fs::write(&file, b"Lustro trash test").unwrap();
        let t = trash_with(None, &[]);
        assert!(t.protected_reason(&file).is_none());
        t.move_to_trash(&file).unwrap();
        assert!(!file.exists());
    }

    /// A UNC path is refused from its text: the file doesn't exist, so reaching any file
    /// API would have given NotFound instead.
    #[cfg(windows)]
    #[test]
    fn unc_paths_are_refused_before_any_api_call() {
        let t = trash_with(None, &[]);
        for p in [
            r"\\localhost\c$\lustro-test-does-not-exist\a.txt",
            r"\\?\UNC\localhost\c$\lustro-test-does-not-exist\a.txt",
        ] {
            let err = t.move_to_trash(Path::new(p)).unwrap_err();
            assert!(matches!(err, TrashError::NoRecycleBin), "{err:?}");
            assert!(!err.to_string().contains("localhost"));
        }
    }

    /// Removes the `subst` drive on drop.
    #[cfg(windows)]
    struct Subst(char);
    #[cfg(windows)]
    impl Drop for Subst {
        fn drop(&mut self) {
            let _ = std::process::Command::new("subst")
                .arg(format!("{}:", self.0))
                .arg("/D")
                .output();
        }
    }

    /// A `subst` drive pointing at a fixed-disk temp folder resolves to that disk, so
    /// trashing through it is allowed (and really lands in the Recycle Bin).
    #[cfg(windows)]
    #[test]
    fn subst_drive_to_a_fixed_folder_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let letter = ('M'..='Y')
            .rev()
            .find(|c| !Path::new(&format!(r"{c}:\")).exists())
            .expect("a free drive letter");
        let ok = std::process::Command::new("subst")
            .arg(format!("{letter}:"))
            .arg(dir.path())
            .status()
            .unwrap()
            .success();
        assert!(ok, "subst failed");
        let _guard = Subst(letter);
        let via = PathBuf::from(format!(r"{letter}:\lustro-subst-test.txt"));
        std::fs::write(&via, b"Lustro subst test").unwrap();
        assert!(dir.path().join("lustro-subst-test.txt").exists());
        crate::recycle::check_volume(&crate::recycle::open_item(&via).unwrap()).unwrap();
        trash_with(None, &[]).move_to_trash(&via).unwrap();
        assert!(!dir.path().join("lustro-subst-test.txt").exists());
    }

    /// The long, verbatim-free form of a temp path, so rules match the final path even
    /// when %TEMP% itself is written with 8.3 names.
    #[cfg(windows)]
    fn long(p: &Path) -> PathBuf {
        let c = std::fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
    }

    #[cfg(windows)]
    fn trash_protecting(path: &Path, scope: Scope) -> PlatformTrash {
        let mut p = ProtectedPaths::defaults(Os::current(), &Env::default());
        p.push(path, "test protection", scope);
        PlatformTrash::with(p, Classifier::new(Os::current(), &[]))
    }

    /// Security review R1, probe 1: an 8.3 short name must not get around an Exact rule.
    #[cfg(windows)]
    #[test]
    fn short_name_of_a_protected_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        let folder = base.join("VeryLongProtectedFolderName");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("inside.txt"), b"x").unwrap();
        let short = base.join("VERYLO~1");
        if !short.exists() {
            eprintln!("8.3 names are off on this volume; nothing to probe");
            return;
        }
        let t = trash_protecting(&folder, Scope::Exact);
        assert!(t.protected_reason(&short).is_some());
        let err = t.move_to_trash(&short).unwrap_err();
        assert!(matches!(err, TrashError::Protected(_)), "{err:?}");
        assert!(folder.join("inside.txt").exists());
        // The same through a `\\?\` prefix, which turns off Win32 name rewriting.
        let verbatim = PathBuf::from(format!(r"\\?\{}", short.display()));
        assert!(matches!(
            t.move_to_trash(&verbatim),
            Err(TrashError::Protected(_))
        ));
        assert!(folder.exists());
    }

    /// Security review R1, probe 2: `report.` and `report ` are the same file as `report`
    /// to Win32, so they're refused outright, as are stream and device names.
    #[cfg(windows)]
    #[test]
    fn trailing_dot_space_and_stream_names_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        let report = base.join("report");
        std::fs::write(&report, b"keep").unwrap();
        let t = trash_protecting(&report, Scope::Exact);
        for name in [
            "report.",
            "report ",
            "report. .",
            "report:stream",
            "report::$DATA",
        ] {
            let err = t.move_to_trash(&base.join(name)).unwrap_err();
            assert!(matches!(err, TrashError::UnsafeName), "{name}: {err:?}");
            assert!(t.protected_reason(&base.join(name)).is_some(), "{name}");
        }
        for name in ["NUL", "con.txt", "COM1", "lpt9.log"] {
            assert!(matches!(
                t.move_to_trash(&base.join(name)),
                Err(TrashError::UnsafeName)
            ));
        }
        assert_eq!(std::fs::read(&report).unwrap(), b"keep");
    }

    /// A junction to a protected folder can't be used to reach inside it.
    #[cfg(windows)]
    #[test]
    fn junction_into_a_protected_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        let real = base.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("inside.txt"), b"x").unwrap();
        let junction = base.join("j");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&real)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "mklink /J failed");
        let t = trash_protecting(&real, Scope::Subtree);
        assert!(matches!(
            t.move_to_trash(&junction.join("inside.txt")),
            Err(TrashError::Protected(_))
        ));
        assert!(real.join("inside.txt").exists());
    }

    /// A `\\?\` path to an unprotected temp file is still trashed normally.
    #[cfg(windows)]
    #[test]
    fn verbatim_prefix_path_is_trashed() {
        let dir = tempfile::tempdir().unwrap();
        let f = long(dir.path()).join("lustro-verbatim-test.txt");
        std::fs::write(&f, b"x").unwrap();
        let verbatim = PathBuf::from(format!(r"\\?\{}", f.display()));
        trash_with(None, &[]).move_to_trash(&verbatim).unwrap();
        assert!(!f.exists());
    }

    /// Manual: an item bigger than the Recycle Bin's size limit must fail and stay put,
    /// never be deleted permanently.
    ///
    /// 1. Recycle Bin → Properties → select the drive holding %TEMP% → Custom size: 1 MB.
    /// 2. `cargo test -p lustro-platform oversized -- --ignored --nocapture`
    /// 3. Put the Recycle Bin size back afterwards.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs the Recycle Bin limit set to 1 MB by hand"]
    fn oversized_item_is_refused_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let f = long(dir.path()).join("lustro-oversized-test.bin");
        std::fs::write(&f, vec![0u8; 5 * 1024 * 1024]).unwrap();
        let result = trash_with(None, &[]).move_to_trash(&f);
        assert!(f.exists(), "the file must never be deleted permanently");
        let err = result.expect_err("expected a refusal, got Ok");
        eprintln!("refused as: {err:?}");
    }

    #[test]
    fn no_recycle_bin_message_has_no_path() {
        assert_eq!(
            TrashError::NoRecycleBin.to_string(),
            "This drive has no Recycle Bin, so Lustro won't delete from it. Use Move instead."
        );
    }

    #[test]
    fn free_up_space_only_in_placeholder_roots() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain.txt");
        std::fs::write(&file, b"x").unwrap();
        let t = trash_with(None, &[]);
        assert!(matches!(
            t.free_up_space(&file),
            Err(TrashError::Unsupported(_))
        ));
        // A cache root is not a placeholder root either.
        let t = trash_with(
            None,
            &[SyncRoot {
                provider: lustro_core::model::CloudProvider::GoogleDrive,
                display_name: "Google Drive".into(),
                root_path: PathBuf::from(dir.path()),
                kind: SyncRootKind::Cache,
            }],
        );
        assert!(matches!(
            t.free_up_space(&file),
            Err(TrashError::Unsupported(_))
        ));
    }

    /// In a fake "placeholder" root made of ordinary temp files, the OS must refuse
    /// (they aren't placeholders) and the file must be untouched.
    #[test]
    fn free_up_space_on_a_non_placeholder_is_refused_safely() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain.txt");
        std::fs::write(&file, b"still here").unwrap();
        let t = trash_with(
            None,
            &[SyncRoot {
                provider: lustro_core::model::CloudProvider::Other,
                display_name: "Fake".into(),
                root_path: PathBuf::from(dir.path()),
                kind: SyncRootKind::Placeholder,
            }],
        );
        let before = std::fs::metadata(&file).unwrap();
        assert!(matches!(
            t.free_up_space(&file),
            Err(TrashError::Unsupported(_))
        ));
        let after = std::fs::metadata(&file).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_eq!(before.file_attributes(), after.file_attributes());
        }
        assert_eq!(before.len(), after.len());
        assert_eq!(std::fs::read(&file).unwrap(), b"still here");
    }
}
