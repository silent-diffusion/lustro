//! Mount-table parsing and filesystem-type rules (pure; used on Linux, tested everywhere).

use lustro_core::scan::SkipReason;

/// One mount from `/proc/self/mountinfo` (or `getfsstat` on macOS).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountEntry {
    pub mount_point: String,
    pub fs_type: String,
    pub source: String,
}

/// Parse `/proc/self/mountinfo`. Malformed lines are skipped.
///
/// Format: `id parent major:minor root mount_point options [optional…] - fstype source super_opts`
pub fn parse_mountinfo(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let mount_point = left.split(' ').nth(4)?;
            let mut r = right.split(' ');
            let fs_type = r.next()?;
            let source = r.next().unwrap_or("");
            Some(MountEntry {
                mount_point: unescape(mount_point),
                fs_type: fs_type.to_string(),
                source: unescape(source),
            })
        })
        .collect()
}

/// Undo mountinfo's octal escapes (`\040` space, `\011` tab, `\012` newline, `\134` backslash).
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = u32::from(b[i + 1] - b'0') * 64
                + u32::from(b[i + 2] - b'0') * 8
                + u32::from(b[i + 3] - b'0');
            if let Ok(byte) = u8::try_from(v) {
                out.push(byte);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Kernel/virtual filesystems that hold no user data on disk.
const PSEUDO: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "tmpfs",
    "ramfs",
    "cgroup",
    "cgroup2",
    "securityfs",
    "debugfs",
    "tracefs",
    "configfs",
    "pstore",
    "bpf",
    "mqueue",
    "hugetlbfs",
    "fusectl",
    "binfmt_misc",
    "autofs",
    "efivarfs",
    "rpc_pipefs",
    "nsfs",
    "selinuxfs",
    "fuse.lxcfs",
    "fuse.portal",
    "devfs",
    "overlay.proc",
];

/// Network and remote filesystems. FUSE is remote unless listed as pseudo or local.
const REMOTE: &[&str] = &[
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "afpfs",
    "webdav",
    "ncpfs",
    "sshfs",
    "9p",
    "virtiofs",
    "ceph",
    "glusterfs",
    "afs",
    "davfs",
    "lustre",
    "gpfs",
    "fuse",
];

/// FUSE filesystems that are really local disks (NTFS/exFAT drivers, encrypted overlays).
const LOCAL_FUSE: &[&str] = &[
    "fuseblk",
    "fuse.ntfs-3g",
    "fuse.exfat",
    "fuse.gocryptfs",
    "fuse.encfs",
    "fuse.cryfs",
    "fuse.mergerfs",
    "fuse.bindfs",
];

/// Whether the scanner should stay out of a mount of this filesystem type.
pub fn fs_skip_reason(fs_type: &str) -> Option<SkipReason> {
    let t = fs_type.to_ascii_lowercase();
    if PSEUDO.contains(&t.as_str()) {
        Some(SkipReason::PseudoFilesystem)
    } else if LOCAL_FUSE.contains(&t.as_str()) {
        None
    } else if REMOTE.contains(&t.as_str())
        || t.starts_with("fuse.")
        || t.starts_with("osxfuse")
        || t.starts_with("macfuse")
        || t.starts_with("nfs")
    {
        Some(SkipReason::Remote)
    } else {
        None
    }
}

/// True for mounts that are a cloud service (rclone and similar), as opposed to a NAS share.
pub fn is_cloud_mount(m: &MountEntry) -> bool {
    m.fs_type.eq_ignore_ascii_case("fuse.rclone")
        || crate::provider_from_hint(&m.source) != lustro_core::model::CloudProvider::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
22 1 0:21 / /proc rw,nosuid,nodev,noexec,relatime shared:12 - proc proc rw
23 1 0:22 / /sys rw,nosuid shared:7 - sysfs sysfs rw
24 1 0:5 / /dev rw,nosuid shared:2 - devtmpfs udev rw,size=8000000k
25 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
26 25 0:23 / /run rw,nosuid,nodev shared:5 - tmpfs tmpfs rw
40 25 0:40 / /home/william/OneDrive rw,nosuid,nodev,relatime shared:90 - fuse.rclone onedrive: rw,user_id=1000
41 25 0:41 / /mnt/nas rw,relatime shared:91 - cifs //nas.local/share rw
42 25 8:17 / /media/william/My\\040Disk rw,relatime shared:92 - fuseblk /dev/sdb1 rw
43 25 7:0 / /snap/core/1 ro,nodev,relatime shared:93 - squashfs /dev/loop0 ro
garbage line without separator
";

    #[test]
    fn parses_mountinfo() {
        let m = parse_mountinfo(SAMPLE);
        assert_eq!(m.len(), 9);
        assert_eq!(m[0].mount_point, "/proc");
        assert_eq!(m[0].fs_type, "proc");
        assert_eq!(m[5].mount_point, "/home/william/OneDrive");
        assert_eq!(m[5].source, "onedrive:");
        assert_eq!(m[7].mount_point, "/media/william/My Disk");
    }

    #[test]
    fn classifies_fs_types() {
        use SkipReason::*;
        let m = parse_mountinfo(SAMPLE);
        let r: Vec<_> = m.iter().map(|e| fs_skip_reason(&e.fs_type)).collect();
        assert_eq!(
            r,
            vec![
                Some(PseudoFilesystem),
                Some(PseudoFilesystem),
                Some(PseudoFilesystem),
                None,
                Some(PseudoFilesystem),
                Some(Remote),
                Some(Remote),
                None,
                None,
            ]
        );
        assert_eq!(fs_skip_reason("fuse.sshfs"), Some(Remote));
        assert_eq!(fs_skip_reason("fuse.gvfsd-fuse"), Some(Remote));
        assert_eq!(fs_skip_reason("nfs4"), Some(Remote));
        assert_eq!(fs_skip_reason("9p"), Some(Remote));
        assert_eq!(fs_skip_reason("fuse.lxcfs"), Some(PseudoFilesystem));
        assert_eq!(fs_skip_reason("btrfs"), None);
        assert_eq!(fs_skip_reason("apfs"), None);
        assert_eq!(fs_skip_reason("smbfs"), Some(Remote));
        assert_eq!(fs_skip_reason("macfuse"), Some(Remote));
    }

    #[test]
    fn cloud_mounts() {
        let m = parse_mountinfo(SAMPLE);
        assert!(is_cloud_mount(&m[5]));
        assert!(!is_cloud_mount(&m[6]));
    }

    #[test]
    fn unescape_handles_edge_cases() {
        assert_eq!(unescape(r"a\040b"), "a b");
        assert_eq!(unescape(r"a\134b"), r"a\b");
        assert_eq!(unescape(r"trail\04"), r"trail\04");
        assert_eq!(unescape(r"x\999"), r"x\999");
    }
}
