<script lang="ts">
  // Hosts the Terrain module (src/terrain, separate owner) through the lazy adapter. Shows a
  // placeholder card while it loads, or if the module isn't available.
  import type { NodeId, TreeDto } from "../ipc/types";
  import type { TerrainHandle, TerrainPin } from "../terrain/types";
  import { app } from "../lib/app.svelte";
  import { loadTerrain } from "../lib/terrain";
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    tree: TreeDto | null;
    pins?: TerrainPin[];
    selPath?: NodeId[] | null;
    compact?: boolean;
    flat?: boolean;
    under?: boolean;
    /** Fly the camera to the selected item. */
    follow?: boolean;
    onSelect?: (id: NodeId) => void;
    onAutoFlat?: () => void;
  }
  let { tree, pins = [], selPath = null, compact = false, flat = false, under = false, follow = false, onSelect, onAutoFlat }: Props = $props();

  let el: HTMLDivElement | undefined = $state();
  let handle: TerrainHandle | null = $state(null);
  let status = $state<"loading" | "ready" | "missing">("loading");
  let lastTree: TreeDto | null = null;

  const hasTree = $derived(!!tree);
  $effect(() => {
    const host = el;
    if (!host || !hasTree) return;
    let cancelled = false;
    let h: TerrainHandle | null = null;
    loadTerrain().then((mount) => {
      if (cancelled) return;
      if (!mount) { status = "missing"; return; }
      untrack(() => {
        const s = app.settings.terrain;
        try {
          h = mount(host, $state.snapshot(tree) as TreeDto, {
            categoryTint: s.categoryTint, pins: $state.snapshot(pins), cloudValleys: s.cloudValleys,
            flat, compact, ringLabels: true, theme: app.theme,
          }, {
            onSelect: (id) => onSelect?.(id),
            onAutoFlat: () => onAutoFlat?.(),
          });
          lastTree = tree;
          handle = h;
          status = "ready";
        } catch (e) {
          console.warn("Terrain failed to start", e);
          status = "missing";
        }
      });
    });
    return () => {
      cancelled = true;
      h?.dispose();
      handle = null;
    };
  });

  // Keep the module in sync with props after mounting.
  $effect(() => {
    if (handle && tree && tree !== lastTree) {
      handle.setData($state.snapshot(tree) as TreeDto);
      lastTree = tree;
    }
  });
  $effect(() => { handle?.setOptions({ theme: app.theme }); });
  $effect(() => {
    const s = app.settings.terrain;
    handle?.setOptions({ categoryTint: s.categoryTint, cloudValleys: s.cloudValleys, pins: $state.snapshot(pins) });
  });
  $effect(() => { handle?.setFlat(flat); });
  $effect(() => { handle?.lookUnder(under); });
  $effect(() => {
    if (!handle) return;
    const id = selPath?.length ? selPath[selPath.length - 1] : null;
    handle.select(id, selPath ? [...selPath] : undefined);
    if (follow && id != null) handle.flyTo(id);
  });
</script>

<div class="terrain-host" bind:this={el}>
  {#if status !== "ready"}
    <div class="ph">
      <Icon name="mountain" size={28} />
      {#if status === "loading"}
        <span>Building terrain…</span>
      {:else}
        <span>Terrain view isn't available in this build</span>
        <span class="faint">The block map, ring chart and file tree show the same data.</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* isolation keeps the terrain overlay (pins, labels) from stacking above dialogs. */
  .terrain-host { position: absolute; inset: 0; border-radius: var(--r-md); overflow: hidden; isolation: isolate; }
  .ph { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 8px; color: var(--text-3);
    background: repeating-radial-gradient(circle at 50% 120%, transparent 0 18px, var(--stroke) 18px 19px); text-align: center; padding: 16px; }
</style>
