// The flat 2D contour map: the same terrain seen from straight above, drawn with Canvas 2D so it
// works without WebGL (accessibility, low-end GPUs, weak WebGL on Linux WebKitGTK).
// The base map (hillshade, tint, contours, ring) is rendered once into an offscreen canvas; pan,
// zoom, selection and the route are cheap redraws on top.

import type { Heightmap } from "./heightmap";
import { fromDisc, sampleHeight, sampleIndex, toDisc } from "./heightmap";
import type { Layout, LayoutNode } from "./layout";
import { contourLevels, marchingSquares } from "./contours";
import type { Palette, RGB } from "./palette";
import { css, shade } from "./palette";

/** Half-width of the drawn world (disc radius 1, base ring to 1.2). */
const R = 1.25;
/** Deeper zoom only magnifies the capped heightmap, so it stops here. */
const MAX_ZOOM = 6;
const L = (() => { const v = [-0.5, 1, 0.35], m = Math.hypot(...v); return v.map((x) => x / m); })();

export interface FlatView { zoom: number; cx: number; cy: number }

export class Flat2D {
  readonly canvas = document.createElement("canvas");
  private ctx: CanvasRenderingContext2D;
  private base: HTMLCanvasElement | null = null;
  private baseStale = true;
  private hm: Heightmap | null = null;
  private layout: Layout | null = null;
  private palette: Palette | null = null;
  private tint = true;
  private sel: LayoutNode | null = null;
  private hover: LayoutNode | null = null;
  private route: [number, number][] = [];
  private w = 1;
  private h = 1;
  private dpr = 1;
  private view: FlatView = { zoom: 1, cx: 0, cy: 0 };
  private goal: FlatView | null = null;
  private dirty = true;
  private drag: { x: number; y: number; moved: boolean } | null = null;
  ease = 0.18;

  constructor(
    container: HTMLElement,
    /** Upper bound for the offscreen base map, px. */
    private maxBase: number,
    private onHover: (idx: number, clientX: number, clientY: number) => void,
    private onPick: (idx: number) => void,
  ) {
    this.canvas.className = "svt-flat";
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", "Flat contour map of the scanned folder. Arrow keys pan, plus and minus zoom, 0 resets.");
    this.ctx = this.canvas.getContext("2d")!;
    container.appendChild(this.canvas);
    this.canvas.addEventListener("pointerdown", this.onDown);
    this.canvas.addEventListener("pointermove", this.onMove);
    this.canvas.addEventListener("pointerup", this.onUp);
    this.canvas.addEventListener("pointerleave", this.onLeave);
    this.canvas.addEventListener("wheel", this.onWheel, { passive: false });
    this.canvas.addEventListener("keydown", this.onKey);
  }

  setTerrain(hm: Heightmap, layout: Layout, palette: Palette, tint: boolean): void {
    this.hm = hm; this.layout = layout; this.palette = palette; this.tint = tint;
    this.baseStale = true; // rebuilt lazily on the next draw
    this.dirty = true;
  }

  setSelected(n: LayoutNode | null): void { this.sel = n; this.dirty = true; }
  setHovered(n: LayoutNode | null): void { this.hover = n; this.dirty = true; }
  setRoute(stops: [number, number][]): void { this.route = stops; this.dirty = true; }
  invalidate(): void { this.dirty = true; }

  resize(w: number, h: number, dpr: number): void {
    this.w = w; this.h = h; this.dpr = dpr;
    this.canvas.width = Math.round(w * dpr); this.canvas.height = Math.round(h * dpr);
    this.dirty = true;
  }

  /** Pixels per world unit at the current zoom. */
  // Leaves room around the disc for the ring labels at r = 1.3.
  private ppu(v = this.view): number { return (Math.min(this.w, this.h) / (2 * 1.55)) * v.zoom; }

  /** Disc point → element pixels. */
  toScreen(dx: number, dz: number): [number, number] {
    const p = this.ppu();
    return [this.w / 2 + (dx - this.view.cx) * p, this.h / 2 - (dz - this.view.cy) * p];
  }

  private toDiscPt(px: number, py: number): [number, number] {
    const p = this.ppu();
    return [this.view.cx + (px - this.w / 2) / p, this.view.cy - (py - this.h / 2) / p];
  }

