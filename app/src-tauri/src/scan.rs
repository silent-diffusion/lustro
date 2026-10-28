//! Scan sessions: one scan at a time on a worker thread, feeding `TreeBuilder`, with live
//! updates about 10 times a second. When a scan finishes it is saved as a snapshot, compared
//! with the previous one, and the cleanup suggestions are computed in the background.
//!
//! Nothing here logs paths.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use lustro_core::dto::{
    node_dtos, CloudRootInfo, Deltas, NodeDto, ScanDone, ScanRequest, ScanUpdate, SuggestionDto,
};
use lustro_core::insights::{self, DuplicateOptions, InsightOptions};
use lustro_core::model::{CloudInfo, NodeKind};
use lustro_core::scan::{
    CancelToken, CloudClassifier, Descend, RawAttrs, ScanMode, ScanSummary, ScanWarning, Scanner,
    SkipReason,
};
use lustro_core::snapshot::VolumeUsage;
use lustro_core::tree::{LiveChanges, Tree, TreeBuilder};

use crate::util::now;

/// Where scan events go (the Tauri app handle, or a collector in tests).
pub trait ScanEvents: Send + Sync + 'static {
    fn update(&self, u: &ScanUpdate);
    fn done(&self, d: &ScanDone);
}

/// Shown when the user chose Fast scan but the standard scan ran instead.
pub const FAST_FELL_BACK: &str =
    "Fast scan wasn't available (permission declined or not possible here), so a standard scan ran instead.";

/// How often live updates are sent.
const TICK: Duration = Duration::from_millis(100);
/// Up to this many changed nodes per update are all sent; above it only folders and files
/// of at least [`BIG_FILE`] bytes (the rest appear when their folder is opened).
const SEND_ALL_BELOW: usize = 5_000;
const BIG_FILE: u64 = 1024 * 1024;

/// Saves a snapshot; returns (new snapshot id, deltas against the previous snapshot).
pub type SaveFn = Box<dyn FnOnce(&Tree, Option<VolumeUsage>) -> Saved + Send>;
/// (new snapshot id, deltas against the previous snapshot).
pub type Saved = (Option<String>, Option<Deltas>);
/// Receives each new suggestion list (to persist it).
pub type SuggestionsFn = Box<dyn Fn(&[SuggestionDto]) + Send>;

/// What the worker needs from the rest of the app once a scan finishes.
pub struct FinishContext {
    /// Save a snapshot and return (new snapshot id, previous snapshot's deltas for this tree).
    pub save: SaveFn,
    /// Paths the user chose to keep: never suggested again.
    pub kept: Vec<String>,
    /// Duplicate search options from Settings (`findDuplicates: false` reads no files).
    pub duplicates: DuplicateOptions,
    /// Called when the suggestions are ready (to persist them).
    pub on_suggestions: SuggestionsFn,
}

/// Facts about the machine a scan needs.
#[derive(Default)]
pub struct ScanEnv {
    /// Volume usage at scan time (for the snapshot's trend line).
    pub usage: Option<VolumeUsage>,
    /// The bundled elevated helper, used only for Fast scans.
    pub helper: Option<lustro_scan::HelperExe>,
}

#[derive(Default)]
pub struct Extra {
    pub snapshot_id: Option<String>,
    pub deltas: Option<Deltas>,
    /// `None` until computed.
    pub suggestions: Option<Vec<SuggestionDto>>,
}

pub struct Session {
    pub id: String,
    /// Root as requested (and as snapshots name it).
    pub root: PathBuf,
    pub builder: TreeBuilder,
    pub cancel: CancelToken,
    running: AtomicBool,
    pub cloud_roots: Vec<CloudRootInfo>,
    pub extra: RwLock<Extra>,
}

impl Session {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn read<R>(&self, f: impl FnOnce(&Tree) -> R) -> R {
        self.builder.read(f)
    }

    pub fn deltas(&self) -> Option<Deltas> {
        self.extra
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .deltas
            .clone()
    }
}

#[derive(Default)]
pub struct ScanManager {
    current: RwLock<Option<Arc<Session>>>,
    seq: AtomicU64,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Wraps the cloud classifier so that "include remote drives" overrides its `Remote` answer.
struct Classify<'a> {
    inner: &'a dyn CloudClassifier,
    include_remote: bool,
}

impl CloudClassifier for Classify<'_> {
    fn classify(&self, path: &Path, raw: &RawAttrs) -> Option<CloudInfo> {
        self.inner.classify(path, raw)
    }
    fn should_descend(&self, path: &Path, raw: &RawAttrs) -> Descend {
        match self.inner.should_descend(path, raw) {
            Descend::No(SkipReason::Remote) if self.include_remote => Descend::Yes,
            d => d,
        }
    }
}

