<script lang="ts">
  import { app, type Route } from "./lib/app.svelte";
  import TopBar from "./components/TopBar.svelte";
  import ContextBar from "./components/ContextBar.svelte";
  import Tooltip from "./components/Tooltip.svelte";
  import Toast from "./components/Toast.svelte";
  import UpdateToast from "./components/UpdateToast.svelte";
  import UpdateDialog from "./components/UpdateDialog.svelte";
  import NotesDialog from "./components/NotesDialog.svelte";
  import FastScanConfirm from "./components/FastScanConfirm.svelte";
  import Overview from "./screens/Overview.svelte";
  import Explore from "./screens/Explore.svelte";
  import Grew from "./screens/Grew.svelte";
  import Cleanup from "./screens/Cleanup.svelte";
  import Settings from "./screens/Settings.svelte";
  import Scan from "./screens/Scan.svelte";

  app.init();

  const TABS: Route[] = ["overview", "explore", "grew", "cleanup"];

  // App-wide shortcuts: Ctrl/⌘ + 1–4 tabs, R scan, comma settings, F or K search.
  function onKey(e: KeyboardEvent) {
    const mod = app.platform === "macos" ? e.metaKey : e.ctrlKey;
    if (!mod || e.altKey || !app.ready) return;
    const k = e.key.toLowerCase();
    if (k >= "1" && k <= "4") app.go(TABS[Number(k) - 1]);
    else if (k === "r") app.startScan();
    else if (k === ",") app.go("settings");
    else if (k === "f" || k === "k") {
      app.go("explore");
      setTimeout(() => document.dispatchEvent(new CustomEvent("lustro-focus-search")), 0);
    } else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class:compact={app.settings.density === "compact"}>
  <TopBar />
  {#if !app.ready}
    <main class="page loading">
      <div class="card"><span class="faint">Building the fake tree ({app.dev.nodes.toLocaleString("en-US")} items)…</span></div>
    </main>
  {:else}
    <main class="page">
      {#if app.route !== "settings"}<ContextBar />{/if}
      {#if !app.root && app.route !== "settings" && app.route !== "scan"}
        <section class="card empty">
          <h2>No scan yet</h2>
          <p class="muted">Scan {app.volume?.label ?? "a drive"} to see where your space went. Scanning only reads file details; nothing leaves this computer.</p>
          <button class="btn primary" onclick={() => app.startScan()}>Scan {app.volume?.label}</button>
        </section>
      {:else if app.route === "overview"}<Overview />
      {:else if app.route === "explore"}<Explore />
      {:else if app.route === "grew"}<Grew />
      {:else if app.route === "cleanup"}<Cleanup />
      {:else if app.route === "settings"}<Settings />
      {:else if app.route === "scan"}<Scan />
      {/if}
    </main>
  {/if}
  <Tooltip />
  <Toast />
  {#if app.ready}<UpdateToast /><UpdateDialog /><NotesDialog /><FastScanConfirm />{/if}
</div>

<style>
  .loading .card { text-align: center; padding: 60px; }
  .empty { text-align: center; padding: 60px 20px; display: grid; justify-items: center; gap: 8px; }
  .empty h2 { margin: 0; }
</style>
