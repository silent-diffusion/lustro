//! Windows plumbing for the helper: the app-side named pipe (ACL-restricted,
//! overlapped so every wait can be cancelled or time out), UAC launch, the
//! elevation check, and random pipe names.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::Path;
use std::time::{Duration, Instant};

use lustro_core::scan::CancelToken;
use windows_sys::Win32::Foundation::{
    LocalFree, ERROR_CANCELLED, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::LibraryLoader::{
    SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetProcessId, OpenProcess, OpenProcessToken,
    WaitForMultipleObjects, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// How often blocked waits wake to check for cancellation.
const POLL: Duration = Duration::from_millis(100);
const PIPE_BUFFER: u32 = 1 << 16;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn owned(h: HANDLE) -> io::Result<OwnedHandle> {
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `h` is a valid handle that we own and close exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(h as RawHandle) })
}

/// Is this process running with an elevated (admin) token?
pub fn is_elevated() -> bool {
    let Ok(token) = process_token() else {
        return false;
    };
    let mut elev = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: `elev` is a writable TOKEN_ELEVATION of the size passed.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenElevation,
            (&mut elev as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    ok != 0 && elev.TokenIsElevated != 0
}

fn process_token() -> io::Result<OwnedHandle> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo-handle from GetCurrentProcess needs no closing; `token` is writable.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    owned(token)
}

/// The current user's SID as an SDDL string, e.g. `S-1-5-21-...`.
fn user_sid() -> io::Result<String> {
    let token = process_token()?;
    // u64 storage keeps the TOKEN_USER buffer suitably aligned.
    let mut buf = vec![0u64; 64];
    let mut len = 0u32;
    // SAFETY: `buf` is writable for the size passed; the API fills a TOKEN_USER.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buf.as_mut_ptr().cast(),
            (buf.len() * 8) as u32,
            &mut len,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: on success the buffer starts with an initialised TOKEN_USER.
    let sid = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: `sid` points into `buf`, which is alive; `text` receives a LocalAlloc'd string.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `text` is a NUL-terminated wide string we free right after copying.
    let s = unsafe {
        let n = (0..).take_while(|&i| *text.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text.cast());
        s
    };
    Ok(s)
}

/// Restricts DLL loading to System32 for the rest of the process's life, so a DLL
/// planted next to the helper (in a possibly user-writable folder) is never loaded.
/// The helper calls this first thing; static imports are covered by the
/// `/DEPENDENTLOADFLAG:0x800` link flag (see `lustro-scan-helper/build.rs`).
pub fn harden_dll_search() -> bool {
    // SAFETY: plain call with a constant flag.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) != 0 }
}

/// Opens `path` allowing others only to read (no write, rename or delete) and
/// returns that handle with the SHA-256 of the contents read through it.
pub fn pin_and_hash(path: &Path) -> io::Result<(std::fs::File, [u8; 32])> {
    use sha2::{Digest, Sha256};
    use std::os::windows::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok((file, hasher.finalize().into()))
}

/// `bytes` random bytes from the system CSPRNG, as lowercase hex.
pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buf = vec![0u8; bytes];
    // SAFETY: `buf` is writable for its length; null algorithm handle is allowed with
    // BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other("random generator failed"));
    }
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

pub enum LaunchError {
    /// The user said no to the UAC prompt.
    Declined,
    Failed,
}

/// Starts `exe` elevated via the UAC prompt and returns its process handle.
/// Blocks until the user answers the prompt. `args` must not contain spaces.
///
/// `elevate: false` is for tests only. It starts the helper unelevated despite its
/// `requireAdministrator` manifest, via the documented `RunAsInvoker` compatibility
/// layer, so the handshake can be tested without a UAC prompt.
pub fn launch(exe: &Path, args: &[String], elevate: bool) -> Result<OwnedHandle, LaunchError> {
    if !elevate {
        use std::os::windows::io::IntoRawHandle;
        let child = std::process::Command::new(exe)
            .args(args)
            .env("__COMPAT_LAYER", "RunAsInvoker")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| LaunchError::Failed)?;
        return owned(child.into_raw_handle() as HANDLE).map_err(|_| LaunchError::Failed);
    }
    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let params = wide(args.join(" ").as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: all string pointers are NUL-terminated and outlive the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        return Err(if code == ERROR_CANCELLED {
            LaunchError::Declined
        } else {
            LaunchError::Failed
        });
    }
    owned(info.hProcess).map_err(|_| LaunchError::Failed)
}

pub fn process_id(process: &OwnedHandle) -> u32 {
    // SAFETY: valid process handle.
    unsafe { GetProcessId(process.as_raw_handle()) }
}

/// Opens another process for waiting on its exit only.
pub fn open_process_for_wait(pid: u32) -> io::Result<OwnedHandle> {
    // SAFETY: plain call; the result is checked.
    owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) })
}

/// Waits up to `timeout` for `process` to exit.
pub fn wait_exit(process: &OwnedHandle, timeout: Duration) -> bool {
    // SAFETY: valid handle.
    unsafe {
        WaitForSingleObject(process.as_raw_handle(), timeout.as_millis() as u32) == WAIT_OBJECT_0
    }
}

/// Server process id of a pipe opened as a client (helper side).
pub fn pipe_server_pid(pipe: &impl AsRawHandle) -> io::Result<u32> {
    let mut pid = 0u32;
    // SAFETY: valid pipe handle; `pid` is writable.
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pid)
}

/// App side of the pipe. Single instance, local clients only, and only the current
/// user (or an elevated administrator, for over-the-shoulder UAC) may connect.
pub struct PipeServer {
    pipe: OwnedHandle,
    event: OwnedHandle,
}

