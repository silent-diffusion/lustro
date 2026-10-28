//! The `CloudClassifier` the scanner calls for every entry.

use std::collections::HashMap;
use std::path::Path;

use lustro_core::model::{CloudInfo, CloudProvider, CloudState};
use lustro_core::scan::{CloudClassifier, Descend, RawAttrs, SkipReason};

use crate::mounts::{fs_skip_reason, MountEntry};
use crate::paths;
use crate::rules::{attr_state, is_cloud_only_folder, Os};
use crate::{SyncRoot, SyncRootKind};

#[derive(Clone, Debug)]
struct Root {
    key: String,
    provider: CloudProvider,
    kind: SyncRootKind,
}

/// Classifies entries from their attributes plus a table of sync roots and mounts that was
/// built once before the scan. Every call is in-memory string matching; no I/O.
#[derive(Clone, Debug)]
pub struct Classifier {
    os: Os,
    /// Longest key first, so nested roots (a cache inside a profile, etc.) win.
    roots: Vec<Root>,
    /// Mount points the scanner must not enter, keyed by normalised path.
    mounts: HashMap<String, SkipReason>,
}

impl Classifier {
    /// A classifier for `os` with the given sync roots and no mount table.
    pub fn new(os: Os, roots: &[SyncRoot]) -> Self {
        let ci = os.case_insensitive();
        let mut roots: Vec<Root> = roots
            .iter()
            .map(|r| Root {
                key: paths::key(&r.root_path.to_string_lossy(), ci),
                provider: r.provider,
                kind: r.kind,
            })
            .collect();
        roots.sort_by_key(|r| std::cmp::Reverse(r.key.len()));
        Self {
            os,
            roots,
            mounts: HashMap::new(),
        }
    }

    /// Adds mount points to stay out of (network, FUSE and pseudo filesystems).
    pub fn with_mounts(mut self, mounts: &[MountEntry]) -> Self {
        let ci = self.os.case_insensitive();
        for m in mounts {
            if let Some(reason) = fs_skip_reason(&m.fs_type) {
                self.mounts.insert(paths::key(&m.mount_point, ci), reason);
            }
        }
        self
    }

    /// Discovers this machine's sync roots and mounts. Call once per scan.
    pub fn detect() -> Self {
        Self::new(Os::current(), &crate::list_sync_roots()).with_mounts(&crate::discover::mounts())
    }

    /// The provider and kind of the sync root containing `path`, if any.
    pub fn sync_root_of(&self, path: &Path) -> Option<(CloudProvider, SyncRootKind)> {
        self.find(&path.to_string_lossy())
            .map(|r| (r.provider, r.kind))
    }

    fn find(&self, path: &str) -> Option<&Root> {
        let ci = self.os.case_insensitive();
        self.roots
            .iter()
            .find(|r| paths::is_within(path, &r.key, ci))
    }
}

impl CloudClassifier for Classifier {
    fn classify(&self, path: &Path, raw: &RawAttrs) -> Option<CloudInfo> {
        let p = path.to_string_lossy();
        let root = self.find(&p);
        let state = match root.map(|r| r.kind) {
            // Nothing on a virtual drive or cloud mount is stored on this disk.
            Some(SyncRootKind::VirtualDrive | SyncRootKind::RemoteMount) => CloudState::CloudOnly,
            // A provider's cache is ordinary local data, labelled with the provider.
            Some(SyncRootKind::Cache) => CloudState::Downloaded,
            Some(SyncRootKind::Placeholder | SyncRootKind::FullCopy) => {
                attr_state(self.os, raw).unwrap_or(CloudState::Downloaded)
            }
            // Outside every known root, trust cloud attributes alone (provider unknown).
            None => attr_state(self.os, raw)?,
        };
        Some(CloudInfo {
            provider: root.map_or(CloudProvider::Other, |r| r.provider),
            state,
        })
    }

