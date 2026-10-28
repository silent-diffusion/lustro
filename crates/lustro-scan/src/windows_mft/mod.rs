//! `ScanMode::Fast` on Windows: read the NTFS Master File Table directly.
//!
//! Needs administrator rights (raw read access to the volume). Steps:
//! 1. check the root is on a local NTFS volume and open the volume read-only;
//! 2. read `$MFT`'s own record to find where the table lives, then read the whole
//!    table in large sequential chunks and parse the records in parallel ([`parse`]);
//! 3. link names to parent directories and walk the tree from the scan root
//!    top-down, producing [`RawRecord`]s;
//! 4. apply the same exclusions and cloud checks as the walker (`crate::prune`).
//!
//! Step 4 is separate so the elevated helper (`crate::helper`) can run steps 1–3 and
//! leave the cloud checks to the unelevated app.
//!
//! Only metadata is read: file data, including cloud placeholders, is never touched.
//! If any step before the first record is emitted fails, the caller gets
//! [`FastError::Unavailable`] and falls back to the walker.

// OS API calls need `unsafe`; each block is small and has a SAFETY note.
#![allow(unsafe_code)]

mod parse;

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::path::{Component, Path, Prefix};
use std::sync::mpsc;

use lustro_core::model::{EntryFlags, EntryMeta, FileId, NodeId, NodeKind, ScanRecord, ROOT_ID};
use lustro_core::scan::{
    CancelToken, CloudClassifier, ScanError, ScanMode, ScanOptions, ScanSink, ScanSummary, Scanner,
};
use rayon::prelude::*;
use windows_sys::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetVolumeInformationW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_SPARSE_FILE, FILE_ATTRIBUTE_SYSTEM, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
};

use crate::emit::Emitter;
use crate::prune::Pruner;
use crate::walker::{filetime_to_unix, sys as wsys};
pub use crate::FastError;
use parse::{MftNode, Parsed};

const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOVABLE: u32 = 2;
/// Record number of the volume's root directory.
const MFT_ROOT: u64 = 5;
/// Records below this are NTFS metadata files (`$MFT`, `$Bitmap`, `$Extend`, ...).
const FIRST_USER_RECORD: u64 = 16;
/// Bytes per read from the volume.
const CHUNK: u64 = 8 << 20;
/// Most MFT records read (also the scan's id limit, `prune::MAX_ID`). Bounds memory
/// on crafted media; real volumes stay far below it.
const MAX_RECORDS: u64 = 1 << 27;
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;

/// One entry from the MFT walk, before exclusions and cloud classification.
/// `record.meta.cloud` is always `None`; the root's name is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawRecord {
    pub record: ScanRecord,
    /// `FILE_ATTRIBUTE_*` bits, for `CloudClassifier`.
    pub win_attributes: u32,
}

/// NTFS MFT scanner. As a [`Scanner`] it falls back to [`crate::Walker`] by itself.
#[derive(Clone, Debug, Default)]
pub struct MftScanner {
    pub walker: crate::Walker,
}

impl Scanner for MftScanner {
    fn scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, ScanError> {
        match self.try_scan(options, classifier, sink, cancel) {
            Ok(s) => Ok(s),
            Err(FastError::Unavailable(_)) => self.walker.scan(options, classifier, sink, cancel),
            Err(FastError::Scan(e)) => Err(e),
        }
    }
}

impl MftScanner {
    /// Runs the MFT scan only; never falls back.
    pub fn try_scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, FastError> {
        let root =
            std::path::absolute(&options.root).map_err(|_| FastError::Scan(ScanError::BadRoot))?;
        let prepared = Prepared::new(&root, cancel)?;
        let emit = Emitter::new(sink);
        let mut pruner = Pruner::new(&root, options, classifier);
        emit.run(|| {
            prepared.walk(cancel, &mut |batch| {
                // Our own walk output always validates; a failure here would be a bug,
                // and dropping the batch is safer than emitting it.
                if let Ok(records) = pruner.filter(batch) {
                    emit.push(records);
                }
            })
        });
        Ok(ScanSummary {
            mode_used: ScanMode::Fast,
            progress: emit.progress(),
            cancelled: cancel.is_cancelled(),
        })
    }

