//! `lustro_core::cleanup::FileOps` for the real OS, plus the protected rules core plans with.

use std::io;
use std::path::Path;

use lustro_core::cleanup::{Action, FileOps, Outcome, ProtectScope, ProtectedRule, UndoItem};
use lustro_core::dto::ItemIdentity;

use crate::protected::{ProtectedPaths, Scope};
use crate::trash::{PlatformTrash, TrashError};

/// Default protected paths for this OS as core's `ProtectedRule`s (feed them into
/// `lustro_core::cleanup::ProtectedPaths::for_this_os`). On Windows every drive root is included.
pub fn protected_rules() -> Vec<ProtectedRule> {
    let mut rules = rules_from(&ProtectedPaths::for_current_os());
    if cfg!(windows) {
        for v in crate::list_volumes() {
            rules.push(ProtectedRule {
                path: v.mount_path.into(),
                scope: ProtectScope::ExactOnly,
            });
        }
    }
    rules
}

pub(crate) fn rules_from(list: &ProtectedPaths) -> Vec<ProtectedRule> {
    list.entries()
        .iter()
        .map(|e| ProtectedRule {
            path: e.path.clone(),
            scope: match e.scope {
                Scope::Subtree => ProtectScope::Subtree,
                Scope::Exact => ProtectScope::ExactOnly,
            },
        })
        .collect()
}

/// Maps to `io::Error` without paths in the message.
pub(crate) fn to_io(e: TrashError) -> io::Error {
    let kind = match &e {
        TrashError::Protected(_) => io::ErrorKind::PermissionDenied,
        TrashError::BadPath | TrashError::UnsafeName => io::ErrorKind::InvalidInput,
        TrashError::NotFound => io::ErrorKind::NotFound,
        TrashError::Unsupported(_) => io::ErrorKind::Unsupported,
        TrashError::NoRecycleBin => io::ErrorKind::Unsupported,
        TrashError::Changed | TrashError::TrashFailed => io::ErrorKind::Other,
        TrashError::Io(k) => *k,
    };
    io::Error::new(kind, e.to_string())
}

/// Outcome of a trash: the token is the deletion time (Unix seconds) for `restore`.
fn trashed_now() -> Outcome {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Outcome {
        location: None,
        token: Some(now.to_string()),
        undoable: CAN_RESTORE,
        freed_bytes: None,
    }
}

fn unsupported(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{what} isn't available yet"),
    )
}

/// Undo of a trashed item is supported where the `trash` crate can list and restore
/// (Windows and freedesktop Linux), not on macOS.
const CAN_RESTORE: bool = cfg!(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
));

impl FileOps for PlatformTrash {
    /// Metadata only: the item is opened as itself (links not followed, nothing read or
    /// hydrated). The id matches the scanner's so core can tell the item hasn't changed.
    fn stat_identity(&self, path: &Path) -> io::Result<Option<ItemIdentity>> {
        if cfg!(windows) && crate::recycle::has_unsafe_windows_name(path) {
            // Win32 would quietly look at a different item (`report.` is `report`).
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "this name can't be checked safely",
            ));
        }
        stat_identity(path)
    }

    /// Moves to the trash. The token is the deletion time (Unix seconds), used by `restore`
    /// to pick the right item when the same path was trashed more than once.
    fn trash(&self, path: &Path) -> io::Result<Outcome> {
        self.move_to_trash(path).map_err(to_io)?;
        Ok(trashed_now())
    }

    /// Trashes only if the item is still the one reviewed; a mismatch fails with "changed
    /// since review". On Windows the identity is read from a handle held open until the
    /// delete has run, but the shell re-opens by path, so a tiny rename window remains
    /// (see [`PlatformTrash::move_to_trash_checked`]); the Recycle Bin guard still applies.
    fn trash_checked(&self, path: &Path, expected: &ItemIdentity) -> io::Result<Outcome> {
        self.move_to_trash_checked(path, expected).map_err(to_io)?;
        Ok(trashed_now())
    }

    fn move_to(&self, _path: &Path, _target_dir: &Path) -> io::Result<Outcome> {
        Err(unsupported("Moving files"))
    }

    fn compress(&self, _path: &Path) -> io::Result<Outcome> {
        Err(unsupported("Compressing files"))
    }

    /// Not undoable: the file is still in the cloud and re-downloads when opened.
    fn free_up_cloud(&self, path: &Path) -> io::Result<Outcome> {
        self.free_up_space(path).map_err(to_io)?;
        Ok(Outcome {
            location: None,
            token: None,
            undoable: false,
            freed_bytes: None,
        })
    }

    fn restore(&self, item: &UndoItem) -> io::Result<()> {
        match item.action {
            Action::Trash => restore_from_trash(item),
            _ => Err(unsupported("Undoing this action")),
        }
    }
}

