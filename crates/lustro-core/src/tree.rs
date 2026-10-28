//! Arena tree of scanned entries, and `TreeBuilder`, the `ScanSink` that builds it live.
//!
//! # How totals work
//! Every node keeps the totals of its *attached* subtree. A record is attached as soon as
//! its parent is in the tree; its contribution is then added to every attached ancestor.
//! Records whose parent has not arrived yet (the MFT path sends children first) wait in an
//! orphan buffer keyed by the missing parent id. When that parent arrives, it adopts them
//! and sums their totals before it is itself attached, so the end result does not depend on
//! arrival order.
//!
//! Hard links count once: the first file seen with a given `FileId` (and `link_count > 1`)
//! contributes its bytes; later links contribute zero bytes but still count as files.
//!
//! # Threading
//! `TreeBuilder` wraps the tree in a `Mutex`. The scanner thread locks it once per batch
//! (inserting 10,000 records takes a few milliseconds), and readers lock it through
//! [`TreeBuilder::read`] for short queries. This keeps the design simple and the critical
//! sections short; no channel or builder thread is needed.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::category::{self, Category, PathContext, CATEGORY_COUNT};
use crate::model::{CloudInfo, CloudState, EntryMeta, FileId, NodeId, NodeKind, ScanRecord};
use crate::scan::{ScanProgress, ScanSink, ScanWarning};

const NONE: u32 = u32::MAX;

/// Totals of a node's subtree. For a file: its own sizes and `files = 1`.
/// For a directory: the sum over its descendants (`dirs` does not include itself).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Bytes on this disk, hard links counted once.
    pub allocated: u64,
    pub logical: u64,
    /// Logical bytes of cloud-only files.
    pub cloud_only: u64,
    pub files: u64,
    pub dirs: u64,
}

impl Totals {
    fn add(&mut self, o: &Totals) {
        self.allocated += o.allocated;
        self.logical += o.logical;
        self.cloud_only += o.cloud_only;
        self.files += o.files;
        self.dirs += o.dirs;
    }

    fn sub(&mut self, o: &Totals) {
        self.allocated = self.allocated.saturating_sub(o.allocated);
        self.logical = self.logical.saturating_sub(o.logical);
        self.cloud_only = self.cloud_only.saturating_sub(o.cloud_only);
        self.files = self.files.saturating_sub(o.files);
        self.dirs = self.dirs.saturating_sub(o.dirs);
    }
}

type Mix = [u64; CATEGORY_COUNT];

/// One entry in the tree.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub meta: EntryMeta,
    pub totals: Totals,
    children: Vec<NodeId>,
    /// Files: own category. Directories: unused (see [`Node::category`]).
    file_category: Category,
    /// Directories: allocated bytes per category in the subtree.
    mix: Option<Box<Mix>>,
    attached: bool,
    /// False for the second and later links of a hard-linked file.
    counted: bool,
    dirty: bool,
}

impl Node {
    pub fn name(&self) -> &OsStr {
        &self.meta.name
    }

    pub fn kind(&self) -> NodeKind {
        self.meta.kind
    }

    pub fn is_dir(&self) -> bool {
        self.meta.kind == NodeKind::Dir
    }

    /// Direct children, in arrival order.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Files: own category. Directories: the category with the most allocated bytes
    /// (`Other` when empty).
    pub fn category(&self) -> Category {
        match &self.mix {
            None => self.file_category,
            Some(mix) => dominant(mix),
        }
    }

    /// Allocated bytes per category (directories), or the file's own bytes in its category.
    pub fn category_mix(&self) -> Vec<(Category, u64)> {
        match &self.mix {
            Some(mix) => Category::ALL
                .iter()
                .map(|&c| (c, mix[c.index()]))
                .filter(|&(_, b)| b > 0)
                .collect(),
            None if self.totals.allocated > 0 => {
                vec![(self.file_category, self.totals.allocated)]
            }
            None => Vec::new(),
        }
    }

    /// False for extra links of a hard-linked file (counted with zero bytes).
    pub fn is_counted(&self) -> bool {
        self.counted
    }

    pub fn is_cloud_only(&self) -> bool {
        matches!(self.meta.cloud, Some(c) if c.state == CloudState::CloudOnly)
    }
}

fn dominant(mix: &Mix) -> Category {
    let mut best = Category::Other;
    let mut best_bytes = 0;
    for c in Category::ALL {
        if mix[c.index()] > best_bytes {
            best = c;
            best_bytes = mix[c.index()];
        }
    }
    best
}

/// What a node adds to its ancestors.
struct Contribution {
    totals: Totals,
    mix: MixDelta,
}

