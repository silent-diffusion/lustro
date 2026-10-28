// Elevation: a cushion surface over the treemap, plus cloud-only valleys below ground.
// Pure functions only (no DOM, no three.js) so they can be unit-tested.
//
// Coordinates: the layout lives in the square [-1, 1]² (sx, sy). The ground is that square warped
// onto the unit disc (dx, dz) with the elliptical grid mapping. In world space x = dx, y = height,
// z = -dz, so +sy points away from the default camera ("north").

import type { Layout } from "./layout";

/** Tallest peak, world units (the disc has radius 1). */
export const MAX_HEIGHT = 0.62;
/** Height of one contour band. Every fifth band is a major line. */
export const CONTOUR_STEP = MAX_HEIGHT / 30;

export interface Heightmap {
  /** Grid size: n × n samples spanning the square, sample (i, j) at (i·cell − 1, j·cell − 1). */
  n: number;
  /** Heights, world units. Negative = below ground (cloud-only valleys). */
  h: Float32Array;
  /** Owning layout node per sample (deepest patch containing it), -1 if none. */
  owner: Int32Array;
  /** 1 where the sample lies in a cloud-only patch (drawn as water), else 0. */
  cloud: Float32Array;
  min: number;
  max: number;
}

/** Square [-1,1]² → unit disc (elliptical grid mapping). */
export function toDisc(sx: number, sy: number): [number, number] {
  return [sx * Math.sqrt(1 - (sy * sy) / 2), sy * Math.sqrt(1 - (sx * sx) / 2)];
}

/** Unit disc → square [-1,1]² (inverse of `toDisc`). Points outside the disc are clamped to it. */
export function fromDisc(dx: number, dz: number): [number, number] {
  const r2 = dx * dx + dz * dz;
  if (r2 > 1) { const r = Math.sqrt(r2); dx /= r; dz /= r; }
  const u2 = dx * dx, v2 = dz * dz, t = 2 * Math.SQRT2;
  const sx = 0.5 * Math.sqrt(Math.max(0, 2 + u2 - v2 + t * dx)) - 0.5 * Math.sqrt(Math.max(0, 2 + u2 - v2 - t * dx));
  const sy = 0.5 * Math.sqrt(Math.max(0, 2 - u2 + v2 + t * dz)) - 0.5 * Math.sqrt(Math.max(0, 2 - u2 + v2 - t * dz));
  return [Math.max(-1, Math.min(1, sx)), Math.max(-1, Math.min(1, sy))];
}

const ease = (x: number) => { x = Math.min(1, Math.max(0, x)); return x * x * (3 - 2 * x); };

/** Separable box blur in place, radius in samples (running sums: O(n²) per pass). */
export function boxBlur(a: Float32Array, n: number, rad: number): void {
  if (rad < 1) return;
  const tmp = new Float32Array(n);
  for (let pass = 0; pass < 2; pass++) {
    for (let line = 0; line < n; line++) {
      const at = (k: number) => (pass === 0 ? line * n + k : k * n + line);
      let sum = 0, cnt = 0;
      for (let k = 0; k <= Math.min(rad, n - 1); k++) { sum += a[at(k)]; cnt++; }
      for (let k = 0; k < n; k++) {
        tmp[k] = sum / cnt;
        const add = k + rad + 1, drop = k - rad;
        if (add < n) { sum += a[at(add)]; cnt++; }
        if (drop >= 0) { sum -= a[at(drop)]; cnt--; }
      }
      for (let k = 0; k < n; k++) a[at(k)] = tmp[k];
    }
  }
}

export interface HeightmapOptions {
  /** Carve cloud-only patches as valleys below ground. */
  cloudValleys: boolean;
}

/** A patch counts as cloud-only when most of its bytes live only in the cloud. */
export const isValley = (weight: number, cloudOnly: number) => weight > 0 && cloudOnly / weight >= 0.5;

