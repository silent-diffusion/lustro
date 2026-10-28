//! Cleanup insights: duplicates, dev caches, temp folders, old downloads, stale large files
//! and "free up space" for downloaded cloud files.
//!
//! # Safety
//! Only duplicate detection reads file contents, through the [`FileReader`] trait (so tests
//! can fake it). It never opens cloud-only files, placeholders (0 allocated bytes),
//! remote or not-descended entries, or extra hard links. Everything else here works from
//! the tree's metadata alone.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::category::Category;
use crate::dto::{ItemIdentity, Safety, SuggestionDto, SuggestionGroup};
use crate::model::{CloudState, EntryFlags, NodeId, NodeKind};
use crate::scan::CancelToken;
use crate::tree::{Node, Tree};

const DAY: i64 = 86_400;
const MIB: u64 = 1024 * 1024;

/// A readable, seekable file.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Opens local files for hashing. Implementations must not trigger cloud downloads; the
/// caller already filters out cloud-only files, but a reader may refuse more.
pub trait FileReader: Send + Sync {
    /// Re-reads the file's attributes right now (without opening its data) and says whether
    /// its contents are on this disk. Must return `false` for anything a read could
    /// download: cloud-only placeholders, recall-on-open / recall-on-data-access, offline
    /// (HSM) files, macOS dataless files. Called immediately before every `open`, because
    /// a file may have been freed up since the scan.
    fn is_local(&self, path: &Path) -> io::Result<bool>;
    /// Opens the file for reading. Implementations should refuse non-local files again on
    /// the open handle, and not follow links.
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek + '_>>;
}

/// Reads files with `std::fs`, checking cloud/offline attributes before and after opening.
/// It does not preserve access times; a platform reader can do better.
pub struct StdFileReader;

#[cfg(windows)]
mod local_check {
    use std::fs::Metadata;
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    pub const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    pub const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;

    pub fn is_local(md: &Metadata) -> bool {
        let remote = FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        md.file_attributes() & remote == 0
    }
}

#[cfg(target_os = "macos")]
mod local_check {
    use std::fs::Metadata;
    use std::os::macos::fs::MetadataExt;

    const SF_DATALESS: u32 = 0x4000_0000;

    pub fn is_local(md: &Metadata) -> bool {
        md.st_flags() & SF_DATALESS == 0
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod local_check {
    pub fn is_local(_: &std::fs::Metadata) -> bool {
        true
    }
}

impl FileReader for StdFileReader {
    fn is_local(&self, path: &Path) -> io::Result<bool> {
        // symlink_metadata reads attributes only and never follows links.
        let md = std::fs::symlink_metadata(path)?;
        Ok(md.is_file() && local_check::is_local(&md))
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        if !self.is_local(path)? {
            return Err(io::Error::other("file data is not on this disk"));
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.custom_flags(
                local_check::FILE_FLAG_OPEN_REPARSE_POINT | local_check::FILE_FLAG_SEQUENTIAL_SCAN,
            );
        }
        let f = opts.open(path)?;
        // Check again on the open handle, in case it changed in between.
        let md = f.metadata()?;
        if !md.is_file() || !local_check::is_local(&md) {
            return Err(io::Error::other("file data is not on this disk"));
        }
        Ok(Box::new(io::BufReader::new(f)))
    }
}

/// The duplicate search was cancelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

/// True if reading this node's contents is allowed (local data, real file, counted once).
pub fn is_readable(tree: &Tree, n: &Node) -> bool {
    if n.kind() != NodeKind::File || !n.is_counted() {
        return false;
    }
    if let Some(c) = n.meta.cloud {
        if c.state == CloudState::CloudOnly {
            return false;
        }
    }
    // A placeholder with no local data: reading it would download it.
    if n.meta.allocated_size == 0 && n.meta.logical_size > 0 {
        return false;
    }
    let bad = EntryFlags::REMOTE | EntryFlags::NOT_DESCENDED | EntryFlags::ACCESS_DENIED;
    if n.meta.flags.intersects(bad) {
        return false;
    }
    // Nothing under a remote or cloud-only ancestor.
    let mut p = n.parent;
    while let Some(pid) = p {
        let Some(a) = tree.get(pid) else { break };
        if a.meta.flags.contains(EntryFlags::REMOTE) || a.is_cloud_only() {
            return false;
        }
        p = a.parent;
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateOptions {
    /// `Settings.findDuplicates`. When false, nothing is read and no groups are returned.
    pub enabled: bool,
    /// Files smaller than this are ignored.
    pub min_size: u64,
    /// Bytes read from each end for the partial hash.
    pub partial_bytes: u64,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            min_size: MIB,
            partial_bytes: 16 * 1024,
        }
    }
}

impl DuplicateOptions {
    /// Defaults, switched on or off by the user's `findDuplicates` setting.
    pub fn from_settings(s: &crate::dto::Settings) -> Self {
        Self {
            enabled: s.find_duplicates,
            ..Self::default()
        }
    }
}

/// Opens `path` only if the reader confirms, right now, that its data is local.
fn open_local<'a>(reader: &'a dyn FileReader, path: &Path) -> io::Result<Box<dyn ReadSeek + 'a>> {
    if !reader.is_local(path)? {
        return Err(io::Error::other("not local"));
    }
    reader.open(path)
}

