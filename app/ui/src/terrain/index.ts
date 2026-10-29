// "Terrain of your computer": the folder tree as a 3D contour map, with a flat 2D fallback.
// Implements the contract in ./types.ts. Pipeline: squarified layout → cushion heightmap →
// three.js mesh (3D) or Canvas 2D contour map (flat). See design/README.md for the meaning of
// every feature.

import "./terrain.css";
import type { NodeId, TreeDto } from "../ipc/types";
import type { MountTerrain, TerrainEvents, TerrainHandle, TerrainOptions, TerrainPin } from "./types";
import type { Layout, LayoutNode } from "./layout";
import { chainOf, centroid, layoutTree, locate } from "./layout";
import type { Heightmap } from "./heightmap";
import { buildHeightmap, sampleHeight, sampleIndex, toDisc } from "./heightmap";
import { ringLabels, routePoints, routeStops } from "./geometry";
import type { RingLabel } from "./geometry";
import type { Palette } from "./palette";
import { css, paletteFor } from "./palette";
import { FpsWatch } from "./fps";
import { Flat2D } from "./flat2d";
import { Overlay, PIN_LABEL, formatBytes } from "./overlay";
import { Scene3D } from "./scene3d";
import { AutoSpin, VIEW_MS } from "./rotation";

export type { MountTerrain, TerrainEvents, TerrainHandle, TerrainOptions, TerrainPin } from "./types";

/** Level of detail. The mesh is capped at RES² samples whatever the tree size. */
const RES = { full: 256, compact: 160 };
const MAX_NODES = { full: 8000, compact: 2500 };
const FLAT_BASE = { full: 2048, compact: 1024 };

interface ResolvedPin { pin: TerrainPin; idx: number; lift: number }

class Terrain implements TerrainHandle {
  private root = document.createElement("div");
  private scene: Scene3D | null = null;
  private flat: Flat2D;
  private overlay: Overlay;
  private palette: Palette;
  private layout!: Layout;
  private hm!: Heightmap;
  private labels: RingLabel[] = [];
  private pins: ResolvedPin[] = [];
  private flatMode: boolean;
  private under = false;
  private selId: NodeId | null = null;
  private selPath: NodeId[] | undefined;
  private selIdx = -1;
  private routeStart: [number, number] | null = null;
  private hoverIdx = -1;
  private fps = new FpsWatch();
  private autoFlatAllowed = true;
  private lastRendered = false;
  private overlayDirty = true;
  private pending: { x: number; y: number } | null = null;
  private downAt: { x: number; y: number } | null = null;
  private raf = 0;
  private w = 0;
  private h = 0;
  private disposed = false;
  private ro: ResizeObserver;
  private spin = new AutoSpin();
  /** A selection or fly-to holds the view: no auto-rotation until the user returns to full view. */
  private held = false;
  /** A fly-to moved the camera off the full view (until resetView, or the user navigates home). */
  private flown = false;
  private fullView = true;
  private lastNow = -1;
  private motion = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;

  constructor(el: HTMLElement, private tree: TreeDto, private opts: TerrainOptions, private events: TerrainEvents) {
    this.root.className = "svt-root";
    el.appendChild(this.root);
    this.palette = paletteFor(opts.theme);
    const dpr = typeof devicePixelRatio === "number" ? devicePixelRatio : 1;
    try {
      this.scene = new Scene3D(this.root, Math.min(dpr, opts.compact ? 1.5 : 2));
    } catch {
      this.scene = null; // no WebGL, or only a software renderer: stay on the 2D map
    }
    this.flat = new Flat2D(this.root, FLAT_BASE[opts.compact ? "compact" : "full"],
      (k, x, y) => this.setHover(k, x, y), (k) => this.click(k));
    this.overlay = new Overlay(this.root, {
      onPinEnter: (i, ev) => this.pinHover(i, ev),
      onPinLeave: () => this.setHover(-1, 0, 0),
      onPinClick: (i) => { const p = this.pins[i]; if (p) { this.pick(p.pin.nodeId); this.flyTo(p.pin.nodeId); } },
    });
    this.flatMode = opts.flat || !this.scene;
    if (this.scene) {
      const c = this.scene.canvas;
      c.addEventListener("pointermove", this.onMove);
      c.addEventListener("pointerdown", this.onDown);
      c.addEventListener("pointerup", this.onUp);
      c.addEventListener("pointerleave", this.onLeave);
      this.scene.onInteract = () => {
        this.spin.poke(performance.now());
        // Navigating back to the full view by hand counts as returning from a fly-to.
        if (this.scene && !this.scene.interacting && this.scene.isHome(this.under)) this.flown = false;
      };
    }
    this.root.addEventListener("keydown", this.onKey);
    this.motion?.addEventListener?.("change", this.applyMotion);
    this.applyTheme();
    this.rebuild();
    this.applyMode();
    this.applyMotion();
    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(this.root);
    this.resize();
    this.raf = requestAnimationFrame(this.tick);
    if (!this.scene && !opts.flat) setTimeout(() => !this.disposed && this.events.onAutoFlat?.(), 0);
  }

