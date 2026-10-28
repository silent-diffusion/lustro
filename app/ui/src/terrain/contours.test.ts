import { describe, expect, it } from "vitest";
import { contourLevels, marchingSquares } from "./contours";
import { FpsWatch } from "./fps";

describe("contourLevels", () => {
  it("skips ground level and marks every fifth band major", () => {
    const lv = contourLevels(-0.25, 1.05, 0.1);
    expect(lv.map((l) => Math.round(l.level * 10))).toEqual([-2, -1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(lv.filter((l) => l.major).map((l) => Math.round(l.level * 10))).toEqual([5, 10]);
    expect(lv.filter((l) => l.below).length).toBe(2);
  });
  it("is empty for a flat map or bad step", () => {
    expect(contourLevels(0, 0, 0.1)).toEqual([]);
    expect(contourLevels(0, 1, 0)).toEqual([]);
  });
});

describe("marchingSquares", () => {
  const cone = (n: number) => {
    const h = new Float32Array(n * n), cell = 2 / (n - 1);
    for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) {
      const x = i * cell - 1, y = j * cell - 1;
      h[j * n + i] = 1 - Math.hypot(x, y);
    }
    return h;
  };

  it("traces a circle on a cone at the right radius", () => {
    const n = 65, segs = marchingSquares(cone(n), n, 0.5);
    expect(segs.length).toBeGreaterThan(0);
    expect(segs.length % 4).toBe(0);
    for (let q = 0; q < segs.length; q += 2) expect(Math.hypot(segs[q], segs[q + 1])).toBeCloseTo(0.5, 2);
    // Closed loop: total length ≈ circumference.
    let len = 0;
    for (let q = 0; q < segs.length; q += 4) len += Math.hypot(segs[q + 2] - segs[q], segs[q + 3] - segs[q + 1]);
    expect(len).toBeCloseTo(Math.PI, 1);
  });

  it("returns nothing when the level is outside the data", () => {
    const n = 17, h = cone(n);
    expect(marchingSquares(h, n, 2).length).toBe(0);
    expect(marchingSquares(h, n, -5).length).toBe(0);
  });

  it("resolves saddles with two segments", () => {
    // a=1 (above) b=0 c=1 d=0: diagonal saddle.
    const h = new Float32Array([1, 0, 0, 1]);
    const segs = marchingSquares(h, 2, 0.5);
    expect(segs.length).toBe(8);
  });
});

describe("FpsWatch", () => {
  const run = (w: FpsWatch, fps: number, ms: number, start = 0) => {
    let fired = false;
    for (let t = start; t <= start + ms; t += 1000 / fps) fired = w.frame(t, true) || fired;
    return fired;
  };

  it("fires after 3 s of continuous rendering under 30 fps", () => {
    const w = new FpsWatch();
    expect(run(w, 20, 2500)).toBe(false);
    expect(run(new FpsWatch(), 20, 3200)).toBe(true);
  });

  it("does not fire at 60 fps or on short hitches", () => {
    expect(run(new FpsWatch(), 60, 10_000)).toBe(false);
    const w = new FpsWatch();
    run(w, 60, 3000);
    expect(run(w, 15, 500, 3000)).toBe(false);
  });

  it("ignores a throttled background window (about 1 fps)", () => {
    expect(run(new FpsWatch(), 1, 20_000)).toBe(false);
  });

  it("ignores idle gaps between rendered frames", () => {
    const w = new FpsWatch();
    let fired = false;
    // One rendered frame every 100 ms, but each after an idle frame: never measured.
    for (let t = 0; t < 10_000; t += 100) fired = w.frame(t, false) || fired;
    expect(fired).toBe(false);
    expect(w.fps).toBeNull();
  });
});
