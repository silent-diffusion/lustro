<script lang="ts">
  import type { CloudProvider, MainView, Settings, Theme } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { NODE_SIZES, writeDevOptions } from "../lib/backend";
  import type { MockBackend, UpdateSim } from "../ipc/mock";
  import { fmtBytes, fmtMB } from "../lib/format";
  import { PROVIDER, wording } from "../lib/labels";
  import type { IconName } from "../lib/icons";
  import Icon from "../components/Icon.svelte";
  import Switch from "../components/Switch.svelte";

  const s = $derived(app.settings);
  const save = (patch: Partial<Settings>) => app.saveSettings(patch);
  const terrain = (patch: Partial<Settings["terrain"]>) => save({ terrain: { ...$state.snapshot(s.terrain), ...patch } });
  const pins = (patch: Partial<Settings["terrain"]["pins"]>) => terrain({ pins: { ...$state.snapshot(s.terrain.pins), ...patch } });

  const NAV: [string, string, IconName][] = [
    ["viz", "Visualization", "blocks"], ["appearance", "Appearance", "spark"], ["scanning", "Scanning", "scan"],
    ["snapshots", "Snapshots", "clock"], ["safety", "Cleanup safety", "shield"], ["updates", "Updates", "cloudDown"], ["privacy", "Privacy", "info"],
  ];
  let section = $state("viz");
  function jump(id: string) {
    section = id;
    document.getElementById(`set-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  const VIEWS: [MainView, string][] = [["blockMap", "Block map"], ["ringChart", "Ring chart"], ["fileTree", "File tree"], ["terrain", "Terrain 3D"]];
  const c = (k: string) => `var(--cat-${k})`;

  // Options that only exist in this UI for now (not in the Settings contract): kept per viewer.
  let local = $state<Record<string, boolean>>({});
  const opt = (k: string, def: boolean) => local[k] ?? def;
  const setOpt = (k: string, v: boolean) => (local = { ...local, [k]: v });

  const scanOpts = $derived<[string, string, string, boolean, boolean?][]>(({
    windows: [["hardlinks", "Count hard links once", "Avoids double-counting files that appear in more than one folder", true],
      ["junctions", "Don't follow junctions and symbolic links", "Prevents loops and double counting", true, true]],
    macos: [["hardlinks", "Count hard links and APFS clones once", "Clones share storage until edited; counting them twice overstates use", true],
      ["firmlinks", "Skip firmlinked system folders", "/System/Volumes would otherwise show macOS twice", true, true],
      ["tmsnap", "Count Time Machine local snapshots", "Shown in “Where did my space go?”", true]],
    linux: [["hardlinks", "Count hard links once", "Matched by device and inode", true],
      ["onefs", "Stay on one filesystem", "Don't cross into other mounts unless you add them", true],
      ["pseudo", "Skip /proc, /sys, /dev and /run", "Not real files", true, true],
      ["btrfs", "Count btrfs/ZFS snapshots", "Only when the filesystem supports them", false]],
  } as const)[app.platform] as never);

  function toggleProvider(p: CloudProvider, show: boolean) {
    const hidden = new Set(s.hiddenProviders);
    if (show) hidden.delete(p); else hidden.add(p);
    save({ hiddenProviders: [...hidden] });
  }
  /** Native folder picker (Rust side); the picked path is the only kind of new path the backend accepts. */
  async function addPicked(key: "excluded" | "protectedPaths") {
    const path = await app.backend.pickFolder();
    if (!path || s[key].includes(path)) return;
    await save({ [key]: [...s[key], path] });
  }
  function removeFrom(key: "excluded" | "protectedPaths" | "keptItems", path: string) {
    save({ [key]: s[key].filter((x) => x !== path) });
  }

  let confirmClear = $state(false);
  async function clearAll() {
    await app.backend.clearHistory();
    confirmClear = false;
    app.say("Snapshots and cleanup history deleted");
    await app.changed();
  }

  const up = $derived(app.update);
  const date = (t: number) => new Date(t * 1000).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
  /** "today at 9:14", "Oct 21 at 8:12 PM". */
  function when(t: number | null): string {
    if (t == null) return "never";
    const d = new Date(t * 1000);
    const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
    if (d.toDateString() === new Date().toDateString()) return `today at ${time}`;
    return `${d.toLocaleDateString("en-US", { month: "short", day: "numeric" })} at ${time}`;
  }
  async function undoSkip() {
    await save({ skippedVersion: null });
    app.say("You'll be reminded about it again");
  }
  async function showInstalledNotes() {
    app.notesDialog = { title: `Release notes · ${app.appVersion}`, notes: await app.backend.currentReleaseNotes(), restart: false };
  }

  // Dev-only switches for the mock's update and scan simulation.
  // The mock exposes extra dev switches; the Tauri backend doesn't.
  const mock = $derived("setUpdateSim" in app.backend ? (app.backend as unknown as MockBackend) : null);
  const SIMS: [UpdateSim, string][] = [["ok", "Update available"], ["upToDate", "Up to date"], ["offline", "Offline"], ["download", "Download fails"], ["verify", "Signature fails"], ["unknownSize", "Unknown size"]];
  function setSim(sim: UpdateSim) {
    mock?.setUpdateSim(sim);
    writeDevOptions({ updateSim: sim });
    app.dev = { ...app.dev, updateSim: sim };
  }
  function setScanLength(ms: number) {
    mock?.setScanMs(ms);
    writeDevOptions({ scanMs: ms });
    app.dev = { ...app.dev, scanMs: ms };
  }
  let fastFallback = $state(false);
  function setFallback(on: boolean) {
    mock?.setFastFallback(on);
    fastFallback = on;
  }
  function resetVersion() {
    mock?.resetVersion();
    location.reload();
  }

  function setDev(patch: Parameters<typeof writeDevOptions>[0]) {
    writeDevOptions(patch);
    location.search = "";
  }
  const isDev = import.meta.env.DEV;
</script>

<div class="grid st">
  <nav class="card side" aria-label="Settings sections">
    {#each NAV as [id, l, ic] (id)}<button class:on={section === id} onclick={() => jump(id)}><Icon name={ic} />{l}</button>{/each}
  </nav>
  <div class="sections">
    <section class="card" id="set-viz">
      <div class="card-head"><div class="ic"><Icon name="blocks" size={18} /></div><h2>Visualization</h2></div>
      <h3>Main view · used in Explore and on the Cleanup map</h3>
      <div class="tiles">
        {#each VIEWS as [v, l] (v)}
          <button class="tile" class:on={s.mainView === v} aria-pressed={s.mainView === v} onclick={() => { save({ mainView: v }); app.exploreView = null; }}>
            {#if v === "blockMap"}
              <svg class="pv" viewBox="0 0 120 70"><rect x="0" y="0" width="58" height="44" rx="3" fill={c("video")} /><rect x="0" y="46" width="58" height="24" rx="3" fill={c("dev")} /><rect x="60" y="0" width="60" height="30" rx="3" fill={c("apps")} /><rect x="60" y="32" width="28" height="38" rx="3" fill={c("images")} /><rect x="90" y="32" width="30" height="20" rx="3" fill={c("system")} /><rect x="90" y="54" width="30" height="16" rx="3" fill={c("archives")} /></svg>
            {:else if v === "ringChart"}
              <svg class="pv" viewBox="0 0 120 70"><g transform="translate(60 35)" fill="none" stroke-width="9"><circle r="14" stroke={c("apps")} stroke-dasharray="40 100" /><circle r="14" stroke={c("video")} stroke-dasharray="0 40 48 100" /><circle r="25" stroke={c("images")} stroke-dasharray="60 200" /><circle r="25" stroke={c("dev")} stroke-dasharray="0 62 50 200" /><circle r="25" stroke={c("system")} stroke-dasharray="0 115 30 200" /></g></svg>
            {:else if v === "fileTree"}
              <svg class="pv" viewBox="0 0 120 70"><rect x="0" y="0" width="28" height="70" rx="3" fill={c("apps")} /><rect x="30" y="0" width="28" height="40" rx="3" fill={c("video")} /><rect x="30" y="42" width="28" height="28" rx="3" fill={c("system")} /><rect x="60" y="0" width="28" height="22" rx="3" fill={c("video")} /><rect x="60" y="24" width="28" height="16" rx="3" fill={c("images")} /><rect x="90" y="0" width="30" height="12" rx="3" fill={c("video")} /><rect x="90" y="14" width="30" height="8" rx="3" fill={c("dev")} /></svg>
            {:else}
              <svg class="pv" viewBox="0 0 120 70" fill="none" stroke="var(--terrain-line)" stroke-width=".8"><ellipse cx="60" cy="56" rx="54" ry="11" stroke="var(--accent)" stroke-width="1.4" /><path d="M14 56c14-6 18-24 30-26s10-16 18-17 12 10 18 14 14 20 26 29" /><path d="M22 54c10-5 16-18 25-20s9-12 15-12 9 8 14 11 12 16 22 21" opacity=".7" /><path d="M32 52c8-4 12-12 18-13s7-8 12-8 7 6 11 8 9 10 16 13" opacity=".5" /><line x1="62" y1="13" x2="62" y2="4" stroke="var(--text)" /><circle cx="62" cy="4" r="3" fill="var(--text)" /></svg>
            {/if}
            <span class="t"><span>{l}</span>{#if v === "blockMap"}<span class="tag">Recommended</span>{/if}</span>
          </button>
        {/each}
      </div>
      <h3>Terrain</h3>
      <div class="opt"><div>Tint ground by file type<small>Colors each patch with its main category, like a geology map</small></div><Switch label="Tint ground by file type" on={s.terrain.categoryTint} onchange={(v) => terrain({ categoryTint: v })} /></div>
      <div class="opt"><div>Show largest-file pins<small>Flag pins on the biggest files</small></div><Switch label="Largest-file pins" on={s.terrain.pins.largest} onchange={(v) => pins({ largest: v })} /></div>
      <div class="opt"><div>Show growth pins<small>Coral arrows on the top growers since the last scan</small></div><Switch label="Growth pins" on={s.terrain.pins.grew} onchange={(v) => pins({ grew: v })} /></div>
      <div class="opt"><div>Show duplicate pins<small>Blue pins on duplicate sets</small></div><Switch label="Duplicate pins" on={s.terrain.pins.duplicates} onchange={(v) => pins({ duplicates: v })} /></div>
      <div class="opt"><div>Show cloud pins<small>Hollow cloud pins on large cloud-only folders</small></div><Switch label="Cloud pins" on={s.terrain.pins.cloud} onchange={(v) => pins({ cloud: v })} /></div>
      <div class="opt"><div>Cloud-only files as valleys<small>Land slopes to ground level, then dips below: depth = size stored only in the cloud</small></div><Switch label="Cloud valleys" on={s.terrain.cloudValleys} onchange={(v) => terrain({ cloudValleys: v })} /></div>
      <div class="opt"><div>Start in top-down view<small>Flat contour map instead of 3D, easier to read for some people</small></div><Switch label="Start in top-down view" on={s.terrain.prefer2d} onchange={(v) => terrain({ prefer2d: v })} /></div>
      <div class="opt"><div>Use 2D when 3D is slow<small>Switches to the flat map if the frame rate drops, with a “Try 3D anyway” button</small></div><Switch label="Use 2D when slow" on={opt("autoflat", true)} onchange={(v) => setOpt("autoflat", v)} /></div>
    </section>

    <section class="card" id="set-appearance">
      <div class="card-head"><div class="ic"><Icon name="spark" size={18} /></div><h2>Appearance</h2></div>
      <h3>Theme</h3>
      <div class="seg" role="radiogroup" aria-label="Theme">
        {#each [["dark", "Dark"], ["light", "Light"], ["system", app.w.themeSystem]] as [t, l] (t)}
          <button role="radio" aria-checked={s.theme === t} class:on={s.theme === t} onclick={() => save({ theme: t as Theme })}>{l}</button>
        {/each}
      </div>
      <h3>Density</h3>
      <div class="seg" role="radiogroup" aria-label="Density">
        <button role="radio" aria-checked={s.density === "comfortable"} class:on={s.density === "comfortable"} onclick={() => save({ density: "comfortable" })}>Comfortable</button>
        <button role="radio" aria-checked={s.density === "compact"} class:on={s.density === "compact"} onclick={() => save({ density: "compact" })}>Compact</button>
      </div>
      <div class="opt gap"><div>Show sizes in binary units<small>GiB (1 GiB = 1,024 MiB) instead of GB · now {fmtBytes(1.5e9, { binary: app.binary })}</small></div><Switch label="Binary units" on={app.binary} onchange={(v) => app.setBinary(v)} /></div>
    </section>

    <section class="card" id="set-scanning">
      <div class="card-head"><div class="ic"><Icon name="scan" size={18} /></div><h2>Scanning</h2></div>
      <h3>Scan mode</h3>
      <div class="seg" role="radiogroup" aria-label="Scan mode">
        <button role="radio" aria-checked={s.scanMode === "standard"} class:on={s.scanMode === "standard"} onclick={() => save({ scanMode: "standard" })}>Standard</button>
        <button role="radio" aria-checked={s.scanMode === "fast"} class:on={s.scanMode === "fast"} onclick={() => save({ scanMode: "fast" })}>Fast scan</button>
      </div>
      <p class="faint xs note">{app.w.standardScan} {app.w.fastScan}</p>
      {#if s.scanMode === "fast" && app.platform === "windows"}
        <p class="xs note">Windows will ask for permission each time. Fast scan reads the drive index directly; it can't change files. If you say no, a Standard scan runs instead.</p>
      {/if}
      {#if app.platform === "macos"}
        <div class="card fda">
          <div class="card-head"><div class="ic"><Icon name="shield" size={18} /></div><h2>Full Disk Access</h2><span class="badge review">Not granted</span></div>
          <div class="cols">
            <div><b>Why we ask</b><ul><li>macOS hides Mail, Messages, Safari and some Library folders from apps by default.</li><li>Without access those show up as “Unexplained” space.</li></ul></div>
            <div><b>What we never do</b><ul><li>Read your files' contents, except to compare same-size files when “Find duplicate files” is on.</li><li>Download cloud files or send anything off this Mac.</li><li>Change or delete anything without your review.</li></ul></div>
          </div>
          <div class="btnrow"><button class="btn primary" onclick={() => app.backend.openSetting("macos:fullDiskAccess").then(() => app.say("Opening Privacy & Security…"))}>Open Privacy &amp; Security settings</button><button class="btn">Not now</button></div>
        </div>
      {/if}
      <h3>Excluded folders</h3>
      <div class="chips">
        {#each s.excluded as x (x)}<span class="chip mono">{x}<button class="x" aria-label="Remove {x}" onclick={() => removeFrom("excluded", x)}>✕</button></span>{/each}
        <button class="chip" onclick={() => addPicked("excluded")}>+ Add folder</button>
      </div>
      {#each scanOpts as [k, t, sub, def, locked] (k)}
        <div class="opt"><div>{t}<small>{sub}</small></div><Switch label={t} on={opt(k, def)} locked={!!locked} onchange={(v) => setOpt(k, v)} /></div>
      {/each}
      <div class="opt"><div>Find duplicate files<small>Reads the contents of same-size files on this computer to compare them. Never reads cloud-only files, and nothing leaves your computer.</small></div><Switch label="Find duplicate files" on={s.findDuplicates} onchange={(v) => save({ findDuplicates: v })} /></div>
      <div class="opt"><div>Scan on startup<small>Uses Standard scan and takes a snapshot</small></div><Switch label="Scan on startup" on={opt("startup", false)} onchange={(v) => setOpt("startup", v)} /></div>
      <h3>Cloud files · same rules for every provider</h3>
      <p class="faint xs">{app.w.cloudNote}</p>
      {#each app.cloud as p (p.provider + p.rootPath)}
        <div class="opt">
          <div><span class="cmark">{PROVIDER[p.provider].short}</span> {p.displayName} {#if p.remote && !p.virtualVolumeId}<span class="badge">remote mount</span>{/if}
            <small class="mono">{p.virtualVolumeId ? `${p.virtualVolumeId} virtual drive (Stream) · cache in ${p.cacheRoot}` : p.rootPath}{p.remote && !p.virtualVolumeId ? " · scan anyway" : ""}</small></div>
          {#if p.remote && !p.virtualVolumeId}
            <Switch label="Scan {p.displayName}" on={opt(`remote-${p.rootPath}`, false)} onchange={(v) => setOpt(`remote-${p.rootPath}`, v)} />
          {:else}
            <Switch label="Show {p.displayName} in views" on={!s.hiddenProviders.includes(p.provider)} onchange={(v) => toggleProvider(p.provider, v)} />
          {/if}
        </div>
      {/each}
      <div class="opt"><div>Count cloud-only files in totals<small>Off: totals show what's really on this disk. On: adds cloud-only sizes, marked as not local</small></div><Switch label="Count cloud-only files in totals" on={s.countCloudOnlyInTotals} onchange={(v) => save({ countCloudOnlyInTotals: v })} /></div>
      <div class="opt"><div>Mark downloaded and pinned files<small>Small provider tag on blocks and rows; they keep their file-type color</small></div><Switch label="Mark downloaded and pinned files" on={opt("cmarks", true)} onchange={(v) => setOpt("cmarks", v)} /></div>
      <div class="opt"><div>Never download cloud files<small>Scanning reads attributes only. Duplicate checks skip cloud-only files</small></div><Switch label="Never download cloud files" on={true} locked /></div>
    </section>

    <section class="card" id="set-snapshots">
      <div class="card-head"><div class="ic"><Icon name="clock" size={18} /></div><h2>Snapshots</h2></div>
      <div class="opt"><div>Save a snapshot after every scan<small>Needed for What grew</small></div><Switch label="Save snapshots" on={s.keepSnapshots} onchange={(v) => save({ keepSnapshots: v })} /></div>
      <div class="opt"><div>How many to keep<small>Older ones are deleted automatically</small></div>
        <select class="chip" value={s.snapshotsToKeep} onchange={(e) => save({ snapshotsToKeep: Number(e.currentTarget.value) })} aria-label="Snapshots to keep">
          {#each [4, 8, 12, 24, 52] as n (n)}<option value={n}>Last {n}</option>{/each}
        </select></div>
      <div class="opt"><div>Stored at<small class="mono">{app.w.snapshotDir} · on this computer only</small></div>
        {#if confirmClear}
          <span class="btnrow"><button class="btn small" onclick={() => (confirmClear = false)}>Cancel</button><button class="btn small danger" onclick={clearAll}>Delete all</button></span>
        {:else}
          <button class="btn small" onclick={() => (confirmClear = true)}>Delete all snapshots</button>
        {/if}
      </div>
    </section>

    <section class="card" id="set-safety">
      <div class="card-head"><div class="ic"><Icon name="shield" size={18} /></div><h2>Cleanup safety</h2></div>
      <div class="opt"><div>Always use the {app.w.trash}<small>Lustro never permanently deletes files</small></div><Switch label="Always use the {app.w.trash}" on={true} locked /></div>
      <div class="opt"><div>Review screen before applying<small>Shows every path and the total before anything moves</small></div><Switch label="Review before applying" on={true} locked /></div>
      <div class="opt"><div>Block protected paths<small>System and app folders can't be added to the basket</small></div><Switch label="Block protected paths" on={true} locked /></div>
      <h3>Protected paths</h3>
      <div class="chips">
        {#each s.protectedPaths as x (x)}<span class="chip mono">{x}</span>{/each}
        <button class="chip" onclick={() => addPicked("protectedPaths")}>+ Add path</button>
      </div>
      <h3>Kept items</h3>
      {#each s.keptItems as x (x)}
        <div class="opt"><div class="mono xs brk">{x}</div><button class="btn small" onclick={() => removeFrom("keptItems", x)}>Suggest again</button></div>
      {:else}
        <p class="faint xs">Items you choose to keep in Cleanup are listed here and never suggested again.</p>
      {/each}
    </section>

    <section class="card" id="set-updates">
      <div class="card-head"><div class="ic"><Icon name="cloudDown" size={18} /></div><h2>Updates</h2></div>
      <div class="opt">
        <div>Lustro <span class="mono">{app.appVersion}</span><small>{app.w.label} · {app.portable ? "portable version" : "signed updates from GitHub Releases"}</small></div>
        <button class="btn small" onclick={showInstalledNotes}>Release notes</button>
      </div>
      {#if app.portable}
        <div class="opt">
          <div>Update automatically<small>Not available in the portable version.</small></div>
          <Switch label="Update automatically" on={false} locked />
        </div>
        <div class="opt"><div>Portable version: download new versions from the GitHub Releases page.</div></div>
      {:else}
      <div class="opt">
        <div>Update automatically<small>Checks once a day and downloads new versions in the background. They install the next time Lustro starts. Never interrupts a scan or a cleanup.</small></div>
        <Switch label="Update automatically" on={s.autoUpdate} onchange={(v) => save({ autoUpdate: v })} />
      </div>
      <div class="opt" aria-live="polite">
        <div>
          {#if up.state === "checking"}
            <span class="inl"><span class="spin" aria-hidden="true"></span>Checking for updates…</span><small>Asking GitHub Releases for the latest version</small>
          {:else if up.state === "upToDate"}
            <span class="inl"><span class="badge safe">Up to date</span>You have the latest version</span><small>Checked {when(s.lastUpdateCheck ?? up.checkedAt)}</small>
          {:else if up.state === "available"}
            <span class="inl"><span class="badge cloud">{up.info.version} available</span>A new version is ready to download</span>
            <small>{[up.info.publishedAt != null ? `Released ${date(up.info.publishedAt)}` : null, up.info.sizeBytes != null ? fmtMB(up.info.sizeBytes) : null].filter(Boolean).join(" · ")}</small>
          {:else if up.state === "downloading"}
            <span>Downloading {up.info.version}{#if up.total} · {Math.round((up.downloaded / up.total) * 100)}%{/if}</span>
            <div class="progress" class:indet={!up.total}><i style:width={up.total ? `${(up.downloaded / up.total) * 100}%` : undefined}></i></div>
          {:else if up.state === "ready"}
            <span class="inl"><span class="badge cloud">{up.info.version} ready</span>Downloaded and verified</span><small>Installs the next time Lustro starts</small>
          {:else if up.state === "error" && up.kind === "verify"}
            <span class="inl"><span class="badge locked">Not installed</span>The last update couldn't be verified</span><small>Your current version is unchanged</small>
          {:else if up.state === "error" && up.kind === "download"}
            <span class="inl"><span class="badge review">Download failed</span>The update couldn't be downloaded</span><small>Nothing was changed · last successful check {when(s.lastUpdateCheck)}</small>
          {:else if up.state === "error"}
            <span class="inl"><span class="badge review">Couldn't check</span>Couldn't check for updates. You're offline or GitHub is unreachable.</span><small>Last successful check {when(s.lastUpdateCheck)}</small>
          {:else}
            Check for updates<small>Last checked {when(s.lastUpdateCheck)}</small>
          {/if}
        </div>
        {#if up.state === "available"}
          <button class="btn small primary" onclick={() => { app.installing = false; app.updateDialog = true; }}>View update</button>
        {:else if up.state === "ready"}
          <button class="btn small primary" disabled={app.busy} title={app.busy ? "Waits until the scan or cleanup finishes" : undefined} onclick={() => app.restartToUpdate()}>Restart to update</button>
        {:else if up.state === "error"}
          <button class="btn small" onclick={() => (up.state === "error" && up.kind !== "offline" ? app.installUpdate() : app.checkForUpdates())}>Retry</button>
        {:else if up.state !== "downloading"}
          <button class="btn small" disabled={up.state === "checking"} onclick={() => app.checkForUpdates()}>Check for updates</button>
        {/if}
      </div>
      {#if s.skippedVersion}
        <div class="opt">
          <div>Skipped <span class="mono">{s.skippedVersion}</span><small>You won't be reminded about this version. Newer versions still show up.</small></div>
          <button class="btn small" onclick={undoSkip}>Undo</button>
        </div>
      {/if}
      {/if}
    </section>

    <section class="card privacy" id="set-privacy">
      <div class="card-head"><div class="ic"><Icon name="shield" size={18} /></div><h2>Privacy</h2><span class="badge safe">Private by design</span></div>
      <div>Lustro works entirely on this computer.</div>
      <ul>
        <li>No telemetry, analytics or crash uploads.</li>
        {#if app.portable}
          <li><b>No network requests.</b> This portable version never checks for updates.</li>
        {:else}
        <li>
          <b>One network request: the update check.</b> Lustro asks GitHub Releases over HTTPS whether a newer version exists{#if s.autoUpdate}, once a day while auto-update is on, and when you click “Check for updates”{:else}. Auto-update is off, so this only happens when you click “Check for updates”{/if}. It sends no personal information: no ID, no usage data, nothing about your files. Like any web request, GitHub can see your IP address.{#if s.autoUpdate}{" "}Turn off auto-update to stop the daily check.{/if}
        </li>
        {/if}
        <li>
          {#if s.findDuplicates}<b>Duplicate detection reads file contents, locally.</b> To find duplicates, Lustro reads same-size files on this computer and compares them. Cloud-only files are never read, and nothing is sent anywhere. You can turn this off in Scanning → “Find duplicate files”.
          {:else}<b>No file contents are read.</b> “Find duplicate files” is off, so Lustro only reads file names, sizes and dates.{/if}
        </li>
        <li>No account or sign-in.</li>
        <li>Scan snapshots stay in your user folder and are never shared.</li>
        <li>Exports (CSV/JSON) include full paths. Review them before sharing.</li>
      </ul>
    </section>

    {#if isDev && mock}
      <section class="card dev" id="set-dev">
        <div class="card-head"><div class="ic"><Icon name="info" size={18} /></div><h2>Developer preview</h2><span class="sub">mock backend only · not product UI</span></div>
        <h3>Platform</h3>
        <div class="seg">
          {#each ["windows", "macos", "linux"] as const as p (p)}
            <button class:on={app.platform === p} onclick={() => setDev({ platform: p })}>{wording(p).label}</button>
          {/each}
        </div>
        <h3>Fake tree size</h3>
        <div class="seg">
          {#each NODE_SIZES as n (n)}
            <button class:on={app.dev.nodes === n} onclick={() => setDev({ nodes: n })}>{n >= 1e6 ? `${n / 1e6}M` : `${n / 1e3}k`} items</button>
          {/each}
        </div>
        <p class="faint xs">Switching reloads the page and regenerates the fake tree. Current tree: {app.dev.nodes.toLocaleString("en-US")} items on {app.w.label}.</p>
        <h3>Next update check</h3>
        <div class="seg">
          {#each SIMS as [k, l] (k)}<button class:on={app.dev.updateSim === k} onclick={() => setSim(k)}>{l}</button>{/each}
        </div>
        <h3>Scan length (to test "busy")</h3>
        <div class="seg">
          <button class:on={app.dev.scanMs !== 60_000} onclick={() => setScanLength(8000)}>8 s</button>
          <button class:on={app.dev.scanMs === 60_000} onclick={() => setScanLength(60_000)}>60 s</button>
        </div>
        <h3>Build</h3>
        <div class="seg">
          <button class:on={!app.dev.portable} onclick={() => setDev({ portable: false })}>Installer</button>
          <button class:on={app.dev.portable} onclick={() => setDev({ portable: true })}>Portable (no updater)</button>
        </div>
        <h3>Fast scan</h3>
        <div class="seg">
          <button class:on={!fastFallback} onclick={() => setFallback(false)}>Works</button>
          <button class:on={fastFallback} onclick={() => setFallback(true)}>Declined (falls back to Standard)</button>
        </div>
        <p class="faint xs">Cleanup apply takes 1.5 s in the mock. <button class="btn small" onclick={resetVersion}>Back to version 0.2.0</button></p>
      </section>
    {/if}
  </div>
</div>

<style>
  .st { grid-template-columns: 220px minmax(0, 1fr); align-items: start; }
  .side { position: sticky; top: 16px; display: grid; gap: 2px; padding: 10px; }
  .side button { padding: 9px 12px; border-radius: var(--r-sm); color: var(--text-2); display: flex; gap: 10px; align-items: center; border: 0; background: none; text-align: left; }
  .side button:hover, .side button.on { background: var(--surface-2); color: var(--text); }
  .sections { display: grid; gap: var(--sp-4); }
  .sections > section { scroll-margin-top: 16px; }
  h3 { margin: 18px 0 8px; font-size: var(--fs-xs); text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); font-weight: var(--fw-strong); }
  .tiles { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; }
  .tile { border: 1.5px solid var(--stroke); border-radius: var(--r-md); padding: 10px; cursor: pointer; background: var(--surface-2); display: grid; gap: 8px; color: inherit; text-align: left; }
  .tile.on { border-color: var(--accent); }
  .pv { width: 100%; height: 86px; display: block; }
  .tile .t { display: flex; justify-content: space-between; align-items: center; }
  .tag { font-size: 10px; color: var(--accent); }
  .opt { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 14px; align-items: center; padding: 12px 0; border-top: 1px solid var(--stroke); }
  .opt small { display: block; color: var(--text-3); font-size: var(--fs-xs); margin-top: 2px; }
  .gap { margin-top: 8px; }
  .note { margin: 8px 0 0; }
  .xs { font-size: var(--fs-xs); }
  .brk { word-break: break-all; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
  .x { border: 0; background: none; color: var(--text-3); padding: 0 0 0 4px; font-size: 10px; }
  .fda { margin-top: 14px; background: var(--surface-2); box-shadow: none; }
  .fda .ic { background: var(--surface-1); }
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; font-size: var(--fs-sm); }
  .cols ul { margin: 6px 0 0; padding-left: 18px; color: var(--text-2); }
  .btnrow { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .opt .btnrow { margin-top: 0; }
  .progress { height: 6px; border-radius: 3px; background: var(--surface-3); overflow: hidden; margin-top: 10px; }
  .progress i { display: block; height: 100%; background: var(--accent); transition: width .2s; }
  .inl { display: inline-flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--accent); display: inline-block; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .opt .progress { max-width: 320px; background: var(--surface-2); position: relative; margin-top: 8px; }
  .progress.indet i { position: absolute; width: 35%; animation: indet 1.2s ease-in-out infinite; }
  @keyframes indet { from { left: -35%; } to { left: 100%; } }
  .privacy { background: color-mix(in srgb, var(--lime) 6%, var(--surface-1)); }
  .privacy ul { margin: 8px 0 0; padding-left: 18px; color: var(--text-2); }
  .privacy li { margin: 4px 0; }
  .dev { border-style: dashed; box-shadow: none; }
  @media (max-width: 1000px) { .st { grid-template-columns: 1fr; } .side { position: static; grid-auto-flow: column; overflow-x: auto; } .tiles { grid-template-columns: 1fr 1fr; } .cols { grid-template-columns: 1fr; } }
</style>
