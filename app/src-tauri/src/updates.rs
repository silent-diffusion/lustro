//! Updates through `tauri-plugin-updater`, behind our own commands.
//!
//! - The only network request is the GET of the signed manifest (and then the package), and
//!   only when the user checks or auto-update is on. No identifiers are sent.
//! - Every package is signature-checked by the plugin before it is kept or installed.
//! - Auto-update: check on launch and every 24 h, download in the background, install when
//!   the app closes (so the next start runs the new version), or now via "Restart to update".
//! - Never restarts while a scan or a cleanup is running.
//!
//! Dev only (debug builds): `LUSTRO_DEV_UPDATE_MANIFEST` points the check at a local test
//! manifest (http allowed), and `LUSTRO_DEV_UPDATE_PUBKEY` replaces the public key, so the whole
//! flow can be tried with a throwaway key that never enters the repo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use lustro_core::dto::{UpdateErrorKind, UpdateInfo, UpdateStatus};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Error as UpError, Update, UpdaterExt};

use crate::state::AppState;
use crate::util::now;

pub const EVENT: &str = "update-status";
const DAY: i64 = 24 * 3600;
/// Wait after launch before the automatic check, so start-up stays quick.
const LAUNCH_DELAY: Duration = Duration::from_secs(5);
/// How often the background loop looks at the clock.
const LOOP_TICK: Duration = Duration::from_secs(15 * 60);

/// Release notes of this version, shown in Settings and the "What's new" card.
pub const RELEASE_NOTES: &str = include_str!("../RELEASE_NOTES.md");

#[derive(Default)]
pub struct Updates {
    status: Mutex<Option<UpdateStatus>>,
    /// The update found by the last check.
    found: Mutex<Option<Update>>,
    /// Downloaded and verified package, ready to install.
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    /// A check or download is in flight.
    busy: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Which `kind` an updater error is shown as.
pub fn error_kind(e: &UpError, downloading: bool) -> UpdateErrorKind {
    match e {
        UpError::Minisign(_) | UpError::Base64(_) | UpError::SignatureUtf8(_) => {
            UpdateErrorKind::Verify
        }
        _ if e.to_string().to_lowercase().contains("signed") => UpdateErrorKind::Verify,
        _ if downloading => UpdateErrorKind::Download,
        _ => UpdateErrorKind::Offline,
    }
}

/// A user-facing message for an updater error. Never includes URLs or paths.
pub fn error_message(kind: UpdateErrorKind) -> String {
    match kind {
        UpdateErrorKind::Offline => {
            "Couldn't reach the update server. Check your connection and try again."
        }
        UpdateErrorKind::Download => "The download didn't finish. Try again later.",
        UpdateErrorKind::Verify => "The update's signature didn't match, so nothing was installed.",
    }
    .into()
}

fn info(u: &Update) -> UpdateInfo {
    UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        published_at: u.date.map(|d| d.unix_timestamp()),
        notes: u.body.clone().unwrap_or_default(),
        size_bytes: None,
    }
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        lock(&self.status).clone().unwrap_or(UpdateStatus::Idle)
    }

    pub fn is_ready(&self) -> bool {
        lock(&self.ready).is_some()
    }

    fn set<R: Runtime>(&self, app: &AppHandle<R>, s: UpdateStatus) {
        *lock(&self.status) = Some(s.clone());
        let _ = app.emit(EVENT, s);
    }

    /// Forgets a found update the user chose to skip.
    pub fn skip<R: Runtime>(&self, app: &AppHandle<R>, version: &str) {
        let found = lock(&self.found).as_ref().map(|u| u.version.clone());
        if found.as_deref() == Some(version) && !self.is_ready() {
            *lock(&self.found) = None;
            self.set(app, UpdateStatus::Idle);
        }
    }
}

fn updater<R: Runtime>(app: &AppHandle<R>) -> Result<tauri_plugin_updater::Updater, UpError> {
    #[allow(unused_mut)]
    let mut b = app.updater_builder().timeout(Duration::from_secs(60));
    #[cfg(debug_assertions)]
    {
        if let Ok(url) = std::env::var("LUSTRO_DEV_UPDATE_MANIFEST") {
            b = b.endpoints(vec![url.parse()?])?;
        }
        if let Ok(key) = std::env::var("LUSTRO_DEV_UPDATE_PUBKEY") {
            b = b.pubkey(key);
        }
    }
    b.build()
}