/// Files with identical contents. `files[0]` is the copy to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size: u64,
    /// Hex blake3 of the contents.
    pub hash: String,
    pub files: Vec<NodeId>,
}

impl DuplicateGroup {
    /// Bytes freed by removing every copy but the first.
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64).saturating_sub(1)
    }
}

const CHUNK: usize = 64 * 1024;

fn hash_partial(
    r: &mut dyn ReadSeek,
    size: u64,
    part: u64,
    cancel: &CancelToken,
) -> io::Result<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    h.update(&size.to_le_bytes());
    if size <= part * 2 {
        return hash_all(r, cancel);
    }
    let mut buf = vec![0u8; part as usize];
    r.read_exact(&mut buf)?;
    h.update(&buf);
    r.seek(SeekFrom::Start(size - part))?;
    r.read_exact(&mut buf)?;
    h.update(&buf);
    Ok(*h.finalize().as_bytes())
}

fn hash_all(r: &mut dyn ReadSeek, cancel: &CancelToken) -> io::Result<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(*h.finalize().as_bytes())
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// One file that may have a duplicate: its path and the order in which to prefer keeping it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: NodeId,
    pub path: PathBuf,
    /// Lower is a better copy to keep (see [`duplicate_candidates`]).
    keep_rank: (bool, usize, i64, NodeId),
}

/// Readable files grouped by size (only sizes with two or more files), taken from the
/// tree so that [`hash_candidates`] can run without it, i.e. without holding the
/// `TreeBuilder` lock while it reads file contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidates {
    groups: Vec<(u64, Vec<Candidate>)>,
}

