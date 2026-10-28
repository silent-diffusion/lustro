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
}

export interface TerrainEvents {
  onSelect?: (id: NodeId) => void;
  onHover?: (id: NodeId | null) => void;
  /** Fired when the module switches to 2D because frame rate stayed low. */
  onAutoFlat?: () => void;
}

export interface TerrainHandle {
  setData(tree: TreeDto): void;
  setOptions(opts: Partial<TerrainOptions>): void;
  /** Highlight a node and draw the dashed route to it from the root. */
  select(id: NodeId | null, path?: NodeId[]): void;
  /** Animate the camera to frame a node. */
  flyTo(id: NodeId): void;
  setFlat(flat: boolean): void;
  lookUnder(on: boolean): void;
  resize(): void;
  dispose(): void;
}

export type MountTerrain = (el: HTMLElement, tree: TreeDto, opts: TerrainOptions, events?: TerrainEvents) => TerrainHandle;
