//! Deterministic fake file trees for tests, benchmarks and the UI mock.
//!
//! Every name is invented (`C:\Users\William`, `/Users/william`, `/home/william`); nothing
//! here looks at the real machine. The same spec always gives the same records.
//!
//! A fixture contains, per platform: system and app folders, a home folder with documents,
//! media and Downloads (old installers), cloud folders for every provider in every state
//! (plus a cloud-only folder that was not descended), developer caches, temp folders,
//! hidden and access-denied entries, a hard-linked pair, duplicate files, and bulk filler
//! up to `target_nodes`.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::insights::{FileReader, ReadSeek};
use crate::model::{
    CloudInfo, CloudProvider, CloudState, EntryFlags, EntryMeta, FileId, NodeId, NodeKind,
    ScanRecord,
};
use crate::tree::Tree;

/// "Now" for every fixture (2026-09-21), so ages are stable.
pub const FIXTURE_NOW: i64 = 1_790_000_000;

const DAY: i64 = 86_400;
const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FixturePlatform {
    #[default]
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordOrder {
    /// Parents before children (like the portable walker).
    #[default]
    TopDown,
    /// Exactly reversed: children before parents (like the MFT path).
    ChildrenFirst,
    /// Seeded shuffle.
    Shuffled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixtureSpec {
    pub platform: FixturePlatform,
    pub seed: u64,
    /// Approximate total number of records (at least the fixed layout, ~300).
    pub target_nodes: usize,
    pub order: RecordOrder,
}

impl Default for FixtureSpec {
    fn default() -> Self {
        Self {
            platform: FixturePlatform::Windows,
            seed: 42,
            target_nodes: 2_000,
            order: RecordOrder::TopDown,
        }
    }
}

/// splitmix64: tiny, fast, good enough for fake data.
#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
    /// Log-uniform size between `lo` and `hi` bytes.
    fn size(&mut self, lo: u64, hi: u64) -> u64 {
        let (a, b) = ((lo.max(1) as f64).ln(), (hi.max(2) as f64).ln());
        let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        (a + (b - a) * u).exp() as u64
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A generated fake tree.
#[derive(Clone, Debug)]
pub struct Fixture {
    pub spec: FixtureSpec,
    /// Records in the order chosen by the spec.
    pub records: Vec<ScanRecord>,
    /// Display path of the root (`C:\` or `/`).
    pub root_path: String,
    /// Named entries for assertions: "home", "downloads", "dupA", "dupB", "dupC",
    /// "hardlinkA", "hardlinkB", "cloudOnly", "downloaded", "pinned", "cloudOnlyFolder",
    /// "nodeModules", "cargoTarget", "temp", "oldInstaller", "staleVideo", "denied", ...
    pub ids: HashMap<&'static str, NodeId>,
    /// Content seed per file for [`FixtureReader`]; equal seeds mean equal bytes.
    /// Files not listed use their id as the seed.
    pub content_seeds: HashMap<NodeId, u64>,
}

impl Fixture {
    /// Small Windows fixture with the default spec.
    pub fn small() -> Fixture {
        Fixture::generate(FixtureSpec::default())
    }

    pub fn generate(spec: FixtureSpec) -> Fixture {
        Gen::new(spec).run()
    }

    pub fn id(&self, name: &str) -> NodeId {
        *self
            .ids
            .get(name)
            .unwrap_or_else(|| panic!("fixture has no entry named {name}"))
    }

    /// Builds the tree (records in fixture order, path rules applied).
    pub fn tree(&self) -> Tree {
        let mut t = Tree::with_root_path(&self.root_path);
        for r in &self.records {
            t.insert(r.clone());
        }
        t.apply_path_rules();
        let _ = t.drain_changes();
        t
    }

    /// A reader serving deterministic contents for this fixture's files.
    pub fn reader(&self, tree: &Tree) -> FixtureReader {
        let mut files = HashMap::new();
        for n in tree.iter() {
            if n.kind() == NodeKind::File {
                let seed = self
                    .content_seeds
                    .get(&n.id)
                    .copied()
                    .unwrap_or(n.id as u64);
                files.insert(tree.path_of(n.id), (seed, n.meta.logical_size));
            }
        }
        FixtureReader {
            files,
            opened: Mutex::new(Vec::new()),
            freed: Mutex::new(HashSet::new()),
        }
    }
}

/// [`FileReader`] over fixture contents, generated on the fly (nothing is stored).
/// Records every path opened so tests can check that cloud-only files never are.
#[derive(Debug)]
pub struct FixtureReader {
    files: HashMap<PathBuf, (u64, u64)>,
    opened: Mutex<Vec<PathBuf>>,
    freed: Mutex<HashSet<PathBuf>>,
}

impl FixtureReader {
    /// Simulates the user freeing up `path` after the scan: `is_local` then returns false.
    pub fn free_up(&self, path: &Path) {
        self.freed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_path_buf());
    }

    pub fn opened(&self) -> Vec<PathBuf> {
        self.opened
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl FileReader for FixtureReader {
    fn is_local(&self, path: &Path) -> io::Result<bool> {
        if !self.files.contains_key(path) {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(!self
            .freed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(path))
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        self.opened
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(path.to_path_buf());
        let &(seed, len) = self.files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(Box::new(GenReader { seed, len, pos: 0 }))
    }
}

/// Deterministic bytes: block `i` of 8 bytes is `mix(seed ^ i)`.
struct GenReader {
    seed: u64,
    len: u64,
    pos: u64,
}

impl Read for GenReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = (self.len.saturating_sub(self.pos)).min(buf.len() as u64) as usize;
        for (i, b) in buf[..n].iter_mut().enumerate() {
            let p = self.pos + i as u64;
            *b = mix(self.seed ^ (p / 8).wrapping_mul(0x2545_F491_4F6C_DD1D)).to_le_bytes()
                [(p % 8) as usize];
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for GenReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if p < 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

struct Gen {
    spec: FixtureSpec,
    rng: Rng,
    recs: Vec<ScanRecord>,
    ids: HashMap<&'static str, NodeId>,
    seeds: HashMap<NodeId, u64>,
    next_file_index: u64,
}

fn base_meta(name: &str, kind: NodeKind) -> EntryMeta {
    EntryMeta {
        name: OsString::from(name),
        kind,
        logical_size: 0,
        allocated_size: 0,
        modified: Some(FIXTURE_NOW - 30 * DAY),
        accessed: None,
        file_id: None,
        link_count: 1,
        cloud: None,
        flags: EntryFlags::empty(),
    }
}

impl Gen {
    fn new(spec: FixtureSpec) -> Self {
        Self {
            spec,
            rng: Rng(spec.seed ^ 0x5157_5649_4557),
            recs: Vec::with_capacity(spec.target_nodes + 512),
            ids: HashMap::new(),
            seeds: HashMap::new(),
            next_file_index: 1,
        }
    }

    fn push(&mut self, parent: Option<NodeId>, meta: EntryMeta) -> NodeId {
        let id = self.recs.len() as NodeId;
        self.recs.push(ScanRecord { id, parent, meta });
        id
    }

    fn dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        let mut m = base_meta(name, NodeKind::Dir);
        m.modified = Some(FIXTURE_NOW - self.rng.below(400) as i64 * DAY);
        self.push(Some(parent), m)
    }

    fn path(&mut self, parent: NodeId, parts: &[&str]) -> NodeId {
        parts.iter().fold(parent, |p, name| self.dir(p, name))
    }

    fn file_meta(&mut self, name: &str, size: u64, age_days: i64) -> EntryMeta {
        let mut m = base_meta(name, NodeKind::File);
        m.logical_size = size;
        // Round up to 4 KiB clusters like a real disk.
        m.allocated_size = size.div_ceil(4096) * 4096;
        m.modified = Some(FIXTURE_NOW - age_days * DAY);
        m.accessed = Some(FIXTURE_NOW - (age_days / 2) * DAY);
        m.file_id = Some(FileId {
            volume: 0xC0FFEE,
            index: self.next_file_index,
        });
        self.next_file_index += 1;
        m
    }

    fn file(&mut self, parent: NodeId, name: &str, size: u64, age_days: i64) -> NodeId {
        let m = self.file_meta(name, size, age_days);
        self.push(Some(parent), m)
    }

    fn random_file(
        &mut self,
        parent: NodeId,
        stem: &str,
        exts: &[&str],
        lo: u64,
        hi: u64,
    ) -> NodeId {
        let ext = exts[self.rng.below(exts.len() as u64) as usize];
        let n = self.rng.below(100_000);
        let size = self.rng.size(lo, hi);
        let age = self.rng.below(900) as i64;
        self.file(parent, &format!("{stem}_{n:05}.{ext}"), size, age)
    }

    fn cloud_file(
        &mut self,
        parent: NodeId,
        name: &str,
        size: u64,
        provider: CloudProvider,
        state: CloudState,
    ) -> NodeId {
        let mut m = self.file_meta(name, size, 200);
        m.cloud = Some(CloudInfo { provider, state });
        if state == CloudState::CloudOnly {
            m.allocated_size = 0;
        }
        self.push(Some(parent), m)
    }

    fn name(&mut self, key: &'static str, id: NodeId) {
        self.ids.insert(key, id);
    }

    fn run(mut self) -> Fixture {
        let p = self.spec.platform;
        let root_path = match p {
            FixturePlatform::Windows => "C:\\",
            _ => "/",
        };
        let root = self.push(None, base_meta(root_path, NodeKind::Dir));

        // System and apps.
        let (sys, apps, home_parts, temp_parts): (&[&str], &[&str], &[&str], &[&str]) = match p {
            FixturePlatform::Windows => (
                &["Windows", "System32"],
                &["Program Files"],
                &["Users", "William"],
                &["AppData", "Local", "Temp"],
            ),
            FixturePlatform::Macos => (
                &["System", "Library"],
                &["Applications"],
                &["Users", "william"],
                &["tmp"],
            ),
            FixturePlatform::Linux => (&["usr", "lib"], &["opt"], &["home", "william"], &["tmp"]),
        };
        let sysdir = self.path(root, sys);
        for _ in 0..40 {
            self.random_file(
                sysdir,
                "sys",
                &["dll", "sys", "mui", "so"],
                20 * KIB,
                30 * MIB,
            );
        }
        if p == FixturePlatform::Windows {
            for (name, size) in [("pagefile.sys", 8 * GIB), ("hiberfil.sys", 6 * GIB)] {
                let mut m = self.file_meta(name, size, 1);
                m.flags = EntryFlags::HIDDEN | EntryFlags::SYSTEM;
                self.push(Some(root), m);
            }
            let wt = self.find_or_make(root, &["Windows", "Temp"]);
            for _ in 0..5 {
                self.random_file(wt, "cbs", &["log", "tmp"], 100 * KIB, 20 * MIB);
            }
        }
        let appsdir = self.path(root, apps);
        for a in ["PhotoStudio", "CodeEditor", "GameLauncher"] {
            let d = match p {
                FixturePlatform::Macos => {
                    let app = self.dir(appsdir, &format!("{a}.app"));
                    self.path(app, &["Contents", "MacOS"])
                }
                _ => self.dir(appsdir, a),
            };
            self.random_file(
                d,
                "bin",
                &["exe", "dll", "pak", "so", "dylib"],
                MIB,
                400 * MIB,
            );
            self.random_file(d, "res", &["pak", "dat", "png"], 10 * KIB, 50 * MIB);
            self.random_file(d, "intro", &["mp4"], 10 * MIB, 80 * MIB);
        }

        // Home.
        let home = self.path(root, home_parts);
        self.name("home", home);
        let docs = self.dir(home, "Documents");
        self.name("documents", docs);
        for _ in 0..30 {
            self.random_file(
                docs,
                "doc",
                &["docx", "pdf", "xlsx", "pptx", "txt"],
                10 * KIB,
                20 * MIB,
            );
        }
        let pics = self.dir(home, "Pictures");
        for year in ["2023", "2024", "2025"] {
            let y = self.dir(pics, year);
            for _ in 0..20 {
                self.random_file(y, "IMG", &["jpg", "heic", "png"], 500 * KIB, 12 * MIB);
            }
        }
        let vids = self.dir(home, "Videos");
        for _ in 0..8 {
            self.random_file(vids, "clip", &["mp4", "mkv", "mov"], 50 * MIB, 4 * GIB);
        }
        let stale = self.file(vids, "Old holiday 2019.mkv", 3 * GIB, 900);
        if let Some(r) = self.recs.get_mut(stale as usize) {
            r.meta.accessed = Some(FIXTURE_NOW - 800 * DAY);
        }
        self.name("staleVideo", stale);
        let music = self.dir(home, "Music");
        for _ in 0..25 {
            self.random_file(music, "track", &["mp3", "flac", "m4a"], 3 * MIB, 60 * MIB);
        }

        // Downloads: old and new installers, archives, a duplicate.
        let dl = self.dir(home, "Downloads");
        self.name("downloads", dl);
        let (old_inst, new_inst) = match p {
            FixturePlatform::Windows => ("OldToolSetup.exe", "NewToolSetup.msi"),
            FixturePlatform::Macos => ("OldTool.dmg", "NewTool.pkg"),
            FixturePlatform::Linux => ("old-tool.deb", "new-tool.AppImage"),
        };
        let oi = self.file(dl, old_inst, 180 * MIB, 200);
        self.name("oldInstaller", oi);
        let ni = self.file(dl, new_inst, 90 * MIB, 3);
        self.name("newInstaller", ni);
        self.file(dl, "photos-backup.zip", 700 * MIB, 120);

        // Duplicates: A (Documents) == B (Downloads) == C (Desktop); D has the same size
        // but different contents.
        let dup_size = 3 * MIB + 123;
        let a = self.file(docs, "Tax return 2025.pdf", dup_size, 100);
        let b = self.file(dl, "Tax return 2025 (1).pdf", dup_size, 90);
        let desk = self.dir(home, "Desktop");
        let c = self.file(desk, "Tax return 2025 - copy.pdf", dup_size, 80);
        let d = self.file(desk, "Not the same.pdf", dup_size, 80);
        for (k, id) in [("dupA", a), ("dupB", b), ("dupC", c), ("sameSizeOther", d)] {
            self.name(k, id);
        }
        for id in [a, b, c] {
            self.seeds.insert(id, 0xD0D0);
        }
        self.seeds.insert(d, 0xBEEF);

        // A hard-linked pair (one FileId, two names): counted once.
        let backup = self.dir(home, "Backup");
        let mut hl = self.file_meta("archive.vhdx", 2 * GIB, 60);
        hl.link_count = 2;
        let hla = self.push(Some(docs), hl.clone());
        let hlb = self.push(Some(backup), hl);
        self.name("hardlinkA", hla);
        self.name("hardlinkB", hlb);

        // Hidden and access-denied entries.
        let mut hid = self.file_meta(".settings.json", 4 * KIB, 10);
        hid.flags = EntryFlags::HIDDEN;
        let hid = self.push(Some(home), hid);
        self.name("hidden", hid);
        let mut den = base_meta("Locked", NodeKind::Dir);
        den.flags = EntryFlags::ACCESS_DENIED | EntryFlags::NOT_DESCENDED;
        let den = self.push(Some(home), den);
        self.name("denied", den);

        // Cloud folders: every provider, every state.
        self.cloud(home);

        // Developer caches.
        let proj = self.dir(home, "Projects");
        let web = self.dir(proj, "webapp");
        self.file(web, "package.json", 2 * KIB, 20);
        let nm = self.dir(web, "node_modules");
        self.name("nodeModules", nm);
        for pkg in ["react", "lodash", "typescript", "vite"] {
            let pd = self.dir(nm, pkg);
            for _ in 0..6 {
                self.random_file(pd, "index", &["js", "ts", "json", "map"], 4 * KIB, 8 * MIB);
            }
        }
        let rs = self.dir(proj, "rustapp");
        self.file(rs, "Cargo.toml", KIB, 20);
        let src = self.dir(rs, "src");
        self.file(src, "main.rs", 12 * KIB, 5);
        let target = self.dir(rs, "target");
        self.name("cargoTarget", target);
        let dbg = self.dir(target, "debug");
        for _ in 0..10 {
            self.random_file(dbg, "dep", &["rlib", "rmeta", "pdb", "exe"], MIB, 120 * MIB);
        }
        let gradle = self.path(home, &[".gradle", "caches"]);
        for _ in 0..5 {
            self.random_file(gradle, "module", &["jar"], MIB, 40 * MIB);
        }
        let cache_parent: &[&str] = match p {
            FixturePlatform::Windows => &["AppData", "Local"],
            FixturePlatform::Macos => &["Library", "Caches"],
            FixturePlatform::Linux => &[".cache"],
        };
        let cp = self.find_or_make(home, cache_parent);
        let pip = self.dir(cp, "pip");
        let pipc = self.dir(pip, "cache");
        self.name("pipCache", pipc);
        for _ in 0..6 {
            self.random_file(pipc, "wheel", &["whl"], MIB, 30 * MIB);
        }
        let npmc = self.dir(cp, "npm-cache");
        for _ in 0..6 {
            self.random_file(npmc, "tarball", &["tgz"], 100 * KIB, 20 * MIB);
        }

        // Temp folder.
        // Windows: the user's AppData\Local\Temp. macOS and Linux: /tmp.
        let temp_base = if p == FixturePlatform::Windows {
            home
        } else {
            root
        };
        let tp = self.find_or_make(temp_base, temp_parts);
        self.name("temp", tp);
        for _ in 0..12 {
            self.random_file(tp, "tmp", &["tmp", "log", "dat"], 100 * KIB, 40 * MIB);
        }

        // Bulk filler up to the target size.
        let archive = self.dir(home, "Archive");
        let mut batch = 0usize;
        while self.recs.len() < self.spec.target_nodes {
            let bd = self.dir(archive, &format!("Batch {batch:04}"));
            batch += 1;
            for f in 0..20 {
                if self.recs.len() >= self.spec.target_nodes {
                    break;
                }
                let sub = self.dir(bd, &format!("Folder {f:02}"));
                let n = 5 + self.rng.below(40) as usize;
                for _ in 0..n {
                    if self.recs.len() >= self.spec.target_nodes {
                        break;
                    }
                    self.random_file(
                        sub,
                        "item",
                        &["jpg", "pdf", "mp3", "txt", "docx", "zip", "rs", "dat"],
                        KIB,
                        50 * MIB,
                    );
                }
            }
        }

        self.finish(root_path)
    }

    /// Follows `parts` below `parent`, reusing existing dirs of the same name.
    fn find_or_make(&mut self, parent: NodeId, parts: &[&str]) -> NodeId {
        let mut cur = parent;
        for name in parts {
            let existing = self.recs.iter().find(|r| {
                r.parent == Some(cur) && r.meta.kind == NodeKind::Dir && r.meta.name == **name
            });
            cur = match existing {
                Some(r) => r.id,
                None => self.dir(cur, name),
            };
        }
        cur
    }

    fn cloud(&mut self, home: NodeId) {
        use CloudProvider::*;
        use CloudState::*;
        let p = self.spec.platform;
        let roots: Vec<(CloudProvider, Vec<&str>)> = match p {
            FixturePlatform::Windows => vec![
                (OneDrive, vec!["OneDrive"]),
                (GoogleDrive, vec!["Google Drive"]),
                (ICloud, vec!["iCloudDrive"]),
                (Dropbox, vec!["Dropbox"]),
                (Box, vec!["Box"]),
                (Other, vec!["SyncThing"]),
            ],
            FixturePlatform::Macos => vec![
                (
                    OneDrive,
                    vec!["Library", "CloudStorage", "OneDrive-Personal"],
                ),
                (
                    GoogleDrive,
                    vec!["Library", "CloudStorage", "GoogleDrive-william"],
                ),
                (
                    ICloud,
                    vec!["Library", "Mobile Documents", "com~apple~CloudDocs"],
                ),
                (Dropbox, vec!["Library", "CloudStorage", "Dropbox"]),
                (Box, vec!["Library", "CloudStorage", "Box-Box"]),
                (Other, vec!["Library", "CloudStorage", "OtherSync"]),
            ],
            FixturePlatform::Linux => vec![
                (Dropbox, vec!["Dropbox"]),
                (OneDrive, vec!["OneDrive"]),
                (GoogleDrive, vec!["GoogleDrive"]),
                (ICloud, vec!["iCloud"]),
                (Box, vec!["Box"]),
                (Other, vec!["Nextcloud"]),
            ],
        };
        for (i, (prov, parts)) in roots.into_iter().enumerate() {
            let r = self.find_or_make(home, &parts);
            if i == 0 {
                self.name("cloudRoot", r);
            }
            let co = self.cloud_file(r, "Presentation.pptx", 80 * MIB, prov, CloudOnly);
            let dl = self.cloud_file(r, "Trip video.mp4", 600 * MIB, prov, Downloaded);
            let pin = self.cloud_file(r, "Offline maps.zip", 300 * MIB, prov, Pinned);
            self.cloud_file(r, "notes.txt", 3 * KIB, prov, Downloaded);
            if i == 0 {
                self.name("cloudOnly", co);
                self.name("downloaded", dl);
                self.name("pinned", pin);
                // A cloud-only folder that the scanner did not enter.
                let mut f = base_meta("Shared albums", NodeKind::Dir);
                f.cloud = Some(CloudInfo {
                    provider: prov,
                    state: CloudOnly,
                });
                f.flags = EntryFlags::NOT_DESCENDED;
                let fid = self.push(Some(r), f);
                self.name("cloudOnlyFolder", fid);
                // A cloud-only file with the same size as a local one: never hashed.
                let twin = self.file_meta("Tax return 2025.pdf", 3 * MIB + 123, 100);
                let mut twin = twin;
                twin.allocated_size = 0;
                twin.cloud = Some(CloudInfo {
                    provider: prov,
                    state: CloudOnly,
                });
                let tw = self.push(Some(r), twin);
                self.name("cloudOnlyTwin", tw);
                self.seeds.insert(tw, 0xD0D0);
            }
        }
        if p == FixturePlatform::Linux {
            // An rclone mount that is not scanned by default.
            let mut m = base_meta("rclone-remote", NodeKind::Dir);
            m.flags = EntryFlags::REMOTE | EntryFlags::NOT_DESCENDED;
            let id = self.push(Some(home), m);
            self.name("remoteMount", id);
        }
    }

    fn finish(mut self, root_path: &str) -> Fixture {
        match self.spec.order {
            RecordOrder::TopDown => {}
            RecordOrder::ChildrenFirst => self.recs.reverse(),
            RecordOrder::Shuffled => {
                for i in (1..self.recs.len()).rev() {
                    let j = self.rng.below(i as u64 + 1) as usize;
                    self.recs.swap(i, j);
                }
            }
        }
        Fixture {
            spec: self.spec,
            records: self.recs,
            root_path: root_path.to_string(),
            ids: self.ids,
            content_seeds: self.seeds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;
    use crate::insights::{find_duplicates, suggest, DuplicateOptions, InsightOptions};
    use crate::scan::CancelToken;

    #[test]
    fn deterministic_and_order_independent() {
        let a = Fixture::small();
        let b = Fixture::small();
        assert_eq!(a.records, b.records);
        let t = a.tree();
        assert_eq!(t.orphan_count(), 0);
        let total = t.root().unwrap().totals;
        for order in [RecordOrder::ChildrenFirst, RecordOrder::Shuffled] {
            let f = Fixture::generate(FixtureSpec {
                order,
                ..FixtureSpec::default()
            });
            let t2 = f.tree();
            assert_eq!(t2.orphan_count(), 0);
            assert_eq!(t2.root().unwrap().totals, total, "{order:?}");
            assert_eq!(t2.root().unwrap().category(), t.root().unwrap().category());
        }
        assert!(a.records.len() >= 2_000);
    }

    #[test]
    fn contents_of_each_platform() {
        for platform in [
            FixturePlatform::Windows,
            FixturePlatform::Macos,
            FixturePlatform::Linux,
        ] {
            let f = Fixture::generate(FixtureSpec {
                platform,
                target_nodes: 500,
                ..FixtureSpec::default()
            });
            let t = f.tree();
            let home = t.display_path(f.id("home"));
            assert!(
                home.contains("William") || home.contains("william"),
                "{home}"
            );
            // Hard links counted once.
            assert!(t.get(f.id("hardlinkA")).unwrap().is_counted());
            assert!(!t.get(f.id("hardlinkB")).unwrap().is_counted());
            // Cloud states.
            let co = t.get(f.id("cloudOnly")).unwrap();
            assert_eq!(co.totals.allocated, 0);
            assert!(co.totals.cloud_only > 0);
            assert!(t.root().unwrap().totals.cloud_only > 0);
            // Every provider appears.
            let providers: std::collections::HashSet<_> = t
                .iter()
                .filter_map(|n| n.meta.cloud.map(|c| c.provider))
                .collect();
            assert_eq!(providers.len(), 6, "{platform:?}");
            let states: std::collections::HashSet<_> = t
                .iter()
                .filter_map(|n| n.meta.cloud.map(|c| c.state))
                .collect();
            assert_eq!(states.len(), 3);
            // Apps context applied.
            let apps_bytes: u64 = t
                .root()
                .unwrap()
                .category_mix()
                .iter()
                .filter(|(c, _)| *c == Category::Apps)
                .map(|(_, b)| *b)
                .sum();
            assert!(apps_bytes > 0, "{platform:?}");
        }
    }

    #[test]
    fn insights_on_fixture_never_read_cloud_only() {
        let f = Fixture::generate(FixtureSpec {
            target_nodes: 400,
            ..FixtureSpec::default()
        });
        let t = f.tree();
        let reader = f.reader(&t);
        let groups = find_duplicates(
            &t,
            &reader,
            DuplicateOptions {
                min_size: MIB,
                partial_bytes: 16 * KIB,
                ..DuplicateOptions::default()
            },
            &CancelToken::new(),
        )
        .unwrap();
        let dup = groups
            .iter()
            .find(|g| g.files.contains(&f.id("dupA")))
            .expect("duplicate group");
        let mut ids = dup.files.clone();
        ids.sort();
        let mut want = vec![f.id("dupA"), f.id("dupB"), f.id("dupC")];
        want.sort();
        assert_eq!(ids, want);
        let opened = reader.opened();
        for key in ["cloudOnly", "cloudOnlyTwin", "hardlinkB"] {
            assert!(
                !opened.contains(&t.path_of(f.id(key))),
                "{key} must never be opened"
            );
        }
        assert!(opened.contains(&t.path_of(f.id("sameSizeOther"))));

        let s = suggest(&t, &InsightOptions::new(FIXTURE_NOW), &groups);
        let has = |key: &str| s.iter().any(|x| x.node_id == f.id(key));
        for key in [
            "dupB",
            "dupC",
            "nodeModules",
            "cargoTarget",
            "pipCache",
            "temp",
            "oldInstaller",
            "staleVideo",
            "downloaded",
            "pinned",
        ] {
            assert!(has(key), "missing suggestion for {key}");
        }
        for key in ["dupA", "newInstaller", "cloudOnly", "cloudOnlyFolder"] {
            assert!(!has(key), "unexpected suggestion for {key}");
        }
    }

    #[test]
    fn gen_reader_is_deterministic_and_seekable() {
        let mut a = GenReader {
            seed: 7,
            len: 100,
            pos: 0,
        };
        let mut all = Vec::new();
        a.read_to_end(&mut all).unwrap();
        assert_eq!(all.len(), 100);
        a.seek(SeekFrom::Start(37)).unwrap();
        let mut b = [0u8; 10];
        a.read_exact(&mut b).unwrap();
        assert_eq!(&b, &all[37..47]);
    }

    /// `cargo test -p lustro-core --release -- --ignored --nocapture build_1m`
    #[test]
    #[ignore = "benchmark; run in release"]
    fn build_1m_nodes() {
        use std::time::Instant;
        let t0 = Instant::now();
        let f = Fixture::generate(FixtureSpec {
            target_nodes: 1_000_000,
            order: RecordOrder::TopDown,
            ..FixtureSpec::default()
        });
        let gen = t0.elapsed();
        for order in ["top-down", "children-first"] {
            let mut recs = f.records.clone();
            if order == "children-first" {
                recs.reverse();
            }
            // Pre-split into scanner-sized batches outside the timing.
            let mut batches = Vec::new();
            let mut it = recs.into_iter();
            loop {
                let chunk: Vec<_> = it.by_ref().take(10_000).collect();
                if chunk.is_empty() {
                    break;
                }
                batches.push(chunk);
            }
            let t1 = Instant::now();
            let b = crate::tree::TreeBuilder::with_root_path(&f.root_path);
            for chunk in batches {
                crate::scan::ScanSink::batch(&b, chunk);
            }
            let (tree, _) = b.finish();
            let build = t1.elapsed();
            println!(
                "1M fixture ({order}): generate {gen:?}, build {build:?}, nodes {}, root {} bytes",
                tree.len(),
                tree.root().unwrap().totals.allocated
            );
            assert_eq!(tree.orphan_count(), 0);
            if !cfg!(debug_assertions) {
                assert!(build.as_secs_f64() < 2.0, "build took {build:?}");
            }
        }
    }
}