  // ---------- Data ----------
  private get compact() { return this.opts.compact; }

  private rebuild(): void {
    const res = RES[this.compact ? "compact" : "full"];
    const cell = 2 / (res - 1);
    this.layout = layoutTree(this.tree, {
      maxNodes: MAX_NODES[this.compact ? "compact" : "full"],
      minArea: cell * cell * 1.5,
      includeCloud: this.opts.cloudValleys,
    });
    this.hm = buildHeightmap(this.layout, res, { cloudValleys: this.opts.cloudValleys });
    this.scene?.setTerrain(this.hm, this.layout, this.palette);
    this.scene?.setPalette(this.palette, this.hm, this.layout, this.opts.categoryTint);
    this.flat.setTerrain(this.hm, this.layout, this.palette, this.opts.categoryTint);
    this.applyLabels();
    this.applyPins();
    this.hoverIdx = -1;
    this.applySelection();
  }

  private applyLabels(): void {
    this.labels = this.opts.ringLabels ? ringLabels(this.layout, this.compact ? 5 : 8) : [];
    this.overlay.setLabels(this.labels.map((l) => this.layout.nodes[l.index].src?.name ?? ""));
    this.overlayDirty = true;
  }

  private applyPins(): void {
    const found = locate(this.tree, this.layout, this.opts.pins.map((p) => p.nodeId));
    this.pins = [];
    for (const pin of this.opts.pins) {
      const idx = found.get(pin.nodeId);
      if (idx === undefined) continue;
      this.pins.push({ pin, idx, lift: (this.compact ? 0.2 : 0.32) + (this.pins.length % 3) * 0.07 });
    }
    this.overlay.setPins(this.pins.map((p) => ({ kind: p.pin.kind, label: p.pin.label })));
    this.applyDim();
    this.overlayDirty = true;
  }

  private applyTheme(): void {
    const p = this.palette, s = this.root.style;
    s.setProperty("--svt-text", css(p.text));
    s.setProperty("--svt-text2", css(p.text2));
    s.setProperty("--svt-accent", css(p.accent));
    s.setProperty("--svt-cloud", css(p.cloud));
    s.setProperty("--svt-grew", css(p.grew));
    s.setProperty("--svt-surface", css(p.surface));
    s.setProperty("--svt-stroke", css(p.stroke));
    s.setProperty("--svt-surface-ink", css(p.fill));
    s.setProperty("--svt-pin-ink", p.light ? "#ffffff" : "#121314");
    s.setProperty("--svt-halo", p.light ? "rgba(255,255,255,.6)" : "rgba(0,0,0,.25)");
    this.root.classList.toggle("svt-compact", this.compact);
  }

  private applyMotion = (): void => {
    const reduce = !!this.motion?.matches;
    if (this.scene) {
      this.scene.duration = reduce ? 0 : VIEW_MS;
      this.scene.setZoomEnabled(!this.compact);
    }
    this.flat.duration = reduce ? 0 : VIEW_MS;
  };

  /** Auto-rotation wanted at all: on by default in the compact card, never with reduced motion. */
  private get spinAllowed(): boolean {
    return (this.opts.autoRotate ?? this.compact) && !this.motion?.matches && !this.under;
  }

  private setFullView(full: boolean): void {
    if (full === this.fullView) return;
    this.fullView = full;
    this.events.onFullViewChange?.(full);
  }

  private onKey = () => { this.spin.poke(performance.now()); };

