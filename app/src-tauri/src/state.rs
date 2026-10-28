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

/// Every file in the data dir that can hold paths or file names, including SQLite's side files
/// and `write_atomic` leftovers. Settings (config dir) are cleared field by field instead.
const HISTORY_FILES: &[&str] = &[
    SNAPSHOTS_FILE,
    "snapshots.sqlite-journal",
    "snapshots.sqlite-wal",
    "snapshots.sqlite-shm",
    UNDO_FILE,
    "undo-log.tmp",
    SUGGESTIONS_FILE,
    "suggestions.tmp",
];

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

    /// Forgets everything that names files: snapshots, cleanup (undo) history, saved
    /// suggestions and "keep" decisions. The files are deleted, not emptied. The user's own
    /// excluded and protected folders stay in the settings. Items already in the trash stay
    /// there but can no longer be undone from the app.
    pub fn clear_history(&self) -> Result<(), String> {
        if self.scans.is_running() || self.applying.swap(true, Ordering::SeqCst) {
            return Err("Wait for the scan or cleanup to finish".into());
        }
        // Holds the "applying" flag so no cleanup or undo starts meanwhile.
        struct Release<'a>(&'a AtomicBool);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _release = Release(&self.applying);
        {
            // Close the database so its file can be deleted; it reopens empty on next use.
            let mut store = lock(&self.store);
            match &self.data_dir {
                Some(_) => *store = None,
                None => {
                    if let Some(s) = store.as_mut() {
                        s.clear_all().map_err(|_| "Couldn't clear the history")?;
                    }
                }
            }
            *lock(&self.undo) = UndoLog::new();
            lock(&self.saved_suggestions).clear();
            if let Some(dir) = &self.data_dir {
                for name in HISTORY_FILES {
                    match std::fs::remove_file(dir.join(name)) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => return Err("Couldn't clear the history".into()),
                    }
                }
            }
        }
        self.settings.update(|s| s.kept_items.clear())?;
        self.scans.clear_deltas();
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use lustro_core::cleanup::{Action, UndoItem};
    use lustro_core::fixtures::Fixture;
    use lustro_core::insights::{suggest, InsightOptions};
    use lustro_core::snapshot::SaveOptions;

    /// Fills every store with fake data, then checks `clear_history` leaves no file that
    /// names a path, while the user's own excluded folders stay.
    #[test]
    fn clear_history_deletes_everything_that_names_files() {
        let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let fake = if cfg!(windows) {
            r"C:\Users\William\Videos\old.mkv"
        } else {
            "/home/william/Videos/old.mkv"
        };
        let tree = Fixture::small().tree();
        let st = AppState::new(config.path(), data.path());
        st.with_store(|s| {
            s.save(&tree, 1, None, SaveOptions::default())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        let list = suggest(&tree, &InsightOptions::new(1), &[]);
        assert!(!list.is_empty());
        st.save_suggestions(&list);
        let record = serde_json::json!({
            "id": "u1-1", "at": 1, "destination": { "kind": "trash" },
            "items": [UndoItem {
                original: fake.into(), location: None, token: None, bytes: 1,
                action: Action::Trash, undoable: true, restored: false,
            }],
        });
        let log = UndoLog::from_json(
            &serde_json::json!({ "records": [record], "seq": 1, "keep": 100 }).to_string(),
        )
        .unwrap();
        st.save_undo(&log);
        *lock(&st.undo) = log;
        st.settings
            .update(|s| {
                s.kept_items.push(fake.into());
                s.excluded.push(fake.into());
            })
            .unwrap();
        for name in ["undo-log.tmp", "suggestions.tmp"] {
            std::fs::write(data.path().join(name), fake).unwrap();
        }
        for name in [SNAPSHOTS_FILE, UNDO_FILE, SUGGESTIONS_FILE] {
            assert!(data.path().join(name).exists(), "{name}");
        }

        st.clear_history().unwrap();

        let left: Vec<_> = std::fs::read_dir(data.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(left.is_empty(), "{left:?}");
        assert!(lock(&st.undo).entries().is_empty());
        assert!(lock(&st.saved_suggestions).is_empty());
        let s = st.settings.get();
        assert!(s.kept_items.is_empty());
        assert_eq!(s.excluded, vec![fake.to_string()]);
        let settings = std::fs::read_to_string(config.path().join("settings.json")).unwrap();
        assert!(!settings.contains("keptItems\":[\""), "{settings}");
        // The history opens again, empty.
        assert!(st
            .with_store(|s| s
                .list(&tree.root_display().unwrap().to_string_lossy())
                .map_err(|e| e.to_string()))
            .unwrap()
            .is_empty());

        // A fresh start finds nothing either.
        drop(st);
        let again = AppState::new(config.path(), data.path());
        assert!(lock(&again.undo).entries().is_empty());
        assert!(lock(&again.saved_suggestions).is_empty());
    }

    #[test]
    fn clear_history_waits_for_a_cleanup() {
        let st = AppState::in_memory();
        st.applying.store(true, Ordering::SeqCst);
        assert!(st.clear_history().is_err());
        assert!(st.applying.load(Ordering::SeqCst), "flag left to its owner");
        st.applying.store(false, Ordering::SeqCst);
        st.clear_history().unwrap();
        assert!(!st.applying.load(Ordering::SeqCst));
    }
}
