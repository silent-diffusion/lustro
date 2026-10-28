// Small adapter around the Terrain module (`src/terrain`). It loads the module lazily,
// so three.js stays out of the main bundle, and returns null when the module isn't there yet:
// callers then show a placeholder instead.

import type { MountTerrain } from "../terrain/types";

const modules = import.meta.glob("../terrain/index.ts");

let cached: Promise<MountTerrain | null> | null = null;

export function loadTerrain(): Promise<MountTerrain | null> {
  if (!cached) {
    const load = modules["../terrain/index.ts"];
    cached = !load
      ? Promise.resolve(null)
      : load()
          .then((m) => (m as { mountTerrain?: MountTerrain }).mountTerrain ?? null)
          .catch((e) => {
            console.warn("Terrain module failed to load", e);
            return null;
          });
  }
  return cached;
}