    /// Steps 1–3 only: reads the MFT under `root` and hands over raw batches
    /// (root first, then one batch per folder, parents before children).
    /// No exclusions or cloud checks are applied. Used by the elevated helper.
    pub fn scan_raw(
        &self,
        root: &Path,
        cancel: &CancelToken,
        out: &mut dyn FnMut(Vec<RawRecord>),
    ) -> Result<(), FastError> {
        let root = std::path::absolute(root).map_err(|_| FastError::Scan(ScanError::BadRoot))?;
        Prepared::new(&root, cancel)?.walk(cancel, out);
        Ok(())
    }
}

/// The MFT read into memory, ready to walk from the scan root.
struct Prepared {
    table: Table,
    root_rec: u32,
    root_meta: EntryMeta,
    serial: u64,
}

impl Prepared {
    /// This runs elevated (in the helper), on a root another process supplied, so:
    /// - only `X:\...` paths on local fixed/removable drives are accepted, checked
    ///   before *anything* is opened (no UNC, device or volume-GUID paths: an elevated
    ///   open of `\\server\share` would send the admin's credentials there);
    /// - the root is opened one component at a time relative to its parent, never
    ///   following a link, so a junction or symlink can't lead it anywhere else;
    /// - the volume read must be the volume the root is on (serial and sequence checks).
    fn new(root: &Path, cancel: &CancelToken) -> Result<Self, FastError> {
        let (letter, parts) = local_drive_path(root).ok_or(FastError::Unavailable(
            "fast scan needs a folder on a local drive",
        ))?;
        let drive_root = format!("{}:\\", char::from(letter));
        check_local_ntfs(&drive_root)?;

        let root_entry = open_root(&drive_root, &parts)?;
        let root_id = root_entry
            .meta
            .file_id
            .ok_or(FastError::Unavailable("no file id"))?;

        let volume = open_volume(letter)?;
        let (table, boot_serial) = read_table(&volume, cancel)?;
        drop(volume);

        // GetFileInformationByHandle reports the low 32 bits of the NTFS serial.
        if boot_serial & 0xFFFF_FFFF != root_id.volume {
            return Err(FastError::Unavailable("root is not on this volume"));
        }
        let root_rec = root_id.index & 0x0000_FFFF_FFFF_FFFF;
        let root_seq = (root_id.index >> 48) as u16;
        if !table
            .nodes
            .get(root_rec as usize)
            .is_some_and(|n| n.in_use && n.is_dir && (root_seq == 0 || n.seq == root_seq))
        {
            return Err(FastError::Unavailable("root not found in MFT"));
        }
        Ok(Self {
            table,
            root_rec: root_rec as u32,
            root_meta: root_entry.meta,
            serial: root_id.volume,
        })
    }

