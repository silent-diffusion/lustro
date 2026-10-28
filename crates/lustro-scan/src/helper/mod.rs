//! Elevated helper for the MFT fast path, so the app itself never runs elevated.
//!
//! - App side: [`HelperScanner`] creates a per-launch named pipe, starts
//!   `lustro-scan-helper.exe` through the UAC prompt, checks the client really is that
//!   process, sends the scan root and turns the record stream back into a scan,
//!   applying exclusions and the cloud classifier locally (`crate::prune`).
//! - Helper side: [`run_helper`] is the whole of the helper binary's `main`.
//!
//! Wire format: [`protocol`] and `crates/lustro-scan-helper/PROTOCOL.md`.

// OS API calls need `unsafe`; each block is small and has a SAFETY note.
#![allow(unsafe_code)]

mod child;
pub mod protocol;
mod sys;

use std::time::Duration;

use lustro_core::scan::{
    CancelToken, CloudClassifier, ScanError, ScanMode, ScanOptions, ScanSink, ScanSummary,
    ScanWarning, Scanner,
};

use crate::emit::Emitter;
use crate::prune::Pruner;
use crate::windows_mft::MftScanner;
use crate::{FastError, HelperExe};
pub use child::run_helper;
use protocol::{ToHelper, ToParent};
pub use sys::{harden_dll_search, is_elevated};

/// Helper file name, expected next to the app's executable by default.
pub const HELPER_EXE: &str = "lustro-scan-helper.exe";
/// After the UAC prompt, how long the helper may take to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// The helper pings every 250 ms; this much silence means it's stuck.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const PIPE_PREFIX: &str = r"\\.\pipe\lustro-scan-";
/// Shown to the user (via `ScanSink::warning`) when the helper's hash is wrong.
pub const INTEGRITY_WARNING: &str = "Fast scan unavailable: helper failed integrity check";

/// Fast scan through the elevated helper. If the process is already elevated it
/// reads the MFT in-process instead. As a [`Scanner`] it falls back to the walker
/// whenever the fast path is unavailable (UAC declined, not NTFS, ...).
#[derive(Clone, Debug)]
pub struct HelperScanner {
    pub helper: HelperExe,
    pub walker: crate::Walker,
    /// Tests only: start the helper without UAC (it then reports "unavailable").
    #[doc(hidden)]
    pub no_uac_for_tests: bool,
}

impl HelperScanner {
    pub fn new(helper: HelperExe) -> Self {
        Self {
            helper,
            walker: crate::Walker::default(),
            no_uac_for_tests: false,
        }
    }

