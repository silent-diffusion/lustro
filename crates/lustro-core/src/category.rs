//! File categories: extension rules plus path-context rules.
//!
//! Classification never reads file contents. It uses the name, the entry flags and,
//! when known, the names of the ancestor directories (for "apps" and "system" context).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::model::{EntryFlags, NodeKind};

/// The ten categories of `types.ts`. Order is stable and used for array indexing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Video,
    Images,
    Audio,
    Documents,
    Archives,
    Installers,
    Code,
    Apps,
    System,
    Other,
}

/// Number of categories.
pub const CATEGORY_COUNT: usize = 10;

impl Category {
    pub const ALL: [Category; CATEGORY_COUNT] = [
        Category::Video,
        Category::Images,
        Category::Audio,
        Category::Documents,
        Category::Archives,
        Category::Installers,
        Category::Code,
        Category::Apps,
        Category::System,
        Category::Other,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Category {
        Category::ALL.get(i).copied().unwrap_or(Category::Other)
    }

    /// The camelCase name used in the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Video => "video",
            Category::Images => "images",
            Category::Audio => "audio",
            Category::Documents => "documents",
            Category::Archives => "archives",
            Category::Installers => "installers",
            Category::Code => "code",
            Category::Apps => "apps",
            Category::System => "system",
            Category::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.iter().copied().find(|c| c.as_str() == s)
    }
}

/// Where an entry sits, derived from its ancestors' names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathContext {
    #[default]
    None,
    /// Program Files, /Applications, /opt, a `.app` bundle, game libraries.
    Apps,
    /// The OS itself: Windows, /System, /usr, /bin, ...
    System,
}

const VIDEO: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "wmv", "webm", "m4v", "flv", "mpg", "mpeg", "3gp", "mts", "m2ts",
    "vob",
];
const IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif", "raw", "cr2", "cr3",
    "nef", "arw", "dng", "orf", "rw2", "psd", "svg", "ico", "avif", "xcf",
];
const AUDIO: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "opus", "wma", "aiff", "aif", "alac", "mid", "midi",
];
const DOCUMENTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "md",
    "csv", "epub", "pages", "numbers", "key", "one", "pst", "ost", "eml", "msg",
];
const ARCHIVES: &[&str] = &[
    "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "lz", "lzma", "cab", "iso", "img",
];
const INSTALLERS: &[&str] = &[
    "exe",
    "msi",
    "msix",
    "msixbundle",
    "appx",
    "dmg",
    "pkg",
    "deb",
    "rpm",
    "appimage",
    "flatpak",
    "snap",
    "apk",
];
// `ts` is ambiguous (TypeScript vs MPEG transport stream); code is far more common.
const CODE: &[&str] = &[
    "rs", "c", "h", "cpp", "hpp", "cc", "cs", "java", "kt", "go", "py", "js", "mjs", "cjs", "ts",
    "tsx", "jsx", "svelte", "vue", "rb", "php", "swift", "m", "sh", "ps1", "bat", "lua", "sql",
    "json", "toml", "yaml", "yml", "xml", "html", "css", "scss", "o", "obj", "pdb", "rlib",
    "rmeta", "class", "jar", "pyc", "wasm", "lock", "ipynb",
];
const APPS: &[&str] = &[
    "dll",
    "so",
    "dylib",
    "app",
    "framework",
    "pak",
    "asar",
    "vpk",
];
const SYSTEM: &[&str] = &[
    "sys", "drv", "mui", "cat", "etl", "evtx", "dmp", "swp", "swap",
];

/// Exact file names that always belong to the system.
const SYSTEM_FILES: &[&str] = &[
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "dumpstack.log",
    "sleepimage",
    "swapfile",
];

/// Lowercased extension of `name` (no dot), if any. Dotfiles have none.
pub fn extension(name: &OsStr) -> Option<String> {
    let s = name.to_string_lossy();
    let dot = s.rfind('.')?;
    if dot == 0 || dot + 1 == s.len() {
        return None;
    }
    Some(s[dot + 1..].to_ascii_lowercase())
}

fn extension_table() -> &'static HashMap<&'static str, Category> {
    static TABLE: OnceLock<HashMap<&'static str, Category>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let lists: [(&[&str], Category); 9] = [
            (SYSTEM, Category::System),
            (APPS, Category::Apps),
            (INSTALLERS, Category::Installers),
            (ARCHIVES, Category::Archives),
            (DOCUMENTS, Category::Documents),
            (AUDIO, Category::Audio),
            (IMAGES, Category::Images),
            (VIDEO, Category::Video),
            (CODE, Category::Code),
        ];
        // Later lists win on overlap, so code (listed last) takes `ts`.
        let mut m = HashMap::new();
        for (list, cat) in lists {
            for e in list {
                m.insert(*e, cat);
            }
        }
        m
    })
}

/// Category from a lowercase extension alone.
pub fn by_extension(ext: &str) -> Category {
    extension_table()
        .get(ext)
        .copied()
        .unwrap_or(Category::Other)
}

/// Longest extension we look up; longer ones are `Other`.
const MAX_EXT: usize = 16;

