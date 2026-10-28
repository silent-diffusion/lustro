//! Portable parallel directory walker.
//!
//! Each directory is one rayon task. A task lists its directory, builds one record
//! per entry, decides which subdirectories to enter, pushes the records and only then
//! spawns the subdirectory tasks, so every parent reaches the sink before its children.
//!
//! Subdirectories are opened relative to their parent (`sys::open_child`), not by
//! re-resolving full paths, so a folder swapped for a link mid-scan can't lead the
//! walk outside the root.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use lustro_core::model::{EntryFlags, EntryMeta, NodeId, NodeKind, ScanRecord, ROOT_ID};
use lustro_core::scan::{
    CancelToken, CloudClassifier, Descend, RawAttrs, ScanError, ScanMode, ScanOptions, ScanSink,
    ScanSummary, Scanner, SkipReason,
};

use crate::emit::Emitter;

#[cfg(unix)]
#[path = "unix.rs"]
pub(crate) mod sys;
#[cfg(windows)]
#[path = "windows.rs"]
pub(crate) mod sys;

/// Folders nested deeper than this aren't entered (recorded with `NOT_DESCENDED`).
/// Real trees stay far below it; it bounds memory and work on crafted ones.
pub(crate) const MAX_DEPTH: u32 = 1024;
/// Nor are folders whose path is longer than this (UTF-16 units on Windows, bytes
/// elsewhere): the Windows maximum, and well past `PATH_MAX` on Unix.
pub(crate) const MAX_PATH_UNITS: usize = 32_767;

/// One directory entry as read from the OS, before cloud classification.
pub(crate) struct Entry {
    pub name: OsString,
    pub meta: EntryMeta,
    pub raw: RawAttrs,
    /// `st_dev` on Unix (filesystem boundary checks); 0 on Windows.
    pub dev: u64,
}

/// Portable scanner. Works everywhere without privileges.
#[derive(Clone, Debug, Default)]
pub struct Walker {
    /// Worker threads; `None` uses one per logical CPU.
    pub threads: Option<usize>,
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    excluded: Vec<OsString>,
    classifier: &'a dyn CloudClassifier,
    emit: &'a Emitter<'a>,
    cancel: &'a CancelToken,
    next_id: AtomicU32,
    vol: sys::Volume,
}

struct DirTask {
    id: NodeId,
    path: PathBuf,
    pending: sys::Pending,
    dev: u64,
    depth: u32,
}

impl Scanner for Walker {
    fn scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, ScanError> {
        let root = std::path::absolute(&options.root).map_err(|_| ScanError::BadRoot)?;
        let (mut root_entry, vol) = match sys::root(&root) {
            Ok(r) => r,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ScanError::BadRoot),
            Err(e) => return Err(e.into()),
        };
        if root_entry.meta.kind != NodeKind::Dir {
            return Err(ScanError::BadRoot);
        }
        root_entry.name = root
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| root.clone().into_os_string());

        let mut builder =
            rayon::ThreadPoolBuilder::new().thread_name(|i| format!("lustro-scan-{i}"));
        // One thread per CPU measured best; 2x was ~25% slower in testing.
        if let Some(n) = self.threads {
            builder = builder.num_threads(n);
        }
        #[cfg(target_os = "macos")]
        {
            builder = builder.start_handler(|_| crate::macos::forbid_dataless_materialization());
        }
        let pool = builder
            .build()
            .map_err(|e| ScanError::Io(io::Error::other(e)))?;

        let emit = Emitter::new(sink);
        let ctx = Ctx {
            opts: options,
            excluded: options
                .excluded
                .iter()
                .filter_map(|p| std::path::absolute(p).ok())
                .map(|p| norm(&p))
                .collect(),
            classifier,
            emit: &emit,
            cancel,
            next_id: AtomicU32::new(ROOT_ID + 1),
            vol,
        };

        emit.run(|| {
            // The root was picked by the user, so it's entered even when remote or on
            // another filesystem; only exclusions and the cloud classifier can stop it.
            let dev = root_entry.dev;
            root_entry.meta.cloud = classifier.classify(&root, &root_entry.raw);
            let task = ctx.decide_and_open(&root, &mut root_entry, None);
            emit.push(vec![ScanRecord {
                id: ROOT_ID,
                parent: None,
                meta: root_entry.meta,
            }]);
            if let Some(pending) = task {
                let task = DirTask {
                    id: ROOT_ID,
                    path: root,
                    pending,
                    dev,
                    depth: 0,
                };
                pool.scope(|s| ctx.walk(s, task));
            }
        });

        Ok(ScanSummary {
            mode_used: ScanMode::Standard,
            progress: emit.progress(),
            cancelled: cancel.is_cancelled(),
        })
    }
}

