//! Turns raw MFT records into final `ScanRecord`s: applies exclusions and the cloud
//! classifier, drops everything under folders that must not be entered, and checks
//! that the stream is a well-formed top-down tree.
//!
//! Records from the elevated helper arrive over a pipe, so this is also the
//! validation boundary for untrusted input: it never panics, and bounds memory by
//! capping ids, path length and depth. Folders are remembered as (parent, name)
//! rather than full paths, so memory grows with the input, not with depth x width.
//! Paths are rebuilt on demand, with one cached per batch since a batch is usually
//! one folder's children.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use lustro_core::model::{EntryFlags, NodeId, NodeKind, ScanRecord, ROOT_ID};
use lustro_core::scan::{CloudClassifier, Descend, RawAttrs, ScanOptions, SkipReason};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;

use crate::walker::{norm, MAX_DEPTH, MAX_PATH_UNITS};
use crate::windows_mft::RawRecord;

/// Highest accepted id + 1. Bounds the seen-id bitset (16 MiB) and the folder map.
pub(crate) const MAX_ID: u32 = 1 << 27;
/// Longest NTFS name, in UTF-16 units.
pub(crate) const MAX_NAME: usize = 255;

/// Why a record stream was rejected. Fixed strings: never paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invalid(pub &'static str);

pub(crate) struct Pruner<'a> {
    root: PathBuf,
    root_name: OsString,
    excluded: Vec<OsString>,
    classifier: &'a dyn CloudClassifier,
    /// Folders whose children may still arrive.
    dirs: HashMap<NodeId, DirInfo>,
    /// Last folder path built, as `path_of` is called once per record.
    cache: Option<(NodeId, PathBuf)>,
    /// Folders not entered (or under one): their descendants are dropped.
    pruned: HashSet<NodeId>,
    seen: Vec<u64>,
    root_done: bool,
}

