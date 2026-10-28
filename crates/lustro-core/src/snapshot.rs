//! Scan history on SQLite: per-folder totals per scan, diffs, growth trend.
//!
//! The database lives at a path the caller chooses (the app's local data folder; tests
//! use a temp dir). It holds display paths, so it never leaves the machine.
//!
//! What is stored per snapshot: every directory (down to `SaveOptions::max_depth`) and
//! files of at least `SaveOptions::min_file_bytes`. Entries are keyed by their path
//! relative to the root, with `/` between names (see [`Tree::relative_key`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::category::Category;
use crate::dto::{Deltas, DiffDto, DiffEntryDto, DiffStatus, SnapshotMeta, TrendPoint};
use crate::model::{NodeId, NodeKind};
use crate::tree::Tree;

/// Number of snapshots shown in each diff entry's sparkline (oldest first).
pub const SPARKLINE_POINTS: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("snapshot not found")]
    NotFound,
    #[error("the tree has no root")]
    EmptyTree,
    #[error("snapshots belong to different roots")]
    DifferentRoots,
}

pub type Result<T> = std::result::Result<T, SnapshotError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveOptions {
    /// Deepest directory level stored (root = 0). `None` = all.
    pub max_depth: Option<usize>,
    /// Files at least this big are stored individually.
    pub min_file_bytes: u64,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            min_file_bytes: 100 * 1024 * 1024,
        }
    }
}

/// Volume usage at scan time, for the trend line and "days until full".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeUsage {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

pub struct SnapshotStore {
    conn: Connection,
    keep: usize,
}

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS snapshots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    root        TEXT    NOT NULL,
    taken_at    INTEGER NOT NULL,
    allocated   INTEGER NOT NULL,
    file_count  INTEGER NOT NULL,
    used_bytes  INTEGER,
    total_bytes INTEGER
);
CREATE INDEX IF NOT EXISTS snapshots_by_root ON snapshots (root, taken_at);
CREATE TABLE IF NOT EXISTS entries (
    snapshot_id INTEGER NOT NULL REFERENCES snapshots (id) ON DELETE CASCADE,
    key         TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    kind        INTEGER NOT NULL,
    category    INTEGER NOT NULL,
    depth       INTEGER NOT NULL,
    allocated   INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, key)
) WITHOUT ROWID;
PRAGMA user_version = 1;
";

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    kind: NodeKind,
    category: Category,
    depth: u32,
    allocated: u64,
}

fn kind_to_i(k: NodeKind) -> i64 {
    match k {
        NodeKind::File => 0,
        NodeKind::Dir => 1,
        NodeKind::Link => 2,
        NodeKind::Other => 3,
    }
}

fn kind_from_i(i: i64) -> NodeKind {
    match i {
        0 => NodeKind::File,
        1 => NodeKind::Dir,
        2 => NodeKind::Link,
        _ => NodeKind::Other,
    }
}

