//! Link-time hardening for the elevated helper (MSVC). The helper may live in a
//! user-writable folder, so nothing next to it may influence what it loads:
//!
//! - `/DEPENDENTLOADFLAG:0x800` (`LOAD_LIBRARY_SEARCH_SYSTEM32`): static DLL imports
//!   resolve from System32 only. `main` also calls `SetDefaultDllDirectories`.
//! - An embedded manifest (`lustro-scan-helper.manifest`, `requireAdministrator`, no
//!   dependencies): with one embedded, Windows ignores `.exe.local` redirection
//!   folders and external `.exe.manifest` files.
//! - Hybrid CRT: the VC runtime is linked statically (no `VCRUNTIME140.dll`, which is
//!   not a system DLL); the Universal CRT stays dynamic through the system's
//!   `api-ms-win-crt-*` API sets, which the loader maps inside System32.
//!
//! All three are checked by `tests/pe_hardening.rs`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=lustro-scan-helper.manifest");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("lustro-scan-helper.manifest");
    for arg in [
        "/DEPENDENTLOADFLAG:0x800".to_string(),
        "/MANIFEST:EMBED".to_string(),
        format!("/MANIFESTINPUT:{}", manifest.display()),
        // The UAC level comes from the manifest file, not the linker's default.
        "/MANIFESTUAC:NO".to_string(),
        // Hybrid CRT (static VC runtime, dynamic UCRT via API sets).
        "/NODEFAULTLIB:vcruntime.lib".to_string(),
        "/NODEFAULTLIB:msvcrt.lib".to_string(),
        "/NODEFAULTLIB:libucrt.lib".to_string(),
        "/DEFAULTLIB:libvcruntime.lib".to_string(),
        "/DEFAULTLIB:libcmt.lib".to_string(),
        "/DEFAULTLIB:ucrt.lib".to_string(),
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
