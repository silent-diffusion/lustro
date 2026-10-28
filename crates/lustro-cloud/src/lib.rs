//! `lustro_core::scan::CloudClassifier` implementations.
//!
//! The same rules apply to every provider (OneDrive, Google Drive, iCloud, Dropbox, Box and
//! anything else that registers a sync root):
//! - cloud-only files use 0 bytes locally (`CloudState::CloudOnly`);
//! - downloaded and pinned files count as local space (`Downloaded` / `Pinned`).
//!
//! Classification looks only at attributes the scanner already has (`RawAttrs`) plus a table of
//! sync roots discovered once up front. Nothing here opens, reads or hydrates a file.
//!
//! Layout:
//! - [`rules`]: pure per-OS attribute rules (compiled and unit-tested on every OS);
//! - [`classifier`]: [`Classifier`], the `CloudClassifier` used by the scanner;
//! - [`mounts`]: Linux `mountinfo` parsing and filesystem-type rules (pure);
//! - `discover`: per-OS sync-root discovery (registry, `~/Library/CloudStorage`, mount table).

pub mod classifier;
mod discover;
pub mod mounts;
pub mod paths;
pub mod provider;
pub mod rules;

use std::path::PathBuf;

use lustro_core::model::CloudProvider;
use serde::{Deserialize, Serialize};

pub use classifier::Classifier;
pub use provider::provider_from_hint;

/// How a provider puts its files on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRootKind {
    /// Placeholder files in a normal folder (Windows Cloud Files API, macOS File Provider).
    /// Per-file state comes from attributes; "free up space" is possible.
    Placeholder,
    /// Ordinary full local copies (e.g. classic Dropbox, Google Drive Mirror mode).
    FullCopy,
    /// A virtual drive whose data is not on this disk (Google Drive Stream mode `G:`).
    /// Listed as cloud-only and not descended by default.
    VirtualDrive,
    /// The local cache backing a virtual drive (e.g. Google DriveFS). Counts as local space
    /// and is labelled with the provider, but cannot be "freed up" per file.
    Cache,
    /// A network/FUSE mount of a cloud service (rclone etc.). Not descended by default.
    RemoteMount,
}

/// One discovered sync root. `root_path` is display-only: never log it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRoot {
    pub provider: CloudProvider,
    /// As the client names itself where known (e.g. "OneDrive - Personal"), otherwise the
    /// provider's name.
    pub display_name: String,
    pub root_path: PathBuf,
    pub kind: SyncRootKind,
}

/// Every sync root, virtual drive, provider cache and cloud mount found on this machine, for
/// Settings' per-provider show/hide and the cloud summary. Reads only registry values,
/// directory listings of known config folders and the mount table.
pub fn list_sync_roots() -> Vec<SyncRoot> {
    discover::sync_roots()
}

/// Google Drive for desktop's local cache folder (where Stream-mode files that are available
/// offline actually live), if the client is installed. Reads the configured path when the
/// client stores one, otherwise the default location if it exists.
pub fn google_drive_cache() -> Option<PathBuf> {
    discover::google_drive_cache()
}

/// The OS mount table (Linux `mountinfo`, macOS `getfsstat`, Windows mapped network drives).
pub fn system_mounts() -> Vec<mounts::MountEntry> {
    discover::mounts()
}

/// Human-readable provider name used when a client doesn't give one.
pub fn provider_name(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::OneDrive => "OneDrive",
        CloudProvider::GoogleDrive => "Google Drive",
        CloudProvider::ICloud => "iCloud Drive",
        CloudProvider::Dropbox => "Dropbox",
        CloudProvider::Box => "Box",
        CloudProvider::Other => "Cloud storage",
    }
}
