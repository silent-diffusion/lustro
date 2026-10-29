<script lang="ts">
  import type { ApplyResult, Decision, Destination, MainView, NodeDto, NodeId, SuggestionDto, SuggestionGroup, UndoEntry } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { fmtAge, fmtWhen } from "../lib/format";
  import { CATEGORY_LABEL, CLOUD_STATE, GROUP_LABEL, PROVIDER } from "../lib/labels";
  import type { IconName } from "../lib/icons";
  import { size } from "../lib/viz";
  import Icon from "../components/Icon.svelte";
  import { inTauri } from "../ipc";
  import MainViz from "../components/MainViz.svelte";
  import type { Mark } from "../components/CanvasMap.svelte";
  import QueueBar from "../components/QueueBar.svelte";
  import ScanBanner from "../components/ScanBanner.svelte";

  const GROUP_ICON: Record<SuggestionGroup, IconName> = {
    duplicates: "twins", devCaches: "treeIc", tempFiles: "cleanup", oldDownloads: "clock", staleLarge: "flag", freeUpCloud: "cloud",
  };
  const safetyLabel = (s: SuggestionDto) =>
    s.group === "duplicates" ? "Hash-verified" : s.group === "devCaches" ? "Rebuildable" : s.group === "tempFiles" ? app.w.tempLabel
    : s.group === "freeUpCloud" ? (s.safety === "caution" ? "Removes offline access" : "Stays in the cloud") : "Check first";
  const VIEWS: [MainView, string, IconName][] = [["blockMap", "Block map", "blocks"], ["ringChart", "Ring chart", "ring"], ["fileTree", "File tree", "treeIc"], ["terrain", "Terrain 3D", "mountain"]];

  let Q = $state.raw<SuggestionDto[]>([]);
  // Decisions live in app state (persisted), so they survive tab switches and an update restart.
  const decision = $derived(app.cleanupDecisions);
  let cur = $state(app.cleanupCursor);
  $effect(() => { app.cleanupCursor = cur; });
  let group = $state<SuggestionGroup | null>(null);
  let view = $state<MainView>(app.settings.mainView);
  let path = $state.raw<NodeDto[]>([]);
  let undo = $state.raw<UndoEntry[]>([]);
  let dialog = $state(false);
  let dest = $state<"trash" | "move" | "compress">("trash");
  let busy = $state(false);
  /** Result of an apply that had failures: shown in the dialog instead of a toast. */
  let result = $state<ApplyResult | null>(null);
  // Move and Compress need platform support the desktop app doesn't have yet (only the Trash and freeing
  // cloud files work there); the mock keeps them for design review.
  const canMove = !inTauri();
  const canCompress = !inTauri() && app.platform === "windows";

  $effect(() => {
    void app.version;
    app.backend.suggestions().then((s) => {
      Q = s;
      app.setCleanupDecisions(new Map([...decision].filter(([id]) => s.some((x) => x.id === id))));
      cur = Math.min(cur, Math.max(0, s.length - 1));
    });
    app.backend.undoLog().then((u) => (undo = u));
  });

  const q = $derived(Q[cur] as SuggestionDto | undefined);
  $effect(() => {
    const s = q;
    if (!s) { path = []; return; }
    let stale = false;
    // The node can disappear between an apply and the refreshed queue; keep the old path until then.
    app.backend.pathTo(s.nodeId).then((p) => { if (!stale) path = p; }, () => {});
    return () => { stale = true; };
  });
  const d = $derived(q ? decision.get(q.id) : undefined);
  const parent = $derived(path.length > 1 ? path[path.length - 2] : null);
  const focusPath = $derived(path.length && q && path[path.length - 1].id === q.nodeId ? path.map((p) => p.id) : null);
  const isFree = (s: SuggestionDto) => s.group === "freeUpCloud";
  /** After a stopped scan the current tree is partial, while suggestions come from the last complete scan:
   *  their node ids don't match it, so the map (and where-it-lives path) is hidden until the next full scan. */
  const mapStale = $derived(!!app.live?.partial);

  const visible = $derived(Q.map((_, i) => i).filter((i) => !group || Q[i].group === group));
  const total = $derived(Q.reduce((s, x) => s + x.bytes, 0));
  const removing = $derived(Q.filter((s) => decision.get(s.id) === "remove"));
  const removeBytes = $derived(removing.reduce((s, x) => s + x.bytes, 0));
  const count = (d: Decision) => Q.filter((s) => decision.get(s.id) === d).length;
  const decided = $derived(Q.filter((s) => { const d = decision.get(s.id); return d && d !== "later"; }).length);
  const groups = $derived([...new Set(Q.map((s) => s.group))]);

  const marks = $derived.by(() => {
    const m = new Map<NodeId, Mark>();
    for (const s of Q) {
      const d = decision.get(s.id);
      m.set(s.nodeId, d === "remove" ? (isFree(s) ? "free" : "remove") : d === "keep" ? "keep" : "queued");
    }
    return m;
  });

  function nextUndecided(from: number): number | undefined {
    const v = visible, pos = v.indexOf(from);
    return v.slice(pos + 1).concat(v.slice(0, Math.max(0, pos))).find((i) => !decision.has(Q[i].id));
  }
  function decide(d: Decision) {
    const s = q;
    if (!s) return;
    const m = new Map(decision);
    if (m.get(s.id) === d) m.delete(s.id); else m.set(s.id, d);
    app.setCleanupDecisions(m);
    const nxt = nextUndecided(cur);
    if (m.has(s.id) && nxt !== undefined) setTimeout(() => (cur = nxt), 260);
  }
  function step(k: number) {
    const v = visible, pos = v.indexOf(cur);
    if (v.length) cur = v[(pos + k + v.length) % v.length];
  }
  function pickGroup(g: SuggestionGroup | null) {
    group = g;
    const v = Q.map((_, i) => i).filter((i) => !g || Q[i].group === g);
    cur = v.find((i) => !decision.has(Q[i].id)) ?? v[0] ?? 0;
  }
  /** Undecided items in view after the current one, in queue order. */
  const upAll = $derived.by(() => {
    const v = visible, pos = v.indexOf(cur);
    return v.slice(pos + 1).concat(v.slice(0, Math.max(0, pos))).filter((i) => !decision.has(Q[i].id));
  });
  const UP_MAX = 8;
  let showAll = $state(false);
  // The expanded list resets when the group filter changes.
  $effect(() => { void group; showAll = false; });
  const upNext = $derived(showAll ? upAll : upAll.slice(0, UP_MAX));

  function onKey(e: KeyboardEvent) {
    const target = e.target instanceof Element ? e.target : null;
    if (dialog || app.updateDialog || app.notesDialog || e.ctrlKey || e.metaKey || e.altKey || target?.closest("input, textarea, select")) return;
    const k = e.key.toLowerCase();
    if (k === "r") decide("remove");
    else if (k === "k") decide("keep");
    else if (k === "l") decide("later");
    else if (k === "arrowright" || k === "j") step(1);
    else if (k === "arrowleft") step(-1);
    else return;
    e.preventDefault();
  }

  function pickFromMap(id: NodeId) {
    const i = Q.findIndex((s) => s.nodeId === id);
    if (i >= 0) cur = i;
  }

  // Pin over the current item on the block map / file tree.
  let viz: MainViz | undefined = $state();
  let pin = $state<{ x: number; y: number } | null>(null);
  let mapW = $state(0);
  $effect(() => {
    void mapW; void view; void app.version;
    const fp = focusPath;
    pin = null;
    if (!fp || (view !== "blockMap" && view !== "fileTree")) return;
    let tries = 0, raf = 0;
    const place = () => {
      const r = viz?.rectOf(fp);
      if (r) pin = { x: Math.min(Math.max(r.x + r.w / 2, 80), mapW - 80), y: r.y + Math.min(r.h / 2, 30) + 4 };
      else if (tries++ < 60) raf = requestAnimationFrame(place);
    };
    raf = requestAnimationFrame(place);
    return () => cancelAnimationFrame(raf);
  });

  const other = $derived(app.volumes.find((v) => v.id !== app.volumeId && !v.isRemote));
  const toDelete = $derived(removing.filter((s) => !isFree(s)));
  const toFree = $derived(removing.filter(isFree));
  async function apply() {
    busy = true;
    app.applying = true;
    const destination: Destination = !toDelete.length ? { kind: "freeUpCloud" }
      : dest === "move" ? { kind: "move", target: other?.mountPath ?? "" } : dest === "compress" ? { kind: "compress" } : { kind: "trash" };
    const decisions = Q.filter((s) => { const d = decision.get(s.id); return d === "remove" || d === "keep"; }).map((s) => ({ suggestionId: s.id, decision: decision.get(s.id)! }));
    try {
      const r = await app.backend.applyCleanup({ decisions, destination });
      // Failed items stay in the queue, undecided, so the user can choose again.
      const failedPaths = new Set(r.failed.map((f) => f.path));
      const failedIds = new Set(Q.filter((s) => failedPaths.has(s.path)).map((s) => s.id));
      app.setCleanupDecisions(new Map([...decision].filter(([id, d]) => d === "later" && !failedIds.has(id))));
      cur = 0;
      const updated = await app.changedAfterCleanup(r.freedBytes);
      if (r.failed.length) {
        result = r;
      } else {
        dialog = false;
        const msg = `Freed ${app.fb(r.freedBytes)} · ${r.applied} items. Undo from History below.`;
        app.say(updated ? msg : `${msg} Sizes update after the next scan.`, updated ? 2600 : 6000);
      }
    } catch (e) {
      app.say(`Nothing was changed: ${e instanceof Error ? e.message : "the cleanup couldn't run"}`, 6000);
    } finally {
      busy = false;
      app.applying = false;
    }
  }
  async function doUndo(u: UndoEntry) {
    await app.backend.undo(u.id);
    const updated = await app.changedAfterCleanup(u.bytes);
    app.say(updated ? "Undone" : "Undone. Sizes update after the next scan.", updated ? 2600 : 6000);
  }
  const destLabel = (d: Destination) => d.kind === "trash" ? app.w.trash : d.kind === "move" ? `Moved to ${d.target}` : d.kind === "compress" ? "Compressed" : "Freed to cloud-only";