/// One check. `manual`: the user clicked "Check for updates" (a skipped version is still
/// offered then).
pub async fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) -> UpdateStatus {
    let state = app.state::<AppState>();
    let up = &state.updates;
    if let Some((u, _)) = lock(&up.ready).as_ref() {
        // Already downloaded: nothing to fetch.
        let s = UpdateStatus::Ready { info: info(u) };
        up.set(app, s.clone());
        return s;
    }
    if up.busy.swap(true, Ordering::SeqCst) {
        return up.status();
    }
    up.set(app, UpdateStatus::Checking);
    let result = match updater(app) {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    up.busy.store(false, Ordering::SeqCst);
    let status = match result {
        Ok(found) => {
            let t = now();
            let _ = state.settings.update(|s| s.last_update_check = Some(t));
            let skipped = state.settings.get().skipped_version;
            match found {
                Some(u) if manual || skipped.as_deref() != Some(u.version.as_str()) => {
                    let s = UpdateStatus::Available { info: info(&u) };
                    *lock(&up.found) = Some(u);
                    s
                }
                _ => UpdateStatus::UpToDate { checked_at: t },
            }
        }
        Err(e) => {
            let kind = error_kind(&e, false);
            UpdateStatus::Error {
                kind,
                message: error_message(kind),
            }
        }
    };
    up.set(app, status.clone());
    status
}

/// Downloads (and verifies) the update found by the last check. Ok(true) when ready.
pub async fn download<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let up = &state.updates;
    if up.is_ready() {
        return Ok(true);
    }
    let Some(update) = lock(&up.found).clone() else {
        return Err("No update to download. Check for updates first.".into());
    };
    if up.busy.swap(true, Ordering::SeqCst) {
        return Err("An update check or download is already running".into());
    }
    let i = info(&update);
    let mut got = 0u64;
    let mut last = std::time::Instant::now() - Duration::from_secs(1);
    up.set(
        app,
        UpdateStatus::Downloading {
            info: i.clone(),
            downloaded: 0,
            total: None,
        },
    );
    let result = update
        .download(
            |chunk, total| {
                got += chunk as u64;
                // At most ~10 progress events a second.
                if last.elapsed() >= Duration::from_millis(100) {
                    last = std::time::Instant::now();
                    up.set(
                        app,
                        UpdateStatus::Downloading {
                            info: i.clone(),
                            downloaded: got,
                            total,
                        },
                    );
                }
            },
            || {},
        )
        .await;
    up.busy.store(false, Ordering::SeqCst);
    match result {
        Ok(bytes) => {
            *lock(&up.ready) = Some((update, bytes));
            up.set(app, UpdateStatus::Ready { info: i });
            Ok(true)
        }
        Err(e) => {
            let kind = error_kind(&e, true);
            let message = error_message(kind);
            up.set(
                app,
                UpdateStatus::Error {
                    kind,
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}

/// Installs the ready update and restarts, unless a scan or cleanup is running.
/// On Windows the installer takes over and this process exits.
pub fn install_now<R: Runtime>(app: &AppHandle<R>, restart: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.is_busy() {
        return Err("Finish or stop the scan or cleanup first, then restart to update.".into());
    }
    let Some((update, bytes)) = lock(&state.updates.ready).take() else {
        return Err("No update is ready to install".into());
    };
    let update = update.restart_after_install(restart);
    match update.install(&bytes) {
        Ok(()) => {
            if restart {
                app.restart();
            }
            Ok(())
        }
        Err(e) => {
            let kind = error_kind(&e, true);
            let message = error_message(kind);
            state.updates.set(
                app,
                UpdateStatus::Error {
                    kind,
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}

/// When the app closes with a downloaded update and nothing running, install it so the next
/// start runs the new version (without reopening the app now).
pub fn install_on_exit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if state.updates.is_ready() && state.settings.get().auto_update && !state.is_busy() {
        let _ = install_now(app, false);
    }
}

/// Auto-update loop: on launch and every 24 h while `autoUpdate` is on.
pub fn spawn_auto<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(LAUNCH_DELAY).await;
        let mut first = true;
        loop {
            let s = app.state::<AppState>().settings.get();
            let due = first || s.last_update_check.is_none_or(|t| now() - t >= DAY);
            if s.auto_update && due {
                first = false;
                if let UpdateStatus::Available { .. } = check(&app, false).await {
                    let _ = download(&app).await;
                }
            }
            tokio_sleep(LOOP_TICK).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    // tauri's async runtime is tokio; a blocking sleep on a blocking thread keeps us from
    // depending on tokio's timer feature directly.
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_kinds_without_urls() {
        let net = UpError::Network("status 500 for https://example.invalid/x".into());
        assert_eq!(error_kind(&net, false), UpdateErrorKind::Offline);
        assert_eq!(error_kind(&net, true), UpdateErrorKind::Download);
        let sig = UpError::SignatureUtf8("x".into());
        assert_eq!(error_kind(&sig, true), UpdateErrorKind::Verify);
        for k in [
            UpdateErrorKind::Offline,
            UpdateErrorKind::Download,
            UpdateErrorKind::Verify,
        ] {
            let m = error_message(k);
            assert!(!m.contains("http") && !m.contains('\\') && !m.contains('/'));
        }
    }

    #[test]
    fn release_notes_present() {
        assert!(RELEASE_NOTES.contains("Lustro"));
    }
}
