//! Read-only queries over a [`Tree`]: sorted children with paging, ancestry, chart subtrees
//! with small items folded into "other", top-N and search.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use crate::category::{self, Category};
use crate::dto::{node_dto, CloudFilter, Deltas, Filter, NodeDto, SortKey, TreeDto};
use crate::model::{NodeId, NodeKind};
use crate::scan::SkipReason;
use crate::tree::{Node, Tree};

const DAY: i64 = 86_400;

fn compare(a: &Node, b: &Node, key: SortKey, deltas: Option<&Deltas>) -> Ordering {
    let by_size = || b.totals.allocated.cmp(&a.totals.allocated);
    let primary = match key {
        SortKey::Allocated => by_size(),
        SortKey::Logical => b.totals.logical.cmp(&a.totals.logical),
        SortKey::Name => {
            let (x, y) = (a.name().to_string_lossy(), b.name().to_string_lossy());
            x.to_lowercase()
                .cmp(&y.to_lowercase())
                .then_with(|| x.cmp(&y))
        }
        SortKey::Modified => b.meta.modified.cmp(&a.meta.modified),
        SortKey::FileCount => b.totals.files.cmp(&a.totals.files),
        SortKey::Delta => {
            let d = |n: &Node| deltas.and_then(|d| d.get(&n.id).copied());
            // Biggest growth first; unknown deltas last.
            match (d(a), d(b)) {
                (Some(x), Some(y)) => y.cmp(&x),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
    };
    primary.then_with(by_size).then_with(|| a.id.cmp(&b.id))
}

/// Children of `id`, sorted, then `offset..offset+limit`.
pub fn children(
    tree: &Tree,
    id: NodeId,
    sort: SortKey,
    offset: usize,
    limit: usize,
    deltas: Option<&Deltas>,
) -> Vec<NodeDto> {
    let Some(n) = tree.get(id) else {
        return Vec::new();
    };
    let mut kids: Vec<&Node> = n.children().iter().filter_map(|&c| tree.get(c)).collect();
    kids.sort_by(|a, b| compare(a, b, sort, deltas));
    kids.into_iter()
        .skip(offset)
        .take(limit)
        .map(|k| node_dto(k, deltas))
        .collect()
}

/// Ancestors from the root to `id`, inclusive (breadcrumbs).
pub fn path_to(tree: &Tree, id: NodeId, deltas: Option<&Deltas>) -> Vec<NodeDto> {
    tree.ancestry(id)
        .into_iter()
        .filter_map(|a| tree.get(a))
        .map(|n| node_dto(n, deltas))
        .collect()
}

/// Size a chart gives a node: bytes on disk plus cloud-only bytes, as the UI sizes nodes
/// (so cloud-only folders show up, e.g. as terrain lakes, instead of always folding).
fn display_size(n: &Node) -> u64 {
    n.totals.allocated.saturating_add(n.totals.cloud_only)
}

/// Nested subtree under `id` for charts, `max_depth` levels deep (0 = just the node).
/// Children whose display size (allocated + cloud-only) is below `min_fraction` of `id`'s
/// display size are folded into one "other" node per parent (id `-(parentId + 1)`,
/// `childCount` = number folded). Children are ordered by display size, largest first.
pub fn subtree(
    tree: &Tree,
    id: NodeId,
    max_depth: u32,
    min_fraction: f64,
    deltas: Option<&Deltas>,
) -> Option<TreeDto> {
    let top = tree.get(id)?;
    let threshold = (display_size(top) as f64 * min_fraction.max(0.0)).ceil() as u64;
    Some(build_subtree(tree, top, max_depth, threshold, deltas))
}

fn build_subtree(
    tree: &Tree,
    n: &Node,
    depth_left: u32,
    threshold: u64,
    deltas: Option<&Deltas>,
) -> TreeDto {
    let mut children = Vec::new();
    if depth_left > 0 && !n.children().is_empty() {
        let mut kids: Vec<&Node> = n.children().iter().filter_map(|&c| tree.get(c)).collect();
        kids.sort_by(|a, b| {
            display_size(b)
                .cmp(&display_size(a))
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut fold: Option<TreeDto> = None;
        for k in kids {
            if threshold > 0 && display_size(k) < threshold {
                let f = fold.get_or_insert_with(|| fold_node(n));
                f.allocated += k.totals.allocated;
                f.logical += k.totals.logical;
                f.cloud_only += k.totals.cloud_only;
                f.file_count += k.totals.files;
                f.dir_count += k.totals.dirs + u64::from(k.is_dir());
                f.child_count += 1;
            } else {
                children.push(build_subtree(tree, k, depth_left - 1, threshold, deltas));
            }
        }
        children.extend(fold);
    }
    TreeDto::from_node(node_dto(n, deltas), children)
}

fn fold_node(parent: &Node) -> TreeDto {
    TreeDto {
        id: TreeDto::fold_id(parent.id),
        parent: Some(i64::from(parent.id)),
        name: "Other".into(),
        kind: NodeKind::Other,
        allocated: 0,
        logical: 0,
        cloud_only: 0,
        file_count: 0,
        dir_count: 0,
        category: Category::Other,
        modified: None,
        accessed: None,
        cloud: None,
        delta: None,
        hidden: false,
        access_denied: false,
        not_descended: None::<SkipReason>,
        child_count: 0,
        children: Vec::new(),
    }
}

/// A compiled [`Filter`], evaluated against nodes. `now` is Unix seconds.
pub struct Matcher<'a> {
    filter: &'a Filter,
    text: Option<TextMatch>,
    now: i64,
}

enum TextMatch {
    /// `*.mp4`, `.mp4`: extension match.
    Ext(String),
    Substring(String),
}

impl<'a> Matcher<'a> {
    pub fn new(filter: &'a Filter, now: i64) -> Self {
        let text = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| {
                let lower = t.to_lowercase();
                match lower.strip_prefix("*.").or_else(|| lower.strip_prefix('.')) {
                    Some(ext) if !ext.is_empty() && !ext.contains(['.', ' ']) => {
                        TextMatch::Ext(ext.to_string())
                    }
                    _ => TextMatch::Substring(lower),
                }
            });
        Self { filter, text, now }
    }

    pub fn matches(&self, n: &Node) -> bool {
        let f = self.filter;
        let size = n.totals.allocated;
        if f.min_bytes.is_some_and(|m| size < m) || f.max_bytes.is_some_and(|m| size > m) {
            return false;
        }
        if let Some(cats) = &f.categories {
            if !cats.is_empty() && !cats.contains(&n.category()) {
                return false;
            }
        }
        if let Some(days) = f.older_than_days {
            let cutoff = self.now - i64::from(days) * DAY;
            match n.meta.modified {
                Some(m) if m <= cutoff => {}
                _ => return false,
            }
        }
        match f.cloud.unwrap_or_default() {
            CloudFilter::Any => {}
            CloudFilter::Local => {
                if n.is_cloud_only()
                    || (n.is_dir() && n.totals.allocated == 0 && n.totals.cloud_only > 0)
                {
                    return false;
                }
            }
            CloudFilter::CloudOnly => {
                if !(n.is_cloud_only() || (n.is_dir() && n.totals.cloud_only > 0)) {
                    return false;
                }
            }
        }
        match &self.text {
            None => true,
            Some(TextMatch::Ext(e)) => category::extension(n.name()).is_some_and(|x| &x == e),
            Some(TextMatch::Substring(s)) => n.name().to_string_lossy().to_lowercase().contains(s),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopKind {
    Files,
    Dirs,
}

/// Largest `n` files or directories (by allocated bytes) matching `filter`.
/// For directories the scan root itself is left out.
pub fn top(
    tree: &Tree,
    kind: TopKind,
    n: usize,
    filter: &Filter,
    now: i64,
    deltas: Option<&Deltas>,
) -> Vec<NodeDto> {
    let m = Matcher::new(filter, now);
    let root = tree.root_id();
    let wanted = |node: &Node| match kind {
        TopKind::Files => node.kind() == NodeKind::File,
        TopKind::Dirs => node.is_dir() && Some(node.id) != root,
    };
    largest(tree.iter().filter(|x| wanted(x) && m.matches(x)), n)
        .into_iter()
        .map(|x| node_dto(x, deltas))
        .collect()
}

/// Nodes matching `filter`, largest first, at most `limit`. The root is never returned.
pub fn search(
    tree: &Tree,
    filter: &Filter,
    limit: usize,
    now: i64,
    deltas: Option<&Deltas>,
) -> Vec<NodeDto> {
    let m = Matcher::new(filter, now);
    let root = tree.root_id();
    largest(
        tree.iter().filter(|x| Some(x.id) != root && m.matches(x)),
        limit,
    )
    .into_iter()
    .map(|x| node_dto(x, deltas))
    .collect()
}

/// The `n` largest nodes by allocated bytes (ties: lower id first), largest first.
pub fn largest<'a>(it: impl Iterator<Item = &'a Node>, n: usize) -> Vec<&'a Node> {
    if n == 0 {
        return Vec::new();
    }
    // Min-heap of the best n so far.
    let mut heap: BinaryHeap<Reverse<(u64, Reverse<NodeId>, usize)>> = BinaryHeap::new();
    let mut nodes: Vec<&Node> = Vec::new();
    for node in it {
        let key = (node.totals.allocated, Reverse(node.id), nodes.len());
        if heap.len() < n {
            heap.push(Reverse(key));
            nodes.push(node);
        } else if let Some(Reverse(min)) = heap.peek() {
            if (key.0, key.1) > (min.0, min.1) {
                heap.pop();
                heap.push(Reverse(key));
                nodes.push(node);
            }
        }
    }
    let mut out: Vec<_> = heap.into_iter().map(|Reverse(k)| k).collect();
    out.sort_by(|a, b| b.cmp(a));
    out.into_iter().map(|(_, _, i)| nodes[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{meta, rec, sample};

    fn tree() -> Tree {
        Tree::from_records(sample())
    }

    #[test]
    fn children_sorted_and_paged() {
        let t = tree();
        let names = |v: Vec<NodeDto>| v.into_iter().map(|n| n.name).collect::<Vec<_>>();
        assert_eq!(
            names(children(&t, 2, SortKey::Allocated, 0, 10, None)),
            ["a.mp4", "Docs", "b.txt"]
        );
        assert_eq!(
            names(children(&t, 2, SortKey::Name, 0, 10, None)),
            ["a.mp4", "b.txt", "Docs"]
        );
        assert_eq!(
            names(children(&t, 2, SortKey::Allocated, 1, 1, None)),
            ["Docs"]
        );
        assert!(children(&t, 2, SortKey::Allocated, 5, 1, None).is_empty());
        assert!(children(&t, 999, SortKey::Allocated, 0, 1, None).is_empty());
        let mut d = Deltas::new();
        d.insert(4, 9);
        d.insert(3, -1);
        assert_eq!(
            names(children(&t, 2, SortKey::Delta, 0, 10, Some(&d))),
            ["b.txt", "a.mp4", "Docs"]
        );
    }

    #[test]
    fn path_to_root() {
        let t = tree();
        let ids: Vec<_> = path_to(&t, 6, None).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, [0, 1, 2, 5, 6]);
        assert!(path_to(&t, 77, None).is_empty());
    }

    #[test]
    fn subtree_folds_small_children() {
        let t = tree();
        // Root is 160 bytes; 0.2 => threshold 32: b.txt (10) folds.
        let s = subtree(&t, 2, 2, 0.2, None).unwrap();
        assert_eq!(s.id, 2);
        let names: Vec<_> = s.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a.mp4", "Docs", "Other"]);
        let fold = s.children.last().unwrap();
        assert_eq!(fold.id, -3);
        assert_eq!(fold.parent, Some(2));
        assert_eq!(fold.child_count, 1);
        assert_eq!(fold.allocated, 10);
        assert_eq!(fold.kind, NodeKind::Other);
        // Docs has its child at depth 2.
        assert_eq!(s.children[1].children.len(), 1);
        // Depth limit.
        let s0 = subtree(&t, 2, 0, 0.0, None).unwrap();
        assert!(s0.children.is_empty());
        let s1 = subtree(&t, 2, 1, 0.0, None).unwrap();
        assert_eq!(s1.children.len(), 3);
        assert!(s1.children.iter().all(|c| c.children.is_empty()));
    }

    #[test]
    fn top_and_search() {
        let t = tree();
        let f = Filter::default();
        let files: Vec<_> = top(&t, TopKind::Files, 2, &f, 0, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(files, ["a.mp4", "c.pdf"]);
        let dirs: Vec<_> = top(&t, TopKind::Dirs, 10, &f, 0, None)
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(dirs, [1, 2, 5]);

        let by_ext = Filter {
            text: Some("*.PDF".into()),
            ..Filter::default()
        };
        let r = search(&t, &by_ext, 10, 0, None);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "c.pdf");
        let sub = Filter {
            text: Some("doc".into()),
            ..Filter::default()
        };
        assert_eq!(search(&t, &sub, 10, 0, None)[0].name, "Docs");
        let cat = Filter {
            categories: Some(vec![Category::Video]),
            ..Filter::default()
        };
        // a.mp4 plus the folders dominated by video.
        let names: Vec<_> = search(&t, &cat, 10, 0, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, ["Users", "William", "a.mp4"]);
        let size = Filter {
            min_bytes: Some(20),
            max_bytes: Some(60),
            ..Filter::default()
        };
        let names: Vec<_> = search(&t, &size, 10, 0, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, ["Docs", "c.pdf"]);
        assert_eq!(search(&t, &f, 2, 0, None).len(), 2);
    }

    /// sample() plus, under William (2): a cloud-only folder OneDrive (7) holding a 500-byte
    /// cloud-only file (8), and a 5-byte cloud-only file (9).
    fn with_cloud() -> Tree {
        use crate::model::{CloudInfo, CloudProvider, CloudState};
        let cloud = |name: &str, size: u64| {
            let mut m = meta(name, NodeKind::File, 0);
            m.logical_size = size;
            m.cloud = Some(CloudInfo {
                provider: CloudProvider::OneDrive,
                state: CloudState::CloudOnly,
            });
            m
        };
        let mut r = sample();
        r.push(rec(7, Some(2), meta("OneDrive", NodeKind::Dir, 0)));
        r.push(rec(8, Some(7), cloud("movie.mkv", 500)));
        r.push(rec(9, Some(2), cloud("note.txt", 5)));
        Tree::from_records(r)
    }

    #[test]
    fn subtree_sizes_by_allocated_plus_cloud_only() {
        let t = with_cloud();
        // William: 160 allocated + 505 cloud-only = 665; 0.1 => threshold 67.
        let s = subtree(&t, 2, 2, 0.1, None).unwrap();
        let names: Vec<_> = s.children.iter().map(|c| c.name.as_str()).collect();
        // (c) ordered by allocated + cloud-only, (a) the cloud-only folder is kept.
        assert_eq!(names, ["OneDrive", "a.mp4", "Other"]);
        let od = &s.children[0];
        assert_eq!(od.allocated, 0);
        assert_eq!(od.cloud_only, 500);
        assert_eq!(
            od.children.len(),
            1,
            "its cloud-only file is listed by name"
        );
        // (b) small cloud-only files still fold, and their bytes go to the fold node.
        let fold = s.children.last().unwrap();
        assert_eq!(fold.id, TreeDto::fold_id(2));
        assert_eq!(fold.child_count, 3); // Docs, b.txt, note.txt
        assert_eq!(fold.allocated, 60);
        assert_eq!(fold.cloud_only, 5);
        // Lists still sort by allocated only.
        let by_alloc: Vec<_> = children(&t, 2, SortKey::Allocated, 0, 10, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(by_alloc[0], "a.mp4");
    }

    #[test]
    fn age_and_cloud_filters() {
        use crate::model::{CloudInfo, CloudProvider, CloudState};
        let mut r = sample();
        let mut m = meta("cloud.mkv", NodeKind::File, 0);
        m.logical_size = 900;
        m.cloud = Some(CloudInfo {
            provider: CloudProvider::Dropbox,
            state: CloudState::CloudOnly,
        });
        m.modified = Some(1_000);
        r.push(rec(7, Some(2), m));
        let t = Tree::from_records(r);
        let now = 1_700_000_000 + 10 * DAY;
        let old = Filter {
            older_than_days: Some(5),
            ..Filter::default()
        };
        assert_eq!(search(&t, &old, 100, now, None).len(), 7);
        let older = Filter {
            older_than_days: Some(20),
            ..Filter::default()
        };
        let names: Vec<_> = search(&t, &older, 100, now, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, ["cloud.mkv"]);
        let cloud = Filter {
            cloud: Some(CloudFilter::CloudOnly),
            ..Filter::default()
        };
        let names: Vec<_> = search(&t, &cloud, 100, now, None)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, ["Users", "William", "cloud.mkv"]);
        let local = Filter {
            cloud: Some(CloudFilter::Local),
            text: Some("mkv".into()),
            ..Filter::default()
        };
        assert!(search(&t, &local, 100, now, None).is_empty());
    }

    #[test]
    fn largest_keeps_top_n() {
        let t = tree();
        let l = largest(t.iter(), 3);
        let ids: Vec<_> = l.iter().map(|n| n.id).collect();
        // 0, 1, 2 all have 160 bytes: ties broken by lower id.
        assert_eq!(ids, [0, 1, 2]);
        assert!(largest(t.iter(), 0).is_empty());
        assert_eq!(largest(t.iter(), 100).len(), 7);
    }
}