    /// [`HELPER_EXE`] next to the running executable, expected to hash to
    /// `sha256_hex`. `None` if the hash is malformed or there's no exe folder.
    pub fn next_to_current_exe(sha256_hex: &str) -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        HelperExe::new(exe.parent()?.join(HELPER_EXE), sha256_hex).map(Self::new)
    }

    /// Fast scan only; never falls back. `Unavailable` means nothing was emitted.
    pub fn try_scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, FastError> {
        if is_elevated() && !self.no_uac_for_tests {
            return MftScanner::default().try_scan(options, classifier, sink, cancel);
        }
        let root =
            std::path::absolute(&options.root).map_err(|_| FastError::Scan(ScanError::BadRoot))?;
        if !root.is_dir() {
            return Err(FastError::Scan(ScanError::BadRoot));
        }
        // The helper only accepts local drive-letter paths; don't prompt for anything else.
        if crate::windows_mft::local_drive_path(&root).is_none()
            || root.as_os_str().len() > protocol::MAX_WIDE
        {
            return Err(FastError::Unavailable(
                "fast scan needs a folder on a local drive",
            ));
        }
        // Pin the helper (nobody may write, rename or delete it while we hold this)
        // and check its hash. The pin is held until the scan ends, across the launch.
        let _pinned = match sys::pin_and_hash(&self.helper.path) {
            Ok((file, digest)) if digest == self.helper.sha256 => file,
            Ok(_) => {
                sink.warning(ScanWarning {
                    path: String::new(),
                    message: INTEGRITY_WARNING.into(),
                });
                return Err(FastError::Unavailable("helper failed integrity check"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(FastError::Unavailable("helper not found"))
            }
            Err(_) => return Err(FastError::Unavailable("helper could not be checked")),
        };

        let name = sys::random_hex(16)
            .map(|hex| format!("{PIPE_PREFIX}{hex}"))
            .map_err(|_| FastError::Unavailable("no randomness"))?;
        let pipe = sys::PipeServer::create(&name)
            .map_err(|_| FastError::Unavailable("cannot create pipe"))?;
        let args = [
            "--pipe".to_string(),
            name,
            "--parent".to_string(),
            std::process::id().to_string(),
        ];
        let process =
            sys::launch(&self.helper.path, &args, !self.no_uac_for_tests).map_err(|e| match e {
                sys::LaunchError::Declined => FastError::Unavailable("elevation declined"),
                sys::LaunchError::Failed => FastError::Unavailable("helper launch failed"),
            })?;
        let helper_pid = sys::process_id(&process);

        let cancelled = || {
            Ok(ScanSummary {
                mode_used: ScanMode::Fast,
                progress: Default::default(),
                cancelled: true,
            })
        };
        let unavailable = |e: std::io::Error, why| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                None
            } else {
                Some(FastError::Unavailable(why))
            }
        };

        if let Err(e) = pipe.connect(&process, CONNECT_TIMEOUT, cancel) {
            return unavailable(e, "helper did not connect").map_or_else(cancelled, Err);
        }
        // Only the process we launched may be on the other end.
        if helper_pid == 0 || pipe.client_pid().ok() != Some(helper_pid) {
            return Err(FastError::Unavailable("unexpected pipe client"));
        }
        let mut conn = Conn {
            pipe: &pipe,
            cancel,
            buf: Vec::new(),
        };
        match conn.read() {
            Ok(ToParent::Hello { version, pid })
                if version == protocol::VERSION && pid == helper_pid => {}
            Ok(_) => return Err(FastError::Unavailable("helper protocol mismatch")),
            Err(e) => {
                return unavailable(e, "helper did not say hello").map_or_else(cancelled, Err)
            }
        }
        let request = protocol::encode_to_helper(&ToHelper::Request {
            version: protocol::VERSION,
            root: root.clone().into_os_string(),
        });
        if let Err(e) = pipe.write_all(&request, WRITE_TIMEOUT, cancel) {
            return unavailable(e, "helper did not take the request").map_or_else(cancelled, Err);
        }

        // Until the first records arrive, any failure still allows a clean fallback.
        let first = loop {
            match conn.read() {
                Ok(ToParent::Ping) => {}
                Ok(ToParent::Records(batch)) => break batch,
                Ok(ToParent::Unavailable(_)) => {
                    return Err(FastError::Unavailable(
                        "fast scan unavailable for this folder",
                    ))
                }
                Ok(ToParent::Failed(_)) => return Err(FastError::Unavailable("helper failed")),
                Ok(_) => return Err(FastError::Unavailable("helper protocol error")),
                Err(e) => return unavailable(e, "helper stopped").map_or_else(cancelled, Err),
            }
        };
        let mut pruner = Pruner::new(&root, options, classifier);
        let first = pruner
            .filter(first)
            .map_err(|_| FastError::Unavailable("helper sent bad records"))?;

        let emit = Emitter::new(sink);
        let result = emit.run(|| -> Result<(), ScanError> {
            emit.push(first);
            loop {
                match conn.read() {
                    Ok(ToParent::Ping) => {}
                    Ok(ToParent::Records(batch)) => {
                        let records = pruner.filter(batch).map_err(|e| invalid(e.0))?;
                        emit.push(records);
                    }
                    Ok(ToParent::Warning { path, message }) => {
                        emit.warn(path.to_string_lossy().into_owned(), message);
                    }
                    Ok(ToParent::Done { .. }) => return Ok(()),
                    Ok(_) => return Err(invalid("unexpected message")),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return Ok(()),
                    Err(e) => return Err(ScanError::Io(e)),
                }
            }
        });
        // Closing the pipe (dropping it) tells the helper to stop if it hasn't.
        drop(conn);
        drop(pipe);
        sys::wait_exit(&process, Duration::from_secs(2));
        result.map_err(FastError::Scan)?;
        Ok(ScanSummary {
            mode_used: ScanMode::Fast,
            progress: emit.progress(),
            cancelled: cancel.is_cancelled(),
        })
    }
}

impl Scanner for HelperScanner {
    fn scan(
        &self,
        options: &ScanOptions,
        classifier: &dyn CloudClassifier,
        sink: &dyn ScanSink,
        cancel: &CancelToken,
    ) -> Result<ScanSummary, ScanError> {
        match self.try_scan(options, classifier, sink, cancel) {
            Ok(s) => Ok(s),
            Err(FastError::Unavailable(_)) => self.walker.scan(options, classifier, sink, cancel),
            Err(FastError::Scan(e)) => Err(e),
        }
    }
}

fn invalid(why: &'static str) -> ScanError {
    ScanError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, why))
}

/// Frame reader over the app side of the pipe. Holds at most one frame
/// ([`protocol::MAX_FRAME`]) in memory.
struct Conn<'a> {
    pipe: &'a sys::PipeServer,
    cancel: &'a CancelToken,
    buf: Vec<u8>,
}

impl Conn<'_> {
    fn read(&mut self) -> std::io::Result<ToParent> {
        let mut prefix = [0u8; 4];
        self.pipe
            .read_exact(&mut prefix, IDLE_TIMEOUT, self.cancel)?;
        let len = protocol::frame_len(prefix).map_err(|e| bad_data(e.0))?;
        self.buf.resize(len, 0);
        self.pipe
            .read_exact(&mut self.buf, IDLE_TIMEOUT, self.cancel)?;
        protocol::decode_to_parent(&self.buf).map_err(|e| bad_data(e.0))
    }
}

fn bad_data(why: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, why)
}
