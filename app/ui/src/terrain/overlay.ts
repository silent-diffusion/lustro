// DOM layer over the map: pins on stems, the route's start marker and waypoints, the trail card,
// compass ring labels and a tooltip. File names are untrusted, so they only ever go through
// textContent.

import type { PinKind } from "./types";
import type { Trail } from "./trail";
import { fmtShare, visibleHops } from "./trail";

const ICONS: Record<PinKind, string> = {
  largest: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5 21V4h11l-1.5 4L16 12H7v9z"/></svg>',
  grew: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 19V5M6 11l6-6 6 6"/></svg>',
  duplicate: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true"><rect x="3" y="7" width="11" height="13" rx="2"/><rect x="10" y="3" width="11" height="13" rx="2" fill="currentColor" fill-opacity=".25"/></svg>',
  cloud: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19.5 11 3.5 3.5 0 0 1 18 18z"/></svg>',
};

export const PIN_LABEL: Record<PinKind, string> = {
  largest: "Largest file", grew: "Top grower", duplicate: "Duplicate", cloud: "Cloud only",
};

export function formatBytes(b: number): string {
  if (!(b > 0)) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB", "PB"];
  const e = Math.min(u.length - 1, Math.floor(Math.log(b) / Math.log(1024)));
  const v = b / Math.pow(1024, e);
  return `${v >= 100 || e === 0 ? Math.round(v) : v.toFixed(1)} ${u[e]}`;
}

export interface PinEl { el: HTMLDivElement; stem: HTMLDivElement }

export interface OverlayHandlers {
  onPinEnter(i: number, ev: PointerEvent): void;
  onPinLeave(i: number): void;
  onPinClick(i: number): void;
  /** A trail card row or waypoint was clicked: hop k (0 = the root). */
  onTrailPick(k: number): void;
  /** Pointer or focus on hop k, or -1 when it leaves. */
  onTrailHover(k: number): void;
}

export interface TrailView {
  trail: Trail;
  /** Text of the start marker ("C:"), repeated on the root row. */
  rootBadge: string;
  /** Why the selection stands out ("Largest file"), when it is pinned. */
  tag: string | null;
}

const CHEVRON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';

export class Overlay {
  readonly el = document.createElement("div");
  readonly pins: PinEl[] = [];
  readonly labels: HTMLDivElement[] = [];
  readonly start: PinEl;
  readonly waypoints: HTMLDivElement[] = [];
  private tip = document.createElement("div");
  private startText: HTMLDivElement;
  private card = document.createElement("div");
  private trail: TrailView | null = null;
  private trailKey = "";
  private cardOpen = true;
  private cardAll = false;

  constructor(parent: HTMLElement, private h: OverlayHandlers) {
    this.el.className = "svt-overlay";
    parent.appendChild(this.el);
    this.start = this.makePin("svt-pin svt-start");
    this.startText = this.start.el.firstElementChild as HTMLDivElement;
    this.start.el.style.display = "none";
    this.tip.className = "svt-tip";
    this.tip.setAttribute("role", "tooltip");
    this.tip.style.display = "none";
    this.card.className = "svt-trail";
    this.card.setAttribute("role", "navigation");
    this.card.setAttribute("aria-label", "Route to the selected item");
    this.card.style.display = "none";
    this.el.append(this.card, this.tip);
  }

  private makePin(cls: string): PinEl {
    const el = document.createElement("div");
    el.className = cls;
    const head = document.createElement("div");
    head.className = "svt-head";
    const stem = document.createElement("div");
    stem.className = "svt-stem";
    el.append(head, stem);
    this.el.insertBefore(el, this.card.parentNode ? this.card : null);
    return { el, stem };
  }

