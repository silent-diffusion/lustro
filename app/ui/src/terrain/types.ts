// CONTRACT FILE: the terrain module's public API (see docs/CONTRACTS.md).
// Implemented in `src/terrain/index.ts`; used by the screens.

import type { NodeId, TreeDto } from "../ipc/types";

export type PinKind = "largest" | "grew" | "duplicate" | "cloud";

export interface TerrainPin { kind: PinKind; nodeId: NodeId; label: string }

export interface TerrainOptions {
  /** Tint ground by dominant category. */
  categoryTint: boolean;
  pins: TerrainPin[];
  /** Draw cloud-only data as valleys below ground. */
  cloudValleys: boolean;
  /** Start in the 2D flat contour map. */
  flat: boolean;
  /** Small card version (Overview hero): fewer details, auto-rotate. */
  compact: boolean;
  /** Labels around the compass ring for top-level children. */
  ringLabels: boolean;
  theme: "dark" | "light";
  /**
   * Slow "looking around" orbit. Defaults to `compact`. It never stops on its own: interaction
   * pauses it, and it eases back in after a few idle seconds, but only at the full view with
   * nothing selected. Always off with prefers-reduced-motion.
   */
  autoRotate?: boolean;
}

export interface TerrainEvents {
  onSelect?: (id: NodeId) => void;
  onHover?: (id: NodeId | null) => void;
  /** Fired when the module switches to 2D because frame rate stayed low. */
  onAutoFlat?: () => void;
  /**
   * Fired when the camera arrives at, or leaves, the full view. Not at full view = after a fly-to (follow, pin click), or the target
   * is more than 2% of the disc radius from home, or the zoom distance is more than 8% off home,
   * or the 2D map is zoomed or panned. Orbiting alone at home distance and target is full view.
   */
  onFullViewChange?: (full: boolean) => void;
}

export interface TerrainHandle {
  /** New data (live scans call this about once a second). Keeps the camera; heights ease in over 0.4s. */
  setData(tree: TreeDto): void;
  setOptions(opts: Partial<TerrainOptions>): void;
  /** Highlight a node and draw the dashed route to it from the root. */
  select(id: NodeId | null, path?: NodeId[]): void;
  /** Animate the camera to frame a node. */
  flyTo(id: NodeId): void;
  setFlat(flat: boolean): void;
  lookUnder(on: boolean): void;
  /**
   * Animate (600ms) back to the full view in the current mode: 3D home, the full 2D map, or the
   * full Look-under view (the design's `home()`). The selection highlight and route stay;
   * auto-rotate resumes after the normal idle delay.
   */
  resetView(): void;
  /** Whether the camera currently shows the full view of the whole map. */
  isFullView(): boolean;
  resize(): void;
  dispose(): void;
}

export type MountTerrain = (el: HTMLElement, tree: TreeDto, opts: TerrainOptions, events?: TerrainEvents) => TerrainHandle;
