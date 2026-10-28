//! `lustro-scan-helper`: the elevated half of Lustro's fast scan on Windows.
//!
//! Launched by the app (through UAC) with `--pipe <name> --parent <pid>`. It reads
//! the NTFS MFT for the one root the app asks for and streams the records back over
//! that pipe, then exits. It has no other commands and touches no files beyond the
//! volume read. All logic lives in `lustro_scan::helper`; see `PROTOCOL.md`.

// No console window: the helper never prints anything.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    // First thing: only ever load DLLs from System32 (see build.rs for static imports).
    // Fail closed if that can't be set.
    if !lustro_scan::helper::harden_dll_search() {
        std::process::exit(1);
    }
    std::process::exit(lustro_scan::helper::run_helper());
}

#[cfg(not(windows))]
fn main() {
    // The MFT fast path is Windows-only.
    std::process::exit(1);
}
