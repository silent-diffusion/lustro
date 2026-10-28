//! Small helpers shared by the modules.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Now, Unix seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Writes `bytes` to a temp file next to `path`, then renames it over `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Comparison key for a path: components only (no trailing separator, no `.`), lower-case
/// on Windows and macOS where the file system ignores case. `None` for relative paths or
/// paths with `..`, which are never accepted from the UI.
pub fn path_key(p: &Path) -> Option<String> {
    if !p.is_absolute() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => return None,
            Component::Prefix(x) => parts.push(x.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir => parts.push(String::new()),
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
        }
    }
    let key = parts.join("/");
    Some(if cfg!(any(windows, target_os = "macos")) {
        key.to_lowercase()
    } else {
        key
    })
}

/// True when `a` and `b` name the same absolute path (see [`path_key`]).
pub fn same_path(a: &Path, b: &Path) -> bool {
    matches!((path_key(a), path_key(b)), (Some(x), Some(y)) if x == y)
}

/// True when `child` is `parent` or lies inside it.
pub fn is_within(child: &Path, parent: &Path) -> bool {
    match (path_key(child), path_key(parent)) {
        (Some(c), Some(p)) => {
            c == p || (c.starts_with(&p) && (p.ends_with('/') || c[p.len()..].starts_with('/')))
        }
        _ => false,
    }
}

/// A path string from the UI: absolute, no `..`, reasonably short.
pub fn ui_path(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if s.is_empty() || s.len() > 4096 || s.contains('\0') || path_key(&p).is_none() {
        return Err("Invalid path".into());
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_keys() {
        assert!(same_path(
            Path::new(r"C:\Users\William\"),
            Path::new(r"c:\users\william")
        ));
        assert!(is_within(
            Path::new(r"C:\Users\William\Videos"),
            Path::new(r"C:\Users\William")
        ));
        assert!(is_within(Path::new(r"C:\Users"), Path::new(r"C:\")));
        assert!(!is_within(
            Path::new(r"C:\Users\Williams"),
            Path::new(r"C:\Users\William")
        ));
        assert!(ui_path(r"C:\Users\William\..\Bob").is_err());
        assert!(ui_path(r"Users\William").is_err());
        assert!(ui_path(r"C:\Users\William").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn unix_keys() {
        assert!(same_path(
            Path::new("/home/william/"),
            Path::new("/home/william")
        ));
        assert!(is_within(Path::new("/home/william/x"), Path::new("/")));
        assert!(ui_path("/home/william/../bob").is_err());
        assert!(ui_path("home").is_err());
    }
}
