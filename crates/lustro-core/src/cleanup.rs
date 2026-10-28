//! Cleanup: review decisions -> apply plan -> execution through [`FileOps`] -> undo log.
//!
//! Nothing in this crate deletes, moves or changes files. The actual operations (move to
//! trash, move, compress, free up cloud space, restore) are done by a [`FileOps`]
//! implementation in `lustro-platform`, and only for items in an [`ApplyPlan`] built here from
//! the user's explicit decisions. Protected paths are supplied by the caller.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf, Prefix};

use serde::{Deserialize, Serialize};

use crate::dto::{
    ApplyRequest, ApplyResult, Decision, Destination, FailedItem, ItemIdentity, SuggestionDto,
    SuggestionGroup, UndoEntry,
};
use crate::model::{CloudInfo, CloudState, NodeId};
use crate::scan::CancelToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProtectScope {
    /// The path and everything inside it (C:\Windows, /System, /usr).
    Subtree,
    /// The path itself only; its contents may be cleaned (the home folder, Documents).
    ExactOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedRule {
    pub path: PathBuf,
    pub scope: ProtectScope,
}

/// Paths that cleanup must never touch. Removing an ancestor of a protected path is also
/// refused, since it would remove the protected path with it.
#[derive(Clone, Debug, Default)]
pub struct ProtectedPaths {
    rules: Vec<(Vec<String>, ProtectScope, PathBuf)>,
    case_insensitive: bool,
}

fn normalize(p: &Path, ci: bool) -> Vec<String> {
    let text = p.to_string_lossy();
    // Windows-style text (`C:\...`) is split by hand on other OSes, so fixtures and tests
    // that use Windows paths behave the same everywhere.
    if !cfg!(windows) && text.contains('\\') && !text.contains('/') {
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        let mut out: Vec<String> = Vec::new();
        for (i, part) in text.split('\\').enumerate() {
            match part {
                "" | "." => {}
                ".." => {
                    if out.last().is_some_and(|l| l != "/" && !l.ends_with(':')) {
                        out.pop();
                    }
                }
                _ => out.push(if ci {
                    part.to_lowercase()
                } else {
                    part.to_string()
                }),
            }
            if i == 0 && part.ends_with(':') {
                out.push("/".into());
            }
        }
        return out;
    }
    let mut out: Vec<String> = Vec::new();
    for c in p.components() {
        let s = match c {
            Component::CurDir => continue,
            Component::ParentDir => {
                if out.last().is_some_and(|l| l != "/") {
                    out.pop();
                }
                continue;
            }
            Component::RootDir => "/".to_string(),
            // Verbatim prefixes name the same place as plain ones.
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(d) | Prefix::VerbatimDisk(d) => format!("{}:", d as char),
                Prefix::UNC(a, b) | Prefix::VerbatimUNC(a, b) => {
                    format!(r"\\{}\{}", a.to_string_lossy(), b.to_string_lossy())
                }
                _ => p.as_os_str().to_string_lossy().into_owned(),
            },
            c => c.as_os_str().to_string_lossy().into_owned(),
        };
        out.push(if ci { s.to_lowercase() } else { s });
    }
    out
}

impl ProtectedPaths {
    pub fn new(rules: impl IntoIterator<Item = ProtectedRule>, case_insensitive: bool) -> Self {
        let mut p = Self {
            rules: Vec::new(),
            case_insensitive,
        };
        for r in rules {
            p.add(r);
        }
        p
    }

    /// Case-insensitive on Windows and macOS (the usual filesystem defaults).
    pub fn for_this_os(rules: impl IntoIterator<Item = ProtectedRule>) -> Self {
        Self::new(rules, cfg!(any(windows, target_os = "macos")))
    }

    pub fn add(&mut self, r: ProtectedRule) {
        let n = normalize(&r.path, self.case_insensitive);
        self.rules.push((n, r.scope, r.path));
    }

    /// Adds the user's own protected paths from Settings (whole subtrees).
    pub fn add_user_paths<'a>(&mut self, paths: impl IntoIterator<Item = &'a str>) {
        for p in paths {
            self.add(ProtectedRule {
                path: PathBuf::from(p),
                scope: ProtectScope::Subtree,
            });
        }
    }

    /// The protected path that blocks changing `path`, if any. Relative paths are blocked.
    pub fn blocking(&self, path: &Path) -> Option<&Path> {
        let target = normalize(path, self.case_insensitive);
        if target.len() < 2 {
            // Relative, empty or a bare root: never.
            return Some(Path::new("/"));
        }
        for (rule, scope, orig) in &self.rules {
            let is_ancestor_of_rule =
                rule.len() >= target.len() && rule[..target.len()] == target[..];
            let inside_rule = target.len() > rule.len() && target[..rule.len()] == rule[..];
            if is_ancestor_of_rule || (inside_rule && *scope == ProtectScope::Subtree) {
                return Some(orig);
            }
        }
        None
    }

    pub fn is_protected(&self, path: &Path) -> bool {
        self.blocking(path).is_some()
    }
}