    /// Emits the root, then each folder's children as one batch, top-down.
    fn walk(&self, cancel: &CancelToken, out: &mut dyn FnMut(Vec<RawRecord>)) {
        if cancel.is_cancelled() {
            return;
        }
        let t = &self.table;
        let mut root_meta = self.root_meta.clone();
        root_meta.name = OsString::new();
        out(vec![RawRecord {
            record: ScanRecord {
                id: ROOT_ID,
                parent: None,
                meta: root_meta,
            },
            win_attributes: raw_attrs(&t.nodes[self.root_rec as usize]),
        }]);

        // Each folder record is entered at most once, through the first name met, so
        // crafted cycles or folders with many names can't loop or blow up the walk.
        let mut visited = vec![false; t.nodes.len()];
        visited[self.root_rec as usize] = true;
        let mut next_id: NodeId = ROOT_ID + 1;
        let mut stack: Vec<(NodeId, u32)> = vec![(ROOT_ID, self.root_rec)];
        while let Some((dir_id, dir_rec)) = stack.pop() {
            if cancel.is_cancelled() {
                return;
            }
            let kids = &t.children
                [t.offsets[dir_rec as usize] as usize..t.offsets[dir_rec as usize + 1] as usize];
            if kids.is_empty() {
                continue;
            }
            let mut records = Vec::with_capacity(kids.len());
            for &(rec, name_idx) in kids {
                let node = &t.nodes[rec as usize];
                let is_dir = !node.is_link() && node.is_dir;
                if is_dir && std::mem::replace(&mut visited[rec as usize], true) {
                    continue;
                }
                let name = OsString::from_wide(&node.names[name_idx as usize].name);
                let id = next_id;
                next_id += 1;
                let meta = self.meta(node, rec, name);
                if is_dir {
                    stack.push((id, rec));
                }
                records.push(RawRecord {
                    record: ScanRecord {
                        id,
                        parent: Some(dir_id),
                        meta,
                    },
                    win_attributes: raw_attrs(node),
                });
            }
            out(records);
        }
    }

    fn meta(&self, node: &MftNode, rec: u32, name: OsString) -> EntryMeta {
        let kind = if node.is_link() {
            NodeKind::Link
        } else if node.is_dir {
            NodeKind::Dir
        } else {
            NodeKind::File
        };
        let mut flags = EntryFlags::empty();
        for (bit, flag) in [
            (FILE_ATTRIBUTE_HIDDEN, EntryFlags::HIDDEN),
            (FILE_ATTRIBUTE_SYSTEM, EntryFlags::SYSTEM),
            (FILE_ATTRIBUTE_READONLY, EntryFlags::READONLY),
            (FILE_ATTRIBUTE_COMPRESSED, EntryFlags::COMPRESSED),
            (FILE_ATTRIBUTE_SPARSE_FILE, EntryFlags::SPARSE),
        ] {
            if node.attrs & bit != 0 {
                flags |= flag;
            }
        }
        if node.volume_mount {
            flags |= EntryFlags::MOUNT_POINT;
        }
        let files = kind == NodeKind::File;
        EntryMeta {
            name,
            kind,
            logical_size: if files { node.logical } else { 0 },
            allocated_size: if files { node.allocated } else { 0 },
            modified: filetime_to_unix(node.modified),
            accessed: filetime_to_unix(node.accessed),
            file_id: Some(FileId {
                volume: self.serial,
                index: (u64::from(node.seq) << 48) | u64::from(rec),
            }),
            link_count: (node.names.len() as u32).max(1),
            cloud: None,
            flags,
        }
    }
}

