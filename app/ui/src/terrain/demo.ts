// Dev-only demo page for the terrain module (served by Vite at /src/terrain/demo.html; not part
// of the production build, which only bundles index.html). Fake data only.

import "@fontsource/manrope/500.css";
import "@fontsource/manrope/700.css";
import "./demo.css";
import type { NodeId, TreeDto } from "../ipc/types";
import type { TerrainHandle, TerrainOptions, TerrainPin } from "./types";
import { mountTerrain } from "./index";
import { bigTree, countNodes, demoTree } from "./fakeTree";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const host = $("terrain");
const readout = $("readout");

let tree: TreeDto = demoTree();
let parents = new Map<NodeId, TreeDto | null>();
let pinSet: TerrainPin[] = [];
const opts: TerrainOptions = {
  categoryTint: true, pins: [], cloudValleys: true, flat: new URLSearchParams(location.search).has("flat"),
  compact: false, ringLabels: true, theme: "dark",
};
let handle: TerrainHandle;

function index(t: TreeDto): void {
  parents = new Map();
  const stack: [TreeDto, TreeDto | null][] = [[t, null]];
  while (stack.length) {
    const [n, p] = stack.pop()!;
    parents.set(n.id, p);
    for (const c of n.children) stack.push([c, n]);
  }
}

function files(t: TreeDto): TreeDto[] {
  const out: TreeDto[] = [];
  const stack = [t];
  while (stack.length) { const n = stack.pop()!; if (n.kind === "file") out.push(n); else stack.push(...n.children); }
  return out;
}

/** Same picks the app makes: two largest files, three top growers, duplicates, cloud-only. */
function makePins(t: TreeDto): TerrainPin[] {
  const fs = files(t);
  const pins: TerrainPin[] = [];
  const add = (kind: TerrainPin["kind"], n: TreeDto) => { if (!pins.some((p) => p.nodeId === n.id)) pins.push({ kind, nodeId: n.id, label: n.name }); };
  fs.filter((f) => f.allocated > 0).sort((a, b) => b.allocated - a.allocated).slice(0, 2).forEach((n) => add("largest", n));
  fs.filter((f) => (f.delta ?? 0) > 0).sort((a, b) => (b.delta ?? 0) - (a.delta ?? 0)).slice(0, 3).forEach((n) => add("grew", n));
  const bySize = new Map<number, TreeDto[]>();
  for (const f of fs) if (f.allocated > 1024 ** 3) bySize.set(f.allocated, [...(bySize.get(f.allocated) ?? []), f]);
  [...bySize.values()].filter((g) => g.length > 1).slice(0, 2).forEach((g) => add("duplicate", g[0]));
  fs.filter((f) => f.cloudOnly > 0).sort((a, b) => b.cloudOnly - a.cloudOnly).slice(0, 2).forEach((n) => add("cloud", n));
  return pins;
}

function pathTo(id: NodeId): NodeId[] {
  const out: NodeId[] = [];
  let n: TreeDto | null | undefined = find(id);
  while (n) { out.unshift(n.id); n = parents.get(n.id); }
  return out;
}

function find(id: NodeId): TreeDto | undefined {
  const stack = [tree];
  while (stack.length) { const n = stack.pop()!; if (n.id === id) return n; stack.push(...n.children); }
  return undefined;
}

function setData(t: TreeDto): void {
  tree = t;
  index(t);
  pinSet = makePins(t);
  opts.pins = ($("pins") as HTMLInputElement).checked ? pinSet : [];
}

function mount(): void {
  handle = mountTerrain(host, tree, { ...opts }, {
    onSelect: (id) => {
      const path = pathTo(id);
      handle.select(id, path);
      const names = path.map((p) => find(p)?.name ?? "?");
      readout.textContent = `Selected:\n${names.join(" › ")}`;
    },
    onHover: () => {},
    onFullViewChange: (full) => { $("fullview").textContent = full ? "full view" : "zoomed / moved"; },
    onAutoFlat: () => {
      $("perf").hidden = false;
      opts.flat = true;
      syncButtons();
    },
  });
}