enum MixDelta {
    None,
    Single(Category, u64),
    Full(Mix),
}

fn add_mix(target: &mut Mix, delta: &MixDelta) {
    match delta {
        MixDelta::None => {}
        MixDelta::Single(c, b) => target[c.index()] += b,
        MixDelta::Full(m) => {
            for (t, v) in target.iter_mut().zip(m.iter()) {
                *t += v;
            }
        }
    }
}

fn sub_mix(target: &mut Mix, delta: &MixDelta) {
    match delta {
        MixDelta::None => {}
        MixDelta::Single(c, b) => target[c.index()] = target[c.index()].saturating_sub(*b),
        MixDelta::Full(m) => {
            for (t, v) in target.iter_mut().zip(m.iter()) {
                *t = t.saturating_sub(*v);
            }
        }
    }
}

/// The scanned tree. Node ids are the scanner's ids; the lookup table is indexed by id,
/// so ids should be reasonably dense (scanners assign them sequentially, MFT record
/// numbers are fine too).
#[derive(Debug, Default)]
pub struct Tree {
    slots: Vec<u32>,
    nodes: Vec<Node>,
    root: Option<NodeId>,
    root_path: Option<PathBuf>,
    /// Missing parent id -> arena indexes of children waiting for it.
    orphans: HashMap<NodeId, Vec<u32>>,
    orphan_count: usize,
    seen_links: HashSet<FileId>,
    dirty_list: Vec<NodeId>,
    /// Allocated bytes added per direct parent since the last drain.
    growth: HashMap<NodeId, u64>,
    duplicate_records: u64,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    /// A tree whose root displays as `root_path` (instead of the root record's name).
    pub fn with_root_path(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: Some(root_path.into()),
            ..Self::default()
        }
    }

    /// Builds a tree from records in any order, then applies the path rules.
    pub fn from_records(records: impl IntoIterator<Item = ScanRecord>) -> Self {
        let mut t = Self::new();
        for r in records {
            t.insert(r);
        }
        t.apply_path_rules();
        t.clear_dirty();
        t
    }

    pub fn set_root_path(&mut self, p: impl Into<PathBuf>) {
        self.root_path = Some(p.into());
    }

    /// Number of entries received (attached or waiting for a parent).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Entries still waiting for their parent to arrive.
    pub fn orphan_count(&self) -> usize {
        self.orphan_count
    }

    /// Records ignored because their id was already in the tree.
    pub fn duplicate_records(&self) -> u64 {
        self.duplicate_records
    }

    pub fn root_id(&self) -> Option<NodeId> {
        self.root
    }

    pub fn root(&self) -> Option<&Node> {
        self.root.and_then(|r| self.get(r))
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let idx = *self.slots.get(id as usize)?;
        if idx == NONE {
            None
        } else {
            self.nodes.get(idx as usize)
        }
    }

    /// True if `id` is in the tree and connected to the root.
    pub fn is_attached(&self, id: NodeId) -> bool {
        self.get(id).is_some_and(|n| n.attached)
    }

    fn idx(&self, id: NodeId) -> Option<usize> {
        match self.slots.get(id as usize) {
            Some(&i) if i != NONE => Some(i as usize),
            _ => None,
        }
    }

    /// All nodes connected to the root, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| n.attached)
    }

    /// Ids from the root down to `id`, inclusive. Empty if `id` is unknown or detached.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        if !self.is_attached(id) {
            return out;
        }
        let mut cur = Some(id);
        while let Some(c) = cur {
            out.push(c);
            cur = self.get(c).and_then(|n| n.parent);
        }
        out.reverse();
        out
    }

    /// Depth below the root (root = 0).
    pub fn depth(&self, id: NodeId) -> usize {
        self.ancestry(id).len().saturating_sub(1)
    }

    /// Full display path. The root is shown as the root path (if set) or its name.
    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let chain = self.ancestry(id);
        let mut p = PathBuf::new();
        for (i, nid) in chain.iter().enumerate() {
            match (i, &self.root_path) {
                (0, Some(rp)) => p.push(rp),
                _ => p.push(self.nodes[self.idx(*nid).unwrap_or(0)].name()),
            }
        }
        p
    }

    /// Path as a string for DTOs (display only, never logged).
    pub fn display_path(&self, id: NodeId) -> String {
        self.path_of(id).to_string_lossy().into_owned()
    }

    /// Names below the root down to `id`, joined with `/` (root = ""). Used as a stable key
    /// across scans of the same root; `/` cannot appear in a file name on any supported OS.
    pub fn relative_key(&self, id: NodeId) -> String {
        let chain = self.ancestry(id);
        let mut s = String::new();
        for nid in chain.iter().skip(1) {
            if !s.is_empty() {
                s.push('/');
            }
            if let Some(n) = self.get(*nid) {
                s.push_str(&n.name().to_string_lossy());
            }
        }
        s
    }

    /// Child of `parent` with exactly this name.
    pub fn child_by_name(&self, parent: NodeId, name: &OsStr) -> Option<NodeId> {
        self.get(parent)?
            .children
            .iter()
            .copied()
            .find(|&c| self.get(c).is_some_and(|n| n.name() == name))
    }

    /// Looks up a path below the root by its components.
    pub fn find(&self, components: &[&str]) -> Option<NodeId> {
        let mut cur = self.root?;
        for c in components {
            cur = self.child_by_name(cur, OsStr::new(c))?;
        }
        Some(cur)
    }

    /// Display path of the root (root path override or root name).
    pub fn root_display(&self) -> Option<PathBuf> {
        let r = self.root?;
        Some(self.path_of(r))
    }

    /// Adds one record. Duplicate ids are ignored.
    pub fn insert(&mut self, rec: ScanRecord) {
        let ScanRecord { id, parent, meta } = rec;
        let parent = parent.filter(|&p| p != id);
        if self.idx(id).is_some() {
            self.duplicate_records += 1;
            return;
        }
        if parent.is_none() && self.root.is_some() {
            // A second root: keep the first one.
            self.duplicate_records += 1;
            return;
        }

        let (totals, counted, file_category, mix) = match meta.kind {
            NodeKind::Dir => (
                Totals::default(),
                true,
                Category::Other,
                Some(Box::new([0; 10])),
            ),
            NodeKind::File => {
                let mut counted = true;
                if meta.link_count > 1 {
                    if let Some(fid) = meta.file_id {
                        counted = self.seen_links.insert(fid);
                    }
                }
                let cloud_only = matches!(meta.cloud, Some(c) if c.state == CloudState::CloudOnly);
                let t = Totals {
                    allocated: if counted { meta.allocated_size } else { 0 },
                    logical: if counted { meta.logical_size } else { 0 },
                    cloud_only: if counted && cloud_only {
                        meta.logical_size
                    } else {
                        0
                    },
                    files: 1,
                    dirs: 0,
                };
                let cat = category::classify_name(&meta.name, meta.kind, meta.flags);
                (t, counted, cat, None)
            }
            // Links are never followed and take (almost) no space; count their own bytes.
            NodeKind::Link | NodeKind::Other => {
                let t = Totals {
                    allocated: meta.allocated_size,
                    logical: meta.logical_size,
                    ..Totals::default()
                };
                (t, true, Category::Other, None)
            }
        };

        let idx = self.nodes.len() as u32;
        if self.slots.len() <= id as usize {
            self.slots.resize(id as usize + 1, NONE);
        }
        self.slots[id as usize] = idx;
        self.nodes.push(Node {
            id,
            parent,
            meta,
            totals,
            children: Vec::new(),
            file_category,
            mix,
            attached: false,
            counted,
            dirty: false,
        });

        // Adopt children that arrived before us.
        if let Some(waiting) = self.orphans.remove(&id) {
            self.orphan_count -= waiting.len();
            for cidx in waiting {
                let contrib = self.contribution(cidx as usize);
                let cid = self.nodes[cidx as usize].id;
                let me = &mut self.nodes[idx as usize];
                me.children.push(cid);
                me.totals.add(&contrib.totals);
                if let Some(m) = me.mix.as_deref_mut() {
                    add_mix(m, &contrib.mix);
                }
            }
        }

        match parent {
            None => {
                self.root = Some(id);
                self.attach_subtree(idx as usize);
            }
            Some(p) => match self.idx(p) {
                Some(pidx) => {
                    self.nodes[pidx].children.push(id);
                    if self.nodes[pidx].attached {
                        self.attach_subtree(idx as usize);
                    }
                    let contrib = self.contribution(idx as usize);
                    self.propagate(pidx, &contrib);
                    if self.nodes[idx as usize].meta.kind == NodeKind::File {
                        *self.growth.entry(p).or_default() += contrib.totals.allocated;
                    }
                }
                None => {
                    self.orphans.entry(p).or_default().push(idx);
                    self.orphan_count += 1;
                }
            },
        }
    }

    fn contribution(&self, idx: usize) -> Contribution {
        let n = &self.nodes[idx];
        let mut totals = n.totals;
        let mix = match (&n.mix, n.meta.kind) {
            (Some(m), _) => {
                totals.dirs += 1;
                MixDelta::Full(**m)
            }
            (None, NodeKind::File) if n.totals.allocated > 0 => {
                MixDelta::Single(n.file_category, n.totals.allocated)
            }
            _ => MixDelta::None,
        };
        Contribution { totals, mix }
    }

    /// Adds `c` to the node at `idx` and all its ancestors present in the tree.
    fn propagate(&mut self, mut idx: usize, c: &Contribution) {
        loop {
            let n = &mut self.nodes[idx];
            n.totals.add(&c.totals);
            if let Some(m) = n.mix.as_deref_mut() {
                add_mix(m, &c.mix);
            }
            if n.attached && !n.dirty {
                n.dirty = true;
                self.dirty_list.push(n.id);
            }
            match n.parent.and_then(|p| self.idx(p)) {
                Some(p) => idx = p,
                None => break,
            }
        }
    }

    /// Subtracts `c` from the node at `idx` and all its ancestors present in the tree.
    fn unpropagate(&mut self, mut idx: usize, c: &Contribution) {
        loop {
            let n = &mut self.nodes[idx];
            n.totals.sub(&c.totals);
            if let Some(m) = n.mix.as_deref_mut() {
                sub_mix(m, &c.mix);
            }
            if n.attached && !n.dirty {
                n.dirty = true;
                self.dirty_list.push(n.id);
            }
            match n.parent.and_then(|p| self.idx(p)) {
                Some(p) => idx = p,
                None => break,
            }
        }
    }

    /// Drops `id` and everything below it (after a cleanup moved them away), subtracting
    /// their bytes from every ancestor. Returns the removed node's totals, or `None` for
    /// the root or an unknown/detached id. The removed ids stop resolving with `get`.
    /// Parents show up in the next [`drain_changes`](Self::drain_changes).
    ///
    /// A hard link that was the counted copy stays "seen": its other links keep counting
    /// zero bytes until the next scan.
    pub fn remove_subtree(&mut self, id: NodeId) -> Option<Totals> {
        let idx = self.idx(id)?;
        if !self.nodes[idx].attached || Some(id) == self.root {
            return None;
        }
        let pidx = self.nodes[idx].parent.and_then(|p| self.idx(p))?;
        let contrib = self.contribution(idx);
        self.unpropagate(pidx, &contrib);
        self.nodes[pidx].children.retain(|&c| c != id);
        let removed = self.nodes[idx].totals;
        let mut stack = vec![idx];
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i];
            n.attached = false;
            n.dirty = false;
            let nid = n.id;
            for c in std::mem::take(&mut n.children) {
                if let Some(ci) = self.idx(c) {
                    stack.push(ci);
                }
            }
            self.slots[nid as usize] = NONE;
        }
        Some(removed)
    }

    /// Records that a downloaded or pinned cloud file was freed up: its allocated bytes
    /// become 0 and its logical bytes count as cloud-only, all the way up. Returns the
    /// bytes freed, or `None` if `id` is not an attached local cloud file.
    pub fn mark_cloud_only(&mut self, id: NodeId) -> Option<u64> {
        let idx = self.idx(id)?;
        let n = &self.nodes[idx];
        let cloud = n.meta.cloud?;
        if !n.attached || n.meta.kind != NodeKind::File || cloud.state == CloudState::CloudOnly {
            return None;
        }
        let parent = n.parent.and_then(|p| self.idx(p));
        let before = self.contribution(idx);
        if let Some(p) = parent {
            self.unpropagate(p, &before);
        }
        let n = &mut self.nodes[idx];
        let freed = n.totals.allocated;
        n.meta.allocated_size = 0;
        n.meta.cloud = Some(CloudInfo {
            state: CloudState::CloudOnly,
            ..cloud
        });
        n.totals.allocated = 0;
        if n.counted {
            n.totals.cloud_only = n.totals.logical;
        }
        if !n.dirty {
            n.dirty = true;
            self.dirty_list.push(id);
        }
        let after = self.contribution(idx);
        if let Some(p) = parent {
            self.propagate(p, &after);
        }
        Some(freed)
    }

    /// Marks the node at `idx` and everything below it as attached (and dirty).
    fn attach_subtree(&mut self, idx: usize) {
        let mut stack = vec![idx];
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i];
            if n.attached {
                continue;
            }
            n.attached = true;
            if !n.dirty {
                n.dirty = true;
                self.dirty_list.push(n.id);
            }
            for c in n.children.clone() {
                if let Some(ci) = self.idx(c) {
                    stack.push(ci);
                }
            }
        }
    }

    /// Ids of attached nodes added or changed since the last call, and the folder that
    /// gained the most bytes from new files in that time.
    pub fn drain_changes(&mut self) -> (Vec<NodeId>, Option<NodeId>) {
        let ids = std::mem::take(&mut self.dirty_list);
        for &id in &ids {
            if let Some(i) = self.idx(id) {
                self.nodes[i].dirty = false;
            }
        }
        let growth = std::mem::take(&mut self.growth);
        let busiest = growth
            .into_iter()
            .filter(|&(id, b)| b > 0 && self.is_attached(id))
            .max_by_key(|&(id, b)| (b, std::cmp::Reverse(id)))
            .map(|(id, _)| id);
        (ids, busiest)
    }

    fn clear_dirty(&mut self) {
        let _ = self.drain_changes();
    }

    /// Recomputes file categories with path context (Program Files, Windows, /usr, ...) and
    /// rebuilds the directory category mixes. Call once the scan is done; while scanning,
    /// categories come from names only because ancestors may not have arrived.
    pub fn apply_path_rules(&mut self) {
        let Some(root) = self.root.and_then(|r| self.idx(r)) else {
            return;
        };
        // Pre-order walk with each node's context.
        let mut order: Vec<usize> = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![(root, PathContext::None, 0usize)];
        while let Some((i, ctx, depth)) = stack.pop() {
            order.push(i);
            let n = &mut self.nodes[i];
            match n.meta.kind {
                NodeKind::Dir => {
                    let my_ctx = if depth == 0 {
                        PathContext::None
                    } else {
                        category::dir_context(ctx, &n.meta.name, depth)
                    };
                    if let Some(m) = n.mix.as_deref_mut() {
                        *m = [0; CATEGORY_COUNT];
                    }
                    for &c in &n.children {
                        if let Some(ci) = self.slots.get(c as usize).copied() {
                            if ci != NONE {
                                stack.push((ci as usize, my_ctx, depth + 1));
                            }
                        }
                    }
                }
                NodeKind::File => {
                    let base = category::classify_name(&n.meta.name, n.meta.kind, n.meta.flags);
                    n.file_category = category::apply_context(base, ctx);
                }
                _ => {}
            }
        }
        // Children come after parents in `order`, so a reverse pass sums bottom-up.
        for &i in order.iter().rev() {
            let Some(p) = self.nodes[i].parent.and_then(|p| self.idx(p)) else {
                continue;
            };
            let delta = match (&self.nodes[i].mix, self.nodes[i].meta.kind) {
                (Some(m), _) => MixDelta::Full(**m),
                (None, NodeKind::File) if self.nodes[i].totals.allocated > 0 => {
                    MixDelta::Single(self.nodes[i].file_category, self.nodes[i].totals.allocated)
                }
                _ => MixDelta::None,
            };
            if let Some(m) = self.nodes[p].mix.as_deref_mut() {
                add_mix(m, &delta);
            }
        }
    }
}

