<script lang="ts">
  // "Update available" dialog. One click on "Download and install" runs the whole
  // flow: download → signature check → install and restart. It never restarts during a scan or a cleanup.
  import type { UpdateInfo } from "../ipc/types";
  import { app } from "../lib/app.svelte";
  import { fmtMB } from "../lib/format";
  import Icon from "./Icon.svelte";
  import Notes from "./Notes.svelte";

  const st = $derived(app.update);
  // Keep the release we're showing, even when the status moves on to an error.
  let info = $state<UpdateInfo | null>(null);
  $effect(() => {
    if ("info" in st) info = st.info;
  });

  const verifying = $derived(st.state === "downloading" && st.total != null && st.downloaded >= st.total);
  const busyWhat = $derived(app.scan && !app.scan.done ? "scan" : "cleanup");
  const pct = $derived(st.state === "downloading" && st.total ? Math.round((st.downloaded / st.total) * 100) : null);

  function close() {
    app.updateDialog = false;
    if (st.state !== "downloading") app.installing = false;
  }
  async function skip() {
    if (!info) return;
    await app.backend.skipVersion(info.version);
    app.settings = await app.backend.getSettings();
    app.updateDialog = false;
    app.say(`You won't be reminded about ${info.version}`);
  }
  const date = (s: number) => new Date(s * 1000).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
  const focus = (el: HTMLElement) => { el.focus(); };
</script>

<svelte:window onkeydown={(e) => app.updateDialog && e.key === "Escape" && close()} />

{#if app.updateDialog && info && !app.portable}
  <div class="modal-bg" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="card upd" role="dialog" aria-modal="true" aria-labelledby="upd-title" tabindex="-1">
      <div class="card-head">
        <div class="ic mark"><Icon name="logo" size={22} /></div><h2 id="upd-title">Update available</h2><span class="badge cloud">New</span>
      </div>
      <div class="ver"><span class="mono faint">{info.currentVersion}</span><span class="faint">→</span><span class="mono strong">{info.version}</span></div>
      <div class="meta">
        {[info.publishedAt != null ? `Released ${date(info.publishedAt)}` : null, info.sizeBytes != null ? fmtMB(info.sizeBytes) : null, "from GitHub Releases, signature checked before install"].filter(Boolean).join(" · ")}
      </div>
      <div class="box" class:dim={app.installing}><Notes src={info.notes} /></div>

      {#if !app.installing}
        <div class="foot">
          <button class="linkbtn" onclick={skip}>Skip this version</button>
          <span class="sp"></span>
          <button class="btn" onclick={close}>Not now</button>
          <button class="btn primary" use:focus onclick={() => app.installUpdate()}><Icon name="cloudDown" />Download and install</button>
        </div>
      {:else}
        <div class="status" aria-live="polite">
          {#if st.state === "error" && st.kind === "verify"}
            <div class="err danger"><Icon name="shield" size={18} /><div><b>This update couldn't be verified, so it wasn't installed.</b> The download didn't match Lustro's signature. Your current version is unchanged. Try again later; if it keeps happening, download the installer from the official GitHub Releases page.</div></div>
            <div class="foot"><span class="sp"></span><button class="btn" onclick={close}>Close</button></div>
          {:else if st.state === "error"}
            <div class="err"><Icon name="info" size={18} /><div><b>Download failed.</b> Nothing was changed. Check your connection and try again.</div></div>
            <div class="foot"><span class="sp"></span><button class="btn" onclick={close}>Close</button><button class="btn primary" onclick={() => app.installUpdate()}>Try again</button></div>
          {:else if verifying}
            <div class="row"><b><Icon name="shield" />Checking signature</b><span class="spin" aria-hidden="true"></span></div>
            <div class="progress"><i style:width="100%"></i></div>
          {:else if st.state === "downloading"}
            <div class="row"><b>Downloading {info.version}</b>{#if st.total != null}<span class="mono faint">{fmtMB(st.downloaded)} of {fmtMB(st.total)}</span>{:else}<span class="mono faint">{fmtMB(st.downloaded)}</span>{/if}</div>
            <div class="progress" class:indet={pct == null} role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100}><i style:width={pct == null ? undefined : `${pct}%`}></i></div>
            {#if app.busy}<p class="faint">A {busyWhat} is running. The update downloads now and installs when the {busyWhat} finishes.</p>{/if}
            <p class="faint">You can keep using Lustro while it downloads.</p>
          {:else if st.state === "ready" && app.busy}
            <div class="row"><b><Icon name="shield" />Downloaded and verified</b></div>
            <p>Lustro {info.version} will install when the {busyWhat} finishes. We'll ask before restarting.</p>
            <div class="foot"><span class="sp"></span><button class="btn primary" use:focus onclick={() => { app.installing = false; app.updateDialog = false; }}>OK</button></div>
          {:else if st.state === "ready"}
            <div class="row"><b>Installing {info.version}</b><span class="spin" aria-hidden="true"></span></div>
            <p>Lustro will restart in a moment. Your settings, snapshots and Cleanup decisions are kept.</p>
          {:else}
            <div class="row"><b>Preparing download</b><span class="spin" aria-hidden="true"></span></div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.55); display: grid; place-items: center; z-index: 60; padding: 16px; }
  .upd { width: min(560px, 100%); max-height: calc(100vh - 32px); display: flex; flex-direction: column; overflow: auto; }
  .mark { --bg: var(--surface-2); color: var(--text); }
  .ver { display: flex; align-items: baseline; gap: 10px; font-size: var(--fs-xl); }
  .strong { font-weight: var(--fw-strong); color: var(--text); }
  .meta { color: var(--text-3); font-size: var(--fs-xs); margin: 4px 0 14px; }
  .box { background: var(--surface-2); border-radius: var(--r-md); padding: 4px 16px; max-height: 280px; overflow: auto; transition: opacity .2s, max-height .2s; flex: none; }
  .box.dim { opacity: .45; max-height: 120px; }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 16px; }
  .sp { flex: 1; }
  .linkbtn { border: 0; background: none; padding: 4px 2px; color: var(--text-3); font-size: var(--fs-xs); text-decoration: underline; text-underline-offset: 3px; }
  .linkbtn:hover { color: var(--text); }
  .status { margin-top: 16px; font-size: var(--fs-sm); }
  .status p { margin: 8px 0 0; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
  .row b { display: inline-flex; align-items: center; gap: 6px; }
  .progress { height: 6px; border-radius: 3px; background: var(--surface-3); overflow: hidden; margin-top: 10px; position: relative; }
  .progress i { display: block; height: 100%; width: 0; background: var(--accent); transition: width .2s var(--ease); }
  .progress.indet i { position: absolute; width: 35%; animation: indet 1.2s ease-in-out infinite; }
  @keyframes indet { from { left: -35%; } to { left: 100%; } }
  .spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--accent); display: inline-block; animation: spin .8s linear infinite; flex: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .err { display: flex; gap: 10px; align-items: flex-start; padding: 12px; border-radius: var(--r-md); border: 1px solid var(--amber); color: var(--text-2); }
  .err :global(.icon) { color: var(--amber); margin-top: 1px; }
  .err.danger { border-color: var(--danger); }
  .err.danger :global(.icon) { color: var(--danger); }
  .err b { color: var(--text); }
</style>
