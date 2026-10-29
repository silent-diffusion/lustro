//! `lustro_core::cleanup::FileOps` for the real OS, plus the protected rules core plans with.

use std::io;
use std::path::{Path, PathBuf};

use lustro_core::cleanup::{Action, FileOps, Outcome, ProtectScope, ProtectedRule, UndoItem};
use lustro_core::dto::ItemIdentity;
use lustro_core::model::FileId;

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

/// Outcome of a trash. The token is `"<deletion time>[:<volume>:<index>] <final path>"`:
/// the time (Unix seconds) limits restore to items deleted around then, the final path
/// (long names on Windows) is where the item was, and on Windows the file id identifies
/// the item in the Recycle Bin, whose listed names are display names (no extension when
/// Explorer hides known ones), so names there can't be matched.
fn trashed_now((full, id): (PathBuf, Option<FileId>)) -> Outcome {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let id = id.map_or(String::new(), |i| format!(":{}:{}", i.volume, i.index));
    Outcome {
        location: None,
        token: Some(format!("{now}{id} {}", full.to_string_lossy())),
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
        Ok(trashed_now(self.trash_final(path, None).map_err(to_io)?))
    }

    /// Trashes only if the item is still the one reviewed; a mismatch fails with "changed
    /// since review". On Windows the identity is read from a handle held open until the
    /// delete has run, but the shell re-opens by path, so a tiny rename window remains
    /// (see [`PlatformTrash::move_to_trash_checked`]); the Recycle Bin guard still applies.
    fn trash_checked(&self, path: &Path, expected: &ItemIdentity) -> io::Result<Outcome> {
        Ok(trashed_now(
            self.trash_final(path, Some(expected)).map_err(to_io)?,
        ))
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
    let want = Wanted::from(item);
    let items =
        trash::os_limited::list().map_err(|_| io::Error::other("couldn't read the trash"))?;
    // If several match, the one closest to our deletion time.
    let deleted_at = want.deleted_at;
    let (found, to) = items
        .into_iter()
        .filter_map(|t| want.matching(&t, bin_item_id).map(|to| (t, to.to_owned())))
        .min_by_key(|(t, _)| deleted_at.map_or(-t.time_deleted, |d| (t.time_deleted - d).abs()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "item is no longer in the trash"))?;
    put_back(found, &to, want.file_id)
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
fn collision() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "something with the same name is already in the original folder",
    )
}

/// Windows: not through the `trash` crate, which restores to the bin's display name
/// (`x.txt` comes back as `x` when Explorer hides known extensions) and lets the shell
/// replace whatever is at that name. Instead the item is moved back to exactly the path
/// we recorded, and the move itself refuses to replace anything.
#[cfg(windows)]
fn put_back(found: trash::TrashItem, to: &Path, id: Option<FileId>) -> io::Result<()> {
    crate::recycle::unrecycle(Path::new(&found.id), to, id).map_err(|e| match e.kind() {
        io::ErrorKind::AlreadyExists => collision(),
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            "the original folder no longer exists",
        ),
        _ => io::Error::other("couldn't restore the item"),
    })
}

/// freedesktop: the `.trashinfo` holds the real name, and the crate reserves the
/// destination (`create_new`) before moving, so nothing is replaced.
#[cfg(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
fn put_back(found: trash::TrashItem, _to: &Path, _id: Option<FileId>) -> io::Result<()> {
    trash::os_limited::restore_all([found]).map_err(|e| match e {
        trash::Error::RestoreCollision { .. } => collision(),
        _ => io::Error::other("couldn't restore the item"),
    })
}

