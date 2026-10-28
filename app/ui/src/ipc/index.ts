// Picks the backend: Tauri commands inside the desktop app, the mock in a plain browser (dev flow).
// Both are loaded lazily so each build only pulls in what it uses.

import { isTauri } from "@tauri-apps/api/core";
import type { Backend } from "./backend";
import type { MockOptions } from "./mock";

export function inTauri(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

export async function createBackend(mockOptions: MockOptions): Promise<Backend> {
  if (inTauri()) {
    const { TauriBackend } = await import("./tauri");
    return new TauriBackend();
  }
  const { MockBackend } = await import("./mock");
  return new MockBackend(mockOptions);
}
