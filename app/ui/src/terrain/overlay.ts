// DOM layer over the map: pins on stems, the route's start marker, compass ring labels and a
// tooltip. File names are untrusted, so they only ever go through textContent.

import type { PinKind } from "./types";

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
}

export class Overlay {
  readonly el = document.createElement("div");
  readonly pins: PinEl[] = [];
  readonly labels: HTMLDivElement[] = [];
  readonly start: PinEl;
  private tip = document.createElement("div");
  private startText: HTMLDivElement;

  constructor(parent: HTMLElement, private h: OverlayHandlers) {
    this.el.className = "svt-overlay";
    parent.appendChild(this.el);
    this.start = this.makePin("svt-pin svt-start");
    this.startText = this.start.el.firstElementChild as HTMLDivElement;
    this.start.el.style.display = "none";
    this.tip.className = "svt-tip";
    this.tip.setAttribute("role", "tooltip");
    this.tip.style.display = "none";
    this.el.appendChild(this.tip);
  }

  private makePin(cls: string): PinEl {
    const el = document.createElement("div");
    el.className = cls;
    const head = document.createElement("div");
    head.className = "svt-head";
    const stem = document.createElement("div");
    stem.className = "svt-stem";
    el.append(head, stem);
    this.el.insertBefore(el, this.tip.parentNode ? this.tip : null);
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
      this.el.insertBefore(e, this.tip);
      this.labels.push(e);
    }
  }

  setStartLabel(text: string | null): void {
    if (text === null) { this.start.el.style.display = "none"; return; }
    this.start.el.style.display = "";
    this.startText.textContent = text;
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
