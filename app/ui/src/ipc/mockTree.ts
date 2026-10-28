// Deterministic fake tree for the mock backend. The base tree (mockData.ts) is expanded into a large
// generated tree, stored as columns in typed arrays so a million nodes stay cheap.
// Node ids are array indices; a parent always has a smaller id than its children.

import type { Category, CloudProvider, CloudState, Platform } from "./types";
import { baseWorld, type BaseNode, type BaseWorld } from "./mockData";

export const GB = 1e9;
const CLUSTER = 4096;
/** Fixed "scan time" so the fake data is the same on every run: 2026-10-28 09:41 UTC. */
export const MOCK_NOW = Date.UTC(2026, 9, 28, 9, 41) / 1000;
const DAY = 86400;

export const CATS: Category[] = ["video", "images", "audio", "documents", "archives", "installers", "code", "apps", "system", "other"];
export const PROVIDERS: (CloudProvider | null)[] = [null, "oneDrive", "googleDrive", "iCloud", "dropbox", "box", "other"];
export const STATES: (CloudState | null)[] = [null, "cloudOnly", "downloaded", "pinned"];

export const K_FILE = 0, K_DIR = 1, K_OTHER = 2;
export const FL_HIDDEN = 1, FL_DENIED = 2, FL_BASE = 4, FL_NEW = 8, FL_REMOVED = 16, FL_SYNCROOT = 32;

export interface SysInfo { key: string; explanation: string; settingAction: string | null }

export interface MockTree {
  world: BaseWorld;
  count: number;
  parent: Int32Array;
  kind: Uint8Array;
  name: string[];
  allocated: Float64Array;
  logical: Float64Array;
  cloudOnly: Float64Array;
  pinned: Float64Array;
  delta: Float64Array;
  fileCount: Float64Array;
  dirCount: Float64Array;
  category: Uint8Array;
  modified: Float64Array;
  accessed: Float64Array;
  provider: Uint8Array;
  state: Uint8Array;
  flags: Uint8Array;
  depth: Uint8Array;
  childStart: Int32Array;
  childCount: Int32Array;
  childList: Int32Array;
  /** Per-directory category mix (10 slots per dir, display bytes); -1 for files. */
  dirSlot: Int32Array;
  mix: Float64Array;
  /** Stable keys from the base data -> node id. */
  keys: Map<string, number>;
  /** Key of the original for duplicates: node id -> original node id. */
  dupOf: Map<number, number>;
  stale: Set<number>;
  sys: Map<number, SysInfo>;
}

/** mulberry32: small, fast, deterministic. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hashStr(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** Display size: what a chart draws. Cloud-only bytes count here (hatched), but not as used space. */
export function disp(t: MockTree, id: number): number {
  return t.allocated[id] + t.cloudOnly[id];
}

// ---- Name generation ----

const pick = <T>(r: () => number, a: readonly T[]): T => a[Math.floor(r() * a.length)];
const num = (r: () => number, n: number, w = 4) => String(Math.floor(r() * n)).padStart(w, "0");

