//! Serde mirrors of the UI DTOs in `app/ui/src/ipc/types.ts` (and the `Filter`/`SortKey`
//! of `backend.ts`), plus conversions from the tree. JSON is camelCase.
//!
//! Keep this file in step with `types.ts`: the `field_names` test pins every key.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::category::Category;
use crate::model::{CloudInfo, CloudProvider, CloudState, EntryFlags, FileId, NodeId, NodeKind};
use crate::scan::{ScanMode, ScanOptions, ScanProgress, ScanWarning, SkipReason};
use crate::tree::{Node, Tree};

/// Change per node since the comparison snapshot, bytes (see `snapshot::SnapshotStore::deltas`).
pub type Deltas = HashMap<NodeId, i64>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    /// The platform this binary was built for (Linux for other Unixes).
    pub fn current() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub name: String,
    pub kind: NodeKind,
    pub allocated: u64,
    pub logical: u64,
    pub cloud_only: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub category: Category,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub cloud: Option<CloudInfo>,
    pub delta: Option<i64>,
    pub hidden: bool,
    pub access_denied: bool,
    pub not_descended: Option<SkipReason>,
    pub child_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBytes {
    pub category: Category,
    pub allocated: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetailDto {
    #[serde(flatten)]
    pub node: NodeDto,
    pub path: String,
    pub category_mix: Vec<CategoryBytes>,
    pub largest_children: Vec<NodeDto>,
    pub share_of_parent: f64,
}

/// Nested subtree for charts. Ids are `i64` because fold nodes use `-(parentId + 1)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeDto {
    pub id: i64,
    pub parent: Option<i64>,
    pub name: String,
    pub kind: NodeKind,
    pub allocated: u64,
    pub logical: u64,
    pub cloud_only: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub category: Category,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub cloud: Option<CloudInfo>,
    pub delta: Option<i64>,
    pub hidden: bool,
    pub access_denied: bool,
    pub not_descended: Option<SkipReason>,
    pub child_count: u32,
    pub children: Vec<TreeDto>,
}

impl TreeDto {
    pub fn from_node(n: NodeDto, children: Vec<TreeDto>) -> TreeDto {
        TreeDto {
            id: i64::from(n.id),
            parent: n.parent.map(i64::from),
            name: n.name,
            kind: n.kind,
            allocated: n.allocated,
            logical: n.logical,
            cloud_only: n.cloud_only,
            file_count: n.file_count,
            dir_count: n.dir_count,
            category: n.category,
            modified: n.modified,
            accessed: n.accessed,
            cloud: n.cloud,
            delta: n.delta,
            hidden: n.hidden,
            access_denied: n.access_denied,
            not_descended: n.not_descended,
            child_count: n.child_count,
            children,
        }
    }

