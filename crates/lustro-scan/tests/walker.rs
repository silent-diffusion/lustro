//! Walker tests on temp-directory trees. Nothing here prints paths.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use lustro_core::model::{
    CloudInfo, CloudProvider, CloudState, EntryFlags, NodeId, NodeKind, ScanRecord, ROOT_ID,
};
use lustro_core::scan::{
    CancelToken, CloudClassifier, Descend, NoCloud, RawAttrs, ScanError, ScanMode, ScanOptions,
    ScanProgress, ScanSink, ScanSummary, ScanWarning, Scanner, SkipReason,
};
use lustro_scan::{SystemScanner, Walker};

#[derive(Default)]
struct Collect {
    records: Mutex<Vec<ScanRecord>>,
    batches: Mutex<Vec<usize>>,
    warnings: Mutex<Vec<ScanWarning>>,
    progress: AtomicUsize,
}

impl ScanSink for Collect {
    fn batch(&self, records: Vec<ScanRecord>) {
        self.batches.lock().unwrap().push(records.len());
        self.records.lock().unwrap().extend(records);
    }
    fn progress(&self, _: ScanProgress) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }
    fn warning(&self, w: ScanWarning) {
        self.warnings.lock().unwrap().push(w);
    }
}

/// Scan output keyed by path relative to the root ("" is the root).
struct Out {
    summary: ScanSummary,
    by_path: HashMap<PathBuf, ScanRecord>,
    records: Vec<ScanRecord>,
    sink: Collect,
}

impl Out {
    fn get(&self, rel: &str) -> &ScanRecord {
        self.by_path
            .get(Path::new(rel))
            .unwrap_or_else(|| panic!("missing record for a test entry"))
    }
    fn has(&self, rel: &str) -> bool {
        self.by_path.contains_key(Path::new(rel))
    }
    fn count(&self, kind: NodeKind) -> usize {
        self.records.iter().filter(|r| r.meta.kind == kind).count()
    }
}

fn scan_with(
    scanner: &dyn Scanner,
    options: ScanOptions,
    classifier: &dyn CloudClassifier,
) -> Result<Out, ScanError> {
    let sink = Collect::default();
    let summary = scanner.scan(&options, classifier, &sink, &CancelToken::new())?;
    let records = sink.records.lock().unwrap().clone();
    Ok(Out {
        summary,
        by_path: paths(&records),
        records,
        sink,
    })
}

fn scan(root: &Path) -> Out {
    scan_with(&Walker::default(), opts(root), &NoCloud).unwrap()
}

fn opts(root: &Path) -> ScanOptions {
    ScanOptions {
        root: root.to_path_buf(),
        ..ScanOptions::default()
    }
}

/// Checks structural invariants and returns records keyed by relative path.
fn paths(records: &[ScanRecord]) -> HashMap<PathBuf, ScanRecord> {
    let mut by_id: HashMap<NodeId, &ScanRecord> = HashMap::new();
    for (pos, r) in records.iter().enumerate() {
        assert!(by_id.insert(r.id, r).is_none(), "duplicate id");
        match r.parent {
            None => assert_eq!((r.id, pos), (ROOT_ID, 0), "root must come first"),
            Some(p) => {
                let parent = by_id.get(&p).expect("parent emitted before child");
                assert_eq!(parent.meta.kind, NodeKind::Dir);
            }
        }
    }
    records
        .iter()
        .map(|r| {
            let mut parts = Vec::new();
            let mut cur = r;
            while let Some(p) = cur.parent {
                parts.push(cur.meta.name.clone());
                cur = by_id[&p];
            }
            let rel: PathBuf = parts.iter().rev().collect();
            (rel, r.clone())
        })
        .collect()
}

fn write(path: &Path, len: usize) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(path, vec![0x5Au8; len]).unwrap();
}

