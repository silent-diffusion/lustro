// Generates an `allow-<command>` permission for every app command, so the capability file
// grants exactly our own commands and nothing else (see capabilities/main.json).

const COMMANDS: &[&str] = &[
    "platform",
    "list_volumes",
    "start_scan",
    "cancel_scan",
    "root",
    "children",
    "node",
    "path_to",
    "subtree",
    "top",
    "search",
    "space_breakdown",
    "cloud_summary",
    "list_snapshots",
    "diff",
    "suggestions",
    "apply_cleanup",
    "undo_log",
    "undo",
    "reveal",
    "open_setting",
    "get_settings",
    "set_settings",
    "clear_history",
    "pick_folder",
    "app_version",
    "check_for_update",
    "install_update",
    "current_release_notes",
    "restart_to_update",
    "skip_version",
    "update_status",
    "build_info",
];

/// Bakes the SHA-256 of the bundled fast-scan helper into the app (`LUSTRO_HELPER_SHA256`).
/// `scripts/build-installer.ps1` builds `lustro-scan-helper` and copies it to
/// `binaries/lustro-scan-helper-<target>.exe`, which Tauri installs next to the app exe as a
/// sidecar. The app refuses to launch a helper whose hash differs. No file, empty hash: the
/// app then never launches a helper (Fast works only when already elevated).
fn helper_hash() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let target = std::env::var("TARGET").expect("set by cargo");
    let exe = std::path::Path::new(&dir)
        .join("binaries")
        .join(format!("lustro-scan-helper-{target}.exe"));
    println!("cargo:rerun-if-changed={}", exe.display());
    let hex = lustro_scan::sha256_file(&exe)
        .map(|d| lustro_scan::to_hex(&d))
        .unwrap_or_default();
    println!("cargo:rustc-env=LUSTRO_HELPER_SHA256={hex}");
}

/// `updater_enabled`: the updater is compiled in only with feature `updater` and without
/// `portable` (so `--all-features`, as in CI, builds the portable variant).
fn updater_cfg() {
    println!("cargo::rustc-check-cfg=cfg(updater_enabled)");
    let on = |f: &str| std::env::var_os(format!("CARGO_FEATURE_{f}")).is_some();
    if on("UPDATER") && !on("PORTABLE") {
        println!("cargo:rustc-cfg=updater_enabled");
    }
}

fn main() {
    updater_cfg();
    helper_hash();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed");
}