/// Changes since the last [`TreeBuilder::drain`] call, for the ~10/s live UI update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveChanges {
    /// New or changed attached nodes (including every ancestor whose totals moved).
    pub changed: Vec<NodeId>,
    /// The folder that gained the most bytes from new files since the last drain.
    pub busiest: Option<NodeId>,
    pub progress: ScanProgress,
}

#[derive(Debug, Default)]
struct BuilderState {
    tree: Tree,
    progress: ScanProgress,
    warnings: Vec<ScanWarning>,
}

/// `ScanSink` that builds a [`Tree`] while the scan runs. See the module docs for the
/// locking design.
#[derive(Debug, Default)]
pub struct TreeBuilder {
    state: Mutex<BuilderState>,
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The root will display as `root_path` (e.g. `C:\`) whatever the root record's name.
    pub fn with_root_path(root_path: impl AsRef<Path>) -> Self {
        Self {
            state: Mutex::new(BuilderState {
                tree: Tree::with_root_path(root_path.as_ref()),
                ..BuilderState::default()
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BuilderState> {
        // A panic while holding the lock can only come from a bug in this module; the
        // tree is still usable for reading, so recover instead of poisoning the UI.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` with the tree locked. Keep `f` short: the scanner waits meanwhile.
    pub fn read<R>(&self, f: impl FnOnce(&Tree) -> R) -> R {
        f(&self.lock().tree)
    }

    /// Changed node ids, busiest folder and latest progress since the last call.
    pub fn drain(&self) -> LiveChanges {
        let mut s = self.lock();
        let (changed, busiest) = s.tree.drain_changes();
        LiveChanges {
            changed,
            busiest,
            progress: s.progress,
        }
    }

    /// Like [`drain`](Self::drain), and runs `f` on the tree under the same lock (e.g. to
    /// turn the changed ids into DTOs consistently).
    pub fn drain_with<R>(&self, f: impl FnOnce(&Tree, &LiveChanges) -> R) -> R {
        let mut s = self.lock();
        let (changed, busiest) = s.tree.drain_changes();
        let changes = LiveChanges {
            changed,
            busiest,
            progress: s.progress,
        };
        f(&s.tree, &changes)
    }

    pub fn latest_progress(&self) -> ScanProgress {
        self.lock().progress
    }

    pub fn warnings(&self) -> Vec<ScanWarning> {
        self.lock().warnings.clone()
    }

    /// Applies the path rules and returns the finished tree and the warnings.
    pub fn finish(self) -> (Tree, Vec<ScanWarning>) {
        let s = self.state.into_inner().unwrap_or_else(|e| e.into_inner());
        let mut tree = s.tree;
        tree.apply_path_rules();
        (tree, s.warnings)
    }

    /// Applies the path rules in place (for when the builder is shared behind an `Arc`).
    pub fn finalize(&self) {
        self.lock().tree.apply_path_rules();
    }

    /// [`Tree::remove_subtree`] under the builder lock.
    pub fn remove_subtree(&self, id: NodeId) -> Option<Totals> {
        self.lock().tree.remove_subtree(id)
    }

    /// [`Tree::mark_cloud_only`] under the builder lock.
    pub fn mark_cloud_only(&self, id: NodeId) -> Option<u64> {
        self.lock().tree.mark_cloud_only(id)
    }
}

impl ScanSink for TreeBuilder {
    fn batch(&self, records: Vec<ScanRecord>) {
        let mut s = self.lock();
        for r in records {
            s.tree.insert(r);
        }
    }