  setPins(pins: { kind: PinKind; label: string }[]): void {
    for (const p of this.pins) p.el.remove();
    this.pins.length = 0;
    pins.forEach((p, i) => {
      const pe = this.makePin(`svt-pin svt-${p.kind}`);
      const head = pe.el.firstElementChild as HTMLDivElement;
      head.innerHTML = ICONS[p.kind]; // static markup only
      pe.el.setAttribute("role", "button");
      pe.el.tabIndex = 0;
      pe.el.setAttribute("aria-label", `${PIN_LABEL[p.kind]}: ${p.label}`);
      pe.el.addEventListener("pointerenter", (ev) => this.h.onPinEnter(i, ev));
      pe.el.addEventListener("pointerleave", () => this.h.onPinLeave(i));
      pe.el.addEventListener("click", () => this.h.onPinClick(i));
      pe.el.addEventListener("keydown", (ev) => { if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); this.h.onPinClick(i); } });
      this.pins.push(pe);
    });
  }

  setLabels(texts: string[]): void {
    for (const l of this.labels) l.remove();
    this.labels.length = 0;
    for (const t of texts) {
      const e = document.createElement("div");
      e.className = "svt-ring-label";
      e.textContent = t;
      this.el.insertBefore(e, this.card);
      this.labels.push(e);
    }
  }

  setStartLabel(text: string | null): void {
    if (text === null) { this.start.el.style.display = "none"; return; }
    this.start.el.style.display = "";
    this.startText.textContent = text;
  }

  /** Numbered waypoint markers for hops 1..count (hop 0 is the start marker). */
  setWaypoints(count: number): void {
    while (this.waypoints.length > count) this.waypoints.pop()!.remove();
    while (this.waypoints.length < count) {
      const k = this.waypoints.length + 1;
      const e = document.createElement("div");
      e.className = "svt-wp";
      e.textContent = String(k);
      e.addEventListener("click", () => this.h.onTrailPick(k));
      e.addEventListener("pointerenter", () => this.h.onTrailHover(k));
      e.addEventListener("pointerleave", () => this.h.onTrailHover(-1));
      this.el.insertBefore(e, this.card);
      this.waypoints.push(e);
    }
    this.waypoints.forEach((e, i) => e.classList.toggle("svt-wp-end", i === count - 1));
  }

  placeWaypoint(i: number, x: number, y: number, depth: number, visible: boolean): void {
    const e = this.waypoints[i];
    e.style.visibility = visible ? "" : "hidden";
    e.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%)`;
    e.style.zIndex = String(Math.round((1 - depth) * 10000));
  }

  /** Lights up hop k on the map and in the card (-1: none). */
  hotHop(k: number): void {
    this.start.el.classList.toggle("svt-hot", k === 0);
    this.waypoints.forEach((e, i) => e.classList.toggle("svt-hot", i + 1 === k));
    for (const r of this.card.querySelectorAll<HTMLElement>(".svt-hop")) r.classList.toggle("svt-hot", r.dataset.k === String(k));
  }

  /** The trail card: the route as a list of waypoints with sizes. `null` hides it. */
  setTrail(v: TrailView | null): void {
    this.trail = v;
    const key = v ? v.trail.hops.map((h) => h.id).join("/") : "";
    if (key !== this.trailKey) { this.trailKey = key; this.cardAll = false; }
    this.renderCard();
  }

  private renderCard(): void {
    const v = this.trail;
    if (!v) { this.card.style.display = "none"; this.card.replaceChildren(); return; }
    // Live scans re-render about once a second: keep keyboard focus on the same row.
    const act = document.activeElement as HTMLElement | null;
    const focusKey = act && this.card.contains(act) ? act.dataset.k : undefined;
    this.card.replaceChildren();
    this.card.style.display = "";
    const hops = v.trail.hops;

    const head = document.createElement("button");
    head.className = "svt-trail-head";
    head.dataset.k = "head";
    head.setAttribute("aria-expanded", String(this.cardOpen));
    const title = document.createElement("span");
    title.textContent = "Route";
    const count = document.createElement("span");
    count.className = "svt-trail-count";
    count.textContent = `${hops.length - 1} waypoint${hops.length === 2 ? "" : "s"}`;
    const chev = document.createElement("span");
    chev.className = "svt-chev";
    chev.innerHTML = CHEVRON; // static markup only
    head.append(title, count, chev);
    head.addEventListener("click", () => { this.cardOpen = !this.cardOpen; this.renderCard(); });
    this.card.appendChild(head);

    if (this.cardOpen) {
      const list = document.createElement("ol");
      list.className = "svt-trail-list";
      const shown = this.cardAll ? hops.map((_, i) => i) : visibleHops(hops.length);
      for (const k of shown) {
        const li = document.createElement("li");
        if (k === null) {
          const more = hops.length - 5;
          const b = document.createElement("button");
          b.className = "svt-gap";
          b.dataset.k = "gap";
          b.textContent = `${more} more folder${more === 1 ? "" : "s"}`;
          b.setAttribute("aria-label", `Show ${more} more folders on the route`);
          b.addEventListener("click", () => {
            this.cardAll = true;
            this.renderCard();
            this.card.querySelector<HTMLElement>('[data-k="2"]')?.focus();
          });
          li.appendChild(b);
        } else {
          li.appendChild(this.hopRow(v, k));
        }
        list.appendChild(li);
      }
      if (v.trail.hidden !== 0) {
        const li = document.createElement("li");
        li.className = "svt-trail-note";
        const n = v.trail.hidden;
        li.textContent = n > 0
          ? `+ ${n} more level${n === 1 ? "" : "s"}, too small to draw`
          : "The item is too small to draw: marked on its folder";
        list.appendChild(li);
      }
      this.card.appendChild(list);
    }
    if (focusKey) this.card.querySelector<HTMLElement>(`[data-k="${focusKey}"]`)?.focus();
  }

  private hopRow(v: TrailView, k: number): HTMLButtonElement {
    const hop = v.trail.hops[k];
    const end = k === v.trail.hops.length - 1;
    const b = document.createElement("button");
    b.className = end ? "svt-hop svt-here" : "svt-hop";
    b.dataset.k = String(k);
    const badge = document.createElement("span");
    badge.className = k === 0 ? "svt-badge svt-badge-root" : "svt-badge";
    badge.textContent = k === 0 ? v.rootBadge : String(k);
    const name = document.createElement("span");
    name.className = "svt-hop-name";
    name.textContent = hop.name;
    name.title = hop.name;
    const meta = document.createElement("span");
    meta.className = "svt-hop-meta";
    const size = hop.cloud ? `${formatBytes(hop.cloudOnly)} cloud only` : formatBytes(hop.bytes);
    meta.textContent = hop.share === null ? size : `${size} · ${fmtShare(hop.share)}`;
    b.append(badge, name, meta);
    if (end && v.tag) {
      const tag = document.createElement("span");
      tag.className = "svt-tag";
      tag.textContent = v.tag;
      b.appendChild(tag);
    }
    if (hop.share !== null) {
      const bar = document.createElement("span");
      bar.className = "svt-share";
      bar.style.width = `max(2px, calc((100% - 44px) * ${hop.share.toFixed(4)}))`;
      b.appendChild(bar);
    }
    const of = hop.share === null ? "" : `, ${fmtShare(hop.share)} of ${hop.parentName}`;
    b.setAttribute("aria-label", `${k === 0 ? "Start" : `Waypoint ${k}`}: ${hop.name}, ${size}${of}${end && v.tag ? `, ${v.tag}` : ""}`);
    if (end) b.setAttribute("aria-current", "location");
    b.addEventListener("click", () => this.h.onTrailPick(k));
    b.addEventListener("pointerenter", () => this.h.onTrailHover(k));
    b.addEventListener("pointerleave", () => this.h.onTrailHover(-1));
    b.addEventListener("focus", () => this.h.onTrailHover(k));
    b.addEventListener("blur", () => this.h.onTrailHover(-1));
    return b;
  }

  /** Puts a pin's head at (hx, hy) with its stem reaching down to the ground at gy. */
  placePin(p: PinEl, hx: number, hy: number, gy: number, depth: number, visible: boolean): void {
    p.el.style.visibility = visible ? "" : "hidden";
    p.el.style.transform = `translate(${hx}px, ${hy}px) translate(-50%, -50%)`;
    p.stem.style.height = Math.max(0, gy - hy) + "px";
    p.el.style.zIndex = String(Math.round((1 - depth) * 10000) + 10);
  }

  placeLabel(i: number, x: number, y: number, visible: boolean): void {
    const l = this.labels[i];
    l.style.visibility = visible ? "" : "hidden";
    l.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%)`;
  }

  showTip(title: string, lines: string[], x: number, y: number): void {
    this.tip.replaceChildren();
    const b = document.createElement("div");
    b.className = "svt-tip-title";
    b.textContent = title;
    this.tip.appendChild(b);
    for (const ln of lines) {
      const d = document.createElement("div");
      d.textContent = ln;
      this.tip.appendChild(d);
    }
    this.tip.style.display = "";
    const r = this.el.getBoundingClientRect();
    const tw = this.tip.offsetWidth, th = this.tip.offsetHeight;
    const left = Math.min(Math.max(4, x + 14), r.width - tw - 4);
    const top = y + 16 + th > r.height ? y - th - 10 : y + 16;
    this.tip.style.transform = `translate(${left}px, ${Math.max(4, top)}px)`;
  }

  hideTip(): void { this.tip.style.display = "none"; }

  dispose(): void { this.el.remove(); }
}
