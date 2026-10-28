<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { CATEGORY_LABEL, CLOUD_STATE, PROVIDER, catVar } from "../lib/labels";

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    const t = app.tip;
    if (!t || !el) return;
    const pad = 14, r = el.getBoundingClientRect();
    let x = t.x + pad, y = t.y + pad;
    if (x + r.width > innerWidth - 8) x = t.x - r.width - pad;
    if (y + r.height > innerHeight - 8) y = t.y - r.height - pad;
    pos = { left: x, top: y };
  });

  const cloudOnly = (n: { allocated: number; cloudOnly: number }) => n.allocated === 0 && n.cloudOnly > 0;
</script>

{#if app.tip}
  {@const t = app.tip}
  <div class="tip" bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px">
    {#if t.kind === "node"}
      {@const n = t.node}
      <b>{n.name}</b>
      {#if t.path}<span class="mono faint">{t.path}</span><br />{/if}
      {#if cloudOnly(n)}
        <span class="dot cloud"></span> Cloud only · <span class="mono">{app.fb(n.cloudOnly)}</span>
      {:else}
        <span class="dot" style:background="var({catVar(n.category)})"></span> {CATEGORY_LABEL[n.category]} ·
        <span class="mono">{app.fb(n.allocated)}</span>{#if n.cloudOnly > 0}<span class="faint"> + {app.fb(n.cloudOnly)} cloud only</span>{/if}
      {/if}
      {#if n.cloud}
        <br /><span style:color="var(--cat-cloud)">{PROVIDER[n.cloud.provider].label} · {CLOUD_STATE[n.cloud.state].label}</span>
        <br /><span class="muted">{CLOUD_STATE[n.cloud.state].note}</span>
      {/if}
      {#if n.delta}
        <br /><span class={n.delta > 0 ? "up" : "down"}>{app.fb(n.delta, true)} since last scan</span>
      {/if}
      {#if n.accessDenied}<br /><span class="muted">Needs admin to look inside</span>{/if}
      {#if t.extra}<br /><span class="muted">{t.extra}</span>{/if}
    {:else}
      <b>{t.title}</b>
      {#each t.lines as l, i (i)}<span class="muted">{l}</span><br />{/each}
    {/if}
  </div>
{/if}