impl<'a> Pruner<'a> {
    /// `root` must be absolute.
    pub(crate) fn new(
        root: &Path,
        options: &ScanOptions,
        classifier: &'a dyn CloudClassifier,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            root_name: root
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_else(|| root.as_os_str().to_os_string()),
            excluded: options
                .excluded
                .iter()
                .filter_map(|p| std::path::absolute(p).ok())
                .map(|p| norm(&p))
                .collect(),
            classifier,
            dirs: HashMap::new(),
            cache: None,
            pruned: HashSet::new(),
            seen: Vec::new(),
            root_done: false,
        }
    }

    /// Validates and filters one batch. On `Err` nothing from the batch is kept and
    /// the stream should be abandoned.
    pub(crate) fn filter(&mut self, batch: Vec<RawRecord>) -> Result<Vec<ScanRecord>, Invalid> {
        let mut out = Vec::with_capacity(batch.len());
        for raw in batch {
            let RawRecord {
                record: mut r,
                win_attributes,
            } = raw;
            self.mark_seen(r.id)?;
            let is_dir = r.meta.kind == NodeKind::Dir;
            let (path, info) = match r.parent {
                None => {
                    if self.root_done || r.id != ROOT_ID || !is_dir {
                        return Err(Invalid("bad root record"));
                    }
                    self.root_done = true;
                    r.meta.name = self.root_name.clone();
                    let info = DirInfo {
                        parent: None,
                        name: OsString::new(),
                        depth: 0,
                        units: path_units(&self.root),
                    };
                    (self.root.clone(), info)
                }
                Some(parent) => {
                    if !self.root_done || r.id == ROOT_ID {
                        return Err(Invalid("record before root"));
                    }
                    check_name(&r.meta.name)?;
                    if self.pruned.contains(&parent) {
                        if is_dir {
                            self.pruned.insert(r.id);
                        }
                        continue;
                    }
                    let Some(dir) = self.dirs.get(&parent) else {
                        return Err(Invalid("unknown parent"));
                    };
                    let info = DirInfo {
                        parent: Some(parent),
                        name: r.meta.name.clone(),
                        depth: dir.depth + 1,
                        units: dir.units + 1 + r.meta.name.encode_wide().count(),
                    };
                    (self.path_of(parent).join(&r.meta.name), info)
                }
            };
            r.meta.link_count = r.meta.link_count.max(1);
            r.meta.cloud = None;
            let raw_attrs = RawAttrs {
                win_attributes,
                st_flags: 0,
                is_dir: win_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 || is_dir,
            };
            r.meta.cloud = self.classifier.classify(&path, &raw_attrs);
            if is_dir {
                let too_deep = info.depth > MAX_DEPTH || info.units > MAX_PATH_UNITS;
                let skip = if too_deep {
                    Some(None)
                } else {
                    self.skip(&path, &raw_attrs).map(Some)
                };
                match skip {
                    None => {
                        self.dirs.insert(r.id, info);
                    }
                    Some(None) => {
                        // Nested past the path/depth limit: recorded, not entered.
                        r.meta.flags |= EntryFlags::NOT_DESCENDED;
                        self.pruned.insert(r.id);
                    }
                    Some(Some(reason)) => {
                        r.meta.flags |= EntryFlags::NOT_DESCENDED;
                        if reason == SkipReason::Remote {
                            r.meta.flags |= EntryFlags::REMOTE;
                        }
                        self.pruned.insert(r.id);
                    }
                }
            }
            out.push(r);
        }
        Ok(out)
    }

    /// Full path of a known folder, rebuilt from (parent, name) links. Depth is
    /// capped at [`MAX_DEPTH`], so this walks at most that many steps.
    fn path_of(&mut self, id: NodeId) -> PathBuf {
        if let Some((cached, path)) = &self.cache {
            if *cached == id {
                return path.clone();
            }
        }
        let mut names = Vec::new();
        let mut cur = id;
        while let Some(info) = self.dirs.get(&cur) {
            match info.parent {
                Some(p) => {
                    names.push(&info.name);
                    cur = p;
                }
                None => break,
            }
        }
        let mut path = self.root.clone();
        path.extend(names.into_iter().rev());
        self.cache = Some((id, path.clone()));
        path
    }

    fn skip(&self, path: &Path, raw: &RawAttrs) -> Option<SkipReason> {
        if !self.excluded.is_empty() && self.excluded.contains(&norm(path)) {
            return Some(SkipReason::Excluded);
        }
        match self.classifier.should_descend(path, raw) {
            Descend::Yes => None,
            Descend::No(r) => Some(r),
        }
    }

    fn mark_seen(&mut self, id: NodeId) -> Result<(), Invalid> {
        if id >= MAX_ID {
            return Err(Invalid("id out of range"));
        }
        let (word, bit) = ((id / 64) as usize, 1u64 << (id % 64));
        if word >= self.seen.len() {
            self.seen.resize(word + 1, 0);
        }
        if self.seen[word] & bit != 0 {
            return Err(Invalid("duplicate id"));
        }
        self.seen[word] |= bit;
        Ok(())
    }
}

/// What the pruner keeps per folder: enough to rebuild its path and check limits.
struct DirInfo {
    parent: Option<NodeId>,
    name: OsString,
    depth: u32,
    /// Length of the full path in UTF-16 units.
    units: usize,
}

fn path_units(path: &Path) -> usize {
    path.as_os_str().encode_wide().count()
}

