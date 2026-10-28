// Fake trees for the demo page and tests. Fake names only, never real paths.

import type { Category, CloudInfo, TreeDto } from "../ipc/types";

/** Small deterministic PRNG (mulberry32). */
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

const GB = 1024 ** 3, MB = 1024 ** 2;

interface FileSpec { name: string; cat: Category; size: number; cloud?: CloudInfo["state"]; delta?: number }
type Spec = FileSpec | { name: string; kids: Spec[]; delta?: number };

class Builder {
  private next = 1;
  build(spec: Spec, parent: number | null): TreeDto {
    const id = this.next++;
    if (!("kids" in spec)) {
      const cloudOnly = spec.cloud === "cloudOnly";
      return {
        id, parent, name: spec.name, kind: "file",
        allocated: cloudOnly ? 0 : spec.size, logical: spec.size, cloudOnly: cloudOnly ? spec.size : 0,
        fileCount: 1, dirCount: 0, category: spec.cat, modified: 1761600000, accessed: null,
        cloud: spec.cloud ? { provider: "oneDrive", state: spec.cloud } : null,
        delta: spec.delta ?? 0, hidden: false, accessDenied: false, notDescended: null, childCount: 0, children: [],
      };
    }
    const children = spec.kids.map((k) => this.build(k, id));
    return dirNode(id, parent, spec.name, children);
  }
}

export function dirNode(id: number, parent: number | null, name: string, children: TreeDto[]): TreeDto {
  const sum = (f: (c: TreeDto) => number) => children.reduce((a, c) => a + f(c), 0);
  const byCat = new Map<Category, number>();
  for (const c of children) byCat.set(c.category, (byCat.get(c.category) ?? 0) + c.allocated);
  let category: Category = "other", best = -1;
  for (const [k, v] of byCat) if (v > best) { best = v; category = k; }
  return {
    id, parent, name, kind: "dir",
    allocated: sum((c) => c.allocated), logical: sum((c) => c.logical), cloudOnly: sum((c) => c.cloudOnly),
    fileCount: sum((c) => c.fileCount), dirCount: sum((c) => c.dirCount + (c.kind === "dir" ? 1 : 0)),
    category, modified: 1761600000, accessed: null, cloud: null, delta: sum((c) => c.delta ?? 0),
    hidden: false, accessDenied: false, notDescended: null, childCount: children.length, children,
  };
}

/** A filler folder of `count` random files of one category. */
function many(r: () => number, prefix: string, ext: string, cat: Category, count: number, avg: number): FileSpec[] {
  return Array.from({ length: count }, (_, i) => ({ name: `${prefix}${String(i + 1).padStart(3, "0")}.${ext}`, cat, size: Math.round(avg * (0.2 + r() * r() * 3)) }));
}

