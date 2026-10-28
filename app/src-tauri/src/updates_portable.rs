//! Updates in the portable build (`--no-default-features --features portable`): turned off. Same API as
//! `updates.rs`, but nothing here can make a network request, and the updater plugin is never
//! registered. New versions come from the GitHub Releases page.

use lustro_core::dto::UpdateStatus;
use tauri::{AppHandle, Runtime};

pub const EVENT: &str = "update-status";

/// Release notes of this version, shown in Settings and the "What's new" card.
pub const RELEASE_NOTES: &str = include_str!("../RELEASE_NOTES.md");

/// Returned by every command that would update.
pub const DISABLED: &str = "Updates are turned off in the portable version";

#[derive(Default)]
pub struct Updates {
    _off: (),
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        UpdateStatus::Idle
    }

    pub fn is_ready(&self) -> bool {
        false
    }

    pub fn skip<R: Runtime>(&self, _app: &AppHandle<R>, _version: &str) {}
}

/// No request: always idle.
pub async fn check<R: Runtime>(_app: &AppHandle<R>, _manual: bool) -> UpdateStatus {
    UpdateStatus::Idle
}

pub async fn download<R: Runtime>(_app: &AppHandle<R>) -> Result<bool, String> {
    Err(DISABLED.into())
}

pub fn install_now<R: Runtime>(_app: &AppHandle<R>, _restart: bool) -> Result<(), String> {
    Err(DISABLED.into())
}

pub fn install_on_exit<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_never_updates() {
        let u = Updates::default();
        assert_eq!(u.status(), UpdateStatus::Idle);
        assert!(!u.is_ready());
        assert!(!DISABLED.contains("http"));
        assert!(RELEASE_NOTES.contains("Lustro"));
    }
}