  /** Layout index under element pixels, or -1. */
  pickAt(px: number, py: number): number {
    if (!this.hm) return -1;
    const [dx, dz] = this.toDiscPt(px, py);
    if (dx * dx + dz * dz > 1) return -1;
    const [sx, sy] = fromDisc(dx, dz);
    return this.hm.owner[sampleIndex(this.hm, sx, sy)];
  }

  /** Zoom in on a patch so it fills about 40% of the view. */
  flyTo(n: LayoutNode): void {
    const pts = [[n.x0, n.y0], [n.x1, n.y0], [n.x0, n.y1], [n.x1, n.y1]].map(([x, y]) => toDisc(x, y));
    const size = Math.max(Math.max(...pts.map((p) => p[0])) - Math.min(...pts.map((p) => p[0])),
      Math.max(...pts.map((p) => p[1])) - Math.min(...pts.map((p) => p[1])), 0.01);
    const [cx, cy] = toDisc((n.x0 + n.x1) / 2, (n.y0 + n.y1) / 2);
    this.goal = this.clampView({ zoom: Math.max(1, Math.min(4, 1.04 / size)), cx, cy });
  }

  home(): void { this.goal = { zoom: 1, cx: 0, cy: 0 }; }

  private clampView(v: FlatView): FlatView {
    const r = Math.hypot(v.cx, v.cy), lim = 1;
    const k = r > lim ? lim / r : 1;
    return { zoom: Math.max(1, Math.min(MAX_ZOOM, v.zoom)), cx: v.cx * k, cy: v.cy * k };
  }

  /** Advances view animation and redraws if needed. Returns whether it drew. */
  update(): boolean {
    if (this.goal) {
      const v = this.view, g = this.goal, e = this.ease;
      v.zoom += (g.zoom - v.zoom) * e; v.cx += (g.cx - v.cx) * e; v.cy += (g.cy - v.cy) * e;
      if (Math.abs(g.zoom - v.zoom) < 0.002 && Math.hypot(g.cx - v.cx, g.cy - v.cy) < 0.0005) { this.view = { ...g }; this.goal = null; }
      this.dirty = true;
    }
    if (!this.dirty || !this.hm || !this.palette) return false;
    this.dirty = false;
    this.draw();
    return true;
  }

  private draw(): void {
    const { ctx, palette: p } = this;
    // The base map is sized to the view (1.5× for some zoom headroom), so lines stay crisp.
    const shown = 2 * R * this.ppu({ zoom: 1, cx: 0, cy: 0 });
    const size = Math.round(Math.max(512, Math.min(this.maxBase, shown * this.dpr * 1.5)) / 64) * 64;
    if (!this.base || this.baseStale || Math.abs(size - this.base.width) / this.base.width > 0.2) {
      this.base = this.renderBase(size, size / shown);
      this.baseStale = false;
    }
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.w, this.h);
    const ppu = this.ppu();
    const [left, top] = this.toScreen(-R, R);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(this.base, left, top, 2 * R * ppu, 2 * R * ppu);
    if (this.hover && this.hover !== this.sel) this.outline(this.hover, css(p!.line, 0.07), css(p!.line, 0.5), 1);
    if (this.sel) this.outline(this.sel, css(p!.accent, 0.12), css(p!.accent, 1), 2);
    if (this.route.length >= 2) {
      ctx.save();
      ctx.setLineDash([5, 4]);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = css(p!.text, 0.95);
      ctx.beginPath();
      this.route.forEach(([sx, sy], k) => {
        const [x, y] = this.toScreen(...toDisc(sx, sy));
        if (k === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      });
      ctx.stroke();
      ctx.restore();
    }
  }