impl Candidates {
    /// Number of files that will be hashed (at least partially).
    pub fn file_count(&self) -> usize {
        self.groups.iter().map(|(_, g)| g.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// Stage 1, under the tree lock: same-size groups of readable files (see [`is_readable`]),
/// with their paths and keep preference. Cheap: no file is touched. Empty when
/// `opts.enabled` is false.
pub fn duplicate_candidates(tree: &Tree, opts: DuplicateOptions) -> Candidates {
    if !opts.enabled {
        return Candidates::default();
    }
    let mut by_size: BTreeMap<u64, Vec<NodeId>> = BTreeMap::new();
    for n in tree.iter() {
        let size = n.meta.logical_size;
        if size >= opts.min_size.max(1) && is_readable(tree, n) {
            by_size.entry(size).or_default().push(n.id);
        }
    }
    let groups = by_size
        .into_iter()
        .filter(|(_, ids)| ids.len() >= 2)
        .map(|(size, mut ids)| {
            ids.sort_unstable();
            let files = ids
                .into_iter()
                .map(|id| {
                    let path = tree.path_of(id);
                    let modified = tree
                        .get(id)
                        .and_then(|n| n.meta.modified)
                        .unwrap_or(i64::MAX);
                    let rank = (
                        in_disposable_place(tree, id),
                        path.as_os_str().to_string_lossy().chars().count(),
                        modified,
                        id,
                    );
                    Candidate {
                        id,
                        path,
                        keep_rank: rank,
                    }
                })
                .collect();
            (size, files)
        })
        .collect();
    Candidates { groups }
}

/// Stages 2 and 3, without the tree: same partial hash (both ends), then same full blake3.
/// Every open goes through [`FileReader::is_local`] first. Unreadable files are skipped.
/// Groups are sorted by wasted bytes, largest first; `files[0]` is the copy to keep.
pub fn hash_candidates(
    candidates: Candidates,
    reader: &dyn FileReader,
    opts: DuplicateOptions,
    cancel: &CancelToken,
) -> Result<Vec<DuplicateGroup>, Cancelled> {
    if cancel.is_cancelled() {
        return Err(Cancelled);
    }
    if !opts.enabled {
        return Ok(Vec::new());
    }
    let part = opts.partial_bytes.max(1);
    let mut groups = Vec::new();
    for (size, files) in candidates.groups {
        // Stage 2: partial hash.
        let mut by_partial: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
        for c in files {
            if cancel.is_cancelled() {
                return Err(Cancelled);
            }
            let res = open_local(reader, &c.path)
                .and_then(|mut r| hash_partial(&mut *r, size, part, cancel));
            match res {
                Ok(h) => by_partial.entry(h).or_default().push(c),
                Err(e) if e.kind() == io::ErrorKind::Interrupted && cancel.is_cancelled() => {
                    return Err(Cancelled)
                }
                Err(_) => {}
            }
        }
        // Stage 3: full hash (the partial hash already is one for small files).
        for (ph, cands) in by_partial {
            if cands.len() < 2 {
                continue;
            }
            let mut by_full: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            if size <= part * 2 {
                by_full.insert(ph, cands);
            } else {
                for c in cands {
                    if cancel.is_cancelled() {
                        return Err(Cancelled);
                    }
                    match open_local(reader, &c.path).and_then(|mut r| hash_all(&mut *r, cancel)) {
                        Ok(h) => by_full.entry(h).or_default().push(c),
                        Err(_) if cancel.is_cancelled() => return Err(Cancelled),
                        Err(_) => {}
                    }
                }
            }
            for (h, mut files) in by_full {
                if files.len() < 2 {
                    continue;
                }
                files.sort_by_key(|c| c.keep_rank);
                groups.push(DuplicateGroup {
                    size,
                    hash: hex(&h),
                    files: files.into_iter().map(|c| c.id).collect(),
                });
            }
        }
    }
    groups.sort_by(|a, b| {
        b.wasted()
            .cmp(&a.wasted())
            .then_with(|| a.files[0].cmp(&b.files[0]))
    });
    Ok(groups)
}

/// [`duplicate_candidates`] then [`hash_candidates`] in one call. Holds `tree` for the
/// whole read; with a shared `TreeBuilder`, prefer the two calls so the lock is released
/// while hashing.
pub fn find_duplicates(
    tree: &Tree,
    reader: &dyn FileReader,
    opts: DuplicateOptions,
    cancel: &CancelToken,
) -> Result<Vec<DuplicateGroup>, Cancelled> {
    if cancel.is_cancelled() {
        return Err(Cancelled);
    }
    hash_candidates(duplicate_candidates(tree, opts), reader, opts, cancel)
}

/// True if `id` sits somewhere a cleanup is likely to empty: Downloads, Temp, caches,
/// developer caches, the recycle bin or a trash folder.
fn in_disposable_place(tree: &Tree, id: NodeId) -> bool {
    tree.ancestry(id).into_iter().any(|a| {
        tree.get(a).is_some_and(|n| {
            n.is_dir()
                && (dev_cache_reason(tree, n).is_some()
                    || is_temp_dir(tree, n)
                    || matches!(
                        lower(n).as_str(),
                        "downloads"
                            | "cache"
                            | "caches"
                            | ".cache"
                            | "$recycle.bin"
                            | ".trash"
                            | ".trashes"
                    ))
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsightOptions {
    /// Now, Unix seconds.
    pub now: i64,
    /// Installers/archives in Downloads older than this are suggested.
    pub old_download_days: u32,
    /// Large files not used for this long are suggested.
    pub stale_days: u32,
    pub stale_min_bytes: u64,
    pub dev_cache_min_bytes: u64,
    pub temp_min_bytes: u64,
    pub cloud_min_bytes: u64,
    /// At most this many suggestions per group (largest first).
    pub max_per_group: usize,
}

impl InsightOptions {
    pub fn new(now: i64) -> Self {
        Self {
            now,
            old_download_days: 30,
            stale_days: 365,
            stale_min_bytes: 500 * MIB,
            dev_cache_min_bytes: 10 * MIB,
            temp_min_bytes: 10 * MIB,
            cloud_min_bytes: 50 * MIB,
            max_per_group: 200,
        }
    }
}

fn lower(n: &Node) -> String {
    n.name().to_string_lossy().to_lowercase()
}

/// If `n` is a regenerable developer cache, a short explanation.
fn dev_cache_reason(tree: &Tree, n: &Node) -> Option<&'static str> {
    if !n.is_dir() {
        return None;
    }
    let name = lower(n);
    let parent = n.parent.and_then(|p| tree.get(p));
    let parent_name = parent.map(lower).unwrap_or_default();
    let sibling = |file: &str| {
        parent.is_some_and(|p| {
            p.children()
                .iter()
                .filter_map(|&c| tree.get(c))
                .any(|c| lower(c) == file)
        })
    };
    Some(match name.as_str() {
        "node_modules" => "JavaScript packages; `npm install` downloads them again",
        "target" if sibling("cargo.toml") => "Rust build output; `cargo build` recreates it",
        ".gradle" => "Gradle caches and build state; recreated on the next build",
        "__pycache__" => "Python bytecode cache; recreated automatically",
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" => {
            "Python tool cache; recreated automatically"
        }
        ".next" | ".nuxt" | ".parcel-cache" | ".turbo" | ".svelte-kit" => {
            "Web build cache; recreated on the next build"
        }
        "deriveddata" => "Xcode build data; recreated on the next build",
        "npm-cache" | "_cacache" => "npm download cache; refilled when needed",
        "cache" if parent_name == "pip" => "pip download cache; refilled when needed",
        "pip" if matches!(parent_name.as_str(), "cache" | "caches" | ".cache") => {
            "pip download cache; refilled when needed"
        }
        _ => return None,
    })
}

fn is_temp_dir(tree: &Tree, n: &Node) -> bool {
    if !n.is_dir() {
        return false;
    }
    let name = lower(n);
    if name != "temp" && name != "tmp" {
        return false;
    }
    // AppData\Local\Temp, C:\Windows\Temp, /tmp, /var/tmp.
    let parent = n.parent.and_then(|p| tree.get(p)).map(lower);
    matches!(parent.as_deref(), Some("local" | "windows" | "var")) || tree.depth(n.id) == 1
}

struct Walk {
    in_downloads: bool,
    user_area: bool,
}

/// Builds the suggestion list from the tree and the duplicate groups. Each node appears in
/// at most one group, and nothing inside a suggested folder is suggested again.
pub fn suggest(
    tree: &Tree,
    opts: &InsightOptions,
    duplicates: &[DuplicateGroup],
) -> Vec<SuggestionDto> {
    let mut out: HashMap<SuggestionGroup, Vec<SuggestionDto>> = HashMap::new();
    let mut taken: HashSet<NodeId> = HashSet::new();
    let mk = |n: &Node, group: SuggestionGroup, safety: Safety, reason: String, how: &str| {
        SuggestionDto {
            id: format!("{}:{}", group_key(group), n.id),
            group,
            safety,
            node_id: n.id,
            path: tree.display_path(n.id),
            name: n.name().to_string_lossy().into_owned(),
            bytes: n.totals.allocated,
            reason,
            how_found: how.to_string(),
            last_opened: last_used(n),
            keep_path: None,
            cloud: n.meta.cloud,
            identity: Some(ItemIdentity::of(n)),
            keep_identity: None,
        }
    };

    // Groups may have been hashed while the tree changed (see `hash_candidates`): ignore a
    // group whose kept copy is no longer in the tree.
    let duplicates: Vec<&DuplicateGroup> = duplicates
        .iter()
        .filter(|g| g.files.first().is_some_and(|&k| tree.is_attached(k)))
        .collect();
    let dup_ids: HashMap<NodeId, (&DuplicateGroup, NodeId)> = duplicates
        .iter()
        .copied()
        .flat_map(|g| g.files.iter().skip(1).map(move |&id| (id, (g, g.files[0]))))
        .collect();
    // Kept copies are never suggested for removal in another group.
    let keepers: HashSet<NodeId> = duplicates.iter().map(|g| g.files[0]).collect();

    let Some(root) = tree.root_id() else {
        return Vec::new();
    };
    let mut stack = vec![(
        root,
        Walk {
            in_downloads: false,
            user_area: false,
        },
    )];
    while let Some((id, w)) = stack.pop() {
        let Some(n) = tree.get(id) else { continue };
        let name = lower(n);

        if n.is_dir() && id != root {
            if let Some(why) = dev_cache_reason(tree, n) {
                if n.totals.allocated >= opts.dev_cache_min_bytes {
                    out.entry(SuggestionGroup::DevCaches).or_default().push(mk(
                        n,
                        SuggestionGroup::DevCaches,
                        Safety::Safe,
                        why.to_string(),
                        "Known developer cache folder name",
                    ));
                    taken.insert(id);
                }
                continue;
            }
            if is_temp_dir(tree, n) {
                if n.totals.allocated >= opts.temp_min_bytes {
                    let safety = if w.user_area {
                        Safety::Safe
                    } else {
                        Safety::Review
                    };
                    out.entry(SuggestionGroup::TempFiles).or_default().push(mk(
                        n,
                        SuggestionGroup::TempFiles,
                        safety,
                        "Temporary files left behind by apps and installers".into(),
                        "Temp folder",
                    ));
                    taken.insert(id);
                }
                continue;
            }
        }

        if n.kind() == NodeKind::File {
            if let Some((g, keep)) = dup_ids.get(&id) {
                let mut s = mk(
                    n,
                    SuggestionGroup::Duplicates,
                    Safety::Review,
                    format!(
                        "Identical to another copy ({} copies in all)",
                        g.files.len()
                    ),
                    "Same size and same blake3 hash of the full contents",
                );
                s.keep_path = Some(tree.display_path(*keep));
                s.keep_identity = tree.get(*keep).map(ItemIdentity::of);
                out.entry(SuggestionGroup::Duplicates).or_default().push(s);
                taken.insert(id);
            } else if keepers.contains(&id) {
                // The copy a duplicate suggestion keeps.
            } else if let Some(s) = file_suggestion(n, &w, opts, &mk) {
                out.entry(s.group).or_default().push(s);
                taken.insert(id);
            }
        }

        for &c in n.children() {
            stack.push((
                c,
                Walk {
                    in_downloads: w.in_downloads || name == "downloads",
                    user_area: w.user_area || matches!(name.as_str(), "users" | "home" | "appdata"),
                },
            ));
        }
    }

    let order = [
        SuggestionGroup::Duplicates,
        SuggestionGroup::DevCaches,
        SuggestionGroup::TempFiles,
        SuggestionGroup::OldDownloads,
        SuggestionGroup::StaleLarge,
        SuggestionGroup::FreeUpCloud,
    ];
    let mut all = Vec::new();
    for g in order {
        if let Some(mut v) = out.remove(&g) {
            v.sort_by(|a, b| {
                b.bytes
                    .cmp(&a.bytes)
                    .then_with(|| a.node_id.cmp(&b.node_id))
            });
            v.truncate(opts.max_per_group);
            all.extend(v);
        }
    }
    all
}

fn file_suggestion(
    n: &Node,
    w: &Walk,
    opts: &InsightOptions,
    mk: &dyn Fn(&Node, SuggestionGroup, Safety, String, &str) -> SuggestionDto,
) -> Option<SuggestionDto> {
    let age_days = |t: Option<i64>| t.map(|t| (opts.now - t) / DAY);
    let cat = n.category();
    let cloud_state = n.meta.cloud.map(|c| c.state);
    if cloud_state == Some(CloudState::CloudOnly) {
        return None;
    }

    if w.in_downloads && matches!(cat, Category::Installers | Category::Archives) {
        let days = age_days(n.meta.modified)?;
        if days >= i64::from(opts.old_download_days) && n.totals.allocated > 0 {
            let (safety, what) = if cat == Category::Installers {
                (Safety::Safe, "Installer")
            } else {
                (Safety::Review, "Archive")
            };
            return Some(mk(
                n,
                SuggestionGroup::OldDownloads,
                safety,
                format!("{what} downloaded {days} days ago"),
                "Installer or archive in Downloads, by age",
            ));
        }
    }

    if let Some(state) = cloud_state {
        if n.totals.allocated >= opts.cloud_min_bytes {
            let (safety, reason) = match state {
                CloudState::Pinned => (
                    Safety::Caution,
                    "Set to always stay on this device. Freeing space removes the local copy; the cloud keeps it".to_string(),
                ),
                _ => (
                    Safety::Safe,
                    "Downloaded cloud file. Freeing space keeps it in the cloud and removes only the local copy".to_string(),
                ),
            };
            return Some(mk(
                n,
                SuggestionGroup::FreeUpCloud,
                safety,
                reason,
                "Cloud file stored on this device",
            ));
        }
        return None;
    }

    if n.totals.allocated >= opts.stale_min_bytes
        && cat != Category::System
        && cat != Category::Apps
    {
        let days = age_days(last_used(n))?;
        if days >= i64::from(opts.stale_days) {
            return Some(mk(
                n,
                SuggestionGroup::StaleLarge,
                Safety::Review,
                format!("Large file not used for {days} days"),
                "Size and last-used date",
            ));
        }
    }
    None
}

/// Latest of accessed/modified (access times are often stale or off, so use both).
fn last_used(n: &Node) -> Option<i64> {
    match (n.meta.accessed, n.meta.modified) {
        (Some(a), Some(m)) => Some(a.max(m)),
        (a, m) => a.or(m),
    }
}

fn group_key(g: SuggestionGroup) -> &'static str {
    match g {
        SuggestionGroup::Duplicates => "duplicates",
        SuggestionGroup::DevCaches => "devCaches",
        SuggestionGroup::TempFiles => "tempFiles",
        SuggestionGroup::OldDownloads => "oldDownloads",
        SuggestionGroup::StaleLarge => "staleLarge",
        SuggestionGroup::FreeUpCloud => "freeUpCloud",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{CloudInfo, CloudProvider, EntryMeta, FileId, ScanRecord};
    use crate::tree::tests::{meta, rec};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// In-memory reader that records every path opened.
    #[derive(Default)]
    pub struct FakeReader {
        pub files: HashMap<PathBuf, Vec<u8>>,
        pub opened: Mutex<Vec<PathBuf>>,
        /// Paths that became cloud-only after the scan.
        pub freed: Mutex<HashSet<PathBuf>>,
    }

    impl FileReader for FakeReader {
        fn is_local(&self, path: &Path) -> io::Result<bool> {
            Ok(!self.freed.lock().unwrap().contains(path))
        }
        fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
            self.opened.lock().unwrap().push(path.to_path_buf());
            match self.files.get(path) {
                Some(b) => Ok(Box::new(io::Cursor::new(b.clone()))),
                None => Err(io::ErrorKind::NotFound.into()),
            }
        }
    }

    struct B {
        recs: Vec<ScanRecord>,
        next: NodeId,
    }

    impl B {
        fn new() -> Self {
            Self {
                recs: vec![rec(0, None, meta("C:\\", NodeKind::Dir, 0))],
                next: 1,
            }
        }
        fn add(&mut self, parent: NodeId, m: EntryMeta) -> NodeId {
            let id = self.next;
            self.next += 1;
            self.recs.push(rec(id, Some(parent), m));
            id
        }
        fn dir(&mut self, parent: NodeId, name: &str) -> NodeId {
            self.add(parent, meta(name, NodeKind::Dir, 0))
        }
        fn file(&mut self, parent: NodeId, name: &str, size: u64) -> NodeId {
            self.add(parent, meta(name, NodeKind::File, size))
        }
    }

    fn content(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31) ^ seed)
            .collect()
    }

    #[test]
    fn duplicates_three_stages_and_cloud_never_read() {
        let mut b = B::new();
        let docs = b.dir(0, "Docs");
        let dl = b.dir(0, "Downloads");
        let len = 100_000usize;
        let a = b.file(docs, "a.bin", len as u64);
        let a2 = b.file(dl, "a (1).bin", len as u64);
        let near = b.file(dl, "near.bin", len as u64); // same ends, different middle
        let other = b.file(dl, "other.bin", len as u64); // different ends
        let mut cm = meta("cloud.bin", NodeKind::File, len as u64);
        cm.allocated_size = 0;
        cm.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::CloudOnly,
        });
        let cloud = b.add(docs, cm);
        let mut hl = meta("link.bin", NodeKind::File, len as u64);
        hl.file_id = Some(FileId {
            volume: 1,
            index: 9,
        });
        hl.link_count = 2;
        let l1 = b.add(docs, hl.clone());
        let l2 = b.add(dl, hl);
        let small1 = b.file(docs, "s1", 10);
        let _small2 = b.file(dl, "s2", 10);
        let t = Tree::from_records(b.recs.clone());

        let mut r = FakeReader::default();
        let base = content(1, len);
        let mut nearc = base.clone();
        nearc[len / 2] ^= 0xFF;
        r.files.insert(t.path_of(a), base.clone());
        r.files.insert(t.path_of(a2), base.clone());
        r.files.insert(t.path_of(near), nearc);
        r.files.insert(t.path_of(other), content(2, len));
        r.files.insert(t.path_of(cloud), base.clone());
        r.files.insert(t.path_of(l1), base.clone());
        r.files.insert(t.path_of(l2), base.clone());

        let opts = DuplicateOptions {
            min_size: 1000,
            partial_bytes: 4096,
            ..DuplicateOptions::default()
        };
        let g = find_duplicates(&t, &r, opts, &CancelToken::new()).unwrap();
        assert_eq!(g.len(), 1, "{g:?}");
        // l1 is the first link and has identical content, so it joins the group.
        let mut ids = g[0].files.clone();
        ids.sort();
        assert_eq!(ids, vec![a, a2, l1]);
        assert_eq!(g[0].files[0], a, "shortest path kept");
        assert_eq!(g[0].wasted(), 2 * len as u64);
        let opened = r.opened.lock().unwrap();
        assert!(
            !opened.contains(&t.path_of(cloud)),
            "cloud-only file was opened"
        );
        assert!(
            !opened.contains(&t.path_of(l2)),
            "extra hard link was opened"
        );
        assert!(!opened.contains(&t.path_of(small1)));
        // `other` is rejected by the partial hash: opened once only.
        assert_eq!(opened.iter().filter(|p| **p == t.path_of(other)).count(), 1);
        // `near` survives the partial hash: opened twice.
        assert_eq!(opened.iter().filter(|p| **p == t.path_of(near)).count(), 2);
    }

    /// Three identical 5,000-byte files: Downloads/a, Docs/b, Temp area/c.
    fn three_copies() -> (Tree, FakeReader, [NodeId; 3]) {
        let mut b = B::new();
        let dl = b.dir(0, "Downloads");
        let docs = b.dir(0, "Docs");
        let a = b.file(dl, "a.bin", 5000);
        let bb = b.file(docs, "report-final.bin", 5000);
        let c = b.file(docs, "c.bin", 5000);
        let t = Tree::from_records(b.recs);
        let mut r = FakeReader::default();
        for id in [a, bb, c] {
            r.files.insert(t.path_of(id), content(9, 5000));
        }
        (t, r, [a, bb, c])
    }

    fn small_opts() -> DuplicateOptions {
        DuplicateOptions {
            min_size: 1000,
            partial_bytes: 1024,
            ..DuplicateOptions::default()
        }
    }

    #[test]
    fn split_hashing_runs_without_the_tree() {
        let (t, r, _) = three_copies();
        let together = find_duplicates(&t, &r, small_opts(), &CancelToken::new()).unwrap();
        let cands = duplicate_candidates(&t, small_opts());
        assert_eq!(cands.file_count(), 3);
        drop(t);
        let split = hash_candidates(cands, &r, small_opts(), &CancelToken::new()).unwrap();
        assert_eq!(split, together);
        let off = DuplicateOptions {
            enabled: false,
            ..small_opts()
        };
        let (t, _, _) = three_copies();
        assert!(duplicate_candidates(&t, off).is_empty());
    }

    #[test]
    fn freed_after_scan_is_never_opened() {
        let (t, r, [a, bb, c]) = three_copies();
        r.freed.lock().unwrap().insert(t.path_of(c));
        let g = find_duplicates(&t, &r, small_opts(), &CancelToken::new()).unwrap();
        assert_eq!(g.len(), 1);
        let mut ids = g[0].files.clone();
        ids.sort();
        assert_eq!(ids, vec![a, bb]);
        assert!(!r.opened.lock().unwrap().contains(&t.path_of(c)));
    }

    #[test]
    fn disabled_reads_nothing() {
        let (t, r, _) = three_copies();
        let off = DuplicateOptions {
            enabled: false,
            ..small_opts()
        };
        assert!(find_duplicates(&t, &r, off, &CancelToken::new())
            .unwrap()
            .is_empty());
        assert!(r.opened.lock().unwrap().is_empty());
        let mut s = crate::dto::Settings::default();
        assert!(DuplicateOptions::from_settings(&s).enabled);
        s.find_duplicates = false;
        assert!(!DuplicateOptions::from_settings(&s).enabled);
    }

    #[test]
    fn keeper_outside_downloads_and_never_suggested() {
        let (t, r, [a, _, c]) = three_copies();
        let g = find_duplicates(&t, &r, small_opts(), &CancelToken::new()).unwrap();
        // c.bin has the shortest path among the copies outside Downloads.
        assert_eq!(g[0].files[0], c);
        assert_eq!(*g[0].files.last().unwrap(), a, "Downloads copy last");

        // A keeper that would otherwise be an old download is not suggested.
        let now = 1_800_000_000;
        let mut b = B::new();
        let dl = b.dir(0, "Downloads");
        let mut m = meta("setup.exe", NodeKind::File, 80 * MIB);
        m.modified = Some(now - 90 * DAY);
        let keep = b.add(dl, m.clone());
        let other = b.add(dl, m);
        let t = Tree::from_records(b.recs);
        let g = DuplicateGroup {
            size: 80 * MIB,
            hash: "00".into(),
            files: vec![keep, other],
        };
        let s = suggest(&t, &InsightOptions::new(now), &[g]);
        assert!(s.iter().all(|x| x.node_id != keep));
        let d = s.iter().find(|x| x.node_id == other).unwrap();
        assert_eq!(d.group, SuggestionGroup::Duplicates);
        assert_eq!(d.identity, Some(ItemIdentity::of(t.get(other).unwrap())));
        assert_eq!(
            d.keep_identity,
            Some(ItemIdentity::of(t.get(keep).unwrap()))
        );
    }

    #[test]
    fn std_reader_reads_local_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("local.bin");
        std::fs::write(&p, b"hello").unwrap();
        assert!(StdFileReader.is_local(&p).unwrap());
        let mut buf = String::new();
        StdFileReader
            .open(&p)
            .unwrap()
            .read_to_string(&mut buf)
            .unwrap();
        assert_eq!(buf, "hello");
        assert!(
            !StdFileReader.is_local(dir.path()).unwrap(),
            "dirs are not files"
        );
        assert!(StdFileReader.is_local(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn duplicate_search_cancels() {
        let mut b = B::new();
        for i in 0..4 {
            b.file(0, &format!("f{i}"), 5000);
        }
        let t = Tree::from_records(b.recs);
        let r = FakeReader::default();
        let c = CancelToken::new();
        c.cancel();
        assert_eq!(
            find_duplicates(
                &t,
                &r,
                DuplicateOptions {
                    min_size: 1000,
                    partial_bytes: 1024,
                    ..DuplicateOptions::default()
                },
                &c
            ),
            Err(Cancelled)
        );
        assert!(r.opened.lock().unwrap().is_empty());
    }

    #[test]
    fn placeholders_and_remote_are_not_readable() {
        let mut b = B::new();
        let mut ph = meta("ph.docx", NodeKind::File, 100);
        ph.allocated_size = 0;
        let p = b.add(0, ph);
        let mut rm = meta("NAS", NodeKind::Dir, 0);
        rm.flags = EntryFlags::REMOTE;
        let nas = b.add(0, rm);
        let under = b.file(nas, "x.bin", 100);
        let ok = b.file(0, "ok.bin", 100);
        let mut pinned = meta("pinned.bin", NodeKind::File, 100);
        pinned.cloud = Some(CloudInfo {
            provider: CloudProvider::Dropbox,
            state: CloudState::Pinned,
        });
        let pin = b.add(0, pinned);
        let t = Tree::from_records(b.recs);
        assert!(!is_readable(&t, t.get(p).unwrap()));
        assert!(!is_readable(&t, t.get(under).unwrap()));
        assert!(is_readable(&t, t.get(ok).unwrap()));
        assert!(
            is_readable(&t, t.get(pin).unwrap()),
            "local cloud data is fine"
        );
        assert!(!is_readable(&t, t.get(nas).unwrap()));
    }

    #[test]
    fn suggestions() {
        let now = 1_800_000_000;
        let old = now - 400 * DAY;
        let mut b = B::new();
        let users = b.dir(0, "Users");
        let w = b.dir(users, "William");
        let dl = b.dir(w, "Downloads");
        let mut inst = meta("setup.exe", NodeKind::File, 80 * MIB);
        inst.modified = Some(now - 60 * DAY);
        let inst = b.add(dl, inst);
        let mut fresh = meta("new.msi", NodeKind::File, 80 * MIB);
        fresh.modified = Some(now - DAY);
        b.add(dl, fresh);
        let proj = b.dir(w, "proj");
        b.file(proj, "Cargo.toml", 1);
        let target = b.dir(proj, "target");
        b.file(target, "big.rlib", 50 * MIB);
        let nm = b.dir(proj, "node_modules");
        let inner = b.dir(nm, "node_modules");
        b.file(inner, "x.js", 30 * MIB);
        let other_target = b.dir(w, "target"); // no Cargo.toml next to it
        b.file(other_target, "keep.dat", 50 * MIB);
        let appdata = b.dir(w, "AppData");
        let local = b.dir(appdata, "Local");
        let temp = b.dir(local, "Temp");
        b.file(temp, "junk.tmp", 20 * MIB);
        let mut stale = meta("old.mkv", NodeKind::File, 600 * MIB);
        stale.modified = Some(old);
        stale.accessed = Some(old);
        let stale = b.add(w, stale);
        let od = b.dir(w, "OneDrive");
        let mut dlc = meta("trip.mp4", NodeKind::File, 200 * MIB);
        dlc.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::Downloaded,
        });
        let dlc = b.add(od, dlc);
        let mut pin = meta("pinned.mp4", NodeKind::File, 200 * MIB);
        pin.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::Pinned,
        });
        let pin = b.add(od, pin);
        let mut co = meta("cloud.mp4", NodeKind::File, 0);
        co.logical_size = 900 * MIB;
        co.modified = Some(old);
        co.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::CloudOnly,
        });
        b.add(od, co);
        let t = Tree::from_records(b.recs);

        let s = suggest(&t, &InsightOptions::new(now), &[]);
        let find = |id: NodeId| s.iter().find(|x| x.node_id == id);
        assert_eq!(find(inst).unwrap().group, SuggestionGroup::OldDownloads);
        assert_eq!(find(inst).unwrap().safety, Safety::Safe);
        assert_eq!(find(target).unwrap().group, SuggestionGroup::DevCaches);
        assert_eq!(find(nm).unwrap().group, SuggestionGroup::DevCaches);
        assert!(find(inner).is_none(), "nested node_modules not repeated");
        assert!(find(other_target).is_none());
        assert_eq!(find(temp).unwrap().group, SuggestionGroup::TempFiles);
        assert_eq!(find(temp).unwrap().safety, Safety::Safe);
        assert_eq!(find(stale).unwrap().group, SuggestionGroup::StaleLarge);
        assert_eq!(find(dlc).unwrap().group, SuggestionGroup::FreeUpCloud);
        assert_eq!(find(dlc).unwrap().safety, Safety::Safe);
        assert_eq!(find(pin).unwrap().safety, Safety::Caution);
        assert!(
            s.iter().all(|x| x.name != "cloud.mp4"),
            "cloud-only never suggested"
        );
        assert!(s.iter().all(|x| x.name != "new.msi"));
        let ids: HashSet<_> = s.iter().map(|x| x.id.clone()).collect();
        assert_eq!(ids.len(), s.len(), "ids are unique");
        assert_eq!(find(inst).unwrap().id, format!("oldDownloads:{inst}"));

        // Duplicates take precedence and carry the kept path.
        let g = DuplicateGroup {
            size: 600 * MIB,
            hash: "00".into(),
            files: vec![dlc, stale],
        };
        let s = suggest(&t, &InsightOptions::new(now), &[g]);
        let d = s.iter().find(|x| x.node_id == stale).unwrap();
        assert_eq!(d.group, SuggestionGroup::Duplicates);
        assert_eq!(d.keep_path.as_deref(), Some(t.display_path(dlc).as_str()));
        let mut limited = InsightOptions::new(now);
        limited.max_per_group = 1;
        let s = suggest(&t, &limited, &[]);
        assert_eq!(
            s.iter()
                .filter(|x| x.group == SuggestionGroup::DevCaches)
                .count(),
            1
        );
    }
}