impl ScanManager {
    pub fn current(&self) -> Option<Arc<Session>> {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_running(&self) -> bool {
        self.current().is_some_and(|s| s.is_running())
    }

    /// Drops the current scan (e.g. after "clear history" the deltas are stale).
    pub fn clear_deltas(&self) {
        if let Some(s) = self.current() {
            s.extra.write().unwrap_or_else(|e| e.into_inner()).deltas = None;
        }
    }

    /// Starts a scan. The caller has validated `req` (root and excluded paths).
    pub fn start(
        &self,
        req: &ScanRequest,
        classifier: Box<dyn CloudClassifier>,
        cloud_roots: Vec<CloudRootInfo>,
        env: ScanEnv,
        events: Arc<dyn ScanEvents>,
        finish: FinishContext,
    ) -> Result<String, String> {
        let mut cur = self.current.write().unwrap_or_else(|e| e.into_inner());
        if cur.as_ref().is_some_and(|s| s.is_running()) {
            return Err("A scan is already running".into());
        }
        let n = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("scan-{}-{n}", now());
        let opts = req.to_options();
        let session = Arc::new(Session {
            id: id.clone(),
            root: opts.root.clone(),
            builder: TreeBuilder::with_root_path(&opts.root),
            cancel: CancelToken::new(),
            running: AtomicBool::new(true),
            cloud_roots,
            extra: RwLock::new(Extra::default()),
        });
        // A previous session's background insights stop when it is replaced.
        if let Some(old) = cur.replace(session.clone()) {
            old.cancel.cancel();
        }
        drop(cur);
        let include_remote = req.include_remote;
        let handle = std::thread::Builder::new()
            .name("lustro-scan".into())
            .spawn(move || {
                let classify = Classify {
                    inner: classifier.as_ref(),
                    include_remote,
                };
                run(&session, &opts, &classify, env, events.as_ref(), finish);
            })
            .map_err(|_| "Couldn't start the scan".to_string())?;
        *self.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        Ok(id)
    }

    pub fn cancel(&self, scan_id: &str) -> Result<(), String> {
        match self.current() {
            Some(s) if s.id == scan_id => {
                s.cancel.cancel();
                Ok(())
            }
            _ => Err("No such scan".into()),
        }
    }

    /// Waits for the worker thread (tests).
    pub fn join(&self) {
        let h = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = h {
            let _ = h.join();
        }
    }
}

fn run(
    session: &Session,
    opts: &lustro_core::scan::ScanOptions,
    classifier: &dyn CloudClassifier,
    env: ScanEnv,
    events: &dyn ScanEvents,
    finish: FinishContext,
) {
    let ScanEnv { usage, helper } = env;
    // Fast mode (the user's choice) launches the elevated helper through a UAC prompt; it
    // falls back to the walker if the user declines or the helper is missing or altered.
    let scanner = lustro_scan::SystemScanner {
        elevation_helper: if opts.mode == ScanMode::Fast {
            helper
        } else {
            None
        },
        ..Default::default()
    };
    let run_opts = opts.clone();
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|s| {
        let worker = s.spawn(|| {
            let r = scanner.scan(&run_opts, classifier, &session.builder, &session.cancel);
            finished.store(true, Ordering::SeqCst);
            r
        });
        while !finished.load(Ordering::SeqCst) {
            std::thread::sleep(TICK);
            send_update(session, events);
        }
        worker.join()
    });
    let summary: Result<ScanSummary, String> = match result {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(match e {
            lustro_core::scan::ScanError::BadRoot => "The folder can't be scanned".to_string(),
            lustro_core::scan::ScanError::Io(_) => {
                "The scan stopped because of a read error".into()
            }
        }),
        Err(_) => Err("The scan stopped unexpectedly".into()),
    };

    session.builder.finalize();
    send_update(session, events);

    let mut warnings = session.builder.warnings();
    let progress = session.builder.latest_progress();
    let (mode_used, cancelled, progress) = match &summary {
        Ok(s) => {
            if opts.mode == ScanMode::Fast && s.mode_used != ScanMode::Fast {
                warnings.insert(
                    0,
                    ScanWarning {
                        path: String::new(),
                        message: FAST_FELL_BACK.into(),
                    },
                );
            }
            (s.mode_used, s.cancelled, s.progress)
        }
        Err(msg) => {
            warnings.insert(
                0,
                ScanWarning {
                    path: session.root.to_string_lossy().into_owned(),
                    message: msg.clone(),
                },
            );
            (run_opts.mode, true, progress)
        }
    };

