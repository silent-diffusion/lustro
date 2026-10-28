<script lang="ts">
  import type { Category } from "../ipc/types";
  import { CATEGORIES, CATEGORY_LABEL, catVar } from "../lib/labels";
  import CloudMark from "./CloudMark.svelte";
  let { cats = CATEGORIES, cloud = true }: { cats?: Category[]; cloud?: boolean } = $props();
</script>

<div class="legend">
  {#each cats as c (c)}
    <span><span class="dot" style:background="var({catVar(c)})"></span>{CATEGORY_LABEL[c]}</span>
  {/each}
  {#if cloud}
    <span><span class="dot cloud"></span>Cloud only</span>
    <span><CloudMark cloud={{ provider: "oneDrive", state: "downloaded" }} /> Downloaded</span>
    <span><CloudMark cloud={{ provider: "oneDrive", state: "pinned" }} /> Always on device</span>
  {/if}
</div>