export function buildHeightmap(layout: Layout, n: number, opts: HeightmapOptions): Heightmap {
  const N = Math.max(8, Math.floor(n));
  const H = new Float32Array(N * N);
  const owner = new Int32Array(N * N).fill(-1);
  const cloud = new Float32Array(N * N);
  const cell = 2 / (N - 1);
  const range = (a: number, b: number): [number, number] =>
    [Math.max(0, Math.ceil((a + 1) / cell)), Math.min(N - 1, Math.floor((b + 1) / cell))];

  const mask = new Float32Array(N * N), floor = new Float32Array(N * N);
  let anyValley = false;
  const RIM = 0.15;
  layout.nodes.forEach((r, k) => {
    const w = r.x1 - r.x0, h = r.y1 - r.y0;
    if (w <= 0 || h <= 0) return;
    const [i0, i1] = range(r.x0, r.x1), [j0, j1] = range(r.y0, r.y1);
    const valley = opts.cloudValleys && r.leaf && isValley(r.weight, r.cloudOnly);
    // Nodes are stored parents-first, so deeper patches overwrite their ancestors.
    for (let j = j0; j <= j1; j++) for (let i = i0; i <= i1; i++) {
      owner[j * N + i] = k;
      cloud[j * N + i] = valley ? 1 : 0;
    }
    if (r.depth === 0) return;
    if (valley) {
      // Valley floor: depth grows with the patch (how much lives only in the cloud).
      anyValley = true;
      const amp = Math.sqrt(w * h) * 2.2 * Math.min(1, r.cloudOnly / r.weight);
      for (let j = j0; j <= j1; j++) {
        const ti = ease(((j * cell - 1 - r.y0) / h - RIM) / (1 - 2 * RIM));
        for (let i = i0; i <= i1; i++) {
          const si = ease(((i * cell - 1 - r.x0) / w - RIM) / (1 - 2 * RIM));
          mask[j * N + i] = 1;
          floor[j * N + i] = amp * Math.pow(Math.sin(Math.PI * ti) * Math.sin(Math.PI * si), 0.9);
        }
      }
      return;
    }
    // Cushion: every patch adds a rounded hill, scaled by its on-disk share.
    const onDisk = r.weight > 0 ? Math.min(1, r.allocated / r.weight) : 0;
    const amp = Math.sqrt(w * h) * onDisk * (r.leaf ? 0.8 : 1);
    if (amp <= 0) return;
    for (let j = j0; j <= j1; j++) {
      const t = (j * cell - 1 - r.y0) / h, by = Math.pow(Math.max(0, 4 * t * (1 - t)), 0.65);
      if (by <= 0) continue;
      for (let i = i0; i <= i1; i++) {
        const s = (i * cell - 1 - r.x0) / w, bx = Math.pow(Math.max(0, 4 * s * (1 - s)), 0.65);
        H[j * N + i] += amp * bx * by;
      }
    }
  });

  // Valleys: a blurred mask lowers the surrounding land to ground level, so the slopes read
  // like hillsides, then the floor sinks below ground.
  if (anyValley) {
    const mb = mask.slice();
    const rad = Math.max(1, Math.round(N / 28));
    boxBlur(mb, N, rad); boxBlur(mb, N, rad);
    for (let k = 0; k < H.length; k++) {
      const lower = Math.max(mask[k], ease(mb[k] * 2.2));
      H[k] = H[k] * (1 - lower) - floor[k];
    }
  }

  // Soften creases between siblings so it reads as terrain, not boxes.
  for (let pass = 0; pass < 3; pass++) {
    const C = H.slice();
    for (let j = 1; j < N - 1; j++) for (let i = 1; i < N - 1; i++) {
      const k = j * N + i;
      H[k] = (C[k] * 4 + C[k - 1] + C[k + 1] + C[k - N] + C[k + N]) / 8;
    }
  }

  // Normalize with a sign-preserving pow(0.7), so small hills stay visible next to big peaks.
  let hmax = 0, hneg = 0;
  for (let k = 0; k < H.length; k++) { if (H[k] > hmax) hmax = H[k]; if (-H[k] > hneg) hneg = -H[k]; }
  const norm = hmax > 0 ? hmax : hneg;
  if (norm > 0) {
    for (let k = 0; k < H.length; k++) {
      const v = Math.pow(Math.abs(H[k]) / norm, 0.7) * MAX_HEIGHT;
      H[k] = H[k] < 0 ? -Math.min(v, MAX_HEIGHT * 1.2) : v;
    }
  }

  // Foothills: ease the rim to ground level and lift the middle a little. The same factor
  // applies everywhere at a given radius, so neighbours still compare honestly.
  let min = 0, max = 0;
  for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
    const sx = i * cell - 1, sy = j * cell - 1;
    const e = ease(Math.min(1, Math.min(1 - Math.abs(sx), 1 - Math.abs(sy)) / 0.3));
    const [dx, dz] = toDisc(sx, sy);
    const k = j * N + i;
    H[k] *= e * (0.6 + 0.4 * (1 - dx * dx - dz * dz));
    if (H[k] < min) min = H[k];
    if (H[k] > max) max = H[k];
  }
  return { n: N, h: H, owner, cloud, min, max };
}