function fileName(cat: Category, r: () => number, p: Platform, ctx: string): string {
  switch (cat) {
    case "video": return pick(r, [`clip-${num(r, 9999)}.mp4`, `VID_2025${num(r, 1231)}_${num(r, 2359)}.mp4`, `take-${num(r, 999, 3)}.mov`, `render-${num(r, 99, 2)}.mkv`]);
    case "images": return pick(r, [`IMG_${num(r, 9999)}.jpg`, `DSC${num(r, 99999, 5)}.raw`, `photo-${num(r, 9999)}.heic`, `screenshot-${num(r, 999, 3)}.png`]);
    case "audio": return pick(r, [`track-${num(r, 99, 2)}.flac`, `song-${num(r, 999, 3)}.mp3`, `memo-${num(r, 999, 3)}.m4a`, `sample-${num(r, 99, 2)}.wav`]);
    case "documents": return pick(r, [`report-${num(r, 99, 2)}.pdf`, `notes-${num(r, 999, 3)}.docx`, `budget-${num(r, 99, 2)}.xlsx`, `slides-${num(r, 99, 2)}.pptx`, `scan-${num(r, 9999)}.pdf`]);
    case "archives": return pick(r, [`backup-part${num(r, 9, 1)}.zip`, `export-${num(r, 999, 3)}.7z`, `bundle-${num(r, 99, 2)}.tar.gz`]);
    case "installers": return p === "windows" ? pick(r, [`setup-${num(r, 999, 3)}-x64.exe`, `installer-${num(r, 99, 2)}.msi`])
      : p === "macos" ? pick(r, [`App-${num(r, 999, 3)}.dmg`, `package-${num(r, 99, 2)}.pkg`])
      : pick(r, [`app-${num(r, 999, 3)}.deb`, `tool-${num(r, 99, 2)}.AppImage`]);
    case "code": return ctx === "node_modules" ? pick(r, ["index.js", "package.json", "README.md", "LICENSE", `chunk-${num(r, 999, 3)}.js`, "index.d.ts"])
      : pick(r, [`module-${num(r, 999, 3)}.ts`, `main-${num(r, 99, 2)}.py`, `lib${num(r, 99, 2)}.so`, `pack-${num(r, 9999)}.pack`, `data-${num(r, 999, 3)}.bin`, `cache-${num(r, 9999)}.pyc`]);
    case "apps": return p === "windows" ? pick(r, [`data${num(r, 99, 2)}.pak`, `lib${num(r, 99, 2)}.dll`, `assets-${num(r, 999, 3)}.bundle`, `app-${num(r, 9, 1)}.exe`])
      : p === "macos" ? pick(r, [`data${num(r, 99, 2)}.pak`, `lib${num(r, 99, 2)}.dylib`, `assets-${num(r, 999, 3)}.car`])
      : pick(r, [`data${num(r, 99, 2)}.pak`, `lib${num(r, 99, 2)}.so`, `assets-${num(r, 999, 3)}.bin`]);
    case "system": return p === "windows" ? pick(r, [`${pick(r, ["kernel", "net", "ui", "crypt", "audio"])}${num(r, 99, 2)}.dll`, `res-${num(r, 999, 3)}.mui`, `update-${num(r, 9999)}.cab`, `trace-${num(r, 999, 3)}.log`])
      : pick(r, [`lib${pick(r, ["core", "net", "ui", "crypto"])}${num(r, 99, 2)}.so`, `log-${num(r, 999, 3)}.log`, `rev-${num(r, 999, 3)}.img`]);
    default: return pick(r, [`f_${num(r, 99999, 5)}`, `cache_${num(r, 9999)}`, `tmp${num(r, 9999)}.tmp`, `data_${num(r, 999, 3)}.dat`]);
  }
}

function dirName(cat: Category, r: () => number, ctx: string, depth: number): string {
  if (ctx === "node_modules" && depth === 0) return pick(r, ["@scope-", "lib-", "util-", "core-", "ui-"]) + num(r, 9999);
  switch (cat) {
    case "video": return pick(r, ["clips", "raw", "exports", "proxies", `shoot-${num(r, 99, 2)}`]);
    case "images": return pick(r, [`${pick(r, ["Jan", "Mar", "May", "Jul", "Sep", "Nov"])} ${2019 + Math.floor(r() * 7)}`, "edits", "originals", `album-${num(r, 99, 2)}`]);
    case "audio": return pick(r, ["album", "stems", "podcasts", `disc-${num(r, 9, 1)}`]);
    case "documents": return pick(r, ["drafts", "final", "archive", "shared", `project-${num(r, 99, 2)}`]);
    case "code": return pick(r, ["src", "lib", "dist", "build", "objects", "tests", "vendor", `pkg-${num(r, 999, 3)}`]);
    case "apps": return pick(r, ["data", "assets", "bin", "resources", "plugins", `content-${num(r, 99, 2)}`]);
    case "system": return pick(r, ["drivers", "catalog", "en-US", "config", `x64-${num(r, 999, 3)}`]);
    default: return pick(r, ["cache", "blobs", "store", "parts", `d${num(r, 999, 3)}`]);
  }
}

/** How eagerly a base item spreads into many small files. */
const CAT_DENSITY: Record<Category, number> = {
  video: 0.4, images: 2, audio: 1.5, documents: 2, archives: 0.5, installers: 0.6, code: 6, apps: 2, system: 3, other: 3,
};

const looksHidden = (n: string) => n.startsWith(".") || n.startsWith("$") || n === "AppData" || n === "ProgramData" || n === "System Volume Information" || n === "private";

// ---- Build ----

interface Builder {
  t: MockTree;
  r: () => number;
}

