/* Lustro prototype: app updates. Update dialog, "ready" toast, "What's new" card and a small Markdown renderer.
   All data here is fake. The real app gets it from the Backend update methods (UpdateInfo / UpdateStatus). */
(function () {
  const Lustro = window.Lustro;
  const I = Lustro.icons;

  const CURRENT = "0.2.0";
  const FAKE = {
    version: "0.3.0", currentVersion: CURRENT, publishedAt: Date.UTC(2026, 9, 27) / 1000, sizeBytes: 14.2e6,
    notes: [
      "## What's new",
      "- **Faster scans** on large drives: the folder tree fills in about twice as fast.",
      "- **Terrain:** looking from below now shows the land's footprint, so cloud-only valleys are easier to read.",
      "- New **app icon** and logo.",
      "- Settings has a new **Updates** section.",
      "## Fixes",
      "- Hard-linked files are no longer counted twice in What grew.",
      "- The Cleanup queue keeps your place when you switch tabs.",
      "- `Copy path` now works for paths with spaces.",
      "## Notes",
      "Every update is signed. Lustro checks the signature before installing and refuses anything that doesn't match.",
    ].join("\n"),
  };

  const esc = (s) => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  // Simple Markdown: headings, bullets, paragraphs, **bold**, `code`. Everything is escaped first, so raw HTML in
  // release notes shows as text. Links stay plain text: nothing in the notes can open a page.
  function md(src) {
    let out = "", list = false;
    const inline = (t) => esc(t).replace(/\*\*(.+?)\*\*/g, "<b>$1</b>").replace(/`(.+?)`/g, "<code>$1</code>");
    src.split("\n").forEach((line) => {
      const li = line.match(/^\s*[-*] (.*)/);
      if (!li && list) { out += "</ul>"; list = false; }
      if (/^#{1,6} /.test(line)) out += `<h4>${inline(line.replace(/^#+ /, ""))}</h4>`;
      else if (li) { if (!list) { out += "<ul>"; list = true; } out += `<li>${inline(li[1])}</li>`; }
      else if (line.trim()) out += `<p>${inline(line)}</p>`;
    });
    return out + (list ? "</ul>" : "");
  }
  const mb = (b) => (b / 1e6).toFixed(1) + " MB";
  const date = (s) => new Date(s * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });

  // ---------- Update available dialog ----------
  // States: available -> downloading -> verifying -> installing (restart). Errors: download failed, not verified.
  function dialog(info, opts = {}) {
    const bg = document.createElement("div");
    bg.className = "modal-bg show upd-bg";
    bg.innerHTML = `<section class="card modal upd" role="dialog" aria-modal="true" aria-labelledby="upd-title">
      <div class="card-head"><div class="ic">${I.logo}</div><h2 id="upd-title">Update available</h2><span class="badge cloud">New</span></div>
      <div class="upd-ver"><span class="mono faint">${esc(info.currentVersion)}</span><span class="faint">→</span><span class="mono strong">${esc(info.version)}</span></div>
      <div class="upd-meta">Released ${date(info.publishedAt)} · ${mb(info.sizeBytes)} · from GitHub Releases, signature checked before install</div>
      <div class="upd-notes scroll">${md(info.notes)}</div>
      <div class="upd-busy" hidden></div>
      <div class="upd-foot">
        <button class="btn link" data-a="skip">Skip this version</button>
        <span class="spacer"></span>
        <button class="btn" data-a="later">Not now</button>
        <button class="btn primary" data-a="install">${I.cloudDown}Download and install</button>
      </div></section>`;
    document.body.appendChild(bg);
    const $ = (s) => bg.querySelector(s), close = () => bg.remove();
    const busyNote = opts.busy ? `<p class="faint">A scan is running. The update downloads now and installs when the scan finishes.</p>` : "";
    const setBusy = (html) => { $(".upd-busy").hidden = false; $(".upd-busy").innerHTML = html; };
    $('[data-a="later"]').onclick = close;
    $('[data-a="skip"]').onclick = () => { close(); opts.onSkip && opts.onSkip(info.version); };
    bg.onclick = (e) => { if (e.target === bg) close(); };
    bg.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
    $('[data-a="install"]').onclick = () => {
      $(".upd-foot").hidden = true; $(".upd-notes").classList.add("dim");
      let got = 0; const total = info.sizeBytes;
      const tick = setInterval(() => {
        got = Math.min(total, got + total / 14);
        const pct = Math.round((got / total) * 100);
        if (opts.fail === "download" && pct > 55) {
          clearInterval(tick);
          setBusy(`<div class="upd-err">${I.info}<div><b>Download failed.</b> Nothing was changed. Check your connection and try again.</div></div>
            <div class="upd-foot"><span class="spacer"></span><button class="btn" data-a="x">Close</button><button class="btn primary" data-a="retry">Try again</button></div>`);
          $('[data-a="x"]').onclick = close; $('[data-a="retry"]').onclick = () => { close(); dialog(info, { ...opts, fail: null }); };
          return;
        }
        setBusy(`<div class="upd-row"><b>Downloading ${esc(info.version)}</b><span class="mono faint">${mb(got)} of ${mb(total)}</span></div>
          <div class="progress"><i style="width:${pct}%"></i></div>${busyNote}<p class="faint">You can keep using Lustro while it downloads.</p>`);
        opts.onStatus && opts.onStatus({ state: "downloading", got, total });
        if (got >= total) { clearInterval(tick); verify(); }
      }, 160);
      function verify() {
        setBusy(`<div class="upd-row"><b>${I.shield}Checking signature</b><span class="spin"></span></div><div class="progress"><i style="width:100%"></i></div>`);
        setTimeout(() => {
          if (opts.fail === "verify") {
            setBusy(`<div class="upd-err danger">${I.shield}<div><b>This update couldn't be verified, so it wasn't installed.</b> The download didn't match Lustro's signature. Your current version is unchanged. Try again later; if it keeps happening, download the installer from the official GitHub Releases page.</div></div>
              <div class="upd-foot"><span class="spacer"></span><button class="btn" data-a="x">Close</button></div>`);
            $('[data-a="x"]').onclick = close; opts.onStatus && opts.onStatus({ state: "error", kind: "verify" });
            return;
          }
          if (opts.busy) {
            setBusy(`<div class="upd-row"><b>${I.shield}Downloaded and verified</b></div><p>Lustro ${esc(info.version)} will install when the scan finishes. We'll ask before restarting.</p>
              <div class="upd-foot"><span class="spacer"></span><button class="btn primary" data-a="x">OK</button></div>`);
            $('[data-a="x"]').onclick = close; opts.onStatus && opts.onStatus({ state: "ready" });
            return;
          }
          setBusy(`<div class="upd-row"><b>Installing ${esc(info.version)}</b><span class="spin"></span></div>
            <p>Lustro will restart in a moment. Your settings, snapshots and Cleanup decisions are kept.</p>`);
          setTimeout(() => restart(info), 1800);
        }, 1100);
      }
    };
    $('[data-a="install"]').focus();
    return bg;
  }

  // Prototype "restart": reopen Overview, which shows What's new once.
  function restart(info) { location.href = "index.html?updated=" + encodeURIComponent(info.version); }

  // ---------- Auto-update "ready" toast ----------
  // Never shown while a scan or cleanup is running; the caller holds it until that finishes.
  function toast(info, opts = {}) {
    document.querySelectorAll(".toast.upd-toast").forEach((t) => t.remove());
    const t = document.createElement("div");
    t.className = "toast upd-toast"; t.setAttribute("role", "status");
    t.innerHTML = `<div class="ic">${I.logo}</div>
      <div class="tx"><b>Lustro ${esc(info.version)} is ready</b><small>Downloaded and verified. It installs the next time Lustro starts.</small>
        <button class="btn link" data-a="notes">See what's new</button></div>
      <div class="acts"><button class="btn small" data-a="later">Later</button><button class="btn small primary" data-a="restart">Restart to update</button></div>`;
    document.body.appendChild(t);
    t.querySelector('[data-a="later"]').onclick = () => { t.remove(); opts.onLater && opts.onLater(); };
    t.querySelector('[data-a="restart"]').onclick = () => restart(info);
    t.querySelector('[data-a="notes"]').onclick = () => notesDialog(info);
    return t;
  }

  function notesDialog(info) {
    const bg = document.createElement("div");
    bg.className = "modal-bg show upd-bg";
    bg.innerHTML = `<section class="card modal upd"><div class="card-head"><div class="ic">${I.logo}</div><h2>What's new in ${esc(info.version)}</h2></div>
      <div class="upd-notes scroll">${md(info.notes)}</div>
      <div class="upd-foot"><span class="spacer"></span><button class="btn" data-a="x">Close</button><button class="btn primary" data-a="restart">Restart to update</button></div></section>`;
    document.body.appendChild(bg);
    bg.querySelector('[data-a="x"]').onclick = () => bg.remove();
    bg.querySelector('[data-a="restart"]').onclick = () => restart(info);
    bg.onclick = (e) => { if (e.target === bg) bg.remove(); };
  }

  // A small dot on the Settings gear while an update is ready but held back (scan or cleanup running, or "Later").
  function gearDot(on) {
    const g = document.querySelector('.top-actions .icon-btn[href="settings.html"]');
    if (!g) return;
    g.classList.toggle("has-dot", !!on);
    g.title = on ? "Settings · update ready" : g.title.replace(" · update ready", "");
  }

  // ---------- "What's new" card, shown once after an update ----------
  function whatsNew() {
    const v = new URLSearchParams(location.search).get("updated");
    if (!v) return;
    const c = document.createElement("section");
    c.className = "card whatsnew";
    c.innerHTML = `<div class="card-head"><div class="ic">${I.spark}</div><h2>Updated to ${esc(v)}</h2><span class="sub">Here's what changed</span>
      <div class="tools"><button class="icon-btn" title="Dismiss" aria-label="Dismiss">✕</button></div></div>
      <div class="wn-notes">${md(FAKE.notes)}</div>`;
    const page = document.querySelector(".page");
    const ctx = page.querySelector(".context");
    ctx ? ctx.after(c) : page.prepend(c);
    c.querySelector(".tools button").onclick = () => { c.remove(); history.replaceState(null, "", location.pathname); };
  }

  Lustro.updates = { CURRENT, FAKE, md, dialog, toast, notesDialog, gearDot, whatsNew, restart };
  document.addEventListener("DOMContentLoaded", whatsNew);
  if (document.readyState !== "loading") whatsNew();
})();
