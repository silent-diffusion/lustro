//! Batching and progress reporting shared by all scanners.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use lustro_core::model::{NodeKind, ScanRecord};
use lustro_core::scan::{ScanProgress, ScanSink, ScanWarning};

/// Flush once this many records are buffered.
pub(crate) const BATCH_RECORDS: usize = 4096;
/// Flush and report progress at least this often.
pub(crate) const TICK: Duration = Duration::from_millis(100);

/// Buffers records into batches, keeps running totals and forwards to the sink.
///
/// Batches reach the sink in push order, so a parent pushed before its children
/// is also delivered before them.
pub(crate) struct Emitter<'a> {
    sink: &'a dyn ScanSink,
    buf: Mutex<Vec<ScanRecord>>,
    /// Held while taking the buffer and handing it to the sink, so batches can't overtake each other.
    send: Mutex<()>,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    start: Instant,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(sink: &'a dyn ScanSink) -> Self {
        Self {
            sink,
            buf: Mutex::new(Vec::with_capacity(BATCH_RECORDS)),
            send: Mutex::new(()),
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            start: Instant::now(),
        }
    }

    /// Queue records. Totals count every record (a hard-linked file counts once per
    /// link); exact deduplicated sizes are `TreeBuilder`'s job.
    pub(crate) fn push(&self, records: Vec<ScanRecord>) {
        if records.is_empty() {
            return;
        }
        let (mut files, mut dirs, mut bytes) = (0, 0, 0);
        for r in &records {
            if r.meta.kind == NodeKind::Dir {
                dirs += 1;
            } else {
                files += 1;
                bytes += r.meta.allocated_size;
            }
        }
        self.files.fetch_add(files, Ordering::Relaxed);
        self.dirs.fetch_add(dirs, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);

        let full = {
            let mut buf = lock(&self.buf);
            if records.len() >= BATCH_RECORDS {
                // Big blocks (huge folders) go straight out in batch-sized pieces,
                // still ordered under `send`.
                drop(buf);
                let _send = lock(&self.send);
                self.flush_locked();
                let mut rest = records.into_iter();
                loop {
                    let chunk: Vec<_> = rest.by_ref().take(BATCH_RECORDS).collect();
                    if chunk.is_empty() {
                        return;
                    }
                    self.sink.batch(chunk);
                }
            }
            buf.extend(records);
            buf.len() >= BATCH_RECORDS
        };
        if full {
            self.flush();
        }
    }

    pub(crate) fn flush(&self) {
        let _send = lock(&self.send);
        self.flush_locked();
    }

    /// Caller holds `send`.
    fn flush_locked(&self) {
        let records = std::mem::take(&mut *lock(&self.buf));
        if !records.is_empty() {
            self.sink.batch(records);
        }
    }

    pub(crate) fn warn(&self, path: String, message: String) {
        self.sink.warning(ScanWarning { path, message });
    }

    pub(crate) fn progress(&self) -> ScanProgress {
        ScanProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            allocated_bytes: self.bytes.load(Ordering::Relaxed),
            elapsed_ms: self.start.elapsed().as_millis() as u64,
        }
    }

    pub(crate) fn report(&self) {
        self.sink.progress(self.progress());
    }

    /// Runs `work` while a background thread flushes partial batches and reports
    /// progress every [`TICK`]. Flushes and reports once more at the end.
    pub(crate) fn run<R: Send>(&self, work: impl FnOnce() -> R + Send) -> R {
        let done = (Mutex::new(false), Condvar::new());
        let result = std::thread::scope(|s| {
            s.spawn(|| {
                let mut finished = lock(&done.0);
                while !*finished {
                    let (guard, _) = done
                        .1
                        .wait_timeout(finished, TICK)
                        .unwrap_or_else(|e| e.into_inner());
                    finished = guard;
                    if !*finished {
                        self.flush();
                        self.report();
                    }
                }
            });
            let result = work();
            *lock(&done.0) = true;
            done.1.notify_all();
            result
        });
        self.flush();
        self.report();
        result
    }
}

/// Lock that survives a poisoned mutex (a panicking sink shouldn't wedge the scan).
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