#[cfg(windows)]
pub(crate) fn stat_identity(path: &Path) -> io::Result<Option<ItemIdentity>> {
    crate::recycle::identity(path)
}

/// From `lstat`, exactly as the scanner (`lustro-scan` walker/unix.rs) builds it.
#[cfg(unix)]
pub(crate) fn stat_identity(path: &Path) -> io::Result<Option<ItemIdentity>> {
    use lustro_core::model::{FileId, NodeKind};
    use std::os::unix::fs::MetadataExt;
    let m = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let t = m.file_type();
    let kind = if t.is_symlink() {
        NodeKind::Link
    } else if t.is_dir() {
        NodeKind::Dir
    } else if t.is_file() {
        NodeKind::File
    } else {
        NodeKind::Other
    };
    Ok(Some(ItemIdentity {
        kind,
        file_id: Some(FileId {
            volume: m.dev(),
            index: m.ino(),
        }),
        logical_size: m.size(),
        modified: Some(m.mtime()),
    }))
}

#[cfg(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn restore_from_trash(item: &UndoItem) -> io::Result<()> {
    use lustro_cloud::paths;
    use lustro_cloud::rules::Os;

    let ci = Os::current().case_insensitive();
    let want = paths::key(&item.original.to_string_lossy(), ci);
    let deleted_at: Option<i64> = item.token.as_deref().and_then(|t| t.parse().ok());
    let items =
        trash::os_limited::list().map_err(|_| io::Error::other("couldn't read the trash"))?;
    // Same original path; if trashed several times, the one closest to our deletion time.
    let found = items
        .into_iter()
        .filter(|t| paths::key(&t.original_path().to_string_lossy(), ci) == want)
        .min_by_key(|t| deleted_at.map_or(-t.time_deleted, |d| (t.time_deleted - d).abs()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "item is no longer in the trash"))?;
    trash::os_limited::restore_all([found]).map_err(|e| match e {
        trash::Error::RestoreCollision { .. } => io::Error::new(
            io::ErrorKind::AlreadyExists,
            "something with the same name is already in the original folder",
        ),
        _ => io::Error::other("couldn't restore the item"),
    })
}

