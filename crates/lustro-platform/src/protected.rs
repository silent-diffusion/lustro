//! Paths that cleanup must never touch, per OS, each with a reason shown to the user.
//!
//! A path is refused when it
//! - is inside a `Subtree` entry (e.g. anything under `C:\Windows`),
//! - equals an `Exact` entry (e.g. the user profile folder itself), or
//! - contains any entry (trashing `C:\Users` would take every profile with it).
//!
//! Matching is string-based (see `lustro_cloud::paths`) so every OS's list is testable anywhere.

use std::path::{Path, PathBuf};

use lustro_cloud::paths;
use lustro_cloud::rules::Os;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Only this exact path (its contents may be cleaned).
    Exact,
    /// This path and everything inside it.
    Subtree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedEntry {
    /// Display only; never log it.
    pub path: PathBuf,
    pub reason: String,
    pub scope: Scope,
    key: String,
}

/// Why a path was refused. `reason` is safe to log (it never contains the path).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: String,
}

/// Environment the default lists are built from. Separate from the process environment so
/// tests can use fake values.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub home: Option<PathBuf>,
    /// Windows only: `%SystemDrive%`, `%SystemRoot%`, `%ProgramFiles%`, `%ProgramFiles(x86)%`,
    /// `%ProgramW6432%`, `%ProgramData%`, `%PUBLIC%`.
    pub system_drive: Option<PathBuf>,
    pub system_root: Option<PathBuf>,
    pub program_files: Vec<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub public: Option<PathBuf>,
    /// Sync roots: trashing one would delete the whole cloud folder.
    pub sync_roots: Vec<PathBuf>,
}

