// Default settings: what a fresh install starts with (the Rust side mirrors these).

import type { Settings } from "./types";

export const DEFAULT_SETTINGS: Settings = {
  mainView: "blockMap",
  theme: "dark",
  density: "comfortable",
  scanMode: "standard",
  excluded: [],
  countCloudOnlyInTotals: false,
  hiddenProviders: [],
  keepSnapshots: true,
  snapshotsToKeep: 12,
  terrain: { categoryTint: true, pins: { largest: true, grew: true, duplicates: true, cloud: true }, cloudValleys: true, prefer2d: false },
  keptItems: [],
  protectedPaths: [],
  findDuplicates: true,
  autoUpdate: true,
  skippedVersion: null,
  lastUpdateCheck: null,
  notesShownForVersion: null,
};
