// Formatting helpers. Sizes are bytes; decimal units by default (1 GB = 10^9 bytes),
// binary units (GiB) when the user asks for them.

const MINUS = "−";

export interface SizeOpts {
  /** Prefix positive values with "+". */
  sign?: boolean;
  binary?: boolean;
}

export function fmtBytes(bytes: number, opts: SizeOpts = {}): string {
  const sign = bytes < 0 ? MINUS : opts.sign && bytes > 0 ? "+" : "";
  const v = Math.abs(bytes);
  const k = opts.binary ? 1024 : 1000;
  const units = opts.binary ? ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] : ["B", "KB", "MB", "GB", "TB", "PB"];
  if (v < k) return `${sign}${Math.round(v)} ${units[0]}`;
  let i = 0;
  let x = v;
  while (x >= k && i < units.length - 1) {
    x /= k;
    i++;
  }
  // GB and above: one decimal below 100, none above; TB gets two decimals like the mockups.
  let s: string;
  if (i >= 4) s = x.toFixed(2);
  else if (i === 3) s = x >= 100 ? x.toFixed(0) : x.toFixed(1);
  else s = x >= 10 ? x.toFixed(0) : x.toFixed(1);
  return `${sign}${s} ${units[i]}`;
}

export function fmtCount(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

export function fmtPct(fraction: number, digits = 1): string {
  if (!isFinite(fraction)) return "–";
  return `${(fraction * 100).toFixed(digits)}%`;
}

export function fmtDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "Today, 9:41 AM" / "Oct 21, 8:12 PM" / "Mar 14, 2025". `unix` is seconds. */
export function fmtWhen(unix: number, now = Date.now() / 1000, withTime = true): string {
  const d = new Date(unix * 1000);
  const n = new Date(now * 1000);
  const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  const sameDay = d.toDateString() === n.toDateString();
  if (sameDay && withTime) return `Today, ${time}`;
  const md = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  if (d.getFullYear() !== n.getFullYear()) return `${md}, ${d.getFullYear()}`;
  return withTime ? `${md}, ${time}` : md;
}

/** Short relative age: "today", "3 days ago", "4 months ago", "Feb 2024". */
export function fmtAge(unix: number | null, now = Date.now() / 1000): string {
  if (unix == null) return "—";
  const days = (now - unix) / 86400;
  if (days < 1) return "today";
  if (days < 2) return "yesterday";
  if (days < 14) return `${Math.floor(days)} days ago`;
  if (days < 60) return `${Math.floor(days / 7)} weeks ago`;
  if (days < 365) return `${Math.floor(days / 30)} months ago`;
  const d = new Date(unix * 1000);
  return `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** "10/21" for chart axes. */
export function fmtMonthDay(unix: number): string {
  const d = new Date(unix * 1000);
  return `${String(d.getMonth() + 1).padStart(2, "0")}/${String(d.getDate()).padStart(2, "0")}`;
}

/** Download sizes, as the update UI shows them: "6.1 MB of 14.2 MB". */
export function fmtMB(bytes: number): string {
  return `${(bytes / 1e6).toFixed(1)} MB`;
}