  private applyMode(): void {
    if (this.scene) this.scene.canvas.style.display = this.flatMode ? "none" : "";
    this.flat.canvas.style.display = this.flatMode ? "" : "none";
    this.flat.invalidate();
    this.scene?.invalidate();
    this.fps.reset();
    this.lastRendered = false;
    this.setHover(-1, 0, 0);
    this.overlayDirty = true;
  }

  // ---------- Selection and route ----------
  /** Index of a node on the map: itself, or the nearest laid-out ancestor if it was folded. */
  private indexOf(id: NodeId, path?: NodeId[]): number {
    const k = this.layout.byId.get(id);
    if (k !== undefined) return k;
    if (path) for (let i = path.length - 1; i >= 0; i--) {
      const a = this.layout.byId.get(path[i]);
      if (a !== undefined) return a;
    }
    return locate(this.tree, this.layout, [id]).get(id) ?? -1;
  }

  private applySelection(): void {
    this.selIdx = this.selId === null ? -1 : this.indexOf(this.selId, this.selPath);
    const node = this.selIdx > 0 ? this.layout.nodes[this.selIdx] : null;
    const stops = node ? routeStops(this.layout, chainOf(this.layout, this.selIdx)) : [];
    this.scene?.setSelected(node);
    this.scene?.setRoute(stops.length >= 2 ? routePoints(this.hm, stops) : null);
    this.flat.setSelected(node);
    this.flat.setRoute(stops);
    this.routeStart = stops.length >= 2 ? stops[0] : null;
    this.overlay.setStartLabel(this.routeStart ? shortRoot(this.tree.name) : null);
    this.applyDim();
    this.overlayDirty = true;
  }

  /** Pins outside the selected subtree fade back. */
  private applyDim(): void {
    this.pins.forEach((p, i) => {
      const dim = this.selIdx > 0 && !chainOf(this.layout, p.idx).includes(this.selIdx);
      this.overlay.pins[i]?.el.classList.toggle("svt-dim", dim);
    });
  }

  private pick(id: NodeId): void {
    this.select(id);
    this.held = true;
    this.events.onSelect?.(id);
  }

  private click(idx: number): void {
    if (idx < 0) return;
    this.pick(this.layout.nodes[idx].id);
  }

  // ---------- Hover ----------
  private setHover(idx: number, clientX: number, clientY: number): void {
    const node = idx > 0 ? this.layout.nodes[idx] : null;
    if (idx !== this.hoverIdx) {
      this.hoverIdx = idx;
      this.scene?.setHovered(node);
      this.flat.setHovered(node);
      this.events.onHover?.(node ? node.id : null);
    }
    if (!node || this.compact) { this.overlay.hideTip(); return; }
    const r = this.root.getBoundingClientRect();
    this.overlay.showTip(nodeTitle(this.layout, node), nodeLines(node), clientX - r.left, clientY - r.top);
  }

  private pinHover(i: number, ev: PointerEvent): void {
    const p = this.pins[i];
    if (!p) return;
    const node = this.layout.nodes[p.idx];
    if (this.hoverIdx !== p.idx) {
      this.hoverIdx = p.idx;
      this.scene?.setHovered(node);
      this.flat.setHovered(node);
    }
    this.events.onHover?.(p.pin.nodeId);
    if (this.compact) return;
    const r = this.root.getBoundingClientRect();
    this.overlay.showTip(p.pin.label, [PIN_LABEL[p.pin.kind], ...nodeLines(node)], ev.clientX - r.left, ev.clientY - r.top);
  }

  private onMove = (ev: PointerEvent) => {
    if (ev.buttons) { this.pending = null; this.setHover(-1, 0, 0); return; }
    this.pending = { x: ev.clientX, y: ev.clientY }; // picked once per frame in tick()
  };
  private onDown = (ev: PointerEvent) => { this.downAt = { x: ev.clientX, y: ev.clientY }; };
  private onUp = (ev: PointerEvent) => {
    const d = this.downAt;
    this.downAt = null;
    if (!d || Math.hypot(ev.clientX - d.x, ev.clientY - d.y) > 4 || !this.scene) return;
    const sq = this.scene.pick(ev.clientX, ev.clientY);
    if (sq) this.click(this.hm.owner[sampleIndex(this.hm, sq[0], sq[1])]);
  };
  private onLeave = () => { this.pending = null; this.setHover(-1, 0, 0); };

