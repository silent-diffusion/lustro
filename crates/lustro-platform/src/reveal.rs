//! "Reveal in Explorer / Finder / Files": open the containing folder with the item selected.
//!
//! Launches the system file manager by absolute path; no shell is involved and the item is
//! never opened.

use std::path::Path;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum RevealError {
    #[error("path must be absolute")]
    BadPath,
    #[error("item not found")]
    NotFound,
    #[error("couldn't start the file manager")]
    Launch,
}

/// Shows `path` selected in the OS file manager.
pub fn reveal(path: &Path) -> Result<(), RevealError> {
    if !path.is_absolute() {
        return Err(RevealError::BadPath);
    }
    // symlink_metadata: works for broken links too, never follows them.
    std::fs::symlink_metadata(path).map_err(|_| RevealError::NotFound)?;
    let mut cmd = os::command(path)?;
    cmd.spawn().map(|_| ()).map_err(|_| RevealError::Launch)
}

#[cfg(windows)]
mod os {
    use super::*;
    use std::os::windows::process::CommandExt;

    pub(super) fn command(path: &Path) -> Result<Command, RevealError> {
        let p = path.to_string_lossy();
        // Explorer parses its own command line: `/select,"<path>"`. A quote can't appear in a
        // Windows path, but refuse it anyway so it can't break out of the argument.
        if p.contains('"') {
            return Err(RevealError::BadPath);
        }
        let p = p.strip_prefix(r"\\?\").unwrap_or(&p);
        let mut cmd = Command::new(super::super::system_space::windows_exe("explorer.exe"));
        cmd.raw_arg(format!("/select,\"{p}\""));
        Ok(cmd)
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::*;

    pub(super) fn command(path: &Path) -> Result<Command, RevealError> {
        let mut cmd = Command::new("/usr/bin/open");
        // Absolute paths start with "/", so they can't be read as options.
        cmd.arg("-R").arg(path);
        Ok(cmd)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    use super::*;

    /// freedesktop `FileManager1.ShowItems` over D-Bus (Files, Dolphin, Nemo, …).
    pub(super) fn command(path: &Path) -> Result<Command, RevealError> {
        let mut cmd = Command::new("dbus-send");
        cmd.args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
        ])
        .arg(format!("array:string:{}", super::file_uri(path)))
        .arg("string:");
        Ok(cmd)
    }
}

/// `file://` URI with every byte outside the unreserved set percent-encoded.
#[allow(dead_code)]
pub(crate) fn file_uri(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = path.to_string_lossy().replace('\\', "/").into_bytes();
    let mut out = String::from("file://");
    for b in bytes {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_and_missing() {
        assert!(matches!(
            reveal(Path::new("relative")),
            Err(RevealError::BadPath)
        ));
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            reveal(&dir.path().join("missing")),
            Err(RevealError::NotFound)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn uri_encoding() {
        assert_eq!(
            file_uri(Path::new("/home/william/My Files/a,b#.txt")),
            "file:///home/william/My%20Files/a%2Cb%23.txt"
        );
    }

    #[cfg(windows)]
    #[test]
    fn explorer_command_line() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = os::command(&dir.path().join("a b.txt")).unwrap();
        assert!(cmd
            .get_program()
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with("explorer.exe"));
    }
}
