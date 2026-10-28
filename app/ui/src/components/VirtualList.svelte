<script lang="ts" generics="T">
  // Fixed-row-height virtual list: only the rows in view (plus a small margin) are in the DOM.
  import type { Snippet } from "svelte";

  interface Props {
    items: readonly T[];
    rowHeight: number;
    row: Snippet<[T, number]>;
    overscan?: number;
    label?: string;
  }
  let { items, rowHeight, row, overscan = 8, label }: Props = $props();

  let viewport: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let height = $state(0);

  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan));
  const visible = $derived(items.slice(start, end));

  /** Scroll so the row at `index` is in view. */
  export function scrollToIndex(index: number) {
    if (!viewport) return;
    const top = index * rowHeight;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + rowHeight > viewport.scrollTop + viewport.clientHeight) viewport.scrollTop = top + rowHeight - viewport.clientHeight;
  }
</script>

<div class="vlist" bind:this={viewport} bind:clientHeight={height} onscroll={() => (scrollTop = viewport!.scrollTop)} role="list" aria-label={label}>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    <div class="win" style:transform="translateY({start * rowHeight}px)">
      <!-- Unkeyed on purpose: rows are reused by position and always show the current item. -->
      {#each visible as item, i}
        <div class="vrow" style:height="{rowHeight}px" role="listitem">{@render row(item, start + i)}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .vlist { position: absolute; inset: 0; overflow-y: auto; overflow-x: hidden; }
  .spacer { position: relative; }
  .win { position: absolute; left: 0; right: 0; top: 0; }
  .vrow { overflow: hidden; }
</style>
