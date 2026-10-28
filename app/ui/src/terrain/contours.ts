// Contour lines for the flat 2D map (the 3D view draws them in its fragment shader).
// Pure functions only so they can be unit-tested.

import { CONTOUR_STEP } from "./heightmap";

export interface ContourLevel {
  level: number;
  /** Every fifth band is a major line. */
  major: boolean;
  /** Below ground (valley) contours are drawn in the cloud color. */
  below: boolean;
}

/** Contour levels between min and max, skipping ground level (the shoreline is drawn separately). */
export function contourLevels(min: number, max: number, step = CONTOUR_STEP): ContourLevel[] {
  const out: ContourLevel[] = [];
  if (!(step > 0)) return out;
  const k0 = Math.ceil(min / step), k1 = Math.floor(max / step);
  for (let k = k0; k <= k1; k++) {
    if (k === 0) continue;
    out.push({ level: k * step, major: k % 5 === 0, below: k < 0 });
  }
  return out;
}

/**
 * Marching squares on an n × n grid spanning the square [-1, 1]². Returns line segments in square
 * coordinates, packed as [x0, y0, x1, y1, ...]. A sample counts as "above" when value >= level.
 * Saddles are resolved by the cell's average.
 */
export function marchingSquares(h: ArrayLike<number>, n: number, level: number): Float32Array {
  const cell = 2 / (n - 1);
  const out: number[] = [];
  const lerp = (a: number, b: number) => { const d = b - a; return d === 0 ? 0.5 : (level - a) / d; };
  for (let j = 0; j < n - 1; j++) {
    for (let i = 0; i < n - 1; i++) {
      const k = j * n + i;
      const a = h[k], b = h[k + 1], c = h[k + n + 1], d = h[k + n];
      // Corners: a (i,j) b (i+1,j) c (i+1,j+1) d (i,j+1).
      const code = (a >= level ? 8 : 0) | (b >= level ? 4 : 0) | (c >= level ? 2 : 0) | (d >= level ? 1 : 0);
      if (code === 0 || code === 15) continue;
      const x = i * cell - 1, y = j * cell - 1;
      // Edge midpoints (interpolated): bottom (a-b), right (b-c), top (d-c), left (a-d).
      const bot = () => [x + lerp(a, b) * cell, y];
      const right = () => [x + cell, y + lerp(b, c) * cell];
      const top = () => [x + lerp(d, c) * cell, y + cell];
      const left = () => [x, y + lerp(a, d) * cell];
      const seg = (p: number[], q: number[]) => out.push(p[0], p[1], q[0], q[1]);
      switch (code) {
        case 1: case 14: seg(left(), top()); break;
        case 2: case 13: seg(top(), right()); break;
        case 3: case 12: seg(left(), right()); break;
        case 4: case 11: seg(bot(), right()); break;
        case 6: case 9: seg(bot(), top()); break;
        case 7: case 8: seg(left(), bot()); break;
        case 5: case 10: {
          const centerAbove = (a + b + c + d) / 4 >= level;
          // code 5: b and d above; code 10: a and c above.
          if ((code === 5) === centerAbove) { seg(left(), bot()); seg(top(), right()); }
          else { seg(left(), top()); seg(bot(), right()); }
          break;
        }
      }
    }
  }
  return new Float32Array(out);
}