    let mut snapshot_id = None;
    if !cancelled {
        let (id, deltas) = session.read(|t| (finish.save)(t, usage));
        snapshot_id = id.clone();
        let mut x = session.extra.write().unwrap_or_else(|e| e.into_inner());
        x.snapshot_id = id;
        x.deltas = deltas;
    }
    session.running.store(false, Ordering::SeqCst);
    events.done(&ScanDone {
        scan_id: session.id.clone(),
        mode_used,
        progress,
        cancelled,
        warnings,
        snapshot_id,
    });

    if !cancelled {
        // Fast suggestions first (no file reads), then again with duplicates.
        let quick = session.read(|t| suggestions(t, &[], &finish.kept));
        (finish.on_suggestions)(&quick);
        session
            .extra
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .suggestions = Some(quick);
        // Candidates under the tree lock (no file reads), hashing outside it so the UI's
        // queries aren't blocked while files are read.
        let candidates = session.read(|t| insights::duplicate_candidates(t, finish.duplicates));
        let dupes = if candidates.is_empty() {
            Ok(Vec::new())
        } else {
            insights::hash_candidates(
                candidates,
                &lustro_platform::PlatformFileReader,
                finish.duplicates,
                &session.cancel,
            )
        };
        if let Ok(groups) = dupes {
            let full = session.read(|t| suggestions(t, &groups, &finish.kept));
            (finish.on_suggestions)(&full);
            session
                .extra
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .suggestions = Some(full);
        }
    }
}

/// Suggestions for `tree`, without the items the user chose to keep.
pub fn suggestions(
    tree: &Tree,
    dupes: &[insights::DuplicateGroup],
    kept: &[String],
) -> Vec<SuggestionDto> {
    let mut list = insights::suggest(tree, &InsightOptions::new(now()), dupes);
    list.retain(|s| !kept.iter().any(|k| k == &s.path));
    list
}

fn send_update(session: &Session, events: &dyn ScanEvents) {
    let update = session.builder.drain_with(|tree, ch| ScanUpdate {
        scan_id: session.id.clone(),
        progress: ch.progress,
        nodes: live_nodes(tree, ch),
        busiest: ch.busiest,
    });
    events.update(&update);
}

/// The root's DTO name is the scanned path (`C:\`, `/home/william`), as the UI shows it and
/// as snapshots name the root; the scanner's own root name can be empty for a drive.
pub fn name_root(tree: &Tree, nodes: &mut [NodeDto]) {
    let Some(root) = tree.root_id() else { return };
    for n in nodes.iter_mut().filter(|n| n.id == root) {
        n.name = tree.display_path(root);
    }
}

/// Changed nodes for one live update (see [`SEND_ALL_BELOW`]).
pub fn live_nodes(tree: &Tree, ch: &LiveChanges) -> Vec<NodeDto> {
    let mut out = live_ids(tree, ch);
    name_root(tree, &mut out);
    out
}