/// A single path component: 1–255 UTF-16 units, no separators, NUL, `:` or dot names.
fn check_name(name: &OsString) -> Result<(), Invalid> {
    let mut len = 0;
    for c in name.encode_wide() {
        len += 1;
        if len > MAX_NAME || matches!(c, 0 | 0x2F | 0x3A | 0x5C) {
            return Err(Invalid("bad name"));
        }
    }
    if len == 0 || name == "." || name == ".." {
        return Err(Invalid("bad name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lustro_core::model::EntryMeta;
    use lustro_core::scan::NoCloud;

    fn rec(id: NodeId, parent: Option<NodeId>, name: &str, kind: NodeKind) -> RawRecord {
        RawRecord {
            record: ScanRecord {
                id,
                parent,
                meta: EntryMeta {
                    name: name.into(),
                    kind,
                    logical_size: 1,
                    allocated_size: 0,
                    modified: None,
                    accessed: None,
                    file_id: None,
                    link_count: 0,
                    cloud: None,
                    flags: EntryFlags::empty(),
                },
            },
            win_attributes: if kind == NodeKind::Dir { 0x10 } else { 0 },
        }
    }

    fn pruner(excluded: Vec<PathBuf>) -> Pruner<'static> {
        let opts = ScanOptions {
            root: PathBuf::from(r"C:\Users\William"),
            excluded,
            ..ScanOptions::default()
        };
        Pruner::new(&opts.root.clone(), &opts, &NoCloud)
    }

    #[test]
    fn accepts_a_tree_and_prunes_excluded() {
        let mut p = pruner(vec![PathBuf::from(r"C:\Users\William\skip")]);
        let out = p
            .filter(vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "skip", NodeKind::Dir),
                rec(2, Some(0), "keep", NodeKind::Dir),
            ])
            .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].meta.name, "William");
        assert!(out[1].meta.flags.contains(EntryFlags::NOT_DESCENDED));
        assert_eq!(out[2].meta.link_count, 1);
        let out = p
            .filter(vec![
                rec(3, Some(1), "gone.bin", NodeKind::File),
                rec(4, Some(1), "gonedir", NodeKind::Dir),
                rec(5, Some(4), "deeper.bin", NodeKind::File),
                rec(6, Some(2), "here.bin", NodeKind::File),
            ])
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 6);
    }

    #[test]
    fn deep_or_long_nesting_is_cut_off() {
        // A chain of folders deeper than MAX_DEPTH: the first one past the limit is
        // recorded but not entered, and nothing below it is kept.
        let mut p = pruner(vec![]);
        let n = MAX_DEPTH + 5;
        let mut batch = vec![rec(0, None, "", NodeKind::Dir)];
        for i in 1..=n {
            batch.push(rec(i, Some(i - 1), "d", NodeKind::Dir));
        }
        let out = p.filter(batch).unwrap();
        assert_eq!(out.len(), MAX_DEPTH as usize + 2);
        let last = out.last().unwrap();
        assert_eq!(last.id, MAX_DEPTH + 1);
        assert!(last.meta.flags.contains(EntryFlags::NOT_DESCENDED));
        assert!(p.dirs.len() <= MAX_DEPTH as usize + 1);

        // 255-unit names: the path limit is reached long before the depth limit.
        let mut p = pruner(vec![]);
        let long = "x".repeat(MAX_NAME);
        let mut batch = vec![rec(0, None, "", NodeKind::Dir)];
        for i in 1..=200 {
            batch.push(rec(i, Some(i - 1), &long, NodeKind::Dir));
        }
        let out = p.filter(batch).unwrap();
        let cut = out
            .iter()
            .position(|r| r.meta.flags.contains(EntryFlags::NOT_DESCENDED))
            .expect("cut off somewhere");
        assert!(cut < 200 && out.len() == cut + 1);
        assert!(p.dirs.values().all(|d| d.units <= MAX_PATH_UNITS));
    }

    #[test]
    fn rejects_malformed_streams() {
        let cases: Vec<Vec<RawRecord>> = vec![
            vec![rec(1, Some(0), "a", NodeKind::File)],
            vec![rec(3, None, "", NodeKind::Dir)],
            vec![rec(0, None, "", NodeKind::File)],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(0, None, "", NodeKind::Dir),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(9), "a", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "a", NodeKind::File),
                rec(1, Some(0), "b", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "f", NodeKind::File),
                rec(2, Some(1), "x", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(MAX_ID, Some(0), "a", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), r"..\..\x", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "..", NodeKind::Dir),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), "a:stream", NodeKind::File),
            ],
            vec![
                rec(0, None, "", NodeKind::Dir),
                rec(1, Some(0), &"x".repeat(256), NodeKind::File),
            ],
        ];
        for (i, batch) in cases.into_iter().enumerate() {
            assert!(pruner(vec![]).filter(batch).is_err(), "case {i} accepted");
        }
    }
}
