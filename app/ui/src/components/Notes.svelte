<script lang="ts">
  // Release notes, rendered from the safe subset in lib/notes.ts as text nodes only (no {@html}).
  import { parseNotes, type Inline } from "../lib/notes";
  let { src, columns = false, dim = false }: { src: string; columns?: boolean; dim?: boolean } = $props();
  const blocks = $derived(parseNotes(src));
</script>

{#snippet inline(c: Inline[])}{#each c as x, i (i)}{#if x.t === "b"}<b>{x.s}</b>{:else if x.t === "code"}<code>{x.s}</code>{:else}{x.s}{/if}{/each}{/snippet}

<div class="notes" class:columns class:dim>
  {#each blocks as b, i (i)}
    {#if b.t === "h"}<h4>{@render inline(b.c)}</h4>
    {:else if b.t === "p"}<p>{@render inline(b.c)}</p>
    {:else}<ul>{#each b.items as it, j (j)}<li>{@render inline(it)}</li>{/each}</ul>{/if}
  {/each}
</div>

<style>
  .notes { font-size: var(--fs-sm); color: var(--text-2); overflow-wrap: anywhere; }
  .notes.columns { columns: 2 280px; column-gap: 32px; }
  h4 { margin: 14px 0 6px; font-size: var(--fs-xs); text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); break-after: avoid; }
  ul { margin: 0; padding-left: 18px; }
  li { margin: 4px 0; }
  p { margin: 6px 0; }
  b { color: var(--text); font-weight: var(--fw-strong); }
  code { font-family: var(--font-mono); font-size: .92em; background: var(--surface-3); padding: 1px 5px; border-radius: 4px; }
</style>