impl Env {
    /// Reads the current process environment and discovered sync roots.
    pub fn current() -> Self {
        let var = |n: &str| {
            std::env::var_os(n)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = if cfg!(windows) {
            var("USERPROFILE")
        } else {
            var("HOME")
        };
        Env {
            home,
            system_drive: var("SystemDrive"),
            system_root: var("SystemRoot"),
            program_files: ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                .iter()
                .filter_map(|n| var(n))
                .collect(),
            program_data: var("ProgramData"),
            public: var("PUBLIC"),
            sync_roots: lustro_cloud::list_sync_roots()
                .into_iter()
                .map(|r| r.root_path)
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProtectedPaths {
    os: Os,
    entries: Vec<ProtectedEntry>,
}

const WIN_SYSTEM: &str =
    "Windows system files. Removing anything here can stop Windows from starting.";
const PROGRAMS: &str =
    "Installed programs. Uninstall apps from Settings instead of deleting their files.";
const PROFILE_ROOT: &str = "Your user profile folder. It holds all your files and settings.";
const KNOWN_FOLDER: &str = "A standard folder that apps rely on. Clean up inside it instead.";
const APP_DATA: &str = "App settings and data for every app you use. Clean up inside it instead.";
const SYNC_ROOT: &str =
    "A cloud sync folder. Deleting it would delete the files from the cloud too.";
const ALL_USERS: &str = "Holds every user's profile.";
const DRIVE_ROOT: &str = "The root of a drive.";
const SYSTEM_MANAGED: &str =
    "Managed by the system. Change its size in the system settings instead.";

impl ProtectedPaths {
    /// Defaults for this OS from the current environment.
    pub fn for_current_os() -> Self {
        Self::defaults(Os::current(), &Env::current())
    }

    /// Default list for `os` built from `env`.
    pub fn defaults(os: Os, env: &Env) -> Self {
        let mut list = Self {
            os,
            entries: Vec::new(),
        };
        match os {
            Os::Windows => list.windows(env),
            Os::MacOs => list.macos(env),
            Os::Linux => list.linux(env),
        }
        for r in &env.sync_roots {
            list.push(r.clone(), SYNC_ROOT, Scope::Exact);
        }
        list
    }

    pub fn entries(&self) -> &[ProtectedEntry] {
        &self.entries
    }

    /// Adds an entry (e.g. a user-configured path).
    pub fn push(&mut self, path: impl Into<PathBuf>, reason: &str, scope: Scope) {
        let path = path.into();
        let key = paths::key(&path.to_string_lossy(), self.os.case_insensitive());
        if self
            .entries
            .iter()
            .any(|e| e.key == key && e.scope == scope)
        {
            return;
        }
        self.entries.push(ProtectedEntry {
            path,
            reason: reason.to_string(),
            scope,
            key,
        });
    }

    /// Checks the path as written. Use [`ProtectedPaths::check_resolved`] for real deletions,
    /// which also resolves junctions and symlinks in the parent folders.
    pub fn check(&self, path: &Path) -> Option<Refusal> {
        let ci = self.os.case_insensitive();
        let p = paths::key(&path.to_string_lossy(), ci);
        if self.os == Os::Windows && is_windows_drive_or_share_root(&p) {
            return Some(Refusal {
                reason: DRIVE_ROOT.into(),
            });
        }
        for e in &self.entries {
            let hit = match e.scope {
                Scope::Subtree => paths::is_within(&p, &e.key, false),
                Scope::Exact => p == e.key,
            };
            if hit {
                return Some(Refusal {
                    reason: e.reason.clone(),
                });
            }
        }
        // Trashing a parent of a protected path takes the protected path with it.
        if let Some(e) = self
            .entries
            .iter()
            .find(|e| e.key != p && paths::is_within(&e.key, &p, false))
        {
            return Some(Refusal {
                reason: format!("Contains a protected folder. {}", e.reason),
            });
        }
        None
    }

    /// Checks the literal path, then the path with its parent folders resolved (so a
    /// junction pointing into `C:\Windows` can't be used to get around the list).
    /// The final component is not resolved: trashing a link removes only the link.
    pub fn check_resolved(&self, path: &Path) -> Option<Refusal> {
        if let Some(r) = self.check(path) {
            return Some(r);
        }
        let parent = path.parent()?;
        let name = path.file_name()?;
        let resolved = std::fs::canonicalize(parent).ok()?.join(name);
        self.check(&resolved)
    }

    fn windows(&mut self, env: &Env) {
        use Scope::*;
        let drive = env
            .system_drive
            .clone()
            .unwrap_or_else(|| PathBuf::from("C:"));
        let d = |rest: &str| PathBuf::from(format!(r"{}\{rest}", drive.display()));
        let sysroot = env.system_root.clone().unwrap_or_else(|| d("Windows"));
        self.push(sysroot, WIN_SYSTEM, Subtree);
        let mut pf = env.program_files.clone();
        if pf.is_empty() {
            pf = vec![d("Program Files"), d("Program Files (x86)")];
        }
        for p in pf {
            self.push(p, PROGRAMS, Subtree);
        }
        self.push(
            env.program_data.clone().unwrap_or_else(|| d("ProgramData")),
            "Shared app data and settings for all users.",
            Subtree,
        );
        for (rest, reason) in [
            (
                "System Volume Information",
                "Restore points and system indexes. Managed by Windows.",
            ),
            (
                "$Recycle.Bin",
                "The Recycle Bin itself. Empty it from Explorer instead.",
            ),
            ("Recovery", "Windows recovery tools."),
            ("Boot", WIN_SYSTEM),
            ("EFI", WIN_SYSTEM),
            ("$WinREAgent", WIN_SYSTEM),
            (
                "Windows.old",
                "A previous Windows installation. Remove it with Disk Cleanup or Storage settings.",
            ),
            (r"Users\Default", "Template for new user profiles."),
            (
                r"Users\All Users",
                "Shared app data and settings for all users.",
            ),
        ] {
            self.push(d(rest), reason, Subtree);
        }
        for rest in ["pagefile.sys", "hiberfil.sys", "swapfile.sys"] {
            self.push(d(rest), SYSTEM_MANAGED, Exact);
        }
        self.push(d("Users"), ALL_USERS, Exact);
        self.push(d("Documents and Settings"), ALL_USERS, Exact);
        self.push(
            env.public.clone().unwrap_or_else(|| d(r"Users\Public")),
            KNOWN_FOLDER,
            Exact,
        );
        if let Some(home) = &env.home {
            self.push(home.clone(), PROFILE_ROOT, Exact);
            for rest in [
                "AppData",
                r"AppData\Local",
                r"AppData\LocalLow",
                r"AppData\Roaming",
            ] {
                self.push(home.join(rest), APP_DATA, Exact);
            }
            for rest in [
                "Desktop",
                "Documents",
                "Downloads",
                "Pictures",
                "Music",
                "Videos",
                "Favorites",
                "Contacts",
                "Links",
                "Saved Games",
                "Searches",
                "OneDrive",
            ] {
                self.push(home.join(rest), KNOWN_FOLDER, Exact);
            }
            self.push(
                home.join(r"AppData\Local\Microsoft\Windows"),
                WIN_SYSTEM,
                Exact,
            );
        }
    }

    fn macos(&mut self, env: &Env) {
        use Scope::*;
        self.push("/", DRIVE_ROOT, Exact);
        for p in [
            "/System",
            "/bin",
            "/sbin",
            "/usr",
            "/etc",
            "/private/etc",
            "/var/db",
            "/private/var/db",
            "/var/vm",
            "/private/var/vm",
            "/Library/Apple",
            "/cores",
            "/dev",
        ] {
            self.push(p, "macOS system files. Protected by the system.", Subtree);
        }
        for p in [
            "/private",
            "/var",
            "/private/var",
            "/tmp",
            "/private/tmp",
            "/Library",
            "/Applications",
            "/opt",
            "/Volumes",
            "/Network",
        ] {
            self.push(p, KNOWN_FOLDER, Exact);
        }
        self.push("/Users", ALL_USERS, Exact);
        self.push("/Users/Shared", KNOWN_FOLDER, Exact);
        if let Some(home) = &env.home {
            self.push(home.clone(), PROFILE_ROOT, Exact);
            for rest in [
                "Library",
                "Library/Application Support",
                "Library/Containers",
                "Library/Group Containers",
                "Library/Preferences",
                "Library/Mobile Documents",
                "Library/CloudStorage",
            ] {
                self.push(home.join(rest), APP_DATA, Exact);
            }
            for rest in [
                "Desktop",
                "Documents",
                "Downloads",
                "Pictures",
                "Music",
                "Movies",
                "Public",
                "Applications",
            ] {
                self.push(home.join(rest), KNOWN_FOLDER, Exact);
            }
            self.push(
                home.join("Library/Keychains"),
                "Your saved passwords and certificates.",
                Subtree,
            );
        }
    }

    fn linux(&mut self, env: &Env) {
        use Scope::*;
        self.push("/", DRIVE_ROOT, Exact);
        for p in [
            "/bin",
            "/sbin",
            "/lib",
            "/lib32",
            "/lib64",
            "/libx32",
            "/usr",
            "/etc",
            "/boot",
            "/efi",
            "/proc",
            "/sys",
            "/dev",
            "/run",
            "/var/lib",
            "/snap",
            "/lost+found",
        ] {
            self.push(
                p,
                "Operating system files. Removing anything here can break the system.",
                Subtree,
            );
        }
        for p in [
            "/var",
            "/var/log",
            "/var/cache",
            "/opt",
            "/root",
            "/srv",
            "/mnt",
            "/media",
            "/tmp",
            "/var/tmp",
        ] {
            self.push(p, KNOWN_FOLDER, Exact);
        }
        self.push("/home", ALL_USERS, Exact);
        if let Some(home) = &env.home {
            self.push(home.clone(), PROFILE_ROOT, Exact);
            for rest in [
                ".config",
                ".local",
                ".local/share",
                ".cache",
                ".ssh",
                ".gnupg",
            ] {
                self.push(home.join(rest), APP_DATA, Exact);
            }
            self.push(home.join(".ssh"), "Your SSH keys.", Subtree);
            self.push(home.join(".gnupg"), "Your encryption keys.", Subtree);
            self.push(
                home.join(".local/share/keyrings"),
                "Your saved passwords.",
                Subtree,
            );
            for rest in [
                "Desktop",
                "Documents",
                "Downloads",
                "Pictures",
                "Music",
                "Videos",
                "Public",
                "Templates",
            ] {
                self.push(home.join(rest), KNOWN_FOLDER, Exact);
            }
        }
    }
}

/// `c:` or `//server/share` (keys are already normalised).
fn is_windows_drive_or_share_root(key: &str) -> bool {
    let b = key.as_bytes();
    if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return true;
    }
    if let Some(rest) = key.strip_prefix("//") {
        return rest.split('/').filter(|s| !s.is_empty()).count() <= 2;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win_env() -> Env {
        Env {
            home: Some(PathBuf::from(r"C:\Users\William")),
            system_drive: Some(PathBuf::from("C:")),
            system_root: Some(PathBuf::from(r"C:\WINDOWS")),
            program_files: vec![
                PathBuf::from(r"C:\Program Files"),
                PathBuf::from(r"C:\Program Files (x86)"),
            ],
            program_data: Some(PathBuf::from(r"C:\ProgramData")),
            public: None,
            sync_roots: vec![PathBuf::from(r"C:\Users\William\OneDrive - Contoso")],
        }
    }

    fn refused(list: &ProtectedPaths, p: &str) -> bool {
        list.check(Path::new(p)).is_some()
    }

    #[test]
    fn windows_defaults() {
        let l = ProtectedPaths::defaults(Os::Windows, &win_env());
        for p in [
            r"C:\",
            r"D:\",
            r"c:\windows",
            r"C:\Windows\System32\drivers\etc\hosts",
            r"\\?\C:\Windows\WinSxS",
            r"C:\Program Files\App",
            r"C:\Program Files (x86)",
            r"C:\ProgramData\Vendor\cache",
            r"C:\pagefile.sys",
            r"C:\hiberfil.sys",
            r"C:\Users",
            r"C:\Users\William",
            r"C:\Users\William\",
            r"C:\Users\William\AppData",
            r"C:\Users\William\AppData\Local",
            r"C:\Users\William\Documents",
            r"C:\Users\William\OneDrive - Contoso",
            r"C:\System Volume Information",
            r"C:\$Recycle.Bin\S-1-5-21",
            r"\\nas\share",
        ] {
            assert!(refused(&l, p), "should refuse {p}");
        }
        for p in [
            r"C:\Users\William\Downloads\setup.exe",
            r"C:\Users\William\Documents\old",
            r"C:\Users\William\AppData\Local\Temp\x",
            r"C:\Users\William\AppData\Local\npm-cache",
            r"C:\Users\William\OneDrive - Contoso\big.mov",
            r"D:\Games\old",
            r"C:\Windowsish\x",
            r"\\nas\share\folder",
        ] {
            assert!(!refused(&l, p), "should allow {p}");
        }
    }

    #[test]
    fn ancestors_of_protected_paths_are_refused() {
        let l = ProtectedPaths::defaults(Os::Windows, &win_env());
        // C:\Users\William\AppData contains protected AppData\Local.
        let r = l.check(Path::new(r"C:\Users\William\AppData")).unwrap();
        assert!(!r.reason.is_empty());
        let mut l2 = ProtectedPaths::defaults(Os::Linux, &Env::default());
        l2.push("/data/keep/this", "custom", Scope::Exact);
        assert!(refused(&l2, "/data/keep"));
        assert!(refused(&l2, "/data"));
        assert!(!refused(&l2, "/data/other"));
    }

    #[test]
    fn reasons_never_contain_paths() {
        let l = ProtectedPaths::defaults(Os::Windows, &win_env());
        for p in [r"C:\Users\William", r"C:\Windows\x", r"C:\Users"] {
            let r = l.check(Path::new(p)).unwrap();
            assert!(!r.reason.contains("William"), "{}", r.reason);
            assert!(!r.reason.contains(":\\"), "{}", r.reason);
        }
    }

    #[test]
    fn macos_defaults() {
        let env = Env {
            home: Some(PathBuf::from("/Users/william")),
            ..Env::default()
        };
        let l = ProtectedPaths::defaults(Os::MacOs, &env);
        for p in [
            "/",
            "/System/Library",
            "/usr/bin/true",
            "/etc",
            "/Library",
            "/Applications",
            "/Users",
            "/Users/william",
            "/users/William/",
            "/Users/william/Library",
            "/Users/william/Library/Keychains/login.keychain-db",
        ] {
            assert!(refused(&l, p), "should refuse {p}");
        }
        for p in [
            "/Users/william/Library/Caches/com.example",
            "/Users/william/Downloads/old.dmg",
            "/Applications/Old.app",
            "/Library/Caches/x",
        ] {
            assert!(!refused(&l, p), "should allow {p}");
        }
    }

    #[test]
    fn linux_defaults() {
        let env = Env {
            home: Some(PathBuf::from("/home/william")),
            ..Env::default()
        };
        let l = ProtectedPaths::defaults(Os::Linux, &env);
        for p in [
            "/",
            "/usr",
            "/usr/lib/x",
            "/etc/fstab",
            "/proc/1",
            "/home",
            "/home/william",
            "/home/william/.ssh/id_ed25519",
            "/var/lib/dpkg",
            "/boot/vmlinuz",
        ] {
            assert!(refused(&l, p), "should refuse {p}");
        }
        for p in [
            "/home/william/.cache/pip",
            "/home/william/Downloads/x.iso",
            "/var/cache/apt/archives/x.deb",
            "/home/William",
            "/opt/old-app",
        ] {
            assert!(!refused(&l, p), "should allow {p}");
        }
    }

    #[test]
    fn current_os_list_is_not_empty() {
        let l = ProtectedPaths::for_current_os();
        assert!(l.entries().len() > 5);
        assert!(l
            .check(&std::env::temp_dir().join("lustro-test-file"))
            .is_none());
    }
}