</script>

<svelte:window onkeydown={onKey} />

<ScanBanner text="Scanning… suggestions refresh when it finishes. Decisions you've already made are kept." />

<section class="card">
  <div class="qbar">
    <div><h2>Review queue</h2><div class="faint xs">{Q.length} suggestions · {app.fb(total)} could be freed</div></div>
    <div class="min0">
      <QueueBar queue={Q} items={visible} {cur} {decision} {decided} onPick={(i) => (cur = i)} />
    </div>
    <div class="tally">
      <div title={toFree.length ? `${toDelete.length} to ${app.w.trash}, ${toFree.length} back to cloud-only` : undefined}><span class="k">Remove</span><span class="v up">{removing.length} · {app.fb(removeBytes)}</span></div>
      <div><span class="k">Keep</span><span class="v down">{count("keep")}</span></div>
      <div><span class="k">Later</span><span class="v amber">{count("later")}</span></div>
      <button class="btn primary" disabled={app.scanning || (!removing.length && !count("keep"))} title={app.scanning ? "Waits for the scan to finish" : undefined} onclick={() => { result = null; dialog = true; }}>{removing.length ? `Apply · free ${app.fb(removeBytes)}` : "Apply"}</button>
    </div>
  </div>
  <div class="groups">
    <button class="chip" class:on={!group} onclick={() => pickGroup(null)}>All {Q.length}</button>
    {#each groups as g (g)}
      {@const left = Q.filter((s) => s.group === g && !decision.has(s.id)).length}
      <button class="chip" class:on={group === g} onclick={() => pickGroup(g)}>
        <Icon name={GROUP_ICON[g]} size={14} />{GROUP_LABEL[g]}{#if g === "freeUpCloud"} <span class="faint">(optional)</span>{/if}
        <span class="mono faint">{left ? `${left} left` : "✓"}</span>
      </button>
    {/each}
  </div>
</section>

{#if !Q.length}
  <section class="card done-card">
    <h2>Nothing left to review</h2>
    <p class="muted">Suggestions come back after the next scan. Kept items stay out of the queue (manage them in Settings).</p>
  </section>
{:else if q}
  <div class="grid cq">
    <section class="card map-card">
      <div class="card-head">
        <div class="ic"><Icon name="map" size={18} /></div><h2>Where it lives</h2>
        <div class="tools"><div class="seg">
          {#each VIEWS as [v, l, ic] (v)}<button class:on={view === v} onclick={() => (view = v)}><Icon name={ic} size={14} />{l}</button>{/each}
        </div></div>
      </div>
      <div class="crumbs mono">
        {#if !mapStale}{#each path as p, i (p.id)}{#if i}<span>›</span>{/if}{#if i === path.length - 1}<b>{p.name}</b>{:else}<span>{p.name}</span>{/if}{/each}{/if}
      </div>
      <div class="map-body" bind:clientWidth={mapW}>
        {#if mapStale}
          <div class="stale"><Icon name="map" size={24} /><span>The map comes back after a complete scan.</span><span class="faint xs">The last scan was stopped, so it doesn't match these suggestions.</span></div>
        {:else if app.root}
          <MainViz bind:this={viz} rootId={app.root.id} {view} maxDepth={8} {focusPath} {marks} follow onSelect={pickFromMap} />
        {/if}
        {#if pin && !mapStale}
          <div class="mark-pin" style:left="{pin.x}px" style:top="{pin.y}px"><div class="head">{q.name} · {app.fb(q.bytes)}</div><div class="stem"></div><div class="dot2"></div></div>
        {/if}
      </div>
      <div class="context-line">
        {#if parent && !mapStale}<span>Inside <b>{parent.name}</b> ({app.fb(size(parent))}). Removing this frees <b class="up">{((q.bytes / Math.max(1, size(parent))) * 100).toFixed(0)}%</b> of that folder.</span>{/if}
        <span class="legend">
          <span><span class="sw" style:box-shadow="inset 0 0 0 2px var(--accent)"></span>This item</span>
          <span><span class="sw rm"></span>Remove</span>
          <span><span class="sw" style:box-shadow="inset 0 0 0 2px var(--lime)"></span>Keep</span>
          <span><span class="sw" style:box-shadow="inset 0 0 0 1.5px var(--text)"></span>In queue</span>
        </span>
      </div>
    </section>

    <div class="rcol">
    <section class="card dc">
      <div class="card-head head"><div class="ic"><Icon name={GROUP_ICON[q.group]} size={18} /></div><h2>{GROUP_LABEL[q.group]}</h2><span class="badge {q.safety}">{safetyLabel(q)}</span><span class="pos">{cur + 1} / {Q.length}</span></div>
      <h3>{q.name}</h3>
      {#if q.cloud}<div class="cb"><span class="badge cloud">{PROVIDER[q.cloud.provider].label} · {CLOUD_STATE[q.cloud.state].label}</span></div>{/if}
      <div class="mono faint xs brk">{parent && !mapStale ? q.path.slice(0, q.path.length - q.name.length - 1) : q.path}</div>
      <div class="bigsize">{app.fb(q.bytes)}</div>
      {#if parent && app.root && !mapStale}<div class="faint xs">{((q.bytes / Math.max(1, size(parent))) * 100).toFixed(0)}% of {parent.name} · {((q.bytes / Math.max(1, size(app.root))) * 100).toFixed(1)}% of {app.volume?.label}</div>{/if}
      <div class="why"><span class="k">Why it's suggested</span>{q.reason}</div>
      {#if q.cloud?.state === "pinned"}
        <div class="pinwarn"><b>Pinned.</b> You chose to keep this on the device, probably for offline use. Freeing it unpins it, and it won't open without an internet connection.</div>
      {/if}
      {#if q.keepPath}
        <div class="twin">
          <div class="keep"><span class="lbl down">Keeps</span><span class="mono brk">{q.keepPath}</span></div>
          <div class="rem"><span class="lbl up">Removes</span><span class="mono brk">{q.path}</span></div>
        </div>
      {/if}
      <div class="kv">
        <span>Last opened</span><span>{fmtAge(q.lastOpened)}</span>
        <span>Type</span><span>{path.length && !mapStale ? CATEGORY_LABEL[path[path.length - 1].category] : ""}</span>
        <span>Found by</span><span>{q.howFound}</span>
      </div>
      <div class="decide">
        {#if isFree(q)}
          <button class="fr" class:chosen={d === "remove"} onclick={() => decide("remove")}>Free up space<kbd>R</kbd></button>
        {:else}
          <button class="rm" class:chosen={d === "remove"} onclick={() => decide("remove")}>Remove<kbd>R</kbd></button>
        {/if}
        <button class="kp" class:chosen={d === "keep"} onclick={() => decide("keep")}>Keep<kbd>K</kbd></button>
        <button class="lt" class:chosen={d === "later"} onclick={() => decide("later")}>Later<kbd>L</kbd></button>
      </div>
      <div class="decide-note">{isFree(q) && q.cloud ? `The file stays in ${PROVIDER[q.cloud.provider].label} and stays listed; only the local copy goes.` : "Remove adds it to the list you apply at the end."} Keep stops suggesting it.</div>
      <div class="navrow"><button onclick={() => step(-1)}>← Previous</button><button onclick={() => step(1)}>Next →</button></div>
    </section>

    <section class="card upnext">
      <div class="card-head"><div class="ic"><Icon name="clock" size={18} /></div><h2>Up next</h2><span class="sub">{upAll.length} left in {group ? "this group" : "the queue"}</span></div>
      <div class="up-list" class:expanded={showAll}>
        {#each upNext as i (Q[i].id)}
          <button class="up-row" onclick={() => (cur = i)}><Icon name={GROUP_ICON[Q[i].group]} size={16} /><span class="name">{Q[i].name}</span><span class="mono sz">{app.fb(Q[i].bytes)}</span></button>
        {:else}
          <div class="up-empty">Nothing left in this view.</div>
        {/each}
        {#if upAll.length > UP_MAX}
          <div class="up-more">
            {#if showAll}
              <span></span><button class="linkbtn" onclick={() => (showAll = false)}>Show fewer</button>
            {:else}
              <span>+{upAll.length - UP_MAX} more</span><button class="linkbtn" onclick={() => (showAll = true)}>Show all</button>
            {/if}
          </div>
        {/if}
      </div>
    </section>
    </div>
  </div>
{/if}

<details class="card more">
  <summary><Icon name="shield" /><span>{app.w.protectedPaths.length + 1} protected paths are never offered</span><span class="faint">·</span><span>History: {undo.length} past cleanup{undo.length === 1 ? "" : "s"} you can undo</span><span class="faint show">Show ▾</span></summary>
  <div class="cols">
    <div class="lock">
      <div class="sec-t">Protected</div>
      <div class="list">
        {#each [...app.w.protectedPaths, ["Cloud-only files (all providers)", "Already 0 bytes on this disk, and never opened (that would download them)"]] as [n, why] (n)}
          <div class="row"><Icon name="shield" /><div class="min0"><div class="name">{n}</div><div class="path">{why}</div></div><span class="badge locked">locked</span></div>
        {/each}
      </div>
    </div>
    <div class="hist">
      <div class="sec-t">History</div>
      <div class="list">
        {#each undo as u (u.id)}
          <div class="row"><div class="min0"><div class="name">{destLabel(u.destination)} · {u.items} item{u.items === 1 ? "" : "s"}</div><div class="path">{fmtWhen(u.at)}</div></div><span class="mono down">{app.fb(-u.bytes, true)}</span>
            <button class="btn small" disabled={!u.undoable} onclick={() => doUndo(u)}>Undo</button></div>
        {:else}
          <div class="faint xs">No cleanups yet.</div>
        {/each}
      </div>
    </div>
  </div>
</details>

{#if dialog}
  <div class="modal-bg" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) { dialog = false; result = null; } }}>
    <div class="card modal" role="dialog" aria-modal="true" aria-labelledby="apply-title" tabindex="-1">
      <div class="card-head"><div class="ic"><Icon name="basket" size={18} /></div><h2 id="apply-title">Apply your decisions</h2></div>
      {#if result}
        <p class="sum">
          {#if result.applied}<b>{result.applied} item{result.applied === 1 ? "" : "s"}</b> · <span class="mono">{app.fb(result.freedBytes)}</span> freed. {/if}
          <b class="amber">{result.failed.length} {result.failed.length === 1 ? "wasn't" : "weren't"} removed</b> and stay{result.failed.length === 1 ? "s" : ""} in the queue:
        </p>
        <div class="list mlist">
          {#each result.failed as f, i (i)}
            <div class="fail"><span class="path mono">{f.path}</span><span class="msg">{f.message}</span></div>
          {/each}
        </div>
        {#if result.applied}<p class="muted xs">What was removed can be undone from History below.</p>{/if}
        <div class="mbtns"><button class="btn primary" onclick={() => { dialog = false; result = null; }}>Close</button></div>
      {:else}
      <p class="sum"><b>{removing.length} items</b> · <span class="mono">{app.fb(removeBytes)}</span> will be freed on {app.volume?.label}</p>
      <div class="list mlist">
        {#if toDelete.length}<div class="faint xs grp">REMOVE · {toDelete.length}</div>{/if}
        {#each toDelete as s (s.id)}<div class="row two"><span class="path mono">{s.path}</span><span class="mono muted">{app.fb(s.bytes)}</span></div>{/each}
        {#if toFree.length}<div class="faint xs grp">FREE UP SPACE · back to cloud-only · {toFree.length}</div>{/if}
        {#each toFree as s (s.id)}<div class="row two"><span class="path mono">{s.path}</span><span class="mono muted">{app.fb(s.bytes)}</span></div>{/each}
      </div>
      {#if toDelete.length}
        {#if (other && canMove) || canCompress}
        <div class="dest" role="radiogroup">
          <label class:on={dest === "trash"}><input type="radio" bind:group={dest} value="trash" /><span>Send to {app.w.trash}<small>Default · you can undo from History while they're still in the {app.w.trash}</small></span></label>
          {#if other && canMove}<label class:on={dest === "move"}><input type="radio" bind:group={dest} value="move" /><span>Move to {other.label}<small>Frees this drive and keeps the files</small></span></label>{/if}
          {#if canCompress}<label class:on={dest === "compress"}><input type="radio" bind:group={dest} value="compress" /><span>Compress in place<small>NTFS compression · slower to open</small></span></label>{/if}
        </div>
        {:else}
          <p class="trash-only"><Icon name="cleanup" size={14} />Items go to the {app.w.trash}. You can undo from History while they're still there.</p>
        {/if}
      {/if}
      <p class="muted xs">{count("keep")} kept items won't be suggested again (manage in Settings). {Q.length - removing.length - count("keep")} undecided items stay in the queue. System and app folders are always blocked.</p>
      <div class="mbtns">
        <button class="btn" onclick={() => (dialog = false)}>Back to queue</button>
        <button class="btn primary" disabled={busy} onclick={apply}>
          {!toDelete.length ? (removing.length ? "Free up space" : "Save decisions") : dest === "move" ? `Move to ${other?.label}` : dest === "compress" ? "Compress" : `Send to ${app.w.trash}`}
        </button>
      </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .xs { font-size: var(--fs-xs); }
  .min0 { min-width: 0; }
  .brk { word-break: break-all; }
  .qbar { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: var(--sp-6); align-items: center; }
  .qbar h2 { margin: 0; font-size: var(--fs-lg); }
  .tally { display: flex; gap: var(--sp-4); align-items: center; }
  .tally > div { display: grid; text-align: right; }
  .tally .k { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); }
  .tally .v { font-family: var(--font-mono); font-weight: var(--fw-strong); }
  .amber { color: var(--amber); }
  .groups { display: flex; gap: 6px; flex-wrap: wrap; margin-top: var(--sp-4); }

  .cq { grid-template-columns: minmax(0, 1fr) 400px; margin-top: var(--sp-4); align-items: stretch; }
  .map-card { display: flex; flex-direction: column; min-height: 600px; }
  .map-body { flex: 1; position: relative; min-height: 420px; }
  .crumbs { display: flex; gap: 4px; flex-wrap: wrap; font-size: var(--fs-xs); color: var(--text-3); margin: -6px 0 10px; }
  .crumbs b { color: var(--text); font-weight: var(--fw-regular); }
  .context-line { margin-top: 12px; font-size: var(--fs-sm); color: var(--text-2); display: flex; gap: 14px; flex-wrap: wrap; align-items: center; }
  .context-line .legend { margin-left: auto; }
  .sw { width: 12px; height: 12px; border-radius: 3px; display: inline-block; }
  .sw.rm { background: repeating-linear-gradient(135deg, var(--coral) 0 2px, transparent 2px 5px); box-shadow: inset 0 0 0 1.5px var(--coral); }
  .mark-pin { position: absolute; transform: translate(-50%, -100%); pointer-events: none; display: flex; flex-direction: column; align-items: center; z-index: 6; transition: left .35s var(--ease), top .35s var(--ease); }
  .mark-pin .head { background: var(--accent); color: var(--accent-ink); border-radius: var(--r-pill); padding: 4px 10px; font-size: 11px; font-weight: var(--fw-strong); white-space: nowrap; box-shadow: var(--shadow); }
  .mark-pin .stem { width: 2px; height: 16px; background: var(--accent); }
  .mark-pin .dot2 { width: 9px; height: 9px; border-radius: 50%; background: var(--accent); animation: ping 1.6s infinite; }
  @keyframes ping { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 70%, transparent); } 100% { box-shadow: 0 0 0 14px transparent; } }

  .stale { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 6px; text-align: center; color: var(--text-3); padding: 16px; }
  .rcol { display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .dc { display: flex; flex-direction: column; flex: none; }
  .min0 { min-width: 0; }
  .upnext { flex: 1; display: flex; flex-direction: column; min-height: 0; }
  .up-list { display: grid; gap: 2px; align-content: start; }
  .up-list.expanded { max-height: 420px; overflow-y: auto; }
  .up-row {
    display: grid; grid-template-columns: 16px minmax(0, 1fr) auto; gap: 10px; align-items: center; min-height: 36px; padding: 8px 10px;
    border: 0; border-radius: var(--r-sm); background: none; color: var(--text); text-align: left; font: inherit; font-size: var(--fs-sm); cursor: pointer;
  }
  .up-row :global(svg) { color: var(--text-3); }
  .up-row:hover { background: var(--surface-2); }
  .up-row:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .up-row .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .up-row .sz { font-size: var(--fs-xs); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .up-more { display: flex; justify-content: space-between; align-items: center; padding: 8px 10px 8px 36px; font-size: var(--fs-xs); color: var(--text-3); }
  .up-empty { padding: 8px 10px; font-size: var(--fs-sm); color: var(--text-3); }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--text-3); font-size: var(--fs-xs); text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
  .linkbtn:hover { color: var(--text); }
  .head { margin-bottom: 8px; }
  .pos { margin-left: auto; font-family: var(--font-mono); font-size: var(--fs-xs); color: var(--text-3); }
  .dc h3 { margin: 4px 0 2px; font-size: var(--fs-xl); word-break: break-word; line-height: 1.2; }
  .cb { margin: 4px 0; }
  .bigsize { font-family: var(--font-mono); font-size: var(--fs-hero); font-weight: var(--fw-strong); letter-spacing: -.03em; margin: 12px 0 4px; line-height: 1; }
  .why { background: var(--surface-2); border-radius: var(--r-md); padding: 12px 14px; margin: 14px 0 4px; font-size: var(--fs-sm); }
  .why .k { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); display: block; margin-bottom: 2px; }
  .pinwarn { border: 1px solid var(--amber); color: var(--amber); border-radius: var(--r-md); padding: 10px 12px; font-size: var(--fs-xs); margin: 8px 0 4px; }
  .twin { display: grid; gap: 6px; margin: 4px 0 8px; }
  .twin > div { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 8px; font-size: var(--fs-xs); align-items: center; padding: 8px 10px; border-radius: var(--r-sm); border: 1px solid var(--stroke); }
  .twin .keep { border-color: var(--lime); }
  .twin .rem { border-color: var(--coral); }
  .twin .lbl { font-weight: var(--fw-strong); text-transform: uppercase; letter-spacing: .05em; font-size: 10px; }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; font-size: var(--fs-sm); margin: 12px 0; }
  .kv span:nth-child(odd) { color: var(--text-3); }
  .kv span:nth-child(even) { text-align: right; }
  .decide { display: grid; grid-template-columns: 1.4fr 1fr 1fr; gap: 8px; margin-top: auto; padding-top: 14px; }
  .decide button { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 12px 8px; border-radius: var(--r-md); border: 1.5px solid var(--stroke-strong); background: var(--surface-2); font-weight: var(--fw-strong); }
  .decide .rm { background: var(--coral); border-color: var(--coral); color: #1a0d08; }
  .decide .fr { background: var(--cat-cloud); border-color: var(--cat-cloud); color: var(--accent-ink); }
  .decide .rm kbd, .decide .fr kbd { color: rgba(0,0,0,.55); }
  .decide .kp { border-color: var(--lime); color: var(--lime); }
  .decide .lt { border-style: dashed; color: var(--text-2); }
  .decide button.chosen { outline: 2px solid var(--text); outline-offset: 2px; }
  .decide-note { font-size: var(--fs-xs); color: var(--text-3); margin-top: 8px; text-align: center; }
  .navrow { display: flex; justify-content: space-between; margin-top: 10px; }
  .navrow button { border: 0; background: none; color: var(--text-2); font-size: var(--fs-xs); }
  .done-card { margin-top: var(--sp-4); text-align: center; padding: 40px 20px; }

  details.more { margin-top: var(--sp-4); }
  details.more summary { cursor: pointer; list-style: none; display: flex; gap: var(--sp-5); color: var(--text-2); font-size: var(--fs-sm); align-items: center; flex-wrap: wrap; }
  details.more summary::-webkit-details-marker { display: none; }
  .show { margin-left: auto; }
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-6); margin-top: var(--sp-4); }
  .lock .row { grid-template-columns: 18px minmax(0, 1fr) auto; color: var(--text-3); }
  .lock .name { color: var(--text); }
  .hist .row { grid-template-columns: minmax(0, 1fr) auto auto; }

  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.55); display: grid; place-items: center; z-index: 60; }
  .modal { width: min(560px, calc(100vw - 32px)); max-height: calc(100vh - 40px); overflow: auto; }
  .sum { margin-top: 0; }
  .mlist { max-height: 200px; overflow: auto; }
  .grp { margin: 8px 0 4px; }
  .two { grid-template-columns: minmax(0, 1fr) auto; padding: 4px 6px; }
  .dest { display: grid; gap: 8px; margin: 12px 0; }
  .dest label { display: grid; grid-template-columns: 20px 1fr; gap: 8px; padding: 10px 12px; border: 1px solid var(--stroke); border-radius: var(--r-md); cursor: pointer; align-items: start; }
  .dest label.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 7%, transparent); }
  .dest input { accent-color: var(--accent); margin-top: 3px; }
  .dest small { display: block; color: var(--text-3); }
  .trash-only { display: flex; gap: 8px; align-items: center; font-size: var(--fs-sm); color: var(--text-2); margin: 12px 0; }
  .fail { display: grid; gap: 2px; padding: 8px 6px; border-bottom: 1px solid var(--stroke); font-size: var(--fs-xs); }
  .fail .path { overflow-wrap: anywhere; color: var(--text-2); }
  .fail .msg { color: var(--amber); }
  .mbtns { display: flex; gap: 8px; justify-content: flex-end; }
  @media (max-width: 1150px) { .cq { grid-template-columns: 1fr; } .qbar { grid-template-columns: 1fr; } .tally { justify-content: space-between; } .cols { grid-template-columns: 1fr; } }
</style>