/// `X:\\a\\b` (plain or `\\?\\X:\\...`) as its drive letter and folder names.
/// Anything else (UNC, devices, volume GUIDs, relative or `..` paths) is `None`.
pub(crate) fn local_drive_path(root: &Path) -> Option<(u8, Vec<OsString>)> {
    let mut parts = root.components();
    let letter = match parts.next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) if l.is_ascii_alphabetic() => {
                l.to_ascii_uppercase()
            }
            _ => return None,
        },
        _ => return None,
    };
    if parts.next()? != Component::RootDir {
        return None;
    }
    // Verbatim (`\\?\`) paths aren't normalised, so `..` can arrive as a plain name.
    let names = parts
        .map(|c| match c {
            Component::Normal(n)
                if n != "."
                    && n != ".."
                    && !n.encode_wide().any(|u| matches!(u, 0 | 0x2F | 0x3A | 0x5C)) =>
            {
                Some(n.to_os_string())
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((letter, names))
}

/// Refuses anything but a local fixed/removable NTFS drive. Touches only the drive
/// letter's root, and only after its type is known to be local.
fn check_local_ntfs(drive_root: &str) -> Result<(), FastError> {
    let wide: Vec<u16> = drive_root.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated.
    let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
    if drive_type != DRIVE_FIXED && drive_type != DRIVE_REMOVABLE {
        return Err(FastError::Unavailable("not a local drive"));
    }
    let mut fs_name = [0u16; 64];
    // SAFETY: `wide` is NUL-terminated; out-pointers are valid or null as allowed.
    let ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    let fs_len = fs_name.iter().position(|&c| c == 0).unwrap_or(0);
    if ok == 0 || OsString::from_wide(&fs_name[..fs_len]) != "NTFS" {
        return Err(FastError::Unavailable("not NTFS"));
    }
    Ok(())
}

/// Opens the scan root from the drive root down, one folder at a time relative to
/// its parent's handle, refusing any link on the way.
fn open_root(drive_root: &str, parts: &[OsString]) -> Result<crate::walker::Entry, FastError> {
    let access = FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
    let mut h = wsys::open_meta(Path::new(drive_root), access)
        .map_err(|_| FastError::Unavailable("cannot open drive"))?;
    for name in parts {
        h = wsys::open_at(&h, name, access, true).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => {
                FastError::Scan(ScanError::BadRoot)
            }
            _ => FastError::Unavailable("cannot open folder"),
        })?;
        let (attrs, tag) =
            wsys::attribute_tag(&h).map_err(|_| FastError::Unavailable("cannot open folder"))?;
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && tag & TAG_NAME_SURROGATE != 0 {
            return Err(FastError::Unavailable("path goes through a link"));
        }
    }
    let entry =
        wsys::entry_from_handle(&h).map_err(|_| FastError::Unavailable("cannot read folder"))?;
    if entry.meta.kind != NodeKind::Dir {
        return Err(FastError::Scan(ScanError::BadRoot));
    }
    Ok(entry)
}

/// Opens drive `letter`'s volume device read-only (needs admin).
fn open_volume(letter: u8) -> Result<File, FastError> {
    let device = format!("\\\\.\\{}:", char::from(letter));
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(device)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                FastError::Unavailable("needs administrator")
            } else {
                FastError::Unavailable("cannot open volume")
            }
        })
}

struct Table {
    nodes: Vec<MftNode>,
    /// `children[offsets[r]..offsets[r + 1]]` are the (record, name index) pairs whose
    /// parent is record `r`. u64: edge counts can exceed u32 on crafted volumes.
    offsets: Vec<u64>,
    children: Vec<(u32, u16)>,
}

fn read_exact_at(volume: &File, buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let n = volume.seek_read(&mut buf[done..], offset)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        done += n;
        offset += n as u64;
    }
    Ok(())
}

