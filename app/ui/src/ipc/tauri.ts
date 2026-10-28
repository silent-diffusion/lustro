// Desktop-app `Backend`: every method is a Tauri command of the same name in snake_case (docs/CONTRACTS.md).
// Argument keys are camelCase; Tauri maps them to the snake_case Rust parameters. Results are the
// types.ts DTOs as-is. Commands reject with a plain message string, which is wrapped in an Error here.
// No Tauri plugins are used from the webview: updates, trash and reveal all go through our own commands.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Backend, Filter, SortKey, Unsubscribe } from "./backend";
import type {
  ApplyRequest, ApplyResult, BuildInfo, CloudProviderSummary, DiffDto, NodeDetailDto, NodeDto, NodeId, Platform, ScanDone,
  ScanRequest, ScanUpdate, Settings, SnapshotMeta, SpaceBreakdownDto, SuggestionDto, TreeDto, UndoEntry,
  UpdateStatus, VolumeInfo,
} from "./types";

export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type Listen = <T>(event: string, cb: (payload: T) => void) => Promise<UnlistenFn>;

export const EVENTS = { scanUpdate: "scan-update", scanDone: "scan-done", updateStatus: "update-status" } as const;

/** Commands reject with a string; give callers a real Error. */
function wrap(invoke: Invoke): Invoke {
  return async <T>(cmd: string, args?: Record<string, unknown>) => {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : `${cmd} failed`);
    }
  };
}

export class TauriBackend implements Backend {
  private call: Invoke;
  private on: Listen;

  /** `invoke` and `listen` are injectable for tests; by default they come from @tauri-apps/api. */
  constructor(invoke: Invoke = tauriInvoke as Invoke, onEvent: Listen = (e, cb) => listen(e, (ev) => cb(ev.payload as never))) {
    this.call = wrap(invoke);
    this.on = onEvent;
  }

  /** Bridge Tauri's async `listen` to the synchronous Unsubscribe in the contract. */
  private subscribe<T>(event: string, cb: (p: T) => void): Unsubscribe {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    this.on<T>(event, cb).then(
      (u) => { if (cancelled) u(); else unlisten = u; },
      (e) => console.warn(`listen ${event} failed`, e),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }

  platform() { return this.call<Platform>("platform"); }
  listVolumes() { return this.call<VolumeInfo[]>("list_volumes"); }

  startScan(req: ScanRequest) { return this.call<string>("start_scan", { req }); }
  cancelScan(scanId: string) { return this.call<void>("cancel_scan", { scanId }); }
  onScanUpdate(cb: (u: ScanUpdate) => void) { return this.subscribe(EVENTS.scanUpdate, cb); }
  onScanDone(cb: (d: ScanDone) => void) { return this.subscribe(EVENTS.scanDone, cb); }

  root() { return this.call<NodeDto | null>("root"); }
  children(id: NodeId, sort: SortKey, offset: number, limit: number) { return this.call<NodeDto[]>("children", { id, sort, offset, limit }); }
  node(id: NodeId) { return this.call<NodeDetailDto>("node", { id }); }
  pathTo(id: NodeId) { return this.call<NodeDto[]>("path_to", { id }); }
  subtree(id: NodeId, maxDepth: number, minFraction: number) { return this.call<TreeDto>("subtree", { id, maxDepth, minFraction }); }
  top(kind: "files" | "dirs", n: number, filter: Filter) { return this.call<NodeDto[]>("top", { kind, n, filter }); }
  search(filter: Filter, limit: number) { return this.call<NodeDto[]>("search", { filter, limit }); }
  spaceBreakdown(volumeId: string) { return this.call<SpaceBreakdownDto>("space_breakdown", { volumeId }); }
  cloudSummary() { return this.call<CloudProviderSummary[]>("cloud_summary"); }

  listSnapshots(root: string) { return this.call<SnapshotMeta[]>("list_snapshots", { root }); }
  diff(fromId: string, toId: string, limit: number) { return this.call<DiffDto>("diff", { fromId, toId, limit }); }

  suggestions() { return this.call<SuggestionDto[]>("suggestions"); }
  applyCleanup(req: ApplyRequest) { return this.call<ApplyResult>("apply_cleanup", { req }); }
  undoLog() { return this.call<UndoEntry[]>("undo_log"); }
  undo(undoId: string) { return this.call<void>("undo", { undoId }); }

  reveal(id: NodeId) { return this.call<void>("reveal", { id }); }
  openSetting(action: string) { return this.call<void>("open_setting", { action }); }
  getSettings() { return this.call<Settings>("get_settings"); }
  setSettings(settings: Settings) { return this.call<void>("set_settings", { settings }); }
  clearHistory() { return this.call<void>("clear_history"); }

  appVersion() { return this.call<string>("app_version"); }
  checkForUpdate() { return this.call<UpdateStatus>("check_for_update"); }
  installUpdate() { return this.call<void>("install_update"); }
  currentReleaseNotes() { return this.call<string>("current_release_notes"); }
  restartToUpdate() { return this.call<void>("restart_to_update"); }
  skipVersion(version: string) { return this.call<void>("skip_version", { version }); }

  buildInfo() { return this.call<BuildInfo>("build_info"); }

  getUpdateStatus() { return this.call<UpdateStatus>("update_status"); }
  onUpdateStatus(cb: (s: UpdateStatus) => void): Unsubscribe { return this.subscribe(EVENTS.updateStatus, cb); }

  /** Native folder picker (Rust side). The picked path is then allowed as a scan root, an excluded or
   *  protected path, or a Move destination. Resolves null when cancelled. */
  pickFolder() { return this.call<string | null>("pick_folder"); }
}