/// What `restore` looks for in the trash, from an undo item and its token.
#[cfg(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
struct Wanted {
    deleted_at: Option<i64>,
    /// Where the item was, as recorded (the first one is where it goes back).
    paths: Vec<PathBuf>,
    file_id: Option<FileId>,
    ci: bool,
    /// The token didn't parse: nothing matches.
    malformed: bool,
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
impl Wanted {
    fn from(item: &UndoItem) -> Self {
        let ci = lustro_cloud::rules::Os::current().case_insensitive();
        // Older tokens are just the time; newer ones also carry the final path, and on
        // Windows the file id after the time.
        let (head, trashed_from) = match item.token.as_deref() {
            Some(t) => t
                .split_once(' ')
                .map_or((Some(t), None), |(a, b)| (Some(a), Some(b))),
            None => (None, None),
        };
        // A token that's there but doesn't parse matches nothing, rather than dropping
        // the time window or the file-id check.
        let parts: Vec<&str> = head.map_or(Vec::new(), |h| h.split(':').collect());
        let deleted_at = parts.first().and_then(|t| t.parse().ok());
        let file_id = match parts[..] {
            [_, v, i] => v
                .parse()
                .ok()
                .zip(i.parse().ok())
                .map(|(volume, index)| FileId { volume, index }),
            _ => None,
        };
        let malformed = head.is_some()
            && (deleted_at.is_none() || !matches!((parts.len(), file_id), (1, _) | (3, Some(_))));
        // A newer token names exactly the path that was trashed; only that matches. For an
        // older time-only token: the path as given, or with its folder made long and final.
        let paths = match trashed_from {
            Some(p) => vec![PathBuf::from(p)],
            // Long form first: it's where the item goes back to.
            None => long_form(&item.original)
                .into_iter()
                .chain(std::iter::once(item.original.clone()))
                .collect(),
        };
        Wanted {
            deleted_at,
            paths,
            file_id,
            ci,
            malformed,
        }
    }

    /// Whether `t` is our item. All of:
    /// - deleted within RESTORE_WINDOW_SECS of our deletion time (when known), so an older
    ///   item the user trashed from the same place is never picked;
    /// - from the same folder: as recorded, or in its long, final form (the same folder
    ///   spelled differently, never a different one);
    /// - with the same name. On Windows the bin lists display names, which drop the
    ///   extension when Explorer hides known ones (the default), so the name also counts
    ///   with the extension of the item in the bin (`$R….txt` keeps the real one);
    /// - with a file id in the token (Windows): the item in the bin is that very file.
    #[cfg(test)]
    fn matches(
        &self,
        t: &trash::TrashItem,
        id_of: impl Fn(&trash::TrashItem) -> Option<FileId>,
    ) -> bool {
        self.matching(t, id_of).is_some()
    }

