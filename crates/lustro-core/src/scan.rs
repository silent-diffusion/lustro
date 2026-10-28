//! Scanner and cloud-classifier interfaces. CONTRACT FILE: see `docs/CONTRACTS.md` before changing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::{CloudInfo, ScanRecord};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanMode {
    /// Portable directory walk. No special privileges.
    #[default]
    Standard,
    /// Platform fast path (Windows: NTFS MFT, needs admin). Falls back to Standard.
    Fast,
}

#[derive(Clone, Debug, Default)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub mode: ScanMode,
    /// Descend into other filesystems/mount points under `root`.
    pub cross_filesystems: bool,
    /// Descend into network/FUSE/virtual cloud drives.
    pub include_remote: bool,
    /// Absolute paths not to descend into.
    pub excluded: Vec<PathBuf>,
}

/// Raw, platform-specific attributes the scanner already has in hand.
/// Classifiers must decide from these alone; they must never open file contents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawAttrs {
    /// Windows `FILE_ATTRIBUTE_*` bits (0 elsewhere).
    pub win_attributes: u32,
    /// macOS/BSD `st_flags` (0 elsewhere).
    pub st_flags: u32,
    pub is_dir: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Descend {
    Yes,
    /// Don't enter this directory. The entry is still recorded with `NOT_DESCENDED`.
    No(SkipReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    Excluded,
    Remote,
    /// Entering would make the cloud provider download it (e.g. a dataless folder on macOS).
    CloudOnlyFolder,
    OtherFilesystem,
    PseudoFilesystem,
}

/// Implemented by `lustro-cloud`; consumed by `lustro-scan`. Must be cheap and never block on I/O
/// beyond what the scanner already did.
pub trait CloudClassifier: Send + Sync {
    fn classify(&self, path: &Path, raw: &RawAttrs) -> Option<CloudInfo>;
    fn should_descend(&self, path: &Path, raw: &RawAttrs) -> Descend;
}

/// A classifier that treats nothing as cloud and descends everywhere. For tests.
pub struct NoCloud;

impl CloudClassifier for NoCloud {
    fn classify(&self, _: &Path, _: &RawAttrs) -> Option<CloudInfo> {
        None
    }
    fn should_descend(&self, _: &Path, _: &RawAttrs) -> Descend {
        Descend::Yes
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub files: u64,
    pub dirs: u64,
    pub allocated_bytes: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWarning {
    /// Display path. Shown to the user only; never logged or sent anywhere.
    pub path: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub mode_used: ScanMode,
    pub progress: ScanProgress,
    pub cancelled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("root not found or not a directory")]
    BadRoot,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Receives scan output. Scanners call `batch` with groups of records (aim for
/// roughly 1,000–10,000 records or every ~100 ms, whichever comes first).
pub trait ScanSink: Send + Sync {
    fn batch(&self, records: Vec<ScanRecord>);
    fn progress(&self, progress: ScanProgress);
    fn warning(&self, warning: ScanWarning);
}

#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub trait Scanner {
    fn scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, ScanError>;
}