    fn progress(&self, progress: ScanProgress) {
        self.lock().progress = progress;
    }

    fn warning(&self, warning: ScanWarning) {
        self.lock().warnings.push(warning);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{CloudInfo, CloudProvider, EntryFlags};
    use std::ffi::OsString;

    pub fn meta(name: &str, kind: NodeKind, size: u64) -> EntryMeta {
        EntryMeta {
            name: OsString::from(name),
            kind,
            logical_size: size,
            allocated_size: size,
            modified: Some(1_700_000_000),
            accessed: None,
            file_id: None,
            link_count: 1,
            cloud: None,
            flags: EntryFlags::empty(),
        }
    }

    pub fn rec(id: NodeId, parent: Option<NodeId>, m: EntryMeta) -> ScanRecord {
        ScanRecord {
            id,
            parent,
            meta: m,
        }
    }

    /// root(0) / Users(1) / William(2) / {a.mp4(3) 100, b.txt(4) 10, Docs(5) / c.pdf(6) 50}
    pub fn sample() -> Vec<ScanRecord> {
        vec![
            rec(0, None, meta("C:\\", NodeKind::Dir, 0)),
            rec(1, Some(0), meta("Users", NodeKind::Dir, 0)),
            rec(2, Some(1), meta("William", NodeKind::Dir, 0)),
            rec(3, Some(2), meta("a.mp4", NodeKind::File, 100)),
            rec(4, Some(2), meta("b.txt", NodeKind::File, 10)),
            rec(5, Some(2), meta("Docs", NodeKind::Dir, 0)),
            rec(6, Some(5), meta("c.pdf", NodeKind::File, 50)),
        ]
    }

    fn check_sample(t: &Tree) {
        let root = t.root().unwrap();
        assert_eq!(root.totals.allocated, 160);
        assert_eq!(root.totals.files, 3);
        assert_eq!(root.totals.dirs, 3);
        assert_eq!(root.category(), Category::Video);
        let w = t.get(2).unwrap();
        assert_eq!(w.totals.allocated, 160);
        assert_eq!(w.totals.dirs, 1);
        assert_eq!(t.get(5).unwrap().totals.allocated, 50);
        assert_eq!(t.get(5).unwrap().category(), Category::Documents);
        assert_eq!(t.orphan_count(), 0);
        assert!(t.iter().all(|n| n.attached));
    }

    #[test]
    fn top_down() {
        let t = Tree::from_records(sample());
        check_sample(&t);
    }

    #[test]
    fn children_first_and_shuffled() {
        let mut r = sample();
        r.reverse();
        check_sample(&Tree::from_records(r));
        // A few fixed permutations.
        for perm in [
            [3, 6, 0, 5, 1, 4, 2],
            [6, 5, 4, 3, 2, 1, 0],
            [2, 3, 6, 1, 5, 0, 4],
        ] {
            let s = sample();
            let r: Vec<_> = perm.iter().map(|&i| s[i].clone()).collect();
            check_sample(&Tree::from_records(r));
        }
    }

    #[test]
    fn orphans_wait_and_are_not_counted() {
        let mut t = Tree::new();
        t.insert(rec(0, None, meta("root", NodeKind::Dir, 0)));
        t.insert(rec(6, Some(5), meta("c.pdf", NodeKind::File, 50)));
        assert_eq!(t.orphan_count(), 1);
        assert_eq!(t.root().unwrap().totals.allocated, 0);
        assert!(!t.is_attached(6));
        t.insert(rec(5, Some(0), meta("Docs", NodeKind::Dir, 0)));
        assert_eq!(t.orphan_count(), 0);
        assert_eq!(t.root().unwrap().totals.allocated, 50);
        assert!(t.is_attached(6));
    }

    #[test]
    fn hard_links_count_once() {
        let mut r = sample();
        let fid = FileId {
            volume: 1,
            index: 42,
        };
        for (id, parent) in [(7, 2), (8, 5)] {
            let mut m = meta("big.iso", NodeKind::File, 1000);
            m.file_id = Some(fid);
            m.link_count = 2;
            r.push(rec(id, Some(parent), m));
        }
        let t = Tree::from_records(r);
        assert_eq!(t.root().unwrap().totals.allocated, 1160);
        assert_eq!(t.root().unwrap().totals.files, 5);
        assert!(t.get(7).unwrap().is_counted());
        assert!(!t.get(8).unwrap().is_counted());
    }

    #[test]
    fn cloud_only_totals() {
        let mut r = sample();
        let mut m = meta("movie.mkv", NodeKind::File, 0);
        m.logical_size = 5000;
        m.allocated_size = 0;
        m.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::CloudOnly,
        });
        r.push(rec(7, Some(2), m));
        let t = Tree::from_records(r);
        let root = t.root().unwrap();
        assert_eq!(root.totals.allocated, 160);
        assert_eq!(root.totals.cloud_only, 5000);
        assert_eq!(root.totals.logical, 5160);
    }