export function buildMockTree(platform: Platform, targetNodes: number, seed = 7): MockTree {
  const world = baseWorld(platform);

  // Pass 1: collect base nodes, count them, and weigh the expandable leaves.
  const baseNodes: BaseNode[] = [];
  const expandable: { n: BaseNode; w: number }[] = [];
  (function walk(n: BaseNode) {
    baseNodes.push(n);
    if (n.children) n.children.forEach(walk);
    else if (!n.file && !n.other && !n.accessDenied) expandable.push({ n, w: Math.sqrt(n.size ?? 0) * CAT_DENSITY[n.cat ?? "other"] });
  })(world.root);
  const extra = Math.max(0, targetNodes - baseNodes.length);
  const wsum = expandable.reduce((s, e) => s + e.w, 0);
  const budgets = new Map<BaseNode, number>();
  let given = 0;
  expandable.forEach((e) => {
    const b = Math.floor((extra * e.w) / wsum);
    budgets.set(e.n, b);
    given += b;
  });
  // Hand the rounding remainder to the largest item.
  if (expandable.length) {
    const top = expandable.reduce((a, b) => (b.w > a.w ? b : a));
    budgets.set(top.n, (budgets.get(top.n) ?? 0) + (extra - given));
  }
  const cap = baseNodes.length + extra;

  const t: MockTree = {
    world, count: 0,
    parent: new Int32Array(cap), kind: new Uint8Array(cap), name: new Array<string>(cap),
    allocated: new Float64Array(cap), logical: new Float64Array(cap), cloudOnly: new Float64Array(cap),
    pinned: new Float64Array(cap), delta: new Float64Array(cap),
    fileCount: new Float64Array(cap), dirCount: new Float64Array(cap), category: new Uint8Array(cap),
    modified: new Float64Array(cap), accessed: new Float64Array(cap),
    provider: new Uint8Array(cap), state: new Uint8Array(cap), flags: new Uint8Array(cap), depth: new Uint8Array(cap),
    childStart: new Int32Array(cap), childCount: new Int32Array(cap), childList: new Int32Array(cap),
    dirSlot: new Int32Array(cap).fill(-1), mix: new Float64Array(0),
    keys: new Map(), dupOf: new Map(), stale: new Set(), sys: new Map(),
  };
  const b: Builder = { t, r: rng(seed) };

  const add = (parent: number, name: string, kind: number, cat: Category, prov: number, st: number, depth: number): number => {
    const id = t.count++;
    t.parent[id] = parent;
    t.name[id] = name;
    t.kind[id] = kind;
    t.category[id] = CATS.indexOf(cat);
    t.provider[id] = prov;
    t.state[id] = st;
    t.depth[id] = Math.min(255, depth);
    if (looksHidden(name)) t.flags[id] |= FL_HIDDEN;
    return id;
  };

  const setFile = (id: number, bytes: number, st: number, delta: number, mtime: number, r: () => number) => {
    const logical = Math.max(1, Math.round(bytes));
    t.logical[id] = logical;
    if (STATES[st] === "cloudOnly") {
      t.cloudOnly[id] = logical;
    } else {
      t.allocated[id] = Math.ceil(logical / CLUSTER) * CLUSTER;
      if (STATES[st] === "pinned") t.pinned[id] = t.allocated[id];
    }
    t.delta[id] = delta;
    t.modified[id] = mtime;
    t.accessed[id] = Math.min(MOCK_NOW, mtime + r() * 120 * DAY);
  };

  const dupNames: [number, string][] = [];

  // Pass 2: emit nodes depth-first.
  const emit = (n: BaseNode, parent: number, depth: number, prov: number, st: number) => {
    const p = n.provider ? PROVIDERS.indexOf(n.provider) : prov;
    const s = n.state ? STATES.indexOf(n.state) : st;
    const isLeaf = !n.children;
    const expand = isLeaf && (budgets.get(n) ?? 0) > 0;
    const kind = n.other ? K_OTHER : isLeaf && !expand && !n.accessDenied ? K_FILE : K_DIR;
    const name = parent < 0 ? world.rootPath : n.name;
    const id = add(parent, name, kind, n.cat ?? "other", p, s, depth);
    if (parent >= 0) t.flags[id] |= FL_BASE;
    if (n.syncRoot) t.flags[id] |= FL_SYNCROOT;
    if (n.isNew) t.flags[id] |= FL_NEW;
    if (n.accessDenied) t.flags[id] |= FL_DENIED;
    if (n.key) t.keys.set(n.key, id);
    if (n.dupOf) dupNames.push([id, n.dupOf]);
    if (n.stale) t.stale.add(id);
    if (n.sys) t.sys.set(id, n.sys);

    const r = rng(seed ^ hashStr(n.name + ":" + depth + ":" + (n.size ?? 0)));
    const mtime = n.d && n.d > 0 ? MOCK_NOW - (0.2 + r() * 6) * DAY
      : n.stale ? Date.UTC(2024, 1 + Math.floor(r() * 4), 1 + Math.floor(r() * 27)) / 1000
      : MOCK_NOW - (30 + r() * 900) * DAY;

    if (n.children) {
      n.children.forEach((c) => emit(c, id, depth + 1, p, s));
      return;
    }
    const bytes = (n.size ?? 0) * GB;
    const delta = (n.d ?? 0) * GB;
    if (kind === K_FILE || kind === K_OTHER) {
      setFile(id, bytes, s, n.isNew ? Math.ceil(bytes / CLUSTER) * CLUSTER : delta, mtime, r);
      return;
    }
    if (n.accessDenied) {
      // Can't be read without admin: the size comes from the volume, the contents stay unknown.
      t.allocated[id] = Math.round(bytes);
      t.logical[id] = Math.round(bytes);
      t.modified[id] = mtime;
      return;
    }
    generate(b, id, budgets.get(n) ?? 0, bytes, delta / Math.max(bytes, 1), n.cat ?? "other", p, s, depth + 1, mtime, n.name, 0, r);
  };

  function generate(bd: Builder, parent: number, budget: number, bytes: number, deltaRatio: number, cat: Category,
    prov: number, st: number, depth: number, mtime: number, ctx: string, level: number, r: () => number) {
    if (budget <= 0) return;
    let nFiles: number, nDirs: number;
    if (budget <= 24 || level >= 9) {
      nFiles = budget;
      nDirs = 0;
    } else {
      nFiles = Math.min(budget - 1, 2 + Math.floor(r() * 14));
      const rem = budget - nFiles;
      nDirs = Math.max(1, Math.min(Math.floor(rem / 2), Math.round(Math.pow(rem, 0.4) * (0.6 + r() * 0.8)), 60));
    }
    // Every subfolder gets at least one descendant; the rest is spread with random weights.
    const sub = budget - nFiles - nDirs * 2;
    // Split the remaining descendants across folders with random weights.
    const dw: number[] = [];
    let dws = 0;
    for (let i = 0; i < nDirs; i++) { const w = Math.exp(r() * 2.5); dw.push(w); dws += w; }
    const dBudget = dw.map((w) => 1 + Math.floor((sub * w) / dws));
    let left = sub + nDirs - dBudget.reduce((a, c) => a + c, 0);
    for (let i = 0; left > 0; i = (i + 1) % Math.max(1, nDirs), left--) dBudget[i]++;
    // Split bytes: folders by budget share, files lognormal.
    const weights: number[] = [];
    const meanFile = 1;
    for (let i = 0; i < nDirs; i++) weights.push((dBudget[i] + 1) * meanFile * (0.5 + r()));
    for (let i = 0; i < nFiles; i++) weights.push(Math.exp((r() + r() + r() - 1.5) * 2.2));
    const wTotal = weights.reduce((a, c) => a + c, 0);

    const seen = new Set<string>();
    const unique = (s: string) => {
      let x = s, k = 1;
      while (seen.has(x)) {
        const dot = s.lastIndexOf(".");
        x = dot > 0 ? `${s.slice(0, dot)}-${k}${s.slice(dot)}` : `${s}-${k}`;
        k++;
      }
      seen.add(x);
      return x;
    };
    for (let i = 0; i < nDirs; i++) {
      const id = add(parent, unique(dirName(cat, r, ctx, level)), K_DIR, cat, prov, st, depth);
      generate(bd, id, dBudget[i], (bytes * weights[i]) / wTotal, deltaRatio, cat, prov, st, depth + 1,
        mtime - r() * 60 * DAY, ctx, level + 1, r);
    }
    for (let i = 0; i < nFiles; i++) {
      const id = add(parent, unique(fileName(cat, r, platform, ctx)), K_FILE, cat, prov, st, depth);
      const fb = (bytes * weights[nDirs + i]) / wTotal;
      setFile(id, fb, st, fb * deltaRatio, mtime - r() * 200 * DAY, r);
    }
  }

  emit(world.root, -1, 0, 0, 0);
  for (const [id, key] of dupNames) {
    const o = t.keys.get(key);
    if (o !== undefined) t.dupOf.set(id, o);
  }
  finalize(t);
  return t;
}