/// What to do with one item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Trash,
    Move { target: PathBuf },
    Compress,
    FreeUpCloud,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanItem {
    pub suggestion_id: String,
    pub node_id: NodeId,
    pub path: PathBuf,
    pub bytes: u64,
    pub action: Action,
    pub cloud: Option<CloudInfo>,
    /// The item as scanned; [`apply`] checks it again first and skips it if it changed.
    pub identity: Option<ItemIdentity>,
    /// Duplicates: the copy that must still exist, unchanged, before this one is removed.
    pub keep: Option<(PathBuf, Option<ItemIdentity>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipCause {
    UnknownSuggestion,
    Protected(PathBuf),
    /// Already covered by a selected folder that contains it.
    InsideSelected(String),
    NotACloudFile,
    AlreadyCloudOnly,
    BadTarget,
    /// Removing it would remove the copy a selected duplicate keeps (the last copy).
    WouldRemoveKeptCopy(PathBuf),
}

impl SkipCause {
    pub fn message(&self) -> String {
        match self {
            SkipCause::UnknownSuggestion => "No longer in the suggestion list".into(),
            SkipCause::Protected(p) => format!("Protected location ({})", p.display()),
            SkipCause::InsideSelected(_) => "Included in a selected folder".into(),
            SkipCause::NotACloudFile => {
                "Not a cloud file, so space can't be freed to the cloud".into()
            }
            SkipCause::AlreadyCloudOnly => "Already only in the cloud".into(),
            SkipCause::BadTarget => "The destination folder is not valid for this item".into(),
            SkipCause::WouldRemoveKeptCopy(_) => {
                "Holds the copy kept for a duplicate you are removing, so it stays".into()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedItem {
    pub suggestion_id: String,
    pub path: String,
    pub cause: SkipCause,
}

/// The reviewed, checked list of operations. Built only from explicit "remove" decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyPlan {
    pub destination: Destination,
    pub items: Vec<PlanItem>,
    pub skipped: Vec<SkippedItem>,
}

impl ApplyPlan {
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }
}

fn is_within(child: &[String], parent: &[String]) -> bool {
    child.len() > parent.len() && child[..parent.len()] == parent[..]
}

/// Builds the plan. Only `Remove` decisions count. "Free up space" suggestions are always
/// freed to the cloud (the provider keeps the file); everything else goes to `destination`.
pub fn plan(
    suggestions: &[SuggestionDto],
    req: &ApplyRequest,
    protected: &ProtectedPaths,
) -> ApplyPlan {
    let ci = protected.case_insensitive;
    let by_id: HashMap<&str, &SuggestionDto> =
        suggestions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut skipped = Vec::new();
    let mut picked: Vec<(&SuggestionDto, Action)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for d in &req.decisions {
        if d.decision != Decision::Remove || !seen.insert(d.suggestion_id.as_str()) {
            continue;
        }
        let Some(&s) = by_id.get(d.suggestion_id.as_str()) else {
            skipped.push(SkippedItem {
                suggestion_id: d.suggestion_id.clone(),
                path: String::new(),
                cause: SkipCause::UnknownSuggestion,
            });
            continue;
        };
        let skip = |cause| SkippedItem {
            suggestion_id: s.id.clone(),
            path: s.path.clone(),
            cause,
        };
        let path = PathBuf::from(&s.path);
        if let Some(p) = protected.blocking(&path) {
            skipped.push(skip(SkipCause::Protected(p.to_path_buf())));
            continue;
        }
        let wants_cloud =
            s.group == SuggestionGroup::FreeUpCloud || req.destination == Destination::FreeUpCloud;
        let action = if wants_cloud {
            match s.cloud.map(|c| c.state) {
                None => {
                    skipped.push(skip(SkipCause::NotACloudFile));
                    continue;
                }
                Some(CloudState::CloudOnly) => {
                    skipped.push(skip(SkipCause::AlreadyCloudOnly));
                    continue;
                }
                Some(_) => Action::FreeUpCloud,
            }
        } else {
            match &req.destination {
                Destination::Trash => Action::Trash,
                Destination::Compress => Action::Compress,
                Destination::FreeUpCloud => unreachable!("handled above"),
                Destination::Move { target } => {
                    let t = PathBuf::from(target);
                    let tn = normalize(&t, ci);
                    let sn = normalize(&path, ci);
                    if tn.len() < 2 || tn == sn || is_within(&tn, &sn) {
                        skipped.push(skip(SkipCause::BadTarget));
                        continue;
                    }
                    Action::Move { target: t }
                }
            }
        };
        picked.push((s, action));
    }

    // Never remove the last copy: the copy a selected duplicate keeps, or a folder holding
    // it, stays. (Checked before nesting, so a folder can't hide the keeper.)
    let keepers: Vec<(Vec<String>, &str)> = picked
        .iter()
        .filter(|(s, _)| s.group == SuggestionGroup::Duplicates)
        .filter_map(|(s, _)| s.keep_path.as_deref())
        .map(|k| (normalize(Path::new(k), ci), k))
        .collect();
    picked.retain(|(s, _)| {
        let me = normalize(Path::new(&s.path), ci);
        match keepers.iter().find(|(k, _)| *k == me || is_within(k, &me)) {
            Some((_, k)) => {
                skipped.push(SkippedItem {
                    suggestion_id: s.id.clone(),
                    path: s.path.clone(),
                    cause: SkipCause::WouldRemoveKeptCopy(PathBuf::from(k)),
                });
                false
            }
            None => true,
        }
    });

    // Drop items inside another selected item (shortest paths first).
    let mut norm: Vec<(Vec<String>, &SuggestionDto, Action)> = picked
        .into_iter()
        .map(|(s, a)| (normalize(Path::new(&s.path), ci), s, a))
        .collect();
    norm.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    let mut kept: Vec<(Vec<String>, &SuggestionDto, Action)> = Vec::new();
    for (n, s, a) in norm {
        if let Some((_, outer, _)) = kept.iter().find(|(k, _, _)| is_within(&n, k) || *k == n) {
            skipped.push(SkippedItem {
                suggestion_id: s.id.clone(),
                path: s.path.clone(),
                cause: SkipCause::InsideSelected(outer.id.clone()),
            });
            continue;
        }
        kept.push((n, s, a));
    }

    // Keep the user's review order.
    let order: HashMap<&str, usize> = req
        .decisions
        .iter()
        .enumerate()
        .map(|(i, d)| (d.suggestion_id.as_str(), i))
        .collect();
    let mut items: Vec<PlanItem> = kept
        .into_iter()
        .map(|(_, s, action)| PlanItem {
            suggestion_id: s.id.clone(),
            node_id: s.node_id,
            path: PathBuf::from(&s.path),
            bytes: s.bytes,
            action,
            cloud: s.cloud,
            identity: s.identity,
            keep: s
                .keep_path
                .as_ref()
                .filter(|_| s.group == SuggestionGroup::Duplicates)
                .map(|k| (PathBuf::from(k), s.keep_identity)),
        })
        .collect();
    items.sort_by_key(|i| {
        order
            .get(i.suggestion_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });

    ApplyPlan {
        destination: req.destination.clone(),
        items,
        skipped,
    }
}

/// Result of one file operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Where the item went (trash location, moved path, archive), if known.
    pub location: Option<PathBuf>,
    /// Opaque data the platform needs to restore it (e.g. a trash item id).
    pub token: Option<String>,
    pub undoable: bool,
    /// Bytes actually freed on this disk, if different from the item's size
    /// (compressing frees less).
    pub freed_bytes: Option<u64>,
}

/// File operations, implemented by `lustro-platform`. Only [`apply`] and [`UndoLog::undo`] call
/// them, and only for items of a plan. Implementations should still refuse protected paths
/// (defence in depth) and return `io::ErrorKind::Unsupported` for operations the platform
/// can't do (e.g. `free_up_cloud` on a provider without placeholders).
pub trait FileOps: Send + Sync {
    /// The item at `path` as it is now, or `Ok(None)` if nothing is there. Must not follow
    /// links or read file data. `file_id` must be built like the scanner's (Windows: volume
    /// serial + file index; Unix: dev + inode) and `modified` in Unix seconds.
    /// The default refuses, so [`apply`] skips every item until a platform implements it.
    fn stat_identity(&self, path: &Path) -> io::Result<Option<ItemIdentity>> {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform can't check items before cleanup yet",
        ))
    }
    fn trash(&self, path: &Path) -> io::Result<Outcome>;
    /// Trashes `path` only if it is still `expected`. [`apply`] uses this for trash
    /// actions. The default checks with [`stat_identity`](Self::stat_identity) and then
    /// calls [`trash`](Self::trash); a platform should override it to compare on the same
    /// handle it deletes through, which closes the gap between the check and the delete.
    fn trash_checked(&self, path: &Path, expected: &ItemIdentity) -> io::Result<Outcome> {
        match self.stat_identity(path)? {
            Some(now) if expected.still_matches(&now) => self.trash(path),
            Some(_) => Err(io::Error::other("changed since review")),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }
    fn move_to(&self, path: &Path, target_dir: &Path) -> io::Result<Outcome>;
    fn compress(&self, path: &Path) -> io::Result<Outcome>;
    /// Returns a downloaded cloud file to cloud-only. Must not delete cloud data.
    fn free_up_cloud(&self, path: &Path) -> io::Result<Outcome>;
    /// Puts an item back where it was.
    fn restore(&self, item: &UndoItem) -> io::Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoItem {
    pub original: PathBuf,
    pub location: Option<PathBuf>,
    pub token: Option<String>,
    pub bytes: u64,
    pub action: Action,
    pub undoable: bool,
    #[serde(default)]
    pub restored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoRecord {
    pub id: String,
    pub at: i64,
    pub destination: Destination,
    pub items: Vec<UndoItem>,
}

impl UndoRecord {
    pub fn bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }

    /// True while at least one item can still be restored.
    pub fn undoable(&self) -> bool {
        self.items.iter().any(|i| i.undoable && !i.restored)
    }

    pub fn entry(&self) -> UndoEntry {
        UndoEntry {
            id: self.id.clone(),
            at: self.at,
            items: u32::try_from(self.items.len()).unwrap_or(u32::MAX),
            bytes: self.bytes(),
            destination: self.destination.clone(),
            undoable: self.undoable(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    #[error("no such cleanup in the undo log")]
    NotFound,
    #[error("this cleanup can't be undone")]
    NotUndoable,
}

/// The undo log. Serializable so the app can store it next to the settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoLog {
    records: Vec<UndoRecord>,
    seq: u64,
    #[serde(default = "default_keep")]
    keep: usize,
}

fn default_keep() -> usize {
    100
}

impl UndoLog {
    pub fn new() -> Self {
        Self {
            keep: default_keep(),
            ..Self::default()
        }
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Newest first.
    pub fn entries(&self) -> Vec<UndoEntry> {
        self.records.iter().rev().map(UndoRecord::entry).collect()
    }

    pub fn get(&self, id: &str) -> Option<&UndoRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    fn push(&mut self, at: i64, destination: Destination, items: Vec<UndoItem>) -> String {
        self.seq += 1;
        let id = format!("u{at}-{}", self.seq);
        self.records.push(UndoRecord {
            id: id.clone(),
            at,
            destination,
            items,
        });
        let keep = if self.keep == 0 {
            default_keep()
        } else {
            self.keep
        };
        if self.records.len() > keep {
            let extra = self.records.len() - keep;
            self.records.drain(..extra);
        }
        id
    }

    /// Restores every restorable item of record `id`. Returns the items that failed.
    pub fn undo(&mut self, id: &str, ops: &dyn FileOps) -> Result<Vec<FailedItem>, UndoError> {
        let rec = self
            .records
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(UndoError::NotFound)?;
        if !rec.undoable() {
            return Err(UndoError::NotUndoable);
        }
        let mut failed = Vec::new();
        for item in rec.items.iter_mut().filter(|i| i.undoable && !i.restored) {
            match ops.restore(item) {
                Ok(()) => item.restored = true,
                Err(e) => failed.push(FailedItem {
                    path: item.original.to_string_lossy().into_owned(),
                    message: e.to_string(),
                }),
            }
        }
        Ok(failed)
    }
}

/// Checks that the item (and, for a duplicate, its kept copy) is still what was scanned.
fn verify(item: &PlanItem, ops: &dyn FileOps) -> Result<(), String> {
    let check = |path: &Path, expected: Option<ItemIdentity>, what: &str| {
        let Some(expected) = expected else {
            return Err(format!("Scan again first: no scan details for {what}"));
        };
        match ops.stat_identity(path) {
            Ok(Some(now)) if expected.still_matches(&now) => Ok(()),
            Ok(Some(_)) => Err(format!("Changed since review ({what})")),
            Ok(None) => Err(format!("No longer there ({what})")),
            Err(e) => Err(format!("Could not check {what}: {e}")),
        }
    };
    check(&item.path, item.identity, "this item")?;
    if let Some((keep, keep_id)) = &item.keep {
        check(keep, *keep_id, "the copy that is kept")?;
    }
    Ok(())
}

/// Runs the plan through `ops`, stopping early if cancelled, and records it in `log`.
/// Every item is checked first with [`FileOps::stat_identity`]: an item that is gone or
/// changed since the scan, or a duplicate whose kept copy is gone or changed, is skipped and
/// reported in `failed`, never touched.
pub fn apply(
    plan: &ApplyPlan,
    ops: &dyn FileOps,
    log: &mut UndoLog,
    now: i64,
    cancel: &CancelToken,
) -> ApplyResult {
    let mut failed: Vec<FailedItem> = plan
        .skipped
        .iter()
        .filter(|s| !matches!(s.cause, SkipCause::InsideSelected(_)))
        .map(|s| FailedItem {
            path: s.path.clone(),
            message: s.cause.message(),
        })
        .collect();
    let mut done = Vec::new();
    let mut freed = 0u64;
    for item in &plan.items {
        if cancel.is_cancelled() {
            break;
        }
        if let Err(message) = verify(item, ops) {
            failed.push(FailedItem {
                path: item.path.to_string_lossy().into_owned(),
                message,
            });
            continue;
        }
        let res = match &item.action {
            // `verify` guarantees an identity here.
            Action::Trash => match &item.identity {
                Some(id) => ops.trash_checked(&item.path, id),
                None => Err(io::Error::other("no scan details")),
            },
            Action::Move { target } => ops.move_to(&item.path, target),
            Action::Compress => ops.compress(&item.path),
            Action::FreeUpCloud => ops.free_up_cloud(&item.path),
        };
        match res {
            Ok(o) => {
                let bytes = o.freed_bytes.unwrap_or(item.bytes);
                freed += bytes;
                done.push(UndoItem {
                    original: item.path.clone(),
                    location: o.location,
                    token: o.token,
                    bytes,
                    action: item.action.clone(),
                    undoable: o.undoable,
                    restored: false,
                });
            }
            Err(e) => failed.push(FailedItem {
                path: item.path.to_string_lossy().into_owned(),
                message: e.to_string(),
            }),
        }
    }
    let applied = u32::try_from(done.len()).unwrap_or(u32::MAX);
    let undo_id = log.push(now, plan.destination.clone(), done);
    ApplyResult {
        applied,
        freed_bytes: freed,
        failed,
        undo_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{DecisionItem, Safety};
    use crate::model::CloudProvider;
    use std::sync::Mutex;

    fn sug(id: &str, path: &str, group: SuggestionGroup, bytes: u64) -> SuggestionDto {
        SuggestionDto {
            id: id.into(),
            group,
            safety: Safety::Safe,
            node_id: 1,
            path: path.into(),
            name: path.rsplit(['\\', '/']).next().unwrap().into(),
            bytes,
            reason: String::new(),
            how_found: String::new(),
            last_opened: None,
            keep_path: None,
            cloud: None,
            identity: Some(ident(path)),
            keep_identity: None,
        }
    }

    /// The identity FakeOps reports for `path` (stable per path).
    fn ident(path: &str) -> ItemIdentity {
        ItemIdentity {
            kind: crate::model::NodeKind::File,
            file_id: Some(crate::model::FileId {
                volume: 1,
                index: path.len() as u64,
            }),
            logical_size: 7,
            modified: Some(100),
        }
    }

    fn req(ids: &[(&str, Decision)], destination: Destination) -> ApplyRequest {
        ApplyRequest {
            decisions: ids
                .iter()
                .map(|(i, d)| DecisionItem {
                    suggestion_id: i.to_string(),
                    decision: *d,
                })
                .collect(),
            destination,
        }
    }

    fn protected() -> ProtectedPaths {
        ProtectedPaths::new(
            [
                ProtectedRule {
                    path: "C:\\Windows".into(),
                    scope: ProtectScope::Subtree,
                },
                ProtectedRule {
                    path: "C:\\Users\\William".into(),
                    scope: ProtectScope::ExactOnly,
                },
                ProtectedRule {
                    path: "/usr".into(),
                    scope: ProtectScope::Subtree,
                },
            ],
            true,
        )
    }

    #[test]
    fn protected_paths() {
        let p = protected();
        assert!(p.is_protected(Path::new("C:\\Windows")));
        assert!(p.is_protected(Path::new("c:\\windows\\System32\\x.dll")));
        assert!(p.is_protected(Path::new("C:\\Users\\William")));
        assert!(
            p.is_protected(Path::new("C:\\Users")),
            "ancestor of protected"
        );
        assert!(p.is_protected(Path::new("C:\\")));
        assert!(!p.is_protected(Path::new("C:\\Users\\William\\Downloads\\a.exe")));
        assert!(!p.is_protected(Path::new("C:\\Windowsill\\x")));
        assert!(p.is_protected(Path::new("/usr/lib/x")));
        assert!(!p.is_protected(Path::new("/home/william/x")));
        assert!(p.is_protected(Path::new("relative")));
        assert!(p.is_protected(Path::new("")));
        assert!(p.is_protected(Path::new("C:\\Users\\William\\..\\William")));
        let cs = ProtectedPaths::new(
            [ProtectedRule {
                path: "/home/william/Keep".into(),
                scope: ProtectScope::Subtree,
            }],
            false,
        );
        assert!(!cs.is_protected(Path::new("/home/william/keep/x")));
        assert!(cs.is_protected(Path::new("/home/william/Keep/x")));
        let mut user = ProtectedPaths::default();
        user.add_user_paths(["D:\\Photos"]);
        assert!(user.is_protected(Path::new("D:\\Photos\\2024")));
    }

    #[test]
    fn plan_rules() {
        let mut cloud = sug(
            "freeUpCloud:5",
            "C:\\Users\\William\\OneDrive\\trip.mp4",
            SuggestionGroup::FreeUpCloud,
            200,
        );
        cloud.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::Downloaded,
        });
        let s = vec![
            sug(
                "devCaches:1",
                "C:\\Users\\William\\proj\\node_modules",
                SuggestionGroup::DevCaches,
                100,
            ),
            sug(
                "x:2",
                "C:\\Users\\William\\proj\\node_modules\\pkg",
                SuggestionGroup::DevCaches,
                10,
            ),
            sug(
                "tempFiles:3",
                "C:\\Windows\\Temp",
                SuggestionGroup::TempFiles,
                50,
            ),
            sug(
                "staleLarge:4",
                "C:\\Users\\William\\old.mkv",
                SuggestionGroup::StaleLarge,
                70,
            ),
            cloud,
            sug(
                "keep:6",
                "C:\\Users\\William\\keep.bin",
                SuggestionGroup::StaleLarge,
                1,
            ),
        ];
        let r = req(
            &[
                ("staleLarge:4", Decision::Remove),
                ("x:2", Decision::Remove),
                ("devCaches:1", Decision::Remove),
                ("tempFiles:3", Decision::Remove),
                ("freeUpCloud:5", Decision::Remove),
                ("keep:6", Decision::Keep),
                ("gone:9", Decision::Remove),
            ],
            Destination::Trash,
        );
        let p = plan(&s, &r, &protected());
        let ids: Vec<_> = p.items.iter().map(|i| i.suggestion_id.as_str()).collect();
        assert_eq!(ids, ["staleLarge:4", "devCaches:1", "freeUpCloud:5"]);
        assert_eq!(p.items[0].action, Action::Trash);
        assert_eq!(p.items[2].action, Action::FreeUpCloud);
        assert_eq!(p.total_bytes(), 370);
        let causes: HashMap<_, _> = p
            .skipped
            .iter()
            .map(|s| (s.suggestion_id.as_str(), &s.cause))
            .collect();
        assert_eq!(
            causes["x:2"],
            &SkipCause::InsideSelected("devCaches:1".into())
        );
        assert!(matches!(causes["tempFiles:3"], SkipCause::Protected(_)));
        assert_eq!(causes["gone:9"], &SkipCause::UnknownSuggestion);
        assert!(!causes.contains_key("keep:6"));

        // Free-up destination on a non-cloud file, and move-target checks.
        let r2 = req(
            &[("staleLarge:4", Decision::Remove)],
            Destination::FreeUpCloud,
        );
        assert_eq!(
            plan(&s, &r2, &protected()).skipped[0].cause,
            SkipCause::NotACloudFile
        );
        let into_itself = req(
            &[("devCaches:1", Decision::Remove)],
            Destination::Move {
                target: "C:\\Users\\William\\proj\\node_modules\\sub".into(),
            },
        );
        assert_eq!(
            plan(&s, &into_itself, &protected()).skipped[0].cause,
            SkipCause::BadTarget
        );
        let mv = req(
            &[("devCaches:1", Decision::Remove)],
            Destination::Move {
                target: "D:\\Archive".into(),
            },
        );
        assert_eq!(
            plan(&s, &mv, &protected()).items[0].action,
            Action::Move {
                target: "D:\\Archive".into()
            }
        );
    }

    /// Records calls; fails for paths containing "fail".
    #[derive(Default)]
    struct FakeOps {
        calls: Mutex<Vec<String>>,
    }

    impl FakeOps {
        fn op(&self, what: &str, p: &Path, undoable: bool) -> io::Result<Outcome> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{what} {}", p.display()));
            if p.to_string_lossy().contains("fail") {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "in use"));
            }
            Ok(Outcome {
                location: Some(PathBuf::from("trash").join(p.file_name().unwrap())),
                token: Some("t".into()),
                undoable,
                freed_bytes: None,
            })
        }
    }

    impl FileOps for FakeOps {
        /// "gone" paths don't exist; "changed" paths have a new size.
        fn stat_identity(&self, p: &Path) -> io::Result<Option<ItemIdentity>> {
            let text = p.to_string_lossy();
            if text.contains("gone") {
                return Ok(None);
            }
            let mut id = ident(&text);
            if text.contains("changed") {
                id.logical_size += 1;
            }
            Ok(Some(id))
        }
        fn trash(&self, p: &Path) -> io::Result<Outcome> {
            self.op("trash", p, true)
        }
        fn move_to(&self, p: &Path, _: &Path) -> io::Result<Outcome> {
            self.op("move", p, true)
        }
        fn compress(&self, p: &Path) -> io::Result<Outcome> {
            self.op("compress", p, true)
        }
        fn free_up_cloud(&self, p: &Path) -> io::Result<Outcome> {
            self.op("free", p, false)
        }
        fn restore(&self, item: &UndoItem) -> io::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("restore {}", item.original.display()));
            Ok(())
        }
    }

    fn dup(id: &str, path: &str, keep: &str) -> SuggestionDto {
        let mut s = sug(id, path, SuggestionGroup::Duplicates, 5);
        s.keep_path = Some(keep.into());
        s.keep_identity = Some(ident(keep));
        s
    }

    #[test]
    fn never_removes_the_kept_copy() {
        let s = vec![
            dup(
                "duplicates:1",
                "C:\\Users\\William\\Documents\\thesis.pdf",
                "C:\\Users\\William\\Downloads\\thesis.pdf",
            ),
            sug(
                "oldDownloads:2",
                "C:\\Users\\William\\Downloads\\thesis.pdf",
                SuggestionGroup::OldDownloads,
                5,
            ),
            sug(
                "tempFiles:3",
                "C:\\Users\\William\\Downloads",
                SuggestionGroup::TempFiles,
                50,
            ),
            sug(
                "staleLarge:4",
                "C:\\Users\\William\\Videos\\old.mkv",
                SuggestionGroup::StaleLarge,
                70,
            ),
        ];
        let all = [
            ("duplicates:1", Decision::Remove),
            ("oldDownloads:2", Decision::Remove),
            ("tempFiles:3", Decision::Remove),
            ("staleLarge:4", Decision::Remove),
        ];
        let p = plan(&s, &req(&all, Destination::Trash), &protected());
        let ids: Vec<_> = p.items.iter().map(|i| i.suggestion_id.as_str()).collect();
        assert_eq!(ids, ["duplicates:1", "staleLarge:4"]);
        let blocked: Vec<_> = p
            .skipped
            .iter()
            .filter(|x| matches!(x.cause, SkipCause::WouldRemoveKeptCopy(_)))
            .map(|x| x.suggestion_id.as_str())
            .collect();
        assert_eq!(blocked, ["oldDownloads:2", "tempFiles:3"]);
        // Without the duplicate selected, the keeper may go.
        let p = plan(&s, &req(&all[1..], Destination::Trash), &protected());
        assert!(p
            .skipped
            .iter()
            .all(|x| !matches!(x.cause, SkipCause::WouldRemoveKeptCopy(_))));
        // The duplicate item carries its keeper for the apply-time check.
        let p = plan(&s, &req(&all[..1], Destination::Trash), &protected());
        assert_eq!(
            p.items[0].keep.as_ref().map(|k| k.0.clone()),
            Some(PathBuf::from("C:\\Users\\William\\Downloads\\thesis.pdf"))
        );
    }

    #[test]
    fn apply_checks_items_first() {
        let mut no_id = sug(
            "old",
            "C:\\Users\\William\\old.bin",
            SuggestionGroup::StaleLarge,
            1,
        );
        no_id.identity = None;
        let s = vec![
            sug(
                "ok",
                "C:\\Users\\William\\ok.bin",
                SuggestionGroup::StaleLarge,
                1,
            ),
            sug(
                "gone",
                "C:\\Users\\William\\gone.bin",
                SuggestionGroup::StaleLarge,
                1,
            ),
            sug(
                "changed",
                "C:\\Users\\William\\changed.bin",
                SuggestionGroup::StaleLarge,
                1,
            ),
            no_id,
            dup(
                "dupKeepGone",
                "C:\\Users\\William\\a.bin",
                "C:\\Users\\William\\gone-keeper.bin",
            ),
            dup(
                "dupKeepChanged",
                "C:\\Users\\William\\b.bin",
                "C:\\Users\\William\\changed-keeper.bin",
            ),
            dup(
                "dupOk",
                "C:\\Users\\William\\c.bin",
                "C:\\Users\\William\\keeper.bin",
            ),
        ];
        let ids = [
            "ok",
            "gone",
            "changed",
            "old",
            "dupKeepGone",
            "dupKeepChanged",
            "dupOk",
        ];
        let r = req(
            &ids.iter()
                .map(|i| (*i, Decision::Remove))
                .collect::<Vec<_>>(),
            Destination::Trash,
        );
        let p = plan(&s, &r, &protected());
        assert_eq!(p.items.len(), 7);
        let ops = FakeOps::default();
        let mut log = UndoLog::new();
        let res = apply(&p, &ops, &mut log, 1, &CancelToken::new());
        assert_eq!(res.applied, 2);
        let trashed: Vec<_> = ops
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("trash"))
            .cloned()
            .collect();
        assert_eq!(trashed.len(), 2);
        assert!(trashed[0].ends_with("ok.bin"));
        assert!(trashed[1].ends_with("c.bin"));
        assert_eq!(res.failed.len(), 5);
        let msgs: Vec<_> = res.failed.iter().map(|f| f.message.as_str()).collect();
        assert!(msgs.contains(&"No longer there (this item)"));
        assert!(msgs.contains(&"Changed since review (this item)"));
        assert!(msgs.contains(&"Scan again first: no scan details for this item"));
        assert!(msgs.contains(&"No longer there (the copy that is kept)"));
        assert!(msgs.contains(&"Changed since review (the copy that is kept)"));
    }

    /// Implements only the required methods: `stat_identity` keeps its default.
    struct NoStat;

    impl FileOps for NoStat {
        fn trash(&self, _: &Path) -> io::Result<Outcome> {
            panic!("must not be called")
        }
        fn move_to(&self, _: &Path, _: &Path) -> io::Result<Outcome> {
            panic!("must not be called")
        }
        fn compress(&self, _: &Path) -> io::Result<Outcome> {
            panic!("must not be called")
        }
        fn free_up_cloud(&self, _: &Path) -> io::Result<Outcome> {
            panic!("must not be called")
        }
        fn restore(&self, _: &UndoItem) -> io::Result<()> {
            Ok(())
        }
    }

    /// Matches on the first stat, then the file is replaced.
    #[derive(Default)]
    struct Racy {
        stats: Mutex<u32>,
        trashed: Mutex<u32>,
    }

    impl FileOps for Racy {
        fn stat_identity(&self, p: &Path) -> io::Result<Option<ItemIdentity>> {
            let mut n = self.stats.lock().unwrap();
            *n += 1;
            let mut id = ident(&p.to_string_lossy());
            if *n > 1 {
                id.file_id = None;
                id.logical_size += 1;
            }
            Ok(Some(id))
        }
        fn trash(&self, _: &Path) -> io::Result<Outcome> {
            *self.trashed.lock().unwrap() += 1;
            Ok(Outcome::default())
        }
        fn move_to(&self, _: &Path, _: &Path) -> io::Result<Outcome> {
            unreachable!()
        }
        fn compress(&self, _: &Path) -> io::Result<Outcome> {
            unreachable!()
        }
        fn free_up_cloud(&self, _: &Path) -> io::Result<Outcome> {
            unreachable!()
        }
        fn restore(&self, _: &UndoItem) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn trash_rechecks_right_before_deleting() {
        let s = vec![sug(
            "a",
            "C:\\Users\\William\\a.bin",
            SuggestionGroup::StaleLarge,
            1,
        )];
        let p = plan(
            &s,
            &req(&[("a", Decision::Remove)], Destination::Trash),
            &protected(),
        );
        let ops = Racy::default();
        let res = apply(&p, &ops, &mut UndoLog::new(), 1, &CancelToken::new());
        assert_eq!(res.applied, 0);
        assert_eq!(*ops.trashed.lock().unwrap(), 0);
        assert!(res.failed[0].message.contains("changed since review"));
    }

    #[test]
    fn default_stat_identity_refuses_everything() {
        let s = vec![sug(
            "a",
            "C:\\Users\\William\\a.bin",
            SuggestionGroup::StaleLarge,
            1,
        )];
        let p = plan(
            &s,
            &req(&[("a", Decision::Remove)], Destination::Trash),
            &protected(),
        );
        let res = apply(&p, &NoStat, &mut UndoLog::new(), 1, &CancelToken::new());
        assert_eq!(res.applied, 0);
        assert!(res.failed[0].message.starts_with("Could not check"));
    }

    #[test]
    fn verbatim_paths_are_protected_too() {
        let p = protected();
        assert!(p.is_protected(Path::new(r"\\?\C:\Windows\System32")));
        assert!(p.is_protected(Path::new(r"\\?\c:\users")));
        assert!(!p.is_protected(Path::new(r"\\?\C:\Users\William\Downloads\a.exe")));
    }

    #[test]
    fn apply_and_undo() {
        let s = vec![
            sug(
                "a",
                "C:\\Users\\William\\a.bin",
                SuggestionGroup::StaleLarge,
                10,
            ),
            sug(
                "b",
                "C:\\Users\\William\\fail.bin",
                SuggestionGroup::StaleLarge,
                20,
            ),
            sug("c", "C:\\Windows\\c.bin", SuggestionGroup::StaleLarge, 30),
        ];
        let r = req(
            &[
                ("a", Decision::Remove),
                ("b", Decision::Remove),
                ("c", Decision::Remove),
            ],
            Destination::Trash,
        );
        let p = plan(&s, &r, &protected());
        let ops = FakeOps::default();
        let mut log = UndoLog::new();
        let res = apply(&p, &ops, &mut log, 1_000, &CancelToken::new());
        assert_eq!(res.applied, 1);
        assert_eq!(res.freed_bytes, 10);
        assert_eq!(res.failed.len(), 2, "{:?}", res.failed);
        assert_eq!(
            ops.calls.lock().unwrap().len(),
            2,
            "protected item never reached ops"
        );
        let e = log.entries();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, res.undo_id);
        assert!(e[0].undoable);
        assert_eq!(e[0].bytes, 10);

        // Persist and reload.
        let mut log = UndoLog::from_json(&log.to_json().unwrap()).unwrap();
        assert!(log.undo(&res.undo_id, &ops).unwrap().is_empty());
        assert!(ops
            .calls
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .starts_with("restore"));
        assert!(!log.entries()[0].undoable);
        assert!(matches!(
            log.undo(&res.undo_id, &ops),
            Err(UndoError::NotUndoable)
        ));
        assert!(matches!(log.undo("nope", &ops), Err(UndoError::NotFound)));

        // Cancelled before start: nothing runs.
        let c = CancelToken::new();
        c.cancel();
        let before = ops.calls.lock().unwrap().len();
        let res = apply(&p, &ops, &mut log, 2_000, &c);
        assert_eq!(res.applied, 0);
        assert_eq!(ops.calls.lock().unwrap().len(), before);
    }
}
