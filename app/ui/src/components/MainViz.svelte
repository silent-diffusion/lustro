<script lang="ts">
  // The main visualization: block map, ring chart, file tree or terrain, over one folder.
  // Fetches a folded subtree sized for the view, so the browser never sees the whole tree.
  import type { MainView, NodeId, TreeDto } from "../ipc/types";
  import type { TerrainPin } from "../terrain/types";
  import { app, liveLoop, type LiveScan } from "../lib/app.svelte";
  import type { VizNode } from "../lib/viz";
  import CanvasMap, { type Mark } from "./CanvasMap.svelte";
  import RingChart from "./RingChart.svelte";
  import TerrainView from "./TerrainView.svelte";
  import { untrack } from "svelte";

  interface Props {
    rootId: NodeId;
    view: MainView;
    selPath?: NodeId[] | null;
    focusPath?: NodeId[] | null;
    marks?: Map<NodeId, Mark> | null;
    colorBy?: "cat" | "delta";
    maxDepth?: number;
    pins?: TerrainPin[];
    flat?: boolean;
    under?: boolean;
    follow?: boolean;
    onSelect?: (id: NodeId, n?: VizNode) => void;
    onOpen?: (n: VizNode) => void;
    onAutoFlat?: () => void;
    /** Draw from this running (or stopped) scan instead of the backend: Overview and Explore during a scan. */
    live?: LiveScan | null;
    /** Terrain only: the camera arrived at, or left, the full view. */
    onFullViewChange?: (full: boolean) => void;
  }
  let { rootId, view, selPath = null, focusPath = null, marks = null, colorBy = "cat", maxDepth, pins = [], flat = false, under = false, follow = false, onSelect, onOpen, onAutoFlat, onFullViewChange, live = null }: Props = $props();
  let terrain: TerrainView | undefined = $state();

  /** Terrain only: animate back to the full view. */
  export function resetView() {
    terrain?.resetView();
  }

  const PARAMS: Record<MainView, [number, number]> = {
    blockMap: [7, 0.0002], ringChart: [4, 0.002], fileTree: [5, 0.001], terrain: [8, 0.0002],
  };

  let tree = $state.raw<TreeDto | null>(null);
  let map: CanvasMap | undefined = $state();

  $effect(() => {
    void app.version;
    const [depth, frac] = PARAMS[view];
    if (live) {
      // Scanning: build from the scan mirror, at most 4 times a second (terrain: once a second, since it
      // rebuilds its heightfield). The root may not be in the mirror yet; fall back to the scan's root.
      const build = () => {
        const id = live.nodes.has(rootId) ? rootId : live.rootId;
        if (id != null) tree = live.tree(id, depth, frac);
      };
      untrack(build);
      if (!live.running) return;
      return liveLoop(build, view === "terrain" ? 1000 : 250);
    }
    let stale = false;
    app.backend.subtree(rootId, depth, frac).then((t) => { if (!stale) tree = t; });
    return () => { stale = true; };
  });

  export function rectOf(path: NodeId[]) {
    return map?.rectOf(path) ?? null;
  }

  // Folded "smaller items" nodes have negative ids: selecting one selects its folder.
  const pick = (n: VizNode) => onSelect?.(n.id < 0 ? -n.id - 1 : n.id, n);
</script>

{#if view === "blockMap"}
  <CanvasMap bind:this={map} {tree} mode="blocks" maxDepth={maxDepth ?? 4} {colorBy} {selPath} {focusPath} {marks} onSelect={pick} {onOpen} />
{:else if view === "fileTree"}
  <CanvasMap bind:this={map} {tree} mode="icicle" label="File tree" {selPath} {focusPath} {marks} onSelect={pick} {onOpen} />
{:else if view === "ringChart"}
  <RingChart {tree} {selPath} {focusPath} {marks} onSelect={pick} {onOpen} />
{:else}
  <TerrainView bind:this={terrain} {tree} {pins} selPath={selPath ?? focusPath} {flat} {under} {follow} onSelect={(id) => onSelect?.(id < 0 ? -id - 1 : id)} {onAutoFlat} {onFullViewChange} />
{/if}
