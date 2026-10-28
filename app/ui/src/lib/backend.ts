// Dev-only switches for the mock backend (platform, tree size, update and scan simulation), kept in
// localStorage or overridden by URL parameters (?platform=macos&nodes=1000000&fresh).
// Inside the desktop app (Tauri) these are ignored: ipc/index.ts picks the real backend.

import type { Platform } from "../ipc/types";
import type { MockOptions, UpdateSim } from "../ipc/mock";
import type { Backend } from "../ipc/backend";
import { createBackend as pick } from "../ipc";

export interface DevOptions {
  platform: Platform;
  nodes: number;
  fresh: boolean;
  /** Simulated scan length, ms (a long scan makes "busy" easy to test). */
  scanMs: number;
  updateSim: UpdateSim;
  portable: boolean;
}

const KEY = "lustro-dev";
export const NODE_SIZES = [10_000, 60_000, 250_000, 1_000_000];

export function readDevOptions(): DevOptions {
  let stored: Partial<DevOptions> = {};
  try {
    stored = JSON.parse(localStorage.getItem(KEY) || "{}");
  } catch { /* storage unavailable */ }
  const q = new URLSearchParams(location.search);
  const p = q.get("platform") ?? stored.platform;
  const platform: Platform = p === "macos" || p === "linux" || p === "windows" ? p : "windows";
  const n = Number(q.get("nodes") ?? stored.nodes);
  const nodes = Number.isFinite(n) && n >= 1000 ? Math.min(n, 2_000_000) : 60_000;
  const sims: UpdateSim[] = ["ok", "upToDate", "offline", "download", "verify", "unknownSize"];
  const updateSim = sims.includes(stored.updateSim as UpdateSim) ? (stored.updateSim as UpdateSim) : "ok";
  const scanMs = stored.scanMs === 60_000 ? 60_000 : 8000;
  return { platform, nodes, fresh: q.has("fresh"), scanMs, updateSim, portable: q.has("portable") || stored.portable === true };
}

export function writeDevOptions(o: Partial<DevOptions>) {
  try {
    const cur = JSON.parse(localStorage.getItem(KEY) || "{}");
    localStorage.setItem(KEY, JSON.stringify({ ...cur, ...o }));
  } catch { /* storage unavailable */ }
}

export function createBackend(o: DevOptions): Promise<Backend> {
  const mock: MockOptions = {
    platform: o.platform, nodes: o.nodes, fresh: o.fresh, persist: true,
    scanMs: o.scanMs, updateSim: o.updateSim, updateCheckMs: 4000, applyMs: 1500, portable: o.portable,
  };
  return pick(mock);
}