    #[test]
    fn paths_and_keys() {
        let t = Tree::from_records(sample());
        assert_eq!(t.relative_key(6), "Users/William/Docs/c.pdf");
        assert_eq!(t.relative_key(0), "");
        assert_eq!(t.ancestry(6), vec![0, 1, 2, 5, 6]);
        assert_eq!(t.depth(6), 4);
        assert_eq!(t.find(&["Users", "William", "Docs"]), Some(5));
        let p = t.path_of(6);
        assert!(p.ends_with("c.pdf"));
        let mut t2 = Tree::with_root_path("/home/william");
        t2.insert(rec(0, None, meta("ignored", NodeKind::Dir, 0)));
        t2.insert(rec(1, Some(0), meta("x", NodeKind::File, 1)));
        assert_eq!(t2.path_of(1), Path::new("/home/william").join("x"));
    }

    #[test]
    fn deltas_and_busiest() {
        let mut t = Tree::new();
        for r in sample() {
            t.insert(r);
        }
        let (ids, busiest) = t.drain_changes();
        assert_eq!(ids.len(), 7);
        assert_eq!(busiest, Some(2));
        let (ids, busiest) = t.drain_changes();
        assert!(ids.is_empty());
        assert_eq!(busiest, None);
        t.insert(rec(7, Some(5), meta("d.pdf", NodeKind::File, 5)));
        let (mut ids, busiest) = t.drain_changes();
        ids.sort();
        assert_eq!(ids, vec![0, 1, 2, 5, 7]);
        assert_eq!(busiest, Some(5));
    }