function syncButtons(): void {
  $("flat").setAttribute("aria-pressed", String(opts.flat));
  $("theme").textContent = opts.theme === "dark" ? "Light theme" : "Dark theme";
}

setData(tree);
mount();
syncButtons();

$("theme").onclick = () => {
  opts.theme = opts.theme === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = opts.theme;
  handle.setOptions({ theme: opts.theme });
  syncButtons();
};
$("flat").onclick = () => { opts.flat = !opts.flat; handle.setFlat(opts.flat); syncButtons(); };
let under = false;
$("under").onclick = () => { under = !under; handle.lookUnder(under); $("under").setAttribute("aria-pressed", String(under)); };
$("home").onclick = () => handle.flyTo(tree.id);
$("reset").onclick = () => handle.resetView();
$<HTMLInputElement>("spin").onchange = (e) => handle.setOptions({ autoRotate: (e.target as HTMLInputElement).checked });
$("try3d").onclick = () => { $("perf").hidden = true; opts.flat = false; handle.setFlat(false); syncButtons(); };

const bind = (id: string, key: "categoryTint" | "cloudValleys" | "ringLabels" | "compact") => {
  const el = $<HTMLInputElement>(id);
  el.onchange = () => {
    opts[key] = el.checked;
    if (key === "compact") host.parentElement!.classList.toggle("compact", el.checked);
    handle.setOptions({ [key]: el.checked });
  };
};
bind("tint", "categoryTint");
bind("valleys", "cloudValleys");
bind("labels", "ringLabels");
bind("compact", "compact");
$<HTMLInputElement>("pins").onchange = (e) => { opts.pins = (e.target as HTMLInputElement).checked ? pinSet : []; handle.setOptions({ pins: opts.pins }); };

document.querySelectorAll<HTMLButtonElement>("[data-fly]").forEach((b) => {
  b.onclick = () => {
    const p = pinSet.find((x) => x.kind === b.dataset.fly);
    if (!p) return;
    handle.select(p.nodeId, pathTo(p.nodeId));
    handle.flyTo(p.nodeId);
    readout.textContent = `Flew to:\n${pathTo(p.nodeId).map((x) => find(x)?.name ?? "?").join(" › ")}`;
  };
});

$("big").onclick = () => {
  readout.textContent = "Building 1M-node tree…";
  setTimeout(() => {
    const t0 = performance.now();
    const t = bigTree(1_000_000);
    const t1 = performance.now();
    setData(t);
    const t2 = performance.now();
    handle.setData(t);
    handle.setOptions({ pins: opts.pins });
    const t3 = performance.now();
    readout.textContent = `Nodes: ${countNodes(t).toLocaleString()}\nGenerate: ${Math.round(t1 - t0)} ms\nPin picks: ${Math.round(t2 - t1)} ms\nsetData + pins: ${Math.round(t3 - t2)} ms`;
  }, 30);
};
$("small").onclick = () => { setData(demoTree()); handle.setData(tree); handle.setOptions({ pins: opts.pins }); readout.textContent = ""; };
$("remount").onclick = () => { handle.dispose(); mount(); readout.textContent = `Remounted. Children in host: ${host.childElementCount}`; };

// Slow GPU simulation: burn ~45 ms per frame and keep the camera moving, so the terrain's own
// frame-rate watch sees a sustained low rate and switches to 2D.
let slow = false;
$("slow").onclick = () => { slow = !slow; $("slow").setAttribute("aria-pressed", String(slow)); };
let flip = false, lastFly = 0;

// FPS readout (whole page).
let frames = 0, since = performance.now();
const loop = (now: number) => {
  requestAnimationFrame(loop);
  if (slow) {
    const until = performance.now() + 45;
    while (performance.now() < until) { /* busy */ }
    if (now - lastFly > 900) {
      lastFly = now;
      flip = !flip;
      handle.lookUnder(flip);
    }
  }
  frames++;
  if (now - since >= 500) {
    $("fps").textContent = `${Math.round((frames * 1000) / (now - since))} fps`;
    frames = 0; since = now;
  }
};
requestAnimationFrame(loop);

// Expose for manual poking in the console during development.
(window as unknown as { terrain: () => TerrainHandle }).terrain = () => handle;