fn to_i(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn to_u(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// Snapshot ids are SQLite row ids, shown to the UI as strings.
fn parse_id(id: &str) -> Result<i64> {
    id.parse().map_err(|_| SnapshotError::NotFound)
}

impl SnapshotStore {
    /// Opens (or creates) the store at `path`. Keeps 30 snapshots per root by default.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// An in-memory store (tests, or history turned off).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn, keep: 30 })
    }

    /// Snapshots to keep per root (at least 1). Older ones are pruned on the next save.
    pub fn set_retention(&mut self, keep: usize) {
        self.keep = keep.max(1);
    }

    pub fn retention(&self) -> usize {
        self.keep
    }

    /// Saves the tree's per-folder totals and returns the new snapshot.
    pub fn save(
        &mut self,
        tree: &Tree,
        taken_at: i64,
        usage: Option<VolumeUsage>,
        opts: SaveOptions,
    ) -> Result<SnapshotMeta> {
        let root_id = tree.root_id().ok_or(SnapshotError::EmptyTree)?;
        let root = tree
            .root_display()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let root_node = tree.get(root_id).ok_or(SnapshotError::EmptyTree)?;
        let (allocated, files) = (root_node.totals.allocated, root_node.totals.files);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshots (root, taken_at, allocated, file_count, used_bytes, total_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                root,
                taken_at,
                to_i(allocated),
                to_i(files),
                usage.map(|u| to_i(u.used_bytes)),
                usage.map(|u| to_i(u.total_bytes)),
            ],
        )?;
        let sid = tx.last_insert_rowid();
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO entries (snapshot_id, key, name, kind, category, depth, allocated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            // Depth-first walk building keys incrementally.
            let mut stack: Vec<(NodeId, String, usize)> = vec![(root_id, String::new(), 0)];
            while let Some((id, key, depth)) = stack.pop() {
                let Some(n) = tree.get(id) else { continue };
                let keep = match n.kind() {
                    NodeKind::Dir => !matches!(opts.max_depth, Some(m) if depth > m),
                    NodeKind::File => n.totals.allocated >= opts.min_file_bytes && depth > 0,
                    _ => false,
                };
                if !keep {
                    continue;
                }
                ins.execute(params![
                    sid,
                    key,
                    n.name().to_string_lossy(),
                    kind_to_i(n.kind()),
                    n.category().index() as i64,
                    depth as i64,
                    to_i(n.totals.allocated),
                ])?;
                if n.is_dir() {
                    for &c in n.children() {
                        if let Some(cn) = tree.get(c) {
                            let name = cn.name().to_string_lossy();
                            let ck = if key.is_empty() {
                                name.into_owned()
                            } else {
                                format!("{key}/{name}")
                            };
                            stack.push((c, ck, depth + 1));
                        }
                    }
                }
            }
        }
        tx.commit()?;
        self.prune(&root)?;
        self.meta(&sid.to_string())
    }

    /// Deletes all but the newest `retention()` snapshots of `root`.
    pub fn prune(&mut self, root: &str) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM snapshots WHERE root = ?1 AND id NOT IN (
                 SELECT id FROM snapshots WHERE root = ?1 ORDER BY taken_at DESC, id DESC LIMIT ?2)",
            params![root, self.keep as i64],
        )?;
        Ok(n)
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM snapshots WHERE id = ?1", [parse_id(id)?])?;
        if n == 0 {
            return Err(SnapshotError::NotFound);
        }
        Ok(())
    }

    /// Deletes all history and compacts the file.
    pub fn clear_all(&mut self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM entries; DELETE FROM snapshots; VACUUM;")?;
        Ok(())
    }

    pub fn meta(&self, id: &str) -> Result<SnapshotMeta> {
        self.conn
            .query_row(
                "SELECT id, root, taken_at, allocated, file_count FROM snapshots WHERE id = ?1",
                [parse_id(id)?],
                row_meta,
            )
            .optional()?
            .ok_or(SnapshotError::NotFound)
    }

    /// Snapshots of `root`, newest first.
    pub fn list(&self, root: &str) -> Result<Vec<SnapshotMeta>> {
        let mut st = self.conn.prepare(
            "SELECT id, root, taken_at, allocated, file_count FROM snapshots
             WHERE root = ?1 ORDER BY taken_at DESC, id DESC",
        )?;
        let rows = st.query_map([root], row_meta)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Distinct roots with history.
    pub fn roots(&self) -> Result<Vec<String>> {
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT root FROM snapshots ORDER BY root")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Used bytes over time for `root`, oldest first, up to and including `until` (Unix s).
    /// Uses the volume's used bytes when recorded, else the scanned total.
    pub fn trend(&self, root: &str, until: Option<i64>) -> Result<Vec<TrendPoint>> {
        let mut st = self.conn.prepare(
            "SELECT taken_at, COALESCE(used_bytes, allocated) FROM snapshots
             WHERE root = ?1 AND taken_at <= ?2 ORDER BY taken_at, id",
        )?;
        let rows = st.query_map(params![root, until.unwrap_or(i64::MAX)], |r| {
            Ok(TrendPoint {
                taken_at: r.get(0)?,
                used_bytes: to_u(r.get(1)?),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Days until the volume is full at the current trend, for the newest snapshot of
    /// `root` that recorded the volume size.
    pub fn days_until_full(&self, root: &str, until: Option<i64>) -> Result<Option<f64>> {
        let total: Option<i64> = self
            .conn
            .query_row(
                "SELECT total_bytes FROM snapshots WHERE root = ?1 AND taken_at <= ?2
                 AND total_bytes IS NOT NULL ORDER BY taken_at DESC, id DESC LIMIT 1",
                params![root, until.unwrap_or(i64::MAX)],
                |r| r.get(0),
            )
            .optional()?;
        let Some(total) = total else { return Ok(None) };
        Ok(days_until_full(&self.trend(root, until)?, to_u(total)))
    }

    fn entries(&self, sid: i64) -> Result<HashMap<String, Entry>> {
        let mut st = self.conn.prepare(
            "SELECT key, name, kind, category, depth, allocated FROM entries WHERE snapshot_id = ?1",
        )?;
        let rows = st.query_map([sid], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Entry {
                    name: r.get(1)?,
                    kind: kind_from_i(r.get(2)?),
                    category: Category::from_index(r.get::<_, i64>(3)? as usize),
                    depth: r.get::<_, i64>(4)? as u32,
                    allocated: to_u(r.get(5)?),
                },
            ))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Compares two snapshots of the same root. Entries are the biggest changes first
    /// (at most `limit`); the root itself is not an entry. `grew`/`shrank` sum the changes
    /// of the root's direct children, so nested folders are not counted twice.
    /// With `tree` (the current scan of that root), entries get their node ids.
    pub fn diff(
        &self,
        from_id: &str,
        to_id: &str,
        limit: usize,
        tree: Option<&Tree>,
    ) -> Result<DiffDto> {
        let from = self.meta(from_id)?;
        let to = self.meta(to_id)?;
        if from.root != to.root {
            return Err(SnapshotError::DifferentRoots);
        }
        let (fid, tid) = (parse_id(from_id)?, parse_id(to_id)?);
        let before = self.entries(fid)?;
        let after = self.entries(tid)?;

        struct Change<'a> {
            key: &'a str,
            entry: &'a Entry,
            before: u64,
            after: u64,
            status: DiffStatus,
        }
        let mut changes: Vec<Change> = Vec::new();
        let (mut grew, mut shrank) = (0u64, 0u64);
        let keys: HashSet<&String> = before.keys().chain(after.keys()).collect();
        for key in keys {
            if key.is_empty() {
                continue;
            }
            let (b, a) = (before.get(key), after.get(key));
            let entry = a.or(b).expect("key from one of the maps");
            let (bv, av) = (b.map_or(0, |e| e.allocated), a.map_or(0, |e| e.allocated));
            let status = match (b, a) {
                (None, Some(_)) => DiffStatus::New,
                (Some(_), None) => DiffStatus::Deleted,
                _ if av > bv => DiffStatus::Grew,
                _ if av < bv => DiffStatus::Shrank,
                _ => continue,
            };
            if entry.depth == 1 {
                if av > bv {
                    grew += av - bv;
                } else {
                    shrank += bv - av;
                }
            }
            changes.push(Change {
                key,
                entry,
                before: bv,
                after: av,
                status,
            });
        }
        let delta = |c: &Change| c.after as i128 - c.before as i128;
        changes.sort_by(|x, y| {
            delta(y)
                .abs()
                .cmp(&delta(x).abs())
                .then_with(|| x.key.cmp(y.key))
        });
        changes.truncate(limit);

        // Sparkline window: the last SPARKLINE_POINTS snapshots of this root up to `to`.
        let mut st = self.conn.prepare(
            "SELECT id FROM snapshots WHERE root = ?1 AND taken_at <= ?2
             ORDER BY taken_at DESC, id DESC LIMIT ?3",
        )?;
        let mut window: Vec<i64> = st
            .query_map(
                params![to.root, to.taken_at, SPARKLINE_POINTS as i64],
                |r| r.get(0),
            )?
            .collect::<std::result::Result<_, _>>()?;
        window.reverse();
        let mut lookup = self
            .conn
            .prepare("SELECT allocated FROM entries WHERE snapshot_id = ?1 AND key = ?2")?;

        let root_path = PathBuf::from(&to.root);
        let mut entries = Vec::with_capacity(changes.len());
        for c in &changes {
            let mut sparkline = Vec::with_capacity(window.len());
            for &sid in &window {
                let v: Option<i64> = lookup
                    .query_row(params![sid, c.key], |r| r.get(0))
                    .optional()?;
                sparkline.push(v.map_or(0, to_u));
            }
            let parts: Vec<&str> = c.key.split('/').collect();
            let mut path = root_path.clone();
            for p in &parts {
                path.push(p);
            }
            entries.push(DiffEntryDto {
                id: tree.and_then(|t| t.find(&parts)),
                path: path.to_string_lossy().into_owned(),
                name: c.entry.name.clone(),
                kind: c.entry.kind,
                category: c.entry.category,
                before: c.before,
                after: c.after,
                delta: i64::try_from(delta(c)).unwrap_or(0),
                sparkline,
                status: c.status,
            });
        }

        Ok(DiffDto {
            net_delta: to.allocated_bytes as i64 - from.allocated_bytes as i64,
            grew,
            shrank,
            entries,
            trend: self.trend(&to.root, Some(to.taken_at))?,
            days_until_full: self.days_until_full(&to.root, Some(to.taken_at))?,
            from,
            to,
        })
    }

    /// Per-node change of `tree` against snapshot `id` (for `NodeDto.delta`). Stored
    /// entries get `now - then`; directories missing from the snapshot are new (`+now`).
    /// Small files that were never stored get no delta.
    pub fn deltas(&self, tree: &Tree, id: &str) -> Result<Deltas> {
        let then = self.entries(parse_id(id)?)?;
        let mut out = Deltas::new();
        let Some(root) = tree.root_id() else {
            return Ok(out);
        };
        let mut stack: Vec<(NodeId, String)> = vec![(root, String::new())];
        while let Some((nid, key)) = stack.pop() {
            let Some(n) = tree.get(nid) else { continue };
            let now = n.totals.allocated as i64;
            match then.get(&key) {
                Some(e) => {
                    out.insert(nid, now - e.allocated as i64);
                }
                None if n.is_dir() => {
                    out.insert(nid, now);
                }
                None => {}
            }
            for &c in n.children() {
                if let Some(cn) = tree.get(c) {
                    let name = cn.name().to_string_lossy();
                    let ck = if key.is_empty() {
                        name.into_owned()
                    } else {
                        format!("{key}/{name}")
                    };
                    stack.push((c, ck));
                }
            }
        }
        Ok(out)
    }
}

fn row_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotMeta> {
    Ok(SnapshotMeta {
        id: r.get::<_, i64>(0)?.to_string(),
        root: r.get(1)?,
        taken_at: r.get(2)?,
        allocated_bytes: to_u(r.get(3)?),
        file_count: to_u(r.get(4)?),
    })
}

/// Least-squares growth in bytes per day over `points`, projected to `total_bytes`.
/// `None` if fewer than two points span time, or usage is not growing.
pub fn days_until_full(points: &[TrendPoint], total_bytes: u64) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let xs: Vec<f64> = points
        .iter()
        .map(|p| p.taken_at as f64 / 86_400.0)
        .collect();
    let ys: Vec<f64> = points.iter().map(|p| p.used_bytes as f64).collect();
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    if sxx <= f64::EPSILON {
        return None;
    }
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    if slope <= 0.0 {
        return None;
    }
    let last = *ys.last()?;
    let free = total_bytes as f64 - last;
    Some((free / slope).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{meta, rec, sample};

    const DAY: i64 = 86_400;

    fn grown() -> Tree {
        // Same as sample, but a.mp4 grew to 300, Docs/c.pdf was deleted, New/ appeared.
        let mut r = sample();
        r[3] = rec(3, Some(2), meta("a.mp4", NodeKind::File, 300));
        r.remove(6);
        r.push(rec(7, Some(2), meta("New", NodeKind::Dir, 0)));
        r.push(rec(8, Some(7), meta("n.bin", NodeKind::File, 40)));
        Tree::from_records(r)
    }

    fn opts() -> SaveOptions {
        SaveOptions {
            max_depth: None,
            min_file_bytes: 50,
        }
    }

    #[test]
    fn save_list_diff() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = SnapshotStore::open(dir.path().join("history.db")).unwrap();
        let t1 = Tree::from_records(sample());
        let a = s.save(&t1, 1_000, None, opts()).unwrap();
        assert_eq!(a.allocated_bytes, 160);
        assert_eq!(a.file_count, 3);
        let t2 = grown();
        let b = s.save(&t2, 1_000 + DAY, None, opts()).unwrap();
        let root = a.root.clone();
        let list = s.list(&root).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id);
        assert_eq!(s.roots().unwrap(), vec![root.clone()]);

        let d = s.diff(&a.id, &b.id, 100, Some(&t2)).unwrap();
        assert_eq!(d.net_delta, 350 - 160);
        // Users is the only depth-1 entry.
        assert_eq!(d.grew, 190);
        assert_eq!(d.shrank, 0);
        let by_name: HashMap<_, _> = d.entries.iter().map(|e| (e.name.as_str(), e)).collect();
        assert_eq!(by_name["a.mp4"].status, DiffStatus::Grew);
        assert_eq!(by_name["a.mp4"].delta, 200);
        assert_eq!(by_name["a.mp4"].id, Some(3));
        assert_eq!(by_name["a.mp4"].sparkline, vec![100, 300]);
        assert_eq!(by_name["c.pdf"].status, DiffStatus::Deleted);
        assert_eq!(by_name["c.pdf"].id, None);
        assert_eq!(by_name["Docs"].status, DiffStatus::Shrank);
        assert_eq!(by_name["New"].status, DiffStatus::New);
        assert!(!by_name.contains_key("b.txt"), "small files are not stored");
        assert!(by_name["New"].path.ends_with("New"));
        // Biggest change first.
        assert_eq!(d.entries[0].delta.abs(), 200);
        assert_eq!(s.diff(&a.id, &b.id, 1, None).unwrap().entries.len(), 1);
        assert_eq!(d.trend.len(), 2);
        assert_eq!(d.days_until_full, None);
    }

    #[test]
    fn deltas_for_tree() {
        let mut s = SnapshotStore::open_in_memory().unwrap();
        let a = s
            .save(&Tree::from_records(sample()), 1, None, opts())
            .unwrap();
        let t2 = grown();
        let d = s.deltas(&t2, &a.id).unwrap();
        assert_eq!(d[&0], 190);
        assert_eq!(d[&3], 200);
        assert_eq!(d[&5], -50);
        assert_eq!(d[&7], 40);
        assert!(!d.contains_key(&4));
    }

    #[test]
    fn retention_and_clear() {
        let mut s = SnapshotStore::open_in_memory().unwrap();
        s.set_retention(2);
        let t = Tree::from_records(sample());
        let first = s.save(&t, 1, None, opts()).unwrap();
        s.save(&t, 2, None, opts()).unwrap();
        s.save(&t, 3, None, opts()).unwrap();
        let list = s.list(&first.root).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].taken_at, 2);
        assert!(matches!(s.meta(&first.id), Err(SnapshotError::NotFound)));
        // Entries went with it (cascade).
        let n: i64 = s
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE snapshot_id = ?1",
                [parse_id(&first.id).unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        s.delete(&list[0].id).unwrap();
        assert!(s.delete(&list[0].id).is_err());
        s.clear_all().unwrap();
        assert!(s.list(&first.root).unwrap().is_empty());
        assert!(matches!(s.meta("nonsense"), Err(SnapshotError::NotFound)));
    }

    #[test]
    fn different_roots_refused() {
        let mut s = SnapshotStore::open_in_memory().unwrap();
        let a = s
            .save(&Tree::from_records(sample()), 1, None, opts())
            .unwrap();
        let mut other = Tree::with_root_path("D:\\");
        other.insert(rec(0, None, meta("D:\\", NodeKind::Dir, 0)));
        let b = s.save(&other, 2, None, opts()).unwrap();
        assert!(matches!(
            s.diff(&a.id, &b.id, 10, None),
            Err(SnapshotError::DifferentRoots)
        ));
    }

    #[test]
    fn trend_and_forecast() {
        let mut s = SnapshotStore::open_in_memory().unwrap();
        let t = Tree::from_records(sample());
        let gb = 1u64 << 30;
        for day in 0..5i64 {
            s.save(
                &t,
                day * DAY,
                Some(VolumeUsage {
                    used_bytes: 100 * gb + day as u64 * gb,
                    total_bytes: 200 * gb,
                }),
                opts(),
            )
            .unwrap();
        }
        let root = s.roots().unwrap().remove(0);
        let tr = s.trend(&root, None).unwrap();
        assert_eq!(tr.len(), 5);
        assert_eq!(tr[4].used_bytes, 104 * gb);
        let days = s.days_until_full(&root, None).unwrap().unwrap();
        assert!((days - 96.0).abs() < 1e-6, "{days}");
        // Shrinking or flat usage: no forecast.
        let flat = [
            TrendPoint {
                taken_at: 0,
                used_bytes: 5,
            },
            TrendPoint {
                taken_at: DAY,
                used_bytes: 5,
            },
        ];
        assert_eq!(days_until_full(&flat, 10), None);
        assert_eq!(days_until_full(&flat[..1], 10), None);
    }
}