    #[test]
    fn duplicates_ignored() {
        let mut r = sample();
        r.push(rec(3, Some(2), meta("a.mp4", NodeKind::File, 100)));
        r.push(rec(99, None, meta("other-root", NodeKind::Dir, 0)));
        let t = Tree::from_records(r);
        check_sample(&t);
        assert_eq!(t.duplicate_records(), 2);
    }

    #[test]
    fn path_rules_on_finish() {
        let r = vec![
            rec(0, None, meta("C:\\", NodeKind::Dir, 0)),
            rec(1, Some(0), meta("Program Files", NodeKind::Dir, 0)),
            rec(2, Some(1), meta("tool.exe", NodeKind::File, 300)),
            rec(3, Some(0), meta("Windows", NodeKind::Dir, 0)),
            rec(4, Some(3), meta("wallpaper.jpg", NodeKind::File, 200)),
            rec(5, Some(0), meta("setup.exe", NodeKind::File, 100)),
        ];
        let t = Tree::from_records(r);
        assert_eq!(t.get(2).unwrap().category(), Category::Apps);
        assert_eq!(t.get(4).unwrap().category(), Category::System);
        assert_eq!(t.get(5).unwrap().category(), Category::Installers);
        assert_eq!(t.root().unwrap().category(), Category::Apps);
        let mix = t.root().unwrap().category_mix();
        assert_eq!(
            mix,
            vec![
                (Category::Installers, 100),
                (Category::Apps, 300),
                (Category::System, 200)
            ]
        );
    }