  // ---------- Frame loop ----------
  private tick = (now: number) => {
    if (this.disposed) return;
    this.raf = requestAnimationFrame(this.tick);
    const dt = this.lastNow < 0 ? 1 / 60 : Math.max(0, now - this.lastNow) / 1000;
    this.lastNow = now;
    if (this.flatMode || !this.scene) {
      if (this.flat.update() || this.overlayDirty) this.placeOverlay();
      this.setFullView(this.flat.isHome());
      this.lastRendered = false;
      return;
    }
    if (this.pending) {
      const { x, y } = this.pending;
      this.pending = null;
      const sq = this.scene.pick(x, y);
      this.setHover(sq ? this.hm.owner[sampleIndex(this.hm, sq[0], sq[1])] : -1, x, y);
    }
    if (this.scene.interacting) this.spin.poke(now);
    const full = this.scene.isHome(this.under) && !this.flown;
    this.setFullView(full);
    const speed = this.spin.update(now, dt * 1000, { allowed: this.spinAllowed, fullView: full, held: this.held });
    const drew = this.scene.update(speed, dt);
    if (drew || this.overlayDirty) this.placeOverlay();
    if (drew && this.autoFlatAllowed && !document.hidden && this.fps.frame(now, this.lastRendered)) {
      this.autoFlatAllowed = false;
      this.flatMode = true;
      this.applyMode();
      this.events.onAutoFlat?.();
    }
    this.lastRendered = drew;
  };

  private placeOverlay(): void {
    this.overlayDirty = false;
    const { w, h } = this;
    if (!w || !h) return;
    const flat = this.flatMode || !this.scene;
    const scene = this.scene;
    // Returns [x, y, depth, visible] for a ground point lifted by `lift` (3D) or `px` (2D).
    const at = (sx: number, sy: number, lift: number, px: number): [number, number, number, boolean] => {
      const [dx, dz] = toDisc(sx, sy);
      if (flat) { const [x, y] = this.flat.toScreen(dx, dz); return [x, y - px, 0.5, true]; }
      const g = Math.max(0, sampleHeight(this.hm, sx, sy));
      const [x, y, z] = scene!.project(dx, g + lift, -dz, w, h);
      return [x, y, z, z < 1];
    };
    this.pins.forEach((p, i) => {
      const [sx, sy] = centroid(this.layout.nodes[p.idx]);
      const [gx, gy] = at(sx, sy, 0, 0);
      const [hx, hy, z, vis] = at(sx, sy, p.lift, this.compact ? 16 : 24 + (i % 3) * 6);
      this.overlay.placePin(this.overlay.pins[i], flat ? gx : hx, hy, gy, z, vis);
    });
    if (this.routeStart) {
      const [sx, sy] = this.routeStart;
      const [, gy] = at(sx, sy, 0, 0);
      const [hx, hy, z, vis] = at(sx, sy, 0.16, 20);
      this.overlay.placePin(this.overlay.start, hx, hy, gy, z, vis);
    }
    this.labels.forEach((l, i) => {
      const x = Math.cos(l.angle) * 1.3, z = Math.sin(l.angle) * 1.3;
      if (flat) { const [px, py] = this.flat.toScreen(x, -z); this.overlay.placeLabel(i, px, py, true); return; }
      const [px, py, d] = scene!.project(x, 0, z, w, h);
      this.overlay.placeLabel(i, px, py, d < 1);
    });
  }

  // ---------- TerrainHandle ----------
  setData(tree: TreeDto): void {
    if (this.disposed) return;
    this.tree = tree;
    this.rebuild();
  }

  setOptions(partial: Partial<TerrainOptions>): void {
    if (this.disposed) return;
    const prev = this.opts;
    this.opts = { ...prev, ...partial };
    const o = this.opts;
    if (o.theme !== prev.theme) {
      this.palette = paletteFor(o.theme);
      this.applyTheme();
    }
    if (o.compact !== prev.compact || o.cloudValleys !== prev.cloudValleys) {
      this.applyTheme();
      this.applyMotion();
      this.rebuild();
    } else {
      if (o.theme !== prev.theme || o.categoryTint !== prev.categoryTint) {
        this.scene?.setPalette(this.palette, this.hm, this.layout, o.categoryTint);
        this.flat.setTerrain(this.hm, this.layout, this.palette, o.categoryTint);
      }
      if (o.pins !== prev.pins) this.applyPins();
      if (o.ringLabels !== prev.ringLabels) this.applyLabels();
    }
    if (o.flat !== prev.flat) this.setFlat(o.flat);
  }