impl PipeServer {
    pub fn create(name: &str) -> io::Result<Self> {
        let sddl = format!("D:P(A;;GA;;;{})(A;;GA;;;BA)", user_sid()?);
        let sddl = wide(sddl.as_ref());
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let name = wide(name.as_ref());
        // SAFETY: `name` is NUL-terminated; `sa` and the descriptor it points to are
        // alive for the call. The descriptor is freed right after.
        let pipe = unsafe {
            let h = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER,
                PIPE_BUFFER,
                0,
                &sa,
            );
            LocalFree(sd);
            h
        };
        let pipe = owned(pipe)?;
        // SAFETY: plain call; manual-reset, initially unsignalled, unnamed event.
        let event = owned(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
        Ok(Self { pipe, event })
    }

    /// Waits for the helper to connect, giving up if `process` exits first.
    pub fn connect(
        &self,
        process: &OwnedHandle,
        timeout: Duration,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let pipe = self.pipe.as_raw_handle();
        // SAFETY: `ov` stays alive until `overlapped` has waited for completion.
        self.overlapped(
            |ov| unsafe { ConnectNamedPipe(pipe, ov) },
            timeout,
            cancel,
            Some(process),
        )
        .map(drop)
    }

    pub fn client_pid(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        // SAFETY: valid pipe handle; `pid` is writable.
        if unsafe { GetNamedPipeClientProcessId(self.pipe.as_raw_handle(), &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }

    /// Fills `buf`, failing if no data arrives for `idle` or on cancellation
    /// (`ErrorKind::Interrupted`).
    pub fn read_exact(
        &self,
        mut buf: &mut [u8],
        idle: Duration,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let pipe = self.pipe.as_raw_handle();
        while !buf.is_empty() {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let ptr = buf.as_mut_ptr();
            // SAFETY: `buf` and `ov` stay alive until the read has completed or been
            // cancelled and drained inside `overlapped`.
            let n = self.overlapped(
                |ov| unsafe { ReadFile(pipe, ptr, len, std::ptr::null_mut(), ov) },
                idle,
                cancel,
                None,
            )? as usize;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            buf = &mut buf[n..];
        }
        Ok(())
    }

    pub fn write_all(
        &self,
        mut buf: &[u8],
        timeout: Duration,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let pipe = self.pipe.as_raw_handle();
        while !buf.is_empty() {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let ptr = buf.as_ptr();
            // SAFETY: as in `read_exact`.
            let n = self.overlapped(
                |ov| unsafe { WriteFile(pipe, ptr, len, std::ptr::null_mut(), ov) },
                timeout,
                cancel,
                None,
            )? as usize;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            buf = &buf[n..];
        }
        Ok(())
    }

    /// Runs one overlapped operation to completion, polling for cancellation, the
    /// deadline and (optionally) `process` exiting. On any early exit the I/O is
    /// cancelled and drained before returning, so `ov` and buffers are never used
    /// after this returns.
    fn overlapped(
        &self,
        start: impl FnOnce(*mut OVERLAPPED) -> i32,
        timeout: Duration,
        cancel: &CancelToken,
        process: Option<&OwnedHandle>,
    ) -> io::Result<u32> {
        let pipe = self.pipe.as_raw_handle();
        let mut ov = OVERLAPPED {
            hEvent: self.event.as_raw_handle(),
            ..Default::default()
        };
        let mut n = 0u32;
        if start(&mut ov) != 0 {
            // SAFETY: the operation completed; `ov` is valid.
            return if unsafe { GetOverlappedResult(pipe, &ov, &mut n, 0) } != 0 {
                Ok(n)
            } else {
                Err(io::Error::last_os_error())
            };
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error().map(|c| c as u32) {
            Some(ERROR_PIPE_CONNECTED) => return Ok(0),
            Some(ERROR_IO_PENDING) => {}
            _ => return Err(err),
        }
        let deadline = Instant::now() + timeout;
        let mut handles = vec![self.event.as_raw_handle()];
        if let Some(p) = process {
            handles.push(p.as_raw_handle());
        }
        loop {
            // SAFETY: `handles` holds valid handles for the duration of the wait.
            let w = unsafe {
                WaitForMultipleObjects(
                    handles.len() as u32,
                    handles.as_ptr(),
                    0,
                    POLL.as_millis() as u32,
                )
            };
            let stop = if w == WAIT_OBJECT_0 {
                // SAFETY: the event fired, so the operation has completed.
                return if unsafe { GetOverlappedResult(pipe, &ov, &mut n, 0) } != 0 {
                    Ok(n)
                } else {
                    Err(io::Error::last_os_error())
                };
            } else if w == WAIT_OBJECT_0 + 1 {
                Some(io::Error::new(io::ErrorKind::BrokenPipe, "helper exited"))
            } else if w == WAIT_FAILED {
                // Don't spin on a wait that can't succeed.
                Some(io::Error::last_os_error())
            } else if cancel.is_cancelled() {
                Some(io::ErrorKind::Interrupted.into())
            } else if Instant::now() >= deadline {
                Some(io::ErrorKind::TimedOut.into())
            } else {
                None
            };
            if let Some(e) = stop {
                // SAFETY: cancel this operation, then wait for it to finish so the
                // kernel no longer references `ov` or the caller's buffer.
                unsafe {
                    CancelIoEx(pipe, &ov);
                    GetOverlappedResult(pipe, &ov, &mut n, 1);
                }
                return Err(e);
            }
        }
    }
}
