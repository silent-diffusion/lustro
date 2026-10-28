//! Settings as JSON in the app config dir. Missing or unreadable files fall back to the
//! defaults; unknown fields are ignored and missing ones take their default.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lustro_core::dto::Settings;

use crate::util::write_atomic;

pub struct SettingsStore {
    path: Option<PathBuf>,
    value: Mutex<Settings>,
}

/// Longest string accepted in a settings list (paths, kept items).
const MAX_ENTRY: usize = 4096;
/// Most entries accepted in one settings list.
const MAX_ENTRIES: usize = 10_000;

impl SettingsStore {
    /// Loads `dir/settings.json`, or the defaults.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("settings.json");
        let value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| parse(&s))
            .unwrap_or_default();
        Self {
            path: Some(path),
            value: Mutex::new(value),
        }
    }

    /// Not backed by a file (tests).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            value: Mutex::new(Settings::default()),
        }
    }

    pub fn get(&self) -> Settings {
        self.lock().clone()
    }

    /// Changes the settings and saves them.
    pub fn update<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> Result<R, String> {
        let mut s = self.lock();
        let r = f(&mut s);
        if let Some(p) = &self.path {
            let json = serde_json::to_vec_pretty(&*s).map_err(|_| "Couldn't save settings")?;
            write_atomic(p, &json).map_err(|_| "Couldn't save settings")?;
        }
        Ok(r)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Settings> {
        self.value.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn parse(s: &str) -> Option<Settings> {
    serde_json::from_str(s).ok()
}

/// Rejects oversized lists or entries (a misbehaving page can't bloat the file).
pub fn check_sizes(s: &Settings) -> Result<(), String> {
    for list in [&s.excluded, &s.protected_paths, &s.kept_items] {
        if list.len() > MAX_ENTRIES || list.iter().any(|x| x.len() > MAX_ENTRY || x.is_empty()) {
            return Err("Invalid settings".into());
        }
    }
    if s.snapshots_to_keep == 0 || s.snapshots_to_keep > 1000 {
        return Err("Invalid settings".into());
    }
    if s.notes_shown_for_version
        .as_ref()
        .is_some_and(|v| v.len() > 64)
    {
        return Err("Invalid settings".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_missing_or_broken() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        assert!(s.get().auto_update);
        assert_eq!(s.get(), Settings::default());
        std::fs::write(dir.path().join("settings.json"), "{not json").unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get(), Settings::default());
    }

    #[test]
    fn round_trip_and_auto_update_default() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        s.update(|v| {
            v.snapshots_to_keep = 5;
            v.last_update_check = Some(42);
        })
        .unwrap();
        let again = SettingsStore::load(dir.path()).get();
        assert_eq!(again.snapshots_to_keep, 5);
        assert_eq!(again.last_update_check, Some(42));

        // A file from before the update fields existed still turns auto-update on.
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("settings.json")).unwrap())
                .unwrap();
        let o = json.as_object_mut().unwrap();
        for k in [
            "autoUpdate",
            "skippedVersion",
            "lastUpdateCheck",
            "notesShownForVersion",
        ] {
            o.remove(k);
        }
        std::fs::write(dir.path().join("settings.json"), json.to_string()).unwrap();
        let old = SettingsStore::load(dir.path()).get();
        assert!(old.auto_update);
        assert_eq!(old.last_update_check, None);
    }

    #[test]
    fn size_limits() {
        let mut s = Settings::default();
        assert!(check_sizes(&s).is_ok());
        s.excluded.push(String::new());
        assert!(check_sizes(&s).is_err());
        s.excluded = vec!["x".repeat(MAX_ENTRY + 1)];
        assert!(check_sizes(&s).is_err());
    }
}