    /// Id of the "other" fold node under `parent`.
    pub fn fold_id(parent: NodeId) -> i64 {
        -(i64::from(parent) + 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProviderSummary {
    pub provider: CloudProvider,
    pub display_name: String,
    pub root_path: String,
    pub local_bytes: u64,
    pub pinned_bytes: u64,
    pub cloud_only_bytes: u64,
    pub remote: bool,
    pub virtual_volume_id: Option<String>,
    pub cache_root: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub id: String,
    pub label: String,
    pub mount_path: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub is_remote: bool,
    pub is_removable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub root: String,
    pub mode: ScanMode,
    pub cross_filesystems: bool,
    pub include_remote: bool,
    pub excluded: Vec<String>,
}

impl ScanRequest {
    /// Scanner options for this request. The caller validates the paths first.
    pub fn to_options(&self) -> ScanOptions {
        ScanOptions {
            root: PathBuf::from(&self.root),
            mode: self.mode,
            cross_filesystems: self.cross_filesystems,
            include_remote: self.include_remote,
            excluded: self.excluded.iter().map(PathBuf::from).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanUpdate {
    pub scan_id: String,
    pub progress: ScanProgress,
    pub nodes: Vec<NodeDto>,
    pub busiest: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDone {
    pub scan_id: String,
    pub mode_used: ScanMode,
    pub progress: ScanProgress,
    pub cancelled: bool,
    pub warnings: Vec<ScanWarning>,
    pub snapshot_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceBreakdownDto {
    pub volume_id: String,
    pub used_bytes: u64,
    pub segments: Vec<SpaceSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    Category,
    System,
    CloudOnly,
    Unexplained,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceSegment {
    pub kind: SegmentKind,
    pub key: String,
    pub label: String,
    pub bytes: u64,
    pub explanation: String,
    pub setting_action: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub root: String,
    pub taken_at: i64,
    pub allocated_bytes: u64,
    pub file_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffStatus {
    Grew,
    Shrank,
    New,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntryDto {
    pub id: Option<NodeId>,
    pub path: String,
    pub name: String,
    pub kind: NodeKind,
    pub category: Category,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
    pub sparkline: Vec<u64>,
    pub status: DiffStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub taken_at: i64,
    pub used_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDto {
    pub from: SnapshotMeta,
    pub to: SnapshotMeta,
    pub net_delta: i64,
    pub grew: u64,
    pub shrank: u64,
    pub entries: Vec<DiffEntryDto>,
    pub trend: Vec<TrendPoint>,
    pub days_until_full: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionGroup {
    Duplicates,
    DevCaches,
    TempFiles,
    OldDownloads,
    StaleLarge,
    FreeUpCloud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Safety {
    Safe,
    Review,
    Caution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDto {
    pub id: String,
    pub group: SuggestionGroup,
    pub safety: Safety,
    pub node_id: NodeId,
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub reason: String,
    pub how_found: String,
    pub last_opened: Option<i64>,
    pub keep_path: Option<String>,
    pub cloud: Option<CloudInfo>,
    /// The item as scanned. Cleanup checks it again right before acting and skips the item
    /// if it changed (backend-only; the UI never needs to read or send it back).
    #[serde(default)]
    pub identity: Option<ItemIdentity>,
    /// Duplicates: the kept copy as scanned, checked before removing this one.
    #[serde(default)]
    pub keep_identity: Option<ItemIdentity>,
}

/// What an item was when it was scanned. The file id is text so JavaScript can't round it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemIdentity {
    pub kind: NodeKind,
    #[serde(with = "file_id_text")]
    pub file_id: Option<FileId>,
    pub logical_size: u64,
    pub modified: Option<i64>,
}

impl ItemIdentity {
    pub fn of(n: &Node) -> ItemIdentity {
        ItemIdentity {
            kind: n.kind(),
            file_id: n.meta.file_id,
            logical_size: n.meta.logical_size,
            modified: n.meta.modified,
        }
    }

    /// True if `now` (read from disk just before cleanup) is still the scanned item.
    /// Files must keep their id, size and modified time (2 s slack for FAT); folders their
    /// id (their contents change all the time). A missing file id on either side is a
    /// mismatch; a missing modified time is not compared.
    pub fn still_matches(&self, now: &ItemIdentity) -> bool {
        if self.kind != now.kind {
            return false;
        }
        // Without an id on both sides we can't tell a replaced item from the original.
        match (self.file_id, now.file_id) {
            (Some(a), Some(b)) if a == b => {}
            _ => return false,
        }
        if self.kind == NodeKind::File {
            if self.logical_size != now.logical_size {
                return false;
            }
            if let (Some(a), Some(b)) = (self.modified, now.modified) {
                if (a - b).abs() > 2 {
                    return false;
                }
            }
        }
        true
    }
}

/// `FileId` as `"<volume hex>:<index hex>"`.
mod file_id_text {
    use crate::model::FileId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<FileId>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(f) => s.serialize_some(&format!("{:x}:{:x}", f.volume, f.index)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<FileId>, D::Error> {
        let Some(text) = Option::<String>::deserialize(d)? else {
            return Ok(None);
        };
        let bad = || serde::de::Error::custom("bad file id");
        let (v, i) = text.split_once(':').ok_or_else(bad)?;
        Ok(Some(FileId {
            volume: u64::from_str_radix(v, 16).map_err(|_| bad())?,
            index: u64::from_str_radix(i, 16).map_err(|_| bad())?,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Remove,
    Keep,
    Later,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Destination {
    Trash,
    Move { target: String },
    Compress,
    FreeUpCloud,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionItem {
    pub suggestion_id: String,
    pub decision: Decision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub decisions: Vec<DecisionItem>,
    pub destination: Destination,
}

/// A path that could not be processed, with a user-facing message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedItem {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub applied: u32,
    pub freed_bytes: u64,
    pub failed: Vec<FailedItem>,
    pub undo_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoEntry {
    pub id: String,
    pub at: i64,
    pub items: u32,
    pub bytes: u64,
    pub destination: Destination,
    pub undoable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MainView {
    BlockMap,
    RingChart,
    FileTree,
    Terrain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    Comfortable,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainPins {
    pub largest: bool,
    pub grew: bool,
    pub duplicates: bool,
    pub cloud: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainSettings {
    pub category_tint: bool,
    pub pins: TerrainPins,
    pub cloud_valleys: bool,
    #[serde(rename = "prefer2d")]
    pub prefer_2d: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub main_view: MainView,
    pub theme: Theme,
    pub density: Density,
    pub scan_mode: ScanMode,
    pub excluded: Vec<String>,
    pub count_cloud_only_in_totals: bool,
    pub hidden_providers: Vec<CloudProvider>,
    pub keep_snapshots: bool,
    pub snapshots_to_keep: u32,
    pub terrain: TerrainSettings,
    pub kept_items: Vec<String>,
    pub protected_paths: Vec<String>,
    /// Find duplicates by hashing same-size local files after a scan (default true). When
    /// false, no file contents are read at all. See `insights::DuplicateOptions::enabled`.
    #[serde(default = "default_true")]
    pub find_duplicates: bool,
    /// Defaults to true, also when loading settings saved before this field existed.
    #[serde(default = "default_true")]
    pub auto_update: bool,
    #[serde(default)]
    pub skipped_version: Option<String>,
    /// Unix seconds of the last update check.
    #[serde(default)]
    pub last_update_check: Option<i64>,
    /// Version whose "What is new" card was already shown.
    #[serde(default)]
    pub notes_shown_for_version: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            main_view: MainView::BlockMap,
            theme: Theme::Dark,
            density: Density::Comfortable,
            scan_mode: ScanMode::Standard,
            excluded: Vec::new(),
            count_cloud_only_in_totals: false,
            hidden_providers: Vec::new(),
            keep_snapshots: true,
            snapshots_to_keep: 12,
            terrain: TerrainSettings {
                category_tint: true,
                pins: TerrainPins {
                    largest: true,
                    grew: true,
                    duplicates: true,
                    cloud: true,
                },
                cloud_valleys: true,
                prefer_2d: false,
            },
            kept_items: Vec::new(),
            protected_paths: Vec::new(),
            find_duplicates: true,
            auto_update: true,
            skipped_version: None,
            last_update_check: None,
            notes_shown_for_version: None,
        }
    }
}

/// An available update (from the signed manifest; the update logic lives in the Tauri app).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub published_at: Option<i64>,
    /// Markdown release notes. The UI never renders them as raw HTML.
    pub notes: String,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    Idle,
    Checking,
    #[serde(rename_all = "camelCase")]
    UpToDate {
        checked_at: i64,
    },
    Available {
        info: UpdateInfo,
    },
    Downloading {
        info: UpdateInfo,
        downloaded: u64,
        total: Option<u64>,
    },
    Ready {
        info: UpdateInfo,
    },
    Error {
        kind: UpdateErrorKind,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateErrorKind {
    Offline,
    Download,
    Verify,
}

/// `SortKey` of `backend.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    Allocated,
    Logical,
    Name,
    Modified,
    FileCount,
    Delta,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudFilter {
    #[default]
    Any,
    Local,
    CloudOnly,
}

/// `Filter` of `backend.ts`. All fields optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<Category>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub older_than_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudFilter>,
}

/// Why a directory was not descended. The model only has the `NOT_DESCENDED` flag, so the
/// reason is inferred from the other flags.
pub fn skip_reason(n: &Node) -> Option<SkipReason> {
    let f = n.meta.flags;
    if !f.contains(EntryFlags::NOT_DESCENDED) {
        return None;
    }
    Some(if f.contains(EntryFlags::REMOTE) {
        SkipReason::Remote
    } else if matches!(n.meta.cloud, Some(c) if c.state == CloudState::CloudOnly) {
        SkipReason::CloudOnlyFolder
    } else if f.contains(EntryFlags::MOUNT_POINT) {
        SkipReason::OtherFilesystem
    } else {
        SkipReason::Excluded
    })
}

/// DTO for one node.
pub fn node_dto(n: &Node, deltas: Option<&Deltas>) -> NodeDto {
    NodeDto {
        id: n.id,
        parent: n.parent,
        name: n.name().to_string_lossy().into_owned(),
        kind: n.kind(),
        allocated: n.totals.allocated,
        logical: n.totals.logical,
        cloud_only: n.totals.cloud_only,
        file_count: n.totals.files,
        dir_count: n.totals.dirs,
        category: n.category(),
        modified: n.meta.modified,
        accessed: n.meta.accessed,
        cloud: n.meta.cloud,
        delta: deltas.and_then(|d| d.get(&n.id).copied()),
        hidden: n.meta.flags.contains(EntryFlags::HIDDEN),
        access_denied: n.meta.flags.contains(EntryFlags::ACCESS_DENIED),
        not_descended: skip_reason(n),
        child_count: u32::try_from(n.children().len()).unwrap_or(u32::MAX),
    }
}

/// DTOs for a list of ids (unknown ids are skipped).
pub fn node_dtos(tree: &Tree, ids: &[NodeId], deltas: Option<&Deltas>) -> Vec<NodeDto> {
    ids.iter()
        .filter_map(|&id| tree.get(id))
        .map(|n| node_dto(n, deltas))
        .collect()
}

/// Detail DTO for `id`, with the `largest` biggest children.
pub fn node_detail(
    tree: &Tree,
    id: NodeId,
    largest: usize,
    deltas: Option<&Deltas>,
) -> Option<NodeDetailDto> {
    let n = tree.get(id)?;
    let mut kids: Vec<&Node> = n.children().iter().filter_map(|&c| tree.get(c)).collect();
    kids.sort_by_key(|k| std::cmp::Reverse(k.totals.allocated));
    let share = match n.parent.and_then(|p| tree.get(p)) {
        Some(p) if p.totals.allocated > 0 => n.totals.allocated as f64 / p.totals.allocated as f64,
        Some(_) => 0.0,
        None => 1.0,
    };
    Some(NodeDetailDto {
        node: node_dto(n, deltas),
        path: tree.display_path(id),
        category_mix: n
            .category_mix()
            .into_iter()
            .map(|(category, allocated)| CategoryBytes {
                category,
                allocated,
            })
            .collect(),
        largest_children: kids
            .into_iter()
            .take(largest)
            .map(|k| node_dto(k, deltas))
            .collect(),
        share_of_parent: share,
    })
}

/// One sync root as the caller knows it. The app maps `lustro_cloud::SyncRoot` to this
/// (lustro-core cannot depend on lustro-cloud). Paths are display-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudRootInfo {
    pub provider: CloudProvider,
    pub display_name: String,
    pub root_path: PathBuf,
    /// A remote/FUSE/virtual drive that is not scanned by default.
    pub remote: bool,
    pub virtual_volume_id: Option<String>,
    /// Local cache folder holding the provider's on-disk bytes (Google DriveFS).
    pub cache_root: Option<PathBuf>,
}

/// Node at `path` in the tree, if the path lies inside the scanned root.
fn find_path(tree: &Tree, path: &std::path::Path) -> Option<NodeId> {
    let root = tree.root_display()?;
    let rest = path.strip_prefix(&root).ok()?;
    let parts: Vec<String> = rest
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    tree.find(&refs)
}

#[derive(Default)]
struct CloudBytes {
    local: u64,
    pinned: u64,
    cloud_only: u64,
}

fn add_file(b: &mut CloudBytes, n: &Node) {
    match n.meta.cloud.map(|c| c.state) {
        Some(CloudState::CloudOnly) => b.cloud_only += n.totals.cloud_only,
        Some(CloudState::Pinned) => b.pinned += n.totals.allocated,
        // Downloaded, or a full local copy (classic Dropbox, Drive Mirror mode).
        _ => b.local += n.totals.allocated,
    }
}

fn subtree_bytes(tree: &Tree, top: NodeId, b: &mut CloudBytes) {
    let mut stack = vec![top];
    while let Some(id) = stack.pop() {
        let Some(n) = tree.get(id) else { continue };
        if n.is_dir() {
            stack.extend_from_slice(n.children());
        } else {
            add_file(b, n);
        }
    }
}

/// Per-provider rollup for Overview's "Cloud storage" card. One summary per `roots` entry
/// (roots outside the scanned tree get zero bytes), then one per provider that has cloud
/// files in the tree outside every listed root (`rootPath` empty).
pub fn cloud_summary(tree: &Tree, roots: &[CloudRootInfo]) -> Vec<CloudProviderSummary> {
    let mut out = Vec::new();
    let mut covered: Vec<NodeId> = Vec::new();
    for r in roots {
        let mut b = CloudBytes::default();
        if let Some(id) = find_path(tree, &r.root_path) {
            subtree_bytes(tree, id, &mut b);
            covered.push(id);
        }
        if let Some(cache) = r.cache_root.as_deref().and_then(|c| find_path(tree, c)) {
            let mut c = CloudBytes::default();
            subtree_bytes(tree, cache, &mut c);
            b.local += c.local + c.pinned;
            covered.push(cache);
        }
        out.push(CloudProviderSummary {
            provider: r.provider,
            display_name: r.display_name.clone(),
            root_path: r.root_path.to_string_lossy().into_owned(),
            local_bytes: b.local,
            pinned_bytes: b.pinned,
            cloud_only_bytes: b.cloud_only,
            remote: r.remote,
            virtual_volume_id: r.virtual_volume_id.clone(),
            cache_root: r
                .cache_root
                .as_ref()
                .map(|c| c.to_string_lossy().into_owned()),
        });
    }

    let inside_covered = |n: &Node| {
        let mut p = Some(n.id);
        while let Some(id) = p {
            if covered.contains(&id) {
                return true;
            }
            p = tree.get(id).and_then(|x| x.parent);
        }
        false
    };
    let mut extra: HashMap<CloudProvider, CloudBytes> = HashMap::new();
    for n in tree.iter() {
        if let Some(c) = n.meta.cloud {
            if n.kind() == NodeKind::File && !inside_covered(n) {
                add_file(extra.entry(c.provider).or_default(), n);
            }
        }
    }
    let mut extra: Vec<_> = extra.into_iter().collect();
    extra.sort_by_key(|(p, _)| *p as u8);
    for (provider, b) in extra {
        out.push(CloudProviderSummary {
            provider,
            display_name: provider_name(provider).into(),
            root_path: String::new(),
            local_bytes: b.local,
            pinned_bytes: b.pinned,
            cloud_only_bytes: b.cloud_only,
            remote: false,
            virtual_volume_id: None,
            cache_root: None,
        });
    }
    out
}

/// Generic display name for a provider.
pub fn provider_name(p: CloudProvider) -> &'static str {
    match p {
        CloudProvider::OneDrive => "OneDrive",
        CloudProvider::GoogleDrive => "Google Drive",
        CloudProvider::ICloud => "iCloud Drive",
        CloudProvider::Dropbox => "Dropbox",
        CloudProvider::Box => "Box",
        CloudProvider::Other => "Other cloud",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CloudProvider;
    use crate::tree::tests::sample;
    use serde_json::Value;

    fn keys(v: &Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
        k.sort();
        k
    }

    fn assert_keys<T: Serialize>(value: &T, expected: &[&str]) {
        let v = serde_json::to_value(value).unwrap();
        let mut e: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        e.sort();
        assert_eq!(keys(&v), e, "{v}");
    }

    const NODE_KEYS: &[&str] = &[
        "id",
        "parent",
        "name",
        "kind",
        "allocated",
        "logical",
        "cloudOnly",
        "fileCount",
        "dirCount",
        "category",
        "modified",
        "accessed",
        "cloud",
        "delta",
        "hidden",
        "accessDenied",
        "notDescended",
        "childCount",
    ];

    fn sample_node() -> NodeDto {
        let t = Tree::from_records(sample());
        node_dto(t.get(2).unwrap(), None)
    }

    fn meta() -> SnapshotMeta {
        SnapshotMeta {
            id: "1".into(),
            root: "C:\\".into(),
            taken_at: 1,
            allocated_bytes: 2,
            file_count: 3,
        }
    }

    #[test]
    fn field_names() {
        let n = sample_node();
        assert_keys(&n, NODE_KEYS);

        let t = Tree::from_records(sample());
        let d = node_detail(&t, 2, 5, None).unwrap();
        let mut dk = NODE_KEYS.to_vec();
        dk.extend(["path", "categoryMix", "largestChildren", "shareOfParent"]);
        assert_keys(&d, &dk);
        assert_keys(&d.category_mix[0], &["category", "allocated"]);

        let td = TreeDto::from_node(n.clone(), vec![]);
        let mut tk = NODE_KEYS.to_vec();
        tk.push("children");
        assert_keys(&td, &tk);

        assert_keys(
            &CloudProviderSummary {
                provider: CloudProvider::OneDrive,
                display_name: "OneDrive - Personal".into(),
                root_path: "C:\\Users\\William\\OneDrive".into(),
                local_bytes: 1,
                pinned_bytes: 2,
                cloud_only_bytes: 3,
                remote: false,
                virtual_volume_id: None,
                cache_root: None,
            },
            &[
                "provider",
                "displayName",
                "rootPath",
                "localBytes",
                "pinnedBytes",
                "cloudOnlyBytes",
                "remote",
                "virtualVolumeId",
                "cacheRoot",
            ],
        );
        assert_keys(
            &VolumeInfo {
                id: "C:".into(),
                label: "C:".into(),
                mount_path: "C:\\".into(),
                file_system: "NTFS".into(),
                total_bytes: 1,
                free_bytes: 1,
                is_remote: false,
                is_removable: false,
            },
            &[
                "id",
                "label",
                "mountPath",
                "fileSystem",
                "totalBytes",
                "freeBytes",
                "isRemote",
                "isRemovable",
            ],
        );
        assert_keys(
            &ScanRequest {
                root: "C:\\".into(),
                mode: ScanMode::Fast,
                cross_filesystems: false,
                include_remote: false,
                excluded: vec![],
            },
            &[
                "root",
                "mode",
                "crossFilesystems",
                "includeRemote",
                "excluded",
            ],
        );
        assert_keys(
            &ScanProgress::default(),
            &["files", "dirs", "allocatedBytes", "elapsedMs"],
        );
        assert_keys(
            &ScanUpdate {
                scan_id: "s".into(),
                progress: ScanProgress::default(),
                nodes: vec![n.clone()],
                busiest: None,
            },
            &["scanId", "progress", "nodes", "busiest"],
        );
        let done = ScanDone {
            scan_id: "s".into(),
            mode_used: ScanMode::Standard,
            progress: ScanProgress::default(),
            cancelled: false,
            warnings: vec![ScanWarning {
                path: "p".into(),
                message: "m".into(),
            }],
            snapshot_id: None,
        };
        assert_keys(
            &done,
            &[
                "scanId",
                "modeUsed",
                "progress",
                "cancelled",
                "warnings",
                "snapshotId",
            ],
        );
        assert_keys(&done.warnings[0], &["path", "message"]);
        let seg = SpaceSegment {
            kind: SegmentKind::CloudOnly,
            key: "pagefile".into(),
            label: "l".into(),
            bytes: 1,
            explanation: "e".into(),
            setting_action: None,
        };
        assert_keys(
            &seg,
            &[
                "kind",
                "key",
                "label",
                "bytes",
                "explanation",
                "settingAction",
            ],
        );
        assert_keys(
            &SpaceBreakdownDto {
                volume_id: "C:".into(),
                used_bytes: 1,
                segments: vec![seg],
            },
            &["volumeId", "usedBytes", "segments"],
        );
        assert_keys(
            &meta(),
            &["id", "root", "takenAt", "allocatedBytes", "fileCount"],
        );
        let entry = DiffEntryDto {
            id: Some(1),
            path: "p".into(),
            name: "n".into(),
            kind: NodeKind::Dir,
            category: Category::Video,
            before: 1,
            after: 2,
            delta: 1,
            sparkline: vec![1, 2],
            status: DiffStatus::Grew,
        };
        assert_keys(
            &entry,
            &[
                "id",
                "path",
                "name",
                "kind",
                "category",
                "before",
                "after",
                "delta",
                "sparkline",
                "status",
            ],
        );
        let diff = DiffDto {
            from: meta(),
            to: meta(),
            net_delta: 1,
            grew: 1,
            shrank: 0,
            entries: vec![entry],
            trend: vec![TrendPoint {
                taken_at: 1,
                used_bytes: 2,
            }],
            days_until_full: None,
        };
        assert_keys(
            &diff,
            &[
                "from",
                "to",
                "netDelta",
                "grew",
                "shrank",
                "entries",
                "trend",
                "daysUntilFull",
            ],
        );
        assert_keys(&diff.trend[0], &["takenAt", "usedBytes"]);
        assert_keys(
            &SuggestionDto {
                id: "x".into(),
                group: SuggestionGroup::Duplicates,
                safety: Safety::Review,
                node_id: 1,
                path: "p".into(),
                name: "n".into(),
                bytes: 1,
                reason: "r".into(),
                how_found: "h".into(),
                last_opened: None,
                keep_path: None,
                cloud: None,
                identity: Some(ItemIdentity {
                    kind: NodeKind::File,
                    file_id: Some(FileId {
                        volume: 0xC0FFEE,
                        index: (7 << 48) | 42,
                    }),
                    logical_size: 1,
                    modified: Some(5),
                }),
                keep_identity: None,
            },
            &[
                "id",
                "group",
                "safety",
                "nodeId",
                "path",
                "name",
                "bytes",
                "reason",
                "howFound",
                "lastOpened",
                "keepPath",
                "cloud",
                "identity",
                "keepIdentity",
            ],
        );
        let req = ApplyRequest {
            decisions: vec![DecisionItem {
                suggestion_id: "x".into(),
                decision: Decision::Remove,
            }],
            destination: Destination::Trash,
        };
        assert_keys(&req, &["decisions", "destination"]);
        assert_keys(&req.decisions[0], &["suggestionId", "decision"]);
        assert_keys(
            &ApplyResult {
                applied: 1,
                freed_bytes: 2,
                failed: vec![FailedItem {
                    path: "p".into(),
                    message: "m".into(),
                }],
                undo_id: "u".into(),
            },
            &["applied", "freedBytes", "failed", "undoId"],
        );
        assert_keys(
            &UndoEntry {
                id: "u".into(),
                at: 1,
                items: 1,
                bytes: 1,
                destination: Destination::Compress,
                undoable: true,
            },
            &["id", "at", "items", "bytes", "destination", "undoable"],
        );
        let s = Settings::default();
        assert_keys(
            &s,
            &[
                "mainView",
                "theme",
                "density",
                "scanMode",
                "excluded",
                "countCloudOnlyInTotals",
                "hiddenProviders",
                "keepSnapshots",
                "snapshotsToKeep",
                "terrain",
                "keptItems",
                "protectedPaths",
                "findDuplicates",
                "autoUpdate",
                "skippedVersion",
                "lastUpdateCheck",
                "notesShownForVersion",
            ],
        );
        assert_keys(
            &s.terrain,
            &["categoryTint", "pins", "cloudValleys", "prefer2d"],
        );
        assert_keys(&s.terrain.pins, &["largest", "grew", "duplicates", "cloud"]);
        let f = Filter {
            text: Some("a".into()),
            categories: Some(vec![Category::Video]),
            min_bytes: Some(1),
            max_bytes: Some(2),
            older_than_days: Some(3),
            cloud: Some(CloudFilter::CloudOnly),
        };
        assert_keys(
            &f,
            &[
                "text",
                "categories",
                "minBytes",
                "maxBytes",
                "olderThanDays",
                "cloud",
            ],
        );
        assert_keys(&Filter::default(), &[]);

        let info = UpdateInfo {
            version: "0.3.0".into(),
            current_version: "0.2.0".into(),
            published_at: Some(1),
            notes: "n".into(),
            size_bytes: None,
        };
        assert_keys(
            &info,
            &[
                "version",
                "currentVersion",
                "publishedAt",
                "notes",
                "sizeBytes",
            ],
        );
        let cases: Vec<(UpdateStatus, &str, Vec<&str>)> = vec![
            (UpdateStatus::Idle, "idle", vec!["state"]),
            (UpdateStatus::Checking, "checking", vec!["state"]),
            (
                UpdateStatus::UpToDate { checked_at: 5 },
                "upToDate",
                vec!["state", "checkedAt"],
            ),
            (
                UpdateStatus::Available { info: info.clone() },
                "available",
                vec!["state", "info"],
            ),
            (
                UpdateStatus::Downloading {
                    info: info.clone(),
                    downloaded: 1,
                    total: None,
                },
                "downloading",
                vec!["state", "info", "downloaded", "total"],
            ),
            (UpdateStatus::Ready { info }, "ready", vec!["state", "info"]),
            (
                UpdateStatus::Error {
                    kind: UpdateErrorKind::Verify,
                    message: "m".into(),
                },
                "error",
                vec!["state", "kind", "message"],
            ),
        ];
        for (status, tag, fields) in cases {
            assert_keys(&status, &fields);
            let v = serde_json::to_value(&status).unwrap();
            assert_eq!(v["state"], tag);
            let back: UpdateStatus = serde_json::from_value(v).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn identity_round_trip_and_matching() {
        let id = ItemIdentity {
            kind: NodeKind::File,
            file_id: Some(FileId {
                volume: 0xDEAD_BEEF,
                index: u64::MAX - 1,
            }),
            logical_size: 100,
            modified: Some(1_000),
        };
        let v = serde_json::to_value(id).unwrap();
        assert_eq!(v["fileId"], "deadbeef:fffffffffffffffe");
        assert_eq!(serde_json::from_value::<ItemIdentity>(v).unwrap(), id);
        let none = ItemIdentity {
            file_id: None,
            ..id
        };
        let v = serde_json::to_value(none).unwrap();
        assert!(v["fileId"].is_null());
        assert_eq!(serde_json::from_value::<ItemIdentity>(v).unwrap(), none);
        // No id on either side: can't prove it's the same item.
        assert!(!id.still_matches(&none));
        assert!(!none.still_matches(&id));
        assert!(!none.still_matches(&none));
        let dir_no_id = ItemIdentity {
            kind: NodeKind::Dir,
            ..none
        };
        assert!(
            !dir_no_id.still_matches(&dir_no_id),
            "folders need an id too"
        );

        assert!(id.still_matches(&id));
        assert!(id.still_matches(&ItemIdentity {
            modified: Some(1_002),
            ..id
        }));
        assert!(!id.still_matches(&ItemIdentity {
            modified: Some(1_010),
            ..id
        }));
        assert!(!id.still_matches(&ItemIdentity {
            logical_size: 101,
            ..id
        }));
        assert!(!id.still_matches(&ItemIdentity {
            file_id: Some(FileId {
                volume: 1,
                index: 2
            }),
            ..id
        }));
        assert!(!id.still_matches(&ItemIdentity {
            kind: NodeKind::Dir,
            ..id
        }));
        // Folders: contents may change, identity may not.
        let dir = ItemIdentity {
            kind: NodeKind::Dir,
            ..id
        };
        assert!(dir.still_matches(&ItemIdentity {
            logical_size: 9,
            modified: Some(99_999),
            ..dir
        }));
        // Old saved suggestions without the new fields still load.
        let old = r#"{"id":"a","group":"staleLarge","safety":"review","nodeId":1,"path":"p","name":"n","bytes":1,"reason":"r","howFound":"h","lastOpened":null,"keepPath":null,"cloud":null}"#;
        let s: SuggestionDto = serde_json::from_str(old).unwrap();
        assert!(s.identity.is_none());
    }

    #[test]
    fn old_settings_default_auto_update() {
        let mut v = serde_json::to_value(Settings::default()).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("autoUpdate");
        o.remove("findDuplicates");
        o.remove("skippedVersion");
        o.remove("lastUpdateCheck");
        o.remove("notesShownForVersion");
        let s: Settings = serde_json::from_value(v).unwrap();
        assert!(s.auto_update);
        assert!(s.find_duplicates);
        assert_eq!(s.skipped_version, None);
        assert_eq!(s.last_update_check, None);
        assert_eq!(s.notes_shown_for_version, None);
        assert_eq!(
            serde_json::to_string(&UpdateErrorKind::Offline).unwrap(),
            "\"offline\""
        );
    }

    #[test]
    fn cloud_summary_per_root() {
        use crate::fixtures::{Fixture, FixtureSpec};
        let f = Fixture::generate(FixtureSpec {
            target_nodes: 400,
            ..FixtureSpec::default()
        });
        let t = f.tree();
        let roots = vec![
            CloudRootInfo {
                provider: CloudProvider::OneDrive,
                display_name: "OneDrive - Personal".into(),
                root_path: t.path_of(f.id("cloudRoot")),
                remote: false,
                virtual_volume_id: None,
                cache_root: None,
            },
            CloudRootInfo {
                provider: CloudProvider::GoogleDrive,
                display_name: "Google Drive".into(),
                root_path: PathBuf::from("G:\\"),
                remote: true,
                virtual_volume_id: Some("G:".into()),
                cache_root: None,
            },
        ];
        let s = cloud_summary(&t, &roots);
        let n = |k: &str| t.get(f.id(k)).unwrap().totals;
        let od = &s[0];
        assert_eq!(od.display_name, "OneDrive - Personal");
        assert_eq!(od.pinned_bytes, n("pinned").allocated);
        assert!(od.local_bytes >= n("downloaded").allocated);
        assert_eq!(
            od.cloud_only_bytes,
            n("cloudOnly").cloud_only + n("cloudOnlyTwin").cloud_only
        );
        let gd = &s[1];
        assert_eq!(gd.local_bytes + gd.pinned_bytes + gd.cloud_only_bytes, 0);
        assert!(gd.remote);
        // The fixture's other five providers have no listed root: fallback entries.
        assert_eq!(s.len(), 2 + 5);
        assert!(s[2..].iter().all(|x| x.root_path.is_empty()));
        assert!(s[2..].iter().all(|x| x.provider != CloudProvider::OneDrive));
    }

    #[test]
    fn enum_strings() {
        let s = |v: &dyn erased::Ser| v.json();
        assert_eq!(s(&NodeKind::Dir), "\"dir\"");
        assert_eq!(s(&CloudProvider::ICloud), "\"iCloud\"");
        assert_eq!(s(&CloudProvider::GoogleDrive), "\"googleDrive\"");
        assert_eq!(s(&CloudState::CloudOnly), "\"cloudOnly\"");
        assert_eq!(s(&SkipReason::CloudOnlyFolder), "\"cloudOnlyFolder\"");
        assert_eq!(s(&ScanMode::Fast), "\"fast\"");
        assert_eq!(s(&Platform::Macos), "\"macos\"");
        assert_eq!(s(&SegmentKind::CloudOnly), "\"cloudOnly\"");
        assert_eq!(s(&SuggestionGroup::FreeUpCloud), "\"freeUpCloud\"");
        assert_eq!(s(&SuggestionGroup::DevCaches), "\"devCaches\"");
        assert_eq!(s(&MainView::BlockMap), "\"blockMap\"");
        assert_eq!(s(&SortKey::FileCount), "\"fileCount\"");
        assert_eq!(s(&DiffStatus::Shrank), "\"shrank\"");
        assert_eq!(s(&Destination::FreeUpCloud), r#"{"kind":"freeUpCloud"}"#);
        assert_eq!(
            s(&Destination::Move {
                target: "D:\\".into()
            }),
            r#"{"kind":"move","target":"D:\\"}"#
        );
    }

    mod erased {
        pub trait Ser {
            fn json(&self) -> String;
        }
        impl<T: serde::Serialize> Ser for T {
            fn json(&self) -> String {
                serde_json::to_string(self).unwrap()
            }
        }
    }

    #[test]
    fn round_trips() {
        let s = Settings::default();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let f: Filter = serde_json::from_str(r#"{"text":"mp4","categories":["video"]}"#).unwrap();
        assert_eq!(f.text.as_deref(), Some("mp4"));
        assert_eq!(f.categories, Some(vec![Category::Video]));
        assert!(f.cloud.is_none());
        let r: ApplyRequest = serde_json::from_str(
            r#"{"decisions":[{"suggestionId":"a","decision":"later"}],"destination":{"kind":"move","target":"E:\\Archive"}}"#,
        )
        .unwrap();
        assert_eq!(r.decisions[0].decision, Decision::Later);
    }

    #[test]
    fn conversions() {
        let t = Tree::from_records(sample());
        let n = node_dto(t.get(2).unwrap(), None);
        assert_eq!(n.allocated, 160);
        assert_eq!(n.file_count, 3);
        assert_eq!(n.dir_count, 1);
        assert_eq!(n.child_count, 3);
        assert_eq!(n.category, Category::Video);
        let mut d = Deltas::new();
        d.insert(2, -5);
        assert_eq!(node_dto(t.get(2).unwrap(), Some(&d)).delta, Some(-5));
        let det = node_detail(&t, 5, 10, None).unwrap();
        assert!((det.share_of_parent - 50.0 / 160.0).abs() < 1e-9);
        assert!(det.path.ends_with("c.pdf") || det.path.ends_with("Docs"));
        let det = node_detail(&t, 2, 2, None).unwrap();
        assert_eq!(det.largest_children.len(), 2);
        assert_eq!(det.largest_children[0].name, "a.mp4");
        assert_eq!(TreeDto::fold_id(0), -1);
        assert_eq!(TreeDto::fold_id(41), -42);
    }
}