impl<'a> Ctx<'a> {
    fn walk<'s>(&'s self, scope: &rayon::Scope<'s>, task: DirTask)
    where
        'a: 's,
    {
        if self.cancel.is_cancelled() {
            return;
        }
        let listed = sys::activate(task.pending, &task.path)
            .and_then(|h| sys::read_dir(&h, &task.path, &self.vol).map(|e| (h, e)));
        let (handle, entries) = match listed {
            Ok(x) => x,
            Err(e) => {
                self.emit
                    .warn(display(&task.path), format!("could not list folder: {e}"));
                return;
            }
        };
        let base = self
            .next_id
            .fetch_add(entries.len() as u32, Ordering::Relaxed);
        let mut records = Vec::with_capacity(entries.len());
        let mut subdirs = Vec::new();
        for (i, mut entry) in entries.into_iter().enumerate() {
            if i % 1024 == 1023 && self.cancel.is_cancelled() {
                return;
            }
            let id = base + i as u32;
            let path = task.path.join(&entry.name);
            entry.meta.cloud = self.classifier.classify(&path, &entry.raw);
            // A directory already marked NOT_DESCENDED couldn't be inspected.
            if entry.meta.kind == NodeKind::Dir
                && !entry.meta.flags.contains(EntryFlags::NOT_DESCENDED)
            {
                let dev = entry.dev;
                let depth = task.depth + 1;
                if depth > MAX_DEPTH || path_units(&path) > MAX_PATH_UNITS {
                    entry.meta.flags |= EntryFlags::NOT_DESCENDED;
                    self.emit.warn(
                        display(&path),
                        "folder nested too deeply; not scanned".into(),
                    );
                } else if let Some(pending) =
                    self.decide_and_open(&path, &mut entry, Some((task.dev, &handle)))
                {
                    subdirs.push(DirTask {
                        id,
                        path,
                        pending,
                        dev,
                        depth,
                    });
                }
            }
            records.push(ScanRecord {
                id,
                parent: Some(task.id),
                meta: entry.meta,
            });
        }
        drop(handle);
        self.emit.push(records);
        for sub in subdirs {
            scope.spawn(move |s| self.walk(s, sub));
        }
    }

    /// Decides whether to enter directory `entry` and opens it if so. Updates its
    /// flags (`NOT_DESCENDED`, `MOUNT_POINT`, `REMOTE`, `ACCESS_DENIED`).
    /// `parent` (its `dev` and open handle) is `None` for the root.
    fn decide_and_open(
        &self,
        path: &Path,
        entry: &mut Entry,
        parent: Option<(u64, &sys::DirHandle)>,
    ) -> Option<sys::Pending> {
        let skip = self.skip_reason(path, entry, parent.map(|p| p.0));
        if skip.is_some() {
            entry.meta.flags |= EntryFlags::NOT_DESCENDED;
            return None;
        }
        let opened = match parent {
            Some((_, dir)) => sys::open_child(dir, &entry.name, entry),
            None => sys::open_root_dir(path),
        };
        match opened {
            Ok(h) => Some(h),
            Err(e) => {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    entry.meta.flags |= EntryFlags::ACCESS_DENIED;
                }
                entry.meta.flags |= EntryFlags::NOT_DESCENDED;
                self.emit
                    .warn(display(path), format!("could not open folder: {e}"));
                None
            }
        }
    }

    fn skip_reason(
        &self,
        path: &Path,
        entry: &mut Entry,
        parent_dev: Option<u64>,
    ) -> Option<SkipReason> {
        if !self.excluded.is_empty() && self.excluded.contains(&norm(path)) {
            return Some(SkipReason::Excluded);
        }
        if let Some(parent_dev) = parent_dev {
            let (flags, reason) = sys::boundary(path, entry, parent_dev, self.opts);
            entry.meta.flags |= flags;
            if reason.is_some() {
                return reason;
            }
        }
        match self.classifier.should_descend(path, &entry.raw) {
            Descend::Yes => None,
            Descend::No(reason) => {
                if reason == SkipReason::Remote {
                    entry.meta.flags |= EntryFlags::REMOTE;
                }
                Some(reason)
            }
        }
    }
}

/// Comparable form of a path for exclusion checks (case-insensitive on Windows).
pub(crate) fn norm(path: &Path) -> OsString {
    let s = path.as_os_str();
    #[cfg(windows)]
    {
        let lower = s.to_string_lossy().to_lowercase();
        OsString::from(lower.trim_end_matches(['\\', '/']))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut b = s.as_bytes();
        while b.len() > 1 && b.ends_with(b"/") {
            b = &b[..b.len() - 1];
        }
        std::ffi::OsStr::from_bytes(b).to_os_string()
    }
}

/// Path length as counted by [`MAX_PATH_UNITS`].
fn path_units(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count()
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().len()
    }
}

/// Display form of a path for `ScanWarning` (shown to the user, never logged).
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Windows FILETIME (100 ns since 1601) to Unix seconds. 0 means unknown.
#[cfg(windows)]
pub(crate) fn filetime_to_unix(ft: i64) -> Option<i64> {
    (ft > 0).then(|| ft / 10_000_000 - 11_644_473_600)
}