#[test]
fn nested_tree_counts_and_sizes() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("a/one.bin"), 1000);
    write(&root.join("a/b/two.bin"), 70_000);
    write(&root.join("a/b/c/empty.bin"), 0);
    write(&root.join("日本語-ñ-🙂.txt"), 10);
    fs::create_dir_all(root.join("a/b/c/d")).unwrap();

    let out = scan(root);
    assert_eq!(out.summary.mode_used, ScanMode::Standard);
    assert!(!out.summary.cancelled);
    assert_eq!(out.count(NodeKind::Dir), 5); // root, a, b, c, d
    assert_eq!(out.count(NodeKind::File), 4);
    assert_eq!(out.summary.progress.files, 4);
    assert_eq!(out.summary.progress.dirs, 5);

    assert_eq!(out.get("").meta.kind, NodeKind::Dir);
    let two = &out.get("a/b/two.bin").meta;
    assert_eq!(two.kind, NodeKind::File);
    assert_eq!(two.logical_size, 70_000);
    assert!(two.allocated_size >= 70_000, "allocated covers the data");
    assert!(two.allocated_size < 70_000 + 1024 * 1024);
    assert_eq!(two.link_count, 1);
    assert!(two.file_id.is_some());
    assert!(two.modified.is_some_and(|t| t > 1_600_000_000));
    assert_eq!(out.get("a/one.bin").meta.logical_size, 1000);
    assert_eq!(out.get("a/b/c/empty.bin").meta.allocated_size, 0);
    assert_eq!(out.get("日本語-ñ-🙂.txt").meta.logical_size, 10);
    assert_eq!(
        out.get("日本語-ñ-🙂.txt").meta.name,
        OsString::from("日本語-ñ-🙂.txt")
    );
    assert!(out.sink.warnings.lock().unwrap().is_empty());
    assert!(out.sink.progress.load(Ordering::Relaxed) >= 1);
}

#[test]
fn hard_links_share_file_id() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("x/orig.bin"), 50_000);
    fs::create_dir_all(root.join("y")).unwrap();
    fs::hard_link(root.join("x/orig.bin"), root.join("y/link.bin")).unwrap();

    let out = scan(root);
    let a = &out.get("x/orig.bin").meta;
    let b = &out.get("y/link.bin").meta;
    assert_eq!(a.link_count, 2);
    assert_eq!(b.link_count, 2);
    assert_eq!(a.file_id, b.file_id);
    assert!(a.file_id.is_some());
    assert_ne!(out.get("x").meta.file_id, a.file_id);
}

#[test]
fn links_are_recorded_not_followed() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("inner/data.bin"), 100);
    let made = make_dir_link(&root.join("inner"), &root.join("inner/loop"));
    if !made {
        eprintln!("skipped: could not create a directory link here");
        return;
    }
    let out = scan(root);
    let link = &out.get("inner/loop").meta;
    assert_eq!(link.kind, NodeKind::Link);
    assert!(!out.has("inner/loop/data.bin"), "link was followed");
    assert_eq!(out.count(NodeKind::File), 1);
}

#[cfg(windows)]
fn make_dir_link(target: &Path, link: &Path) -> bool {
    // Junctions need no special privilege (unlike symlinks).
    std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn make_dir_link(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
#[test]
fn file_symlink_is_a_link_when_allowed() {
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("real.bin"), 4096);
    if std::os::windows::fs::symlink_file(t.path().join("real.bin"), t.path().join("sym.bin"))
        .is_err()
    {
        eprintln!("skipped: symlinks need Developer Mode or admin");
        return;
    }
    let out = scan(t.path());
    assert_eq!(out.get("sym.bin").meta.kind, NodeKind::Link);
    assert_eq!(out.get("real.bin").meta.kind, NodeKind::File);
}

#[test]
fn sparse_file_allocates_less_than_its_length() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("sparse.bin");
    let f = fs::File::create(&path).unwrap();
    make_sparse(&f);
    f.set_len(64 << 20).unwrap();
    drop(f);

    let out = scan(t.path());
    let m = &out.get("sparse.bin").meta;
    assert_eq!(m.logical_size, 64 << 20);
    assert!(
        m.allocated_size < 1 << 20,
        "sparse file allocated {} bytes",
        m.allocated_size
    );
    assert!(m.flags.contains(EntryFlags::SPARSE));
}

#[cfg(windows)]
fn make_sparse(f: &fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let mut returned = 0u32;
    // SAFETY: valid open handle; no buffers; synchronous call.
    #[allow(unsafe_code)]
    let ok = unsafe {
        DeviceIoControl(
            f.as_raw_handle(),
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "FSCTL_SET_SPARSE failed");
}

#[cfg(unix)]
fn make_sparse(_: &fs::File) {
    // Extending with set_len leaves a hole on every common Unix filesystem.
}

#[test]
fn excluded_dir_is_recorded_but_not_entered() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("keep/a.bin"), 10);
    write(&root.join("skipme/b.bin"), 10);
    write(&root.join("skipme/deep/c.bin"), 10);

    let mut o = opts(root);
    o.excluded = vec![root.join("skipme")];
    let out = scan_with(&Walker::default(), o, &NoCloud).unwrap();
    let skipped = &out.get("skipme").meta;
    assert_eq!(skipped.kind, NodeKind::Dir);
    assert!(skipped.flags.contains(EntryFlags::NOT_DESCENDED));
    assert!(!out.has("skipme/b.bin"));
    assert!(!out.has("skipme/deep"));
    assert!(out.has("keep/a.bin"));
    assert!(!out
        .get("keep")
        .meta
        .flags
        .contains(EntryFlags::NOT_DESCENDED));
}