/** Aggregate sizes, counts and category mix bottom-up, then build sorted child lists. */
export function finalize(t: MockTree) {
  const n = t.count;
  let dirs = 0;
  for (let i = 0; i < n; i++) if (t.kind[i] === K_DIR) t.dirSlot[i] = dirs++;
  t.mix = new Float64Array(dirs * 10);
  // Reset aggregates on dirs (files keep theirs), except access-denied folders, which carry a volume size.
  for (let i = 0; i < n; i++) {
    if (t.kind[i] !== K_DIR) { t.fileCount[i] = 0; t.dirCount[i] = 0; continue; }
    t.fileCount[i] = 0; t.dirCount[i] = 0;
    if (t.flags[i] & FL_DENIED) continue;
    t.allocated[i] = 0; t.logical[i] = 0; t.cloudOnly[i] = 0; t.pinned[i] = 0; t.delta[i] = 0; t.modified[i] = 0; t.accessed[i] = 0;
  }
  for (let i = n - 1; i >= 0; i--) {
    if (t.flags[i] & FL_REMOVED) continue;
    const p = t.parent[i];
    if (t.kind[i] === K_DIR) {
      const s = t.dirSlot[i] * 10;
      if (t.flags[i] & FL_DENIED) t.mix[s + CATS.indexOf("system")] += t.allocated[i];
      // Dominant category by display bytes.
      let best = -1, bv = 0;
      for (let c = 0; c < 10; c++) if (t.mix[s + c] > bv) { bv = t.mix[s + c]; best = c; }
      if (best >= 0) t.category[i] = best;
      const st = derivedState(t, i);
      if (st) t.state[i] = st;
    }
    if (p < 0) continue;
    t.allocated[p] += t.allocated[i];
    t.logical[p] += t.logical[i];
    t.cloudOnly[p] += t.cloudOnly[i];
    t.pinned[p] += t.pinned[i];
    t.delta[p] += t.delta[i];
    t.modified[p] = Math.max(t.modified[p], t.modified[i]);
    t.accessed[p] = Math.max(t.accessed[p], t.accessed[i]);
    if (t.kind[i] === K_DIR) {
      t.dirCount[p] += 1 + t.dirCount[i];
      t.fileCount[p] += t.fileCount[i];
      const s = t.dirSlot[i] * 10, ps = t.dirSlot[p] * 10;
      for (let c = 0; c < 10; c++) t.mix[ps + c] += t.mix[s + c];
    } else {
      t.fileCount[p] += 1;
      t.mix[t.dirSlot[p] * 10 + t.category[i]] += disp(t, i);
    }
  }
  buildChildren(t);
}

