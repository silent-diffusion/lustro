// Low frame-rate detection for the automatic switch to the flat 2D map.

export interface FpsWatchOptions {
  /** Frames per second below which rendering counts as slow. */
  threshold: number;
  /** How long (ms of continuous rendering) the rate must stay low. */
  window: number;
  /** Frame gaps longer than this (ms) are not measured: a throttled background or occluded
   *  window (about 1 fps) or a debugger pause is not a slow GPU. */
  maxGap: number;
}

/**
 * Feed it the timestamp of every frame that was rendered. Idle frames (nothing to draw) break
 * the run, so only continuous rendering is measured: a static scene never looks "slow".
 */
export class FpsWatch {
  private o: FpsWatchOptions;
  private last = -1;
  private dts: number[] = [];
  private sum = 0;

  constructor(opts: Partial<FpsWatchOptions> = {}) {
    this.o = { threshold: 30, window: 3000, maxGap: 500, ...opts };
  }

  reset(): void { this.last = -1; this.dts = []; this.sum = 0; }

  /** Current average rate over the measured window, or null before there is enough data. */
  get fps(): number | null { return this.dts.length ? (this.dts.length * 1000) / this.sum : null; }

  /**
   * @param now frame timestamp, ms
   * @param continuous whether the previous animation frame was rendered too
   * @returns true when the rate stayed under the threshold for the whole window
   */
  frame(now: number, continuous: boolean): boolean {
    const dt = now - this.last;
    const prev = this.last;
    this.last = now;
    if (!continuous || prev < 0 || dt <= 0 || dt > this.o.maxGap) return false;
    this.dts.push(dt);
    this.sum += dt;
    while (this.dts.length > 1 && this.sum - this.dts[0] >= this.o.window) this.sum -= this.dts.shift()!;
    if (this.sum < this.o.window) return false;
    return (this.dts.length * 1000) / this.sum < this.o.threshold;
  }
}
