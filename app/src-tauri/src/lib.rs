//! Lustro desktop app: the Tauri 2 shell around the `lustro-*` crates.
//!
//! - [`commands`]: one IPC command per `Backend` method, plus the native folder picker.
//! - [`scan`]: scan sessions (worker thread, live updates, snapshot, suggestions).
//! - [`updates`]: signed updates behind our own commands.
//! - [`settings`], [`state`], [`space`], [`util`].
//!
//! Privacy: nothing here logs paths, and the app makes no network request other than the
//! update check (only when the user checks, or auto-update is on).

pub mod commands;
pub mod scan;
pub mod settings;
pub mod space;
pub mod state;
#[cfg(updater_enabled)]
pub mod updates;
#[cfg(not(updater_enabled))]
#[path = "updates_portable.rs"]
pub mod updates;
pub mod util;

/// Built without the updater (`--no-default-features --features portable`): never updates, never
/// touches the network. `updater_enabled` is set by build.rs: feature `updater` on, `portable` off.
pub const PORTABLE: bool = !cfg!(updater_enabled);

use tauri::Manager;

use crate::state::AppState;

pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(updater_enabled)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    let app = builder
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let data = app.path().app_local_data_dir()?;
            app.manage(AppState::new(&config, &data));
            #[cfg(updater_enabled)]
            updates::spawn_auto(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform,
            commands::list_volumes,
            commands::start_scan,
            commands::cancel_scan,
            commands::root,
            commands::children,
            commands::node,
            commands::path_to,
            commands::subtree,
            commands::top,
            commands::search,
            commands::space_breakdown,
            commands::cloud_summary,
            commands::list_snapshots,
            commands::diff,
            commands::suggestions,
            commands::apply_cleanup,
            commands::undo_log,
            commands::undo,
            commands::reveal,
            commands::open_setting,
            commands::get_settings,
            commands::set_settings,
            commands::clear_history,
            commands::pick_folder,
            commands::app_version,
            commands::check_for_update,
            commands::install_update,
            commands::current_release_notes,
            commands::restart_to_update,
            commands::skip_version,
            commands::update_status,
            commands::build_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lustro");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            // Stop a running scan, then install a downloaded update if nothing is busy.
            if let Some(s) = handle.state::<AppState>().scans.current() {
                s.cancel.cancel();
            }
            updates::install_on_exit(handle);
        }
    });
}