    #[test]
    fn remove_subtree_updates_ancestors() {
        let mut t = Tree::from_records(sample());
        // Docs (5) holds c.pdf (50).
        let removed = t.remove_subtree(5).unwrap();
        assert_eq!(removed.allocated, 50);
        let root = t.root().unwrap();
        assert_eq!(root.totals.allocated, 110);
        assert_eq!(root.totals.files, 2);
        assert_eq!(root.totals.dirs, 2);
        assert!(t.get(5).is_none());
        assert!(t.get(6).is_none());
        assert!(!t.get(2).unwrap().children().contains(&5));
        // Only b.txt (10) is left of the documents.
        assert!(root.category_mix().contains(&(Category::Documents, 10)));
        let (mut changed, _) = t.drain_changes();
        changed.sort();
        assert_eq!(changed, vec![0, 1, 2]);
        // A file, then refusals.
        assert_eq!(t.remove_subtree(3).unwrap().allocated, 100);
        assert_eq!(t.root().unwrap().totals.allocated, 10);
        assert_eq!(t.root().unwrap().category(), Category::Documents);
        assert!(t.remove_subtree(0).is_none(), "root stays");
        assert!(t.remove_subtree(3).is_none(), "already gone");
        assert!(t.remove_subtree(999).is_none());
    }