  select(id: NodeId | null, path?: NodeId[]): void {
    if (this.disposed) return;
    this.selId = id;
    this.selPath = path;
    if (id === null) this.held = false;
    else if (id !== this.tree.id) this.held = true;
    this.applySelection();
  }

  flyTo(id: NodeId): void {
    if (this.disposed) return;
    const k = this.indexOf(id, id === this.selId ? this.selPath : undefined);
    if (k < 0) return;
    const node = this.layout.nodes[k];
    if (k === 0) { this.resetView(); return; }
    this.under = false;
    this.held = true;
    this.flown = true;
    this.scene?.flyTo(node);
    this.flat.flyTo(node);
  }

  /** Back to the full view in the current mode (3D, flat, or Look under); selection and route stay. */
  resetView(): void {
    if (this.disposed) return;
    this.held = false;
    this.flown = false;
    this.scene?.view(this.under ? "under" : "home");
    this.flat.home();
    // Rotation eases back in after the usual idle time once the camera is home.
    this.spin.poke(performance.now());
  }

  isFullView(): boolean { return this.fullView; }

  setFlat(flat: boolean): void {
    if (this.disposed) return;
    this.opts = { ...this.opts, flat };
    const next = flat || !this.scene;
    if (next === this.flatMode) return;
    // The automatic switch fires at most once: if the app goes back to 3D afterwards
    // ("Try 3D anyway"), that choice sticks.
    this.flatMode = next;
    this.applyMode();
  }

  lookUnder(on: boolean): void {
    if (this.disposed || on === this.under) return;
    this.under = on;
    this.scene?.view(on ? "under" : "home");
    this.spin.poke(performance.now());
    this.applyMotion();
  }

  resize(): void {
    if (this.disposed) return;
    const r = this.root.getBoundingClientRect();
    if (!r.width || !r.height) return;
    this.w = r.width; this.h = r.height;
    this.scene?.resize(r.width, r.height);
    this.flat.resize(r.width, r.height, Math.min(typeof devicePixelRatio === "number" ? devicePixelRatio : 1, 2));
    this.overlayDirty = true;
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.ro.disconnect();
    this.motion?.removeEventListener?.("change", this.applyMotion);
    if (this.scene) {
      const c = this.scene.canvas;
      c.removeEventListener("pointermove", this.onMove);
      c.removeEventListener("pointerdown", this.onDown);
      c.removeEventListener("pointerup", this.onUp);
      c.removeEventListener("pointerleave", this.onLeave);
      this.scene.dispose();
      this.scene = null;
    }
    this.flat.dispose();
    this.overlay.dispose();
    this.root.removeEventListener("keydown", this.onKey);
    this.root.remove();
    this.pins = [];
    this.labels = [];
  }
}

/** "C:\" → "C:", "/" → "/", "Macintosh HD" → "MH". */
function shortRoot(name: string): string {
  const t = name.replace(/[\\/]+$/, "");
  if (!t) return "/";
  if (/^[A-Za-z]:$/.test(t)) return t.toUpperCase();
  const words = t.split(/\s+/).filter(Boolean);
  return (words.length > 1 ? words.map((w) => w[0]).join("") : t).slice(0, 2).toUpperCase();
}

function nodeTitle(layout: Layout, n: LayoutNode): string {
  if (n.src) return n.src.name;
  const parent = layout.nodes[n.parent];
  return `Smaller items in ${parent?.src?.name ?? "this folder"}`;
}

function nodeLines(n: LayoutNode): string[] {
  const lines = [`${formatBytes(n.allocated)} on this disk`];
  if (n.cloudOnly > 0) lines.push(`${formatBytes(n.cloudOnly)} cloud only`);
  const s = n.src;
  if (s && s.kind === "dir" && s.fileCount > 0) lines.push(`${s.fileCount.toLocaleString()} files`);
  return lines;
}

export const mountTerrain: MountTerrain = (el, tree, opts, events = {}) => new Terrain(el, tree, opts, events);
