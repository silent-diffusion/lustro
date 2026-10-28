//! Shared data model. CONTRACT FILE: see `docs/CONTRACTS.md` before changing.

use std::ffi::OsString;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// Id of an entry within one scan. Assigned by the scanner; the root is always 0.
pub type NodeId = u32;

/// Id of the root entry of every scan.
pub const ROOT_ID: NodeId = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    File,
    Dir,
    /// Symlink, junction or other reparse point. Never followed.
    Link,
    Other,
}

/// Stable identity of a file on its volume, used to count hard links once.
/// Windows: volume serial + file index. Unix: st_dev + st_ino.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId {
    pub volume: u64,
    pub index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudProvider {
    OneDrive,
    GoogleDrive,
    ICloud,
    Dropbox,
    Box,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudState {
    /// Listed locally but its data lives only in the cloud (~0 bytes on disk).
    CloudOnly,
    /// Data is on disk and can be freed back to the cloud.
    Downloaded,
    /// The user chose to always keep it on this device.
    Pinned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudInfo {
    pub provider: CloudProvider,
    pub state: CloudState,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct EntryFlags: u32 {
        const HIDDEN      = 1 << 0;
        const SYSTEM      = 1 << 1;
        const READONLY    = 1 << 2;
        const COMPRESSED  = 1 << 3;
        const SPARSE      = 1 << 4;
        /// A mount point or a different filesystem than its parent.
        const MOUNT_POINT = 1 << 5;
        /// A network, FUSE or virtual cloud drive. Not descended by default.
        const REMOTE      = 1 << 6;
        /// The scanner could not read this entry (permissions, etc.).
        const ACCESS_DENIED = 1 << 7;
        /// Not descended into (excluded, remote, cloud-only folder, boundary).
        const NOT_DESCENDED = 1 << 8;
    }
}

/// Metadata for one entry, as produced by a scanner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryMeta {
    /// Raw name. May be non-UTF-8 on Linux; display with `to_string_lossy`.
    pub name: OsString,
    pub kind: NodeKind,
    /// Size of the data as reported by the filesystem (bytes).
    pub logical_size: u64,
    /// Bytes actually allocated on this disk. Ground truth for "used space".
    /// ~0 for cloud-only files; smaller than logical for compressed/sparse files.
    pub allocated_size: u64,
    /// Last-modified time, Unix seconds.
    pub modified: Option<i64>,
    /// Last-accessed time, Unix seconds (often unreliable; treat as a hint).
    pub accessed: Option<i64>,
    pub file_id: Option<FileId>,
    /// Number of hard links to this file (1 for most files).
    pub link_count: u32,
    pub cloud: Option<CloudInfo>,
    pub flags: EntryFlags,
}

/// One scanned entry. Scanners may emit children before their parents
/// (the MFT fast path does); `TreeBuilder` must cope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRecord {
    pub id: NodeId,
    /// `None` only for the root.
    pub parent: Option<NodeId>,
    pub meta: EntryMeta,
}