/// Pretends folders named "cloudonly" are dataless and files ending in ".cloud" are
/// cloud-only placeholders. Counts its calls.
#[derive(Default)]
struct FakeCloud {
    classify_calls: AtomicUsize,
    descend_calls: AtomicUsize,
    descend_paths: Mutex<HashSet<PathBuf>>,
}

impl CloudClassifier for FakeCloud {
    fn classify(&self, path: &Path, _raw: &RawAttrs) -> Option<CloudInfo> {
        self.classify_calls.fetch_add(1, Ordering::Relaxed);
        path.extension()
            .is_some_and(|e| e == "cloud")
            .then_some(CloudInfo {
                provider: CloudProvider::OneDrive,
                state: CloudState::CloudOnly,
            })
    }
    fn should_descend(&self, path: &Path, raw: &RawAttrs) -> Descend {
        assert!(raw.is_dir, "should_descend is only asked about folders");
        self.descend_calls.fetch_add(1, Ordering::Relaxed);
        self.descend_paths
            .lock()
            .unwrap()
            .insert(path.to_path_buf());
        if path.file_name().is_some_and(|n| n == "cloudonly") {
            Descend::No(SkipReason::CloudOnlyFolder)
        } else if path.file_name().is_some_and(|n| n == "share") {
            Descend::No(SkipReason::Remote)
        } else {
            Descend::Yes
        }
    }
}

#[test]
fn classifier_is_consulted_for_every_entry_and_folder() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("docs/report.cloud"), 0);
    write(&root.join("docs/local.txt"), 5);
    write(&root.join("cloudonly/hidden.bin"), 5);
    write(&root.join("share/x.bin"), 5);

    let fake = FakeCloud::default();
    let out = scan_with(&Walker::default(), opts(root), &fake).unwrap();

    // root, docs, report.cloud, local.txt, cloudonly, share
    assert_eq!(out.records.len(), 6);
    assert_eq!(fake.classify_calls.load(Ordering::Relaxed), 6);
    // root, docs, cloudonly, share
    assert_eq!(fake.descend_calls.load(Ordering::Relaxed), 4);
    assert!(fake
        .descend_paths
        .lock()
        .unwrap()
        .contains(&root.join("docs")));

    let c = &out.get("docs/report.cloud").meta;
    assert_eq!(c.cloud.map(|c| c.state), Some(CloudState::CloudOnly));
    assert_eq!(out.get("docs/local.txt").meta.cloud, None);
    let dl = &out.get("cloudonly").meta;
    assert!(dl.flags.contains(EntryFlags::NOT_DESCENDED));
    assert!(!out.has("cloudonly/hidden.bin"));
    let share = &out.get("share").meta;
    assert!(share
        .flags
        .contains(EntryFlags::NOT_DESCENDED | EntryFlags::REMOTE));
}

#[test]
fn cancelled_scan_stops_early() {
    let t = tempfile::tempdir().unwrap();
    for i in 0..50 {
        write(&t.path().join(format!("d{i}/f.bin")), 1);
    }
    let sink = Collect::default();
    let cancel = CancelToken::new();
    cancel.cancel();
    let summary = Walker::default()
        .scan(&opts(t.path()), &NoCloud, &sink, &cancel)
        .unwrap();
    assert!(summary.cancelled);
    assert!(sink.records.lock().unwrap().len() <= 1);
}

#[test]
fn bad_roots_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let missing = t.path().join("nope");
    assert!(matches!(
        scan_with(&Walker::default(), opts(&missing), &NoCloud),
        Err(ScanError::BadRoot)
    ));
    write(&t.path().join("file.bin"), 1);
    assert!(matches!(
        scan_with(
            &Walker::default(),
            opts(&t.path().join("file.bin")),
            &NoCloud
        ),
        Err(ScanError::BadRoot)
    ));
}

#[test]
fn big_folders_are_split_into_batches() {
    let t = tempfile::tempdir().unwrap();
    for i in 0..10_000 {
        fs::write(t.path().join(format!("f{i:05}")), b"").unwrap();
    }
    let out = scan_with(&Walker { threads: Some(2) }, opts(t.path()), &NoCloud).unwrap();
    assert_eq!(out.records.len(), 10_001);
    let batches = out.sink.batches.lock().unwrap();
    assert!(batches.len() >= 3);
    assert!(batches.iter().all(|&n| n <= 4096));
}

