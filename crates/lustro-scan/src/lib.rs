//! Scanners implementing `lustro_core::scan::Scanner`.
//!
//! - [`Walker`]: portable parallel directory walk (every OS, no privileges).
//! - `windows_mft::MftScanner` (Windows): reads the NTFS MFT directly. Needs admin.
//! - `helper::HelperScanner` (Windows): runs the MFT read in the elevated
//!   `lustro-scan-helper` process, so the app itself never needs admin.
//! - [`SystemScanner`]: picks the fast path for `ScanMode::Fast` and falls back to
//!   the walker when it is unavailable. This is what the app should use.
//!
//! Safety rules every scanner here follows:
//! - metadata only: no file is ever opened for reading its data;
//! - symlinks, junctions and other name-surrogate reparse points are recorded as
//!   `NodeKind::Link` and never followed;
//! - `CloudClassifier::should_descend` is asked before entering any directory;
//! - nothing in this crate logs or prints paths.

mod emit;
mod walker;

#[cfg(windows)]
pub mod helper;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
mod prune;
#[cfg(windows)]
pub mod windows_mft;

use std::path::PathBuf;

use lustro_core::scan::{
    CancelToken, CloudClassifier, ScanError, ScanMode, ScanOptions, ScanSink, ScanSummary, Scanner,
};

pub use walker::Walker;

/// Why a fast path didn't run, or how it failed after starting.
#[derive(Debug)]
pub enum FastError {
    /// Nothing was emitted; the caller should fall back to the walker.
    /// The message is a fixed reason (never contains paths).
    Unavailable(&'static str),
    /// Failed after emitting records.
    Scan(ScanError),
}

/// The elevated helper executable and the SHA-256 it must have. The hash is baked
/// into the app at build time; the helper is refused if the file on disk differs
/// (its folder may be user-writable). Defined on every OS so app code compiles
/// everywhere; only used on Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperExe {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}

impl HelperExe {
    /// `sha256_hex` is 64 hex digits (any case), e.g. `env!("LUSTRO_HELPER_SHA256")`.
    /// Returns `None` if it isn't a valid digest, so a missing build-time hash
    /// simply disables the helper.
    pub fn new(path: PathBuf, sha256_hex: &str) -> Option<Self> {
        let hex = sha256_hex.trim();
        if hex.len() != 64 {
            return None;
        }
        let mut sha256 = [0u8; 32];
        for (i, byte) in sha256.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(Self { path, sha256 })
    }
}

/// SHA-256 of a file, for build scripts that bake in [`HelperExe::sha256`] and
/// for tests. Works on every OS.
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize().into())
}

/// Lowercase hex form of a digest, as accepted by [`HelperExe::new`].
pub fn to_hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Default scanner: honours `ScanOptions.mode`, falling back to [`Walker`] when the
/// platform fast path can't run (not elevated, UAC declined, not NTFS, other OS, ...).
/// `ScanSummary.mode_used` says which one actually ran.
#[derive(Clone, Debug, Default)]
pub struct SystemScanner {
    pub walker: Walker,
    /// Windows only: `lustro-scan-helper.exe` and its expected hash. When set and the
    /// app isn't elevated, `Fast` launches the helper through a UAC prompt. When
    /// `None`, `Fast` works only if the app is already elevated. Ignored elsewhere.
    pub elevation_helper: Option<HelperExe>,
}

impl Scanner for SystemScanner {
    fn scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, ScanError> {
        if options.mode == ScanMode::Fast {
            #[cfg(windows)]
            {
                let fast = match &self.elevation_helper {
                    Some(exe) => helper::HelperScanner::new(exe.clone())
                        .try_scan(options, classifier, sink, cancel),
                    None => windows_mft::MftScanner::default()
                        .try_scan(options, classifier, sink, cancel),
                };
                match fast {
                    Ok(summary) => return Ok(summary),
                    // Unavailable before anything was emitted: fall back silently.
                    Err(FastError::Unavailable(_)) => {}
                    Err(FastError::Scan(e)) => return Err(e),
                }
            }
        }
        self.walker.scan(options, classifier, sink, cancel)
    }
}
