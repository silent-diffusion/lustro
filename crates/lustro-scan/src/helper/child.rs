//! The helper process (runs elevated). It does one thing: read the MFT under the
//! root the app asks for and stream the records back. No other operations.
//!
//! Stops when the scan finishes, when a write to the pipe fails (the app closed it:
//! that is how the app cancels), or when the app's process exits.

use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lustro_core::scan::CancelToken;
use windows_sys::Win32::Storage::FileSystem::{SECURITY_ANONYMOUS, SECURITY_SQOS_PRESENT};

use super::protocol::{self, ToHelper, ToParent, MAX_RECORDS};
use super::{sys, PIPE_PREFIX};
use crate::emit::lock;
use crate::windows_mft::{MftScanner, RawRecord};
use crate::FastError;

const PING_EVERY: Duration = Duration::from_millis(250);
const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// The request only carries a root path.
const MAX_REQUEST: usize = 128 * 1024;

/// Entry point of `lustro-scan-helper.exe`. Returns the process exit code.
/// Prints nothing, ever.
pub fn run_helper() -> i32 {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

fn run(args: Vec<String>) -> Result<(), i32> {
    let (name, parent_pid) = parse_args(&args).ok_or(2)?;
    let parent = sys::open_process_for_wait(parent_pid).map_err(|_| 3)?;
    // SECURITY_ANONYMOUS: the (unelevated) pipe server can't impersonate this
    // elevated process, nor even identify it.
    let pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS)
        .open(&name)
        .map_err(|_| 4)?;
    // Only talk to the app that launched us.
    if sys::pipe_server_pid(&pipe).ok() != Some(parent_pid) {
        return Err(5);
    }
    let out = Out {
        pipe: Mutex::new(&pipe),
        broken: AtomicBool::new(false),
    };
    out.send(&ToParent::Hello {
        version: protocol::VERSION,
        pid: std::process::id(),
    });
    let root = read_request(&pipe).ok_or(6)?;

    let cancel = CancelToken::new();
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        // Liveness both ways: pings let the app spot a hung helper, and a failed
        // ping means the app closed the pipe, so stop.
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if sys::wait_exit(&parent, PING_EVERY) {
                    cancel.cancel();
                    break;
                }
                if !out.send(&ToParent::Ping) {
                    cancel.cancel();
                    break;
                }
            }
        });

        let mut pending: Vec<RawRecord> = Vec::new();
        let mut last = Instant::now();
        let result = MftScanner::default().scan_raw(&root, &cancel, &mut |batch| {
            pending.extend(batch);
            if pending.len() >= MAX_RECORDS || last.elapsed() >= FLUSH_EVERY {
                out.flush(&mut pending, &cancel);
                last = Instant::now();
            }
        });
        out.flush(&mut pending, &cancel);
        let last_msg = match result {
            Ok(()) => ToParent::Done {
                cancelled: cancel.is_cancelled(),
            },
            Err(FastError::Unavailable(why)) => ToParent::Unavailable(why.into()),
            // ScanError text carries no paths.
            Err(FastError::Scan(e)) => ToParent::Failed(e.to_string()),
        };
        out.send(&last_msg);
        done.store(true, Ordering::Relaxed);
    });
    Ok(())
}

/// `--pipe \\.\pipe\lustro-scan-<32 hex> --parent <pid>`, nothing else.
fn parse_args(args: &[String]) -> Option<(String, u32)> {
    let [flag_pipe, name, flag_parent, pid] = args else {
        return None;
    };
    if flag_pipe != "--pipe" || flag_parent != "--parent" {
        return None;
    }
    let hex = name.strip_prefix(PIPE_PREFIX)?;
    if hex.len() != 32
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let pid: u32 = pid.parse().ok()?;
    (pid != 0).then(|| (name.clone(), pid))
}

fn read_request(mut pipe: &File) -> Option<PathBuf> {
    let mut prefix = [0u8; 4];
    pipe.read_exact(&mut prefix).ok()?;
    let len = protocol::frame_len(prefix).ok()?;
    if len > MAX_REQUEST {
        return None;
    }
    let mut body = vec![0u8; len];
    pipe.read_exact(&mut body).ok()?;
    let ToHelper::Request { version, root } = protocol::decode_to_helper(&body).ok()?;
    let root = PathBuf::from(root);
    (version == protocol::VERSION && valid_root(&root)).then_some(root)
}

/// An absolute local path without `.`/`..` parts.
/// Only `X:\...` on a drive letter, with no `.`/`..` parts. UNC, device and
/// volume-GUID paths are refused here, before anything is opened; the drive-type
/// check follows in `MftScanner` (also before any open).
fn valid_root(root: &Path) -> bool {
    crate::windows_mft::local_drive_path(root).is_some()
}

struct Out<'a> {
    pipe: Mutex<&'a File>,
    broken: AtomicBool,
}

impl Out<'_> {
    /// Returns false once the pipe is broken.
    fn send(&self, msg: &ToParent) -> bool {
        self.write(&protocol::encode_to_parent(msg))
    }

    fn write(&self, frame: &[u8]) -> bool {
        if self.broken.load(Ordering::Relaxed) {
            return false;
        }
        let ok = lock(&self.pipe).write_all(frame).is_ok();
        if !ok {
            self.broken.store(true, Ordering::Relaxed);
        }
        ok
    }

    fn flush(&self, pending: &mut Vec<RawRecord>, cancel: &CancelToken) {
        while !pending.is_empty() {
            let rest = pending.split_off(pending.len().min(MAX_RECORDS));
            let batch = std::mem::replace(pending, rest);
            if !self.send(&ToParent::Records(batch)) {
                cancel.cancel();
                pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_are_strict() {
        let ok = |v: &[&str]| parse_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let name = format!("{PIPE_PREFIX}{}", "0123456789abcdef".repeat(2));
        assert_eq!(
            ok(&["--pipe", &name, "--parent", "42"]),
            Some((name.clone(), 42))
        );
        assert!(ok(&["--pipe", &name, "--parent", "0"]).is_none());
        assert!(ok(&["--pipe", &name, "--parent", "x"]).is_none());
        assert!(ok(&["--pipe", &name]).is_none());
        assert!(ok(&["--pipe", &name, "--parent", "42", "--extra"]).is_none());
        assert!(ok(&["--pipe", r"\\.\pipe\other", "--parent", "42"]).is_none());
        let upper = format!("{PIPE_PREFIX}{}", "0123456789ABCDEF".repeat(2));
        assert!(ok(&["--pipe", &upper, "--parent", "42"]).is_none());
        let short = format!("{PIPE_PREFIX}abc");
        assert!(ok(&["--pipe", &short, "--parent", "42"]).is_none());
    }

    #[test]
    fn roots_must_be_plain_absolute_paths() {
        assert!(valid_root(Path::new(r"C:\")));
        assert!(valid_root(Path::new(r"C:\Users\William")));
        assert!(!valid_root(Path::new(r"Users\William")));
        assert!(!valid_root(Path::new(r"C:\Users\..\Windows")));
        assert!(valid_root(Path::new(r"\\?\C:\Users\William")));
        for bad in [
            r"\\attacker\share",
            r"\\attacker\share\folder",
            r"\\?\UNC\attacker\share",
            r"\\.\C:",
            r"\\.\PhysicalDrive0",
            r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\",
            r"C:relative",
            r"\Users\William",
            r"\\?\C:\Users\..\Windows",
            r"\\?\C:\Users\.\William",
        ] {
            assert!(!valid_root(Path::new(bad)), "accepted {bad}");
        }
    }
}
