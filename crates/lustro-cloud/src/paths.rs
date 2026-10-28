//! Platform-neutral path keys for prefix matching.
//!
//! Paths are compared as strings so Windows rules can be unit-tested on any OS: backslashes
//! become `/`, the `\\?\` verbatim prefix is dropped, trailing separators are trimmed and,
//! for case-insensitive filesystems, characters are lowercased.

/// Normalised comparison key for `path`.
pub fn key(path: &str, case_insensitive: bool) -> String {
    let mut out: String = strip_verbatim(path)
        .chars()
        .map(|c| norm_char(c, case_insensitive))
        .collect();
    while out.ends_with('/') {
        out.pop();
    }
    out
}

/// True when `path` equals the root whose key is `root_key`, or lies inside it.
/// Allocation-free so it can run once per scanned entry.
pub fn is_within(path: &str, root_key: &str, case_insensitive: bool) -> bool {
    let stripped = strip_verbatim(path);
    let mut p = stripped.chars().map(|c| norm_char(c, case_insensitive));
    for r in root_key.chars() {
        if p.next() != Some(r) {
            return false;
        }
    }
    // Root consumed: the path must end here or continue with a separator.
    matches!(p.next(), None | Some('/'))
}

/// True when `path` (normalised) equals `root_key` exactly, ignoring trailing separators.
pub fn is_same(path: &str, root_key: &str, case_insensitive: bool) -> bool {
    key(path, case_insensitive) == root_key
}

fn norm_char(c: char, case_insensitive: bool) -> char {
    if c == '\\' {
        '/'
    } else if case_insensitive {
        // Single-char lowercase covers the cases that matter for path roots; multi-char
        // lowercase mappings are left unchanged.
        let mut l = c.to_lowercase();
        match (l.next(), l.next()) {
            (Some(lc), None) => lc,
            _ => c,
        }
    } else {
        c
    }
}

fn strip_verbatim(path: &str) -> std::borrow::Cow<'_, str> {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}").into()
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.into()
    } else {
        path.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_normalise_separators_case_and_prefix() {
        assert_eq!(
            key(r"C:\Users\William\OneDrive\", true),
            "c:/users/william/onedrive"
        );
        assert_eq!(key(r"\\?\C:\Users\William", true), "c:/users/william");
        assert_eq!(key(r"\\?\UNC\nas\share", true), "//nas/share");
        assert_eq!(key("/home/william/", false), "/home/william");
        assert_eq!(key("/", false), "");
    }

    #[test]
    fn within_matches_whole_components_only() {
        let root = key(r"C:\Users\William\OneDrive", true);
        assert!(is_within(r"C:\Users\William\OneDrive", &root, true));
        assert!(is_within(
            r"c:\users\william\onedrive\Docs\a.txt",
            &root,
            true
        ));
        assert!(is_within(r"\\?\C:\Users\William\OneDrive\a", &root, true));
        assert!(!is_within(
            r"C:\Users\William\OneDrive - Work\a",
            &root,
            true
        ));
        assert!(!is_within(r"C:\Users\William", &root, true));

        let home = key("/home/William", false);
        assert!(!is_within("/home/william/x", &home, false));
        assert!(is_within("/anything", &key("/", false), false));
    }

    #[test]
    fn same_ignores_trailing_separator() {
        let g = key(r"G:\", true);
        assert!(is_same(r"G:\", &g, true));
        assert!(is_same("g:", &g, true));
        assert!(!is_same(r"G:\My Drive", &g, true));
    }
}