    #[test]
    fn mark_cloud_only_moves_bytes() {
        use crate::model::{CloudInfo, CloudProvider};
        let mut r = sample();
        let mut m = meta("trip.mp4", NodeKind::File, 400);
        m.cloud = Some(CloudInfo {
            provider: CloudProvider::OneDrive,
            state: CloudState::Downloaded,
        });
        r.push(rec(7, Some(5), m));
        let mut t = Tree::from_records(r);
        assert_eq!(t.root().unwrap().totals.allocated, 560);
        assert_eq!(t.mark_cloud_only(7), Some(400));
        let root = t.root().unwrap();
        assert_eq!(root.totals.allocated, 160);
        assert_eq!(root.totals.cloud_only, 400);
        assert_eq!(root.totals.logical, 560);
        assert_eq!(root.totals.files, 4);
        assert_eq!(t.get(5).unwrap().totals.cloud_only, 400);
        assert!(t.get(7).unwrap().is_cloud_only());
        assert_eq!(root.category(), Category::Video);
        let (mut changed, _) = t.drain_changes();
        changed.sort();
        assert_eq!(changed, vec![0, 1, 2, 5, 7]);
        assert_eq!(t.mark_cloud_only(7), None, "already cloud-only");
        assert_eq!(t.mark_cloud_only(3), None, "not a cloud file");
        // Through the builder.
        let b = TreeBuilder::new();
        b.batch(sample());
        assert_eq!(b.remove_subtree(4).map(|x| x.allocated), Some(10));
        assert_eq!(b.read(|t| t.root().unwrap().totals.allocated), 150);
        assert_eq!(b.mark_cloud_only(3), None);
    }

    #[test]
    fn builder_is_a_sink() {
        let b = std::sync::Arc::new(TreeBuilder::new());
        let feeder = {
            let b = b.clone();
            std::thread::spawn(move || {
                let s = sample();
                b.batch(s[3..].to_vec());
                b.batch(s[..3].to_vec());
                b.progress(ScanProgress {
                    files: 3,
                    ..Default::default()
                });
                b.warning(ScanWarning {
                    path: "C:\\x".into(),
                    message: "denied".into(),
                });
            })
        };
        feeder.join().unwrap();
        let ch = b.drain();
        assert_eq!(ch.changed.len(), 7);
        assert_eq!(ch.progress.files, 3);
        assert_eq!(b.read(|t| t.root().unwrap().totals.allocated), 160);
        let b = std::sync::Arc::try_unwrap(b).unwrap();
        let (t, w) = b.finish();
        check_sample(&t);
        assert_eq!(w.len(), 1);
    }
}