fn live_ids(tree: &Tree, ch: &LiveChanges) -> Vec<NodeDto> {
    if ch.changed.len() <= SEND_ALL_BELOW {
        return node_dtos(tree, &ch.changed, None);
    }
    let ids: Vec<_> = ch
        .changed
        .iter()
        .copied()
        .filter(|&id| {
            tree.get(id)
                .is_some_and(|n| n.kind() != NodeKind::File || n.totals.allocated >= BIG_FILE)
        })
        .collect();
    node_dtos(tree, &ids, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lustro_core::scan::NoCloud;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Collect {
        updates: Mutex<Vec<ScanUpdate>>,
        done: Mutex<Vec<ScanDone>>,
    }

    impl ScanEvents for Collect {
        fn update(&self, u: &ScanUpdate) {
            self.updates.lock().unwrap().push(u.clone());
        }
        fn done(&self, d: &ScanDone) {
            self.done.lock().unwrap().push(d.clone());
        }
    }

    fn req(root: &Path) -> ScanRequest {
        ScanRequest {
            root: root.to_string_lossy().into_owned(),
            mode: ScanMode::Fast,
            cross_filesystems: false,
            include_remote: false,
            excluded: vec![],
        }
    }

    fn finish(saved: Arc<AtomicBool>) -> FinishContext {
        FinishContext {
            save: Box::new(move |t, _| {
                assert!(t.root().is_some());
                saved.store(true, Ordering::SeqCst);
                (Some("1".into()), None)
            }),
            kept: vec![],
            duplicates: DuplicateOptions::default(),
            on_suggestions: Box::new(|_| {}),
        }
    }

    #[test]
    fn scans_a_temp_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/one.bin"), vec![1u8; 10_000]).unwrap();
        std::fs::write(dir.path().join("two.txt"), b"hello").unwrap();

        let m = ScanManager::default();
        let ev = Arc::new(Collect::default());
        let saved = Arc::new(AtomicBool::new(false));
        let id = m
            .start(
                &req(dir.path()),
                Box::new(NoCloud),
                vec![],
                ScanEnv::default(),
                ev.clone(),
                finish(saved.clone()),
            )
            .unwrap();
        m.join();
        assert!(saved.load(Ordering::SeqCst));
        let done = ev.done.lock().unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].scan_id, id);
        assert!(!done[0].cancelled);
        // Fast without the helper (not elevated) runs the walker and says so.
        if !lustro_platform::is_elevated() {
            assert_eq!(done[0].mode_used, ScanMode::Standard);
            assert_eq!(done[0].warnings[0].message, FAST_FELL_BACK);
            assert!(done[0].warnings[0].path.is_empty());
        }
        assert_eq!(done[0].snapshot_id.as_deref(), Some("1"));
        assert_eq!(done[0].progress.files, 2);
        // Every node reached the UI through the live updates.
        let sent: std::collections::HashSet<u32> = ev
            .updates
            .lock()
            .unwrap()
            .iter()
            .flat_map(|u| u.nodes.iter().map(|n| n.id))
            .collect();
        let s = m.current().unwrap();
        assert!(!s.is_running());
        let all = s.read(|t| t.iter().map(|n| n.id).collect::<Vec<_>>());
        assert_eq!(all.len(), 5);
        assert!(all.iter().all(|id| sent.contains(id)));
        assert!(s.extra.read().unwrap().suggestions.is_some());
        // The root is named by its path (the UI keys snapshots by it).
        let root = s.read(|t| {
            let mut v = node_dtos(t, &[0], None);
            name_root(t, &mut v);
            v.remove(0)
        });
        assert_eq!(root.name, dir.path().to_string_lossy());
    }

    #[test]
    fn bad_root_reports_done_without_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let m = ScanManager::default();
        let ev = Arc::new(Collect::default());
        let saved = Arc::new(AtomicBool::new(false));
        m.start(
            &req(&dir.path().join("missing")),
            Box::new(NoCloud),
            vec![],
            ScanEnv::default(),
            ev.clone(),
            finish(saved.clone()),
        )
        .unwrap();
        m.join();
        let done = ev.done.lock().unwrap();
        assert!(done[0].cancelled);
        assert!(done[0].snapshot_id.is_none());
        assert!(!saved.load(Ordering::SeqCst));
        assert_eq!(done[0].warnings.len(), 1);
    }

    #[test]
    fn one_scan_at_a_time_and_cancel() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..200 {
            let d = dir.path().join(format!("d{i}"));
            std::fs::create_dir_all(&d).unwrap();
            for j in 0..20 {
                std::fs::write(d.join(format!("f{j}")), b"x").unwrap();
            }
        }
        let m = ScanManager::default();
        let ev = Arc::new(Collect::default());
        let saved = Arc::new(AtomicBool::new(false));
        let id = m
            .start(
                &req(dir.path()),
                Box::new(NoCloud),
                vec![],
                ScanEnv::default(),
                ev.clone(),
                finish(saved.clone()),
            )
            .unwrap();
        // Either still running (refused) or already done (accepted): never two at once.
        if m.is_running() {
            assert!(m.cancel("other").is_err());
            m.cancel(&id).unwrap();
        }
        m.join();
        assert!(!m.is_running());
        assert_eq!(ev.done.lock().unwrap().len(), 1);
    }

    #[test]
    fn live_nodes_limits_big_updates() {
        use lustro_core::fixtures::{Fixture, FixtureSpec};
        let f = Fixture::generate(FixtureSpec {
            seed: 1,
            target_nodes: 20_000,
            ..FixtureSpec::default()
        });
        let tree = f.tree();
        let ch = LiveChanges {
            changed: tree.iter().map(|n| n.id).collect(),
            busiest: None,
            progress: Default::default(),
        };
        let nodes = live_nodes(&tree, &ch);
        assert!(nodes.len() < ch.changed.len());
        assert!(nodes
            .iter()
            .all(|n| n.kind != NodeKind::File || n.allocated >= BIG_FILE));
        let dirs = tree.iter().filter(|n| n.is_dir()).count();
        assert!(nodes.len() >= dirs);
    }
}