#[cfg(not(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn restore_from_trash(_item: &UndoItem) -> io::Result<()> {
    Err(unsupported("Restoring from the Trash"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protected::Env;
    use lustro_cloud::rules::Os;
    use lustro_cloud::Classifier;

    fn ops(extra: Option<&Path>) -> PlatformTrash {
        let mut p = ProtectedPaths::defaults(Os::current(), &Env::default());
        if let Some(x) = extra {
            p.push(x, "test protection", Scope::Subtree);
        }
        PlatformTrash::with(p, Classifier::new(Os::current(), &[]))
    }

    #[test]
    fn protected_path_is_permission_denied() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("keep.txt");
        std::fs::write(&f, b"x").unwrap();
        let e = ops(Some(dir.path())).trash(&f).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(!e.to_string().contains(&*dir.path().to_string_lossy()));
        assert!(f.exists());
    }

    #[test]
    fn no_recycle_bin_fails_the_item_with_the_user_message() {
        let e = to_io(TrashError::NoRecycleBin);
        assert_eq!(e.kind(), io::ErrorKind::Unsupported);
        assert_eq!(
            e.to_string(),
            "This drive has no Recycle Bin, so Lustro won't delete from it. Use Move instead."
        );
    }

    #[test]
    fn stat_identity_matches_the_item_and_sees_changes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("id.txt");
        std::fs::write(&f, b"12345").unwrap();
        let o = ops(None);
        let a = o.stat_identity(&f).unwrap().unwrap();
        assert_eq!(a.kind, lustro_core::model::NodeKind::File);
        assert_eq!(a.logical_size, 5);
        assert!(a.file_id.is_some());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!((a.modified.unwrap() - now).abs() < 120);
        // Same item, same identity; a different file with the same name gets a new id.
        assert_eq!(o.stat_identity(&f).unwrap().unwrap().file_id, a.file_id);
        let keep = dir.path().join("other.txt");
        std::fs::write(&keep, b"x").unwrap();
        std::fs::remove_file(&f).unwrap();
        std::fs::rename(&keep, &f).unwrap();
        assert_ne!(o.stat_identity(&f).unwrap().unwrap().file_id, a.file_id);
        let d = o.stat_identity(dir.path()).unwrap().unwrap();
        // Core's comparison: unchanged matches; a replaced file doesn't.
        assert!(a.still_matches(&a));
        assert!(!a.still_matches(&o.stat_identity(&f).unwrap().unwrap()));
        assert_eq!(d.kind, lustro_core::model::NodeKind::Dir);
        assert!(o.stat_identity(&dir.path().join("gone")).unwrap().is_none());
    }

    /// The same file with new contents no longer matches what the scan saw.
    #[test]
    fn modified_file_no_longer_matches() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("m.txt");
        std::fs::write(&f, b"before").unwrap();
        let o = ops(None);
        let seen = o.stat_identity(&f).unwrap().unwrap();
        assert!(seen.still_matches(&o.stat_identity(&f).unwrap().unwrap()));
        std::fs::write(&f, b"after, and longer").unwrap();
        let now = o.stat_identity(&f).unwrap().unwrap();
        assert_eq!(now.file_id, seen.file_id);
        assert!(!seen.still_matches(&now));
        // Same size but written much later also counts as changed.
        std::fs::write(&f, b"BEFORE").unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(!seen.still_matches(&o.stat_identity(&f).unwrap().unwrap()));
        std::fs::remove_file(&f).unwrap();
        assert!(o.stat_identity(&f).unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn stat_identity_of_a_junction_is_a_link_and_not_followed() {
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
        let o = ops(None);
        let link = o.stat_identity(&j).unwrap().unwrap();
        let target = o.stat_identity(&real).unwrap().unwrap();
        assert_eq!(link.kind, lustro_core::model::NodeKind::Link);
        assert_ne!(link.file_id, target.file_id);
        assert_eq!(
            o.stat_identity(&dir.path().join("real."))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn trash_checked_only_trashes_the_reviewed_item() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("reviewed.txt");
        std::fs::write(&f, b"reviewed").unwrap();
        let o = ops(None);
        let seen = o.stat_identity(&f).unwrap().unwrap();
        // Replaced by a different file of the same name and size: refused, left alone.
        let other = dir.path().join("other.txt");
        std::fs::write(&other, b"REVIEWED").unwrap();
        std::fs::remove_file(&f).unwrap();
        std::fs::rename(&other, &f).unwrap();
        let e = o.trash_checked(&f, &seen).unwrap_err();
        assert!(e.to_string().contains("changed since review"), "{e}");
        assert_eq!(std::fs::read(&f).unwrap(), b"REVIEWED");
        // Gone: NotFound.
        let gone = dir.path().join("gone.txt");
        assert_eq!(
            o.trash_checked(&gone, &seen).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        // Unchanged: trashed.
        let now = o.stat_identity(&f).unwrap().unwrap();
        let out = o.trash_checked(&f, &now).unwrap();
        assert!(!f.exists());
        assert_eq!(out.undoable, CAN_RESTORE);
        assert!(out.token.is_some());
    }

    #[test]
    fn unsupported_operations() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let o = ops(None);
        assert_eq!(
            o.move_to(&f, dir.path()).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!(
            o.compress(&f).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!(
            o.free_up_cloud(&f).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert!(f.exists());
    }

    /// Trash a fresh temp file, then undo it through `restore`.
    #[test]
    fn trash_then_restore_a_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let name = format!("lustro-undo-test-{}.txt", std::process::id());
        let f = dir.path().join(name);
        std::fs::write(&f, b"Lustro undo test").unwrap();
        let o = ops(None);
        let out = o.trash(&f).unwrap();
        assert!(!f.exists());
        assert_eq!(out.undoable, CAN_RESTORE);
        let item = UndoItem {
            original: f.clone(),
            location: out.location,
            token: out.token,
            bytes: 21,
            action: Action::Trash,
            undoable: out.undoable,
            restored: false,
        };
        if CAN_RESTORE {
            o.restore(&item).unwrap();
            assert_eq!(std::fs::read(&f).unwrap(), b"Lustro undo test");
        } else {
            assert_eq!(
                o.restore(&item).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
        }
    }

    #[test]
    fn rules_cover_the_list_and_map_scopes() {
        let env = Env {
            home: Some("/home/william".into()),
            ..Env::default()
        };
        let list = ProtectedPaths::defaults(Os::Linux, &env);
        let rules = rules_from(&list);
        assert_eq!(rules.len(), list.entries().len());
        assert!(rules
            .iter()
            .any(|r| r.path == Path::new("/usr") && r.scope == ProtectScope::Subtree));
        assert!(rules
            .iter()
            .any(|r| r.path == Path::new("/home/william") && r.scope == ProtectScope::ExactOnly));
        assert!(protected_rules().len() > 5);
    }
}
