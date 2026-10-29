// Auto-rotation ("looking around") and the "full view" test. Pure, so it can be unit-tested.
//
// Rules: rotation never stops on its own. Interaction pauses it at once; after IDLE ms without
// interaction it ramps back up smoothly (speed only, the camera never jumps), but only while the
// camera frames the whole map and nothing the user picked is holding the view.

type V3 = { x: number; y: number; z: number };

/** Default camera framing ("full view") and the Look-under framing, world units. */
export const HOME = { pos: { x: 0, y: 2.55, z: 3.55 }, target: { x: 0, y: 0.12, z: 0 } };
export const UNDER = { pos: { x: 0.6, y: -1.6, z: 3.4 }, target: HOME.target };

/** Duration of view changes (fly-to, full view, look under), ms. */
export const VIEW_MS = 600;

/** Ease-out progress 0..1 of a view change that started at t0 (cubic: fast start, gentle landing). */
export function viewProgress(t0: number, now: number, ms = VIEW_MS): number {
  if (ms <= 0) return 1;
  const t = Math.min(1, Math.max(0, (now - t0) / ms));
  return 1 - Math.pow(1 - t, 3);
}

const len = (x: number, y: number, z: number) => Math.hypot(x, y, z);
const dist = (a: V3, b: V3) => len(a.x - b.x, a.y - b.y, a.z - b.z);
const HOME_DIST = dist(HOME.pos, HOME.target);
const UNDER_DIST = dist(UNDER.pos, UNDER.target);
const HOME_POLAR = Math.acos((HOME.pos.y - HOME.target.y) / HOME_DIST);

/**
 * Whether the camera frames the whole map: the target within 2% of the
 * disc radius (1) of home, and the zoom distance within 8% of the home distance. Orbiting alone
 * (any bearing or tilt) still counts: rotating isn't zooming. `under` measures against the
 * Look-under framing instead.
 */
export function isHomeView(pos: V3, target: V3, under = false, tol = { target: 0.02, dist: 0.08 }): boolean {
  if (dist(target, HOME.target) > tol.target) return false;
  const ref = under ? UNDER_DIST : HOME_DIST;
  return Math.abs(dist(pos, target) - ref) / ref <= tol.dist;
}

/** Home camera position on the bearing the camera is on now (so going home never swings round). */
export function homeOnBearing(pos: V3, target: V3): V3 {
  const az = Math.atan2(pos.x - target.x, pos.z - target.z);
  const h = Math.sin(HOME_POLAR) * HOME_DIST;
  return { x: HOME.target.x + Math.sin(az) * h, y: HOME.target.y + Math.cos(HOME_POLAR) * HOME_DIST, z: HOME.target.z + Math.cos(az) * h };
}

export interface SpinState {
  /** Auto-rotation wanted at all (option on, motion allowed, 3D, not looking under). */
  allowed: boolean;
  /** Camera frames the whole map and isn't mid-flight. */
  fullView: boolean;
  /** Something the user picked (a selection, a fly-to) holds the view until they go back to full view. */
  held: boolean;
}

export class AutoSpin {
  /** Idle time after the last interaction before rotation resumes, ms. */
  idle = 3500;
  /** Time to ramp from still to full speed, ms. */
  ramp = 1800;
  private lastActive = -Infinity;
  /** 0..1 progress of the ramp; starts at 1 so the view opens already turning. */
  private level = 1;

  /** The user did something (drag, wheel, key): pause now and restart the idle timer. */
  poke(now: number): void {
    this.lastActive = now;
    this.level = 0;
  }

  /** Speed factor 0..1 for this frame (eased, so it starts and settles gently). */
  update(now: number, dt: number, s: SpinState): number {
    const want = s.allowed && s.fullView && !s.held && now - this.lastActive >= this.idle;
    if (!want) { this.level = 0; return 0; }
    this.level = Math.min(1, this.level + Math.max(0, dt) / this.ramp);
    const t = this.level;
    return t * t * (3 - 2 * t);
  }
}
