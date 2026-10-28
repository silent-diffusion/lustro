// Mesh arrays, route and label placement. Pure functions only so they can be unit-tested.

import type { Heightmap } from "./heightmap";
import { sampleHeight, toDisc } from "./heightmap";
import type { Layout } from "./layout";
import { centroid } from "./layout";
import type { Palette } from "./palette";

export interface TerrainArrays {
  /** World positions (x, y, z) per sample. */
  position: Float32Array;
  /** Square coordinates per sample (for selection and hover highlights). */
  sq: Float32Array;
  /** Category tint (sRGB 0..1) per sample. */
  tint: Float32Array;
  /** Cloud-only flag per sample. */
  cloud: Float32Array;
  index: Uint32Array;
}

export function terrainArrays(hm: Heightmap, layout: Layout, palette: Palette): TerrainArrays {
  const N = hm.n, cell = 2 / (N - 1);
  const position = new Float32Array(N * N * 3), sq = new Float32Array(N * N * 2);
  for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
    const k = j * N + i, sx = i * cell - 1, sy = j * cell - 1;
    const [dx, dz] = toDisc(sx, sy);
    position[k * 3] = dx; position[k * 3 + 1] = hm.h[k]; position[k * 3 + 2] = -dz;
    sq[k * 2] = sx; sq[k * 2 + 1] = sy;
  }
  const index = new Uint32Array((N - 1) * (N - 1) * 6);
  let p = 0;
  for (let j = 0; j < N - 1; j++) for (let i = 0; i < N - 1; i++) {
    const a = j * N + i, b = a + 1, c = a + N, d = c + 1;
    index[p++] = a; index[p++] = c; index[p++] = b;
    index[p++] = b; index[p++] = c; index[p++] = d;
  }
  return { position, sq, tint: tintArray(hm, layout, palette), cloud: hm.cloud.slice(), index };
}

export function tintArray(hm: Heightmap, layout: Layout, palette: Palette): Float32Array {
  const out = new Float32Array(hm.n * hm.n * 3);
  const other = palette.categories.other;
  for (let k = 0; k < hm.owner.length; k++) {
    const o = hm.owner[k];
    const c = o >= 0 ? palette.categories[layout.nodes[o].category] ?? other : other;
    out[k * 3] = c[0]; out[k * 3 + 1] = c[1]; out[k * 3 + 2] = c[2];
  }
  return out;
}

/**
 * Route stops in square coordinates: enters from the rim nearest the first folder on the chain,
 * then visits the centre of every laid-out folder down to the target.
 */
export function routeStops(layout: Layout, chain: number[]): [number, number][] {
  const steps = chain.filter((k) => layout.nodes[k].depth > 0);
  if (!steps.length) return [];
  const [fx, fy] = centroid(layout.nodes[steps[0]]);
  const m = Math.max(Math.abs(fx), Math.abs(fy)) || 1;
  return [[fx / m, fy / m], ...steps.map((k) => centroid(layout.nodes[k]))];
}

/** Densifies route stops into world points draped over the terrain (lifted slightly). */
export function routePoints(hm: Heightmap, stops: [number, number][], perLeg = 48, lift = 0.012): Float32Array {
  if (stops.length < 2) return new Float32Array(0);
  const out: number[] = [];
  for (let s = 0; s < stops.length - 1; s++) {
    const [ax, ay] = stops[s], [bx, by] = stops[s + 1];
    for (let q = s === 0 ? 0 : 1; q <= perLeg; q++) {
      const t = q / perLeg, x = ax + (bx - ax) * t, y = ay + (by - ay) * t;
      const [dx, dz] = toDisc(x, y);
      out.push(dx, Math.max(0, sampleHeight(hm, x, y)) + lift, -dz);
    }
  }
  return new Float32Array(out);
}

export interface RingLabel { index: number; angle: number }

/**
 * Top-level folders labelled around the compass ring at the bearing where they sit, largest
 * first, skipping any that would crowd an existing label. Angle is in the ground plane (radians,
 * world x = cos, world z = sin).
 */
export function ringLabels(layout: Layout, max = 8, minGap = 0.32): RingLabel[] {
  const top = layout.nodes
    .map((n, k) => ({ n, k }))
    .filter(({ n }) => n.depth === 1 && !n.folded)
    .sort((a, b) => b.n.weight - a.n.weight)
    .slice(0, max);
  const out: RingLabel[] = [];
  for (const { n, k } of top) {
    const [cx, cy] = centroid(n);
    const [dx, dz] = toDisc(cx, cy);
    if (dx * dx + dz * dz < 1e-6) continue;
    const angle = Math.atan2(-dz, dx);
    if (out.some((l) => Math.abs(Math.atan2(Math.sin(l.angle - angle), Math.cos(l.angle - angle))) < minGap)) continue;
    out.push({ index: k, angle });
  }
  return out;
}
