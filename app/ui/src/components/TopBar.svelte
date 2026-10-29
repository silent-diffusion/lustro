<script lang="ts">
  import { app, type Route } from "../lib/app.svelte";
  import type { IconName } from "../lib/icons";
  import Icon from "./Icon.svelte";

  const tabs: [Route, string, IconName][] = [
    ["overview", "Overview", "overview"],
    ["explore", "Explore", "explore"],
    ["grew", "What grew", "grew"],
    ["cleanup", "Cleanup", "cleanup"],
  ];
  const scanning = $derived(app.scanning);
</script>

<header class="topbar chrome-{app.platform}">
  <div class="brand"><Icon name="logo" size={30} /><span>Lustro</span></div>
  <nav class="nav" aria-label="Main">
    {#each tabs as [r, label, ic] (r)}
      <a href="#/{r}" class:active={app.route === r} aria-current={app.route === r ? "page" : undefined} title="{label} ({app.kbd(String(tabs.findIndex((t) => t[0] === r) + 1))})">
        <Icon name={ic} />{label}
      </a>
    {/each}
  </nav>
  <div class="top-actions">
    {#if scanning}
      <!-- While scanning it only reports progress: clicking does nothing and never cancels. -->
      <button class="btn primary busy" aria-disabled="true" title="Scan in progress. Details are in the bar below.">
        <span class="pulse"></span>Scanning · {Math.round(app.scanPct * 100)}%
      </button>
    {:else}
      <button class="btn primary" onclick={() => app.startScan()} title="Scan ({app.kbd('R')})"><Icon name="scan" />Scan</button>
    {/if}
    <a class="icon-btn gear" class:active={app.route === "settings"} class:has-dot={app.updatePending} href="#/settings"
      title={app.updatePending ? `Settings · ${app.update.state === "ready" ? "update ready" : "update available"}` : `Settings (${app.kbd(",")})`}
      aria-label={app.updatePending ? "Settings, update ready" : "Settings"}>
      <Icon name="gear" size={18} />
    </a>
  </div>
</header>

<style>
  .topbar { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; padding: var(--sp-5) var(--sp-8); gap: var(--sp-4); }
  /* macOS: the window's traffic lights sit at the top left, so the bar starts after them. */
  .topbar.chrome-macos { padding-left: 96px; padding-top: var(--sp-4); }
  .brand { display: flex; align-items: center; gap: 10px; font-weight: 700; font-size: var(--fs-lg); }
  .nav { display: flex; gap: 4px; padding: 5px; border-radius: var(--r-pill); background: var(--surface-2); border: 1px solid var(--stroke); }
  .nav a { display: flex; align-items: center; gap: 8px; padding: 9px 18px; border-radius: var(--r-pill); color: var(--text-2); transition: background .2s var(--ease), color .2s; white-space: nowrap; }
  .nav a:hover { color: var(--text); }
  .nav a.active { background: var(--surface-3); color: var(--text); }
  .gear { position: relative; }
  .gear.has-dot::after { content: ""; position: absolute; top: 6px; right: 6px; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 2px var(--bg); }
  .top-actions { display: flex; justify-content: flex-end; align-items: center; gap: var(--sp-3); }
  .busy { cursor: default; font-variant-numeric: tabular-nums; }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--accent-ink); animation: pulse 1.4s infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: .3; } }
  @media (max-width: 900px) {
    .topbar { grid-template-columns: 1fr; justify-items: center; padding: var(--sp-4); }
    .nav { overflow-x: auto; max-width: 100%; }
  }
</style>