/// Reads and parses the whole MFT. Returns it with the boot sector's volume serial.
/// Every size from the disk is treated as untrusted (the volume may be a crafted USB
/// stick): arithmetic is checked and allocation grows only with data actually read.
fn read_table(volume: &File, cancel: &CancelToken) -> Result<(Table, u64), FastError> {
    let io_err = |_| FastError::Unavailable("volume read failed");
    let bad = || FastError::Unavailable("unsupported $MFT layout");
    let mut boot = vec![0u8; 512];
    read_exact_at(volume, &mut boot, 0).map_err(io_err)?;
    let geo = parse::Geometry::from_boot(&boot).ok_or(FastError::Unavailable("not NTFS"))?;

    // $MFT's own record says where the rest of the table is.
    let mut rec0 = vec![0u8; geo.cluster.max(geo.record) as usize];
    let rec0_at = geo.mft_lcn.checked_mul(geo.cluster).ok_or_else(bad)?;
    read_exact_at(volume, &mut rec0, rec0_at).map_err(io_err)?;
    rec0.truncate(geo.record as usize);
    let (runs, data_size) = parse::mft_runs(&mut rec0).ok_or_else(bad)?;
    // Byte ranges of the runs, checked; the table is no bigger than they are.
    let mut extents = Vec::with_capacity(runs.len());
    let mut run_bytes = 0u64;
    for &(lcn, clusters) in &runs {
        let start = lcn.checked_mul(geo.cluster).ok_or_else(bad)?;
        let len = clusters.checked_mul(geo.cluster).ok_or_else(bad)?;
        start.checked_add(len).ok_or_else(bad)?;
        run_bytes = run_bytes.checked_add(len).ok_or_else(bad)?;
        extents.push((start, len));
    }
    let total = data_size.min(run_bytes) / geo.record;
    if total > MAX_RECORDS {
        return Err(FastError::Unavailable("MFT too large"));
    }

    // Reader thread streams chunks; this thread parses them in parallel.
    let mut nodes: Vec<MftNode> = Vec::new();
    let mut extensions: Vec<Parsed> = Vec::new();
    let result = std::thread::scope(|s| -> Result<(), FastError> {
        let (tx, rx) = mpsc::sync_channel::<io::Result<(u64, Vec<u8>)>>(2);
        s.spawn(move || {
            let mut first_rec = 0u64;
            'runs: for &(start, len) in &extents {
                let mut pos = start;
                // Can't overflow: first_rec < total <= MAX_RECORDS, record <= 64 KiB.
                let mut left = len.min((total - first_rec) * geo.record);
                while left > 0 {
                    if cancel.is_cancelled() {
                        break 'runs;
                    }
                    let n = left.min(CHUNK);
                    let mut buf = vec![0u8; n as usize];
                    let r = read_exact_at(volume, &mut buf, pos).map(|_| (first_rec, buf));
                    let failed = r.is_err();
                    if tx.send(r).is_err() || failed {
                        break 'runs;
                    }
                    first_rec += n / geo.record;
                    pos += n;
                    left -= n;
                }
                if first_rec >= total {
                    break;
                }
            }
        });
        for msg in rx {
            let (first, mut buf) = msg.map_err(io_err)?;
            // Grow only as far as the records actually read.
            let end = (first + buf.len() as u64 / geo.record) as usize;
            if nodes.len() < end {
                nodes.resize_with(end, MftNode::default);
            }
            let parsed: Vec<Parsed> = buf
                .par_chunks_mut(geo.record as usize)
                .enumerate()
                .filter_map(|(i, rec)| parse::record(rec, first + i as u64))
                .collect();
            for p in parsed {
                if p.base == p.number {
                    if let Some(slot) = nodes.get_mut(p.number as usize) {
                        slot.absorb(p, true);
                    }
                } else {
                    extensions.push(p);
                }
            }
        }
        Ok(())
    });
    result?;
    // Attributes that overflowed into extension records belong to their base record.
    for p in extensions {
        if let Some(base) = nodes.get_mut(p.base as usize) {
            if base.in_use {
                base.absorb(p, false);
            }
        }
    }
    Ok((link(nodes), geo.serial))
}

/// Builds each folder's child list (a compressed sparse layout) from the parent
/// references in the records' names. Only well-formed references count: the parent
/// must be an in-use folder with a matching sequence number, and not the record itself.
fn link(nodes: Vec<MftNode>) -> Table {
    let mut edges: Vec<(u32, u32, u16)> = Vec::new();
    for (rec, node) in nodes.iter().enumerate() {
        if !node.in_use || (rec as u64) < FIRST_USER_RECORD {
            continue;
        }
        for (i, name) in node.names.iter().enumerate() {
            let parent = name.parent & 0x0000_FFFF_FFFF_FFFF;
            let parent_seq = (name.parent >> 48) as u16;
            let Some(p) = nodes.get(parent as usize) else {
                continue;
            };
            let parent_ok = p.in_use
                && p.is_dir
                && (parent_seq == 0 || parent_seq == p.seq)
                && (parent == MFT_ROOT || parent >= FIRST_USER_RECORD)
                && parent as usize != rec;
            if parent_ok {
                edges.push((parent as u32, rec as u32, i as u16));
            }
        }
    }
    edges.par_sort_unstable_by_key(|e| e.0);
    let mut offsets = vec![0u64; nodes.len() + 1];
    for e in &edges {
        offsets[e.0 as usize + 1] += 1;
    }
    for i in 1..offsets.len() {
        offsets[i] += offsets[i - 1];
    }
    let children = edges.into_iter().map(|(_, c, n)| (c, n)).collect();
    Table {
        nodes,
        offsets,
        children,
    }
}

