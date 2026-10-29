import { describe, expect, it } from "vitest";
import { AutoSpin, HOME, UNDER, VIEW_MS, homeOnBearing, isHomeView, viewProgress } from "./rotation";

const FULL = { allowed: true, fullView: true, held: false };

/** Runs the spinner at 60 fps from `from` to `to` ms, returning the speed of every frame. */
function run(s: AutoSpin, from: number, to: number, state = FULL): number[] {
  const out: number[] = [];
  for (let t = from; t <= to; t += 1000 / 60) out.push(s.update(t, 1000 / 60, state));
  return out;
}

describe("AutoSpin", () => {
  it("opens turning and never stops on its own", () => {
    const s = new AutoSpin();
    const v = run(s, 0, 120_000);
    expect(v[0]).toBe(1);
    expect(Math.min(...v)).toBe(1);
  });

  it("pauses on interaction, then eases back in after the idle time", () => {
    const s = new AutoSpin();
    run(s, 0, 1000);
    s.poke(1000);
    // Still idle-waiting: no rotation.
    expect(Math.max(...run(s, 1001, 1000 + s.idle - 20))).toBe(0);
    // Ramps up smoothly: starts near zero, never decreases, reaches full speed.
    const ramp = run(s, 1000 + s.idle, 1000 + s.idle + s.ramp + 200);
    expect(ramp[0]).toBeLessThan(0.01);
    for (let i = 1; i < ramp.length; i++) {
      expect(ramp[i]).toBeGreaterThanOrEqual(ramp[i - 1]);
      expect(ramp[i] - ramp[i - 1]).toBeLessThan(0.03); // no jumps
    }
    expect(ramp[ramp.length - 1]).toBe(1);
  });

  it("keeps waiting while the user keeps interacting", () => {
    const s = new AutoSpin();
    for (let t = 0; t < 10_000; t += 500) { s.poke(t); expect(s.update(t, 16, FULL)).toBe(0); }
  });

  it("does not resume away from the full view (zoomed in, flown to a node)", () => {
    const s = new AutoSpin();
    s.poke(0);
    expect(Math.max(...run(s, 0, 20_000, { ...FULL, fullView: false }))).toBe(0);
    // Back at full view: resumes after the idle time measured from the last interaction.
    expect(run(s, 20_000, 23_000).at(-1)).toBe(1);
  });

  it("does not resume while a selection holds the view", () => {
    const s = new AutoSpin();
    expect(Math.max(...run(s, 0, 20_000, { ...FULL, held: true }))).toBe(0);
  });

  it("stays off when not allowed (reduced motion, looking under, option off)", () => {
    expect(Math.max(...run(new AutoSpin(), 0, 20_000, { ...FULL, allowed: false }))).toBe(0);
  });
});

describe("full view", () => {
  const T = HOME.target;
  it("recognises the default framing from any bearing", () => {
    expect(isHomeView(HOME.pos, T)).toBe(true);
    for (const az of [0.5, 1.7, 3, -2.2]) {
      const p = homeOnBearing({ x: Math.sin(az), y: 0, z: Math.cos(az) }, T);
      expect(isHomeView(p, T)).toBe(true);
      expect(Math.atan2(p.x - T.x, p.z - T.z)).toBeCloseTo(az, 6); // keeps the bearing
    }
  });

  it("counts orbiting (tilt too) at home distance and target as full view", () => {
    const d = Math.hypot(HOME.pos.y - T.y, HOME.pos.z - T.z);
    expect(isHomeView({ x: 0, y: T.y + d * 0.999, z: d * 0.04 }, T)).toBe(true); // nearly top-down
  });

  it("uses the 6B thresholds: 2% of the radius for the target, 8% for the distance", () => {
    const at = (k: number) => ({ x: HOME.pos.x, y: T.y + (HOME.pos.y - T.y) * k, z: T.z + (HOME.pos.z - T.z) * k });
    expect(isHomeView(at(1.07), T)).toBe(true);
    expect(isHomeView(at(0.93), T)).toBe(true);
    expect(isHomeView(at(1.09), T)).toBe(false);
    expect(isHomeView(at(0.91), T)).toBe(false);
    expect(isHomeView(HOME.pos, { x: 0.015, y: T.y, z: 0 })).toBe(true);
    expect(isHomeView(HOME.pos, { x: 0.025, y: T.y, z: 0 })).toBe(false);
  });

  it("measures Look under against its own framing", () => {
    expect(isHomeView(UNDER.pos, T)).toBe(false);
    expect(isHomeView(UNDER.pos, T, true)).toBe(true);
    expect(isHomeView(HOME.pos, T, true)).toBe(false);
  });
});

describe("viewProgress", () => {
  it("eases out over 600ms and clamps", () => {
    expect(VIEW_MS).toBe(600);
    expect(viewProgress(0, -5)).toBe(0);
    expect(viewProgress(0, 0)).toBe(0);
    expect(viewProgress(0, 300)).toBeGreaterThan(0.5); // ease-out: past halfway at half time
    expect(viewProgress(0, 600)).toBe(1);
    expect(viewProgress(0, 900)).toBe(1);
    expect(viewProgress(0, 0, 0)).toBe(1); // reduced motion: instant
  });
});
