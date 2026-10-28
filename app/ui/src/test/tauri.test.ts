import { describe, expect, it, vi } from "vitest";
import { TauriBackend, type Invoke, type Listen } from "../ipc/tauri";
import { createBackend, inTauri } from "../ipc";
import { DEFAULT_SETTINGS } from "../ipc/defaults";
import type { UpdateStatus } from "../ipc/types";

function fake(results: Record<string, unknown> = {}) {
  const calls: [string, Record<string, unknown> | undefined][] = [];
  const invoke: Invoke = async <T,>(cmd: string, args?: Record<string, unknown>) => {
    calls.push([cmd, args]);
    if (cmd in results) {
      const r = results[cmd];
      if (r instanceof Error) throw r.message; // Tauri rejects with a plain string
      return r as T;
    }
    return undefined as T;
  };
  const handlers = new Map<string, (p: unknown) => void>();
  const unlistened: string[] = [];
  let release: () => void = () => {};
  const gate = new Promise<void>((r) => (release = r));
  const listen: Listen = async <T,>(event: string, cb: (p: T) => void) => {
    await gate;
    handlers.set(event, cb as (p: unknown) => void);
    return () => { unlistened.push(event); handlers.delete(event); };
  };
  return { calls, invoke, listen, handlers, unlistened, release };
}

describe("TauriBackend", () => {
  it("maps every method to its snake_case command with camelCase args", async () => {
    const f = fake();
    const b = new TauriBackend(f.invoke, f.listen);
    const req = { root: "C:\\", mode: "standard" as const, crossFilesystems: false, includeRemote: false, excluded: [] };
    const apply = { decisions: [], destination: { kind: "trash" as const } };
    await b.platform();
    await b.listVolumes();
    await b.startScan(req);
    await b.cancelScan("scan-1");
    await b.root();
    await b.children(4, "name", 10, 50);
    await b.node(4);
    await b.pathTo(4);
    await b.subtree(0, 7, 0.0002);
    await b.top("files", 10, { minBytes: 1 });
    await b.search({ text: "x" }, 100);
    await b.spaceBreakdown("C:");
    await b.cloudSummary();
    await b.listSnapshots("C:\\");
    await b.diff("a", "b", 200);
    await b.suggestions();
    await b.applyCleanup(apply);
    await b.undoLog();
    await b.undo("u1");
    await b.reveal(9);
    await b.openSetting("windows:power");
    await b.getSettings();
    await b.setSettings(DEFAULT_SETTINGS);
    await b.clearHistory();
    await b.appVersion();
    await b.checkForUpdate();
    await b.installUpdate();
    await b.currentReleaseNotes();
    await b.restartToUpdate();
    await b.skipVersion("0.3.0");
    await b.pickFolder();
    await b.getUpdateStatus();
    expect(f.calls).toEqual([
      ["platform", undefined], ["list_volumes", undefined], ["start_scan", { req }], ["cancel_scan", { scanId: "scan-1" }],
      ["root", undefined], ["children", { id: 4, sort: "name", offset: 10, limit: 50 }], ["node", { id: 4 }], ["path_to", { id: 4 }],
      ["subtree", { id: 0, maxDepth: 7, minFraction: 0.0002 }], ["top", { kind: "files", n: 10, filter: { minBytes: 1 } }],
      ["search", { filter: { text: "x" }, limit: 100 }], ["space_breakdown", { volumeId: "C:" }], ["cloud_summary", undefined],
      ["list_snapshots", { root: "C:\\" }], ["diff", { fromId: "a", toId: "b", limit: 200 }], ["suggestions", undefined],
      ["apply_cleanup", { req: apply }], ["undo_log", undefined], ["undo", { undoId: "u1" }], ["reveal", { id: 9 }],
      ["open_setting", { action: "windows:power" }], ["get_settings", undefined], ["set_settings", { settings: DEFAULT_SETTINGS }],
      ["clear_history", undefined], ["app_version", undefined], ["check_for_update", undefined], ["install_update", undefined],
      ["current_release_notes", undefined], ["restart_to_update", undefined], ["skip_version", { version: "0.3.0" }],
      ["pick_folder", undefined], ["update_status", undefined],
    ]);
  });

  it("reads build_info", async () => {
    const f = fake({ build_info: { portable: true } });
    expect(await new TauriBackend(f.invoke, f.listen).buildInfo()).toEqual({ portable: true });
    expect(f.calls).toEqual([["build_info", undefined]]);
  });

  it("turns string rejections into Errors", async () => {
    const f = fake({ node: new Error("unknown node") });
    const b = new TauriBackend(f.invoke, f.listen);
    await expect(b.node(1)).rejects.toThrow("unknown node");
  });

  it("bridges async listen to a synchronous unsubscribe", async () => {
    const f = fake();
    const b = new TauriBackend(f.invoke, f.listen);
    const got: unknown[] = [];
    const off = b.onScanUpdate((u) => got.push(u));
    const offEarly = b.onScanDone(() => got.push("done"));
    offEarly(); // before listen resolved
    f.release();
    await vi.waitFor(() => expect(f.handlers.has("scan-update")).toBe(true));
    expect(f.unlistened).toContain("scan-done");
    f.handlers.get("scan-update")!({ scanId: "s" });
    expect(got).toEqual([{ scanId: "s" }]);
    off();
    expect(f.unlistened).toContain("scan-update");
  });

  it("forwards update-status events", async () => {
    const f = fake();
    const b = new TauriBackend(f.invoke, f.listen);
    const got: UpdateStatus[] = [];
    b.onUpdateStatus((s) => got.push(s));
    f.release();
    await vi.waitFor(() => expect(f.handlers.has("update-status")).toBe(true));
    f.handlers.get("update-status")!({ state: "checking" });
    expect(got).toEqual([{ state: "checking" }]);
  });
});

describe("backend selection", () => {
  it("uses the mock outside Tauri", async () => {
    expect(inTauri()).toBe(false);
    const b = await createBackend({ nodes: 2000 });
    expect(b.constructor.name).toBe("MockBackend");
    expect(await b.pickFolder()).toBe("C:\\Users\\William\\Projects");
  });
});
