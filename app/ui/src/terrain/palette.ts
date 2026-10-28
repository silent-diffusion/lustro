// Theme colors for the terrain, copied from design/tokens.css so the module renders the same
// without depending on page CSS. Keep in sync with the tokens (one color means one thing).

import type { Category } from "../ipc/types";

export type RGB = [number, number, number];

export interface Palette {
  fill: RGB;
  line: RGB;
  lineMinor: RGB;
  accent: RGB;
  cloud: RGB;
  text: RGB;
  text2: RGB;
  /** Tooltip and marker backgrounds (surface-2) and their border (stroke-strong). */
  surface: RGB;
  stroke: RGB;
  grew: RGB;
  light: boolean;
  categories: Record<Category, RGB>;
}

export function hex(c: string): RGB {
  const v = parseInt(c.replace("#", ""), 16);
  return [((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255];
}

export const css = (c: RGB, a = 1) =>
  `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${a})`;

const dark: Palette = {
  fill: hex("#141517"), line: hex("#c9ccd0"), lineMinor: hex("#55595f"),
  accent: hex("#b9dbe6"), cloud: hex("#b9dbe6"), text: hex("#eceeef"), text2: hex("#a3a7ad"), surface: hex("#202124"), stroke: hex("#45484d"), grew: hex("#ef7654"), light: false,
  categories: {
    video: hex("#ef7654"), images: hex("#d6ec8c"), audio: hex("#f3b05a"), documents: hex("#a8cfde"),
    archives: hex("#c4a5e8"), installers: hex("#e88bb0"), code: hex("#7fd3b8"), apps: hex("#8fa8f0"),
    system: hex("#8a9097"), other: hex("#5c6168"),
  },
};

const light: Palette = {
  fill: hex("#f4f6f7"), line: hex("#3a3e43"), lineMinor: hex("#b5babf"),
  accent: hex("#2f7d95"), cloud: hex("#3f8fae"), text: hex("#15171a"), text2: hex("#565b62"), surface: hex("#f2f4f5"), stroke: hex("#c3c8cc"), grew: hex("#d4512d"), light: true,
  categories: {
    video: hex("#d4512d"), images: hex("#7fa61b"), audio: hex("#c9861a"), documents: hex("#3f8fae"),
    archives: hex("#8a63c9"), installers: hex("#c4527f"), code: hex("#2d9c7c"), apps: hex("#4e6fd6"),
    system: hex("#6b7178"), other: hex("#a4a9af"),
  },
};

export const paletteFor = (theme: "dark" | "light"): Palette => (theme === "light" ? light : dark);

/** Shading shared by the 3D shader and the 2D map (keep the two in step). */
export function shade(p: Palette, lightness: number, tint: RGB, tintOn: boolean): RGB {
  const k = 0.7 + 0.45 * lightness;
  const amt = tintOn ? (p.light ? 0.3 : 0.22) : 0;
  const tk = 0.55 + 0.45 * lightness;
  return [0, 1, 2].map((c) => p.fill[c] * k * (1 - amt) + tint[c] * tk * amt) as RGB;
}