#[test]
fn many_threads_give_the_same_tree() {
    let t = tempfile::tempdir().unwrap();
    for i in 0..20 {
        for j in 0..20 {
            write(&t.path().join(format!("d{i}/e{j}/f.bin")), i * 100 + j);
        }
    }
    let one = scan_with(&Walker { threads: Some(1) }, opts(t.path()), &NoCloud).unwrap();
    let many = scan_with(&Walker { threads: Some(8) }, opts(t.path()), &NoCloud).unwrap();
    assert_eq!(one.records.len(), 1 + 20 + 400 + 400);
    let keys = |o: &Out| {
        let mut v: Vec<(PathBuf, u64)> = o
            .by_path
            .iter()
            .map(|(p, r)| (p.clone(), r.meta.logical_size))
            .collect();
        v.sort();
        v
    };
    assert_eq!(keys(&one), keys(&many));
}

#[test]
fn system_scanner_fast_mode_matches_walker() {
    // Without admin (or off Windows) Fast falls back to the walker; with admin on NTFS
    // it reads the MFT. Either way the tree must match the walker's.
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    write(&root.join("a/one.bin"), 3000);
    write(&root.join("a/b/two.bin"), 70_000);
    write(&root.join("ünï/код.txt"), 7);
    let mut fast_opts = opts(root);
    fast_opts.mode = ScanMode::Fast;
    let fast = scan_with(&SystemScanner::default(), fast_opts, &NoCloud).unwrap();
    let walk = scan(root);
    let mut a: Vec<_> = fast
        .by_path
        .iter()
        .map(|(p, r)| (p.clone(), r.meta.kind, r.meta.logical_size))
        .collect();
    let mut b: Vec<_> = walk
        .by_path
        .iter()
        .map(|(p, r)| (p.clone(), r.meta.kind, r.meta.logical_size))
        .collect();
    a.sort_by(|x, y| x.0.cmp(&y.0));
    b.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(a, b, "fast mode ({:?}) differs", fast.summary.mode_used);
}

#[cfg(unix)]
#[test]
fn non_utf8_names_survive() {
    use std::os::unix::ffi::OsStrExt;
    let t = tempfile::tempdir().unwrap();
    let name = std::ffi::OsStr::from_bytes(b"bad-\xff-name");
    if fs::write(t.path().join(name), b"x").is_err() {
        eprintln!("skipped: filesystem rejects non-UTF-8 names");
        return;
    }
    let out = scan(t.path());
    let rec = out
        .records
        .iter()
        .find(|r| r.meta.name == name)
        .expect("non-UTF-8 entry recorded");
    assert_eq!(rec.meta.logical_size, 1);
}

#[cfg(unix)]
#[test]
fn unreadable_folder_is_flagged() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("locked/secret.bin"), 1);
    let locked = t.path().join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        // Running as root: permissions don't apply.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let out = scan(t.path());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let m = &out.get("locked").meta;
    assert!(m
        .flags
        .contains(EntryFlags::ACCESS_DENIED | EntryFlags::NOT_DESCENDED));
    assert_eq!(out.sink.warnings.lock().unwrap().len(), 1);
}

#[cfg(windows)]
#[test]
fn unreadable_folder_is_flagged() {
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("locked/secret.bin"), 1);
    let locked = t.path().join("locked");
    // Deny "list folder" to Everyone (S-1-1-0), then undo it so cleanup works.
    let icacls = |args: &[&str]| {
        std::process::Command::new("icacls")
            .arg(&locked)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !icacls(&["/deny", "*S-1-1-0:(RD)"]) {
        eprintln!("skipped: icacls unavailable");
        return;
    }
    let out = scan(t.path());
    assert!(icacls(&["/remove:d", "*S-1-1-0"]));
    let m = &out.get("locked").meta;
    assert!(m
        .flags
        .contains(EntryFlags::ACCESS_DENIED | EntryFlags::NOT_DESCENDED));
    assert!(!out.has("locked/secret.bin"));
    assert_eq!(out.sink.warnings.lock().unwrap().len(), 1);
}

#[test]
fn very_deep_nesting_is_cut_off() {
    // Matches walker::MAX_DEPTH: folders deeper than 1024 levels aren't entered.
    const MAX_DEPTH: usize = 1024;
    let t = tempfile::tempdir().unwrap();
    let mut deep = t.path().to_path_buf();
    for _ in 0..MAX_DEPTH + 6 {
        deep.push("d");
    }
    if fs::create_dir_all(&deep).is_err() {
        eprintln!("skipped: filesystem can't nest this deep");
        return;
    }
    let out = scan(t.path());
    let mut rel = PathBuf::new();
    for _ in 0..MAX_DEPTH + 1 {
        rel.push("d");
    }
    let cut = &out.by_path[&rel].meta;
    assert!(cut.flags.contains(EntryFlags::NOT_DESCENDED));
    assert_eq!(out.count(NodeKind::Dir), MAX_DEPTH + 2); // root + 1025 levels
    assert!(!out.sink.warnings.lock().unwrap().is_empty());
}
