//! End-to-end tests of the app ↔ helper pipe. Nothing here prints paths.
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use lustro_core::model::{NodeKind, ScanRecord};
use lustro_core::scan::{
    CancelToken, NoCloud, ScanMode, ScanOptions, ScanProgress, ScanSink, ScanWarning, Scanner,
};
use lustro_scan::helper::{is_elevated, HelperScanner, INTEGRITY_WARNING};
use lustro_scan::{sha256_file, to_hex, FastError, HelperExe, Walker};

fn helper_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lustro-scan-helper"))
}

/// The real helper with its real hash (as the app's build step would bake in).
fn helper() -> HelperScanner {
    let hash = to_hex(&sha256_file(&helper_path()).unwrap());
    HelperScanner::new(HelperExe::new(helper_path(), &hash).unwrap())
}

/// A small temp tree, so walker fallbacks stay quick.
fn small_tree() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir(t.path().join("a")).unwrap();
    std::fs::write(t.path().join("a/f.bin"), b"x").unwrap();
    t
}

#[derive(Default)]
struct Counts {
    files: AtomicU64,
    dirs: AtomicU64,
    links: AtomicU64,
    allocated: AtomicU64,
    warnings: Mutex<Vec<String>>,
}

impl ScanSink for Counts {
    fn batch(&self, records: Vec<ScanRecord>) {
        for r in records {
            let c = match r.meta.kind {
                NodeKind::Dir => &self.dirs,
                NodeKind::Link => &self.links,
                _ => {
                    self.allocated
                        .fetch_add(r.meta.allocated_size, Ordering::Relaxed);
                    &self.files
                }
            };
            c.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn progress(&self, _: ScanProgress) {}
    fn warning(&self, w: ScanWarning) {
        self.warnings.lock().unwrap().push(w.message);
    }
}

fn print(label: &str, c: &Counts, secs: f64) {
    println!(
        "{label:<7} files={} dirs={} links={} allocated={:.2} GiB time={secs:.2}s",
        c.files.load(Ordering::Relaxed),
        c.dirs.load(Ordering::Relaxed),
        c.links.load(Ordering::Relaxed),
        c.allocated.load(Ordering::Relaxed) as f64 / (1u64 << 30) as f64,
    );
}

/// Runs the whole handshake (pipe ACL, PID checks, hello, request) with an
/// unelevated helper, which can't open the volume and must say so cleanly.
#[test]
fn unelevated_helper_reports_unavailable() {
    if is_elevated() {
        eprintln!("skipped: test process is elevated");
        return;
    }
    let mut scanner = helper();
    scanner.no_uac_for_tests = true;
    let t = small_tree();
    let options = ScanOptions {
        root: t.path().to_path_buf(),
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };
    let sink = Counts::default();
    match scanner.try_scan(&options, &NoCloud, &sink, &CancelToken::new()) {
        Err(FastError::Unavailable(why)) => {
            assert_eq!(why, "fast scan unavailable for this folder")
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
    assert_eq!(sink.files.load(Ordering::Relaxed), 0);

    // As a Scanner it falls back to the walker.
    let summary = scanner
        .scan(&options, &NoCloud, &sink, &CancelToken::new())
        .unwrap();
    assert_eq!(summary.mode_used, ScanMode::Standard);
}

#[test]
fn missing_helper_is_unavailable() {
    if is_elevated() {
        return;
    }
    let zeros = "0".repeat(64);
    let exe = HelperExe::new(std::env::temp_dir().join("no-such-helper.exe"), &zeros).unwrap();
    let scanner = HelperScanner::new(exe);
    let options = ScanOptions {
        root: std::env::temp_dir(),
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };
    assert!(matches!(
        scanner.try_scan(&options, &NoCloud, &Counts::default(), &CancelToken::new()),
        Err(FastError::Unavailable("helper not found"))
    ));
}

/// A helper whose bytes don't match the baked-in hash is never launched; the scan
/// warns and falls back to the walker.
#[test]
fn tampered_helper_is_refused() {
    if is_elevated() {
        return;
    }
    let wrong = "ab".repeat(32);
    let mut scanner = HelperScanner::new(HelperExe::new(helper_path(), &wrong).unwrap());
    scanner.no_uac_for_tests = true;
    let t = small_tree();
    let options = ScanOptions {
        root: t.path().to_path_buf(),
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };
    let sink = Counts::default();
    assert!(matches!(
        scanner.try_scan(&options, &NoCloud, &sink, &CancelToken::new()),
        Err(FastError::Unavailable("helper failed integrity check"))
    ));
    assert_eq!(
        *sink.warnings.lock().unwrap(),
        vec![INTEGRITY_WARNING.to_string()]
    );

    let sink = Counts::default();
    let summary = scanner
        .scan(&options, &NoCloud, &sink, &CancelToken::new())
        .unwrap();
    assert_eq!(summary.mode_used, ScanMode::Standard);
    assert_eq!(sink.files.load(Ordering::Relaxed), 1);
}

#[test]
fn malformed_hashes_are_rejected() {
    assert!(HelperExe::new(helper_path(), "").is_none());
    assert!(HelperExe::new(helper_path(), &"0".repeat(63)).is_none());
    assert!(HelperExe::new(helper_path(), &"g".repeat(64)).is_none());
    assert!(HelperExe::new(helper_path(), &"AB".repeat(32)).is_some());
}

/// Shows a UAC prompt. Run manually from a normal (unelevated) terminal:
///
/// ```text
/// cargo test --release -p lustro-scan-helper --test e2e -- --ignored --nocapture
/// ```
///
/// Scans the system drive (or `LUSTRO_MFT_ROOT`) through the helper and with the walker,
/// and prints counts only.
#[test]
#[ignore = "shows a UAC prompt and scans a whole volume"]
fn helper_matches_walker_via_uac() {
    let root = std::env::var_os("LUSTRO_MFT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
            PathBuf::from(format!("{drive}\\"))
        });
    let options = ScanOptions {
        root,
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };

    let fast = Counts::default();
    let t = Instant::now();
    match helper().try_scan(&options, &NoCloud, &fast, &CancelToken::new()) {
        Ok(s) => assert_eq!(s.mode_used, ScanMode::Fast),
        Err(FastError::Unavailable(why)) => panic!("fast path unavailable: {why}"),
        Err(FastError::Scan(e)) => panic!("fast path failed: {e}"),
    }
    print("helper", &fast, t.elapsed().as_secs_f64());

    let walk = Counts::default();
    let t = Instant::now();
    Walker::default()
        .scan(&options, &NoCloud, &walk, &CancelToken::new())
        .unwrap();
    print("walker", &walk, t.elapsed().as_secs_f64());

    let (hf, wf) = (
        fast.files.load(Ordering::Relaxed),
        walk.files.load(Ordering::Relaxed),
    );
    assert!(
        hf as f64 >= wf as f64 * 0.98,
        "helper found far fewer files"
    );
}

/// Cancelling mid-scan closes the pipe; the helper must exit and the scan return.
#[test]
#[ignore = "shows a UAC prompt"]
fn cancel_via_uac_returns_promptly() {
    let options = ScanOptions {
        root: PathBuf::from(format!(
            "{}\\",
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
        )),
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        c2.cancel();
    });
    let t = Instant::now();
    let r = helper().try_scan(&options, &NoCloud, &Counts::default(), &cancel);
    canceller.join().unwrap();
    assert!(matches!(r, Ok(s) if s.cancelled) || r.is_err());
    println!("returned {:.2}s after start", t.elapsed().as_secs_f64());
}