function derivedState(t: MockTree, i: number): number {
  if (!t.provider[i]) return 0;
  if (t.allocated[i] === 0 && t.cloudOnly[i] > 0) return STATES.indexOf("cloudOnly");
  if (t.allocated[i] > 0 && t.pinned[i] >= t.allocated[i] && t.cloudOnly[i] === 0) return STATES.indexOf("pinned");
  return STATES.indexOf("downloaded");
}

/** Child lists grouped by parent (counting sort), each sorted by display size, largest first. */
export function buildChildren(t: MockTree) {
  const n = t.count;
  t.childCount.fill(0);
  for (let i = 1; i < n; i++) if (!(t.flags[i] & FL_REMOVED)) t.childCount[t.parent[i]]++;
  let acc = 0;
  for (let i = 0; i < n; i++) { t.childStart[i] = acc; acc += t.childCount[i]; }
  const fill = new Int32Array(n);
  for (let i = 1; i < n; i++) {
    if (t.flags[i] & FL_REMOVED) continue;
    const p = t.parent[i];
    t.childList[t.childStart[p] + fill[p]++] = i;
  }
  for (let i = 0; i < n; i++) {
    const c = t.childCount[i];
    if (c > 1) {
      const s = t.childStart[i];
      const sub = t.childList.subarray(s, s + c);
      sub.sort((a, b) => disp(t, b) - disp(t, a) || a - b);
    }
  }
}

export function childrenOf(t: MockTree, id: number): Int32Array {
  return t.childList.subarray(t.childStart[id], t.childStart[id] + t.childCount[id]);
}

export function pathOf(t: MockTree, id: number): string {
  const parts: string[] = [];
  for (let x = id; x > 0; x = t.parent[x]) parts.push(t.name[x]);
  const root = t.world.rootPath;
  if (!parts.length) return root;
  parts.reverse();
  return root.endsWith(t.world.sep) ? root + parts.join(t.world.sep) : root + t.world.sep + parts.join(t.world.sep);
}
