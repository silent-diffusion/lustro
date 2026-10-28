//! Pure per-OS attribute rules. No I/O; compiled and tested on every OS.

use lustro_core::model::CloudState;
use lustro_core::scan::RawAttrs;

/// Which operating system's attribute rules to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub const fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }

    /// Whether paths compare case-insensitively (NTFS, default APFS).
    pub const fn case_insensitive(self) -> bool {
        matches!(self, Os::Windows | Os::MacOs)
    }
}

/// Windows `FILE_ATTRIBUTE_*` bits used by the Cloud Files API.
pub mod win {
    pub const OFFLINE: u32 = 0x0000_1000;
    pub const RECALL_ON_OPEN: u32 = 0x0004_0000;
    pub const PINNED: u32 = 0x0008_0000;
    pub const UNPINNED: u32 = 0x0010_0000;
    pub const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    /// Any of these means the data isn't on this disk.
    pub const CLOUD_ONLY: u32 = OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS;
    /// Every attribute that marks an item as managed by a cloud provider.
    pub const ANY_CLOUD: u32 = CLOUD_ONLY | PINNED | UNPINNED;
}

/// macOS `st_flags` bits.
pub mod mac {
    /// `SF_DATALESS`: the file or folder's data lives only with the File Provider.
    pub const SF_DATALESS: u32 = 0x4000_0000;
}

/// State implied by the attributes alone, or `None` when they say nothing about cloud state.
///
/// Windows files: `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN` or `OFFLINE` → cloud-only;
/// otherwise `PINNED` → pinned.
///
/// Windows directories carry no data of their own. Only `RECALL_ON_OPEN` (a virtual,
/// unpopulated folder) makes one cloud-only; `RECALL_ON_DATA_ACCESS` on a folder just means
/// some children are cloud-only, so the folder is treated like any other.
///
/// macOS: `SF_DATALESS` → cloud-only. The pinned state lives in an extended attribute, which
/// is not part of `RawAttrs`, so macOS never reports `Pinned` here.
pub fn attr_state(os: Os, raw: &RawAttrs) -> Option<CloudState> {
    match os {
        Os::Windows => {
            let a = raw.win_attributes;
            let cloud_only = if raw.is_dir {
                a & win::RECALL_ON_OPEN != 0
            } else {
                a & win::CLOUD_ONLY != 0
            };
            if cloud_only {
                Some(CloudState::CloudOnly)
            } else if a & win::PINNED != 0 {
                Some(CloudState::Pinned)
            } else if a & win::UNPINNED != 0 {
                Some(CloudState::Downloaded)
            } else {
                None
            }
        }
        Os::MacOs => (raw.st_flags & mac::SF_DATALESS != 0).then_some(CloudState::CloudOnly),
        Os::Linux => None,
    }
}

/// True for a directory whose listing would make the provider fetch it from the cloud.
/// The scanner must not enter it.
pub fn is_cloud_only_folder(os: Os, raw: &RawAttrs) -> bool {
    raw.is_dir
        && match os {
            Os::Windows => raw.win_attributes & win::RECALL_ON_OPEN != 0,
            Os::MacOs => raw.st_flags & mac::SF_DATALESS != 0,
            Os::Linux => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CloudState::*;

    fn w(attrs: u32, is_dir: bool) -> RawAttrs {
        RawAttrs {
            win_attributes: attrs,
            st_flags: 0,
            is_dir,
        }
    }
    fn m(flags: u32, is_dir: bool) -> RawAttrs {
        RawAttrs {
            win_attributes: 0,
            st_flags: flags,
            is_dir,
        }
    }

    const ARCHIVE: u32 = 0x20;
    const DIRECTORY: u32 = 0x10;
    const REPARSE: u32 = 0x400;
    const SPARSE: u32 = 0x200;

    #[test]
    fn windows_files() {
        let os = Os::Windows;
        assert_eq!(attr_state(os, &w(ARCHIVE, false)), None);
        assert_eq!(
            attr_state(os, &w(ARCHIVE | win::RECALL_ON_DATA_ACCESS, false)),
            Some(CloudOnly)
        );
        assert_eq!(
            attr_state(os, &w(win::RECALL_ON_OPEN, false)),
            Some(CloudOnly)
        );
        assert_eq!(attr_state(os, &w(win::OFFLINE, false)), Some(CloudOnly));
        // Typical OneDrive "online-only" file: sparse reparse point + recall + unpinned.
        assert_eq!(
            attr_state(
                os,
                &w(
                    SPARSE | REPARSE | win::RECALL_ON_DATA_ACCESS | win::UNPINNED,
                    false
                )
            ),
            Some(CloudOnly)
        );
        assert_eq!(
            attr_state(os, &w(ARCHIVE | win::PINNED, false)),
            Some(Pinned)
        );
        assert_eq!(
            attr_state(os, &w(ARCHIVE | win::UNPINNED, false)),
            Some(Downloaded)
        );
        // Pinned but not yet downloaded: still no data on disk.
        assert_eq!(
            attr_state(os, &w(win::PINNED | win::RECALL_ON_DATA_ACCESS, false)),
            Some(CloudOnly)
        );
    }

    #[test]
    fn windows_directories() {
        let os = Os::Windows;
        let partial = w(DIRECTORY | win::RECALL_ON_DATA_ACCESS, true);
        assert_eq!(attr_state(os, &partial), None);
        assert!(!is_cloud_only_folder(os, &partial));

        let virt = w(DIRECTORY | win::RECALL_ON_OPEN, true);
        assert_eq!(attr_state(os, &virt), Some(CloudOnly));
        assert!(is_cloud_only_folder(os, &virt));

        let pinned = w(DIRECTORY | win::PINNED, true);
        assert_eq!(attr_state(os, &pinned), Some(Pinned));
        assert!(!is_cloud_only_folder(os, &pinned));
    }

    #[test]
    fn macos_dataless() {
        let os = Os::MacOs;
        assert_eq!(attr_state(os, &m(0, false)), None);
        assert_eq!(attr_state(os, &m(mac::SF_DATALESS, false)), Some(CloudOnly));
        assert!(is_cloud_only_folder(os, &m(mac::SF_DATALESS, true)));
        assert!(!is_cloud_only_folder(os, &m(mac::SF_DATALESS, false)));
        assert!(!is_cloud_only_folder(os, &m(0, true)));
        // Windows bits mean nothing on macOS and vice versa.
        assert_eq!(attr_state(os, &w(win::RECALL_ON_DATA_ACCESS, false)), None);
        assert_eq!(attr_state(Os::Windows, &m(mac::SF_DATALESS, false)), None);
    }

    #[test]
    fn linux_has_no_attribute_state() {
        assert_eq!(attr_state(Os::Linux, &w(u32::MAX, false)), None);
        assert!(!is_cloud_only_folder(Os::Linux, &m(u32::MAX, true)));
    }
}