/** A believable Windows drive, ~1,500 nodes. Includes cloud-only files, growers and a duplicate pair. */
export function demoTree(seed = 7): TreeDto {
  const r = rng(seed);
  const spec: Spec = {
    name: "C:\\", kids: [
      { name: "Users", kids: [{ name: "William", kids: [
        { name: "Videos", kids: [
          { name: "Holiday 2025.mp4", cat: "video", size: 9.2 * GB, delta: 9.2 * GB },
          { name: "Screen recordings", kids: many(r, "Recording ", "mp4", "video", 30, 0.25 * GB) },
          { name: "Family", kids: many(r, "Clip ", "mov", "video", 40, 0.3 * GB) },
        ] },
        { name: "Pictures", kids: [
          { name: "Camera Roll", kids: many(r, "IMG_", "heic", "images", 400, 4 * MB) },
          { name: "RAW 2024", kids: many(r, "DSC", "arw", "images", 180, 48 * MB) },
          { name: "Wallpapers", kids: many(r, "wall", "png", "images", 30, 6 * MB) },
        ] },
        { name: "Music", kids: [
          { name: "Library", kids: many(r, "Track ", "flac", "audio", 220, 32 * MB) },
          { name: "Podcasts", kids: many(r, "Episode ", "mp3", "audio", 50, 60 * MB) },
        ] },
        { name: "Documents", kids: [
          { name: "Taxes", kids: many(r, "Statement ", "pdf", "documents", 40, 2 * MB) },
          { name: "Work", kids: many(r, "Report ", "docx", "documents", 120, 3 * MB) },
          { name: "Backup 2023.zip", cat: "archives", size: 6.2 * GB },
          { name: "Backup 2023 (copy).zip", cat: "archives", size: 6.2 * GB },
        ] },
        { name: "Downloads", kids: [
          { name: "Installers", kids: many(r, "setup-", "exe", "installers", 35, 180 * MB) },
          { name: "ubuntu-24.04.iso", cat: "archives", size: 5.7 * GB, delta: 5.7 * GB },
          { name: "Archives", kids: many(r, "bundle-", "zip", "archives", 25, 400 * MB) },
        ] },
        { name: "OneDrive", kids: [
          { name: "Photos archive", kids: many(r, "Archive ", "jpg", "images", 120, 40 * MB).map((f) => ({ ...f, cloud: "cloudOnly" as const })) },
          { name: "Old projects", kids: many(r, "Project ", "zip", "archives", 20, 900 * MB).map((f) => ({ ...f, cloud: "cloudOnly" as const })) },
          { name: "Shared", kids: many(r, "Notes ", "docx", "documents", 40, 5 * MB).map((f, i) => ({ ...f, cloud: i % 3 ? "downloaded" as const : "pinned" as const })) },
        ] },
        { name: "AppData", kids: [
          { name: "Local", kids: [
            { name: "Temp", kids: many(r, "tmp", "tmp", "other", 120, 20 * MB), delta: 1.1 * GB },
            { name: "npm-cache", kids: many(r, "pkg", "tgz", "code", 300, 6 * MB) },
            { name: "Browser Cache", kids: many(r, "f_", "bin", "other", 200, 3 * MB) },
          ] },
          { name: "Roaming", kids: many(r, "profile", "dat", "apps", 30, 50 * MB) },
        ] },
        { name: "source", kids: [
          { name: "repos", kids: [
            { name: "website", kids: [{ name: "node_modules", kids: many(r, "module", "js", "code", 250, 2 * MB) }] },
            { name: "game-engine", kids: [{ name: "build", kids: many(r, "obj", "o", "code", 160, 25 * MB), delta: 3.2 * GB }] },
          ] },
        ] },
      ] }] },
      { name: "Windows", kids: [
        { name: "WinSxS", kids: many(r, "component", "dll", "system", 300, 30 * MB) },
        { name: "System32", kids: many(r, "lib", "dll", "system", 250, 12 * MB) },
        { name: "Installer", kids: many(r, "patch", "msp", "installers", 40, 90 * MB) },
      ] },
      { name: "Program Files", kids: [
        { name: "Office", kids: many(r, "app", "dll", "apps", 60, 40 * MB) },
        { name: "Photo Editor", kids: many(r, "plugin", "dll", "apps", 40, 50 * MB) },
        { name: "Code Editor", kids: many(r, "res", "pak", "apps", 30, 30 * MB) },
      ] },
      { name: "Games", kids: [
        { name: "Space Racer", kids: many(r, "pak", "pak", "apps", 40, 1.4 * GB), delta: 4.5 * GB },
        { name: "Castle Builder", kids: many(r, "data", "bin", "apps", 20, 1.4 * GB) },
      ] },
      { name: "ProgramData", kids: many(r, "cache", "db", "other", 60, 40 * MB) },
      { name: "pagefile.sys", cat: "system", size: 12 * GB },
      { name: "hiberfil.sys", cat: "system", size: 6.4 * GB },
    ],
  };
  return new Builder().build(spec, null);
}

const CATS: Category[] = ["video", "images", "audio", "documents", "archives", "installers", "code", "apps", "system", "other"];

/**
 * A large random tree with about `count` nodes, for level-of-detail and performance testing.
 * Built iteratively (no deep recursion), sizes heavy-tailed like real disks.
 */
export function bigTree(count: number, seed = 1): TreeDto {
  const r = rng(seed);
  let id = 1;
  const files = (parent: number, n: number): TreeDto[] => {
    const out: TreeDto[] = [];
    for (let i = 0; i < n; i++) {
      const cat = CATS[Math.floor(r() * CATS.length)];
      const cloudOnly = r() < 0.03;
      const size = Math.round(4096 + Math.pow(r(), 6) * 4 * GB);
      out.push({
        id: id++, parent, name: `file${id}.dat`, kind: "file", allocated: cloudOnly ? 0 : size, logical: size,
        cloudOnly: cloudOnly ? size : 0, fileCount: 1, dirCount: 0, category: cat, modified: null, accessed: null,
        cloud: cloudOnly ? { provider: "other", state: "cloudOnly" } : null, delta: r() < 0.01 ? size : 0,
        hidden: false, accessDenied: false, notDescended: null, childCount: 0, children: [],
      });
    }
    return out;
  };
  // Build top-down, then aggregate bottom-up.
  const rootId = id++;
  const root = dirNode(rootId, null, "C:\\", []);
  const queue: [TreeDto, number][] = [[root, 0]];
  let made = 1;
  for (let q = 0; q < queue.length; q++) {
    const [d, depth] = queue[q];
    const left = count - made;
    if (left <= 0) continue;
    const nDirs = depth > 9 ? 0 : Math.min(left, Math.floor(2 + r() * (depth < 2 ? 10 : 5)));
    const nFiles = Math.min(left - nDirs, Math.floor(r() * r() * 400));
    for (let i = 0; i < nDirs; i++) {
      const c = dirNode(id++, d.id, `Folder ${id}`, []);
      d.children.push(c);
      queue.push([c, depth + 1]);
    }
    d.children.push(...files(d.id, nFiles));
    made += nDirs + nFiles;
  }
  for (let i = queue.length - 1; i >= 0; i--) {
    const d = queue[i][0];
    const agg = dirNode(d.id, d.parent, d.name, d.children);
    Object.assign(d, agg, { children: d.children });
  }
  return root;
}

/** Counts nodes without recursion. */
export function countNodes(t: TreeDto): number {
  let n = 0;
  const stack = [t];
  while (stack.length) { const x = stack.pop()!; n++; for (const c of x.children) stack.push(c); }
  return n;
}
