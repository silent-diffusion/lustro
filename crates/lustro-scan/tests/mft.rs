//! Manual check of the NTFS MFT fast path against the walker on a real volume.
//! Run from an elevated terminal:
//!
//! ```text
//! cargo test --release -p lustro-scan --test mft -- --ignored --nocapture
//! ```
//!
//! Scans the system drive (or `LUSTRO_MFT_ROOT`) both ways and prints counts only.
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use lustro_core::model::{NodeKind, ScanRecord};
use lustro_core::scan::{
    CancelToken, NoCloud, ScanMode, ScanOptions, ScanProgress, ScanSink, ScanWarning, Scanner,
};
use lustro_scan::windows_mft::{FastError, MftScanner};
use lustro_scan::Walker;

#[derive(Default)]
struct Counts {
    files: AtomicU64,
    dirs: AtomicU64,
    links: AtomicU64,
    allocated: AtomicU64,
    denied: AtomicU64,
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
    fn warning(&self, _: ScanWarning) {
        self.denied.fetch_add(1, Ordering::Relaxed);
    }
}

fn print(label: &str, c: &Counts, secs: f64) {
    let files = c.files.load(Ordering::Relaxed);
    println!(
        "{label:<7} files={files} dirs={} links={} allocated={:.2} GiB warnings={} time={secs:.2}s files/s={:.0}",
        c.dirs.load(Ordering::Relaxed),
        c.links.load(Ordering::Relaxed),
        c.allocated.load(Ordering::Relaxed) as f64 / (1u64 << 30) as f64,
        c.denied.load(Ordering::Relaxed),
        files as f64 / secs,
    );
}

#[test]
#[ignore = "needs admin and scans a whole volume"]
fn mft_matches_walker_on_real_volume() {
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
    match MftScanner::default().try_scan(&options, &NoCloud, &fast, &CancelToken::new()) {
        Ok(s) => assert_eq!(s.mode_used, ScanMode::Fast),
        Err(FastError::Unavailable(why)) => panic!("fast path unavailable: {why}"),
        Err(FastError::Scan(e)) => panic!("fast path failed: {e}"),
    }
    print("mft", &fast, t.elapsed().as_secs_f64());

    let walk = Counts::default();
    let t = Instant::now();
    Walker::default()
        .scan(&options, &NoCloud, &walk, &CancelToken::new())
        .unwrap();
    print("walker", &walk, t.elapsed().as_secs_f64());

    // The MFT sees inside folders the walker can't open, so it finds at least as much.
    let (mf, wf) = (
        fast.files.load(Ordering::Relaxed),
        walk.files.load(Ordering::Relaxed),
    );
    assert!(mf as f64 >= wf as f64 * 0.98, "MFT found far fewer files");
}
