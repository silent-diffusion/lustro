//! Scan a folder and print counts, bytes, time and files/sec. Never prints paths.
//!
//! ```text
//! cargo run --release -p lustro-scan --example scanbench -- <folder> [--fast] [--threads N]
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use lustro_core::model::{FileId, NodeKind, ScanRecord};
use lustro_core::scan::{
    CancelToken, NoCloud, ScanMode, ScanOptions, ScanProgress, ScanSink, ScanWarning, Scanner,
};
use lustro_scan::{SystemScanner, Walker};

#[derive(Default)]
struct Counts {
    files: AtomicU64,
    dirs: AtomicU64,
    links: AtomicU64,
    logical: AtomicU64,
    allocated: AtomicU64,
    warnings: AtomicU64,
    batches: AtomicU64,
    /// Hard-linked files seen so far, to count their bytes once.
    seen: Mutex<std::collections::HashSet<FileId>>,
}

impl ScanSink for Counts {
    fn batch(&self, records: Vec<ScanRecord>) {
        self.batches.fetch_add(1, Ordering::Relaxed);
        for r in records {
            let m = r.meta;
            match m.kind {
                NodeKind::Dir => {
                    self.dirs.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                NodeKind::Link => {
                    self.links.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                _ => {}
            }
            self.files.fetch_add(1, Ordering::Relaxed);
            if m.link_count > 1 {
                if let Some(id) = m.file_id {
                    if !self.seen.lock().unwrap().insert(id) {
                        continue;
                    }
                }
            }
            self.logical.fetch_add(m.logical_size, Ordering::Relaxed);
            self.allocated
                .fetch_add(m.allocated_size, Ordering::Relaxed);
        }
    }
    fn progress(&self, _: ScanProgress) {}
    fn warning(&self, _: ScanWarning) {
        self.warnings.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut fast = false;
    let mut threads = None;
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--fast") => fast = true,
            Some("--threads") => {
                threads = args.next().and_then(|n| n.to_str()?.parse().ok());
            }
            _ => root = Some(PathBuf::from(a)),
        }
    }
    let Some(root) = root else {
        eprintln!("usage: scanbench <folder> [--fast] [--threads N]");
        std::process::exit(2);
    };
    let options = ScanOptions {
        root,
        mode: if fast {
            ScanMode::Fast
        } else {
            ScanMode::Standard
        },
        ..ScanOptions::default()
    };
    let scanner = SystemScanner {
        walker: Walker { threads },
        ..SystemScanner::default()
    };
    let sink = Counts::default();
    let start = Instant::now();
    let summary = match scanner.scan(&options, &NoCloud, &sink, &CancelToken::new()) {
        Ok(s) => s,
        Err(e) => {
            // ScanError messages carry no paths.
            eprintln!("scan failed: {e}");
            std::process::exit(1);
        }
    };
    let secs = start.elapsed().as_secs_f64();
    let files = sink.files.load(Ordering::Relaxed);
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    println!("mode:       {:?}", summary.mode_used);
    println!("files:      {files}");
    println!("dirs:       {}", sink.dirs.load(Ordering::Relaxed));
    println!("links:      {}", sink.links.load(Ordering::Relaxed));
    println!(
        "logical:    {:.2} GiB",
        gib(sink.logical.load(Ordering::Relaxed))
    );
    println!(
        "allocated:  {:.2} GiB",
        gib(sink.allocated.load(Ordering::Relaxed))
    );
    println!("warnings:   {}", sink.warnings.load(Ordering::Relaxed));
    println!("batches:    {}", sink.batches.load(Ordering::Relaxed));
    println!("elapsed:    {secs:.2} s");
    println!("files/sec:  {:.0}", files as f64 / secs.max(1e-9));
}