/// Category of an entry from its own name and flags (no path context). Used while scanning,
/// when ancestors may not have arrived yet. Directories are `Other` (their category is the
/// dominant one of their contents). Allocation-free for typical names.
pub fn classify_name(name: &OsStr, kind: NodeKind, flags: EntryFlags) -> Category {
    if kind != NodeKind::File {
        return Category::Other;
    }
    let lossy;
    let s: &str = match name.to_str() {
        Some(s) => s,
        None => {
            lossy = name.to_string_lossy();
            &lossy
        }
    };
    if s.len() <= 16 && SYSTEM_FILES.iter().any(|f| f.eq_ignore_ascii_case(s)) {
        return Category::System;
    }
    let by_ext = match s.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < s.len() && s.len() - dot - 1 <= MAX_EXT => {
            let ext = &s.as_bytes()[dot + 1..];
            let mut buf = [0u8; MAX_EXT];
            let lower = &mut buf[..ext.len()];
            lower.copy_from_slice(ext);
            lower.make_ascii_lowercase();
            std::str::from_utf8(lower).map_or(Category::Other, by_extension)
        }
        _ => Category::Other,
    };
    if by_ext == Category::Other && flags.contains(EntryFlags::SYSTEM) {
        return Category::System;
    }
    by_ext
}

/// Applies the path context to a name-based category.
pub fn apply_context(cat: Category, ctx: PathContext) -> Category {
    match ctx {
        PathContext::None => cat,
        PathContext::System => Category::System,
        // Media and documents inside an app folder stay what they are; binaries, code and
        // unknown files are part of the app.
        PathContext::Apps => match cat {
            Category::Code | Category::Other | Category::Installers | Category::Archives => {
                Category::Apps
            }
            c => c,
        },
    }
}

/// Context of a directory, given its parent's context, its own name and its depth below
/// the scan root (root = 0).
pub fn dir_context(parent: PathContext, name: &OsStr, depth: usize) -> PathContext {
    if parent != PathContext::None {
        return parent;
    }
    let lower = name.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".app") || lower.ends_with(".framework") {
        return PathContext::Apps;
    }
    match lower.as_str() {
        "program files" | "program files (x86)" | "applications" | "steamapps" | "windowsapps" => {
            return PathContext::Apps
        }
        _ => {}
    }
    // Names that only mean "the OS" right below a root (C:\Windows, /usr, /System).
    if depth == 1 {
        match lower.as_str() {
            "windows"
            | "system"
            | "usr"
            | "bin"
            | "sbin"
            | "lib"
            | "lib64"
            | "lib32"
            | "etc"
            | "boot"
            | "private"
            | "system volume information"
            | "$recycle.bin"
            | "recovery"
            | "$winreagent" => return PathContext::System,
            "opt" | "programdata" => return PathContext::Apps,
            _ => {}
        }
    }
    PathContext::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn file(name: &str) -> Category {
        classify_name(&OsString::from(name), NodeKind::File, EntryFlags::empty())
    }

    #[test]
    fn extensions() {
        assert_eq!(file("Holiday.MP4"), Category::Video);
        assert_eq!(file("photo.heic"), Category::Images);
        assert_eq!(file("song.flac"), Category::Audio);
        assert_eq!(file("Report.docx"), Category::Documents);
        assert_eq!(file("backup.tar.gz"), Category::Archives);
        assert_eq!(file("setup.msi"), Category::Installers);
        assert_eq!(file("main.rs"), Category::Code);
        assert_eq!(file("lib.dll"), Category::Apps);
        assert_eq!(file("pagefile.sys"), Category::System);
        assert_eq!(file("README"), Category::Other);
        assert_eq!(file(".bashrc"), Category::Other);
        assert_eq!(file("trailing."), Category::Other);
    }

    #[test]
    fn dirs_are_other_and_system_flag_applies() {
        let n = OsString::from("Videos");
        assert_eq!(
            classify_name(&n, NodeKind::Dir, EntryFlags::empty()),
            Category::Other
        );
        let blob = OsString::from("blob");
        assert_eq!(
            classify_name(&blob, NodeKind::File, EntryFlags::SYSTEM),
            Category::System
        );
    }

    #[test]
    fn context_rules() {
        let pf = dir_context(PathContext::None, OsStr::new("Program Files"), 1);
        assert_eq!(pf, PathContext::Apps);
        assert_eq!(apply_context(Category::Code, pf), Category::Apps);
        assert_eq!(apply_context(Category::Video, pf), Category::Video);
        let win = dir_context(PathContext::None, OsStr::new("Windows"), 1);
        assert_eq!(win, PathContext::System);
        assert_eq!(apply_context(Category::Images, win), Category::System);
        // "Windows" deeper down is just a folder name.
        assert_eq!(
            dir_context(PathContext::None, OsStr::new("Windows"), 3),
            PathContext::None
        );
        // Context is inherited.
        assert_eq!(
            dir_context(PathContext::Apps, OsStr::new("x"), 5),
            PathContext::Apps
        );
        assert_eq!(
            dir_context(PathContext::None, OsStr::new("Foo.app"), 4),
            PathContext::Apps
        );
    }

    #[test]
    fn names_round_trip() {
        for c in Category::ALL {
            assert_eq!(Category::parse(c.as_str()), Some(c));
            assert_eq!(Category::from_index(c.index()), c);
            assert_eq!(
                serde_json::to_string(&c).unwrap(),
                format!("\"{}\"", c.as_str())
            );
        }
    }
}