    /// Like `matches`, returning the recorded path the item goes back to.
    fn matching(
        &self,
        t: &trash::TrashItem,
        id_of: impl Fn(&trash::TrashItem) -> Option<FileId>,
    ) -> Option<&Path> {
        use lustro_cloud::paths;
        if self.malformed {
            return None;
        }
        if self
            .deleted_at
            .is_some_and(|d| (t.time_deleted - d).abs() > RESTORE_WINDOW_SECS)
        {
            return None;
        }
        let key = |p: &Path| paths::key(&p.to_string_lossy(), self.ci);
        let folder = key(&t.original_parent);
        let mut long_folder = None;
        let mut names = vec![key(Path::new(&t.name))];
        if cfg!(windows) {
            if let Some(ext) = Path::new(&t.id).extension() {
                let mut full = t.name.clone();
                full.push(".");
                full.push(ext);
                names.push(key(Path::new(&full)));
            }
        }
        let ours = self.paths.iter().find(|w| {
            let (Some(want_folder), Some(want_name)) = (w.parent().map(key), w.file_name()) else {
                return false;
            };
            names.contains(&key(Path::new(want_name)))
                && (want_folder == folder
                    || long_folder
                        .get_or_insert_with(|| {
                            std::fs::canonicalize(&t.original_parent)
                                .ok()
                                .map(|l| key(&l))
                        })
                        .as_ref()
                        == Some(&want_folder))
        });
        let ours = ours?;
        self.file_id
            .is_none_or(|id| id_of(t) == Some(id))
            .then_some(ours.as_path())
    }
}

/// The file id of an item in the Recycle Bin (`$R…`), from metadata only.
#[cfg(windows)]
fn bin_item_id(t: &trash::TrashItem) -> Option<FileId> {
    crate::recycle::identity(Path::new(&t.id)).ok()??.file_id
}

#[cfg(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
fn bin_item_id(_t: &trash::TrashItem) -> Option<FileId> {
    None
}

/// How far a trash entry's deletion time may be from ours and still count as our item.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
const RESTORE_WINDOW_SECS: i64 = 120;

/// `path` with its folder canonicalized (metadata only; the item itself is gone) and, on
/// Windows, the `\\?\` prefix dropped from drive paths.
#[cfg(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn long_form(path: &Path) -> Option<std::path::PathBuf> {
    let folder = std::fs::canonicalize(path.parent()?).ok()?;
    let full = folder.join(path.file_name()?);
    if cfg!(windows) && crate::recycle::is_drive_letter_path(&full) {
        let s = full.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return Some(rest.into());
        }
    }
    Some(full)
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
            restore_or_explain(&o, &item);
            assert_eq!(std::fs::read(&f).unwrap(), b"Lustro undo test");
        } else {
            assert_eq!(
                o.restore(&item).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
        }
    }

    /// Regression: an item trashed through an 8.3 short path must be found again by undo,
    /// both from the token's final path and (for older tokens) by expanding the folder.
    #[cfg(windows)]
    #[test]
    fn trash_through_a_short_path_then_restore() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("VeryLongUndoFolderName");
        std::fs::create_dir(&folder).unwrap();
        let short = dir.path().join("VERYLO~1");
        if !short.exists() {
            eprintln!("8.3 names are off on this volume; skipping");
            return;
        }
        let o = ops(None);
        for old_token in [false, true] {
            let f = folder.join("x.txt");
            std::fs::write(&f, b"short path undo").unwrap();
            let via = short.join("x.txt");
            let id = o.stat_identity(&via).unwrap().unwrap();
            let out = o.trash_checked(&via, &id).unwrap();
            assert!(!f.exists());
            let token = if old_token {
                out.token
                    .as_deref()
                    .and_then(|t| t.split_once(' '))
                    .map(|(head, _)| head.split(':').next().unwrap().to_owned())
            } else {
                out.token
            };
            let item = UndoItem {
                original: via.clone(),
                location: out.location,
                token,
                bytes: 15,
                action: Action::Trash,
                undoable: out.undoable,
                restored: false,
            };
            restore_or_explain(&o, &item);
            assert_eq!(std::fs::read(&f).unwrap(), b"short path undo");
        }
    }

    /// Restores `item`; on failure, says why without printing any path.
    fn restore_or_explain(o: &PlatformTrash, item: &UndoItem) {
        if let Err(e) = o.restore(item) {
            panic!("{e}; {}", bin_report(item));
        }
    }

    /// Path-free summary of the bin against `item`: counts, and for the items deleted
    /// within the window, which part of the match failed.
    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
    fn bin_report(item: &UndoItem) -> String {
        let Ok(items) = trash::os_limited::list() else {
            return "the bin can't be listed".into();
        };
        let want = Wanted::from(item);
        let name = item
            .original
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase());
        let near: Vec<_> = items
            .iter()
            .filter(|t| {
                want.deleted_at
                    .is_none_or(|d| (t.time_deleted - d).abs() <= RESTORE_WINDOW_SECS)
            })
            .collect();
        let mut out = format!(
            "bin items: {}; within the window: {}; token has id: {}",
            items.len(),
            near.len(),
            want.file_id.is_some()
        );
        for t in near {
            let ext = Path::new(&t.id)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase());
            let shown = t.name.to_string_lossy().to_lowercase();
            out += &format!(
                "\n  delta {:?} s; name as listed: {}; with bin extension: {}; same id: {}; match: {}",
                want.deleted_at.map(|d| t.time_deleted - d),
                Some(&shown) == name.as_ref(),
                ext.map(|e| format!("{shown}.{e}")).as_ref() == name.as_ref(),
                want.file_id.is_some() && bin_item_id(t) == want.file_id,
                want.matches(t, bin_item_id),
            );
        }
        out
    }

    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
    fn bin_report(_item: &UndoItem) -> String {
        String::new()
    }

    /// Trashes a fresh temp file; returns it and its undo item.
    fn trashed_temp(dir: &Path, name: &str) -> (std::path::PathBuf, UndoItem) {
        let f = dir.join(name);
        std::fs::write(&f, b"window").unwrap();
        let out = ops(None).trash(&f).unwrap();
        let item = UndoItem {
            original: f.clone(),
            location: out.location,
            token: out.token,
            bytes: 6,
            action: Action::Trash,
            undoable: out.undoable,
            restored: false,
        };
        (f, item)
    }

    /// A bin item with the same path but deleted well outside the window isn't ours.
    #[test]
    fn restore_ignores_an_item_deleted_outside_the_window() {
        if !CAN_RESTORE {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (f, item) = trashed_temp(&long(dir.path()), "lustro-window-test.txt");
        let token = item.token.clone().unwrap();
        let (head, path) = token.split_once(' ').unwrap();
        let secs = head.split(':').next().unwrap();
        let id = &head[secs.len()..];
        let old = secs.parse::<i64>().unwrap() - 10 * RESTORE_WINDOW_SECS;
        for t in [format!("{old}{id} {path}"), old.to_string()] {
            let stale = UndoItem {
                token: Some(t),
                ..item.clone()
            };
            assert_eq!(
                ops(None).restore(&stale).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
        }
        assert!(!f.exists());
        // Our own token still finds it (and cleans up).
        restore_or_explain(&ops(None), &item);
        assert!(f.exists());
    }

    /// With a path in the token, only that exact path matches: `original` and its long
    /// form aren't tried.
    #[test]
    fn restore_with_a_path_token_uses_only_that_path() {
        if !CAN_RESTORE {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (f, item) = trashed_temp(&long(dir.path()), "lustro-exact-test.txt");
        let token = item.token.clone().unwrap();
        let (head, _) = token.split_once(' ').unwrap();
        let elsewhere = long(dir.path()).join("some-other-name.txt");
        let wrong = UndoItem {
            token: Some(format!("{head} {}", elsewhere.display())),
            ..item.clone()
        };
        assert_eq!(
            ops(None).restore(&wrong).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!f.exists());
        if cfg!(windows) {
            // Case doesn't matter on Windows.
            let upper = UndoItem {
                token: Some(format!("{head} {}", f.to_string_lossy().to_uppercase())),
                original: elsewhere,
                ..item.clone()
            };
            restore_or_explain(&ops(None), &upper);
        } else {
            restore_or_explain(&ops(None), &item);
        }
        assert!(f.exists());
    }

    /// Regression (CI, windows-latest): undo restored `x.txt` as `x` when Explorer hid
    /// extensions. Each item must come back under exactly its own name, with its contents,
    /// and nothing under a shortened name.
    #[test]
    fn restore_keeps_the_exact_name() {
        if !CAN_RESTORE {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        for name in ["x.txt", "a.tar.gz", "noext", "Mixed.Case.JPG"] {
            let (f, item) = trashed_temp(&base, name);
            assert!(!f.exists(), "{name}");
            restore_or_explain(&ops(None), &item);
            assert_eq!(std::fs::read(&f).unwrap(), b"window", "{name}");
            let listed: Vec<_> = std::fs::read_dir(&base)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(listed, [name], "only the exact name is there");
            std::fs::remove_file(&f).unwrap();
        }
    }

    /// Undo never replaces something that has since appeared at the original path: it
    /// fails, the newcomer is untouched, and the item stays in the bin.
    #[test]
    fn restore_never_replaces_a_newer_item() {
        if !CAN_RESTORE {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (f, item) = trashed_temp(&long(dir.path()), "lustro-collision-test.txt");
        std::fs::write(&f, b"newer").unwrap();
        assert_eq!(
            ops(None).restore(&item).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&f).unwrap(), b"newer");
        // Still in the bin: once the newcomer is gone, undo works.
        std::fs::remove_file(&f).unwrap();
        restore_or_explain(&ops(None), &item);
        assert_eq!(std::fs::read(&f).unwrap(), b"window");
    }

    /// `unrecycle` only moves `$R…` items out of a `$Recycle.Bin` folder, only the file
    /// it expects, and never over something already there.
    #[cfg(windows)]
    #[test]
    fn unrecycle_only_takes_the_expected_item_from_a_bin() {
        use crate::recycle::unrecycle;
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        let fake_bin = base.join("$Recycle.Bin").join("S-1-5-21-1");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let plain = base.join("$Rplain.txt");
        let not_r = fake_bin.join("$Iabc.txt");
        std::fs::write(&plain, b"p").unwrap();
        std::fs::write(&not_r, b"i").unwrap();
        let to = base.join("dest.txt");
        for from in [&plain, &not_r] {
            assert_eq!(
                unrecycle(from, &to, None).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(from.exists());
        }
        let r = fake_bin.join("$Rabc.txt");
        std::fs::write(&r, b"r").unwrap();
        let id = stat_identity(&r).unwrap().unwrap().file_id;
        // Another file's id: refused, left in the bin.
        let other = stat_identity(&plain).unwrap().unwrap().file_id;
        assert!(unrecycle(&r, &to, other).is_err());
        assert!(r.exists() && !to.exists());
        // Something already at the destination: refused, untouched.
        std::fs::write(&to, b"taken").unwrap();
        assert_eq!(
            unrecycle(&r, &to, id).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&to).unwrap(), b"taken");
        std::fs::remove_file(&to).unwrap();
        // Destinations Win32 would rewrite or resolve elsewhere: refused.
        for bad in [
            base.join("dest.txt."),
            base.join("dest.txt "),
            base.join("sub").join("..").join("dest.txt"),
            PathBuf::from("dest.txt"),
        ] {
            assert_eq!(
                unrecycle(&r, &bad, id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{}",
                bad.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            );
        }
        // The expected item goes back, and its `$I` record goes with it.
        unrecycle(&r, &to, id).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"r");
        assert!(!r.exists() && !not_r.exists());
    }

    /// If a folder on the way to the original path has become a junction since (here the
    /// original folder itself, pointing somewhere else), restore refuses instead of
    /// following it.
    #[cfg(windows)]
    #[test]
    fn unrecycle_refuses_a_destination_behind_a_junction() {
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        let fake_bin = base.join("$Recycle.Bin").join("S-1-5-21-1");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let r = fake_bin.join("$Rjct.txt");
        std::fs::write(&r, b"r").unwrap();
        let id = stat_identity(&r).unwrap().unwrap().file_id;
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let original = base.join("original");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&original)
            .arg(&elsewhere)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "mklink /J failed");
        let to = original.join("x.txt");
        assert_eq!(
            crate::recycle::unrecycle(&r, &to, id).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(r.exists());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        // A junction higher up in the chain is refused the same way.
        let deeper = elsewhere.join("deeper");
        std::fs::create_dir(&deeper).unwrap();
        assert_eq!(
            crate::recycle::unrecycle(&r, &original.join("deeper").join("x.txt"), id)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(r.exists());
        assert_eq!(std::fs::read_dir(&deeper).unwrap().count(), 0);
    }

    /// A token that's there but doesn't parse matches nothing: a bad time doesn't drop
    /// the time window, and a bad id doesn't drop the file-id check.
    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
    #[test]
    fn malformed_tokens_match_nothing() {
        let original = PathBuf::from(if cfg!(windows) {
            r"C:\Users\William\x.txt"
        } else {
            "/home/william/x.txt"
        });
        let undo = |token: &str| UndoItem {
            original: original.clone(),
            location: None,
            token: Some(token.to_owned()),
            bytes: 1,
            action: Action::Trash,
            undoable: true,
            restored: false,
        };
        let listed = trash::TrashItem {
            id: original.as_os_str().to_owned(),
            name: "x.txt".into(),
            original_parent: original.parent().unwrap().to_owned(),
            time_deleted: 1000,
        };
        let p = original.display();
        let any_id = |_: &trash::TrashItem| {
            Some(FileId {
                volume: 1,
                index: 2,
            })
        };
        // Well-formed: matches.
        assert!(Wanted::from(&undo(&format!("1000 {p}"))).matches(&listed, any_id));
        assert!(Wanted::from(&undo(&format!("1000:1:2 {p}"))).matches(&listed, any_id));
        assert!(Wanted::from(&undo("1000")).matches(&listed, any_id));
        for bad in [
            format!("soon {p}"),
            format!(" {p}"),
            format!("1000:1 {p}"),
            format!("1000:x:2 {p}"),
            format!("1000:1:2:3 {p}"),
            format!("1000:: {p}"),
            "soon".to_owned(),
            String::new(),
        ] {
            assert!(
                !Wanted::from(&undo(&bad)).matches(&listed, any_id),
                "{bad:?}"
            );
        }
    }

    /// Regression (CI, windows-latest): the bin lists display names, so with Explorer
    /// hiding known extensions (the Windows default) `x.txt` is listed as `x`. Matching
    /// must not depend on that setting, and still needs folder, name and file id.
    #[cfg(windows)]
    #[test]
    fn bin_items_match_whatever_explorer_shows() {
        let dir = tempfile::tempdir().unwrap();
        let base = long(dir.path());
        // Stand-ins for items inside the bin, which keep the real extension.
        let in_bin = base.join("$RABC123.txt");
        let other = base.join("$RXYZ789.txt");
        std::fs::write(&in_bin, b"x").unwrap();
        std::fs::write(&other, b"y").unwrap();
        let id = stat_identity(&in_bin).unwrap().unwrap().file_id.unwrap();
        let with_id = format!(":{}:{}", id.volume, id.index);
        let folder = base.join("from");
        let original = folder.join("x.txt");
        let undo = |id: &str| UndoItem {
            original: original.clone(),
            location: None,
            token: Some(format!("1000{id} {}", original.display())),
            bytes: 1,
            action: Action::Trash,
            undoable: true,
            restored: false,
        };
        let listed = |file: &Path, name: &str, at: i64| trash::TrashItem {
            id: file.as_os_str().to_owned(),
            name: name.into(),
            original_parent: folder.clone(),
            time_deleted: at,
        };
        for want in [Wanted::from(&undo(&with_id)), Wanted::from(&undo(""))] {
            // Extension hidden or shown, any case: ours.
            assert!(want.matches(&listed(&in_bin, "x", 1000), bin_item_id));
            assert!(want.matches(&listed(&in_bin, "X.TXT", 1030), bin_item_id));
            // Another name, or deleted well outside the window: not ours.
            assert!(!want.matches(&listed(&in_bin, "y", 1000), bin_item_id));
            assert!(!want.matches(&listed(&in_bin, "x.txt.txt", 1000), bin_item_id));
            let late = 1000 + 10 * RESTORE_WINDOW_SECS;
            assert!(!want.matches(&listed(&in_bin, "x", late), bin_item_id));
            // Another folder: not ours.
            let mut moved = listed(&in_bin, "x", 1000);
            moved.original_parent = base.clone();
            assert!(!want.matches(&moved, bin_item_id));
        }
        // With the file id: the same folder and name but another file is not ours.
        let want = Wanted::from(&undo(&with_id));
        assert!(!want.matches(&listed(&other, "x", 1000), bin_item_id));
    }

    /// The canonical form of a temp folder, without `\\?\`, as the trash records it.
    fn long(p: &Path) -> std::path::PathBuf {
        let c = std::fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        std::path::PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
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
