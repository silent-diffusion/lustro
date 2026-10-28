//! "Where did my space go?": one bar that adds up to the volume's used space. Scanned
//! categories, system items, cloud-only files (0 bytes on disk, shown for context) and the
//! unexplained rest.

use std::collections::HashMap;
use std::path::Path;

use lustro_core::category::Category;
use lustro_core::dto::{SegmentKind, SpaceBreakdownDto, SpaceSegment};
use lustro_core::tree::Tree;
use lustro_platform::system_space::SystemSpaceItem;

/// Where a system item lives below the volume root, so its bytes come out of the scanned
/// categories instead of being counted twice.
fn tree_location(key: &str) -> &'static [&'static [&'static str]] {
    match key {
        "pagefile" => &[&["pagefile.sys"], &["swapfile.sys"]],
        "hibernation" => &[&["hiberfil.sys"]],
        "winsxs" => &[&["Windows", "WinSxS"]],
        "recycleBin" => &[&["$Recycle.Bin"]],
        "restorePoints" => &[&["System Volume Information"]],
        _ => &[],
    }
}

fn find_ci(tree: &Tree, parts: &[&str]) -> Option<u32> {
    let mut cur = tree.root_id()?;
    for p in parts {
        let n = tree.get(cur)?;
        cur = n.children().iter().copied().find(|&c| {
            tree.get(c)
                .is_some_and(|x| x.name().to_string_lossy().eq_ignore_ascii_case(p))
        })?;
    }
    Some(cur)
}

fn label(c: Category) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Breakdown for a volume with `used_bytes` in use. `tree` is the current scan if it lies on
/// this volume; `scanned_whole` says whether it covers the whole volume.
pub fn breakdown(
    volume_id: &str,
    used_bytes: u64,
    tree: Option<(&Tree, bool)>,
    system: &[SystemSpaceItem],
) -> SpaceBreakdownDto {
    let Some((tree, whole)) = tree.filter(|(t, _)| t.root().is_some()) else {
        return SpaceBreakdownDto {
            volume_id: volume_id.into(),
            used_bytes,
            segments: vec![SpaceSegment {
                kind: SegmentKind::Unexplained,
                key: "unscanned".into(),
                label: "Not scanned".into(),
                bytes: used_bytes,
                explanation: "This drive hasn't been scanned yet.".into(),
                setting_action: None,
            }],
        };
    };
    let root = tree.root().expect("checked above");
    let mut by_cat: HashMap<Category, u64> = root.category_mix().into_iter().collect();

    let mut sys = Vec::new();
    for item in system {
        let mut in_tree = 0u64;
        if whole {
            for loc in tree_location(item.key) {
                if let Some(n) = find_ci(tree, loc).and_then(|id| tree.get(id)) {
                    in_tree += n.totals.allocated;
                    for (c, b) in n.category_mix() {
                        if let Some(v) = by_cat.get_mut(&c) {
                            *v = v.saturating_sub(b);
                        }
                    }
                }
            }
        }
        let bytes = item.bytes.unwrap_or(0).max(in_tree);
        if bytes == 0 {
            continue;
        }
        sys.push(SpaceSegment {
            kind: SegmentKind::System,
            key: item.key.into(),
            label: item.label.into(),
            bytes,
            explanation: item.explanation.into(),
            setting_action: item.setting_action.map(str::to_owned),
        });
    }
    sys.sort_by_key(|s| std::cmp::Reverse(s.bytes));

    let mut cats: Vec<SpaceSegment> = by_cat
        .into_iter()
        .filter(|(_, b)| *b > 0)
        .map(|(c, bytes)| SpaceSegment {
            kind: SegmentKind::Category,
            key: label(c),
            label: label(c),
            bytes,
            explanation: String::new(),
            setting_action: None,
        })
        .collect();
    cats.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.key.cmp(&b.key)));

    let explained: u64 = cats.iter().chain(sys.iter()).map(|s| s.bytes).sum();
    let rest = used_bytes.saturating_sub(explained);
    let mut segments = cats;
    segments.extend(sys);
    segments.push(SpaceSegment {
        kind: SegmentKind::CloudOnly,
        key: "cloudOnly".into(),
        label: "Cloud-only files".into(),
        bytes: root.totals.cloud_only,
        explanation: "Listed in folders but stored only in the cloud: 0 bytes on this disk. Not counted in used space.".into(),
        setting_action: None,
    });
    segments.push(SpaceSegment {
        kind: SegmentKind::Unexplained,
        key: if whole { "unexplained" } else { "unscanned" }.into(),
        label: if whole { "Unexplained" } else { "Outside the scanned folder" }.into(),
        bytes: rest,
        explanation: if whole {
            "Space in use that the scan couldn't attribute: usually folders it wasn't allowed to read, or file system metadata."
        } else {
            "Only one folder of this drive was scanned. The rest of the used space is here."
        }
        .into(),
        setting_action: None,
    });
    SpaceBreakdownDto {
        volume_id: volume_id.into(),
        used_bytes,
        segments,
    }
}

/// True when `root` is the volume's root folder itself.
pub fn is_volume_root(root: &Path, mount: &Path) -> bool {
    crate::util::same_path(root, mount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lustro_core::fixtures::Fixture;

    fn item(key: &'static str, bytes: Option<u64>) -> SystemSpaceItem {
        SystemSpaceItem {
            kind: "system",
            key,
            label: "Item",
            bytes,
            explanation: "",
            setting_action: Some("windows:storage"),
        }
    }

    #[test]
    fn not_scanned() {
        let b = breakdown("C:", 100, None, &[]);
        assert_eq!(b.segments.len(), 1);
        assert_eq!(b.segments[0].bytes, 100);
    }

    #[test]
    fn adds_up_to_used_space() {
        let tree = Fixture::small().tree();
        let total = tree.root().unwrap().totals.allocated;
        let used = total + 5_000_000;
        let b = breakdown(
            "C:",
            used,
            Some((&tree, true)),
            &[
                item("pagefile", Some(1_000_000)),
                item("restorePoints", None),
            ],
        );
        let on_disk: u64 = b
            .segments
            .iter()
            .filter(|s| s.kind != SegmentKind::CloudOnly)
            .map(|s| s.bytes)
            .sum();
        assert_eq!(on_disk, used);
        // Unknown sizes are left out.
        assert!(!b.segments.iter().any(|s| s.key == "restorePoints"));
        assert!(b.segments.iter().any(|s| s.key == "pagefile"));
        assert!(b.segments.iter().any(|s| s.kind == SegmentKind::CloudOnly));
    }
}