/** Bilinear height at square point (sx, sy). */
export function sampleHeight(hm: Heightmap, sx: number, sy: number): number {
  const N = hm.n, cell = 2 / (N - 1), H = hm.h;
  const fx = Math.min(N - 1.0001, Math.max(0, (sx + 1) / cell));
  const fy = Math.min(N - 1.0001, Math.max(0, (sy + 1) / cell));
  const i = Math.floor(fx), j = Math.floor(fy), tx = fx - i, ty = fy - j, k = j * N + i;
  return (H[k] * (1 - tx) + H[k + 1] * tx) * (1 - ty) + (H[k + N] * (1 - tx) + H[k + N + 1] * tx) * ty;
}

/** Nearest-sample index for square point (sx, sy). */
export function sampleIndex(hm: Heightmap, sx: number, sy: number): number {
  const N = hm.n, cell = 2 / (N - 1);
  const i = Math.min(N - 1, Math.max(0, Math.round((sx + 1) / cell)));
  const j = Math.min(N - 1, Math.max(0, Math.round((sy + 1) / cell)));
  return j * N + i;
}

/** Height at a world (x, z) point on the disc, or null outside it. */
export function heightAtWorld(hm: Heightmap, x: number, z: number): number | null {
  if (x * x + z * z > 1) return null;
  const [sx, sy] = fromDisc(x, -z);
  return sampleHeight(hm, sx, sy);
}

export type Vec3 = [number, number, number];

/**
 * Ray against the heightfield (world space). Marches the part of the ray inside the terrain's
 * bounding cylinder and refines the first crossing by bisection. Works from above and below.
 * Returns the square coordinates of the hit, or null.
 */
export function raycastHeightfield(hm: Heightmap, origin: Vec3, dir: Vec3, steps = 400): { sx: number; sy: number; point: Vec3 } | null {
  const [ox, oy, oz] = origin, [vx, vy, vz] = dir;
  const lo = hm.min - 0.01, hi = hm.max + 0.01;
  // Ray parameter range inside the slab lo <= y <= hi.
  let t0 = 0, t1 = 1e4;
  if (Math.abs(vy) < 1e-12) { if (oy < lo || oy > hi) return null; }
  else {
    const a = (lo - oy) / vy, b = (hi - oy) / vy;
    t0 = Math.max(t0, Math.min(a, b)); t1 = Math.min(t1, Math.max(a, b));
  }
  // ...and inside the cylinder x² + z² <= 1.
  const A = vx * vx + vz * vz, B = 2 * (ox * vx + oz * vz), C = ox * ox + oz * oz - 1;
  if (A < 1e-12) { if (C > 0) return null; }
  else {
    const disc = B * B - 4 * A * C;
    if (disc < 0) return null;
    const s = Math.sqrt(disc);
    t0 = Math.max(t0, (-B - s) / (2 * A)); t1 = Math.min(t1, (-B + s) / (2 * A));
  }
  if (t1 <= t0) return null;

  const f = (t: number) => {
    const x = ox + vx * t, z = oz + vz * t;
    const hgt = heightAtWorld(hm, x, z);
    return hgt === null ? null : oy + vy * t - hgt;
  };
  let pt = t0, pf = f(t0);
  for (let s = 1; s <= steps; s++) {
    const t = t0 + ((t1 - t0) * s) / steps;
    const v = f(t);
    if (v !== null && pf !== null && (v === 0 || Math.sign(v) !== Math.sign(pf))) {
      let a = pt, b = t, fa = pf;
      for (let it = 0; it < 24; it++) {
        const m = (a + b) / 2, fm = f(m) ?? 0;
        if (Math.sign(fm) === Math.sign(fa)) { a = m; fa = fm; } else b = m;
      }
      const tm = (a + b) / 2;
      const p: Vec3 = [ox + vx * tm, oy + vy * tm, oz + vz * tm];
      const [sx, sy] = fromDisc(p[0], -p[2]);
      return { sx, sy, point: p };
    }
    pt = t; pf = v;
  }
  return null;
}