fn raw_attrs(node: &MftNode) -> u32 {
    let mut attrs = node.attrs;
    if node.is_dir {
        attrs |= FILE_ATTRIBUTE_DIRECTORY;
    }
    if node.reparse_tag != 0 {
        attrs |= FILE_ATTRIBUTE_REPARSE_POINT;
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::parse::Name;
    use super::*;

    fn dir(seq: u16, parents: &[u64]) -> MftNode {
        MftNode {
            in_use: true,
            is_dir: true,
            seq,
            names: parents
                .iter()
                .enumerate()
                .map(|(i, &p)| Name {
                    parent: (1 << 48) | p,
                    name: format!("d{i}").encode_utf16().collect(),
                })
                .collect(),
            ..MftNode::default()
        }
    }

    fn walk_count(nodes: Vec<MftNode>) -> usize {
        let prepared = Prepared {
            table: link(nodes),
            root_rec: MFT_ROOT as u32,
            root_meta: EntryMeta {
                name: OsString::new(),
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
            serial: 1,
        };
        let mut n = 0;
        prepared.walk(&CancelToken::new(), &mut |b| n += b.len());
        n
    }

    #[test]
    fn crafted_cycles_terminate() {
        // Records 16 and 17 are folders that are each other's parent, and 17 also
        // hangs off the root: without a visited set the walk would never end.
        let mut nodes = vec![MftNode::default(); 18];
        nodes[5] = dir(1, &[5]);
        nodes[16] = dir(1, &[17]);
        nodes[17] = dir(1, &[16, 5]);
        // root, 17 (under root), 16 (under 17); 17 again under 16 is skipped.
        assert_eq!(walk_count(nodes), 3);
    }

    #[test]
    fn many_names_for_one_folder_are_entered_once() {
        // A folder (16) with many names in the root, containing a folder with
        // many names back in 16: output stays linear, not exponential.
        let mut nodes = vec![MftNode::default(); 18];
        nodes[5] = dir(1, &[5]);
        nodes[16] = dir(1, &[5; 50]);
        nodes[17] = dir(1, &[16; 50]);
        assert_eq!(walk_count(nodes), 3);
    }

    #[test]
    fn only_local_drive_paths_are_accepted() {
        let ok = |p: &str| local_drive_path(Path::new(p));
        assert_eq!(
            ok(r"c:\Users\William"),
            Some((b'C', vec!["Users".into(), "William".into()]))
        );
        assert_eq!(ok(r"\\?\D:\"), Some((b'D', vec![])));
        for bad in [
            r"\\attacker\share",
            r"\\?\UNC\attacker\share\x",
            r"\\.\C:",
            r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"C:\file.txt:stream",
            r"\\?\C:\a:b",
            r"C:a",
            r"a\b",
        ] {
            assert!(ok(bad).is_none(), "accepted {bad}");
        }
    }

    #[test]
    fn unc_roots_are_refused_before_any_open() {
        // An unreachable address: had anything tried to open it, this would hang or
        // fail differently. The refusal is immediate and says why.
        let r = Prepared::new(Path::new(r"\\192.0.2.1\share"), &CancelToken::new());
        assert!(matches!(
            r,
            Err(FastError::Unavailable(
                "fast scan needs a folder on a local drive"
            ))
        ));
    }
}
