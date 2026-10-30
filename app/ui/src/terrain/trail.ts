// The route's trail card: one hop per laid-out folder from the root down to the selection.
// Pure functions only (no DOM) so they can be unit-tested.

import type { NodeId } from "../ipc/types";
import type { Layout } from "./layout";
import { chainOf } from "./layout";

export interface TrailHop {
  /** Index in `Layout.nodes`. */
  idx: number;
  id: NodeId;
  name: string;
  /** Bytes on this disk plus cloud-only bytes, as the map sizes it (cloud-only data shows as lakes). */
  bytes: number;
  /** Cloud-only part of `bytes`. */
  cloudOnly: number;
  /** Mostly cloud only: the card labels the size as cloud only. */
  cloud: boolean;
  /** Share of the parent hop's bytes, 0..1 (null for the root). */
  share: number | null;
  parentName: string | null;
}

export interface Trail {
  hops: TrailHop[];
  /**
   * Levels below the last hop that are too small to draw (the selection sits on its nearest
   * drawn folder): 0 when the selection itself is on the map, -1 when the count is unknown.
   */
  hidden: number;
}

/** Display name of a laid-out node: folded "smaller items" patches have no source node. */
export function hopName(layout: Layout, idx: number): string {
  const n = layout.nodes[idx];
  if (n.src) return n.src.name;
  const parent = layout.nodes[n.parent];
  return `Smaller items in ${parent?.src?.name ?? "this folder"}`;
}

/** The trail to `idx` (the selection, or its nearest drawn ancestor). `path` is root → selection. */
export function trailOf(layout: Layout, idx: number, selId: NodeId, path?: NodeId[]): Trail {
  const total = (k: number) => layout.nodes[k].allocated + layout.nodes[k].cloudOnly;
  const hops = chainOf(layout, idx).map((k, i, chain): TrailHop => {
    const n = layout.nodes[k];
    const bytes = total(k);
    const pBytes = i > 0 ? total(chain[i - 1]) : 0;
    return {
      idx: k,
      id: n.id,
      name: hopName(layout, k),
      bytes,
      cloudOnly: n.cloudOnly,
      cloud: n.cloudOnly > n.allocated,
      share: i > 0 ? (pBytes > 0 ? Math.min(1, bytes / pBytes) : 0) : null,
      parentName: i > 0 ? hopName(layout, chain[i - 1]) : null,
    };
  });
  let hidden = 0;
  const last = layout.nodes[idx];
  if (last && last.id !== selId) {
    const at = path ? path.lastIndexOf(last.id) : -1;
    hidden = at >= 0 ? path!.length - 1 - at : -1;
  }
  return { hops, hidden };
}

/**
 * Which hops to list when the trail is long: the root and first folder, a gap, then the last
 * three (the hops nearest the selection matter most). `null` marks the gap.
 */
export function visibleHops(count: number, max = 6): (number | null)[] {
  const all = Array.from({ length: count }, (_, i) => i);
  if (count <= max) return all;
  return [0, 1, null, count - 3, count - 2, count - 1];
}

/** "51%", "<1%", "100%". */
export function fmtShare(s: number): string {
  if (s > 0 && s < 0.01) return "<1%";
  return `${Math.round(s * 100)}%`;
}
