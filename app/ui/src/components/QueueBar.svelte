<script lang="ts" module>
  /** The "showing N items as a progress bar" hint appears only the first time compact mode shows in a session. */
  let compactSeen = false;
</script>

<script lang="ts">
  // Cleanup's queue bar. Segments (one per item, flex by size) while they fit at 9px each; past that, a
  // compact bar with one equal slice per item, which reads as progress through the queue (byte totals
  // are in the tally). The mode is re-checked on resize and on queue changes, never mid-click.
  import type { Decision, SuggestionDto } from "../ipc/types";
  import { app } from "../lib/app.svelte";

  interface Props {
    queue: SuggestionDto[];
    /** Indexes into `queue` shown in the bar (the group filter), in order. */
    items: number[];
    cur: number;
    decision: Map<string, Decision>;
    /** Decided items (Remove or Keep), for the line under the bar. */
    decided: number;
    onPick: (i: number) => void;
  }
  let { queue, items, cur, decision, decided, onPick }: Props = $props();

  const isFree = (s: SuggestionDto) => s.group === "freeUpCloud";
  const stateOf = (s: SuggestionDto) => decision.get(s.id) ?? "undecided";

  let W = $state(0);
  let compact = $state(false);
  let pressing = false;
  let pendingMode: boolean | null = null;
  let note = $state(false);
  $effect(() => {
    const want = W > 0 && items.length * 9 > W;
    if (pressing) { pendingMode = want; return; }
    compact = want;
  });
  $effect(() => {
    if (compact && !compactSeen) { compactSeen = true; note = true; }
  });
  function release() {
    pressing = false;
    if (pendingMode != null) { compact = pendingMode; pendingMode = null; }
  }

  // ---- Compact mode ----
  let canvas: HTMLCanvasElement | undefined = $state();
  const pos = $derived(items.indexOf(cur));
  $effect(() => {
    const c = canvas, p = app.palette, n = items.length, w = W;
    void decision;
    if (!c || !p || !n || !w) return;
    const dpr = devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(12 * dpr);
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, 12);
    const sw = w / n;
    for (let k = 0; k < n; k++) {
      const s = queue[items[k]];
      const d = decision.get(s.id);
      const x = k * sw;
      if (d === "later") {
        ctx.strokeStyle = p.amber;
        ctx.lineWidth = 1.5;
        ctx.strokeRect(x + 0.75, 0.75, Math.max(0, sw - 1.5), 10.5);
        continue;
      }
      ctx.fillStyle = d === "remove" ? (isFree(s) ? p.cat.cloud : p.coral) : d === "keep" ? p.lime : p.strokeStrong;
      // Overlap by a hair so equal slices don't leave hairline gaps.
      ctx.fillRect(x, 0, sw + 0.5, 12);
    }
  });
  function indexAt(e: MouseEvent): number {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.max(0, Math.min(items.length - 1, Math.floor(((e.clientX - r.left) / r.width) * items.length)));
  }
  function hover(e: MouseEvent) {
    const s = queue[items[indexAt(e)]];
    if (s) app.showTip({ kind: "text", title: s.name, lines: [`${app.fb(s.bytes)} · ${stateOf(s)}`], x: e.clientX, y: e.clientY });
  }
  function key(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    const n = items.length;
    if (!n) return;
    const k = pos < 0 ? 0 : (pos + (e.key === "ArrowRight" ? 1 : -1) + n) % n;
    onPick(items[k]);
    e.preventDefault();
    e.stopPropagation();
  }
  const curItem = $derived(pos >= 0 ? queue[items[pos]] : undefined);
</script>

<svelte:window onpointerup={release} />

<div class="qbarwrap" role="presentation" bind:clientWidth={W} onpointerdown={() => (pressing = true)}>
  {#if !compact}
    <div class="qsegs">
      {#each items as i (queue[i].id)}
        {@const s = queue[i]}
        {@const d = decision.get(s.id)}
        <button style:flex={Math.max(1, s.bytes / 1e9)} class="{d ?? ''}" class:free={isFree(s)} class:cur={i === cur} title="{s.name} · {app.fb(s.bytes)}" aria-label="{s.name}, {d ?? 'undecided'}" onclick={() => onPick(i)}></button>
      {/each}
    </div>
  {:else}
    <div class="qcompact" role="slider" tabindex="0" aria-label="Review queue"
      aria-valuemin={1} aria-valuemax={items.length} aria-valuenow={pos + 1}
      aria-valuetext={curItem ? `${curItem.name}, ${stateOf(curItem)}` : undefined}
      onclick={(e) => onPick(items[indexAt(e)])} onmousemove={hover} onmouseleave={() => app.showTip(null)} onkeydown={key}>
      <canvas bind:this={canvas} style:width="{W}px" style:height="12px"></canvas>
      {#if pos >= 0}<span class="tick" style:left="{((pos + 0.5) / items.length) * W}px"></span>{/if}
    </div>
  {/if}
</div>
<div class="qmeta">
  <span>{decided} of {queue.length} decided{#if compact && note}{" "}<span class="hint">· showing {items.length} items as a progress bar</span>{/if}</span>
  <span>Nothing changes until you apply</span>
</div>

<style>
  .qbarwrap { min-width: 0; overflow: hidden; padding: 10px 0; margin: -10px 0; }
  .qsegs { display: flex; gap: 3px; height: 12px; min-width: 0; padding: 0 3px; }
  .qsegs button { border: 0; padding: 0; border-radius: 4px; background: var(--stroke-strong); cursor: pointer; min-width: 6px; transition: background .2s; }
  .qsegs button.remove { background: var(--coral); }
  .qsegs button.free.remove { background: var(--cat-cloud); }
  .qsegs button.keep { background: var(--lime); }
  .qsegs button.later { background: transparent; box-shadow: inset 0 0 0 1.5px var(--amber); }
  .qsegs button.cur { outline: 2px solid var(--text); outline-offset: 2px; }
  .qcompact { position: relative; height: 12px; border-radius: 4px; cursor: pointer; overflow: visible; outline-offset: 4px; }
  .qcompact canvas { display: block; border-radius: 4px; }
  .qcompact:focus-visible { outline: 2px solid var(--accent); }
  .tick { position: absolute; top: -3px; width: 2px; height: 18px; margin-left: -1px; background: var(--text); border-radius: 1px; transition: left .2s var(--ease); pointer-events: none; }
  .tick::before { content: ""; position: absolute; left: -2px; top: -6px; border: 3px solid transparent; border-top: 6px solid var(--text); border-bottom: 0; }
  .qmeta { display: flex; justify-content: space-between; gap: 12px; margin-top: 8px; font-size: var(--fs-xs); color: var(--text-3); }
  .hint { color: var(--text-3); }
</style>