  /** Outline a patch, warped onto the disc like the ground. */
  private outline(n: LayoutNode, fill: string, stroke: string, width: number): void {
    const ctx = this.ctx, steps = 16;
    ctx.beginPath();
    const edge = (ax: number, ay: number, bx: number, by: number, first: boolean) => {
      for (let s = first ? 0 : 1; s <= steps; s++) {
        const t = s / steps, [x, y] = this.toScreen(...toDisc(ax + (bx - ax) * t, ay + (by - ay) * t));
        if (first && s === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
    };
    edge(n.x0, n.y0, n.x1, n.y0, true); edge(n.x1, n.y0, n.x1, n.y1, false);
    edge(n.x1, n.y1, n.x0, n.y1, false); edge(n.x0, n.y1, n.x0, n.y0, false);
    ctx.closePath();
    ctx.fillStyle = fill; ctx.fill();
    ctx.lineWidth = width; ctx.strokeStyle = stroke; ctx.stroke();
  }

  /** Hillshade + tint + water per pixel, then contour lines, shoreline and the compass ring. */
  /** @param cssPx base pixels per CSS pixel at zoom 1 (line widths are set in CSS pixels). */
  private renderBase(S: number, cssPx: number): HTMLCanvasElement {
    const hm = this.hm!, layout = this.layout!, p = this.palette!;
    const N = hm.n, cell = 2 / (N - 1), H = hm.h;
    const c = document.createElement("canvas");
    c.width = S; c.height = S;
    const g = c.getContext("2d")!;
    const img = g.createImageData(S, S), px = img.data;
    const toPx = (dx: number, dz: number): [number, number] => [((dx + R) / (2 * R)) * S, ((R - dz) / (2 * R)) * S];
    const tints: RGB[] = layout.nodes.map((n) => p.categories[n.category] ?? p.categories.other);
    for (let y = 0; y < S; y++) {
      const dz = R - ((y + 0.5) / S) * 2 * R;
      for (let x = 0; x < S; x++) {
        const dx = -R + ((x + 0.5) / S) * 2 * R;
        if (dx * dx + dz * dz > 1) continue;
        const [sx, sy] = fromDisc(dx, dz);
        const k = sampleIndex(hm, sx, sy);
        // Bilinear gradient, so the hillshade stays smooth when zoomed in.
        const gx = (sampleHeight(hm, sx + cell, sy) - sampleHeight(hm, sx - cell, sy)) / (2 * cell);
        const gy = (sampleHeight(hm, sx, sy + cell) - sampleHeight(hm, sx, sy - cell)) / (2 * cell);
        const m = Math.hypot(gx, 1, gy);
        const light = Math.max(0, Math.min(1, (-gx * L[0] + L[1] + gy * L[2]) / m));
        const o = hm.owner[k];
        let col = shade(p, light, o >= 0 ? tints[o] : p.categories.other, this.tint);
        const hh = sampleHeight(hm, sx, sy);
        if (hh <= 0 && hm.cloud[k] > 0) {
          const a = 0.22 + Math.min(1, -hh / 0.2) * 0.5;
          col = [0, 1, 2].map((q) => col[q] * (1 - a) + p.cloud[q] * a) as RGB;
        }
        const o4 = (y * S + x) * 4;
        px[o4] = col[0] * 255; px[o4 + 1] = col[1] * 255; px[o4 + 2] = col[2] * 255; px[o4 + 3] = 255;
      }
    }
    g.putImageData(img, 0, 0);

    // Base ring, accent ring and compass ticks (same geometry as the 3D view).
    const ctr = S / 2, u = S / (2 * R);
    g.fillStyle = css(p.fill);
    g.beginPath(); g.arc(ctr, ctr, 1.2 * u, 0, Math.PI * 2); g.arc(ctr, ctr, 0.998 * u, 0, Math.PI * 2, true); g.fill();
    g.strokeStyle = css(p.accent); g.lineWidth = 0.013 * u;
    g.beginPath(); g.arc(ctr, ctr, 1.0815 * u, 0, Math.PI * 2); g.stroke();
    g.strokeStyle = css(p.lineMinor); g.lineWidth = cssPx;
    g.beginPath();
    for (let a = 0; a < 360; a += 2) {
      const r0 = 1.1, r1 = a % 30 === 0 ? 1.17 : a % 10 === 0 ? 1.14 : 1.125, t = (a * Math.PI) / 180;
      g.moveTo(ctr + Math.cos(t) * r0 * u, ctr + Math.sin(t) * r0 * u);
      g.lineTo(ctr + Math.cos(t) * r1 * u, ctr + Math.sin(t) * r1 * u);
    }
    g.stroke();

    // Contours: minor, then major on top; valley contours in the cloud color.
    const lw = cssPx;
    const strokeSegs = (segs: Float32Array, style: string, width: number) => {
      if (!segs.length) return;
      g.strokeStyle = style; g.lineWidth = width;
      g.beginPath();
      for (let q = 0; q < segs.length; q += 4) {
        const [ax, ay] = toPx(...toDisc(segs[q], segs[q + 1]));
        const [bx, by] = toPx(...toDisc(segs[q + 2], segs[q + 3]));
        g.moveTo(ax, ay); g.lineTo(bx, by);
      }
      g.stroke();
    };
    const levels = contourLevels(hm.min, hm.max);
    for (const pass of [false, true]) {
      for (const lv of levels) {
        if (lv.major !== pass) continue;
        const segs = marchingSquares(H, N, lv.level);
        const color = lv.below ? css(p.cloud, lv.major ? 0.8 : 0.5) : lv.major ? css(p.line, 0.9) : css(p.lineMinor, 0.85);
        strokeSegs(segs, color, lv.major ? lw * 1.6 : lw);
      }
    }
    // Shoreline: where the land meets a valley, at ground level.
    if (hm.min < 0) strokeSegs(marchingSquares(H, N, -1e-4), css(p.cloud), lw * 1.8);
    return c;
  }

  // ---------- Interaction ----------
  private local(ev: PointerEvent | WheelEvent): [number, number] {
    const r = this.canvas.getBoundingClientRect();
    return [ev.clientX - r.left, ev.clientY - r.top];
  }
  private onDown = (ev: PointerEvent) => {
    const [x, y] = this.local(ev);
    this.drag = { x, y, moved: false };
    this.canvas.setPointerCapture?.(ev.pointerId);
  };
  private onMove = (ev: PointerEvent) => {
    const [x, y] = this.local(ev);
    if (this.drag && ev.buttons) {
      const dxp = x - this.drag.x, dyp = y - this.drag.y;
      if (!this.drag.moved && Math.hypot(dxp, dyp) < 4) return;
      this.drag.moved = true;
      const p = this.ppu();
      this.goal = null;
      this.view = this.clampView({ zoom: this.view.zoom, cx: this.view.cx - dxp / p, cy: this.view.cy + dyp / p });
      this.drag.x = x; this.drag.y = y;
      this.dirty = true;
      this.onHover(-1, ev.clientX, ev.clientY);
      return;
    }
    this.onHover(this.pickAt(x, y), ev.clientX, ev.clientY);
  };
  private onUp = (ev: PointerEvent) => {
    const d = this.drag;
    this.drag = null;
    if (d && !d.moved) {
      const [x, y] = this.local(ev);
      const k = this.pickAt(x, y);
      if (k >= 0) this.onPick(k);
    }
  };
  private onLeave = (ev: PointerEvent) => { if (!this.drag) this.onHover(-1, ev.clientX, ev.clientY); };
  private onWheel = (ev: WheelEvent) => {
    ev.preventDefault();
    const [x, y] = this.local(ev);
    const before = this.toDiscPt(x, y);
    const zoom = Math.max(1, Math.min(MAX_ZOOM, this.view.zoom * Math.exp(-ev.deltaY * 0.0015)));
    this.goal = null;
    this.view.zoom = zoom;
    // Keep the point under the cursor fixed.
    const after = this.toDiscPt(x, y);
    this.view = this.clampView({ zoom, cx: this.view.cx + before[0] - after[0], cy: this.view.cy + before[1] - after[1] });
    this.dirty = true;
  };
  private onKey = (ev: KeyboardEvent) => {
    const v = this.goal ?? this.view, step = 0.25 / v.zoom;
    const next = { ...v };
    switch (ev.key) {
      case "ArrowLeft": next.cx -= step; break;
      case "ArrowRight": next.cx += step; break;
      case "ArrowUp": next.cy += step; break;
      case "ArrowDown": next.cy -= step; break;
      case "+": case "=": next.zoom *= 1.25; break;
      case "-": case "_": next.zoom /= 1.25; break;
      case "0": this.home(); ev.preventDefault(); return;
      default: return;
    }
    ev.preventDefault();
    this.goal = this.clampView(next);
  };

  dispose(): void {
    this.canvas.removeEventListener("pointerdown", this.onDown);
    this.canvas.removeEventListener("pointermove", this.onMove);
    this.canvas.removeEventListener("pointerup", this.onUp);
    this.canvas.removeEventListener("pointerleave", this.onLeave);
    this.canvas.removeEventListener("wheel", this.onWheel);
    this.canvas.removeEventListener("keydown", this.onKey);
    this.base = null;
    this.canvas.width = this.canvas.height = 0;
    this.canvas.remove();
  }
}