    fn should_descend(&self, path: &Path, raw: &RawAttrs) -> Descend {
        if is_cloud_only_folder(self.os, raw) {
            return Descend::No(SkipReason::CloudOnlyFolder);
        }
        let ci = self.os.case_insensitive();
        let p = path.to_string_lossy();
        if !self.mounts.is_empty() {
            if let Some(reason) = self.mounts.get(&paths::key(&p, ci)) {
                return Descend::No(*reason);
            }
        }
        let remote_root = self.roots.iter().any(|r| {
            matches!(
                r.kind,
                SyncRootKind::VirtualDrive | SyncRootKind::RemoteMount
            ) && paths::is_same(&p, &r.key, ci)
        });
        if remote_root {
            Descend::No(SkipReason::Remote)
        } else {
            Descend::Yes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{mac, win};
    use std::path::PathBuf;
    use CloudProvider::*;
    use CloudState::*;

    fn root(provider: CloudProvider, path: &str, kind: SyncRootKind) -> SyncRoot {
        SyncRoot {
            provider,
            display_name: crate::provider_name(provider).into(),
            root_path: PathBuf::from(path),
            kind,
        }
    }
    fn wa(attrs: u32) -> RawAttrs {
        RawAttrs {
            win_attributes: attrs,
            st_flags: 0,
            is_dir: false,
        }
    }
    fn wd(attrs: u32) -> RawAttrs {
        RawAttrs {
            win_attributes: attrs,
            st_flags: 0,
            is_dir: true,
        }
    }
    fn info(provider: CloudProvider, state: CloudState) -> Option<CloudInfo> {
        Some(CloudInfo { provider, state })
    }

    fn windows() -> Classifier {
        use SyncRootKind::*;
        Classifier::new(
            Os::Windows,
            &[
                root(OneDrive, r"C:\Users\William\OneDrive", Placeholder),
                root(
                    OneDrive,
                    r"C:\Users\William\OneDrive - Contoso",
                    Placeholder,
                ),
                root(ICloud, r"C:\Users\William\iCloudDrive", Placeholder),
                root(Dropbox, r"C:\Users\William\Dropbox", Placeholder),
                root(Box, r"C:\Users\William\Box", Placeholder),
                root(Other, r"D:\Nextcloud", Placeholder),
                root(GoogleDrive, r"G:\", VirtualDrive),
                root(
                    GoogleDrive,
                    r"C:\Users\William\AppData\Local\Google\DriveFS",
                    Cache,
                ),
                root(GoogleDrive, r"C:\Users\William\Google Drive", FullCopy),
            ],
        )
    }

    #[test]
    fn every_placeholder_provider_follows_the_same_rules() {
        let c = windows();
        for (provider, dir) in [
            (OneDrive, r"C:\Users\William\OneDrive"),
            (OneDrive, r"C:\Users\William\OneDrive - Contoso"),
            (ICloud, r"C:\Users\William\iCloudDrive"),
            (Dropbox, r"C:\Users\William\Dropbox"),
            (Box, r"C:\Users\William\Box"),
            (Other, r"D:\Nextcloud"),
        ] {
            let f = PathBuf::from(format!(r"{dir}\Docs\report.docx"));
            assert_eq!(
                c.classify(&f, &wa(win::RECALL_ON_DATA_ACCESS | win::UNPINNED)),
                info(provider, CloudOnly)
            );
            assert_eq!(
                c.classify(&f, &wa(win::RECALL_ON_OPEN)),
                info(provider, CloudOnly)
            );
            assert_eq!(c.classify(&f, &wa(win::OFFLINE)), info(provider, CloudOnly));
            assert_eq!(c.classify(&f, &wa(win::PINNED)), info(provider, Pinned));
            assert_eq!(
                c.classify(&f, &wa(win::UNPINNED)),
                info(provider, Downloaded)
            );
            assert_eq!(c.classify(&f, &wa(0x20)), info(provider, Downloaded));
            assert_eq!(c.should_descend(&f, &wa(0x20)), Descend::Yes);
            let sub = PathBuf::from(format!(r"{dir}\Docs"));
            assert_eq!(
                c.should_descend(&sub, &wd(0x10 | win::RECALL_ON_OPEN)),
                Descend::No(SkipReason::CloudOnlyFolder)
            );
            assert_eq!(
                c.should_descend(&sub, &wd(0x10 | win::RECALL_ON_DATA_ACCESS)),
                Descend::Yes
            );
        }
    }

    #[test]
    fn prefix_match_respects_component_boundaries_and_case() {
        let c = windows();
        let f = Path::new(r"c:\users\william\onedrive - contoso\a.txt");
        assert_eq!(c.classify(f, &wa(0x20)), info(OneDrive, Downloaded));
        assert_eq!(
            c.sync_root_of(Path::new(r"C:\Users\William\OneDriveX\a")),
            None
        );
        assert_eq!(
            c.classify(Path::new(r"\\?\C:\Users\William\Box\a"), &wa(0x20)),
            info(Box, Downloaded)
        );
    }

    #[test]
    fn outside_sync_roots() {
        let c = windows();
        let f = Path::new(r"C:\Users\William\Documents\a.txt");
        assert_eq!(c.classify(f, &wa(0x20)), None);
        // Cloud attributes without a known root: still cloud-only, provider unknown.
        assert_eq!(
            c.classify(f, &wa(win::RECALL_ON_DATA_ACCESS)),
            info(Other, CloudOnly)
        );
    }

    #[test]
    fn google_drive_stream_mirror_and_cache() {
        let c = windows();
        // Stream-mode virtual drive: listed as cloud-only, never entered by default.
        assert_eq!(
            c.should_descend(Path::new(r"G:\"), &wd(0x10)),
            Descend::No(SkipReason::Remote)
        );
        assert_eq!(
            c.classify(Path::new(r"G:\My Drive\big.mov"), &wa(0x20)),
            info(GoogleDrive, CloudOnly)
        );
        assert_eq!(
            c.should_descend(Path::new(r"G:\My Drive"), &wd(0x10)),
            Descend::Yes
        );
        // Cache: local bytes, labelled Google Drive.
        assert_eq!(
            c.classify(
                Path::new(r"C:\Users\William\AppData\Local\Google\DriveFS\1234\content_cache\a"),
                &wa(0x20)
            ),
            info(GoogleDrive, Downloaded)
        );
        // Mirror mode: ordinary local files.
        assert_eq!(
            c.classify(
                Path::new(r"C:\Users\William\Google Drive\notes.txt"),
                &wa(0x20)
            ),
            info(GoogleDrive, Downloaded)
        );
    }

    #[test]
    fn macos_file_provider_and_icloud() {
        use SyncRootKind::*;
        let c = Classifier::new(
            Os::MacOs,
            &[
                root(
                    OneDrive,
                    "/Users/william/Library/CloudStorage/OneDrive-Personal",
                    Placeholder,
                ),
                root(
                    GoogleDrive,
                    "/Users/william/Library/CloudStorage/GoogleDrive-william@example.com",
                    Placeholder,
                ),
                root(
                    Dropbox,
                    "/Users/william/Library/CloudStorage/Dropbox",
                    Placeholder,
                ),
                root(
                    Box,
                    "/Users/william/Library/CloudStorage/Box-Box",
                    Placeholder,
                ),
                root(
                    ICloud,
                    "/Users/william/Library/Mobile Documents",
                    Placeholder,
                ),
            ],
        );
        let dataless = |is_dir| RawAttrs {
            win_attributes: 0,
            st_flags: mac::SF_DATALESS,
            is_dir,
        };
        let local = |is_dir| RawAttrs {
            win_attributes: 0,
            st_flags: 0,
            is_dir,
        };
        for (provider, dir) in [
            (
                OneDrive,
                "/Users/william/Library/CloudStorage/OneDrive-Personal",
            ),
            (
                GoogleDrive,
                "/Users/william/Library/CloudStorage/GoogleDrive-william@example.com/My Drive",
            ),
            (Dropbox, "/Users/william/Library/CloudStorage/Dropbox"),
            (Box, "/Users/william/Library/CloudStorage/Box-Box"),
            (
                ICloud,
                "/Users/william/Library/Mobile Documents/com~apple~CloudDocs",
            ),
        ] {
            let f = PathBuf::from(format!("{dir}/a.pdf"));
            assert_eq!(c.classify(&f, &dataless(false)), info(provider, CloudOnly));
            assert_eq!(c.classify(&f, &local(false)), info(provider, Downloaded));
            let d = PathBuf::from(format!("{dir}/Photos"));
            assert_eq!(
                c.should_descend(&d, &dataless(true)),
                Descend::No(SkipReason::CloudOnlyFolder)
            );
            assert_eq!(c.should_descend(&d, &local(true)), Descend::Yes);
        }
        // A dataless folder anywhere is never entered.
        assert_eq!(
            c.should_descend(Path::new("/Users/william/Elsewhere"), &dataless(true)),
            Descend::No(SkipReason::CloudOnlyFolder)
        );
        assert_eq!(
            c.classify(Path::new("/Users/william/x"), &local(false)),
            None
        );
    }

    #[test]
    fn linux_mounts_and_full_copies() {
        use SyncRootKind::*;
        let mounts = crate::mounts::parse_mountinfo(
            "22 1 0:21 / /proc rw - proc proc rw\n\
             25 1 8:2 / / rw - ext4 /dev/sda2 rw\n\
             40 25 0:40 / /home/william/OneDrive rw - fuse.rclone onedrive: rw\n\
             41 25 0:41 / /mnt/nas rw - nfs4 nas:/export rw\n",
        );
        let c = Classifier::new(
            Os::Linux,
            &[
                root(Dropbox, "/home/william/Dropbox", FullCopy),
                root(OneDrive, "/home/william/OneDrive", RemoteMount),
            ],
        )
        .with_mounts(&mounts);
        let dir = RawAttrs {
            win_attributes: 0,
            st_flags: 0,
            is_dir: true,
        };
        let file = RawAttrs {
            is_dir: false,
            ..dir
        };
        assert_eq!(
            c.should_descend(Path::new("/proc"), &dir),
            Descend::No(SkipReason::PseudoFilesystem)
        );
        assert_eq!(
            c.should_descend(Path::new("/mnt/nas/"), &dir),
            Descend::No(SkipReason::Remote)
        );
        assert_eq!(
            c.should_descend(Path::new("/home/william/OneDrive"), &dir),
            Descend::No(SkipReason::Remote)
        );
        assert_eq!(c.should_descend(Path::new("/"), &dir), Descend::Yes);
        assert_eq!(c.should_descend(Path::new("/home"), &dir), Descend::Yes);
        assert_eq!(
            c.classify(Path::new("/home/william/Dropbox/a.txt"), &file),
            info(Dropbox, Downloaded)
        );
        assert_eq!(
            c.classify(Path::new("/home/william/OneDrive"), &dir),
            info(OneDrive, CloudOnly)
        );
        // Linux is case-sensitive.
        assert_eq!(
            c.classify(Path::new("/home/william/dropbox/a"), &file),
            None
        );
    }

    #[test]
    fn longest_root_wins() {
        use SyncRootKind::*;
        let c = Classifier::new(
            Os::Windows,
            &[
                root(Other, r"C:\Sync", Placeholder),
                root(Dropbox, r"C:\Sync\Dropbox", Placeholder),
            ],
        );
        assert_eq!(
            c.sync_root_of(Path::new(r"C:\Sync\Dropbox\a")),
            Some((Dropbox, Placeholder))
        );
        assert_eq!(
            c.sync_root_of(Path::new(r"C:\Sync\b")),
            Some((Other, Placeholder))
        );
    }
}
