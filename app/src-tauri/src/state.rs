//! Everything the commands share.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use lustro_core::cleanup::UndoLog;
use lustro_core::dto::SuggestionDto;
use lustro_core::snapshot::SnapshotStore;

use crate::scan::ScanManager;
use crate::settings::SettingsStore;
use crate::updates::Updates;
use crate::util::{path_key, write_atomic};

pub struct AppState {
    /// App local data dir: snapshots, undo log, last suggestions. Never inside the repo.
    data_dir: Option<PathBuf>,
    pub settings: SettingsStore,
    pub scans: ScanManager,
    pub updates: Updates,
    store: Mutex<Option<SnapshotStore>>,
    pub undo: Mutex<UndoLog>,
    /// Suggestions of the last finished scan, kept across restarts so unapplied review
    /// decisions (kept by the UI) still resolve.
    pub saved_suggestions: Mutex<Vec<SuggestionDto>>,
    /// A cleanup apply or undo is running.
    pub applying: AtomicBool,
    /// Path keys of folders the user picked with the native dialog in this run.
    picked: Mutex<HashSet<String>>,
    /// Dev only: an extra scan root offered as a volume (see `commands::list_volumes`).
    pub dev_root: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const UNDO_FILE: &str = "undo-log.json";
const SUGGESTIONS_FILE: &str = "suggestions.json";
const SNAPSHOTS_FILE: &str = "snapshots.sqlite";

impl AppState {
    pub fn new(config_dir: &Path, data_dir: &Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(data_dir.join(name)).ok();
        let undo = read(UNDO_FILE)
            .and_then(|s| UndoLog::from_json(&s).ok())
            .unwrap_or_else(UndoLog::new);
        let saved = read(SUGGESTIONS_FILE)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            data_dir: Some(data_dir.to_path_buf()),
            settings: SettingsStore::load(config_dir),
            scans: ScanManager::default(),
            updates: Updates::default(),
            store: Mutex::new(None),
            undo: Mutex::new(undo),
            saved_suggestions: Mutex::new(saved),
            applying: AtomicBool::new(false),
            picked: Mutex::new(HashSet::new()),
            dev_root: dev_root(),
        }
    }

    /// No files at all (tests).
    pub fn in_memory() -> Self {
        Self {
            data_dir: None,
            settings: SettingsStore::in_memory(),
            scans: ScanManager::default(),
            updates: Updates::default(),
            store: Mutex::new(SnapshotStore::open_in_memory().ok()),
            undo: Mutex::new(UndoLog::new()),
            saved_suggestions: Mutex::new(Vec::new()),
            applying: AtomicBool::new(false),
            picked: Mutex::new(HashSet::new()),
            dev_root: None,
        }
    }

    /// A scan or a cleanup is running: no restart for updates now.
    pub fn is_busy(&self) -> bool {
        self.scans.is_running() || self.applying.load(Ordering::SeqCst)
    }

    /// Runs `f` with the snapshot store, opening it on first use.
    pub fn with_store<R>(
        &self,
        f: impl FnOnce(&mut SnapshotStore) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut g = lock(&self.store);
        if g.is_none() {
            let dir = self.data_dir.as_ref().ok_or("No data folder")?;
            std::fs::create_dir_all(dir).map_err(|_| "Couldn't create the data folder")?;
            let s = SnapshotStore::open(dir.join(SNAPSHOTS_FILE))
                .map_err(|_| "Couldn't open the snapshot history")?;
            *g = Some(s);
        }
        f(g.as_mut().expect("opened above"))
    }

    pub fn save_undo(&self, log: &UndoLog) {
        if let (Some(dir), Ok(json)) = (&self.data_dir, log.to_json()) {
            let _ = write_atomic(&dir.join(UNDO_FILE), json.as_bytes());
        }
    }

    pub fn save_suggestions(&self, list: &[SuggestionDto]) {
        *lock(&self.saved_suggestions) = list.to_vec();
        if let (Some(dir), Ok(json)) = (&self.data_dir, serde_json::to_vec(list)) {
            let _ = write_atomic(&dir.join(SUGGESTIONS_FILE), &json);
        }
    }

    pub fn add_picked(&self, p: &Path) {
        if let Some(k) = path_key(p) {
            lock(&self.picked).insert(k);
        }
    }

    pub fn was_picked(&self, p: &Path) -> bool {
        path_key(p).is_some_and(|k| lock(&self.picked).contains(&k))
    }
}

/// `LUSTRO_DEV_SCAN_ROOT` (debug builds only): an absolute folder offered as an extra "volume".
fn dev_root() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let p = PathBuf::from(std::env::var_os("LUSTRO_DEV_SCAN_ROOT")?);
    (p.is_absolute() && p.is_dir()).then_some(p)
}
